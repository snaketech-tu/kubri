//! The fleet home page: every context at once, how to reach it and how healthy it is.

use std::time::{Duration, Instant};

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::{Api, Client};
use serde::Serialize;

use crate::health;
use crate::pulse::{KindPulse, allocatable, list, requested, summarize};
use crate::{Error, Kubri, Result, client_config};

/// How long one context may take before the page calls it unreachable.
const FLEET_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the page keeps using a client it made for a context. The page checks every
/// minute; signing in each time would run a context's exec plugin, like `aws`, each time too.
const KEEP_CLIENT_FOR: Duration = Duration::from_secs(15 * 60);

/// A client the page made for a context, the API server it talks to, and when it was made.
#[derive(Clone)]
pub(crate) struct Kept {
    made: Instant,
    server: String,
    client: Client,
}

/// One context on the fleet page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetCluster {
    pub context: String,
    pub server: Option<String>,
    /// The API server's version; `None` when it could not be reached.
    pub version: Option<String>,
    /// Why it could not be reached.
    pub error: Option<String>,
    /// How long the API server took to say its version.
    pub millis: u64,
    /// Nodes, pods, Deployments, StatefulSets and DaemonSets, healthy or not.
    pub kinds: Vec<KindPulse>,
    /// CPU requested and allocatable, in millicores.
    pub cpu: Option<(f64, f64)>,
    /// Memory requested and allocatable, in bytes.
    pub memory: Option<(f64, f64)>,
}

/// Nodes, pods and workloads of the whole cluster.
async fn gather(client: Client, cluster: &mut FleetCluster) {
    let (nodes, pods, deployments, stateful_sets, daemon_sets) = futures::join!(
        list(Api::<Node>::all(client.clone())),
        list(Api::<Pod>::all(client.clone())),
        list(Api::<Deployment>::all(client.clone())),
        list(Api::<StatefulSet>::all(client.clone())),
        list(Api::<DaemonSet>::all(client.clone())),
    );
    if let (Ok(nodes), Ok(pods)) = (&nodes, &pods) {
        cluster.cpu = Some((
            requested(pods, "cpu") * 1000.0,
            allocatable(nodes, "cpu") * 1000.0,
        ));
        cluster.memory = Some((requested(pods, "memory"), allocatable(nodes, "memory")));
    }
    cluster.kinds = vec![
        summarize(nodes, health::node),
        summarize(pods, health::pod),
        summarize(deployments, health::deployment),
        summarize(stateful_sets, health::stateful_set),
        summarize(daemon_sets, health::daemon_set),
    ];
}

impl Kubri {
    /// How `context` is doing, without connecting Kubri to it. Gives up after ten seconds,
    /// so one cluster that is down does not hold up the others.
    pub async fn fleet_cluster(&self, context: &str) -> Result<FleetCluster> {
        let kubeconfig = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => kubeconfig.clone(),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        if !kubeconfig.contexts.iter().any(|c| c.name == context) {
            return Err(Error::UnknownContext(context.to_owned()));
        }
        let mut cluster = FleetCluster {
            context: context.to_owned(),
            server: None,
            version: None,
            error: None,
            millis: 0,
            kinds: Vec::new(),
            cpu: None,
            memory: None,
        };
        let reached = tokio::time::timeout(FLEET_TIMEOUT, async {
            let client = self.fleet_client(&kubeconfig, context).await?;
            cluster.server = Some(client.server);
            let started = Instant::now();
            cluster.version = Some(client.client.apiserver_version().await?.git_version);
            cluster.millis = started.elapsed().as_millis() as u64;
            gather(client.client, &mut cluster).await;
            Ok::<_, Error>(())
        })
        .await;
        if cluster.version.is_none() {
            // Signing in again may be what helps, as after credentials changed.
            self.fleet
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(context);
        }
        match reached {
            Ok(Ok(())) => {}
            Ok(Err(e)) => cluster.error = Some(e.to_string()),
            // Reachable but slow to list: keep what came back, and say what is missing.
            Err(_) if cluster.version.is_some() => {
                cluster.error = Some("timed out while listing nodes and pods".into());
            }
            Err(_) => cluster.error = Some(Error::Timeout.to_string()),
        }
        Ok(cluster)
    }

    /// The page's client for `context`: the one it made before, while it is fresh.
    async fn fleet_client(&self, kubeconfig: &kube::config::Kubeconfig, context: &str) -> Result<Kept> {
        let kept = self
            .fleet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(context)
            .cloned();
        if let Some(kept) = kept.filter(|k| k.made.elapsed() < KEEP_CLIENT_FOR) {
            return Ok(kept);
        }
        let config = client_config(kubeconfig, context, self.act_as.as_ref()).await?;
        let kept = Kept {
            made: Instant::now(),
            server: config.cluster_url.to_string().trim_end_matches('/').to_owned(),
            client: Client::try_from(config)?,
        };
        self.fleet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(context.to_owned(), kept.clone());
        Ok(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// A Kubri whose kubeconfig has the context `far`, at an address nothing answers.
    fn kubri() -> Kubri {
        let kubeconfig = kube::config::Kubeconfig::from_yaml(
            r#"
clusters: [{name: far, cluster: {server: "https://192.0.2.1:6443"}}]
users: [{name: far, user: {token: t}}]
contexts: [{name: far, context: {cluster: far, user: far}}]
"#,
        )
        .unwrap();
        Kubri::from_kubeconfig(kubeconfig)
    }

    fn keep(kubri: &Kubri, status: u16) {
        let (client, _) = testing::client(move |r| {
            if status != 200 {
                return (status, testing::status(status, "Unauthorized"));
            }
            let body = match r.uri.as_str() {
                "/version" => serde_json::json!({
                    "major": "1", "minor": "36", "gitVersion": "v1.36.0", "gitCommit": "", "gitTreeState": "",
                    "buildDate": "", "goVersion": "", "compiler": "", "platform": ""
                }),
                _ => serde_json::json!({ "apiVersion": "v1", "kind": "List", "metadata": {}, "items": [] }),
            };
            (200, body.to_string())
        });
        let kept = Kept {
            made: Instant::now(),
            server: "https://far.example:6443".into(),
            client,
        };
        kubri.fleet.lock().unwrap().insert("far".into(), kept);
    }

    #[tokio::test]
    async fn checks_with_the_client_it_made_before() {
        let kubri = kubri();
        keep(&kubri, 200);
        let far = kubri.fleet_cluster("far").await.unwrap();
        assert_eq!(far.error, None);
        assert_eq!(far.version.as_deref(), Some("v1.36.0"));
        assert_eq!(far.server.as_deref(), Some("https://far.example:6443"));
        assert!(kubri.fleet.lock().unwrap().contains_key("far"));
    }

    #[tokio::test]
    async fn signs_in_again_after_a_client_failed() {
        let kubri = kubri();
        keep(&kubri, 401);
        let far = kubri.fleet_cluster("far").await.unwrap();
        assert_eq!(far.version, None);
        assert!(far.error.is_some());
        assert!(!kubri.fleet.lock().unwrap().contains_key("far"));
    }
}
