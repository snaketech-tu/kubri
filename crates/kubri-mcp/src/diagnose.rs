//! A pod's likely problem, as the inspector shows it: containers that wait for a reason or
//! ended in failure, and why a pending pod does not start.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContainerProblem {
    pub container: String,
    pub reason: String,
    pub message: Option<String>,
    pub restarts: i64,
    /// The exit code of the instance that ended last.
    pub exit_code: Option<i64>,
    /// An instance ended before this one, whose logs Kubernetes keeps.
    pub has_previous: bool,
}

/// Containers that wait for a reason, such as CrashLoopBackOff, or that ended in failure.
pub(crate) fn container_problems(pod: &Value) -> Vec<ContainerProblem> {
    let statuses = ["initContainerStatuses", "containerStatuses"]
        .iter()
        .filter_map(|field| pod.pointer(&format!("/status/{field}"))?.as_array())
        .flatten();
    statuses
        .filter_map(|status| {
            let waiting = status.pointer("/state/waiting");
            let terminated = status.pointer("/state/terminated");
            let exit = |t: Option<&Value>| t.and_then(|t| t.get("exitCode")).and_then(Value::as_i64);
            let failed_now = exit(terminated).is_some_and(|code| code != 0);
            let waiting_reason = waiting.and_then(|w| w.get("reason")).and_then(Value::as_str);
            // ContainerCreating and PodInitializing are how every container starts.
            let stuck = waiting_reason.is_some_and(|r| r != "ContainerCreating" && r != "PodInitializing");
            if !stuck && !failed_now {
                return None;
            }
            let text = |v: Option<&Value>, key: &str| {
                v.and_then(|v| v.get(key))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            };
            let last = status.pointer("/lastState/terminated");
            Some(ContainerProblem {
                container: status
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                reason: text(waiting, "reason")
                    .or_else(|| text(terminated, "reason"))
                    .unwrap_or_else(|| "Exited".into()),
                message: text(waiting, "message").or_else(|| text(terminated, "message")),
                restarts: status
                    .get("restartCount")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                exit_code: if failed_now { exit(terminated) } else { exit(last) },
                has_previous: last.is_some(),
            })
        })
        .collect()
}

/// Why a pending pod does not start, from its conditions, as the scheduler reports it.
pub(crate) fn pending_reasons(pod: &Value) -> Vec<String> {
    if pod.pointer("/status/phase").and_then(Value::as_str) != Some("Pending") {
        return Vec::new();
    }
    pod.pointer("/status/conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|c| c.get("status").and_then(Value::as_str) == Some("False"))
        .filter_map(|c| {
            let message = c.get("message").and_then(Value::as_str)?;
            let kind = c.get("type").and_then(Value::as_str).unwrap_or("Condition");
            Some(format!("{kind}: {message}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_crash_loops_and_failures() {
        let pod = json!({ "status": { "phase": "Running", "containerStatuses": [
            { "name": "app", "restartCount": 7,
              "state": { "waiting": { "reason": "CrashLoopBackOff", "message": "back-off 5m0s" } },
              "lastState": { "terminated": { "exitCode": 1, "reason": "Error" } } },
            { "name": "sidecar", "restartCount": 0, "state": { "running": {} } },
            { "name": "starting", "restartCount": 0, "state": { "waiting": { "reason": "ContainerCreating" } } }
        ] } });
        assert_eq!(
            container_problems(&pod),
            [ContainerProblem {
                container: "app".into(),
                reason: "CrashLoopBackOff".into(),
                message: Some("back-off 5m0s".into()),
                restarts: 7,
                exit_code: Some(1),
                has_previous: true,
            }]
        );
    }

    #[test]
    fn says_why_a_pod_is_pending() {
        let pod = json!({ "status": { "phase": "Pending", "conditions": [
            { "type": "PodScheduled", "status": "False", "message": "0/1 nodes are available: 1 Insufficient memory." },
            { "type": "Initialized", "status": "True" }
        ] } });
        assert_eq!(
            pending_reasons(&pod),
            ["PodScheduled: 0/1 nodes are available: 1 Insufficient memory."]
        );
    }
}
