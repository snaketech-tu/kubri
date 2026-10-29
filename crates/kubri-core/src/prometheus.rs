//! Charts from Prometheus: CPU, memory, restarts, network and requests of pods, workloads and
//! nodes over a time range, using the metrics kube-prometheus-stack and most setups collect
//! (cAdvisor, kube-state-metrics, node-exporter).

use std::collections::BTreeMap;

use kube::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::integrations::Endpoint;
use crate::{Error, Result};

/// At most this many lines per chart; the rest are added up as "Other".
const MAX_SERIES: usize = 8;
/// About this many points per line, whatever the range.
const POINTS: u64 = 120;

/// One line of a chart; points are `[unix seconds, value]`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub name: String,
    pub points: Vec<[f64; 2]>,
}

/// A level drawn across a chart, like a container's request or limit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartLevel {
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    /// `cpu`, `memory`, `restarts`, `network` or `requests`.
    pub id: String,
    pub title: String,
    /// `millicores`, `bytes`, `count`, `bytesPerSecond` or `perSecond`.
    pub unit: String,
    pub series: Vec<Series>,
    pub levels: Vec<ChartLevel>,
    /// The PromQL behind it, to copy.
    pub query: String,
}

/// Something that happened in the range, such as a rollout, drawn as a line across every chart.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    /// Unix seconds.
    pub at: f64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Charts {
    /// Where the numbers come from, like `monitoring/prometheus:9090`.
    pub source: String,
    pub start: f64,
    pub end: f64,
    pub step: f64,
    /// Charts with data; those Prometheus has nothing for are left out.
    pub charts: Vec<Chart>,
    pub markers: Vec<Marker>,
    /// Titles of the charts Prometheus had no data for, to say so.
    pub missing: Vec<String>,
}

/// What to chart: the kind decides how its pods are found.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Subject {
    Pod {
        namespace: String,
        name: String,
    },
    /// Pods owned by a workload of `kind`, matched by the names their controller gives them.
    Workload {
        kind: String,
        namespace: String,
        name: String,
    },
    Node {
        name: String,
    },
    /// Every pod of a namespace, or of the cluster.
    Namespace {
        namespace: Option<String>,
    },
}

/// The time range to chart.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    /// Seconds back from `end`.
    pub seconds: u64,
    /// Unix seconds; now when left out.
    #[serde(default)]
    pub end: Option<f64>,
}

impl Range {
    pub(crate) fn resolve(&self) -> (f64, f64, f64) {
        let seconds = self.seconds.clamp(300, 30 * 24 * 3600);
        let end = self.end.unwrap_or_else(now).floor();
        let step = (seconds / POINTS).max(15) as f64;
        (end - seconds as f64, end, step)
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

/// A PromQL string literal: Kubernetes names only need dots escaped in a regex, but quote
/// everything properly anyway.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn regex_escape(text: &str) -> String {
    regex::escape(text).replace('\\', "\\\\")
}

/// The pod names a workload's controller gives its pods, as a PromQL regex.
pub(crate) fn pod_pattern(kind: &str, name: &str) -> String {
    let name = regex_escape(name);
    match kind {
        "Deployment" => format!("{name}-[a-z0-9]{{1,10}}-[a-z0-9]{{5}}"),
        "StatefulSet" => format!("{name}-[0-9]+"),
        "CronJob" => format!("{name}-[0-9]+(-[0-9]+)?-[a-z0-9]{{5}}"),
        // DaemonSet, ReplicaSet, Job (an indexed Job adds the index).
        _ => format!("{name}(-[0-9]+)?-[a-z0-9]{{5}}"),
    }
}

impl Subject {
    /// The label matchers for container metrics of this subject's pods.
    fn pods(&self) -> String {
        match self {
            Subject::Pod { namespace, name } => {
                format!("namespace={},pod={}", quoted(namespace), quoted(name))
            }
            Subject::Workload {
                kind,
                namespace,
                name,
            } => {
                format!(
                    "namespace={},pod=~\"{}\"",
                    quoted(namespace),
                    pod_pattern(kind, name)
                )
            }
            Subject::Namespace { namespace: Some(ns) } => format!("namespace={}", quoted(ns)),
            Subject::Namespace { namespace: None } => "namespace!=\"\"".into(),
            Subject::Node { name } => format!("node={}", quoted(name)),
        }
    }

