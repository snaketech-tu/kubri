//! CPU and memory usage from metrics-server (`metrics.k8s.io`).

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub namespace: Option<String>,
    pub name: String,
    /// CPU in millicores.
    pub cpu: f64,
    /// Memory in bytes.
    pub memory: f64,
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
            }
        })
        .collect()
}

/// Parses a Kubernetes quantity such as `250m`, `1.5`, `128Mi` or `2e3` into its base unit.
pub fn parse_quantity(text: &str) -> Option<f64> {
    let text = text.trim();
    let split = text
        .find(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E')
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
    fn reads_node_usage() {
        let list = json!({ "items": [{ "metadata": { "name": "n1" },
                                       "usage": { "cpu": "1", "memory": "1Gi" } }] });
        let usage = parse_usage(&list);
        assert_eq!(usage[0].cpu, 1000.0);
        assert_eq!(usage[0].namespace, None);
    }
}
