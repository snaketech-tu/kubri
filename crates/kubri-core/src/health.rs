//! Judges whether an object is healthy, for the cluster overview and the XRay tree.

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{ContainerStatus, Node, PersistentVolume, PersistentVolumeClaim, Pod};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Ok,
    /// Not broken, but needs a look: starting, partly ready, at a limit.
    Warning,
    Failed,
}

/// A health verdict and the short reason shown next to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Status {
    pub health: Health,
    pub detail: String,
}

impl Status {
    fn new(health: Health, detail: impl Into<String>) -> Self {
        Self {
            health,
            detail: detail.into(),
        }
    }
}

/// Waiting reasons that do not resolve on their own.
const BROKEN: &[&str] = &[
    "CrashLoopBackOff",
    "ImagePullBackOff",
    "ErrImagePull",
    "ErrImageNeverPull",
    "InvalidImageName",
    "CreateContainerConfigError",
    "CreateContainerError",
    "RunContainerError",
    "PreStartHookError",
    "PostStartHookError",
];

pub(crate) fn container(status: &ContainerStatus) -> Status {
    let state = status.state.as_ref();
    let mut verdict = if let Some(waiting) = state.and_then(|s| s.waiting.as_ref()) {
        let reason = waiting.reason.clone().unwrap_or_else(|| "Waiting".to_owned());
        let health = if BROKEN.contains(&reason.as_str()) {
            Health::Failed
        } else {
            Health::Warning
        };
        Status::new(health, reason)
    } else if let Some(terminated) = state.and_then(|s| s.terminated.as_ref()) {
        if terminated.exit_code == 0 {
            Status::new(Health::Ok, "Completed")
        } else {
            let reason = terminated.reason.as_deref().unwrap_or("Error");
            Status::new(
                Health::Failed,
                format!("{reason} (exit {})", terminated.exit_code),
            )
        }
    } else if status.ready {
        Status::new(Health::Ok, "Running")
    } else if state.is_some_and(|s| s.running.is_some()) {
        Status::new(Health::Warning, "Running, not ready")
    } else {
        Status::new(Health::Warning, "Pending")
    };
    if status.restart_count > 0 {
        verdict.detail += &format!(" · {} restarts", status.restart_count);
    }
    verdict
}

pub(crate) fn pod(pod: &Pod) -> Status {
    if pod.metadata.deletion_timestamp.is_some() {
        return Status::new(Health::Warning, "Terminating");
    }
    let Some(status) = &pod.status else {
        return Status::new(Health::Warning, "Pending");
    };
    let phase = status.phase.as_deref().unwrap_or("Pending");
    match phase {
        "Succeeded" => return Status::new(Health::Ok, "Completed"),
        "Failed" => return Status::new(Health::Failed, status.reason.as_deref().unwrap_or("Failed")),
        "Unknown" => return Status::new(Health::Failed, "Unknown"),
        _ => {}
    }

    let init = status.init_container_statuses.as_deref().unwrap_or_default();
    let regular = status.container_statuses.as_deref().unwrap_or_default();
    // A failing container explains the pod better than its phase.
    let broken = init
        .iter()
        .map(|c| (c, "Init:"))
        .chain(regular.iter().map(|c| (c, "")))
        .map(|(c, prefix)| (container(c), prefix))
        .find(|(s, _)| s.health == Health::Failed);
    if let Some((s, prefix)) = broken {
        let reason = s.detail.split(" · ").next().unwrap_or_default();
        return Status::new(Health::Failed, format!("{prefix}{reason}"));
    }

    if phase == "Pending" {
        let unschedulable = status.conditions.iter().flatten().any(|c| {
            c.type_ == "PodScheduled" && c.status == "False" && c.reason.as_deref() == Some("Unschedulable")
        });
        if unschedulable {
            return Status::new(Health::Warning, "Unschedulable");
        }
        let waiting = init
            .iter()
            .find(|c| !c.ready)
            .and_then(|c| waiting_reason(c).map(|r| format!("Init:{r}")))
            .or_else(|| regular.iter().find_map(waiting_reason).map(str::to_owned));
        return Status::new(Health::Warning, waiting.unwrap_or_else(|| "Pending".to_owned()));
    }

    let ready = regular.iter().filter(|c| c.ready).count();
    if ready == regular.len() {
        Status::new(Health::Ok, "Running")
    } else {
        Status::new(
            Health::Warning,
            format!("Running · {ready}/{} ready", regular.len()),
        )
    }
}

