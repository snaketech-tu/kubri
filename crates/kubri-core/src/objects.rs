use serde::{Deserialize, Serialize};

use crate::ResourceRef;

/// Addresses one object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub resource: ResourceRef,
    /// `None` for cluster-scoped objects.
    #[serde(default)]
    pub namespace: Option<String>,
    pub name: String,
}

impl std::fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{} {}/{}", self.resource, ns, self.name),
            None => write!(f, "{} {}", self.resource, self.name),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Propagation {
    #[default]
    Background,
    Foreground,
    Orphan,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeleteOptions {
    /// Seconds to wait before killing; `Some(0)` with `force` deletes immediately.
    pub grace_period_seconds: Option<u32>,
    /// Delete immediately without waiting for confirmation from the kubelet.
    pub force: bool,
    pub propagation: Propagation,
}

impl DeleteOptions {
    pub(crate) fn to_params(&self) -> kube::api::DeleteParams {
        let grace = if self.force {
            Some(0)
        } else {
            self.grace_period_seconds
        };
        kube::api::DeleteParams {
            grace_period_seconds: grace,
            propagation_policy: Some(match self.propagation {
                Propagation::Background => kube::api::PropagationPolicy::Background,
                Propagation::Foreground => kube::api::PropagationPolicy::Foreground,
                Propagation::Orphan => kube::api::PropagationPolicy::Orphan,
            }),
            ..Default::default()
        }
    }
}

/// The merge patch `kubectl rollout restart` sends.
pub(crate) fn restart_patch(now: &str) -> serde_json::Value {
    serde_json::json!({
        "spec": { "template": { "metadata": { "annotations": {
            "kubectl.kubernetes.io/restartedAt": now
        } } } }
    })
}

/// An event about an object, as shown on its details page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventInfo {
    /// `Normal` or `Warning`.
    pub kind: String,
    pub reason: String,
    pub message: String,
    pub count: i32,
    pub source: String,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
}

impl EventInfo {
    pub(crate) fn from_event(event: k8s_openapi::api::core::v1::Event) -> Self {
        let time =
            |t: Option<k8s_openapi::apimachinery::pkg::apis::meta::v1::Time>| t.map(|t| t.0.to_string());
        let event_time = event.event_time.map(|t| t.0.to_string());
        let source = event
            .reporting_component
            .filter(|c| !c.is_empty())
            .or_else(|| event.source.and_then(|s| s.component))
            .unwrap_or_default();
        let first_seen = time(event.first_timestamp).or_else(|| event_time.clone());
        let last_seen = event
            .series
            .and_then(|s| s.last_observed_time.map(|t| t.0.to_string()))
            .or_else(|| time(event.last_timestamp))
            .or(event_time)
            .or_else(|| first_seen.clone());
        Self {
            kind: event.type_.unwrap_or_else(|| "Normal".to_owned()),
            reason: event.reason.unwrap_or_default(),
            message: event.message.unwrap_or_default().trim().to_owned(),
            count: event.count.unwrap_or(1),
            source,
            first_seen,
            last_seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_delete_uses_zero_grace_period() {
        let params = DeleteOptions {
            grace_period_seconds: Some(30),
            force: true,
            ..Default::default()
        }
        .to_params();
        assert_eq!(params.grace_period_seconds, Some(0));
    }

    #[test]
    fn restart_patch_sets_restarted_at_annotation() {
        let patch = restart_patch("2026-10-27T22:00:00Z");
        assert_eq!(
            patch.pointer("/spec/template/metadata/annotations/kubectl.kubernetes.io~1restartedAt"),
            Some(&serde_json::json!("2026-10-27T22:00:00Z"))
        );
    }

    #[test]
    fn object_ref_deserializes_without_namespace() {
        let obj: ObjectRef = serde_json::from_value(serde_json::json!({
            "resource": { "group": "", "version": "v1", "plural": "nodes" },
            "name": "worker-1"
        }))
        .unwrap();
        assert_eq!(obj.namespace, None);
        assert_eq!(obj.to_string(), "v1/nodes worker-1");
    }
}
