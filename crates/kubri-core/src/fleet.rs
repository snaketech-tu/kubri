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
        let started = Instant::now();
        let reached = tokio::time::timeout(FLEET_TIMEOUT, async {
            let config = client_config(&kubeconfig, context).await?;
            cluster.server = Some(config.cluster_url.to_string().trim_end_matches('/').to_owned());
            let client = Client::try_from(config)?;
            cluster.version = Some(client.apiserver_version().await?.git_version);
            cluster.millis = started.elapsed().as_millis() as u64;
            gather(client, &mut cluster).await;
            Ok::<_, Error>(())
        })
        .await;
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
}
