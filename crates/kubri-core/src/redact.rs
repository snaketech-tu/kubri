//! What leaves Kubri without its secrets: the values of Secrets for AI agents, and for Explain
//! also environment variables with secret names and anything in text that looks like a token,
//! a password or a private key.

use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde_json::Value;

const LAST_APPLIED: &str = "kubectl.kubernetes.io/last-applied-configuration";

/// Replaces the values of a Secret with a note of their size, keeping the keys.
/// Other objects stay as they are.
pub fn secret(object: &mut Value) {
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
pub fn noise(object: &mut Value) {
    if let Some(Value::Object(metadata)) = object.get_mut("metadata") {
        metadata.remove("managedFields");
    }
}

/// Words in a name that mark its value as secret.
fn secret_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace(['-', '_', '.'], "");
    [
        "password",
        "passwd",
        "pwd",
        "secret",
        "token",
        "apikey",
        "accesskey",
        "privatekey",
        "credential",
        "auth",
        "dsn",
        "connectionstring",
    ]
    .iter()
    .any(|word| name.contains(word))
}

/// Everything [`secret`] hides, plus the values of environment variables with secret names
/// (anywhere in the object, such as a Deployment's pod template) and the copy kubectl apply
/// keeps of every object.
pub fn for_explain(object: &mut Value) {
    noise(object);
    secret(object);
    if let Some(Value::Object(annotations)) = object.pointer_mut("/metadata/annotations") {
        annotations.remove(LAST_APPLIED);
    }
    hide_env(object);
}

fn hide_env(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(env)) = map.get_mut("env") {
                for var in env.iter_mut() {
                    let named = var.get("name").and_then(Value::as_str).is_some_and(secret_name);
                    if named && let Some(value) = var.get_mut("value") {
                        *value = Value::String("<hidden>".into());
                    }
                }
            }
            map.values_mut().for_each(hide_env);
        }
        Value::Array(items) => items.iter_mut().for_each(hide_env),
        _ => {}
    }
}

/// Private keys and certificates in PEM form.
static PEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)-----BEGIN ([A-Z0-9 ]*PRIVATE KEY)-----.*?(-----END [A-Z0-9 ]*PRIVATE KEY-----|\z)")
        .unwrap()
});
/// JSON web tokens, such as service account tokens.
static JWT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]*").unwrap());
/// Credentials after an HTTP authorization scheme.
static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(bearer|basic|token)(\s+)[A-Za-z0-9._~+/=-]{8,}").unwrap());
/// Keys with a known shape: API keys of model providers, GitHub, Slack, AWS, Google.
static KNOWN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[abposr]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35})",
    )
    .unwrap()
});
/// Passwords in URLs: `scheme://user:password@host`.
static URL_PASSWORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-zA-Z][a-zA-Z0-9+.-]*://[^\s:/@]+:)[^\s@/]+@").unwrap());
/// `name=value`, `name: value` and `"name": "value"` where the name sounds secret.
static ASSIGNED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(["']?[a-z0-9_.-]*(?:password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key|credentials?|auth)[a-z0-9_.-]*["']?\s*[:=]\s*)("[^"]*"|'[^']*'|[^\s,;&"'}]+)"#,
    )
    .unwrap()
});

/// Hides what looks like a secret in free text, such as log lines or YAML.
pub fn text(input: &str) -> String {
    let out = PEM.replace_all(input, "<hidden $1>");
    let out = JWT.replace_all(&out, "<hidden token>");
    let out = SCHEME.replace_all(&out, "$1$2<hidden>");
    let out = KNOWN.replace_all(&out, "<hidden key>");
    let out = URL_PASSWORD.replace_all(&out, "$1<hidden>@");
    let out = ASSIGNED.replace_all(&out, |c: &Captures| {
        let value = &c[2];
        // Already hidden, an authorization scheme whose credentials [`SCHEME`] hid, or a
        // reference such as `secretKeyRef:` with nothing after it.
        let scheme = ["bearer", "basic", "token"].contains(&value.to_ascii_lowercase().as_str());
        if scheme || value.starts_with("<hidden") || value.trim_matches(['"', '\'']).is_empty() {
            return c[0].to_owned();
        }
        let quote = if value.starts_with('"') {
            "\""
        } else if value.starts_with('\'') {
            "'"
        } else {
            ""
        };
        format!("{}{quote}<hidden>{quote}", &c[1])
    });
    out.into_owned()
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

    #[test]
    fn hides_secret_environment_variables() {
        let mut deployment = json!({
            "kind": "Deployment",
            "metadata": { "annotations": { LAST_APPLIED: "{}" }, "managedFields": [{}] },
            "spec": { "template": { "spec": { "containers": [{ "name": "app", "env": [
                { "name": "DB_PASSWORD", "value": "hunter2" },
                { "name": "LOG_LEVEL", "value": "debug" },
                { "name": "API_TOKEN", "valueFrom": { "secretKeyRef": { "name": "api", "key": "token" } } }
            ] }] } } }
        });
        for_explain(&mut deployment);
        let env = &deployment["spec"]["template"]["spec"]["containers"][0]["env"];
        assert_eq!(env[0]["value"], "<hidden>");
        assert_eq!(env[1]["value"], "debug");
        assert_eq!(env[2]["valueFrom"]["secretKeyRef"]["key"], "token");
        assert!(deployment["metadata"].get("managedFields").is_none());
        assert!(deployment["metadata"]["annotations"].get(LAST_APPLIED).is_none());
    }

    #[test]
    fn hides_secrets_in_text() {
        let cases = [
            (
                "connecting with password=hunter2 to db",
                "connecting with password=<hidden> to db",
            ),
            (
                r#"{"user":"ann","api_key":"abc123"}"#,
                r#"{"user":"ann","api_key":"<hidden>"}"#,
            ),
            ("DB_PASSWORD: 's3cret'", "DB_PASSWORD: '<hidden>'"),
            (
                "Authorization: Bearer abcdefghijklmnop",
                "Authorization: Bearer <hidden>",
            ),
            (
                "postgres://app:hunter2@db:5432/shop",
                "postgres://app:<hidden>@db:5432/shop",
            ),
            (
                "key sk-ant-api03-abcdefghijklmnopqrstuv used",
                "key <hidden key> used",
            ),
            (
                "token eyJhbGciOiJSUzI1NiIs.eyJpc3MiOiJrdWJlcm5ldGVz.c2lnbmF0dXJl",
                "token <hidden token>",
            ),
            (
                "got eyJhbGciOiJSUzI1NiIs.eyJpc3MiOiJrdWJlcm5ldGVz.c2lnbmF0dXJl back",
                "got <hidden token> back",
            ),
            (
                "-----BEGIN RSA PRIVATE KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----",
                "<hidden RSA PRIVATE KEY>",
            ),
            ("secretKeyRef:", "secretKeyRef:"),
            (
                "Back-off restarting failed container app",
                "Back-off restarting failed container app",
            ),
            ("tokens: 3 of 5 used", "tokens: <hidden> of 5 used"),
        ];
        for (input, expected) in cases {
            assert_eq!(text(input), expected, "{input}");
        }
        // Hiding twice changes nothing more.
        let once = text("password=hunter2 Bearer abcdefghijklmnop");
        assert_eq!(text(&once), once);
    }
}
