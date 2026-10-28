//! CPU and memory usage from metrics-server (`metrics.k8s.io`).

use std::collections::HashMap;

use k8s_openapi::api::core::v1::{Container, Node, Pod};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub namespace: Option<String>,
    pub name: String,
    /// CPU in millicores.
    pub cpu: f64,
    /// Memory in bytes.
    pub memory: f64,
    /// What a pod's containers request and may use at most, for %CPU/R, %CPU/L and the
    /// like. A limit is only known when every container sets one.
    #[serde(skip_serializing_if = "Limits::is_empty")]
    pub requests: Limits,
    #[serde(skip_serializing_if = "Limits::is_empty")]
    pub limits: Limits,
    /// What a node can give to pods, for its %CPU and %MEM.
    #[serde(skip_serializing_if = "Limits::is_empty")]
    pub allocatable: Limits,
}

/// CPU in millicores and memory in bytes, where they are set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<f64>,
}

impl Limits {
    fn is_empty(&self) -> bool {
        self.cpu.is_none() && self.memory.is_none()
    }
}

/// Which metrics to fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricsKind {
    Pods,
    Nodes,
}

/// Turns a `PodMetricsList` or `NodeMetricsList` into one usage per pod or node.
/// Pod usage is the sum of its containers.
pub(crate) fn parse_usage(list: &Value) -> Vec<Usage> {
    let Some(items) = list.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let usages: Vec<&Value> = match item.get("containers").and_then(Value::as_array) {
                Some(containers) => containers.iter().filter_map(|c| c.get("usage")).collect(),
                None => item.get("usage").into_iter().collect(),
            };
            let sum = |key: &str, unit: f64| -> f64 {
                usages
                    .iter()
                    .filter_map(|u| u.get(key).and_then(Value::as_str))
                    .filter_map(parse_quantity)
                    .sum::<f64>()
                    * unit
            };
            let text = |key: &str| item.pointer(key).and_then(Value::as_str).map(str::to_owned);
            Usage {
                namespace: text("/metadata/namespace"),
                name: text("/metadata/name").unwrap_or_default(),
                cpu: sum("cpu", 1000.0),
                memory: sum("memory", 1.0),
                ..Usage::default()
            }
        })
        .collect()
}

/// Adds the requests and limits of the pods to their usage.
pub(crate) fn add_pod_limits(usage: &mut [Usage], pods: &[Pod]) {
    let by_name: HashMap<(Option<&str>, &str), &Pod> = pods
        .iter()
        .filter_map(|p| Some(((p.metadata.namespace.as_deref(), p.metadata.name.as_deref()?), p)))
        .collect();
    for u in usage {
        let Some(spec) = by_name
            .get(&(u.namespace.as_deref(), u.name.as_str()))
            .and_then(|p| p.spec.as_ref())
        else {
            continue;
        };
        // Sidecars (init containers that keep running) count like the others.
        let running: Vec<&Container> = spec
            .containers
            .iter()
            .chain(
                spec.init_containers
                    .iter()
                    .flatten()
                    .filter(|c| c.restart_policy.as_deref() == Some("Always")),
            )
            .collect();
        u.requests = Limits {
            cpu: total(&running, "cpu", Need::Requests).map(|c| c * 1000.0),
            memory: total(&running, "memory", Need::Requests),
        };
        u.limits = Limits {
            cpu: total(&running, "cpu", Need::Limits).map(|c| c * 1000.0),
            memory: total(&running, "memory", Need::Limits),
        };
    }
}

/// Adds what each node can give to pods to its usage.
pub(crate) fn add_node_allocatable(usage: &mut [Usage], nodes: &[Node]) {
    for u in usage {
        let allocatable = nodes
            .iter()
            .find(|n| n.metadata.name.as_deref() == Some(u.name.as_str()))
            .and_then(|n| n.status.as_ref()?.allocatable.as_ref());
        let get = |key: &str| {
            allocatable
                .and_then(|a| a.get(key))
                .and_then(|q| parse_quantity(&q.0))
        };
        u.allocatable = Limits {
            cpu: get("cpu").map(|c| c * 1000.0),
            memory: get("memory"),
        };
    }
}

#[derive(Clone, Copy)]
enum Need {
    Requests,
    Limits,
}

/// The sum over the containers. Requests need one container that sets it; limits need all,
/// as one without a limit may use the whole node.
fn total(containers: &[&Container], key: &str, need: Need) -> Option<f64> {
    let values: Vec<Option<f64>> = containers
        .iter()
        .map(|c| {
            let resources = c.resources.as_ref()?;
            let map = match need {
                Need::Requests => resources.requests.as_ref(),
                Need::Limits => resources.limits.as_ref(),
            }?;
            parse_quantity(&map.get(key)?.0)
        })
        .collect();
    match need {
        Need::Requests if values.iter().all(Option::is_none) => None,
        Need::Requests => Some(values.into_iter().flatten().sum()),
        Need::Limits if values.is_empty() => None,
        Need::Limits => values.into_iter().sum(),
    }
}

