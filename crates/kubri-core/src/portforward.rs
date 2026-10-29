//! Port-forwards from a local port to a pod, like `kubectl port-forward`.
//!
//! Each accepted connection opens its own stream to the pod, so one forward serves many
//! clients. Forwards keep running when the UI switches context, until they are stopped. A
//! forward to a Service or workload picks another ready pod when its pod is gone, so it
//! survives restarts and rollouts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::Api;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

use crate::{Error, ObjectRef, Result};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortForwardRequest {
    /// A pod, a Service, or a workload whose pods should receive the traffic.
    pub target: ObjectRef,
    /// The container port, or the Service port for Services.
    pub remote_port: u16,
    /// `None` or 0 picks a free port.
    #[serde(default)]
    pub local_port: Option<u16>,
    /// Address to listen on. Defaults to `127.0.0.1`.
    #[serde(default)]
    pub address: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortForwardInfo {
    pub id: u32,
    /// The context the forward runs in.
    pub context: String,
    pub target: ObjectRef,
    pub namespace: String,
    /// The pod that receives the traffic.
    pub pod: String,
    pub remote_port: u16,
    /// The port on the pod, after resolving a Service's target port.
    pub pod_port: u16,
    pub address: String,
    pub local_port: u16,
    pub started: String,
    /// Connections open right now.
    pub connections: u32,
    /// Why the last connection failed, cleared by the next successful one.
    pub error: Option<String>,
}

/// A favorite forward, and why it could not start if it could not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteStart {
    pub favorite: crate::FavoriteForward,
    pub error: Option<String>,
}

struct Forward {
    info: Arc<Mutex<PortForwardInfo>>,
    task: JoinHandle<()>,
}

#[derive(Default)]
pub(crate) struct Forwards {
    next_id: u32,
    active: HashMap<u32, Forward>,
}

impl Forwards {
    pub(crate) fn list(&self) -> Vec<PortForwardInfo> {
        let mut list: Vec<_> = self
            .active
            .values()
            .map(|f| f.info.lock().unwrap().clone())
            .collect();
        list.sort_by_key(|f| f.id);
        list
    }

    pub(crate) fn stop(&mut self, id: u32) -> Result<()> {
        let forward = self
            .active
            .remove(&id)
            .ok_or_else(|| Error::NotFound(format!("port-forward {id}")))?;
        forward.task.abort();
        Ok(())
    }

    pub(crate) fn stop_all(&mut self) {
        for (_, forward) in self.active.drain() {
            forward.task.abort();
        }
    }
}

/// Finds the pod and port to use now, for a forward whose pod went away.
pub(crate) type Repick =
    Arc<dyn Fn() -> futures::future::BoxFuture<'static, Result<(String, u16)>> + Send + Sync>;

/// Where the traffic of a forward goes.
pub(crate) struct Destination {
    pub api: Api<Pod>,
    pub namespace: String,
    pub pod: String,
    pub port: u16,
    /// For a Service or workload: how to find another pod.
    pub repick: Option<Repick>,
}

/// Binds the local port and starts forwarding.
pub(crate) async fn start(
    forwards: &Mutex<Forwards>,
    context: String,
    request: PortForwardRequest,
    destination: Destination,
) -> Result<PortForwardInfo> {
    let address = request.address.clone().unwrap_or_else(|| "127.0.0.1".to_owned());
    let listener = TcpListener::bind((address.as_str(), request.local_port.unwrap_or(0))).await?;
    let local_port = listener.local_addr()?.port();

    let mut guard = forwards.lock().unwrap();
    guard.next_id += 1;
    let id = guard.next_id;
    let info = Arc::new(Mutex::new(PortForwardInfo {
        id,
        context,
        target: request.target,
        namespace: destination.namespace.clone(),
        pod: destination.pod.clone(),
        remote_port: request.remote_port,
        pod_port: destination.port,
        address,
        local_port,
        started: k8s_openapi::jiff::Timestamp::now().to_string(),
        connections: 0,
        error: None,
    }));
    let task = tokio::spawn(serve(listener, destination, Arc::clone(&info)));
    let snapshot = info.lock().unwrap().clone();
    guard.active.insert(id, Forward { info, task });
    tracing::info!(id, pod = %snapshot.pod, port = snapshot.pod_port, local_port, "port-forward started");
    Ok(snapshot)
}

