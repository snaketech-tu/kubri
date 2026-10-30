//! The part of kubectl's JSONPath that custom columns use: `.spec.nodeName`,
//! `.spec.containers[*].image`, `.status.conditions[0].type`, `.metadata.labels['app.kubernetes.io/name']`
//! and `.metadata.labels.app\.kubernetes\.io/name`, with or without the braces and `$`, and
//! filters such as `.status.conditions[?(@.type=="Ready")].status`.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    Key(String),
    /// From the end when negative, as in `[-1]`.
    Index(i64),
    /// Every element of a list, or every value of a map.
    All,
    /// The elements of a list whose value at `path` is, or with `equal` false is not, `value`.
    Filter {
        path: Vec<String>,
        value: String,
        equal: bool,
    },
}

/// A parsed path, ready to read from many objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JsonPath(Vec<Step>);

impl JsonPath {
    pub(crate) fn parse(path: &str) -> Result<Self, String> {
        let bad = |why: &str| format!("`{path}` is not a JSONPath Kubri reads: {why}");
        let mut text = path.trim();
        if let Some(inner) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
            text = inner.trim();
        }
        text = text.strip_prefix('$').unwrap_or(text);
        if text.is_empty() {
            return Err(bad("it is empty"));
        }
        let chars: Vec<char> = text.chars().collect();
        let mut steps = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '[' => {
                    let end = chars[i..]
                        .iter()
                        .position(|&c| c == ']')
                        .map(|p| i + p)
                        .ok_or_else(|| bad("a `[` is not closed"))?;
                    let inner: String = chars[i + 1..end].iter().collect();
                    let inner = inner.trim();
                    let quoted = inner
                        .strip_prefix('\'')
                        .and_then(|s| s.strip_suffix('\''))
                        .or_else(|| inner.strip_prefix('"').and_then(|s| s.strip_suffix('"')));
                    steps.push(
                        if let Some(filter) = inner.strip_prefix("?(").and_then(|f| f.strip_suffix(')')) {
                            parse_filter(filter).ok_or_else(|| {
                                bad(&format!(
                                    "`[{inner}]` is not a filter like [?(@.type==\"Ready\")]"
                                ))
                            })?
                        } else if inner == "*" {
                            Step::All
                        } else if let Some(key) = quoted {
                            Step::Key(key.to_owned())
                        } else {
                            Step::Index(inner.parse().map_err(|_| {
                                bad(&format!("`[{inner}]` is neither an index, `*` nor a quoted key"))
                            })?)
                        },
                    );
                    i = end + 1;
                }
                c => {
                    if c == '.' {
                        i += 1;
                        if chars.get(i) == Some(&'.') {
                            return Err(bad("`..` is not supported"));
                        }
                        if matches!(chars.get(i), None | Some('[')) {
                            continue;
                        }
                    }
                    let mut key = String::new();
                    while let Some(&c) = chars.get(i) {
                        match c {
                            '\\' if i + 1 < chars.len() => {
                                key.push(chars[i + 1]);
                                i += 2;
                            }
                            '.' | '[' => break,
                            '*' if key.is_empty() => {
                                i += 1;
                                break;
                            }
                            '?' | '(' | '@' | '{' | '}' | ' ' => {
                                return Err(bad("filters, functions and several paths are not supported"));
                            }
                            _ => {
                                key.push(c);
                                i += 1;
                            }
                        }
                    }
                    steps.push(if key.is_empty() { Step::All } else { Step::Key(key) });
                }
            }
        }
        Ok(Self(steps))
    }

    /// Every value the path reaches in `value`.
    pub(crate) fn find<'a>(&self, value: &'a Value) -> Vec<&'a Value> {
        let mut found = vec![value];
        for step in &self.0 {
            found = found
                .into_iter()
                .flat_map(|v| -> Vec<&'a Value> {
                    match (step, v) {
                        (Step::Key(k), Value::Object(map)) => map.get(k).into_iter().collect(),
                        (Step::Index(n), Value::Array(list)) => {
                            let at = if *n < 0 { list.len() as i64 + n } else { *n };
                            usize::try_from(at)
                                .ok()
                                .and_then(|at| list.get(at))
                                .into_iter()
                                .collect()
                        }
                        (Step::All, Value::Array(list)) => list.iter().collect(),
                        (Step::All, Value::Object(map)) => map.values().collect(),
                        (Step::Filter { path, value, equal }, Value::Array(list)) => list
                            .iter()
                            .filter(|item| {
                                let found = path.iter().try_fold(*item, |v, key| v.get(key));
                                (found.map(text).as_deref() == Some(value.as_str())) == *equal
                            })
                            .collect(),
                        _ => Vec::new(),
                    }
                })
                .collect();
        }
        found
    }

    /// The values the path reaches as one cell, like kubectl's custom columns: joined with
    /// commas, or `None` when there are none.
    pub(crate) fn cell(&self, value: &Value) -> Value {
        let texts: Vec<String> = self
            .find(value)
            .into_iter()
            .filter(|v| !v.is_null())
            .map(text)
            .collect();
        if texts.is_empty() {
            Value::Null
        } else {
            Value::String(texts.join(","))
        }
    }
}