/// Parses a Kubernetes quantity such as `250m`, `1.5`, `128Mi` or `2e3` into its base unit.
pub fn parse_quantity(text: &str) -> Option<f64> {
    let text = text.trim();
    let bytes = text.as_bytes();
    // An `e` followed by a digit or sign is an exponent ("2e3"); otherwise `E` is exa.
    let exponent = |i: usize| {
        matches!(bytes[i], b'e' | b'E')
            && bytes
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-'))
    };
    let split = (0..bytes.len())
        .find(|&i| bytes[i].is_ascii_alphabetic() && !exponent(i))
        .unwrap_or(text.len());
    let (number, suffix) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let factor = match suffix {
        "" => 1.0,
        "n" => 1e-9,
        "u" => 1e-6,
        "m" => 1e-3,
        "k" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        "T" => 1e12,
        "P" => 1e15,
        "E" => 1e18,
        "Ki" => 1024.0,
        "Mi" => 1024.0_f64.powi(2),
        "Gi" => 1024.0_f64.powi(3),
        "Ti" => 1024.0_f64.powi(4),
        "Pi" => 1024.0_f64.powi(5),
        "Ei" => 1024.0_f64.powi(6),
        _ => return None,
    };
    Some(number * factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_quantities() {
        assert_eq!(parse_quantity("250m"), Some(0.25));
        assert_eq!(parse_quantity("2"), Some(2.0));
        assert_eq!(parse_quantity("128Mi"), Some(134_217_728.0));
        assert_eq!(parse_quantity("1k"), Some(1000.0));
        assert_eq!(parse_quantity("2e3"), Some(2000.0));
        assert_eq!(parse_quantity("5E-1"), Some(0.5));
        assert_eq!(parse_quantity("1E"), Some(1e18));
        assert_eq!(parse_quantity("2Ei"), Some(2.0 * 1024.0_f64.powi(6)));
        assert!((parse_quantity("1500000n").unwrap() - 0.0015).abs() < 1e-12);
        assert_eq!(parse_quantity("12parsecs"), None);
    }

    #[test]
    fn sums_container_usage_per_pod() {
        let list = json!({ "kind": "PodMetricsList", "items": [{
            "metadata": { "name": "web", "namespace": "shop" },
            "containers": [
                { "name": "app", "usage": { "cpu": "100m", "memory": "64Mi" } },
                { "name": "sidecar", "usage": { "cpu": "5000000n", "memory": "16Mi" } }
            ]
        }]});
        let usage = parse_usage(&list);
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].namespace.as_deref(), Some("shop"));
        assert!((usage[0].cpu - 105.0).abs() < 1e-9);
        assert_eq!(usage[0].memory, 80.0 * 1024.0 * 1024.0);
    }

    #[test]
    fn adds_requests_and_limits_of_pods() {
        let pod: Pod = serde_json::from_value(json!({
            "metadata": { "name": "web", "namespace": "shop" },
            "spec": {
                "containers": [
                    { "name": "app", "resources": {
                        "requests": { "cpu": "100m", "memory": "64Mi" },
                        "limits": { "cpu": "500m", "memory": "128Mi" } } },
                    { "name": "proxy", "resources": { "requests": { "cpu": "50m" }, "limits": { "cpu": "1" } } }
                ],
                "initContainers": [
                    { "name": "setup", "resources": { "requests": { "cpu": "2" } } },
                    { "name": "log-shipper", "restartPolicy": "Always",
                      "resources": { "requests": { "cpu": "10m" }, "limits": { "cpu": "20m" } } }
                ]
            }
        }))
        .unwrap();
        let mut usage = vec![
            Usage {
                namespace: Some("shop".into()),
                name: "web".into(),
                ..Usage::default()
            },
            Usage {
                namespace: Some("shop".into()),
                name: "gone".into(),
                ..Usage::default()
            },
        ];
        add_pod_limits(&mut usage, &[pod]);
        assert!((usage[0].requests.cpu.unwrap() - 160.0).abs() < 1e-9);
        assert_eq!(usage[0].requests.memory, Some(64.0 * 1024.0 * 1024.0));
        assert!((usage[0].limits.cpu.unwrap() - 1520.0).abs() < 1e-9);
        // The proxy and the sidecar set no memory limit, so the pod has none.
        assert_eq!(usage[0].limits.memory, None);
        assert_eq!(usage[1].requests, Limits::default());
    }

    #[test]
    fn adds_node_allocatable() {
        let node: Node = serde_json::from_value(json!({
            "metadata": { "name": "n1" },
            "status": { "allocatable": { "cpu": "3500m", "memory": "8Gi" } }
        }))
        .unwrap();
        let mut usage = vec![Usage {
            name: "n1".into(),
            ..Usage::default()
        }];
        add_node_allocatable(&mut usage, &[node]);
        assert_eq!(usage[0].allocatable.cpu, Some(3500.0));
        assert_eq!(usage[0].allocatable.memory, Some(8.0 * 1024.0_f64.powi(3)));
        // Unset limits stay out of the JSON.
        let json = serde_json::to_value(&usage[0]).unwrap();
        assert!(json.get("requests").is_none());
    }

    #[test]
    fn reads_node_usage() {
        let list = json!({ "items": [{ "metadata": { "name": "n1" },
                                       "usage": { "cpu": "1", "memory": "1Gi" } }] });
        let usage = parse_usage(&list);
        assert_eq!(usage[0].cpu, 1000.0);
        assert_eq!(usage[0].namespace, None);
    }
}