    /// What one line stands for: containers of a pod, pods of a workload, namespaces of the
    /// cluster; `None` adds everything up into one line.
    fn by(&self) -> Option<&'static str> {
        match self {
            Subject::Pod { .. } => Some("container"),
            Subject::Workload { .. } => Some("pod"),
            Subject::Namespace { namespace: Some(_) } => None,
            Subject::Namespace { namespace: None } => None,
            Subject::Node { .. } => None,
        }
    }
}

/// A chart and the queries to try for it, in order; the first with data wins.
struct Plan {
    id: &'static str,
    title: &'static str,
    unit: &'static str,
    queries: Vec<(String, Option<&'static str>)>,
    levels: Vec<ChartLevel>,
}

/// `sum by (x)` or `sum`.
fn sum(by: Option<&str>) -> String {
    by.map_or_else(|| "sum".to_owned(), |by| format!("sum by ({by})"))
}

/// The charts to draw for `subject`. `requests` and `limits` are per line (per pod of a
/// workload, or the one container of a pod), in millicores and bytes.
fn plans(subject: &Subject, window: &str, requests: Limits, limits: Limits) -> Vec<Plan> {
    let pods = subject.pods();
    let by = subject.by();
    let s = sum(by);
    let containers = format!("{pods},container!=\"\",container!=\"POD\"");
    let level = |label: &str, value: Option<f64>| {
        value.map(|value| ChartLevel {
            label: label.into(),
            value,
        })
    };
    let levels = |cpu: bool| {
        let (request, limit) = if cpu {
            (requests.cpu, limits.cpu)
        } else {
            (requests.memory, limits.memory)
        };
        [level("Request", request), level("Limit", limit)]
            .into_iter()
            .flatten()
            .collect()
    };
    if let Subject::Node { name } = subject {
        let node = quoted(name);
        let instance = format!("instance=~\"{}(:[0-9]+)?\"", regex_escape(name));
        let uname = format!("on(instance) group_left(nodename) node_uname_info{{nodename={node}}}");
        let devices =
            "device!~\"lo|veth.*|cali.*|flannel.*|cni.*|cilium.*|lxc.*|vxlan.*|tun.*|docker.*|br-.*\"";
        return vec![
            Plan {
                id: "cpu",
                title: "CPU",
                unit: "millicores",
                queries: vec![
                    (
                        format!(
                            "sum(rate(node_cpu_seconds_total{{mode!~\"idle|iowait|steal\"}}[{window}]) * {uname}) * 1000"
                        ),
                        None,
                    ),
                    (
                        format!(
                            "sum(rate(container_cpu_usage_seconds_total{{node={node},id=\"/\"}}[{window}])) * 1000"
                        ),
                        None,
                    ),
                    (
                        format!(
                            "sum(rate(container_cpu_usage_seconds_total{{{instance},id=\"/\"}}[{window}])) * 1000"
                        ),
                        None,
                    ),
                ],
                levels: level("Allocatable", requests.cpu).into_iter().collect(),
            },
            Plan {
                id: "memory",
                title: "Memory",
                unit: "bytes",
                queries: vec![
                    (
                        format!(
                            "sum((node_memory_MemTotal_bytes - node_memory_MemAvailable_bytes) * {uname})"
                        ),
                        None,
                    ),
                    (
                        format!("sum(container_memory_working_set_bytes{{node={node},id=\"/\"}})"),
                        None,
                    ),
                    (
                        format!("sum(container_memory_working_set_bytes{{{instance},id=\"/\"}})"),
                        None,
                    ),
                ],
                levels: level("Allocatable", requests.memory).into_iter().collect(),
            },
            Plan {
                id: "network",
                title: "Network",
                unit: "bytesPerSecond",
                queries: vec![
                    (
                        format!(
                            "label_replace(sum(rate(node_network_receive_bytes_total{{{devices}}}[{window}]) * {uname}), \"direction\", \"Received\", \"\", \"\") \
                         or label_replace(sum(rate(node_network_transmit_bytes_total{{{devices}}}[{window}]) * {uname}), \"direction\", \"Sent\", \"\", \"\")"
                        ),
                        Some("direction"),
                    ),
                    (
                        format!(
                            "label_replace(sum(rate(container_network_receive_bytes_total{{node={node},id=\"/\"}}[{window}])), \"direction\", \"Received\", \"\", \"\") \
                         or label_replace(sum(rate(container_network_transmit_bytes_total{{node={node},id=\"/\"}}[{window}])), \"direction\", \"Sent\", \"\", \"\")"
                        ),
                        Some("direction"),
                    ),
                ],
                levels: Vec::new(),
            },
            Plan {
                id: "restarts",
                title: "Container restarts",
                unit: "count",
                queries: vec![(
                    format!(
                        "sum(increase(kube_pod_container_status_restarts_total[{window}]) * on(namespace, pod) group_left() max by (namespace, pod) (kube_pod_info{{node={node}}}))"
                    ),
                    None,
                )],
                levels: Vec::new(),
            },
        ];
    }
    let restarts_by = match subject {
        Subject::Pod { .. } => Some("container"),
        _ => by,
    };
    let mut plans = vec![
        Plan {
            id: "cpu",
            title: "CPU",
            unit: "millicores",
            queries: vec![(
                format!("{s} (rate(container_cpu_usage_seconds_total{{{containers}}}[{window}])) * 1000"),
                by,
            )],
            levels: levels(true),
        },
        Plan {
            id: "memory",
            title: "Memory",
            unit: "bytes",
            queries: vec![(
                format!("{s} (container_memory_working_set_bytes{{{containers}}})"),
                by,
            )],
            levels: levels(false),
        },
        Plan {
            id: "restarts",
            title: "Container restarts",
            unit: "count",
            queries: vec![(
                format!(
                    "{} (increase(kube_pod_container_status_restarts_total{{{pods}}}[{window}]))",
                    sum(restarts_by)
                ),
                restarts_by,
            )],
            levels: Vec::new(),
        },
        Plan {
            id: "network",
            title: "Network",
            unit: "bytesPerSecond",
            // Network counters belong to the pod, not to a container.
            queries: vec![(
                format!(
                    "label_replace(sum(rate(container_network_receive_bytes_total{{{pods}}}[{window}])), \"direction\", \"Received\", \"\", \"\") \
                     or label_replace(sum(rate(container_network_transmit_bytes_total{{{pods}}}[{window}])), \"direction\", \"Sent\", \"\", \"\")"
                ),
                Some("direction"),
            )],
            levels: Vec::new(),
        },
    ];
    // The whole cluster as metrics-server measures it: the nodes' root cgroups, which include
    // what runs outside pods. Containers added up are the fallback.
    if matches!(subject, Subject::Namespace { namespace: None }) {
        plans[0].queries.insert(
            0,
            (
                format!("sum(rate(container_cpu_usage_seconds_total{{id=\"/\"}}[{window}])) * 1000"),
                None,
            ),
        );
        plans[1].queries.insert(
            0,
            (
                "sum(container_memory_working_set_bytes{id=\"/\"})".to_owned(),
                None,
            ),
        );
    }
    // Request rates under the names common libraries and ingress controllers use.
    if !matches!(subject, Subject::Namespace { namespace: None }) {
        let requests_by = match subject {
            Subject::Workload { .. } | Subject::Pod { .. } => Some("code"),
            _ => None,
        };
        let r = sum(requests_by);
        let code = |metric: &str, label: &str| {
            format!(
                "label_replace({} (label_replace(rate({metric}{{{pods}}}[{window}]), \"code\", \"$1xx\", \"{label}\", \"([0-9]).*\")), \"code\", \"other\", \"code\", \"^$\")",
                r
            )
        };
        plans.push(Plan {
            id: "requests",
            title: "Requests",
            unit: "perSecond",
            queries: vec![
                (code("http_requests_total", "code"), requests_by),
                (code("http_requests_total", "status"), requests_by),
                (code("http_server_requests_seconds_count", "status"), requests_by),
                (
                    code(
                        "http_server_request_duration_seconds_count",
                        "http_response_status_code",
                    ),
                    requests_by,
                ),
                (
                    code("http_server_duration_milliseconds_count", "http_status_code"),
                    requests_by,
                ),
            ],
            levels: Vec::new(),
        });
    }
    plans
}

/// CPU in millicores and memory in bytes, where known.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Limits {
    pub cpu: Option<f64>,
    pub memory: Option<f64>,
}

