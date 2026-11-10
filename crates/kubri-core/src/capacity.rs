//! The room a workload has to grow: what each node can give to pods and what is already
//! requested there, on the nodes the workload's pods may run on, and the namespace's quotas.
//! The dialog for requests and limits shows it as a budget.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::core::v1::{Node, Pod, PodTemplateSpec, ResourceQuota};
use kube::Api;
use kube::api::ListParams;
use serde::Serialize;

use crate::fixes::{node_fits, pod_requests};
use crate::{Error, Kubri, ObjectRef, Result, parse_quantity};

/// CPU in cores and memory in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Amounts {
    pub cpu: f64,
    pub memory: f64,
}

impl Amounts {
    fn add(&mut self, (cpu, memory): (f64, f64)) {
        self.cpu += cpu;
        self.memory += memory;
    }
}

/// One node: what it can give to pods, what its pods request, and how much of that is the
/// workload's own pods.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NodeRoom {
    pub name: String,
    /// Whether the workload's pods may run here: ready, not cordoned, taints tolerated, node
    /// selector and required affinity met.
    pub eligible: bool,
    /// Why not, when it is not.
    pub reasons: Vec<String>,
    pub allocatable: Amounts,
    pub requested: Amounts,
    /// What the workload's own pods on this node request, part of `requested`.
    pub own: Amounts,
    pub own_pods: u32,
}

/// A ResourceQuota limit on CPU or memory in the namespace, with what is used of it.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QuotaRoom {
    pub name: String,
    /// `requests.cpu`, `requests.memory`, `limits.cpu` or `limits.memory`.
    pub resource: String,
    /// Cores or bytes.
    pub hard: f64,
    pub used: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Capacity {
    /// How many pods the workload runs: its replicas, or for a DaemonSet the nodes it wants.
    pub pods: u32,
    /// The nodes, or `None` when Kubri may not list them; `nodes_error` says why.
    pub nodes: Option<Vec<NodeRoom>>,
    pub nodes_error: Option<String>,
    pub quotas: Vec<QuotaRoom>,
}

/// Quota keys about CPU and memory, by the name the room is reported under. `cpu` and
/// `memory` alone mean requests.
const QUOTA_KEYS: [(&str, &str); 6] = [
    ("requests.cpu", "requests.cpu"),
    ("cpu", "requests.cpu"),
    ("requests.memory", "requests.memory"),
    ("memory", "requests.memory"),
    ("limits.cpu", "limits.cpu"),
    ("limits.memory", "limits.memory"),
];

fn quotas(list: &[ResourceQuota]) -> Vec<QuotaRoom> {
    let mut out = Vec::new();
    for quota in list {
        let name = quota.metadata.name.clone().unwrap_or_default();
        let Some(status) = &quota.status else { continue };
        let hard = status.hard.clone().unwrap_or_default();
        let used = status.used.clone().unwrap_or_default();
        for (key, resource) in QUOTA_KEYS {
            let Some(limit) = hard.get(key).and_then(|q| parse_quantity(&q.0)) else {
                continue;
            };
            let now = used.get(key).and_then(|q| parse_quantity(&q.0)).unwrap_or(0.0);
            out.push(QuotaRoom {
                name: name.clone(),
                resource: resource.to_owned(),
                hard: limit,
                used: now,
            });
        }
    }
    out
}

/// Whether `pod` carries every label of `selector`.
fn selected(pod: &Pod, selector: &BTreeMap<String, String>) -> bool {
    let labels = pod.metadata.labels.as_ref();
    !selector.is_empty()
        && selector
            .iter()
            .all(|(k, v)| labels.and_then(|l| l.get(k)) == Some(v))
}