fn waiting_reason(status: &ContainerStatus) -> Option<&str> {
    status.state.as_ref()?.waiting.as_ref()?.reason.as_deref()
}

/// Shared by the workloads that keep a number of ready pods.
fn replicas(ready: i32, desired: i32) -> Status {
    let detail = format!("{ready}/{desired} ready");
    if desired == 0 {
        Status::new(Health::Ok, "scaled to 0")
    } else if ready == 0 {
        Status::new(Health::Failed, detail)
    } else if ready < desired {
        Status::new(Health::Warning, detail)
    } else {
        Status::new(Health::Ok, detail)
    }
}

pub(crate) fn deployment(deployment: &Deployment) -> Status {
    let desired = deployment.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
    let status = deployment.status.as_ref();
    let stuck = status
        .and_then(|s| s.conditions.as_ref())
        .into_iter()
        .flatten()
        .any(|c| c.type_ == "Progressing" && c.reason.as_deref() == Some("ProgressDeadlineExceeded"));
    let mut verdict = replicas(status.and_then(|s| s.ready_replicas).unwrap_or(0), desired);
    if stuck {
        verdict.health = Health::Failed;
        verdict.detail += " · rollout stuck";
    }
    verdict
}

pub(crate) fn stateful_set(set: &StatefulSet) -> Status {
    let desired = set.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
    replicas(
        set.status.as_ref().and_then(|s| s.ready_replicas).unwrap_or(0),
        desired,
    )
}

pub(crate) fn daemon_set(set: &DaemonSet) -> Status {
    let Some(status) = &set.status else {
        return Status::new(Health::Warning, "Pending");
    };
    if status.desired_number_scheduled == 0 {
        return Status::new(Health::Ok, "on no nodes");
    }
    replicas(status.number_ready, status.desired_number_scheduled)
}

pub(crate) fn replica_set(set: &ReplicaSet) -> Status {
    let desired = set.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
    replicas(
        set.status.as_ref().and_then(|s| s.ready_replicas).unwrap_or(0),
        desired,
    )
}

pub(crate) fn job(job: &Job) -> Status {
    let status = job.status.as_ref();
    let condition = |kind: &str| {
        status
            .and_then(|s| s.conditions.as_ref())
            .into_iter()
            .flatten()
            .find(|c| c.type_ == kind && c.status == "True")
    };
    let completions = job.spec.as_ref().and_then(|s| s.completions).unwrap_or(1);
    let succeeded = status.and_then(|s| s.succeeded).unwrap_or(0);
    let failed = status.and_then(|s| s.failed).unwrap_or(0);
    if condition("Complete").is_some() {
        return Status::new(Health::Ok, format!("Complete {succeeded}/{completions}"));
    }
    if let Some(c) = condition("Failed") {
        return Status::new(
            Health::Failed,
            c.reason.clone().unwrap_or_else(|| "Failed".to_owned()),
        );
    }
    if job.spec.as_ref().and_then(|s| s.suspend) == Some(true) {
        return Status::new(Health::Ok, "Suspended");
    }
    if failed > 0 {
        return Status::new(Health::Warning, format!("Running · {failed} failed"));
    }
    Status::new(Health::Ok, format!("Running {succeeded}/{completions}"))
}

