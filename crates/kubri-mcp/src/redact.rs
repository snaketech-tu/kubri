//! What agents do not see: the values of Secrets, unless the user allows them.

use serde_json::Value;

const LAST_APPLIED: &str = "kubectl.kubernetes.io/last-applied-configuration";

/// Replaces the values of a Secret with a note of their size, keeping the keys.
/// Other objects stay as they are.
pub(crate) fn secret(object: &mut Value) {
    if object.get("kind").and_then(Value::as_str) != Some("Secret") {
        return;
    }
    for field in ["data", "stringData"] {
        if let Some(Value::Object(values)) = object.get_mut(field) {
            for value in values.values_mut() {
                let size = value.as_str().map(str::len).unwrap_or_default();
                *value = Value::String(format!("<hidden, {size} characters>"));
            }
        }
    }
    // kubectl apply keeps a copy of the whole Secret here.
    if let Some(Value::Object(annotations)) = object.pointer_mut("/metadata/annotations")
        && let Some(value) = annotations.get_mut(LAST_APPLIED)
    {
        *value = Value::String("<hidden>".into());
    }
}

/// Removes what only makes the object longer for an agent: `managedFields`.
pub(crate) fn noise(object: &mut Value) {
    if let Some(Value::Object(metadata)) = object.get_mut("metadata") {
        metadata.remove("managedFields");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hides_secret_values_only() {
        let mut secret_object = json!({
            "kind": "Secret",
            "metadata": { "annotations": { LAST_APPLIED: "{\"data\":{\"password\":\"aHVudGVyMg==\"}}", "team": "shop" } },
            "data": { "password": "aHVudGVyMg==" },
            "stringData": { "token": "abc" }
        });
        secret(&mut secret_object);
        assert_eq!(secret_object["data"]["password"], "<hidden, 12 characters>");
        assert_eq!(secret_object["stringData"]["token"], "<hidden, 3 characters>");
        assert_eq!(secret_object["metadata"]["annotations"][LAST_APPLIED], "<hidden>");
        assert_eq!(secret_object["metadata"]["annotations"]["team"], "shop");

        let mut config_map = json!({ "kind": "ConfigMap", "data": { "a": "b" } });
        secret(&mut config_map);
        assert_eq!(config_map["data"]["a"], "b");
    }
}
