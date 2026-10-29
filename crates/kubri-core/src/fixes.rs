//! What to do about the likely problem of a pod or Service: a higher memory limit for an
//! OOMKilled container, why each node refuses a pending pod, and why a Service has no endpoints.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::core::v1::{Container, Node, NodeSelectorRequirement, Pod, Service, Taint, Toleration};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::Api;
use kube::api::ListParams;
use serde::Serialize;
use serde_json::{Value, json};

use crate::metrics::parse_quantity;
use crate::{Error, Kubri, ObjectRef, ResourceRef, Result};

const MI: f64 = 1024.0 * 1024.0;

/// A higher memory limit for a container that ran out of memory, set on the workload that
/// owns the pod, since a running pod's limits do not change.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFix {
    /// The Deployment, StatefulSet or DaemonSet to change; `None` for a pod nothing owns.
    pub workload: Option<ObjectRef>,
    pub container: String,
    pub limit: Option<String>,
    pub request: Option<String>,
    /// About twice the limit, in Mi.
    pub suggested: String,
}

/// Whether the pending pod fits on one node, and if not, why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeFit {
    pub node: String,
    pub reasons: Vec<String>,
}

/// Why a Service sends traffic to no pod, part by part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCheck {
    pub selector: BTreeMap<String, String>,
    /// Pods the selector picks.
    pub pods: Vec<PodMatch>,
    /// Pods that miss only some of the selector's labels, with what differs.
    pub near: Vec<NearMiss>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodMatch {
    pub name: String,
    pub ready: bool,
    /// Ports of the Service whose named target port the pod does not have.
    pub missing_ports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearMiss {
    pub name: String,
    /// Such as `app is web-v2, not web`.
    pub differences: Vec<String>,
}

fn quantity(q: Option<&k8s_openapi::apimachinery::pkg::api::resource::Quantity>) -> Option<f64> {
    q.and_then(|q| parse_quantity(&q.0))
}

/// A memory amount as a whole number of Mi, rounded up to 64Mi.
fn mebibytes(bytes: f64) -> String {
    let mi = (bytes / MI / 64.0).ceil().max(1.0) * 64.0;
    format!("{mi}Mi")
}

/// About twice the limit, or the request, or 512Mi where neither is set.
pub(crate) fn suggested_memory(container: &Container) -> String {
    let resources = container.resources.as_ref();
    let limit = quantity(resources.and_then(|r| r.limits.as_ref()?.get("memory")));
    let request = quantity(resources.and_then(|r| r.requests.as_ref()?.get("memory")));
    match limit.or(request) {
        Some(bytes) => mebibytes((bytes * 2.0).max(bytes + 64.0 * MI)),
        None => "512Mi".into(),
    }
}

/// A strategic merge patch setting one container's memory limit in a pod template, merged by
/// container name.
pub(crate) fn memory_patch(container: &str, memory: &str) -> Result<Value> {
    let memory = memory.trim();
    if !parse_quantity(memory).is_some_and(|b| b > 0.0) {
        return Err(Error::Unsupported(format!(
            "`{memory}` is not an amount of memory, such as 512Mi"
        )));
    }
    Ok(json!({ "spec": { "template": { "spec": { "containers": [
        { "name": container, "resources": { "limits": { "memory": memory } } }
    ] } } } }))
}

/// What a pod asks for: CPU in cores and memory in bytes, as the scheduler counts it, with init
/// containers counted one at a time.
fn pod_requests(pod: &Pod) -> (f64, f64) {
    let Some(spec) = &pod.spec else { return (0.0, 0.0) };
    let of = |c: &Container, what: &str| {
        quantity(c.resources.as_ref().and_then(|r| r.requests.as_ref()?.get(what)))
    };
    let sum = |what: &str| {
        spec.containers
            .iter()
            .map(|c| of(c, what).unwrap_or(0.0))
            .sum::<f64>()
    };
    let init = |what: &str| {
        spec.init_containers
            .iter()
            .flatten()
            .map(|c| of(c, what).unwrap_or(0.0))
            .fold(0.0, f64::max)
    };
    (sum("cpu").max(init("cpu")), sum("memory").max(init("memory")))
}

fn tolerates(tolerations: &[Toleration], taint: &Taint) -> bool {
    tolerations.iter().any(|t| {
        let key = t.key.as_deref().unwrap_or_default();
        let exists = t.operator.as_deref() == Some("Exists");
        let key_ok = (key.is_empty() && exists) || key == taint.key;
        let value_ok =
            exists || t.value.as_deref().unwrap_or_default() == taint.value.as_deref().unwrap_or_default();
        let effect_ok = t
            .effect
            .as_deref()
            .is_none_or(|e| e.is_empty() || e == taint.effect);
        key_ok && value_ok && effect_ok
    })
}

fn requirement_met(r: &NodeSelectorRequirement, value: Option<&str>) -> bool {
    let values = r.values.as_deref().unwrap_or_default();
    match r.operator.as_str() {
        "In" => value.is_some_and(|v| values.iter().any(|w| w == v)),
        "NotIn" => value.is_none_or(|v| !values.iter().any(|w| w == v)),
        "Exists" => value.is_some(),
        "DoesNotExist" => value.is_none(),
        "Gt" | "Lt" => {
            let (Some(v), Some(w)) = (
                value.and_then(|v| v.parse::<i64>().ok()),
                values.first().and_then(|w| w.parse::<i64>().ok()),
            ) else {
                return false;
            };
            if r.operator == "Gt" { v > w } else { v < w }
        }
        _ => false,
    }
}

fn describe_requirement(r: &NodeSelectorRequirement) -> String {
    let values = r.values.as_deref().unwrap_or_default().join(", ");
    match r.operator.as_str() {
        "In" => format!("{} in ({values})", r.key),
        "NotIn" => format!("{} not in ({values})", r.key),
        "Exists" => format!("{} set", r.key),
        "DoesNotExist" => format!("{} not set", r.key),
        op => format!("{} {op} {values}", r.key),
    }
}

fn cpu(cores: f64) -> String {
    if cores < 1.0 {
        format!("{}m", (cores * 1000.0).round())
    } else {
        format!("{}", (cores * 100.0).round() / 100.0)
    }
}

fn memory(bytes: f64) -> String {
    if bytes >= 1024.0 * MI {
        format!("{:.1}Gi", bytes / (1024.0 * MI))
    } else {
        format!("{}Mi", (bytes / MI).round())
    }
}

/// Why `pod` would not be scheduled on each node, as far as Kubri can tell: the node's state,
/// taints, node selector, required node affinity, CPU, memory and pod count. Pod affinity,
/// topology spread and volumes are left to the scheduler's own message.
pub(crate) fn node_fits(pod: &Pod, nodes: &[Node], pods: &[Pod]) -> Vec<NodeFit> {
    let spec = pod.spec.clone().unwrap_or_default();
    let tolerations = spec.tolerations.clone().unwrap_or_default();
    let (want_cpu, want_memory) = pod_requests(pod);
    let me = pod.metadata.uid.as_deref();
    nodes
        .iter()
        .map(|node| {
            let name = node.metadata.name.clone().unwrap_or_default();
            let labels = node.metadata.labels.clone().unwrap_or_default();
            let mut reasons = Vec::new();
            if let Some(wanted) = &spec.node_name
                && wanted != &name
            {
                reasons.push(format!("the pod is bound to node {wanted}"));
            }
            let status = node.status.as_ref();
            let ready = status
                .and_then(|s| s.conditions.as_ref())
                .and_then(|c| c.iter().find(|c| c.type_ == "Ready"))
                .is_some_and(|c| c.status == "True");
            if !ready {
                reasons.push("the node is not ready".into());
            }
            let node_spec = node.spec.clone().unwrap_or_default();
            if node_spec.unschedulable == Some(true) {
                reasons.push("the node is cordoned".into());
            }
            for taint in node_spec.taints.iter().flatten() {
                if matches!(taint.effect.as_str(), "NoSchedule" | "NoExecute")
                    && !tolerates(&tolerations, taint)
                {
                    // Cordoning shows as a taint too; it is already said.
                    if taint.key == "node.kubernetes.io/unschedulable"
                        && node_spec.unschedulable == Some(true)
                    {
                        continue;
                    }
                    let value = taint
                        .value
                        .as_deref()
                        .map(|v| format!("={v}"))
                        .unwrap_or_default();
                    reasons.push(format!(
                        "taint {}{value}:{} is not tolerated",
                        taint.key, taint.effect
                    ));
                }
            }
            for (key, value) in spec.node_selector.iter().flatten() {
                match labels.get(key) {
                    Some(v) if v == value => {}
                    Some(v) => reasons.push(format!(
                        "node selector wants {key}={value}, the node has {key}={v}"
                    )),
                    None => reasons.push(format!(
                        "node selector wants {key}={value}, the node has no {key} label"
                    )),
                }
            }
            let required = spec
                .affinity
                .as_ref()
                .and_then(|a| a.node_affinity.as_ref())
                .and_then(|a| a.required_during_scheduling_ignored_during_execution.as_ref());
            if let Some(required) = required {
                // Terms are ORed, the requirements within a term ANDed.
                let mut failed = Vec::new();
                let any = required.node_selector_terms.iter().any(|term| {
                    let unmet: Vec<String> = term
                        .match_expressions
                        .iter()
                        .flatten()
                        .filter(|r| !requirement_met(r, labels.get(&r.key).map(String::as_str)))
                        .chain(
                            term.match_fields
                                .iter()
                                .flatten()
                                .filter(|r| r.key == "metadata.name" && !requirement_met(r, Some(&name))),
                        )
                        .map(describe_requirement)
                        .collect();
                    let ok = unmet.is_empty();
                    failed.extend(unmet);
                    ok
                });
                if !any {
                    reasons.push(format!(
                        "required node affinity is not met: {}",
                        failed.join("; ")
                    ));
                }
            }
            let allocatable = status.and_then(|s| s.allocatable.as_ref());
            let free =
                |what: &str, used: f64| allocatable.and_then(|a| quantity(a.get(what))).map(|a| a - used);
            let here: Vec<&Pod> = pods
                .iter()
                .filter(|p| {
                    p.spec.as_ref().and_then(|s| s.node_name.as_deref()) == Some(name.as_str())
                        && p.metadata.uid.as_deref() != me
                        && !matches!(
                            p.status.as_ref().and_then(|s| s.phase.as_deref()),
                            Some("Succeeded" | "Failed")
                        )
                })
                .collect();
            let (used_cpu, used_memory) = here
                .iter()
                .map(|p| pod_requests(p))
                .fold((0.0, 0.0), |(c, m), (pc, pm)| (c + pc, m + pm));
            if want_cpu > 0.0
                && let Some(left) = free("cpu", used_cpu)
                && left < want_cpu
            {
                reasons.push(format!(
                    "not enough CPU: the pod requests {}, {} is free",
                    cpu(want_cpu),
                    cpu(left.max(0.0))
                ));
            }
            if want_memory > 0.0
                && let Some(left) = free("memory", used_memory)
                && left < want_memory
            {
                reasons.push(format!(
                    "not enough memory: the pod requests {}, {} is free",
                    memory(want_memory),
                    memory(left.max(0.0))
                ));
            }
            if let Some(max) = free("pods", here.len() as f64)
                && max < 1.0
            {
                reasons.push("the node runs as many pods as it may".into());
            }
            NodeFit { node: name, reasons }
        })
        .collect()
}

fn pod_ready(pod: &Pod) -> bool {
    pod.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|c| c.iter().find(|c| c.type_ == "Ready"))
        .is_some_and(|c| c.status == "True")
}