/// `@.type=="Ready"` or `@.status.phase != 'Running'`.
fn parse_filter(filter: &str) -> Option<Step> {
    let (left, right, equal) = match filter.split_once("!=") {
        Some((l, r)) => (l, r, false),
        None => {
            let (l, r) = filter.split_once("==")?;
            (l, r, true)
        }
    };
    let path: Vec<String> = left
        .trim()
        .strip_prefix("@.")?
        .split('.')
        .map(str::to_owned)
        .collect();
    if path.iter().any(String::is_empty) {
        return None;
    }
    let right = right.trim();
    let value = right
        .strip_prefix('\'')
        .and_then(|v| v.strip_suffix('\''))
        .or_else(|| right.strip_prefix('"').and_then(|v| v.strip_suffix('"')))
        .unwrap_or(right);
    Some(Step::Filter {
        path,
        value: value.to_owned(),
        equal,
    })
}

/// A value as a cell shows it: strings as they are, anything else as JSON.
pub(crate) fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pod() -> Value {
        json!({
            "metadata": { "name": "web-1", "labels": { "app.kubernetes.io/name": "web", "tier": "front" } },
            "spec": { "nodeName": "n1", "containers": [
                { "name": "app", "image": "web:2", "ports": [{ "containerPort": 80 }] },
                { "name": "proxy", "image": "envoy:1" }
            ] },
            "status": { "conditions": [{ "type": "Ready", "status": "True" }] }
        })
    }

    fn cell(path: &str) -> Value {
        JsonPath::parse(path).unwrap().cell(&pod())
    }

    #[test]
    fn reads_what_custom_columns_use() {
        assert_eq!(cell(".spec.nodeName"), json!("n1"));
        assert_eq!(cell("{.spec.nodeName}"), json!("n1"));
        assert_eq!(cell("$.spec.nodeName"), json!("n1"));
        assert_eq!(cell("spec.nodeName"), json!("n1"));
        assert_eq!(cell(".spec.containers[*].image"), json!("web:2,envoy:1"));
        assert_eq!(cell(".spec.containers[-1].name"), json!("proxy"));
        assert_eq!(cell(".spec.containers[0].ports[0].containerPort"), json!("80"));
        assert_eq!(
            cell(".spec.containers[0].ports"),
            json!(r#"[{"containerPort":80}]"#)
        );
        assert_eq!(cell(".metadata.labels['app.kubernetes.io/name']"), json!("web"));
        assert_eq!(cell(r".metadata.labels.app\.kubernetes\.io/name"), json!("web"));
        assert_eq!(cell(".metadata.labels.*"), json!("web,front"));
        assert_eq!(cell(".status.conditions[0].status"), json!("True"));
        assert_eq!(cell(".spec.missing"), Value::Null);
        assert_eq!(cell(".spec.containers[5].name"), Value::Null);
        assert_eq!(
            cell(r#".status.conditions[?(@.type=="Ready")].status"#),
            json!("True")
        );
        assert_eq!(
            cell(".spec.containers[?(@.name=='proxy')].image"),
            json!("envoy:1")
        );
        assert_eq!(cell(".spec.containers[?(@.name != 'proxy')].name"), json!("app"));
        assert_eq!(
            cell(r#"{.status.conditions[?(@.type=="Synced")].status}"#),
            Value::Null
        );
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        for path in [
            "",
            "{}",
            "..name",
            ".spec.containers[?(@.name)]",
            ".spec.containers[?(name=='app')]",
            ".spec.containers[?(@.ports[0].containerPort==80)]",
            ".a[",
            ".a[x]",
            "{.a} {.b}",
        ] {
            assert!(JsonPath::parse(path).is_err(), "{path}");
        }
    }
}
