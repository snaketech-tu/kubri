//! Node maintenance: cordon, uncordon and drain, with the safety checks of `kubectl drain`.

use futures::{StreamExt, stream};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::{DeleteParams, EvictParams};
use serde::{Deserialize, Serialize};

const MIRROR_POD_ANNOTATION: &str = "kubernetes.io/config.mirror";
const PARALLEL_EVICTIONS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DrainOptions {
    /// Leave DaemonSet pods running; their controller would recreate them on the node anyway.
    pub ignore_daemonsets: bool,
    /// Evict pods with `emptyDir` volumes, losing that data.
    pub delete_emptydir_data: bool,
    /// Also evict pods that no controller will recreate.
    pub force: bool,
    pub grace_period_seconds: Option<u32>,
}

impl Default for DrainOptions {
    fn default() -> Self {
        Self {
            ignore_daemonsets: true,
            delete_emptydir_data: false,
            force: false,
            grace_period_seconds: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodIssue {
    /// `namespace/name`.
    pub pod: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrainReport {
    /// `namespace/name` of every evicted pod.
    pub evicted: Vec<String>,
    /// Pods drain leaves alone, like DaemonSet and static pods.
    pub skipped: Vec<PodIssue>,
    /// Pods that stopped the drain or could not be evicted. If any pod is blocked by the
    /// options, nothing is evicted.
    pub failed: Vec<PodIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Plan {
    Evict,
    Skip(String),
    Block(String),
}

/// What drain does with `pod`, following the filters of `kubectl drain`.
pub(crate) fn plan(pod: &Pod, options: &DrainOptions) -> Plan {
    let meta = &pod.metadata;
    if meta.deletion_timestamp.is_some() {
        return Plan::Skip("already terminating".into());
    }
    if meta
        .annotations
        .as_ref()
        .is_some_and(|a| a.contains_key(MIRROR_POD_ANNOTATION))
    {
        return Plan::Skip("static pod, managed by the kubelet".into());
    }
    let controller = meta
        .owner_references
        .iter()
        .flatten()
        .find(|o| o.controller == Some(true));
    if controller.is_some_and(|c| c.kind == "DaemonSet") {
        return if options.ignore_daemonsets {
            Plan::Skip("managed by a DaemonSet".into())
        } else {
            Plan::Block("managed by a DaemonSet; enable \"ignore DaemonSets\"".into())
        };
    }
    let phase = pod.status.as_ref().and_then(|s| s.phase.as_deref());
    if matches!(phase, Some("Succeeded" | "Failed")) {
        return Plan::Evict;
    }
    let uses_empty_dir = pod
        .spec
        .as_ref()
        .and_then(|s| s.volumes.as_ref())
        .is_some_and(|v| v.iter().any(|v| v.empty_dir.is_some()));
    if uses_empty_dir && !options.delete_emptydir_data {
        return Plan::Block("has emptyDir data; enable \"delete emptyDir data\"".into());
    }
    if controller.is_none() && !options.force {
        return Plan::Block("not managed by a controller and would be lost; enable \"force\"".into());
    }
    Plan::Evict
}

pub(crate) fn pod_name(pod: &Pod) -> String {
    format!(
        "{}/{}",
        pod.metadata.namespace.as_deref().unwrap_or_default(),
        pod.metadata.name.as_deref().unwrap_or_default()
    )
}

/// Evicts `pods`, respecting PodDisruptionBudgets, and records the outcome in `report`.
pub(crate) async fn evict_all(
    client: kube::Client,
    pods: Vec<Pod>,
    options: &DrainOptions,
    report: &mut DrainReport,
) {
    let params = EvictParams {
        delete_options: Some(DeleteParams {
            grace_period_seconds: options.grace_period_seconds,
            ..Default::default()
        }),
        ..Default::default()
    };
    let results: Vec<(String, Result<(), String>)> = stream::iter(pods)
        .map(|pod| {
            let client = client.clone();
            let params = &params;
            async move {
                let name = pod_name(&pod);
                let namespace = pod.metadata.namespace.clone().unwrap_or_default();
                let api: Api<Pod> = Api::namespaced(client, &namespace);
                let result = match api
                    .evict(pod.metadata.name.as_deref().unwrap_or_default(), params)
                    .await
                {
                    Ok(_) => Ok(()),
                    // Already gone counts as evicted.
                    Err(kube::Error::Api(status)) if status.code == 404 => Ok(()),
                    Err(kube::Error::Api(status)) if status.code == 429 => {
                        Err("a PodDisruptionBudget does not allow evicting it right now".to_owned())
                    }
                    Err(err) => Err(crate::Error::from(err).to_string()),
                };
                (name, result)
            }
        })
        .buffer_unordered(PARALLEL_EVICTIONS)
        .collect()
        .await;
    for (pod, result) in results {
        match result {
            Ok(()) => report.evicted.push(pod),
            Err(reason) => report.failed.push(PodIssue { pod, reason }),
        }
    }
    report.evicted.sort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn pod(extra: Value) -> Pod {
        let mut pod = json!({
            "metadata": {
                "name": "web-1",
                "namespace": "shop",
                "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "web",
                                      "uid": "1", "controller": true }]
            },
            "spec": { "containers": [{ "name": "app" }] },
            "status": { "phase": "Running" }
        });
        merge(&mut pod, extra);
        serde_json::from_value(pod).unwrap()
    }

    fn merge(target: &mut Value, patch: Value) {
        match (target, patch) {
            (Value::Object(target), Value::Object(patch)) => {
                for (k, v) in patch {
                    merge(target.entry(k).or_insert(Value::Null), v);
                }
            }
            (target, patch) => *target = patch,
        }
    }

    fn defaults() -> DrainOptions {
        DrainOptions::default()
    }

    #[test]
    fn replicated_pods_are_evicted() {
        assert_eq!(plan(&pod(json!({})), &defaults()), Plan::Evict);
    }

    #[test]
    fn daemonset_pods_are_skipped_unless_not_ignored() {
        let ds = pod(json!({ "metadata": { "ownerReferences": [
            { "apiVersion": "apps/v1", "kind": "DaemonSet", "name": "agent", "uid": "2", "controller": true }
        ] } }));
        assert!(matches!(plan(&ds, &defaults()), Plan::Skip(_)));
        let strict = DrainOptions {
            ignore_daemonsets: false,
            ..defaults()
        };
        assert!(matches!(plan(&ds, &strict), Plan::Block(_)));
    }

    #[test]
    fn static_and_terminating_pods_are_skipped() {
        let mirror = pod(json!({ "metadata": { "annotations": { "kubernetes.io/config.mirror": "abc" } } }));
        assert!(matches!(plan(&mirror, &defaults()), Plan::Skip(_)));
        let terminating = pod(json!({ "metadata": { "deletionTimestamp": "2026-10-27T10:00:00Z" } }));
        assert!(matches!(plan(&terminating, &defaults()), Plan::Skip(_)));
    }

    #[test]
    fn unmanaged_pods_need_force() {
        let bare = pod(json!({ "metadata": { "ownerReferences": [] } }));
        assert!(matches!(plan(&bare, &defaults()), Plan::Block(_)));
        let forced = DrainOptions {
            force: true,
            ..defaults()
        };
        assert_eq!(plan(&bare, &forced), Plan::Evict);
    }

    #[test]
    fn empty_dir_pods_need_permission_unless_finished() {
        let cache = pod(json!({ "spec": { "volumes": [{ "name": "cache", "emptyDir": {} }] } }));
        assert!(matches!(plan(&cache, &defaults()), Plan::Block(_)));
        let allowed = DrainOptions {
            delete_emptydir_data: true,
            ..defaults()
        };
        assert_eq!(plan(&cache, &allowed), Plan::Evict);
        let done = pod(json!({
            "metadata": { "ownerReferences": [] },
            "spec": { "volumes": [{ "name": "cache", "emptyDir": {} }] },
            "status": { "phase": "Succeeded" }
        }));
        assert_eq!(plan(&done, &defaults()), Plan::Evict);
    }

    #[test]
    fn drain_options_default_to_ignoring_daemonsets() {
        let options: DrainOptions = serde_json::from_value(json!({ "force": true })).unwrap();
        assert!(options.ignore_daemonsets && options.force && !options.delete_emptydir_data);
    }

    #[tokio::test]
    async fn eviction_outcomes_are_reported() {
        let (client, requests) = crate::testing::client(|req| match req.uri.as_str() {
            uri if uri.contains("/pods/web-1/") => (201, crate::testing::status(201, "")),
            uri if uri.contains("/pods/db-0/") => (429, crate::testing::status(429, "Cannot evict pod")),
            _ => (404, crate::testing::status(404, "not found")),
        });
        let named = |name: &str| pod(json!({ "metadata": { "name": name } }));
        let mut report = DrainReport::default();
        let options = DrainOptions {
            grace_period_seconds: Some(5),
            ..defaults()
        };
        evict_all(
            client,
            vec![named("web-1"), named("db-0"), named("gone")],
            &options,
            &mut report,
        )
        .await;

        assert_eq!(report.evicted, ["shop/gone", "shop/web-1"]);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].pod, "shop/db-0");
        assert!(report.failed[0].reason.contains("PodDisruptionBudget"));
        let requests = requests.lock().unwrap();
        assert!(
            requests
                .iter()
                .all(|r| r.method == "POST" && r.uri.contains("/eviction"))
        );
    }
}
