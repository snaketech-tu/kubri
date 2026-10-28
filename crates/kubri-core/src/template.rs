//! Fills in the Go templates of `jumps.yaml`, such as `app={{.metadata.labels.app}}`, from
//! the object a jump starts at. Covers what selectors and namespaces need: field paths like
//! `.metadata.labels.app`, and `index` for keys with dots or slashes.

use serde_json::Value;

/// Renders `template` with the fields of `object`. A field the object does not have is an
/// error, where Go would write `<no value>` into the selector.
pub(crate) fn render(template: &str, object: &Value) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| format!("`{template}` opens `{{{{` without closing it"))?;
        let mut action = &after[..end];
        let mut text = &rest[..start];
        // `{{- ` and ` -}}` trim the white space next to them, as in Go.
        if let Some(trimmed) = action.strip_prefix("- ") {
            text = text.trim_end();
            action = trimmed;
        }
        let trim_after = action.ends_with(" -");
        if trim_after {
            action = &action[..action.len() - 2];
        }
        out.push_str(text);
        out.push_str(&evaluate(action.trim(), object)?);
        rest = &after[end + 2..];
        if trim_after {
            rest = rest.trim_start();
        }
    }
    out.push_str(rest);
    Ok(out)
}

fn evaluate(action: &str, object: &Value) -> Result<String, String> {
    if action.starts_with("/*") && action.ends_with("*/") {
        return Ok(String::new());
    }
    let words = words(action)?;
    let value = match words.as_slice() {
        [path] if path.starts_with('.') => field(object, path)?,
        [index, path, keys @ ..] if index == "index" && path.starts_with('.') && !keys.is_empty() => {
            let mut value = field(object, path)?;
            for key in keys {
                value = lookup(value, key).ok_or_else(|| format!("{path} has no {key}"))?;
            }
            value
        }
        _ => {
            return Err(format!(
                "`{{{{{action}}}}}` is not supported: Kubri fills in fields like {{{{.metadata.name}}}} \
                 and {{{{index .metadata.labels \"app.kubernetes.io/name\"}}}}"
            ));
        }
    };
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err(format!("`{{{{{action}}}}}` is not a single value")),
    }
}

/// A field path such as `.metadata.labels.app`; `.` is the whole object.
fn field<'a>(object: &'a Value, path: &str) -> Result<&'a Value, String> {
    let mut value = object;
    for name in path.split('.').skip(1).filter(|n| !n.is_empty()) {
        value = value
            .as_object()
            .and_then(|o| o.get(name))
            .filter(|v| !v.is_null())
            .ok_or_else(|| format!("the object has no {path}"))?;
    }
    Ok(value)
}

/// A key of `index`: a quoted map key, or a number for a list.
fn lookup<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map.get(key),
        Value::Array(items) => items.get(key.parse::<usize>().ok()?),
        _ => None,
    }
    .filter(|v| !v.is_null())
}

/// Splits an action into words, unquoting `"strings"` and `` `raw strings` ``.
fn words(action: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut chars = action.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut word = String::new();
        match c {
            '"' => {
                chars.next();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => word.extend(chars.next()),
                        Some(c) => word.push(c),
                        None => return Err(format!("`{action}` has an unclosed quote")),
                    }
                }
            }
            '`' => {
                chars.next();
                loop {
                    match chars.next() {
                        Some('`') => break,
                        Some(c) => word.push(c),
                        None => return Err(format!("`{action}` has an unclosed quote")),
                    }
                }
            }
            _ => {
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
            }
        }
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deployment() -> Value {
        json!({
            "metadata": {
                "name": "web",
                "namespace": "shop",
                "labels": { "app": "web", "app.kubernetes.io/part-of": "store" }
            },
            "spec": { "replicas": 3, "paused": false, "ports": [{ "port": 80 }] }
        })
    }

    #[test]
    fn fills_in_fields() {
        let object = deployment();
        assert_eq!(
            render("app={{.metadata.labels.app}}", &object).unwrap(),
            "app=web"
        );
        assert_eq!(
            render("app={{ .metadata.labels.app }},tier=front", &object).unwrap(),
            "app=web,tier=front"
        );
        assert_eq!(render("{{.metadata.namespace}}", &object).unwrap(), "shop");
        assert_eq!(
            render("{{.spec.replicas}} {{.spec.paused}}", &object).unwrap(),
            "3 false"
        );
        assert_eq!(render("no template", &object).unwrap(), "no template");
        assert_eq!(render("a {{- .metadata.name -}} b", &object).unwrap(), "awebb");
        assert_eq!(render("{{/* note */}}x", &object).unwrap(), "x");
    }

    #[test]
    fn index_reaches_keys_with_dots() {
        let object = deployment();
        assert_eq!(
            render(
                r#"part-of={{index .metadata.labels "app.kubernetes.io/part-of"}}"#,
                &object
            )
            .unwrap(),
            "part-of=store"
        );
        assert_eq!(render("{{index .spec.ports 0 `port`}}", &object).unwrap(), "80");
    }

    #[test]
    fn says_what_is_missing_or_unsupported() {
        let object = deployment();
        let err = render("app={{.metadata.labels.tier}}", &object).unwrap_err();
        assert!(err.contains(".metadata.labels.tier"), "{err}");
        assert!(
            render("{{.metadata.labels}}", &object)
                .unwrap_err()
                .contains("single value")
        );
        assert!(
            render("{{ printf \"%s\" .metadata.name }}", &object)
                .unwrap_err()
                .contains("not supported")
        );
        assert!(
            render("app={{.metadata.name", &object)
                .unwrap_err()
                .contains("without closing")
        );
        assert!(render(r#"{{index .metadata.labels "nope"}}"#, &object).is_err());
    }
}
