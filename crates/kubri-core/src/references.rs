//! What uses a ConfigMap, Secret, PersistentVolumeClaim, ServiceAccount or PriorityClass, like
//! k9s `u` (UsedBy): the workloads and bare pods whose pod spec names it.

use serde::Serialize;
use serde_json::Value;

use crate::{ResourceInfo, ResourceRef};

/// An object that uses the one asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub resource: ResourceRef,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
}

/// The kinds Kubri finds the users of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Used {
    ConfigMap,
    Secret,
    Claim,
    ServiceAccount,
    PriorityClass,
}

/// The kinds whose pod specs are searched, with their API versions. Pods only count without an
/// owner, and Jobs without a CronJob: their owner is in the list already.
pub(crate) const USERS: [(&str, &str); 6] = [
    ("apps/v1", "Deployment"),
    ("apps/v1", "StatefulSet"),
    ("apps/v1", "DaemonSet"),
    ("batch/v1", "CronJob"),
    ("batch/v1", "Job"),
    ("v1", "Pod"),
];

impl Used {
    pub fn of(resource: &ResourceInfo) -> Option<Self> {
        match (resource.reference.group.as_str(), resource.kind.as_str()) {
            ("", "ConfigMap") => Some(Self::ConfigMap),
            ("", "Secret") => Some(Self::Secret),
            ("", "PersistentVolumeClaim") => Some(Self::Claim),
            ("", "ServiceAccount") => Some(Self::ServiceAccount),
            ("scheduling.k8s.io", "PriorityClass") => Some(Self::PriorityClass),
            _ => None,
        }
    }

    /// Whether `object` of `kind`, one of [`USERS`], uses the object `name`. Items of lists
    /// come without their kind, so it is passed along.
    pub fn by(self, kind: &str, object: &Value, name: &str) -> bool {
        let owned = object
            .pointer("/metadata/ownerReferences")
            .and_then(Value::as_array)
            .is_some_and(|owners| !owners.is_empty());
        if owned && matches!(kind, "Pod" | "Job") {
            return false;
        }
        pod_spec(kind, object).is_some_and(|spec| self.in_spec(spec, name))
    }

    fn in_spec(self, spec: &Value, name: &str) -> bool {
        let is = |value: &Value| value.as_str() == Some(name);
        let volumes = list(&spec["volumes"]);
        let containers = ["containers", "initContainers", "ephemeralContainers"]
            .iter()
            .flat_map(|key| list(&spec[*key]));
        match self {
            Self::ConfigMap => {
                volumes
                    .iter()
                    .any(|v| is(&v["configMap"]["name"]) || projected(v, "configMap", name))
                    || containers.into_iter().any(|c| {
                        list(&c["env"])
                            .iter()
                            .any(|e| is(&e["valueFrom"]["configMapKeyRef"]["name"]))
                            || list(&c["envFrom"]).iter().any(|e| is(&e["configMapRef"]["name"]))
                    })
            }
            Self::Secret => {
                volumes
                    .iter()
                    .any(|v| is(&v["secret"]["secretName"]) || projected(v, "secret", name))
                    || list(&spec["imagePullSecrets"]).iter().any(|s| is(&s["name"]))
                    || containers.into_iter().any(|c| {
                        list(&c["env"])
                            .iter()
                            .any(|e| is(&e["valueFrom"]["secretKeyRef"]["name"]))
                            || list(&c["envFrom"]).iter().any(|e| is(&e["secretRef"]["name"]))
                    })
            }
            Self::Claim => volumes
                .iter()
                .any(|v| is(&v["persistentVolumeClaim"]["claimName"])),
            // Pods without one run as `default`.
            Self::ServiceAccount => spec["serviceAccountName"].as_str().unwrap_or("default") == name,
            Self::PriorityClass => is(&spec["priorityClassName"]),
        }
    }
}

/// The pod spec of a pod, a workload or a CronJob.
fn pod_spec<'a>(kind: &str, object: &'a Value) -> Option<&'a Value> {
    let pointer = match kind {
        "Pod" => "/spec",
        "CronJob" => "/spec/jobTemplate/spec/template/spec",
        _ => "/spec/template/spec",
    };
    object.pointer(pointer)
}

fn list(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

/// A projected volume with a source of `kind` named `name`.
fn projected(volume: &Value, kind: &str, name: &str) -> bool {
    list(&volume["projected"]["sources"])
        .iter()
        .any(|s| s[kind]["name"].as_str() == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deployment(spec: Value) -> Value {
        json!({ "metadata": { "name": "web" }, "spec": { "template": { "spec": spec } } })
    }

    #[test]
    fn finds_config_maps_and_secrets_wherever_a_pod_spec_names_them() {
        let web = deployment(json!({
            "volumes": [
                { "name": "a", "configMap": { "name": "settings" } },
                { "name": "b", "projected": { "sources": [{ "secret": { "name": "tls" } }] } },
                { "name": "c", "persistentVolumeClaim": { "claimName": "data" } }
            ],
            "imagePullSecrets": [{ "name": "registry" }],
            "initContainers": [{ "name": "init", "envFrom": [{ "configMapRef": { "name": "init-env" } }] }],
            "containers": [{
                "name": "app",
                "env": [{ "name": "TOKEN", "valueFrom": { "secretKeyRef": { "name": "api", "key": "token" } } }]
            }]
        }));
        assert!(Used::ConfigMap.by("Deployment", &web, "settings"));
        assert!(Used::ConfigMap.by("Deployment", &web, "init-env"));
        assert!(!Used::ConfigMap.by("Deployment", &web, "tls"));
        assert!(Used::Secret.by("Deployment", &web, "tls"));
        assert!(Used::Secret.by("Deployment", &web, "registry"));
        assert!(Used::Secret.by("Deployment", &web, "api"));
        assert!(!Used::Secret.by("Deployment", &web, "settings"));
        assert!(Used::Claim.by("Deployment", &web, "data"));
        assert!(Used::ServiceAccount.by("Deployment", &web, "default"));
        assert!(!Used::PriorityClass.by("Deployment", &web, "high"));
    }

    #[test]
    fn reads_cron_jobs_and_skips_what_an_owner_stands_for() {
        let cron = json!({
            "metadata": { "name": "backup" },
            "spec": { "jobTemplate": { "spec": { "template": { "spec": {
                "serviceAccountName": "backup", "priorityClassName": "low", "containers": []
            } } } } }
        });
        assert!(Used::ServiceAccount.by("CronJob", &cron, "backup"));
        assert!(!Used::ServiceAccount.by("CronJob", &cron, "default"));
        assert!(Used::PriorityClass.by("CronJob", &cron, "low"));

        let pod = |owners: Value| {
            json!({
                "metadata": { "name": "web-1", "ownerReferences": owners },
                "spec": { "containers": [{ "name": "app", "envFrom": [{ "secretRef": { "name": "api" } }] }] }
            })
        };
        assert!(Used::Secret.by("Pod", &pod(json!([])), "api"));
        assert!(!Used::Secret.by(
            "Pod",
            &pod(json!([{ "kind": "ReplicaSet", "name": "web-1" }])),
            "api"
        ));
    }
}