pub(crate) fn cron_job(cron: &CronJob) -> Status {
    let schedule = cron.spec.schedule.as_str();
    if cron.spec.suspend == Some(true) {
        return Status::new(Health::Ok, format!("{schedule} · suspended"));
    }
    let status = cron.status.as_ref();
    let active = status
        .and_then(|s| s.active.as_ref())
        .is_some_and(|a| !a.is_empty());
    let scheduled = status.and_then(|s| s.last_schedule_time.as_ref()).map(|t| t.0);
    let succeeded = status.and_then(|s| s.last_successful_time.as_ref()).map(|t| t.0);
    // A run counts as done once a newer success is recorded; until then it is active.
    let last_failed = match (scheduled, succeeded) {
        (Some(s), Some(ok)) => ok < s,
        (Some(_), None) => true,
        _ => false,
    };
    if last_failed && !active {
        Status::new(Health::Warning, format!("{schedule} · last run did not succeed"))
    } else {
        Status::new(Health::Ok, schedule)
    }
}

pub(crate) fn autoscaler(hpa: &HorizontalPodAutoscaler) -> Status {
    let max = hpa.spec.max_replicas;
    let status = hpa.status.as_ref();
    let current = status.and_then(|s| s.current_replicas).unwrap_or(0);
    let failing = status
        .and_then(|s| s.conditions.as_ref())
        .into_iter()
        .flatten()
        .find(|c| matches!(c.type_.as_str(), "AbleToScale" | "ScalingActive") && c.status == "False");
    if let Some(c) = failing {
        return Status::new(
            Health::Warning,
            c.reason.clone().unwrap_or_else(|| c.type_.clone()),
        );
    }
    let detail = format!("{current}/{max} replicas");
    if max > 0 && current >= max {
        Status::new(Health::Warning, format!("{detail} · at maximum"))
    } else {
        Status::new(Health::Ok, detail)
    }
}

pub(crate) fn claim(pvc: &PersistentVolumeClaim) -> Status {
    let phase = pvc
        .status
        .as_ref()
        .and_then(|s| s.phase.as_deref())
        .unwrap_or("Pending");
    let health = match phase {
        "Bound" => Health::Ok,
        "Lost" => Health::Failed,
        _ => Health::Warning,
    };
    let size = pvc
        .status
        .as_ref()
        .and_then(|s| s.capacity.as_ref())
        .and_then(|c| c.get("storage"))
        .map(|q| format!(" · {}", q.0))
        .unwrap_or_default();
    Status::new(health, format!("{phase}{size}"))
}

pub(crate) fn volume(pv: &PersistentVolume) -> Status {
    let phase = pv
        .status
        .as_ref()
        .and_then(|s| s.phase.as_deref())
        .unwrap_or("Pending");
    let health = match phase {
        "Bound" | "Available" => Health::Ok,
        "Failed" => Health::Failed,
        _ => Health::Warning,
    };
    Status::new(health, phase)
}

