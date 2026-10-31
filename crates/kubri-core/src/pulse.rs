//! The cluster overview: how healthy each kind of workload is, recent warnings, and how
//! much CPU and memory is in use.

use std::collections::{HashMap, HashSet};
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
    /// cert-manager Certificates that expire soon or are not ready, soonest first.
    pub certificates: Vec<CertificateAlert>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateAlert {
    pub object: ObjectRef,
    pub health: Health,
    pub detail: String,
    pub not_after: Option<String>,
}

/// Certificates shown in the overview, at most.
const CERTIFICATE_ALERTS: usize = 8;

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
    now: Timestamp,
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
    let (jobs, cron_jobs, autoscalers, claims, volumes, events, usage, certificates) = futures::join!(
        list(scoped::<Job>(c, namespace)),
        list(scoped::<CronJob>(c, namespace)),
        list(scoped::<HorizontalPodAutoscaler>(c, namespace)),
        list(scoped::<PersistentVolumeClaim>(c, namespace)),
        list(Api::<PersistentVolume>::all(c.clone())),
        list(scoped::<Event>(c, namespace)),
        metrics,
        crate::certmanager::list(c, namespace),
    );
    let (certificate_pulse, certificates) = match certificates {
        Some(listed) => certificate_pulse(listed, now),
        None => (None, Vec::new()),
    };

    // Warnings about objects a rollout or a restart already replaced are not news.
    let live = Live::of(&[
        ("Pod", uids(&pods)),
        ("ReplicaSet", uids(&replica_sets)),
        ("Deployment", uids(&deployments)),
        ("StatefulSet", uids(&stateful_sets)),
        ("DaemonSet", uids(&daemon_sets)),
        ("Job", uids(&jobs)),
    ]);

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
        ]
        .into_iter()
        .chain(certificate_pulse)
        .collect(),
        events: match events {
            Ok(events) => tally_events(events, now, &resolve, &live),
            Err(error) => EventPulse {
                error: Some(error),
                ..Default::default()
            },
        },
        cpu,
        memory,
        certificates,
    }
}