/// Which pods a Service's selector picks, whether they are ready and have its named target
/// ports, and which pods nearly match.
pub(crate) fn service_check(service: &Service, pods: &[Pod]) -> ServiceCheck {
    let spec = service.spec.clone().unwrap_or_default();
    let selector = spec.selector.clone().unwrap_or_default();
    let mut check = ServiceCheck {
        selector: selector.clone(),
        pods: Vec::new(),
        near: Vec::new(),
    };
    if selector.is_empty() {
        return check;
    }
    let named_ports: Vec<(String, String)> = spec
        .ports
        .iter()
        .flatten()
        .filter_map(|p| match &p.target_port {
            Some(IntOrString::String(name)) => {
                Some((p.name.clone().unwrap_or_else(|| p.port.to_string()), name.clone()))
            }
            _ => None,
        })
        .collect();
    for pod in pods {
        if pod.metadata.deletion_timestamp.is_some() {
            continue;
        }
        let name = pod.metadata.name.clone().unwrap_or_default();
        let labels = pod.metadata.labels.clone().unwrap_or_default();
        let differences: Vec<String> = selector
            .iter()
            .filter_map(|(k, v)| match labels.get(k) {
                Some(have) if have == v => None,
                Some(have) => Some(format!("{k} is {have}, not {v}")),
                None => Some(format!("no {k} label (wants {v})")),
            })
            .collect();
        if differences.is_empty() {
            let declared: Vec<&str> = pod
                .spec
                .iter()
                .flat_map(|s| s.containers.iter())
                .flat_map(|c| c.ports.iter().flatten())
                .filter_map(|p| p.name.as_deref())
                .collect();
            let missing_ports = named_ports
                .iter()
                .filter(|(_, target)| !declared.contains(&target.as_str()))
                .map(|(port, target)| format!("{port} → {target}"))
                .collect();
            check.pods.push(PodMatch {
                name,
                ready: pod_ready(pod),
                missing_ports,
            });
        } else if differences.len() < selector.len() {
            check.near.push(NearMiss { name, differences });
        }
    }
    check.near.truncate(10);
    check
}

