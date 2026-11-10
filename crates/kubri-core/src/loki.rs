//! Log history from Loki, beyond the lifetime of a pod: the same lines the log workspace shows
//! from the cluster, found with a LogQL query prefilled from the object.

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

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

/// The label names a log collector gives the namespace, pod and container, as a template in
/// the settings names them; Kubri's defaults where it gives none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Labels {
    pub namespace: String,
    pub pod: String,
    pub container: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            namespace: "namespace".into(),
            pod: "pod".into(),
            container: "container".into(),
        }
    }
}

impl Labels {
    /// The names `template` gives, with the defaults for the ones it leaves empty.
    pub(crate) fn from_template(template: Option<&crate::LabelTemplate>) -> Self {
        let mut labels = Self::default();
        if let Some(t) = template {
            for (name, given) in [
                (&mut labels.namespace, &t.namespace),
                (&mut labels.pod, &t.pod),
                (&mut labels.container, &t.container),
            ] {
                if !given.is_empty() {
                    name.clone_from(given);
                }
            }
        }
        labels
    }
}

/// The stream selector for all lines of a namespace.
pub(crate) fn namespace_query(namespace: &str, labels: &Labels) -> String {
    format!("{{{}={}}}", labels.namespace, quoted(namespace))
}

/// The stream selector for an object's pods, by their names: a pod's own, those a workload's
/// controller gives, or for a Service those of the pods it selects now, as log collectors label
/// lines with the pod but rarely with its labels.
pub(crate) fn default_query(
    kind: &str,
    namespace: &str,
    name: &str,
    selected: Option<&[String]>,
    container: Option<&str>,
    labels: &Labels,
) -> String {
    let Labels {
        namespace: ns_label,
        pod: pod_label,
        container: container_label,
    } = labels;
    let mut matchers = vec![format!("{ns_label}={}", quoted(namespace))];
    match (kind, selected) {
        ("Pod", _) => matchers.push(format!("{pod_label}={}", quoted(name))),
        ("Service", Some(pods)) => {
            // Pod names hold only lower-case letters, digits, `-` and `.`, of which `.` is special.
            let names: Vec<String> = pods.iter().map(|p| p.replace('.', "\\\\.")).collect();
            matchers.push(format!("{pod_label}=~\"{}\"", names.join("|")));
        }
        _ => matchers.push(format!(
            "{pod_label}=~\"{}\"",
            crate::prometheus::pod_pattern(kind, name)
        )),
    }
    if let Some(container) = container.filter(|c| !c.is_empty()) {
        matchers.push(format!("{container_label}={}", quoted(container)));
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
    labels: &Labels,
) -> Result<LokiLogs> {
    let end = Timestamp::now().as_nanosecond();
    let start = end - i128::from(seconds) * 1_000_000_000;
    let path = format!(
        "/loki/api/v1/query_range?query={}&start={start}&end={end}&limit={limit}&direction=backward",
        encode(query)
    );
    let answer = endpoint.get_json(client, &path, None).await?;
    let mut logs = parse(&answer, limit, labels)?;
    logs.query = query.to_owned();
    logs.source = endpoint.label();
    Ok(logs)
}

pub(crate) fn parse(answer: &Value, limit: u32, labels: &Labels) -> Result<LokiLogs> {
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
            namespace: label(&labels.namespace),
            pod: Some(label(&labels.pod))
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| label("job")),
            container: label(&labels.container),
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
    fn queries_with_other_label_names() {
        let labels = Labels::from_template(Some(&crate::LabelTemplate {
            name: "otel".into(),
            namespace: "exported_namespace".into(),
            pod: String::new(),
            container: "k8s_container_name".into(),
        }));
        assert_eq!(
            default_query("Pod", "shop", "web-1", None, Some("app"), &labels),
            "{exported_namespace=\"shop\", pod=\"web-1\", k8s_container_name=\"app\"}"
        );
        assert_eq!(namespace_query("shop", &labels), "{exported_namespace=\"shop\"}");
        let answer = serde_json::json!({ "status": "success", "data": { "result": [
            { "stream": { "exported_namespace": "shop", "pod": "web-1", "k8s_container_name": "app" },
              "values": [["1700000000000000000", "hello"]] }
        ] } });
        let logs = parse(&answer, 10, &labels).unwrap();
        assert_eq!(logs.sources[0].namespace, "shop");
        assert_eq!(logs.sources[0].container, "app");
    }

    #[test]
    fn queries_from_objects() {
        assert_eq!(
            default_query("Pod", "shop", "web-1", None, None, &Labels::default()),
            "{namespace=\"shop\", pod=\"web-1\"}"
        );
        assert_eq!(
            default_query(
                "Deployment",
                "shop",
                "web",
                None,
                Some("nginx"),
                &Labels::default()
            ),
            "{namespace=\"shop\", pod=~\"web-[a-z0-9]{1,10}-[a-z0-9]{5}\", container=\"nginx\"}"
        );
        let pods = ["web-1".to_owned(), "web.2".to_owned()];
        assert_eq!(
            default_query("Service", "shop", "web", Some(&pods), None, &Labels::default()),
            "{namespace=\"shop\", pod=~\"web-1|web\\\\.2\"}"
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
        let logs = parse(&answer, 3, &Labels::default()).unwrap();
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