/// The rate window for a step: four scrapes at least, and two steps so no sample is skipped.
fn window(step: f64) -> String {
    format!("{}s", (step as u64 * 2).max(120))
}

/// One series as Prometheus returns it: its labels and points.
pub(crate) type Raw = (BTreeMap<String, String>, Vec<[f64; 2]>);

/// Runs a range query; each result is its labels and points.
pub(crate) async fn query_range(
    client: &Client,
    endpoint: &Endpoint,
    query: &str,
    (start, end, step): (f64, f64, f64),
) -> Result<Vec<Raw>> {
    let path = format!(
        "/api/v1/query_range?query={}&start={start}&end={end}&step={step}",
        encode(query)
    );
    let answer = endpoint.get_json(client, &path, None).await?;
    parse_matrix(&answer)
}

pub(crate) fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub(crate) fn check(answer: &Value) -> Result<()> {
    if answer.get("status").and_then(Value::as_str) == Some("error") {
        let message = answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("query failed");
        return Err(Error::Integration(format!("Prometheus: {message}")));
    }
    Ok(())
}

fn labels(result: &Value) -> BTreeMap<String, String> {
    result
        .get("metric")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

fn number(value: &Value) -> Option<f64> {
    let value: f64 = value.as_str()?.parse().ok()?;
    value.is_finite().then_some(value)
}

pub(crate) fn parse_matrix(answer: &Value) -> Result<Vec<Raw>> {
    check(answer)?;
    let results = answer
        .pointer("/data/result")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(results
        .iter()
        .map(|r| {
            let points = r
                .get("values")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|v| Some([v.get(0)?.as_f64()?, number(v.get(1)?)?]))
                        .collect()
                })
                .unwrap_or_default();
            (labels(r), points)
        })
        .collect())
}

