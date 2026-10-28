//! The cluster overview: how healthy each kind of workload is, recent warnings, and how
//! much CPU and memory is in use.

use std::fmt::Debug;
use std::future::Future;

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{Event, Node, PersistentVolume, PersistentVolumeClaim, Pod};
use k8s_openapi::jiff::{SignedDuration, Timestamp};
use kube::{Api, Client};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::health::{self, Health, Status};
use crate::typed::{resource_ref, scoped};
use crate::{EventInfo, ObjectRef, ResourceRef, Result, Usage, parse_quantity};

/// Warning events older than this are left out of the overview.
const RECENT: SignedDuration = SignedDuration::from_hours(1);
const RECENT_WARNINGS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pulse {
    pub kinds: Vec<KindPulse>,
    pub events: EventPulse,
    /// In millicores.
    pub cpu: Utilization,
    /// In bytes.
    pub memory: Utilization,
}

/// How many objects of one kind are healthy.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindPulse {
    pub kind: String,
    pub resource: ResourceRef,
    pub total: usize,
    pub ok: usize,
    pub warning: usize,
    pub failed: usize,
    /// Why the kind could not be listed, for example missing permissions.
    pub error: Option<String>,
}

/// Events of the last hour.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPulse {
    pub normal: usize,
    pub warning: usize,
    /// The newest warnings, newest first.
    pub recent: Vec<RecentEvent>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEvent {
    #[serde(flatten)]
    pub event: EventInfo,
    /// The object the event is about, such as `Pod`.
    pub object_kind: String,
    pub object: ObjectRef,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Utilization {
    /// Measured by metrics-server; `None` when it is not installed.
    pub used: Option<f64>,
    /// Requested by the pods that are not finished.
    pub requested: Option<f64>,
    /// Allocatable on all nodes.
    pub capacity: Option<f64>,
}

/// Lists everything the overview needs at once. Kinds that cannot be listed are reported
/// in [`KindPulse::error`] instead of failing the whole overview.
pub(crate) async fn gather(
    client: Client,
    namespace: Option<&str>,
    metrics: impl Future<Output = Result<Vec<Usage>>>,
    resolve: impl Fn(&str, &str) -> Option<ResourceRef>,
) -> Pulse {
    let c = &client;
    let (nodes, pods, deployments, stateful_sets, daemon_sets, replica_sets) = futures::join!(
        list(Api::<Node>::all(c.clone())),
        list(scoped::<Pod>(c, namespace)),
        list(scoped::<Deployment>(c, namespace)),
        list(scoped::<StatefulSet>(c, namespace)),
        list(scoped::<DaemonSet>(c, namespace)),
        list(scoped::<ReplicaSet>(c, namespace)),
    );
    let (jobs, cron_jobs, autoscalers, claims, volumes, events, usage) = futures::join!(
        list(scoped::<Job>(c, namespace)),
        list(scoped::<CronJob>(c, namespace)),
        list(scoped::<HorizontalPodAutoscaler>(c, namespace)),
        list(scoped::<PersistentVolumeClaim>(c, namespace)),
        list(Api::<PersistentVolume>::all(c.clone())),
        list(scoped::<Event>(c, namespace)),
        metrics,
    );

    let usage = usage.ok();
    let used = |pick: fn(&Usage) -> f64| usage.as_ref().map(|u| u.iter().map(pick).sum());
    let cpu = Utilization {
        used: used(|u| u.cpu),
        requested: pods.as_ref().ok().map(|p| requested(p, "cpu") * 1000.0),
        capacity: nodes.as_ref().ok().map(|n| allocatable(n, "cpu") * 1000.0),
    };
    let memory = Utilization {
        used: used(|u| u.memory),
        requested: pods.as_ref().ok().map(|p| requested(p, "memory")),
        capacity: nodes.as_ref().ok().map(|n| allocatable(n, "memory")),
    };

    Pulse {
        kinds: vec![
            summarize(nodes, health::node),
            summarize(pods, health::pod),
            summarize(deployments, health::deployment),
            summarize(stateful_sets, health::stateful_set),
            summarize(daemon_sets, health::daemon_set),
            summarize(replica_sets, health::replica_set),
            summarize(jobs, health::job),
            summarize(cron_jobs, health::cron_job),
            summarize(autoscalers, health::autoscaler),
            summarize(claims, health::claim),
            summarize(volumes, health::volume),
        ],
        events: match events {
            Ok(events) => tally_events(events, Timestamp::now(), &resolve),
            Err(error) => EventPulse {
                error: Some(error),
                ..Default::default()
            },
        },
        cpu,
        memory,
    }
}

/// Lists everything, reporting failures as text for [`KindPulse::error`].
async fn list<K>(api: Api<K>) -> Result<Vec<K>, String>
where
    K: kube::Resource + Clone + DeserializeOwned + Debug,
{
    crate::typed::list(api).await.map_err(|e| e.to_string())
}

fn summarize<K: kube::Resource<DynamicType = ()>>(
    items: Result<Vec<K>, String>,
    check: fn(&K) -> Status,
) -> KindPulse {
    let mut pulse = KindPulse {
        kind: K::kind(&()).into_owned(),
        resource: resource_ref::<K>(),
        total: 0,
        ok: 0,
        warning: 0,
        failed: 0,
        error: None,
    };
    match items {
        Ok(items) => {
            pulse.total = items.len();
            for item in &items {
                match check(item).health {
                    Health::Ok => pulse.ok += 1,
                    Health::Warning => pulse.warning += 1,
                    Health::Failed => pulse.failed += 1,
                }
            }
        }
        Err(error) => pulse.error = Some(error),
    }
    pulse
}

/// What the scheduler reserves for pods that still run: the larger of the sum of the
/// containers and the largest init container, as `kubectl describe node` counts it.
fn requested(pods: &[Pod], resource: &str) -> f64 {
    let request = |c: &k8s_openapi::api::core::v1::Container| {
        c.resources
            .as_ref()
            .and_then(|r| r.requests.as_ref())
            .and_then(|r| r.get(resource))
            .and_then(|q| parse_quantity(&q.0))
            .unwrap_or(0.0)
    };
    pods.iter()
        .filter(|p| {
            let phase = p.status.as_ref().and_then(|s| s.phase.as_deref());
            !matches!(phase, Some("Succeeded" | "Failed"))
        })
        .filter_map(|p| p.spec.as_ref())
        .map(|spec| {
            let containers: f64 = spec.containers.iter().map(request).sum();
            let init = spec
                .init_containers
                .iter()
                .flatten()
                .map(request)
                .fold(0.0, f64::max);
            containers.max(init)
        })
        .sum()
}

fn allocatable(nodes: &[Node], resource: &str) -> f64 {
    nodes
        .iter()
        .filter_map(|n| n.status.as_ref()?.allocatable.as_ref()?.get(resource))
        .filter_map(|q| parse_quantity(&q.0))
        .sum()
}

fn last_seen(event: &Event) -> Option<Timestamp> {
    event
        .series
        .as_ref()
        .and_then(|s| s.last_observed_time.as_ref())
        .map(|t| t.0)
        .or_else(|| event.last_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.event_time.as_ref().map(|t| t.0))
        .or_else(|| event.first_timestamp.as_ref().map(|t| t.0))
}

/// Counts the events of the last hour and keeps the newest warnings. `resolve` finds the
/// resource type of an `apiVersion` and kind, so a warning can link to its object.
fn tally_events(
    events: Vec<Event>,
    now: Timestamp,
    resolve: &impl Fn(&str, &str) -> Option<ResourceRef>,
) -> EventPulse {
    let since = now - RECENT;
    let mut pulse = EventPulse::default();
    let mut warnings = Vec::new();
    for event in events {
        let Some(seen) = last_seen(&event).filter(|t| *t >= since) else {
            continue;
        };
        if event.type_.as_deref() == Some("Warning") {
            pulse.warning += 1;
            warnings.push((seen, event));
        } else {
            pulse.normal += 1;
        }
    }
    warnings.sort_by_key(|w| std::cmp::Reverse(w.0));
    pulse.recent = warnings
        .into_iter()
        .take(RECENT_WARNINGS)
        .filter_map(|(_, event)| {
            let involved = event.involved_object.clone();
            let kind = involved.kind.unwrap_or_default();
            let resource = resolve(involved.api_version.as_deref().unwrap_or("v1"), &kind)?;
            Some(RecentEvent {
                object_kind: kind,
                object: ObjectRef {
                    resource,
                    namespace: involved.namespace,
                    name: involved.name.unwrap_or_default(),
                },
                event: EventInfo::from_event(event),
            })
        })
        .collect();
    pulse
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn from<T: DeserializeOwned>(value: serde_json::Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn counts_health_per_kind() {
        let pods: Vec<Pod> = vec![
            from(json!({ "metadata": { "name": "a" }, "status": { "phase": "Succeeded" } })),
            from(json!({ "metadata": { "name": "b" }, "status": { "phase": "Pending" } })),
            from(json!({ "metadata": { "name": "c" }, "status": { "phase": "Failed" } })),
        ];
        let pulse = summarize(Ok(pods), health::pod);
        assert_eq!(pulse.kind, "Pod");
        assert_eq!(pulse.resource.plural, "pods");
        assert_eq!((pulse.total, pulse.ok, pulse.warning, pulse.failed), (3, 1, 1, 1));

        let denied = summarize::<Node>(Err("forbidden".into()), health::node);
        assert_eq!(denied.error.as_deref(), Some("forbidden"));
        assert_eq!(denied.resource.group, "");
        assert_eq!(resource_ref::<Deployment>().group, "apps");
    }

    #[test]
    fn sums_requests_of_running_pods() {
        let pod = |phase: &str| -> Pod {
            from(json!({ "metadata": { "name": "p" }, "spec": {
                "containers": [
                    { "name": "a", "resources": { "requests": { "cpu": "100m", "memory": "64Mi" } } },
                    { "name": "b", "resources": { "requests": { "cpu": "50m" } } }
                ],
                "initContainers": [
                    { "name": "i", "resources": { "requests": { "cpu": "500m" } } }
                ]
            }, "status": { "phase": phase } }))
        };
        let pods = [pod("Running"), pod("Succeeded"), pod("Pending")];
        assert!((requested(&pods, "cpu") - 1.0).abs() < 1e-9);
        assert_eq!(requested(&pods, "memory"), 2.0 * 64.0 * 1024.0 * 1024.0);
    }

    #[test]
    fn sums_node_allocatable() {
        let node = |cpu: &str| -> Node {
            from(
                json!({ "metadata": { "name": "n" }, "status": { "allocatable": { "cpu": cpu, "memory": "1Gi" } } }),
            )
        };
        assert_eq!(allocatable(&[node("2"), node("1500m")], "cpu"), 3.5);
    }

    #[test]
    fn keeps_recent_warnings_newest_first() {
        let event = |name: &str, kind: &str, time: &str| -> Event {
            from(json!({
                "metadata": { "name": name, "namespace": "shop" },
                "involvedObject": { "apiVersion": "v1", "kind": "Pod", "name": name, "namespace": "shop" },
                "type": kind, "reason": "BackOff", "message": "restarting", "lastTimestamp": time
            }))
        };
        let events = vec![
            event("old", "Warning", "2026-10-28T06:00:00Z"),
            event("a", "Warning", "2026-10-28T07:40:00Z"),
            event("b", "Warning", "2026-10-28T07:50:00Z"),
            event("c", "Normal", "2026-10-28T07:55:00Z"),
        ];
        let now: Timestamp = "2026-10-28T08:00:00Z".parse().unwrap();
        let resolve = |api_version: &str, kind: &str| {
            (api_version == "v1" && kind == "Pod").then(|| ResourceRef {
                group: String::new(),
                version: "v1".into(),
                plural: "pods".into(),
            })
        };
        let pulse = tally_events(events, now, &resolve);
        assert_eq!((pulse.normal, pulse.warning), (1, 2));
        let names: Vec<&str> = pulse.recent.iter().map(|e| e.object.name.as_str()).collect();
        assert_eq!(names, ["b", "a"]);
        assert_eq!(pulse.recent[0].object.namespace.as_deref(), Some("shop"));
        assert_eq!(pulse.recent[0].event.reason, "BackOff");
    }
}