fn workload_ref(
    group: &str,
    plural: &str,
    namespace: &str,
    name: &str,
    context: Option<String>,
) -> ObjectRef {
    ObjectRef {
        context,
        resource: ResourceRef {
            group: group.into(),
            version: "v1".into(),
            plural: plural.into(),
        },
        namespace: Some(namespace.into()),
        name: name.into(),
    }
}

impl Kubri {
    /// A higher memory limit for `container` of the pod `target`, on the workload that owns it.
    pub async fn memory_fix(&self, target: &ObjectRef, container: &str) -> Result<MemoryFix> {
        let session = self.object_session(target).await?;
        let namespace = target.namespace.clone().unwrap_or_default();
        let api = session.pod_api(Some(&namespace))?;
        let pod = api.get(&target.name).await?;
        let found = pod
            .spec
            .iter()
            .flat_map(|s| s.containers.iter())
            .find(|c| c.name == container)
            .ok_or_else(|| Error::Unsupported(format!("pod {} has no container {container}", target.name)))?;
        let resources = found.resources.as_ref();
        let context = Some(session.info.context.clone());
        let owner = pod
            .metadata
            .owner_references
            .iter()
            .flatten()
            .find(|o| o.controller == Some(true))
            .cloned();
        let workload = match owner {
            Some(o) if o.kind == "ReplicaSet" => {
                let rs = Api::<ReplicaSet>::namespaced(session.client.clone(), &namespace)
                    .get(&o.name)
                    .await?;
                rs.metadata
                    .owner_references
                    .iter()
                    .flatten()
                    .find(|o| o.controller == Some(true) && o.kind == "Deployment")
                    .map(|d| workload_ref("apps", "deployments", &namespace, &d.name, context.clone()))
            }
            Some(o) if o.kind == "StatefulSet" => Some(workload_ref(
                "apps",
                "statefulsets",
                &namespace,
                &o.name,
                context.clone(),
            )),
            Some(o) if o.kind == "DaemonSet" => Some(workload_ref(
                "apps",
                "daemonsets",
                &namespace,
                &o.name,
                context.clone(),
            )),
            _ => None,
        };
        Ok(MemoryFix {
            workload,
            container: container.to_owned(),
            limit: resources
                .and_then(|r| r.limits.as_ref()?.get("memory"))
                .map(|q| q.0.clone()),
            request: resources
                .and_then(|r| r.requests.as_ref()?.get("memory"))
                .map(|q| q.0.clone()),
            suggested: suggested_memory(found),
        })
    }