/// Names the lines by `by`, biggest first, and adds up all past the first few as "Other".
fn series(results: Vec<Raw>, by: Option<&str>) -> Vec<Series> {
    let mut lines: Vec<Series> = results
        .into_iter()
        .filter(|(_, points)| !points.is_empty())
        .map(|(labels, points)| Series {
            name: by
                .and_then(|by| labels.get(by).cloned())
                .unwrap_or_else(|| "Total".to_owned()),
            points,
        })
        .collect();
    let mean = |s: &Series| s.points.iter().map(|p| p[1]).sum::<f64>() / s.points.len() as f64;
    lines.sort_by(|a, b| mean(b).total_cmp(&mean(a)).then_with(|| a.name.cmp(&b.name)));
    // Sent and received read best in that order whatever their size.
    if by == Some("direction") {
        lines.sort_by(|a, b| a.name.cmp(&b.name));
    }
    if lines.len() > MAX_SERIES {
        let rest = lines.split_off(MAX_SERIES - 1);
        let mut other: BTreeMap<u64, f64> = BTreeMap::new();
        for line in &rest {
            for [at, value] in &line.points {
                *other.entry(at.to_bits()).or_default() += value;
            }
        }
        let mut points: Vec<[f64; 2]> = other.into_iter().map(|(at, v)| [f64::from_bits(at), v]).collect();
        points.sort_by(|a, b| a[0].total_cmp(&b[0]));
        lines.push(Series {
            name: format!("{} others", rest.len()),
            points,
        });
    }
    lines
}

