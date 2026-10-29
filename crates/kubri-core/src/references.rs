//! What uses a ConfigMap, Secret, PersistentVolumeClaim, ServiceAccount, PriorityClass or
//! Service, like k9s `u` (UsedBy): the workloads and bare pods whose pod spec names it, and the
//! Ingresses and HTTPRoutes that send traffic to a Service or serve TLS from a Secret. The
//! delete dialog lists them as what deleting it breaks.

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
    /// How it uses it, when not through its pod spec: `TLS` or `backend`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub how: Option<String>,
}

/// The kinds Kubri finds the users of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Used {
    ConfigMap,
    Secret,
    Claim,
    ServiceAccount,
    PriorityClass,
    Service,
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

/// What routes traffic: Ingresses name Services as backends and Secrets for TLS.
const INGRESS: (&str, &str) = ("networking.k8s.io/v1", "Ingress");
const ROUTE: (&str, &str) = ("gateway.networking.k8s.io/v1", "HTTPRoute");

impl Used {
    pub fn of(resource: &ResourceInfo) -> Option<Self> {
        match (resource.reference.group.as_str(), resource.kind.as_str()) {
            ("", "ConfigMap") => Some(Self::ConfigMap),
            ("", "Secret") => Some(Self::Secret),
            ("", "PersistentVolumeClaim") => Some(Self::Claim),
            ("", "ServiceAccount") => Some(Self::ServiceAccount),
            ("scheduling.k8s.io", "PriorityClass") => Some(Self::PriorityClass),
            ("", "Service") => Some(Self::Service),
            _ => None,
        }
    }

    /// The kinds that may use it, with their API versions.
    pub fn users(self) -> Vec<(&'static str, &'static str)> {
        match self {
            Self::Service => vec![INGRESS, ROUTE],
            Self::Secret => USERS.iter().copied().chain([INGRESS]).collect(),
            _ => USERS.to_vec(),
        }
    }

    /// How `object` of `kind` uses the object `name`, when it is not through a pod spec.
    pub fn how(self, kind: &str) -> Option<&'static str> {
        match (self, kind) {
            (Self::Secret, "Ingress") => Some("TLS"),
            (Self::Service, _) => Some("backend"),
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
        let is = |value: &Value| value.as_str() == Some(name);
        match (self, kind) {
            (Self::Secret, "Ingress") => {
                return list(&object["spec"]["tls"]).iter().any(|t| is(&t["secretName"]));
            }
            (Self::Service, "Ingress") => {
                let spec = &object["spec"];
                return is(&spec["defaultBackend"]["service"]["name"])
                    || list(&spec["rules"]).iter().any(|r| {
                        list(&r["http"]["paths"])
                            .iter()
                            .any(|p| is(&p["backend"]["service"]["name"]))
                    });
            }
            // Backends in another namespace are not found: only this namespace is listed.
            (Self::Service, "HTTPRoute") => {
                return list(&object["spec"]["rules"]).iter().any(|r| {
                    list(&r["backendRefs"]).iter().any(|b| {
                        b["kind"].as_str().unwrap_or("Service") == "Service"
                            && b["group"].as_str().unwrap_or("").is_empty()
                            && b["namespace"].is_null()
                            && is(&b["name"])
                    })
                });
            }
            (Self::Service, _) => return false,
            _ => {}
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
            Self::Service => false,
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
    fn finds_ingresses_and_routes_of_services_and_tls_secrets() {
        let ingress = json!({
            "metadata": { "name": "shop" },
            "spec": {
                "tls": [{ "hosts": ["shop.example"], "secretName": "shop-tls" }],
                "defaultBackend": { "service": { "name": "fallback", "port": { "number": 80 } } },
                "rules": [{ "http": { "paths": [{ "path": "/", "backend": { "service": { "name": "web" } } }] } }]
            }
        });
        assert!(Used::Secret.by("Ingress", &ingress, "shop-tls"));
        assert!(!Used::Secret.by("Ingress", &ingress, "web"));
        assert!(Used::Service.by("Ingress", &ingress, "web"));
        assert!(Used::Service.by("Ingress", &ingress, "fallback"));
        assert!(!Used::Service.by("Ingress", &ingress, "shop-tls"));
        assert_eq!(Used::Secret.how("Ingress"), Some("TLS"));
        assert_eq!(Used::Secret.how("Deployment"), None);

        let route = json!({
            "metadata": { "name": "shop" },
            "spec": { "rules": [{ "backendRefs": [
                { "name": "web", "port": 80 },
                { "name": "other", "namespace": "elsewhere", "port": 80 },
                { "kind": "Backend", "group": "example.com", "name": "custom" }
            ] }] }
        });
        assert!(Used::Service.by("HTTPRoute", &route, "web"));
        assert!(!Used::Service.by("HTTPRoute", &route, "other"));
        assert!(!Used::Service.by("HTTPRoute", &route, "custom"));
        assert!(!Used::Service.by("Deployment", &deployment(json!({})), "web"));
        assert!(Used::Secret.users().contains(&INGRESS));
        assert!(!Used::ConfigMap.users().contains(&INGRESS));
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
