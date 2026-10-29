//! A pod's likely problem, as the inspector shows it: containers that wait for a reason or
//! ended in failure, and why a pending pod does not start. AI agents and Explain read it as text.

use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;

use crate::{Kubri, LogEvent, LogOptions, ObjectRef, Result};

/// How long reading logs for a report may take.
const READ_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq)]
pub struct ContainerProblem {
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
pub fn container_problems(pod: &Value) -> Vec<ContainerProblem> {
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
pub fn pending_reasons(pod: &Value) -> Vec<String> {
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

/// `namespace/name`, or the name of a cluster-scoped object.
fn place(target: &ObjectRef) -> String {
    match &target.namespace {
        Some(ns) => format!("{ns}/{}", target.name),
        None => target.name.clone(),
    }
}

impl Kubri {
    /// An object's events as lines, oldest first: when, type, reason, count and message.
    pub async fn event_lines(&self, target: &ObjectRef) -> Result<Vec<String>> {
        let value = self.get(target).await?;
        let uid = value
            .pointer("/metadata/uid")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut events = self.events(target.namespace.as_deref(), uid).await?;
        events.reverse();
        Ok(events
            .iter()
            .map(|e| {
                let when = e.last_seen.as_deref().or(e.first_seen.as_deref()).unwrap_or("-");
                let count = if e.count > 1 {
                    format!(" (x{})", e.count)
                } else {
                    String::new()
                };
                format!("{when}  {}  {}{count}: {}", e.kind, e.reason, e.message.trim())
            })
            .collect())
    }

    /// Log lines of `target` as text, each marked with its pod and container when there are
    /// several. Reading stops after 20 seconds.
    pub async fn log_text(&self, target: &ObjectRef, options: LogOptions) -> Result<String> {
        let timestamps = options.timestamps;
        let mut events = Box::pin(self.logs(target, options).await?);
        let mut sources = Vec::new();
        let mut lines = Vec::new();
        let mut errors = Vec::new();
        let read = tokio::time::timeout(READ_TIMEOUT, async {
            while let Some(event) = events.next().await {
                match event {
                    LogEvent::Sources { sources: s } => sources = s,
                    LogEvent::Lines { lines: batch } => lines.extend(batch),
                    LogEvent::Error { message, .. } => errors.push(message),
                    LogEvent::Marker { .. } => {}
                    LogEvent::End => break,
                }
            }
        })
        .await;
        let several = sources.len() > 1;
        let mut out: Vec<String> = lines
            .iter()
            .map(|line| {
                let text = if timestamps {
                    line.with_time()
                } else {
                    line.text.clone()
                };
                match sources.get(line.source) {
                    Some(s) if several => format!("[{}/{}] {text}", s.pod, s.container),
                    _ => text,
                }
            })
            .collect();
        if out.is_empty() {
            out.push("(no log lines)".into());
        }
        for error in errors {
            out.push(format!("Note: {error}"));
        }
        if read.is_err() {
            out.push("Note: stopped reading after 20 seconds.".into());
        }
        Ok(out.join("\n"))
    }

    /// What is wrong with a pod, as text: its phase, containers that crash or wait, why it
    /// does not start, its warning events and the last log lines of failing containers.
    pub async fn pod_report(&self, target: &ObjectRef, pod: &Value) -> String {
        let phase = pod
            .pointer("/status/phase")
            .and_then(Value::as_str)
            .unwrap_or("Unknown");
        let node = pod
            .pointer("/spec/nodeName")
            .and_then(Value::as_str)
            .unwrap_or("not scheduled");
        let mut out = vec![format!("Pod {} is {phase}, on node {node}.", place(target))];
        if let Some(reason) = pod.pointer("/status/reason").and_then(Value::as_str) {
            let message = pod
                .pointer("/status/message")
                .and_then(Value::as_str)
                .unwrap_or("");
            out.push(format!("Pod status: {reason} {message}").trim_end().to_owned());
        }
        let problems = container_problems(pod);
        let pending = pending_reasons(pod);
        if problems.is_empty() && pending.is_empty() {
            out.push("No container is crashing or waiting, and nothing keeps the pod from starting.".into());
        }
        for reason in &pending {
            out.push(format!("Not starting: {reason}"));
        }
        for p in &problems {
            let mut line = format!("Container {}: {}", p.container, p.reason);
            if let Some(code) = p.exit_code {
                line.push_str(&format!(", exit code {code}"));
            }
            if p.restarts > 0 {
                line.push_str(&format!(", {} restarts", p.restarts));
            }
            if let Some(message) = &p.message {
                line.push_str(&format!(". {}", message.trim()));
            }
            out.push(line);
        }
        let events = self.event_lines(target).await.unwrap_or_default();
        let warnings: Vec<_> = events.iter().filter(|e| e.contains("  Warning  ")).collect();
        if !warnings.is_empty() {
            out.push(String::new());
            out.push("Warning events:".into());
            let skip = warnings.len().saturating_sub(10);
            out.extend(warnings.into_iter().skip(skip).cloned());
        }
        for p in problems.iter().take(3) {
            // The crashed instance tells why it ended; a failed one that was not restarted is current.
            let previous = p.has_previous && p.reason != "Error" && p.reason != "Completed";
            let options = LogOptions {
                container: Some(p.container.clone()),
                tail_lines: Some(30),
                previous,
                ..Default::default()
            };
            if let Ok(logs) = self.log_text(target, options).await {
                out.push(String::new());
                out.push(format!(
                    "Last log lines of {} ({} instance):",
                    p.container,
                    if previous { "previous" } else { "current" }
                ));
                out.push(logs);
            }
        }
        out.join("\n")
    }
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