/// Draws the charts of `subject` over `range`.
pub(crate) async fn charts(
    client: &Client,
    endpoint: &Endpoint,
    subject: &Subject,
    range: Range,
    requests: Limits,
    limits: Limits,
    markers: Vec<Marker>,
) -> Result<Charts> {
    let bounds = range.resolve();
    let plans = plans(subject, &window(bounds.2), requests, limits);
    let drawn = plans.iter().map(|plan| async move {
        for (query, by) in &plan.queries {
            match query_range(client, endpoint, query, bounds).await {
                Ok(results) => {
                    let lines = series(results, *by);
                    if !lines.is_empty() {
                        return Ok(Some((query.clone(), lines)));
                    }
                }
                Err(err) => return Err(err),
            }
        }
        Ok(None)
    });
    let results = futures::future::join_all(drawn).await;
    let mut charts = Vec::new();
    let mut missing = Vec::new();
    let mut failure = None;
    for (plan, result) in plans.iter().zip(results) {
        match result {
            Ok(Some((query, series))) => charts.push(Chart {
                id: plan.id.into(),
                title: plan.title.into(),
                unit: plan.unit.into(),
                series,
                levels: plan.levels.clone(),
                query,
            }),
            // A chart for request rates only makes sense where the app counts them.
            Ok(None) if plan.id == "requests" => {}
            Ok(None) => missing.push(plan.title.to_owned()),
            Err(err) => {
                missing.push(plan.title.to_owned());
                failure.get_or_insert(err);
            }
        }
    }
    // One broken chart is not worth failing all of them; Prometheus being down is.
    if charts.is_empty()
        && let Some(err) = failure
    {
        return Err(err);
    }
    let (start, end, step) = bounds;
    Ok(Charts {
        source: endpoint.label(),
        start,
        end,
        step,
        charts,
        markers: markers
            .into_iter()
            .filter(|m| m.at >= start && m.at <= end)
            .collect(),
        missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_names_of_workloads() {
        let re = |kind, name| {
            regex::Regex::new(&format!(
                "^(?:{})$",
                pod_pattern(kind, name).replace("\\\\", "\\")
            ))
            .unwrap()
        };
        let deploy = re("Deployment", "web");
        assert!(deploy.is_match("web-6756d48f97-cjzr9"));
        assert!(!deploy.is_match("web-api-6756d48f97-cjzr9"));
        assert!(re("StatefulSet", "db").is_match("db-0"));
        assert!(!re("StatefulSet", "db").is_match("db-api-0"));
        assert!(re("DaemonSet", "agent").is_match("agent-x7k2p"));
        assert!(re("Job", "migrate").is_match("migrate-0-x7k2p"));
        assert!(re("CronJob", "nightly").is_match("nightly-29360640-x7k2p"));
        assert!(re("Deployment", "a.b").is_match("a.b-6756d48f97-cjzr9"));
        assert!(!re("Deployment", "a.b").is_match("axb-6756d48f97-cjzr9"));
    }

    #[test]
    fn queries_name_the_pods() {
        let subject = Subject::Workload {
            kind: "Deployment".into(),
            namespace: "shop".into(),
            name: "web".into(),
        };
        let plans = plans(&subject, "120s", Limits::default(), Limits::default());
        assert_eq!(
            plans[0].queries[0].0,
            "sum by (pod) (rate(container_cpu_usage_seconds_total{namespace=\"shop\",pod=~\"web-[a-z0-9]{1,10}-[a-z0-9]{5}\",container!=\"\",container!=\"POD\"}[120s])) * 1000"
        );
        assert_eq!(
            plans.iter().map(|p| p.id).collect::<Vec<_>>(),
            ["cpu", "memory", "restarts", "network", "requests"]
        );
        let node = plans_for_node();
        assert!(node[0].queries[0].0.contains("node_uname_info{nodename=\"n1\"}"));
    }

    fn plans_for_node() -> Vec<Plan> {
        plans(
            &Subject::Node { name: "n1".into() },
            "120s",
            Limits::default(),
            Limits::default(),
        )
    }

    #[test]
    fn reads_matrices_and_folds_small_lines() {
        let answer = serde_json::json!({
            "status": "success",
            "data": { "resultType": "matrix", "result": (0..10).map(|i| serde_json::json!({
                "metric": { "pod": format!("p{i}") },
                "values": [[100.0, format!("{i}")], [115.0, "NaN"], [130.0, format!("{}", i * 2)]]
            })).collect::<Vec<_>>() }
        });
        let lines = series(parse_matrix(&answer).unwrap(), Some("pod"));
        assert_eq!(lines.len(), MAX_SERIES);
        assert_eq!(lines[0].name, "p9");
        assert_eq!(lines[0].points, vec![[100.0, 9.0], [130.0, 18.0]]);
        let other = lines.last().unwrap();
        assert_eq!(other.name, "3 others");
        assert_eq!(other.points, vec![[100.0, 3.0], [130.0, 6.0]]);
        let error = serde_json::json!({ "status": "error", "error": "parse error" });
        assert!(
            parse_matrix(&error)
                .unwrap_err()
                .to_string()
                .contains("parse error")
        );
    }

    #[test]
    fn ranges_have_about_the_same_points() {
        let (start, end, step) = Range {
            seconds: 3600,
            end: Some(10_000.0),
        }
        .resolve();
        assert_eq!((start, end, step), (6400.0, 10_000.0, 30.0));
        assert_eq!(
            Range {
                seconds: 7 * 86400,
                end: Some(0.0)
            }
            .resolve()
            .2,
            5040.0
        );
        assert_eq!(window(30.0), "120s");
        assert_eq!(encode("a b{c=\"d\"}"), "a%20b%7Bc%3D%22d%22%7D");
    }
}
