//! Port-forwards from a local port to a pod, like `kubectl port-forward`.
//!
//! Each accepted connection opens its own stream to the pod, so one forward serves many
//! clients. Forwards keep running when the UI switches context, until they are stopped. A
//! forward to a Service or workload picks another ready pod when its pod is gone, so it
//! survives restarts and rollouts.
//!
//! The stream to the pod is a WebSocket in the API server's port-forward protocol: each frame
//! starts with its channel, 0 for data and 1 for errors, and the first frame on each channel
//! names the port. Kubri runs it itself instead of through kube-rs's forwarder, for two reasons
//! that ended connections early:
//!
//! - A client that stops sending (shuts down its write half) still gets the pod's answer. The
//!   protocol cannot pass that half-close on to the pod, so the stream stays open until the pod
//!   closes it, or until the pod has sent nothing for [`AFTER_CLIENT_DONE`].
//! - A ping every [`PING_EVERY`] keeps quiet connections alive through load balancers and
//!   proxies in front of the API server, which drop connections that look idle.
//!
//! A local client that leaves without sending anything, such as a check whether the port is
//! open, gives the pod its connection back after [`AFTER_SILENT_CLIENT`]. Servers like MariaDB
//! reset a connection that never signs in, and that reset is not reported as the forward's
//! error when its client is gone anyway.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::{Api, Client, Resource};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};
use tokio_tungstenite::tungstenite::{self, Message};

/// How often a quiet stream to the pod is pinged, so nothing between drops it as idle.
pub(crate) const PING_EVERY: Duration = Duration::from_secs(20);
/// How long the stream stays open for the pod's answer once the local client stopped sending.
pub(crate) const AFTER_CLIENT_DONE: Duration = Duration::from_secs(15);
/// The same, for a client that never sent anything, such as a check whether the port is open.
/// It asked nothing, so it waits for nothing; the pod gets its connection back quickly, before
/// a server such as MariaDB gives up on it and resets it.
const AFTER_SILENT_CLIENT: Duration = Duration::from_secs(1);

use crate::Kubri;
use crate::{Error, ObjectRef, Result};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields = nullable)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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
    /// How and when the last connection ended.
    pub last_end: Option<ConnectionEnd>,
}

/// How a connection through a forward ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEnd {
    pub at: String,
    pub by: EndedBy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub enum EndedBy {
    /// The pod, or the cluster on its way, closed the stream.
    Pod,
    /// The local client closed it, or stopped sending and the pod had nothing more to say.
    Client,
    /// It failed; `error` says why.
    Error,
}

/// A favorite forward, and why it could not start if it could not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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
    let listeners = listen(&address, request.local_port.unwrap_or(0)).await?;
    let local_port = listeners[0].local_addr()?.port();

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
        last_end: None,
    }));
    let task = tokio::spawn(serve(listeners, destination, Arc::clone(&info)));
    let snapshot = info.lock().unwrap().clone();
    guard.active.insert(id, Forward { info, task });
    tracing::info!(id, pod = %snapshot.pod, port = snapshot.pod_port, local_port, "port-forward started");
    Ok(snapshot)
}

/// Listens on `address`. Like kubectl, `localhost` means both loopback addresses, 127.0.0.1
/// and ::1: a client may try either, and a forward on only one of them refuses the other. ::1
/// is left out where the system has no IPv6.
async fn listen(address: &str, port: u16) -> Result<Vec<TcpListener>> {
    if address != "localhost" {
        return Ok(vec![TcpListener::bind((address, port)).await?]);
    }
    let first = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
    // A free port picked for 127.0.0.1 is used for ::1 too.
    let picked = first.local_addr()?.port();
    let mut listeners = vec![first];
    match TcpListener::bind((Ipv6Addr::LOCALHOST, picked)).await {
        Ok(second) => listeners.push(second),
        // Something else listens on the port asked for: clients trying ::1 would reach it.
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse && port != 0 => return Err(err.into()),
        Err(err) => tracing::debug!(%err, "port-forward listens on 127.0.0.1 only"),
    }
    Ok(listeners)
}

/// Waits for the next connection on any of the listeners.
async fn accept(listeners: &[TcpListener]) -> std::io::Result<(TcpStream, std::net::SocketAddr)> {
    std::future::poll_fn(|cx| {
        for listener in listeners {
            if let std::task::Poll::Ready(accepted) = listener.poll_accept(cx) {
                return std::task::Poll::Ready(accepted);
            }
        }
        std::task::Poll::Pending
    })
    .await
}

