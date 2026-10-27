//! Finds the pods behind a workload or Service, for logs and port-forwards.

use serde_json::Value;

/// Turns a selector into the query syntax of `labelSelector`.
///
/// Accepts both shapes Kubernetes uses: a `LabelSelector` with `matchLabels` and
/// `matchExpressions` (Deployments, Jobs…) and a plain label map (Services,
/// ReplicationControllers). Returns `None` for an empty selector, which would match every pod.
pub(crate) fn label_selector(selector: &Value) -> Option<String> {
    let map = selector.as_object()?;
    let mut terms = Vec::new();
    if map.contains_key("matchLabels") || map.contains_key("matchExpressions") {
        if let Some(labels) = map.get("matchLabels").and_then(Value::as_object) {
            terms.extend(
                labels
                    .iter()
                    .filter_map(|(k, v)| Some(format!("{k}={}", v.as_str()?))),
            );
        }
        let expressions = map.get("matchExpressions").and_then(Value::as_array);
        terms.extend(expressions.into_iter().flatten().filter_map(expression));
    } else {
        terms.extend(
            map.iter()
                .filter_map(|(k, v)| Some(format!("{k}={}", v.as_str()?))),
        );
    }
    (!terms.is_empty()).then(|| terms.join(","))
}

fn expression(expr: &Value) -> Option<String> {
    let key = expr.get("key")?.as_str()?;
    let values = || {
        let values: Vec<&str> = expr
            .get("values")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        values.join(",")
    };
    Some(match expr.get("operator")?.as_str()? {
        "In" => format!("{key} in ({})", values()),
        "NotIn" => format!("{key} notin ({})", values()),
        "Exists" => key.to_owned(),
        "DoesNotExist" => format!("!{key}"),
        _ => return None,
    })
}

/// The selector of the pods an object manages or routes to, read from `spec.selector`.
/// This covers the built-in workloads, Services and CRDs that follow the same convention.
pub(crate) fn pod_selector(object: &Value) -> Option<String> {
    label_selector(object.pointer("/spec/selector")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_match_labels_and_expressions() {
        let selector = json!({
            "matchLabels": { "app": "web" },
            "matchExpressions": [
                { "key": "tier", "operator": "In", "values": ["front", "edge"] },
                { "key": "canary", "operator": "DoesNotExist" },
                { "key": "zone", "operator": "Exists" },
                { "key": "env", "operator": "NotIn", "values": ["dev"] }
            ]
        });
        assert_eq!(
            label_selector(&selector).unwrap(),
            "app=web,tier in (front,edge),!canary,zone,env notin (dev)"
        );
    }

    #[test]
    fn reads_plain_label_maps() {
        let service = json!({ "spec": { "selector": { "app": "web", "tier": "front" } } });
        assert_eq!(pod_selector(&service).unwrap(), "app=web,tier=front");
    }

    #[test]
    fn empty_selectors_match_nothing() {
        assert_eq!(label_selector(&json!({})), None);
        assert_eq!(label_selector(&json!({ "matchLabels": {} })), None);
        assert_eq!(pod_selector(&json!({ "spec": {} })), None);
    }
}
