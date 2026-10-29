//! The OpenAPI v3 schema of a kind, as the API server publishes it, for field docs, completion
//! and validation beside the YAML editor (what `kubectl explain` shows).

use std::collections::{BTreeMap, HashSet};

use kube::Client;
use serde_json::{Map, Value};

use crate::{Error, Result};

const REF_PREFIX: &str = "#/components/schemas/";

/// The path of a group-version's document in `/openapi/v3`, as the index names it.
pub(crate) fn group_version_path(group: &str, version: &str) -> String {
    if group.is_empty() {
        format!("api/{version}")
    } else {
        format!("apis/{group}/{version}")
    }
}

async fn get_json(client: &Client, url: &str) -> Result<Value> {
    let request = http::Request::get(url)
        .body(Vec::new())
        .map_err(|e| Error::Unsupported(e.to_string()))?;
    let text = client.request_text(request).await?;
    Ok(serde_json::from_str(&text)?)
}

/// The OpenAPI v3 document of one group-version.
pub(crate) async fn group_version_document(client: &Client, group: &str, version: &str) -> Result<Value> {
    let index = get_json(client, "/openapi/v3").await?;
    let path = group_version_path(group, version);
    let url = index
        .pointer(&format!("/paths/{}", path.replace('~', "~0").replace('/', "~1")))
        .and_then(|p| p.get("serverRelativeURL"))
        .and_then(Value::as_str)
        .ok_or_else(|| Error::NotFound(format!("an OpenAPI schema for {path}")))?
        .to_owned();
    get_json(client, &url).await
}

/// The schema of `kind` from a group-version document: the name of its root and every schema
/// it refers to, by name. `None` when the document has no such kind.
pub(crate) fn kind_schema(document: &Value, group: &str, version: &str, kind: &str) -> Option<Value> {
    let schemas = document.pointer("/components/schemas")?.as_object()?;
    let root = schemas.iter().find_map(|(name, schema)| {
        let gvks = schema.get("x-kubernetes-group-version-kind")?.as_array()?;
        gvks.iter()
            .any(|gvk| {
                gvk.get("group").and_then(Value::as_str).unwrap_or("") == group
                    && gvk.get("version").and_then(Value::as_str) == Some(version)
                    && gvk.get("kind").and_then(Value::as_str) == Some(kind)
            })
            .then(|| name.clone())
    })?;
    let mut found = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut todo = vec![root.clone()];
    while let Some(name) = todo.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(schema) = schemas.get(&name) else {
            continue;
        };
        collect_refs(schema, &mut todo);
        found.insert(name, trim(schema));
    }
    let mut out = Map::new();
    out.insert("root".into(), Value::String(root));
    out.insert("schemas".into(), Value::Object(found.into_iter().collect()));
    Some(Value::Object(out))
}

/// Every schema name `$ref`s in `value` point to.
fn collect_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(name) = map
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix(REF_PREFIX))
            {
                out.push(name.to_owned());
            }
            map.values().for_each(|v| collect_refs(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| collect_refs(v, out)),
        _ => {}
    }
}

/// Keeps what the editor uses: types, descriptions, fields, items, enums and refs.
fn trim(value: &Value) -> Value {
    const KEEP: &[&str] = &[
        "$ref",
        "allOf",
        "type",
        "format",
        "description",
        "properties",
        "items",
        "additionalProperties",
        "required",
        "enum",
        "default",
        "x-kubernetes-preserve-unknown-fields",
        "x-kubernetes-int-or-string",
    ];
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| KEEP.contains(&k.as_str()))
                .map(|(k, v)| {
                    let v = match k.as_str() {
                        // Field names, not schema keywords.
                        "properties" => Value::Object(
                            v.as_object()
                                .map(|p| p.iter().map(|(name, s)| (name.clone(), trim(s))).collect())
                                .unwrap_or_default(),
                        ),
                        "items" | "additionalProperties" => trim(v),
                        "allOf" => Value::Array(
                            v.as_array()
                                .map(|a| a.iter().map(trim).collect())
                                .unwrap_or_default(),
                        ),
                        _ => v.clone(),
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Value {
        serde_json::json!({ "components": { "schemas": {
            "io.k8s.api.apps.v1.Deployment": {
                "description": "Deployment enables declarative updates",
                "type": "object",
                "properties": {
                    "spec": { "allOf": [{ "$ref": "#/components/schemas/io.k8s.api.apps.v1.DeploymentSpec" }], "default": {} },
                    "kind": { "type": "string" }
                },
                "x-kubernetes-group-version-kind": [{ "group": "apps", "kind": "Deployment", "version": "v1" }]
            },
            "io.k8s.api.apps.v1.DeploymentSpec": {
                "type": "object",
                "properties": { "replicas": { "type": "integer", "format": "int32", "x-extra": 1 } },
                "required": ["selector"]
            },
            "io.k8s.api.apps.v1.Unrelated": { "type": "object" }
        } } })
    }

    #[test]
    fn finds_the_kind_and_what_it_refers_to() {
        let schema = kind_schema(&document(), "apps", "v1", "Deployment").unwrap();
        assert_eq!(schema["root"], "io.k8s.api.apps.v1.Deployment");
        let schemas = schema["schemas"].as_object().unwrap();
        assert_eq!(schemas.len(), 2);
        let spec = &schemas["io.k8s.api.apps.v1.DeploymentSpec"];
        assert_eq!(
            spec["properties"]["replicas"],
            serde_json::json!({ "type": "integer", "format": "int32" })
        );
        assert!(
            schemas["io.k8s.api.apps.v1.Deployment"]
                .get("x-kubernetes-group-version-kind")
                .is_none()
        );
        assert!(kind_schema(&document(), "apps", "v1", "StatefulSet").is_none());
    }

    #[test]
    fn names_group_version_paths() {
        assert_eq!(group_version_path("", "v1"), "api/v1");
        assert_eq!(group_version_path("apps", "v1"), "apis/apps/v1");
    }
}