async fn serve(listeners: Vec<TcpListener>, destination: Destination, info: Arc<Mutex<PortForwardInfo>>) {
    // Dropping the set when the forward is stopped closes every open connection.
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = accept(&listeners) => match accepted {
                Ok((mut socket, peer)) => {
                    let client = destination.api.clone().into_client();
                    let namespace = destination.namespace.clone();
                    let repick = destination.repick.clone();
                    let info = Arc::clone(&info);
                    connections.spawn(async move {
                        let (pod, port) = {
                            let mut info = info.lock().unwrap();
                            info.connections += 1;
                            (info.pod.clone(), info.pod_port)
                        };
                        let mut result = connect(&client, &namespace, &pod, port, &mut socket).await;
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
                                    result = connect(&client, &namespace, &pod, port, &mut socket).await;
                                }
                                Err(again) => result = Err(Failure::Open(format!("{err}; no other pod: {again}"))),
                            }
                        }
                        let mut info = info.lock().unwrap();
                        info.connections -= 1;
                        let by = match &result {
                            Ok(by) => *by,
                            Err(_) => EndedBy::Error,
                        };
                        info.last_end = Some(ConnectionEnd { at: k8s_openapi::jiff::Timestamp::now().to_string(), by });
                        match result.map_err(Failure::message) {
                            Ok(_) => info.error = None,
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
async fn connect(
    client: &Client,
    namespace: &str,
    pod: &str,
    port: u16,
    socket: &mut TcpStream,
) -> Result<EndedBy, Failure> {
    let open = |e: kube::Error| Failure::Open(Error::from(e).to_string());
    let request = kube::core::Request::new(Pod::url_path(&(), Some(namespace)))
        .portforward(pod, &[port])
        .map_err(|e| open(kube::Error::BuildRequest(e)))?;
    let stream = client.connect(request).await.map_err(open)?.into_stream();
    relay(stream, socket, PING_EVERY, AFTER_CLIENT_DONE).await
}

/// Relays bytes between a local connection and the port-forward stream of one port, until
/// the pod closes it, the local client goes away, or the cluster reports an error.
async fn relay<W, L>(ws: W, local: L, ping_every: Duration, after_done: Duration) -> Result<EndedBy, Failure>
where
    W: futures::Stream<Item = Result<Message, tungstenite::Error>>
        + futures::Sink<Message, Error = tungstenite::Error>
        + Unpin,
    L: AsyncRead + AsyncWrite + Unpin,
{
    let (mut to_pod, mut from_pod) = ws.split();
    let (mut reader, mut writer) = tokio::io::split(local);
    let mut buffer = vec![0u8; 32 * 1024];
    // The first frame on each channel only names the port.
    let mut named = [false, false];
    let mut client_sent = false;
    let mut client_done = false;
    // Once the client stopped sending: how long the pod may stay quiet before the end.
    let mut wait = after_done;
    let mut ping = tokio::time::interval_at(tokio::time::Instant::now() + ping_every, ping_every);
    // Once the client stopped sending: when the pod last sent something.
    let quiet = tokio::time::sleep(after_done);
    tokio::pin!(quiet);
    let result = loop {
        tokio::select! {
            read = reader.read(&mut buffer), if !client_done => match read {
                Ok(0) => {
                    // A half-close, or the client went away: wait for what the pod still sends.
                    client_done = true;
                    if !client_sent {
                        wait = after_done.min(AFTER_SILENT_CLIENT);
                    }
                    quiet.as_mut().reset(tokio::time::Instant::now() + wait);
                }
                Ok(n) => {
                    client_sent = true;
                    let mut frame = Vec::with_capacity(n + 1);
                    frame.push(0);
                    frame.extend_from_slice(&buffer[..n]);
                    if let Err(e) = to_pod.send(Message::binary(frame)).await {
                        break Err(Failure::Copy(e.to_string()));
                    }
                }
                Err(e) => break Err(Failure::Copy(e.to_string())),
            },
            message = from_pod.next() => match message {
                None | Some(Ok(Message::Close(_))) => break Ok(EndedBy::Pod),
                Some(Err(e)) => {
                    let done = client_done || closed(&mut reader, &mut buffer);
                    break failed(e.to_string(), done, &mut writer);
                }
                Some(Ok(Message::Binary(frame))) => {
                    let Some((&channel, mut data)) = frame.split_first() else { continue };
                    let channel = usize::from(channel);
                    if channel > 1 {
                        continue;
                    }
                    if !named[channel] {
                        named[channel] = true;
                        data = data.get(2..).unwrap_or_default();
                    }
                    if data.is_empty() {
                        continue;
                    }
                    if channel == 1 {
                        let message = String::from_utf8_lossy(data).trim().to_owned();
                        let done = client_done || closed(&mut reader, &mut buffer);
                        break failed(message, done, &mut writer);
                    }
                    if writer.write_all(data).await.is_err() {
                        // The client is gone for good.
                        break Ok(EndedBy::Client);
                    }
                    if client_done {
                        quiet.as_mut().reset(tokio::time::Instant::now() + wait);
                    }
                }
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if let Err(e) = to_pod.send(Message::Ping(Vec::new().into())).await {
                    break Err(Failure::Copy(e.to_string()));
                }
            }
            () = &mut quiet, if client_done => break Ok(EndedBy::Client),
        }
    };
    // The client sees the end of the pod's answer; the pod's side is closed with the stream.
    let _ = writer.shutdown().await;
    let _ = to_pod.close().await;
    result
}

/// Whether the client has closed its side already, as far as can be told without waiting. The
/// pod's error can come in before the end of the client's stream was read, when both arrive at
/// once.
fn closed<R: AsyncRead + Unpin>(reader: &mut R, buffer: &mut [u8]) -> bool {
    use futures::FutureExt;
    matches!(reader.read(buffer).now_or_never(), Some(Ok(0)))
}

/// How a connection ends on an error from the pod's side. Once the local client has stopped
/// sending, it may be gone for good, and a server then often resets the connection nobody
/// answers on: MariaDB does when a client leaves before it signed in. That is no failure of the
/// forward. The client counts as gone when writing nothing to it fails, as it does once its
/// socket was reset because Kubri passed something on after it closed. A client that only
/// stopped sending still waits for the answer, and the error is its connection's.
fn failed<W: AsyncWrite + Unpin>(
    message: String,
    client_done: bool,
    writer: &mut W,
) -> Result<EndedBy, Failure> {
    use futures::FutureExt;
    if client_done && matches!(writer.write(&[]).now_or_never(), Some(Err(_))) {
        tracing::debug!(%message, "port-forward: the pod ended a connection whose client is gone");
        return Ok(EndedBy::Client);
    }
    Err(Failure::Copy(message))
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

impl Kubri {
    /// Running port-forwards, of every context.
    pub fn port_forwards(&self) -> Vec<PortForwardInfo> {
        self.forwards.lock().unwrap().list()
    }

    /// Forwards a local port to a pod. For a Service or workload a ready pod is picked, and a
    /// Service port is mapped to the pod port it targets; when that pod goes away, the forward
    /// picks another.
    pub async fn start_port_forward(&self, request: PortForwardRequest) -> Result<PortForwardInfo> {
        validate_address(request.address.as_deref())?;
        let target = &request.target;
        let session = self.object_session(target).await?;
        let api = session.pod_api(target.namespace.as_deref())?;
        let (pod, port) = session.forward_destination(target, request.remote_port).await?;
        let repick: Option<Repick> = if session.is_core(&target.resource, "Pod")? {
            None
        } else {
            let session = Arc::clone(&session);
            let target = target.clone();
            let remote_port = request.remote_port;
            Some(Arc::new(move || {
                let session = Arc::clone(&session);
                let target = target.clone();
                Box::pin(async move {
                    let (pod, port) = session.forward_destination(&target, remote_port).await?;
                    Ok((pod.metadata.name.unwrap_or_default(), port))
                })
            }))
        };
        let destination = Destination {
            namespace: pod.metadata.namespace.clone().unwrap_or_default(),
            pod: pod.metadata.name.clone().unwrap_or_default(),
            port,
            api,
            repick,
        };
        start(&self.forwards, session.info.context.clone(), request, destination).await
    }

    /// Starts the connected context's favorite port-forwards that start on connect and are not
    /// running yet, and says which could not start.
    pub async fn start_favorite_forwards(&self) -> Result<Vec<FavoriteStart>> {
        let context = self.session().await?.info.context.clone();
        let running = self.port_forwards();
        let mut out = Vec::new();
        let favorites = self.settings().context(&context).forwards;
        for favorite in favorites.into_iter().filter(|f| f.start_on_connect) {
            let already = running
                .iter()
                .any(|f| f.context == context && favorite.matches(f));
            if already {
                continue;
            }
            let request = PortForwardRequest {
                target: ObjectRef {
                    context: Some(context.clone()),
                    ..favorite.target.clone()
                },
                remote_port: favorite.remote_port,
                local_port: Some(favorite.local_port),
                address: Some(favorite.address.clone()).filter(|a| !a.is_empty()),
            };
            let error = self
                .start_port_forward(request)
                .await
                .err()
                .map(|e| e.to_string());
            out.push(FavoriteStart { favorite, error });
        }
        Ok(out)
    }

    pub fn stop_port_forward(&self, id: u32) -> Result<()> {
        self.forwards.lock().unwrap().stop(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio_tungstenite::WebSocketStream;
    use tokio_tungstenite::tungstenite::protocol::Role;

    type PodSide = WebSocketStream<tokio::io::DuplexStream>;

    /// A relay between a local connection and a fake API server, both returned.
    async fn relaying(
        ping: Duration,
        after: Duration,
    ) -> (
        tokio::io::DuplexStream,
        PodSide,
        JoinHandle<Result<EndedBy, Failure>>,
    ) {
        let (local, client) = tokio::io::duplex(1 << 20);
        let (ours, theirs) = tokio::io::duplex(1 << 20);
        let ws = WebSocketStream::from_raw_socket(ours, Role::Client, None).await;
        let pod = WebSocketStream::from_raw_socket(theirs, Role::Server, None).await;
        let task = tokio::spawn(relay(ws, local, ping, after));
        (client, pod, task)
    }

    /// What the API server sends first: the port, on the data and the error channel.
    async fn name_port(pod: &mut PodSide) {
        pod.send(Message::binary(vec![0, 0x50, 0])).await.unwrap();
        pod.send(Message::binary(vec![1, 0x50, 0])).await.unwrap();
    }

    async fn next_data(pod: &mut PodSide) -> Vec<u8> {
        loop {
            match pod.next().await.unwrap().unwrap() {
                Message::Binary(b) => return b.to_vec(),
                _ => continue,
            }
        }
    }

    #[tokio::test]
    async fn relays_both_ways() {
        let (mut client, mut pod, task) = relaying(PING_EVERY, AFTER_CLIENT_DONE).await;
        name_port(&mut pod).await;
        client.write_all(b"ping").await.unwrap();
        assert_eq!(next_data(&mut pod).await, b"\0ping");
        pod.send(Message::binary(b"\0pong".to_vec())).await.unwrap();
        let mut answer = [0u8; 4];
        client.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"pong");
        pod.close(None).await.unwrap();
        assert!(matches!(task.await.unwrap(), Ok(EndedBy::Pod)));
        // The client sees the end.
        assert_eq!(client.read(&mut answer).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_client_that_stops_sending_still_gets_the_answer() {
        let (mut client, mut pod, task) = relaying(PING_EVERY, AFTER_CLIENT_DONE).await;
        name_port(&mut pod).await;
        client.write_all(b"GET / HTTP/1.0\r\n\r\n").await.unwrap();
        client.shutdown().await.unwrap();
        assert_eq!(next_data(&mut pod).await, b"\0GET / HTTP/1.0\r\n\r\n");
        let body = vec![7u8; 300_000];
        for chunk in body.chunks(10_000) {
            let mut frame = vec![0];
            frame.extend_from_slice(chunk);
            pod.send(Message::binary(frame)).await.unwrap();
        }
        pod.close(None).await.unwrap();
        let mut got = Vec::new();
        client.read_to_end(&mut got).await.unwrap();
        assert_eq!(got, body);
        assert!(matches!(task.await.unwrap(), Ok(EndedBy::Pod)));
    }

    #[tokio::test(start_paused = true)]
    async fn ends_when_the_pod_stays_quiet_after_the_client_is_done() {
        let (mut client, mut pod, task) = relaying(Duration::from_secs(3600), Duration::from_secs(30)).await;
        name_port(&mut pod).await;
        client.shutdown().await.unwrap();
        let pod_side = tokio::spawn(async move { while pod.next().await.is_some() {} });
        assert!(matches!(task.await.unwrap(), Ok(EndedBy::Client)));
        let mut rest = Vec::new();
        client.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
        pod_side.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn pings_a_quiet_stream() {
        let (_client, mut pod, task) = relaying(Duration::from_secs(20), AFTER_CLIENT_DONE).await;
        name_port(&mut pod).await;
        let first = pod.next().await.unwrap().unwrap();
        assert!(matches!(first, Message::Ping(_)), "unexpected {first:?}");
        task.abort();
    }

    #[tokio::test]
    async fn reports_the_clusters_error() {
        let (_client, mut pod, task) = relaying(PING_EVERY, AFTER_CLIENT_DONE).await;
        name_port(&mut pod).await;
        let mut frame = vec![1];
        frame.extend_from_slice(b"connection refused");
        pod.send(Message::binary(frame)).await.unwrap();
        match task.await.unwrap() {
            Err(Failure::Copy(message)) => assert_eq!(message, "connection refused"),
            _ => panic!("expected the error"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_client_that_sent_nothing_does_not_hold_the_pod() {
        // A check whether the port is open: connect, then close.
        let (mut client, mut pod, task) = relaying(Duration::from_secs(3600), Duration::from_secs(30)).await;
        name_port(&mut pod).await;
        client.shutdown().await.unwrap();
        let started = tokio::time::Instant::now();
        let pod_side = tokio::spawn(async move { while pod.next().await.is_some() {} });
        assert!(matches!(task.await.unwrap(), Ok(EndedBy::Client)));
        assert!(started.elapsed() <= AFTER_SILENT_CLIENT);
        pod_side.await.unwrap();
    }

    #[tokio::test]
    async fn the_pod_resetting_a_connection_whose_client_is_gone_is_no_failure() {
        let (mut client, mut pod, task) = relaying(PING_EVERY, AFTER_CLIENT_DONE).await;
        name_port(&mut pod).await;
        client.write_all(b"hello").await.unwrap();
        assert_eq!(next_data(&mut pod).await, b"\0hello");
        drop(client);
        let mut frame = vec![1];
        frame.extend_from_slice(b"read: connection reset by peer");
        pod.send(Message::binary(frame)).await.unwrap();
        assert!(matches!(task.await.unwrap(), Ok(EndedBy::Client)));
    }

    #[tokio::test]
    async fn a_client_waiting_for_the_answer_sees_the_error() {
        let (mut client, mut pod, task) = relaying(PING_EVERY, AFTER_CLIENT_DONE).await;
        name_port(&mut pod).await;
        client.write_all(b"hello").await.unwrap();
        client.shutdown().await.unwrap();
        assert_eq!(next_data(&mut pod).await, b"\0hello");
        let mut frame = vec![1];
        frame.extend_from_slice(b"read: connection reset by peer");
        pod.send(Message::binary(frame)).await.unwrap();
        match task.await.unwrap() {
            Err(Failure::Copy(message)) => assert_eq!(message, "read: connection reset by peer"),
            _ => panic!("expected the error"),
        }
    }

    #[tokio::test]
    async fn localhost_listens_on_both_loopback_addresses() {
        let listeners = listen("localhost", 0).await.unwrap();
        let port = listeners[0].local_addr().unwrap().port();
        assert_eq!(listeners[0].local_addr().unwrap().ip(), Ipv4Addr::LOCALHOST);
        let ipv6 = std::net::TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).is_ok();
        assert_eq!(listeners.len(), if ipv6 { 2 } else { 1 });
        for listener in &listeners {
            assert_eq!(listener.local_addr().unwrap().port(), port);
        }
        // Both addresses reach the forward.
        let accepting = tokio::spawn(async move {
            for _ in 0..listeners.len() {
                accept(&listeners).await.unwrap();
            }
        });
        TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.unwrap();
        if ipv6 {
            TcpStream::connect((Ipv6Addr::LOCALHOST, port)).await.unwrap();
        }
        accepting.await.unwrap();
    }

    #[tokio::test]
    async fn localhost_refuses_a_port_in_use() {
        let taken = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        assert!(listen("localhost", port).await.is_err());
        assert!(listen("127.0.0.1", port).await.is_err());
    }

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

        // A connection reaches the forward; the fake cluster refuses the stream, and its reason
        // is what the forward shows.
        let _socket = TcpStream::connect(("127.0.0.1", info.local_port)).await.unwrap();
        for _ in 0..100 {
            if forwards.lock().unwrap().list()[0].error.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let failed = forwards.lock().unwrap().list().remove(0);
        assert_eq!(failed.error.as_deref(), Some("no upgrade"));
        assert_eq!(failed.connections, 0);

        forwards.lock().unwrap().stop(info.id).unwrap();
        assert!(forwards.lock().unwrap().list().is_empty());
        assert!(matches!(
            forwards.lock().unwrap().stop(info.id),
            Err(Error::NotFound(_))
        ));
    }
}