pub(crate) fn node(node: &Node) -> Status {
    let conditions = node
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .map(Vec::as_slice)
        .unwrap_or_default();
    let ready = conditions
        .iter()
        .any(|c| c.type_ == "Ready" && c.status == "True");
    if !ready {
        return Status::new(Health::Failed, "NotReady");
    }
    let pressure: Vec<&str> = conditions
        .iter()
        .filter(|c| c.type_.ends_with("Pressure") && c.status == "True")
        .map(|c| c.type_.as_str())
        .collect();
    if !pressure.is_empty() {
        return Status::new(Health::Warning, pressure.join(", "));
    }
    if node.spec.as_ref().and_then(|s| s.unschedulable) == Some(true) {
        return Status::new(Health::Warning, "Ready · SchedulingDisabled");
    }
    Status::new(Health::Ok, "Ready")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn from<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn pod_with(phase: &str, containers: serde_json::Value) -> Pod {
        from(json!({
            "metadata": { "name": "p" },
            "status": { "phase": phase, "containerStatuses": containers }
        }))
    }

    fn running(name: &str, ready: bool, restarts: i32) -> serde_json::Value {
        json!({ "name": name, "image": "i", "imageID": "", "ready": ready,
                "restartCount": restarts, "state": { "running": {} } })
    }

    fn waiting(name: &str, reason: &str) -> serde_json::Value {
        json!({ "name": name, "image": "i", "imageID": "", "ready": false,
                "restartCount": 3, "state": { "waiting": { "reason": reason } } })
    }

    #[test]
    fn judges_pods() {
        let ok = pod(&pod_with(
            "Running",
            json!([running("a", true, 0), running("b", true, 0)]),
        ));
        assert_eq!(ok, Status::new(Health::Ok, "Running"));

        let partly = pod(&pod_with(
            "Running",
            json!([running("a", true, 0), running("b", false, 0)]),
        ));
        assert_eq!(partly, Status::new(Health::Warning, "Running · 1/2 ready"));

        let crashing = pod(&pod_with(
            "Running",
            json!([running("a", true, 0), waiting("b", "CrashLoopBackOff")]),
        ));
        assert_eq!(crashing, Status::new(Health::Failed, "CrashLoopBackOff"));

        let creating = pod(&pod_with("Pending", json!([waiting("a", "ContainerCreating")])));
        assert_eq!(creating, Status::new(Health::Warning, "ContainerCreating"));

        let done = pod(&pod_with("Succeeded", json!([])));
        assert_eq!(done.health, Health::Ok);

        let evicted: Pod = from(json!({ "metadata": { "name": "p" },
                                        "status": { "phase": "Failed", "reason": "Evicted" } }));
        assert_eq!(pod(&evicted), Status::new(Health::Failed, "Evicted"));
    }

    #[test]
    fn failing_init_containers_are_named() {
        let p: Pod = from(json!({ "metadata": { "name": "p" }, "status": {
            "phase": "Pending",
            "initContainerStatuses": [waiting("migrate", "ImagePullBackOff")],
            "containerStatuses": [waiting("app", "PodInitializing")]
        }}));
        assert_eq!(pod(&p), Status::new(Health::Failed, "Init:ImagePullBackOff"));
    }

    #[test]
    fn unschedulable_pods_say_so() {
        let p: Pod = from(json!({ "metadata": { "name": "p" }, "status": {
            "phase": "Pending",
            "conditions": [{ "type": "PodScheduled", "status": "False", "reason": "Unschedulable" }]
        }}));
        assert_eq!(pod(&p), Status::new(Health::Warning, "Unschedulable"));
    }

    #[test]
    fn terminating_pods_are_warnings() {
        let p: Pod = from(
            json!({ "metadata": { "name": "p", "deletionTimestamp": "2026-10-28T08:00:00Z" },
                                  "status": { "phase": "Running" } }),
        );
        assert_eq!(pod(&p).detail, "Terminating");
    }

    #[test]
    fn judges_containers() {
        let c: ContainerStatus = from(running("a", true, 2));
        assert_eq!(container(&c), Status::new(Health::Ok, "Running · 2 restarts"));
        let c: ContainerStatus = from(json!({ "name": "a", "image": "i", "imageID": "", "ready": false,
            "restartCount": 0, "state": { "terminated": { "exitCode": 137, "reason": "OOMKilled" } } }));
        assert_eq!(container(&c), Status::new(Health::Failed, "OOMKilled (exit 137)"));
    }

    #[test]
    fn judges_deployments_by_ready_replicas() {
        let d = |replicas: i32, ready: i32| -> Deployment {
            from(json!({ "metadata": { "name": "d" }, "spec": {
                "replicas": replicas, "selector": {}, "template": {}
            }, "status": { "readyReplicas": ready } }))
        };
        assert_eq!(deployment(&d(3, 3)), Status::new(Health::Ok, "3/3 ready"));
        assert_eq!(deployment(&d(3, 1)).health, Health::Warning);
        assert_eq!(deployment(&d(3, 0)).health, Health::Failed);
        assert_eq!(deployment(&d(0, 0)), Status::new(Health::Ok, "scaled to 0"));

        let stuck: Deployment = from(json!({ "metadata": { "name": "d" },
            "spec": { "replicas": 2, "selector": {}, "template": {} },
            "status": { "readyReplicas": 2, "conditions": [{
                "type": "Progressing", "status": "False", "reason": "ProgressDeadlineExceeded" }] } }));
        assert_eq!(deployment(&stuck).health, Health::Failed);
    }

    #[test]
    fn judges_jobs() {
        let j = |status: serde_json::Value| -> Job {
            from(json!({ "metadata": { "name": "j" }, "spec": { "template": {} }, "status": status }))
        };
        let complete = j(json!({ "succeeded": 1,
            "conditions": [{ "type": "Complete", "status": "True" }] }));
        assert_eq!(job(&complete), Status::new(Health::Ok, "Complete 1/1"));
        let failed = j(json!({ "failed": 6, "conditions": [{
            "type": "Failed", "status": "True", "reason": "BackoffLimitExceeded" }] }));
        assert_eq!(job(&failed), Status::new(Health::Failed, "BackoffLimitExceeded"));
        assert_eq!(
            job(&j(json!({ "active": 1, "failed": 1 }))).health,
            Health::Warning
        );
    }

    #[test]
    fn cron_jobs_warn_when_the_last_run_failed() {
        let c = |status: serde_json::Value| -> CronJob {
            from(json!({ "metadata": { "name": "c" },
                         "spec": { "schedule": "*/5 * * * *", "jobTemplate": {} }, "status": status }))
        };
        let ok = c(json!({ "lastScheduleTime": "2026-10-28T08:00:00Z",
                           "lastSuccessfulTime": "2026-10-28T08:00:30Z" }));
        assert_eq!(cron_job(&ok), Status::new(Health::Ok, "*/5 * * * *"));
        let failed = c(json!({ "lastScheduleTime": "2026-10-28T08:05:00Z",
                               "lastSuccessfulTime": "2026-10-28T08:00:30Z" }));
        assert_eq!(cron_job(&failed).health, Health::Warning);
        assert_eq!(cron_job(&c(json!({}))).health, Health::Ok);
    }

    #[test]
    fn judges_nodes() {
        let n = |ready: &str, extra: serde_json::Value, unschedulable: bool| -> Node {
            let mut conditions = vec![json!({ "type": "Ready", "status": ready })];
            conditions.extend(extra.as_array().cloned().unwrap_or_default());
            from(
                json!({ "metadata": { "name": "n" }, "spec": { "unschedulable": unschedulable },
                         "status": { "conditions": conditions } }),
            )
        };
        assert_eq!(
            node(&n("True", json!([]), false)),
            Status::new(Health::Ok, "Ready")
        );
        assert_eq!(node(&n("False", json!([]), false)).health, Health::Failed);
        assert_eq!(node(&n("True", json!([]), true)).health, Health::Warning);
        let pressure = node(&n(
            "True",
            json!([{ "type": "DiskPressure", "status": "True" }]),
            false,
        ));
        assert_eq!(pressure, Status::new(Health::Warning, "DiskPressure"));
    }

    #[test]
    fn judges_volumes() {
        let pvc: PersistentVolumeClaim = from(json!({ "metadata": { "name": "c" },
            "status": { "phase": "Bound", "capacity": { "storage": "10Gi" } } }));
        assert_eq!(claim(&pvc), Status::new(Health::Ok, "Bound · 10Gi"));
        let pv: PersistentVolume =
            from(json!({ "metadata": { "name": "v" }, "status": { "phase": "Released" } }));
        assert_eq!(volume(&pv).health, Health::Warning);
    }

    #[test]
    fn autoscalers_warn_at_their_maximum() {
        let h: HorizontalPodAutoscaler = from(json!({ "metadata": { "name": "h" },
            "spec": { "maxReplicas": 5, "scaleTargetRef": { "kind": "Deployment", "name": "d" } },
            "status": { "currentReplicas": 5, "desiredReplicas": 5 } }));
        assert_eq!(
            autoscaler(&h),
            Status::new(Health::Warning, "5/5 replicas · at maximum")
        );
    }
}