async fn serve(listener: TcpListener, destination: Destination, info: Arc<Mutex<PortForwardInfo>>) {
    // Dropping the set when the forward is stopped closes every open connection.
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((mut socket, peer)) => {
                    let api = destination.api.clone();
                    let repick = destination.repick.clone();
                    let info = Arc::clone(&info);
                    connections.spawn(async move {
                        let (pod, port) = {
                            let mut info = info.lock().unwrap();
                            info.connections += 1;
                            (info.pod.clone(), info.pod_port)
                        };
                        let mut result = connect(&api, &pod, port, &mut socket).await;
                        // The pod may be gone after a restart or rollout: another one takes over.
                        if let (Err(Failure::Open(err)), Some(repick)) = (&result, repick) {
                            match repick().await {
                                Ok((pod, port)) => {
                                    tracing::info!(%err, pod, port, "port-forward moves to another pod");
                                    {
                                        let mut info = info.lock().unwrap();
                                        info.pod = pod.clone();
                                        info.pod_port = port;
                                    }
                                    result = connect(&api, &pod, port, &mut socket).await;
                                }
                                Err(again) => result = Err(Failure::Open(format!("{err}; no other pod: {again}"))),
                            }
                        }
                        let mut info = info.lock().unwrap();
                        info.connections -= 1;
                        match result.map_err(Failure::message) {
                            Ok(()) => info.error = None,
                            Err(err) => {
                                tracing::warn!(%peer, pod, port, %err, "port-forward connection failed");
                                info.error = Some(err);
                            }
                        }
                    });
                }
                Err(err) => {
                    tracing::warn!(%err, "port-forward accept failed");
                    info.lock().unwrap().error = Some(err.to_string());
                    // Out of file handles, say: wait instead of spinning while they are gone.
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}

enum Failure {
    /// The stream to the pod did not open, as when the pod is gone.
    Open(String),
    /// The connection broke while copying.
    Copy(String),
}

impl Failure {
    fn message(self) -> String {
        match self {
            Failure::Open(m) | Failure::Copy(m) => m,
        }
    }
}

/// Copies one local connection to and from the pod.
async fn connect(api: &Api<Pod>, pod: &str, port: u16, socket: &mut TcpStream) -> Result<(), Failure> {
    let mut forwarder = api
        .portforward(pod, &[port])
        .await
        .map_err(|e| Failure::Open(Error::from(e).to_string()))?;
    let mut upstream = forwarder
        .take_stream(port)
        .ok_or_else(|| Failure::Open("the pod did not open the port".to_owned()))?;
    let failed = forwarder.take_error(port);
    let result = tokio::select! {
        copied = tokio::io::copy_bidirectional(socket, &mut upstream) => {
            copied.map(|_| ()).map_err(|e| Failure::Copy(e.to_string()))
        }
        Some(Some(message)) = async { match failed { Some(f) => Some(f.await), None => None } } => Err(Failure::Copy(message)),
    };
    drop(upstream);
    forwarder.abort();
    result
}

/// Maps a Service port to the port its pods listen on. Named target ports are looked up
/// in the pod's container ports.
pub(crate) fn service_target_port(service: &Service, port: u16, pod: &Pod) -> Result<u16> {
    let ports = service.spec.as_ref().and_then(|s| s.ports.as_ref());
    let service_port = ports
        .into_iter()
        .flatten()
        .find(|p| p.port == i32::from(port))
        .ok_or_else(|| Error::Unsupported(format!("the service has no port {port}")))?;
    match &service_port.target_port {
        None => Ok(port),
        Some(IntOrString::Int(target)) => {
            u16::try_from(*target).map_err(|_| Error::Unsupported(format!("invalid target port {target}")))
        }
        Some(IntOrString::String(name)) => container_port(pod, name)
            .ok_or_else(|| Error::Unsupported(format!("no container in the pod has a port named {name}"))),
    }
}

fn container_port(pod: &Pod, name: &str) -> Option<u16> {
    let containers = &pod.spec.as_ref()?.containers;
    let port = containers
        .iter()
        .flat_map(|c| c.ports.iter().flatten())
        .find(|p| p.name.as_deref() == Some(name))?;
    u16::try_from(port.container_port).ok()
}

/// The pod that should receive the traffic: a running, ready one, oldest first so that
/// repeated forwards land on the same pod.
pub(crate) fn pick_pod(mut pods: Vec<Pod>) -> Option<Pod> {
    pods.sort_by(|a, b| a.metadata.creation_timestamp.cmp(&b.metadata.creation_timestamp));
    pods.into_iter().find(is_ready)
}

fn is_ready(pod: &Pod) -> bool {
    let Some(status) = &pod.status else {
        return false;
    };
    status.phase.as_deref() == Some("Running")
        && status
            .conditions
            .iter()
            .flatten()
            .any(|c| c.type_ == "Ready" && c.status == "True")
}

/// Rejects listening addresses that are not IPs or `localhost`, before anything is bound.
pub(crate) fn validate_address(address: Option<&str>) -> Result<()> {
    match address {
        None | Some("localhost") => Ok(()),
        Some(address) => address
            .parse::<std::net::IpAddr>()
            .map(|_| ())
            .map_err(|_| Error::Unsupported(format!("invalid listen address {address}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pod(name: &str, created: &str, ready: bool) -> Pod {
        serde_json::from_value(json!({
            "metadata": { "name": name, "creationTimestamp": created },
            "spec": { "containers": [{ "name": "app", "ports": [{ "name": "http", "containerPort": 8080 }] }] },
            "status": {
                "phase": "Running",
                "conditions": [{ "type": "Ready", "status": if ready { "True" } else { "False" } }]
            }
        }))
        .unwrap()
    }

    fn service() -> Service {
        serde_json::from_value(json!({ "spec": { "ports": [
            { "name": "web", "port": 80, "targetPort": "http" },
            { "name": "admin", "port": 9000, "targetPort": 9001 },
            { "name": "same", "port": 7000 }
        ] } }))
        .unwrap()
    }

    #[test]
    fn resolves_service_target_ports() {
        let pod = pod("web-1", "2026-10-27T10:00:00Z", true);
        assert_eq!(service_target_port(&service(), 80, &pod).unwrap(), 8080);
        assert_eq!(service_target_port(&service(), 9000, &pod).unwrap(), 9001);
        assert_eq!(service_target_port(&service(), 7000, &pod).unwrap(), 7000);
        assert!(service_target_port(&service(), 1234, &pod).is_err());
    }

    #[test]
    fn picks_the_oldest_ready_pod() {
        let pods = vec![
            pod("new", "2026-10-27T12:00:00Z", true),
            pod("broken", "2026-10-27T09:00:00Z", false),
            pod("old", "2026-10-27T10:00:00Z", true),
        ];
        assert_eq!(pick_pod(pods).unwrap().metadata.name.as_deref(), Some("old"));
        assert!(pick_pod(vec![pod("broken", "2026-10-27T09:00:00Z", false)]).is_none());
    }

    #[test]
    fn validates_listen_addresses() {
        assert!(validate_address(None).is_ok());
        assert!(validate_address(Some("0.0.0.0")).is_ok());
        assert!(validate_address(Some("::1")).is_ok());
        assert!(validate_address(Some("localhost")).is_ok());
        assert!(validate_address(Some("evil.example.com")).is_err());
    }

    #[test]
    fn request_defaults_to_a_free_local_port() {
        let request: PortForwardRequest = serde_json::from_value(json!({
            "target": { "resource": { "group": "", "version": "v1", "plural": "pods" }, "namespace": "shop", "name": "web-1" },
            "remotePort": 8080
        }))
        .unwrap();
        assert_eq!(request.local_port, None);
        assert_eq!(request.address, None);
    }

    #[tokio::test]
    async fn forwards_can_be_listed_and_stopped() {
        let (client, _) = crate::testing::client(|_| (400, crate::testing::status(400, "no upgrade")));
        let forwards = Mutex::new(Forwards::default());
        let request: PortForwardRequest = serde_json::from_value(json!({
            "target": { "resource": { "group": "", "version": "v1", "plural": "pods" }, "namespace": "shop", "name": "web-1" },
            "remotePort": 8080
        }))
        .unwrap();
        let destination = Destination {
            api: Api::namespaced(client, "shop"),
            namespace: "shop".into(),
            pod: "web-1".into(),
            port: 8080,
            repick: None,
        };
        let info = start(&forwards, "kind".into(), request, destination)
            .await
            .unwrap();
        assert_eq!(info.id, 1);
        assert_eq!(info.address, "127.0.0.1");
        assert_ne!(info.local_port, 0);
        assert_eq!(forwards.lock().unwrap().list(), std::slice::from_ref(&info));

        // A connection reaches the forward; the fake cluster refuses the stream.
        let _socket = TcpStream::connect(("127.0.0.1", info.local_port)).await.unwrap();
        for _ in 0..100 {
            if forwards.lock().unwrap().list()[0].error.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let failed = forwards.lock().unwrap().list().remove(0);
        assert!(failed.error.unwrap().contains("400 Bad Request"));
        assert_eq!(failed.connections, 0);

        forwards.lock().unwrap().stop(info.id).unwrap();
        assert!(forwards.lock().unwrap().list().is_empty());
        assert!(matches!(
            forwards.lock().unwrap().stop(info.id),
            Err(Error::NotFound(_))
        ));
    }
}