/// Each node's room for a pod made from `template`, in `namespace`.
fn node_rooms(
    template: &PodTemplateSpec,
    namespace: &str,
    selector: &BTreeMap<String, String>,
    nodes: &[Node],
    pods: &[Pod],
) -> Vec<NodeRoom> {
    // Where a pod of the workload may go, leaving CPU and memory out: they are the budget.
    let mut spec = template.spec.clone().unwrap_or_default();
    for c in spec
        .containers
        .iter_mut()
        .chain(spec.init_containers.iter_mut().flatten())
    {
        c.resources = None;
    }
    let probe = Pod {
        metadata: template.metadata.clone().unwrap_or_default(),
        spec: Some(spec),
        status: None,
    };
    let fits = node_fits(&probe, nodes, pods);
    let running = |p: &&Pod| {
        !matches!(
            p.status.as_ref().and_then(|s| s.phase.as_deref()),
            Some("Succeeded" | "Failed")
        )
    };
    nodes
        .iter()
        .zip(fits)
        .map(|(node, fit)| {
            let name = node.metadata.name.clone().unwrap_or_default();
            let allocatable = node.status.as_ref().and_then(|s| s.allocatable.as_ref());
            let amount = |what: &str| {
                allocatable
                    .and_then(|a| a.get(what))
                    .and_then(|q| parse_quantity(&q.0))
                    .unwrap_or(0.0)
            };
            let mut requested = Amounts::default();
            let mut own = Amounts::default();
            let mut own_pods = 0;
            for pod in pods
                .iter()
                .filter(running)
                .filter(|p| p.spec.as_ref().and_then(|s| s.node_name.as_deref()) == Some(name.as_str()))
            {
                let wants = pod_requests(pod);
                requested.add(wants);
                if pod.metadata.namespace.as_deref() == Some(namespace) && selected(pod, selector) {
                    own.add(wants);
                    own_pods += 1;
                }
            }
            NodeRoom {
                name,
                eligible: fit.reasons.is_empty(),
                reasons: fit.reasons,
                allocatable: Amounts {
                    cpu: amount("cpu"),
                    memory: amount("memory"),
                },
                requested,
                own,
                own_pods,
            }
        })
        .collect()
}