    /// Sets one container's memory limit in a workload's pod template.
    pub async fn set_memory_limit(&self, target: &ObjectRef, container: &str, memory: &str) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        session.ensure_may("patch", target, "").await?;
        let patch = memory_patch(container, memory)?;
        session
            .api(&target.resource, target.namespace.as_deref())?
            .patch(
                &target.name,
                &kube::api::PatchParams::default(),
                &kube::api::Patch::Strategic(patch),
            )
            .await?;
        Ok(())
    }

    /// Why each node would refuse the pending pod `target`.
    pub async fn scheduling(&self, target: &ObjectRef) -> Result<Vec<NodeFit>> {
        let session = self.object_session(target).await?;
        let pod = session
            .pod_api(target.namespace.as_deref())?
            .get(&target.name)
            .await?;
        let nodes = Api::<Node>::all(session.client.clone())
            .list(&ListParams::default())
            .await?;
        // Without the right to list every pod, CPU and memory are counted from what is visible.
        let pods = Api::<Pod>::all(session.client.clone())
            .list(&ListParams::default().fields("spec.nodeName!="))
            .await
            .map(|l| l.items)
            .unwrap_or_default();
        Ok(node_fits(&pod, &nodes.items, &pods))
    }

    /// Which pods the Service `target` picks, and why it might send traffic to none.
    pub async fn service_check(&self, target: &ObjectRef) -> Result<ServiceCheck> {
        let session = self.object_session(target).await?;
        let namespace = target.namespace.clone().unwrap_or_default();
        let service = Api::<Service>::namespaced(session.client.clone(), &namespace)
            .get(&target.name)
            .await?;
        let pods = session
            .pod_api(Some(&namespace))?
            .list(&ListParams::default())
            .await?;
        Ok(service_check(&service, &pods.items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn suggests_about_twice_the_memory() {
        let c = |r: Value| -> Container { from(json!({ "name": "app", "resources": r })) };
        assert_eq!(
            suggested_memory(&c(json!({ "limits": { "memory": "128Mi" } }))),
            "256Mi"
        );
        assert_eq!(
            suggested_memory(&c(json!({ "limits": { "memory": "1Gi" } }))),
            "2048Mi"
        );
        assert_eq!(
            suggested_memory(&c(json!({ "limits": { "memory": "20Mi" } }))),
            "128Mi"
        );
        assert_eq!(
            suggested_memory(&c(json!({ "requests": { "memory": "100M" } }))),
            "192Mi"
        );
        assert_eq!(suggested_memory(&c(json!({}))), "512Mi");
    }

    #[test]
    fn memory_patches_merge_by_container() {
        let patch = memory_patch("app", " 256Mi ").unwrap();
        assert_eq!(
            patch.pointer("/spec/template/spec/containers/0"),
            Some(&json!({ "name": "app", "resources": { "limits": { "memory": "256Mi" } } }))
        );
        assert!(memory_patch("app", "lots").is_err());
        assert!(memory_patch("app", "0").is_err());
    }

    fn node(name: &str, extra: Value) -> Node {
        let mut node = json!({
            "metadata": { "name": name, "labels": { "kubernetes.io/hostname": name } },
            "spec": {},
            "status": {
                "allocatable": { "cpu": "2", "memory": "4Gi", "pods": "110" },
                "conditions": [{ "type": "Ready", "status": "True" }]
            }
        });
        json_patch(&mut node, extra);
        from(node)
    }

    fn json_patch(target: &mut Value, patch: Value) {
        if let (Some(t), Value::Object(p)) = (target.as_object_mut(), patch) {
            for (k, v) in p {
                match t.get_mut(&k) {
                    Some(existing) if existing.is_object() && v.is_object() => json_patch(existing, v),
                    _ => {
                        t.insert(k, v);
                    }
                }
            }
        }
    }

    #[test]
    fn says_why_each_node_refuses_a_pod() {
        let pod: Pod = from(json!({
            "metadata": { "name": "web", "uid": "me" },
            "spec": {
                "nodeSelector": { "disk": "ssd" },
                "containers": [{ "name": "app", "resources": { "requests": { "cpu": "1500m", "memory": "1Gi" } } }]
            }
        }));
        let nodes = [
            node("a", json!({ "metadata": { "labels": { "disk": "ssd" } } })),
            node(
                "b",
                json!({ "metadata": { "labels": { "disk": "hdd" } },
                "spec": { "taints": [{ "key": "gpu", "value": "yes", "effect": "NoSchedule" }] } }),
            ),
            node(
                "c",
                json!({ "metadata": { "labels": { "disk": "ssd" } }, "spec": { "unschedulable": true,
                "taints": [{ "key": "node.kubernetes.io/unschedulable", "effect": "NoSchedule" }] } }),
            ),
        ];
        let busy: Pod = from(json!({
            "metadata": { "name": "other", "uid": "o" },
            "spec": { "nodeName": "a", "containers": [{ "name": "x", "resources": { "requests": { "cpu": "1" } } }] },
            "status": { "phase": "Running" }
        }));
        let fits = node_fits(&pod, &nodes, &[busy]);
        assert_eq!(
            fits[0].reasons,
            ["not enough CPU: the pod requests 1.5, 1 is free"]
        );
        assert_eq!(
            fits[1].reasons,
            [
                "taint gpu=yes:NoSchedule is not tolerated",
                "node selector wants disk=ssd, the node has disk=hdd"
            ]
        );
        assert_eq!(fits[2].reasons, ["the node is cordoned"]);
    }

    #[test]
    fn tolerations_and_affinity() {
        let taint: Taint = from(json!({ "key": "gpu", "value": "yes", "effect": "NoSchedule" }));
        let t = |v: Value| -> Vec<Toleration> { from(v) };
        assert!(tolerates(&t(json!([{ "operator": "Exists" }])), &taint));
        assert!(tolerates(
            &t(json!([{ "key": "gpu", "operator": "Exists" }])),
            &taint
        ));
        assert!(tolerates(
            &t(json!([{ "key": "gpu", "value": "yes", "effect": "NoSchedule" }])),
            &taint
        ));
        assert!(!tolerates(&t(json!([{ "key": "gpu", "value": "no" }])), &taint));
        assert!(!tolerates(
            &t(json!([{ "key": "gpu", "operator": "Exists", "effect": "NoExecute" }])),
            &taint
        ));

        let pod: Pod = from(
            json!({ "spec": { "containers": [], "affinity": { "nodeAffinity": {
            "requiredDuringSchedulingIgnoredDuringExecution": { "nodeSelectorTerms": [
                { "matchExpressions": [{ "key": "zone", "operator": "In", "values": ["eu-1", "eu-2"] }] }
            ] } } } } }),
        );
        let fits = node_fits(
            &pod,
            &[
                node("x", json!({ "metadata": { "labels": { "zone": "us-1" } } })),
                node("y", json!({ "metadata": { "labels": { "zone": "eu-2" } } })),
            ],
            &[],
        );
        assert_eq!(
            fits[0].reasons,
            ["required node affinity is not met: zone in (eu-1, eu-2)"]
        );
        assert!(fits[1].reasons.is_empty());
    }

    #[test]
    fn says_why_a_service_has_no_endpoints() {
        let service: Service = from(json!({ "spec": {
            "selector": { "app": "web", "tier": "front" },
            "ports": [{ "name": "http", "port": 80, "targetPort": "http" }]
        } }));
        let pod = |name: &str, labels: Value, ready: &str, ports: Value| -> Pod {
            from(json!({
                "metadata": { "name": name, "labels": labels },
                "spec": { "containers": [{ "name": "app", "ports": ports }] },
                "status": { "conditions": [{ "type": "Ready", "status": ready }] }
            }))
        };
        let check = service_check(
            &service,
            &[
                pod(
                    "web-1",
                    json!({ "app": "web", "tier": "front" }),
                    "False",
                    json!([{ "name": "web", "containerPort": 80 }]),
                ),
                pod(
                    "web-2",
                    json!({ "app": "web-v2", "tier": "front" }),
                    "True",
                    json!([]),
                ),
                pod("db", json!({ "app": "db" }), "True", json!([])),
            ],
        );
        assert_eq!(
            check.pods,
            [PodMatch {
                name: "web-1".into(),
                ready: false,
                missing_ports: vec!["http → http".into()]
            }]
        );
        assert_eq!(
            check.near,
            [NearMiss {
                name: "web-2".into(),
                differences: vec!["app is web-v2, not web".into()]
            }]
        );
    }
}