/// How healthy the Certificates are, and those that need a look.
fn certificate_pulse(
    listed: Result<Vec<serde_json::Value>, String>,
    now: Timestamp,
) -> (Option<KindPulse>, Vec<CertificateAlert>) {
    let mut pulse = KindPulse {
        kind: "Certificate".into(),
        resource: crate::certmanager::resource(),
        total: 0,
        ok: 0,
        warning: 0,
        failed: 0,
        error: None,
    };
    let certs = match listed {
        Ok(certs) => certs,
        Err(error) => {
            pulse.error = Some(error);
            return (Some(pulse), Vec::new());
        }
    };
    let mut alerts = Vec::new();
    for cert in &certs {
        let status = crate::certmanager::status(cert, now);
        pulse.total += 1;
        match status.health {
            Health::Ok => {
                pulse.ok += 1;
                continue;
            }
            Health::Warning => pulse.warning += 1,
            Health::Failed => pulse.failed += 1,
        }
        let text = |p: &str| {
            cert.pointer(p)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        alerts.push(CertificateAlert {
            object: ObjectRef {
                context: None,
                resource: crate::certmanager::resource(),
                namespace: text("/metadata/namespace"),
                name: text("/metadata/name").unwrap_or_default(),
            },
            health: status.health,
            detail: status.detail,
            not_after: text("/status/notAfter"),
        });
    }
    // Failed first, then the soonest to expire; those without a date last.
    alerts.sort_by(|a, b| {
        b.health
            .cmp(&a.health)
            .then_with(|| a.not_after.is_none().cmp(&b.not_after.is_none()))
            .then_with(|| a.not_after.cmp(&b.not_after))
    });
    alerts.truncate(CERTIFICATE_ALERTS);
    (Some(pulse), alerts)
}

/// Lists everything, reporting failures as text for [`KindPulse::error`].
pub(crate) async fn list<K>(api: Api<K>) -> Result<Vec<K>, String>
where
    K: kube::Resource + Clone + DeserializeOwned + Debug,
{
    crate::typed::list(api).await.map_err(|e| e.to_string())
}

pub(crate) fn summarize<K: kube::Resource<DynamicType = ()>>(
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
pub(crate) fn requested(pods: &[Pod], resource: &str) -> f64 {
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

pub(crate) fn allocatable(nodes: &[Node], resource: &str) -> f64 {
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

/// The UIDs of the objects that exist now, by kind. A kind that could not be listed is left
/// out, so its warnings stay.
#[derive(Debug, Default)]
struct Live(HashMap<&'static str, HashSet<String>>);

impl Live {
    fn of(kinds: &[(&'static str, Option<HashSet<String>>)]) -> Self {
        Self(
            kinds
                .iter()
                .filter_map(|(kind, uids)| Some((*kind, uids.clone()?)))
                .collect(),
        )
    }

    /// False for an event about an object of a listed kind that is gone. Events without a
    /// UID, and kinds not listed, are kept.
    fn exists(&self, kind: &str, uid: Option<&str>) -> bool {
        match (self.0.get(kind), uid) {
            (Some(uids), Some(uid)) => uids.contains(uid),
            _ => true,
        }
    }
}

fn uids<K: kube::Resource>(listed: &Result<Vec<K>, String>) -> Option<HashSet<String>> {
    listed
        .as_ref()
        .ok()
        .map(|items| items.iter().filter_map(|i| i.meta().uid.clone()).collect())
}

/// Counts the events of the last hour and keeps the newest warnings. `resolve` finds the
/// resource type of an `apiVersion` and kind, so a warning can link to its object. Warnings
/// about objects that no longer exist are counted but not kept.
fn tally_events(
    events: Vec<Event>,
    now: Timestamp,
    resolve: &impl Fn(&str, &str) -> Option<ResourceRef>,
    live: &Live,
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
        .filter_map(|(_, event)| {
            let involved = event.involved_object.clone();
            let kind = involved.kind.unwrap_or_default();
            if !live.exists(&kind, involved.uid.as_deref()) {
                return None;
            }
            let resource = resolve(involved.api_version.as_deref().unwrap_or("v1"), &kind)?;
            Some(RecentEvent {
                object_kind: kind,
                object: ObjectRef {
                    context: None,
                    resource,
                    namespace: involved.namespace,
                    name: involved.name.unwrap_or_default(),
                },
                event: EventInfo::from_event(event),
            })
        })
        .take(RECENT_WARNINGS)
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
    fn keeps_as_many_warnings_as_can_link() {
        let event = |kind: &str, minute: usize| -> Event {
            from(json!({
                "metadata": { "name": format!("e{minute}") },
                "involvedObject": { "apiVersion": "example.com/v1", "kind": kind, "name": format!("o{minute}") },
                "type": "Warning", "reason": "Failed",
                "lastTimestamp": format!("2026-10-28T07:{minute:02}:00Z")
            }))
        };
        // The newest warnings are about a kind the cluster does not serve any more.
        let events = (0..20)
            .map(|minute| event(if minute >= 15 { "Gone" } else { "Thing" }, minute))
            .collect();
        let now: Timestamp = "2026-10-28T08:00:00Z".parse().unwrap();
        let resolve = |_: &str, kind: &str| {
            (kind == "Thing").then(|| ResourceRef {
                group: "example.com".into(),
                version: "v1".into(),
                plural: "things".into(),
            })
        };
        let pulse = tally_events(events, now, &resolve, &Live::default());
        assert_eq!(pulse.warning, 20);
        assert_eq!(pulse.recent.len(), RECENT_WARNINGS);
        assert_eq!(pulse.recent[0].object.name, "o14");
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
        let pulse = tally_events(events, now, &resolve, &Live::default());
        assert_eq!((pulse.normal, pulse.warning), (1, 2));
        let names: Vec<&str> = pulse.recent.iter().map(|e| e.object.name.as_str()).collect();
        assert_eq!(names, ["b", "a"]);
        assert_eq!(pulse.recent[0].object.namespace.as_deref(), Some("shop"));
        assert_eq!(pulse.recent[0].event.reason, "BackOff");
    }

    #[test]
    fn drops_warnings_about_objects_that_are_gone() {
        let event = |name: &str, uid: &str| -> Event {
            from(json!({
                "metadata": { "name": name, "namespace": "shop" },
                "involvedObject": { "apiVersion": "v1", "kind": "Pod", "name": name, "namespace": "shop", "uid": uid },
                "type": "Warning", "reason": "BackOff", "lastTimestamp": "2026-10-28T07:50:00Z"
            }))
        };
        let events = vec![event("new", "u2"), event("old", "u1")];
        let now: Timestamp = "2026-10-28T08:00:00Z".parse().unwrap();
        let resolve = |_: &str, _: &str| {
            Some(ResourceRef {
                group: String::new(),
                version: "v1".into(),
                plural: "pods".into(),
            })
        };
        let live = Live::of(&[("Pod", Some(["u2".to_string()].into()))]);
        let pulse = tally_events(events.clone(), now, &resolve, &live);
        assert_eq!(pulse.warning, 2);
        let names: Vec<&str> = pulse.recent.iter().map(|e| e.object.name.as_str()).collect();
        assert_eq!(names, ["new"]);
        // A kind that could not be listed keeps its warnings.
        let unknown = Live::of(&[("Pod", None)]);
        assert_eq!(tally_events(events, now, &resolve, &unknown).recent.len(), 2);
    }
}