impl Kubri {
    /// The room the workload `target` has on its nodes and in its namespace's quotas.
    pub async fn capacity(&self, target: &ObjectRef) -> Result<Capacity> {
        let session = self.object_session(target).await?;
        let client = session.client.clone();
        let namespace = target
            .namespace
            .clone()
            .ok_or_else(|| Error::Unsupported(format!("{} has no namespace", target.name)))?;
        let (template, selector, pods) =
            match (target.resource.group.as_str(), target.resource.plural.as_str()) {
                ("apps", "deployments") => {
                    let d = Api::<Deployment>::namespaced(client.clone(), &namespace)
                        .get(&target.name)
                        .await?;
                    let spec = d.spec;
                    (
                        spec.template,
                        spec.selector.match_labels.unwrap_or_default(),
                        spec.replicas.unwrap_or(1),
                    )
                }
                ("apps", "statefulsets") => {
                    let s = Api::<StatefulSet>::namespaced(client.clone(), &namespace)
                        .get(&target.name)
                        .await?;
                    let spec = s.spec;
                    (
                        spec.template,
                        spec.selector.match_labels.unwrap_or_default(),
                        spec.replicas.unwrap_or(1),
                    )
                }
                ("apps", "daemonsets") => {
                    let d = Api::<DaemonSet>::namespaced(client.clone(), &namespace)
                        .get(&target.name)
                        .await?;
                    let wanted = d.status.as_ref().map_or(0, |s| s.desired_number_scheduled);
                    let spec = d.spec;
                    (
                        spec.template,
                        spec.selector.match_labels.unwrap_or_default(),
                        wanted,
                    )
                }
                _ => {
                    return Err(Error::Unsupported(format!(
                        "{} has no pod template to size",
                        target.resource.plural
                    )));
                }
            };
        let quota_list = Api::<ResourceQuota>::namespaced(client.clone(), &namespace)
            .list(&ListParams::default())
            .await
            .map(|l| l.items)
            .unwrap_or_default();
        let (nodes, nodes_error) = match Api::<Node>::all(client.clone())
            .list(&ListParams::default())
            .await
        {
            Ok(nodes) => {
                // Without the right to list every pod, what is requested is counted from what is visible.
                let pods = Api::<Pod>::all(client.clone())
                    .list(&ListParams::default().fields("spec.nodeName!="))
                    .await
                    .map(|l| l.items)
                    .unwrap_or_default();
                (
                    Some(node_rooms(&template, &namespace, &selector, &nodes.items, &pods)),
                    None,
                )
            }
            Err(err) => (None, Some(Error::from(err).to_string())),
        };
        Ok(Capacity {
            pods: u32::try_from(pods.max(0)).unwrap_or(0),
            nodes,
            nodes_error,
            quotas: quotas(&quota_list),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn node(name: &str, extra: Value) -> Node {
        let mut node = json!({
            "metadata": { "name": name, "labels": { "kubernetes.io/hostname": name } },
            "status": {
                "allocatable": { "cpu": "2", "memory": "4Gi", "pods": "110" },
                "conditions": [{ "type": "Ready", "status": "True" }]
            }
        });
        if let (Some(node), Some(extra)) = (node.as_object_mut(), extra.as_object()) {
            for (k, v) in extra {
                node.insert(k.clone(), v.clone());
            }
        }
        from(node)
    }

    fn pod(name: &str, namespace: &str, app: &str, node: &str, cpu: &str, memory: &str) -> Pod {
        from(json!({
            "metadata": { "name": name, "namespace": namespace, "labels": { "app": app } },
            "spec": { "nodeName": node, "containers": [
                { "name": "c", "resources": { "requests": { "cpu": cpu, "memory": memory } } }
            ] },
            "status": { "phase": "Running" }
        }))
    }

    #[test]
    fn counts_room_per_node_and_the_workloads_own_pods() {
        let template: PodTemplateSpec = from(json!({
            "metadata": { "labels": { "app": "web" } },
            "spec": { "containers": [{ "name": "web", "resources": { "requests": { "cpu": "8" } } }] }
        }));
        let nodes = [
            node("a", json!({})),
            node(
                "b",
                json!({ "spec": { "taints": [{ "key": "gpu", "effect": "NoSchedule" }] } }),
            ),
            node("c", json!({ "spec": { "unschedulable": true } })),
        ];
        let pods = [
            pod("web-1", "shop", "web", "a", "500m", "256Mi"),
            pod("db-1", "shop", "db", "a", "1", "1Gi"),
            // Same labels in another namespace: not the workload's.
            pod("web-1", "other", "web", "a", "100m", "64Mi"),
        ];
        let selector = BTreeMap::from([("app".to_owned(), "web".to_owned())]);
        let rooms = node_rooms(&template, "shop", &selector, &nodes, &pods);
        let a = &rooms[0];
        // CPU of the template is left out of where it may go: 8 cores fit nowhere, but node a is fine.
        assert!(a.eligible, "{:?}", a.reasons);
        assert_eq!(
            a.allocatable,
            Amounts {
                cpu: 2.0,
                memory: 4.0 * 1024.0 * 1024.0 * 1024.0
            }
        );
        assert!((a.requested.cpu - 1.6).abs() < 1e-9);
        assert!((a.own.cpu - 0.5).abs() < 1e-9);
        assert_eq!(a.own_pods, 1);
        assert!(!rooms[1].eligible);
        assert!(rooms[1].reasons[0].contains("taint gpu"));
        assert!(!rooms[2].eligible);
    }

    #[test]
    fn reads_cpu_and_memory_quotas() {
        let quota: ResourceQuota = from(json!({
            "metadata": { "name": "team" },
            "status": {
                "hard": { "requests.cpu": "4", "memory": "8Gi", "limits.cpu": "8", "pods": "20" },
                "used": { "requests.cpu": "1500m", "memory": "2Gi" }
            }
        }));
        let rooms = quotas(&[quota]);
        assert_eq!(
            rooms
                .iter()
                .map(|q| (q.resource.as_str(), q.hard, q.used))
                .collect::<Vec<_>>(),
            vec![
                ("requests.cpu", 4.0, 1.5),
                (
                    "requests.memory",
                    8.0 * 1024.0 * 1024.0 * 1024.0,
                    2.0 * 1024.0 * 1024.0 * 1024.0
                ),
                ("limits.cpu", 8.0, 0.0),
            ]
        );
    }
}
