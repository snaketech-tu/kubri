//! Log history from Loki, beyond the lifetime of a pod: the same lines the log workspace shows
//! from the cluster, found with a LogQL query prefilled from the object.

use std::collections::BTreeMap;

use k8s_openapi::jiff::Timestamp;
use kube::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::integrations::Endpoint;
use crate::{Error, LogLine, LogSource, Result};

/// The most lines one query returns.
pub const MAX_LINES: u32 = 5000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LokiRequest {
    pub target: crate::ObjectRef,
    /// LogQL; the object's own query when left out.
    #[serde(default)]
    pub query: Option<String>,
    /// Only this container, added to the object's own query.
    #[serde(default)]
    pub container: Option<String>,
    /// How far back from now.
    pub seconds: u64,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LokiLogs {
    /// The query that ran.
    pub query: String,
    /// Where Loki is.
    pub source: String,
    /// The containers the lines came from, as the live log names them.
    pub sources: Vec<LogSource>,
    /// Oldest first.
    pub lines: Vec<LogLine>,
    /// More lines matched than were returned: the newest are shown.
    pub limited: bool,
}

/// A LogQL string, quoted.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Loki label names: what promtail and Alloy make of Kubernetes label keys.
fn label_name(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The stream selector for an object's pods: by name for pods and workloads, by the Service's
/// selector for a Service.
pub(crate) fn default_query(
    kind: &str,
    namespace: &str,
    name: &str,
    selector: Option<&BTreeMap<String, String>>,
    container: Option<&str>,
) -> String {
    let mut matchers = vec![format!("namespace={}", quoted(namespace))];
    match (kind, selector) {
        ("Pod", _) => matchers.push(format!("pod={}", quoted(name))),
        ("Service", Some(selector)) => matchers.extend(
            selector
                .iter()
                .map(|(k, v)| format!("{}={}", label_name(k), quoted(v))),
        ),
        _ => matchers.push(format!("pod=~\"{}\"", crate::prometheus::pod_pattern(kind, name))),
    }
    if let Some(container) = container.filter(|c| !c.is_empty()) {
        matchers.push(format!("container={}", quoted(container)));
    }
    format!("{{{}}}", matchers.join(", "))
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Runs `query` over the last `seconds`, newest lines first as Loki returns them, then puts
/// them in order.
pub(crate) async fn query(
    client: &Client,
    endpoint: &Endpoint,
    query: &str,
    seconds: u64,
    limit: u32,
) -> Result<LokiLogs> {
    let end = Timestamp::now().as_nanosecond();
    let start = end - i128::from(seconds) * 1_000_000_000;
    let path = format!(
        "/loki/api/v1/query_range?query={}&start={start}&end={end}&limit={limit}&direction=backward",
        encode(query)
    );
    let answer = endpoint.get_json(client, &path, None).await?;
    let mut logs = parse(&answer, limit)?;
    logs.query = query.to_owned();
    logs.source = endpoint.label();
    Ok(logs)
}

pub(crate) fn parse(answer: &Value, limit: u32) -> Result<LokiLogs> {
    if answer.get("status").and_then(Value::as_str) == Some("error") {
        let message = answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("query failed");
        return Err(Error::Integration(format!("Loki: {message}")));
    }
    let streams = answer
        .pointer("/data/result")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut sources: Vec<LogSource> = Vec::new();
    let mut raw: Vec<(i128, usize, String)> = Vec::new();
    for stream in &streams {
        let label = |name: &str| {
            stream
                .pointer(&format!("/stream/{name}"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let source = LogSource {
            namespace: label("namespace"),
            pod: Some(label("pod"))
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| label("job")),
            container: label("container"),
        };
        let index = match sources.iter().position(|s| *s == source) {
            Some(index) => index,
            None => {
                sources.push(source);
                sources.len() - 1
            }
        };
        for value in stream
            .get("values")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(at) = value
                .get(0)
                .and_then(Value::as_str)
                .and_then(|t| t.parse::<i128>().ok())
            else {
                continue;
            };
            let text = value.get(1).and_then(Value::as_str).unwrap_or_default();
            raw.push((at, index, text.trim_end_matches('\n').to_owned()));
        }
    }
    let limited = raw.len() >= limit as usize;
    raw.sort_by_key(|(at, _, _)| *at);
    let lines = raw
        .into_iter()
        .map(|(at, source, text)| {
            let time = Timestamp::from_nanosecond(at)
                .map(|t| t.to_string())
                .unwrap_or_default();
            LogLine::parse(source, &format!("{time} {text}"))
        })
        .collect();
    Ok(LokiLogs {
        query: String::new(),
        source: String::new(),
        sources,
        lines,
        limited,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_from_objects() {
        assert_eq!(
            default_query("Pod", "shop", "web-1", None, None),
            "{namespace=\"shop\", pod=\"web-1\"}"
        );
        assert_eq!(
            default_query("Deployment", "shop", "web", None, Some("nginx")),
            "{namespace=\"shop\", pod=~\"web-[a-z0-9]{1,10}-[a-z0-9]{5}\", container=\"nginx\"}"
        );
        let selector = BTreeMap::from([("app.kubernetes.io/name".to_owned(), "web".to_owned())]);
        assert_eq!(
            default_query("Service", "shop", "web", Some(&selector), None),
            "{namespace=\"shop\", app_kubernetes_io_name=\"web\"}"
        );
    }

    #[test]
    fn reads_streams_in_order() {
        let answer = serde_json::json!({
            "status": "success",
            "data": { "resultType": "streams", "result": [
                { "stream": { "namespace": "shop", "pod": "web-2", "container": "nginx" },
                  "values": [["1793300000000000002", "{\"level\":\"error\",\"msg\":\"boom\"}"]] },
                { "stream": { "namespace": "shop", "pod": "web-1", "container": "nginx" },
                  "values": [["1793300000000000003", "third\n"], ["1793300000000000001", "first"]] }
            ]}
        });
        let logs = parse(&answer, 3).unwrap();
        assert_eq!(logs.sources.len(), 2);
        assert_eq!(logs.sources[1].pod, "web-1");
        let texts: Vec<&str> = logs.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            ["first", "{\"level\":\"error\",\"msg\":\"boom\"}", "third"]
        );
        assert_eq!(logs.lines[0].source, 1);
        assert_eq!(
            logs.lines[0].time.as_deref(),
            Some("2026-10-29T18:53:20.000000001Z")
        );
        assert_eq!(logs.lines[1].level, Some(crate::logs::Level::Error));
        assert!(logs.limited);
    }
}
