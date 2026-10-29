//! Tools many clusters run, found without setup: Prometheus, Grafana and Loki by their
//! Services, Flux, Argo CD and cert-manager by their API groups.
//!
//! Kubri reaches a Service through the API server's service proxy, with the kubeconfig's
//! credentials, so nothing needs to be exposed or port-forwarded. A URL in the context's
//! settings takes the place of what was found.

use std::sync::LazyLock;
use std::time::Duration;

use k8s_openapi::api::core::v1::Service;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::prometheus::{self, Charts, Limits, Marker, Range, Subject};
use crate::{Error, ObjectRef, ResourceInfo, Result};
use crate::{grafana, loki};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Where a tool is reached: the turned-off value in the settings, a URL, or a Service.
pub const OFF: &str = "off";

/// The URLs a context's settings give for its tools; empty finds them, `off` turns them off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IntegrationSettings {
    pub prometheus: String,
    pub grafana: String,
    /// A Grafana service account token, sent to a Grafana given by URL.
    pub grafana_token: String,
    pub loki: String,
}

impl IntegrationSettings {
    pub(crate) fn normalized(mut self) -> Result<Self> {
        for (name, url) in [
            ("Prometheus", &mut self.prometheus),
            ("Grafana", &mut self.grafana),
            ("Loki", &mut self.loki),
        ] {
            *url = url.trim().trim_end_matches('/').to_owned();
            if url.eq_ignore_ascii_case(OFF) {
                *url = OFF.to_owned();
            } else if !url.is_empty() {
                Endpoint::from_url(url).map_err(|e| Error::Unsupported(format!("{name}: {e}")))?;
            }
        }
        self.grafana_token = self.grafana_token.trim().to_owned();
        Ok(self)
    }
}

/// How Kubri reaches a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "via")]
pub enum Endpoint {
    /// Through the API server's service proxy.
    #[serde(rename_all = "camelCase")]
    Service {
        namespace: String,
        name: String,
        port: u16,
        https: bool,
    },
    /// Straight to a URL, from the settings.
    Url { url: String },
}

impl Endpoint {
    /// A URL from the settings. One that names a Service in the cluster, like
    /// `http://prometheus.monitoring.svc:9090`, goes through the service proxy.
    pub(crate) fn from_url(url: &str) -> Result<Self, String> {
        let parsed = reqwest::Url::parse(url).map_err(|e| format!("`{url}` is not a URL ({e})"))?;
        let https = match parsed.scheme() {
            "http" => false,
            "https" => true,
            other => {
                return Err(format!(
                    "`{url}` must start with http:// or https://, not {other}://"
                ));
            }
        };
        let host = parsed.host_str().unwrap_or_default();
        let service = host
            .strip_suffix(".svc.cluster.local")
            .or_else(|| host.strip_suffix(".svc"))
            .and_then(|rest| rest.split_once('.'))
            .filter(|(name, namespace)| {
                !name.is_empty() && !namespace.is_empty() && !namespace.contains('.')
            });
        Ok(match service {
            Some((name, namespace)) if parsed.path() == "/" || parsed.path().is_empty() => {
                Endpoint::Service {
                    namespace: namespace.to_owned(),
                    name: name.to_owned(),
                    port: parsed.port_or_known_default().unwrap_or(80),
                    https,
                }
            }
            _ => Endpoint::Url {
                url: url.trim_end_matches('/').to_owned(),
            },
        })
    }

    /// Where it is, for people: `monitoring/prometheus:9090` or the URL.
    pub fn label(&self) -> String {
        match self {
            Endpoint::Service {
                namespace,
                name,
                port,
                ..
            } => format!("{namespace}/{name}:{port}"),
            Endpoint::Url { url } => url.clone(),
        }
    }

    /// The path of the API server's proxy to the Service, with the path and query after it.
    fn proxy_path(namespace: &str, name: &str, port: u16, https: bool, path: &str) -> String {
        let scheme = if https { "https:" } else { "" };
        format!("/api/v1/namespaces/{namespace}/services/{scheme}{name}:{port}/proxy{path}")
    }

    /// GETs `path` (with its query) and returns the body. `token` goes only to URLs: the
    /// service proxy uses the Authorization header for the API server.
    pub(crate) async fn get(&self, client: &Client, path: &str, token: Option<&str>) -> Result<String> {
        match self {
            Endpoint::Service {
                namespace,
                name,
                port,
                https,
            } => {
                let uri = Self::proxy_path(namespace, name, *port, *https, path);
                let request = http::Request::get(uri)
                    .header(http::header::ACCEPT, "application/json")
                    .body(Vec::new())
                    .map_err(|e| Error::Unsupported(e.to_string()))?;
                let call = client.request_text(request);
                match tokio::time::timeout(TIMEOUT, call).await {
                    Ok(Ok(text)) => Ok(text),
                    Ok(Err(kube::Error::Api(status)))
                        if status.message.contains("no endpoints available") =>
                    {
                        Err(Error::Integration(format!(
                            "{} has no ready pod to answer",
                            self.label()
                        )))
                    }
                    Ok(Err(kube::Error::Api(status))) => Err(Error::Integration(format!(
                        "{} answered {} through the API server: {}",
                        self.label(),
                        status.code,
                        short(&status.message)
                    ))),
                    Ok(Err(err)) => Err(Error::Integration(format!(
                        "{}: {}",
                        self.label(),
                        Error::from(err)
                    ))),
                    Err(_) => Err(Error::Integration(format!(
                        "{} did not answer in time",
                        self.label()
                    ))),
                }
            }
            Endpoint::Url { url } => {
                let http = CLIENT.as_ref().map_err(|e| Error::Integration(e.clone()))?;
                let mut request = http
                    .get(format!("{url}{path}"))
                    .header("Accept", "application/json");
                if let Some(token) = token.filter(|t| !t.is_empty()) {
                    request = request.bearer_auth(token);
                }
                let response = request
                    .send()
                    .await
                    .map_err(|e| Error::Integration(format!("{url}: {}", reqwest_message(&e))))?;
                let status = response.status();
                let text = response
                    .text()
                    .await
                    .map_err(|e| Error::Integration(format!("{url}: {}", reqwest_message(&e))))?;
                if !status.is_success() {
                    return Err(Error::Integration(format!(
                        "{url} answered {}: {}",
                        status.as_u16(),
                        short(&text)
                    )));
                }
                Ok(text)
            }
        }
    }

    pub(crate) async fn get_json(&self, client: &Client, path: &str, token: Option<&str>) -> Result<Value> {
        let text = self.get(client, path, token).await?;
        serde_json::from_str(&text).map_err(|_| {
            Error::Integration(format!(
                "{} did not answer with JSON: {}",
                self.label(),
                short(&text)
            ))
        })
    }
}

static CLIENT: LazyLock<std::result::Result<reqwest::Client, String>> = LazyLock::new(|| {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())
});

fn reqwest_message(err: &reqwest::Error) -> String {
    let mut message = err.to_string();
    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        message = format!("{message}: {cause}");
        source = cause.source();
    }
    message
}

/// The first line of an answer, cut short.
fn short(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or_default();
    if line.chars().count() > 200 {
        format!("{}…", line.chars().take(200).collect::<String>())
    } else {
        line.to_owned()
    }
}

/// One tool and how it was found.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    /// `prometheus`, `grafana`, `loki`, `flux`, `argocd` or `certManager`.
    pub id: String,
    pub name: String,
    /// Where Kubri reaches it, for the tools it talks to.
    pub endpoint: Option<Endpoint>,
    /// `found`, `settings` (the URL in the settings), `off` or `missing`.
    pub source: String,
    /// The version it reported, when asked.
    pub version: Option<String>,
    /// Why it cannot be used, when it was found but did not answer.
    pub error: Option<String>,
}

/// What the cluster runs, as the integrations page shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integrations {
    pub items: Vec<Integration>,
}

impl Integrations {
    pub fn get(&self, id: &str) -> Option<&Integration> {
        self.items.iter().find(|i| i.id == id)
    }

    /// The endpoint of a tool Kubri talks to, if it is there and on.
    pub fn endpoint(&self, id: &str) -> Option<&Endpoint> {
        self.get(id).and_then(|i| i.endpoint.as_ref())
    }

    pub fn has(&self, id: &str) -> bool {
        self.get(id)
            .is_some_and(|i| i.source == "found" || i.source == "settings")
    }
}

/// Which of the tools that live on Services a Service could be, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Prometheus,
    Grafana,
    Loki,
}

/// How well a Service fits a tool: higher is better, `None` is not it.
fn score(tool: Tool, service: &Service) -> Option<(i32, u16)> {
    let name = service.metadata.name.as_deref().unwrap_or_default();
    let labels = service.metadata.labels.clone().unwrap_or_default();
    let app = labels
        .get("app.kubernetes.io/name")
        .or_else(|| labels.get("app"))
        .map(String::as_str)
        .unwrap_or_default();
    let component = labels
        .get("app.kubernetes.io/component")
        .map(String::as_str)
        .unwrap_or_default();
    let spec = service.spec.as_ref()?;
    let ports = spec.ports.as_deref().unwrap_or_default();
    let headless = spec.cluster_ip.as_deref() == Some("None");
    let port_named = |names: &[&str], numbers: &[i32]| {
        ports
            .iter()
            .find(|p| p.name.as_deref().is_some_and(|n| names.contains(&n)) || numbers.contains(&p.port))
            .or_else(|| (ports.len() == 1).then(|| &ports[0]))
            .and_then(|p| u16::try_from(p.port).ok())
    };
    match tool {
        Tool::Prometheus => {
            let others = [
                "alertmanager",
                "operator",
                "node-exporter",
                "pushgateway",
                "adapter",
                "kube-state",
                "blackbox",
                "thanos-sidecar",
                "config-reloader",
            ];
            if others.iter().any(|o| name.contains(o)) {
                return None;
            }
            let named = app == "prometheus" || name.contains("prometheus");
            if !named {
                return None;
            }
            let port = port_named(&["http-web", "web", "http"], &[9090, 80])?;
            // kube-prometheus-stack's own Service, then the prometheus chart's, then the
            // operator's headless one.
            let mut score = 10;
            if name.ends_with("-prometheus") || name == "prometheus" || name == "prometheus-k8s" {
                score += 5;
            }
            if name == "prometheus-server" {
                score += 4;
            }
            if headless {
                score -= 3;
            }
            Some((score, port))
        }
        Tool::Grafana => {
            let named = app == "grafana" || name == "grafana" || name.ends_with("-grafana");
            if !named || name.contains("operator") || name.contains("image-renderer") {
                return None;
            }
            Some((
                10 - i32::from(headless) * 3,
                port_named(&["http-web", "service", "http", "grafana"], &[80, 3000])?,
            ))
        }
        Tool::Loki => {
            let named =
                app == "loki" || name == "loki" || name.starts_with("loki-") || name.ends_with("-loki");
            let skip = [
                "memberlist",
                "headless",
                "canary",
                "cache",
                "write",
                "backend",
                "ingester",
                "distributor",
                "compactor",
                "index-gateway",
                "ruler",
                "query-scheduler",
                "results",
            ];
            if !named || name.contains("promtail") || skip.iter().any(|s| name.contains(s)) {
                return None;
            }
            let port = port_named(&["http-metrics", "http", "http-web"], &[3100, 80])?;
            let mut score = 10;
            if component == "gateway" || name.ends_with("-gateway") {
                score += 5;
            } else if component == "read" || name.ends_with("-read") || name.contains("query-frontend") {
                score += 3;
            }
            if headless {
                score -= 3;
            }
            Some((score, port))
        }
    }
}

/// The best Service for `tool`, if any.
fn best(tool: Tool, services: &[Service]) -> Option<Endpoint> {
    services
        .iter()
        .filter_map(|s| score(tool, s).map(|(score, port)| (score, port, s)))
        .max_by_key(|(score, _, s)| (*score, std::cmp::Reverse(s.metadata.name.clone())))
        .map(|(_, port, s)| Endpoint::Service {
            namespace: s.metadata.namespace.clone().unwrap_or_default(),
            name: s.metadata.name.clone().unwrap_or_default(),
            port,
            https: false,
        })
}

/// Finds the tools: Services across the cluster for those Kubri talks to, served API groups
/// for the operators. `settings` override what is found.
pub(crate) async fn detect(
    client: &Client,
    resources: &[ResourceInfo],
    settings: &IntegrationSettings,
) -> Integrations {
    let services = match crate::typed::list(Api::<Service>::all(client.clone())).await {
        Ok(services) => services,
        Err(err) => {
            tracing::debug!("cannot list Services to find integrations: {err}");
            Vec::new()
        }
    };
    let mut items = Vec::new();
    for (id, name, tool, setting) in [
        ("prometheus", "Prometheus", Tool::Prometheus, &settings.prometheus),
        ("grafana", "Grafana", Tool::Grafana, &settings.grafana),
        ("loki", "Loki", Tool::Loki, &settings.loki),
    ] {
        let (endpoint, source) = if setting == OFF {
            (None, "off")
        } else if !setting.is_empty() {
            (Endpoint::from_url(setting).ok(), "settings")
        } else {
            match best(tool, &services) {
                Some(endpoint) => (Some(endpoint), "found"),
                None => (None, "missing"),
            }
        };
        items.push(Integration {
            id: id.into(),
            name: name.into(),
            endpoint,
            source: source.into(),
            version: None,
            error: None,
        });
    }
    let served = |group: &str, plural: &str| {
        resources
            .iter()
            .any(|r| r.reference.group == group && r.reference.plural == plural)
    };
    for (id, name, present) in [
        (
            "flux",
            "Flux",
            served("kustomize.toolkit.fluxcd.io", "kustomizations")
                || served("helm.toolkit.fluxcd.io", "helmreleases"),
        ),
        ("argocd", "Argo CD", served("argoproj.io", "applications")),
        (
            "certManager",
            "cert-manager",
            served("cert-manager.io", "certificates"),
        ),
    ] {
        items.push(Integration {
            id: id.into(),
            name: name.into(),
            endpoint: None,
            source: if present { "found" } else { "missing" }.into(),
            version: None,
            error: None,
        });
    }
    Integrations { items }
}

/// Asks each tool Kubri talks to for its version, which also shows that it answers.
pub(crate) async fn probe(client: &Client, integrations: &mut Integrations, grafana_token: Option<&str>) {
    let checks = integrations.items.iter().map(|item| {
        let path = match item.id.as_str() {
            "prometheus" => "/api/v1/status/buildinfo",
            "grafana" => "/api/health",
            "loki" => "/loki/api/v1/status/buildinfo",
            _ => "",
        };
        let token = (item.id == "grafana").then_some(grafana_token).flatten();
        let endpoint = item.endpoint.clone();
        async move {
            let endpoint = endpoint.filter(|_| !path.is_empty())?;
            Some(match endpoint.get_json(client, path, token).await {
                Ok(answer) => Ok(answer
                    .pointer("/data/version")
                    .or_else(|| answer.get("version"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)),
                Err(err) => Err(err.to_string()),
            })
        }
    });
    let answers = futures::future::join_all(checks).await;
    for (item, answer) in integrations.items.iter_mut().zip(answers) {
        match answer {
            Some(Ok(version)) => item.version = version,
            Some(Err(err)) => item.error = Some(err),
            None => {}
        }
    }
}

/// The containers' requests and limits added up, in millicores and bytes; a sum is only
/// known when every container sets it.
fn pod_spec_limits(spec: &Value) -> (Limits, Limits) {
    let containers = spec
        .get("containers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = |kind: &str, resource: &str, factor: f64| {
        containers
            .iter()
            .map(|c| {
                c.pointer(&format!("/resources/{kind}/{resource}"))
                    .and_then(Value::as_str)
                    .and_then(crate::parse_quantity)
            })
            .sum::<Option<f64>>()
            .filter(|_| !containers.is_empty())
            .map(|v| v * factor)
    };
    (
        Limits {
            cpu: total("requests", "cpu", 1000.0),
            memory: total("requests", "memory", 1.0),
        },
        Limits {
            cpu: total("limits", "cpu", 1000.0),
            memory: total("limits", "memory", 1.0),
        },
    )
}

fn timestamp(value: Option<&Value>) -> Option<f64> {
    let text = value?.as_str()?;
    let at: k8s_openapi::jiff::Timestamp = text.parse().ok()?;
    Some(at.as_millisecond() as f64 / 1000.0)
}

/// When a pod's containers last ended, as markers: a restart shows where it happened.
fn restart_markers(pod: &Value) -> Vec<Marker> {
    let statuses = pod
        .pointer("/status/containerStatuses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    statuses
        .iter()
        .filter_map(|s| {
            let ended = s.pointer("/lastState/terminated")?;
            let at = timestamp(ended.get("finishedAt"))?;
            let reason = ended.get("reason").and_then(Value::as_str).unwrap_or("ended");
            let name = s.get("name").and_then(Value::as_str).unwrap_or_default();
            Some(Marker {
                at,
                label: format!("{name} restarted ({reason})"),
            })
        })
        .collect()
}

impl crate::Kubri {
    /// The tools the cluster runs, found once per connection; `fresh` looks again and asks
    /// each tool whether it answers.
    pub async fn integrations(&self, fresh: bool) -> Result<Integrations> {
        let session = self.session().await?;
        let settings = self.settings().context(&session.info.context).integrations;
        let mut cached = session.integrations.lock().await;
        // Changed settings count as looking again.
        if let Some((_, found)) = cached.as_ref().filter(|(used, _)| !fresh && *used == settings) {
            return Ok(found.clone());
        }
        let mut found = detect(&session.client, &session.resources, &settings).await;
        if fresh {
            let token = Some(settings.grafana_token.as_str()).filter(|t| !t.is_empty());
            probe(&session.proxy_client, &mut found, token).await;
        }
        *cached = Some((settings, found.clone()));
        Ok(found)
    }

    /// Where Prometheus is, or why there is none to ask.
    async fn prometheus(&self) -> Result<Endpoint> {
        self.integrations(false)
            .await?
            .endpoint("prometheus")
            .cloned()
            .ok_or_else(|| {
                Error::Unsupported("no Prometheus found; give its URL on the integrations page".into())
            })
    }

    /// Charts of a pod, workload or node over `range`, from Prometheus.
    pub async fn charts(&self, target: &ObjectRef, range: Range) -> Result<Charts> {
        let session = self.object_session(target).await?;
        let endpoint = self.prometheus().await?;
        let info = session.info_for(&target.resource)?;
        let kind = info.kind.clone();
        let object = serde_json::to_value(
            session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?,
        )?;
        let namespace = target.namespace.clone().unwrap_or_default();
        let mut markers = Vec::new();
        let (subject, requests, limits) = match (target.resource.group.as_str(), kind.as_str()) {
            ("", "Pod") => {
                markers = restart_markers(&object);
                // Request and limit lines only fit a chart with one line.
                let spec = object.get("spec").cloned().unwrap_or_default();
                let single = spec
                    .get("containers")
                    .and_then(Value::as_array)
                    .is_some_and(|c| c.len() == 1);
                let (requests, limits) = if single {
                    pod_spec_limits(&spec)
                } else {
                    Default::default()
                };
                (
                    Subject::Pod {
                        namespace,
                        name: target.name.clone(),
                    },
                    requests,
                    limits,
                )
            }
            ("", "Node") => {
                let allocatable = Limits {
                    cpu: object
                        .pointer("/status/allocatable/cpu")
                        .and_then(Value::as_str)
                        .and_then(crate::parse_quantity)
                        .map(|c| c * 1000.0),
                    memory: object
                        .pointer("/status/allocatable/memory")
                        .and_then(Value::as_str)
                        .and_then(crate::parse_quantity),
                };
                (
                    Subject::Node {
                        name: target.name.clone(),
                    },
                    allocatable,
                    Limits::default(),
                )
            }
            ("apps", "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet")
            | ("batch", "Job" | "CronJob") => {
                let template = if kind == "CronJob" {
                    object.pointer("/spec/jobTemplate/spec/template/spec")
                } else {
                    object.pointer("/spec/template/spec")
                };
                let (requests, limits) = template.map(pod_spec_limits).unwrap_or_default();
                markers = self.rollouts(&session, &kind, &object).await;
                (
                    Subject::Workload {
                        kind: kind.clone(),
                        namespace,
                        name: target.name.clone(),
                    },
                    requests,
                    limits,
                )
            }
            ("", "Namespace") => (
                Subject::Namespace {
                    namespace: Some(target.name.clone()),
                },
                Limits::default(),
                Limits::default(),
            ),
            _ => {
                return Err(Error::Unsupported(format!(
                    "Kubri draws charts for pods, workloads, nodes and namespaces, not for a {kind}"
                )));
            }
        };
        prometheus::charts(
            &session.proxy_client,
            &endpoint,
            &subject,
            range,
            requests,
            limits,
            markers,
        )
        .await
    }

    /// When a workload rolled out a new revision: its ReplicaSets or ControllerRevisions.
    async fn rollouts(&self, session: &crate::Session, kind: &str, object: &Value) -> Vec<Marker> {
        let uid = object
            .pointer("/metadata/uid")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let namespace = object
            .pointer("/metadata/namespace")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let owned: Vec<Value> = match kind {
            "Deployment" => {
                let api: Api<k8s_openapi::api::apps::v1::ReplicaSet> =
                    Api::namespaced(session.client.clone(), namespace);
                crate::typed::list(api)
                    .await
                    .map(|items| {
                        items
                            .into_iter()
                            .filter_map(|i| serde_json::to_value(i).ok())
                            .collect()
                    })
                    .unwrap_or_default()
            }
            "StatefulSet" | "DaemonSet" => {
                let api: Api<k8s_openapi::api::apps::v1::ControllerRevision> =
                    Api::namespaced(session.client.clone(), namespace);
                crate::typed::list(api)
                    .await
                    .map(|items| {
                        items
                            .into_iter()
                            .filter_map(|i| serde_json::to_value(i).ok())
                            .collect()
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };
        let mut markers: Vec<Marker> = owned
            .iter()
            .filter(|o| {
                o.pointer("/metadata/ownerReferences")
                    .and_then(Value::as_array)
                    .is_some_and(|refs| {
                        refs.iter()
                            .any(|r| r.get("uid").and_then(Value::as_str) == Some(uid))
                    })
            })
            .filter_map(|o| {
                let at = timestamp(o.pointer("/metadata/creationTimestamp"))?;
                let revision = o
                    .pointer("/metadata/annotations/deployment.kubernetes.io~1revision")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| o.get("revision").map(|r| r.to_string()));
                Some(Marker {
                    at,
                    label: revision.map_or_else(|| "Rollout".to_owned(), |r| format!("Revision {r}")),
                })
            })
            .collect();
        markers.sort_by(|a, b| a.at.total_cmp(&b.at));
        markers
    }

    /// CPU and memory of a namespace, or the cluster, over `range`, for the overview's trends.
    pub async fn usage_history(&self, namespace: Option<&str>, range: Range) -> Result<Charts> {
        let session = self.session().await?;
        let endpoint = self.prometheus().await?;
        let subject = Subject::Namespace {
            namespace: namespace.map(str::to_owned),
        };
        let mut charts = prometheus::charts(
            &session.proxy_client,
            &endpoint,
            &subject,
            range,
            Limits::default(),
            Limits::default(),
            Vec::new(),
        )
        .await?;
        charts.charts.retain(|c| c.id == "cpu" || c.id == "memory");
        charts.missing.retain(|m| m == "CPU" || m == "Memory");
        Ok(charts)
    }

    /// Runs a PromQL query over a range, for agents and the query field.
    pub async fn prometheus_query(&self, query: &str, range: Range) -> Result<Vec<prometheus::Series>> {
        let session = self.session().await?;
        let endpoint = self.prometheus().await?;
        let results =
            prometheus::query_range(&session.proxy_client, &endpoint, query, range.resolve()).await?;
        Ok(results
            .into_iter()
            .map(|(labels, points)| prometheus::Series {
                name: if labels.is_empty() {
                    "{}".to_owned()
                } else {
                    format!(
                        "{{{}}}",
                        labels
                            .iter()
                            .map(|(k, v)| format!("{k}=\"{v}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
                points,
            })
            .collect())
    }
}

/// How long Grafana's dashboards are kept before Kubri asks again.
const DASHBOARDS_FOR: std::time::Duration = std::time::Duration::from_secs(300);

impl crate::Kubri {
    /// The dashboards of the connected cluster's Grafana, and why Grafana's own list could not
    /// be read if it could not; kept for a few minutes.
    async fn grafana_dashboards(
        &self,
        session: &crate::Session,
    ) -> Result<(Vec<grafana::Dashboard>, Option<String>)> {
        let integrations = self.integrations(false).await?;
        let endpoint = integrations.endpoint("grafana").cloned().ok_or_else(|| {
            Error::Unsupported("no Grafana found; give its URL on the integrations page".into())
        })?;
        let mut cached = session.grafana.lock().await;
        if let Some((at, dashboards, note)) = cached.as_ref()
            && at.elapsed() < DASHBOARDS_FOR
        {
            return Ok((dashboards.clone(), note.clone()));
        }
        let token = self
            .settings()
            .context(&session.info.context)
            .integrations
            .grafana_token;
        let token = Some(token.as_str()).filter(|t| !t.is_empty());
        let (api, mut dashboards) = futures::join!(
            grafana::from_api(&session.proxy_client, &endpoint, token),
            grafana::from_config_maps(&session.client)
        );
        let note = match api {
            Ok(found) => {
                dashboards.splice(0..0, found);
                None
            }
            Err(err) => Some(err.to_string()),
        };
        *cached = Some((std::time::Instant::now(), dashboards.clone(), note.clone()));
        Ok((dashboards, note))
    }

    /// Grafana dashboards that show `target`, as links with its variables filled in.
    pub async fn dashboards(&self, target: &ObjectRef) -> Result<grafana::Dashboards> {
        let session = self.object_session(target).await?;
        let kind = session.info_for(&target.resource)?.kind.clone();
        let namespace = target.namespace.clone().unwrap_or_default();
        let subject = match (target.resource.group.as_str(), kind.as_str()) {
            ("", "Pod") => grafana::Subject::Pod {
                namespace,
                name: target.name.clone(),
            },
            ("", "Node") => grafana::Subject::Node {
                name: target.name.clone(),
            },
            ("", "Namespace") => grafana::Subject::Namespace {
                name: target.name.clone(),
            },
            ("apps", "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet")
            | ("batch", "Job" | "CronJob") => grafana::Subject::Workload {
                kind,
                namespace,
                name: target.name.clone(),
            },
            _ => return Ok(grafana::Dashboards::default()),
        };
        let (dashboards, note) = self.grafana_dashboards(&session).await?;
        let links = grafana::links(&dashboards, &subject);
        Ok(grafana::Dashboards {
            note: note.filter(|_| links.is_empty()),
            links,
        })
    }

    /// Where the browser reaches a tool: its URL, or for one on a Service a port-forward to
    /// it, started once and used again.
    pub async fn integration_url(&self, id: &str) -> Result<String> {
        let integrations = self.integrations(false).await?;
        let endpoint = integrations
            .endpoint(id)
            .cloned()
            .ok_or_else(|| Error::Unsupported(format!("{id} is not found in this cluster")))?;
        let (namespace, name, port, https) = match endpoint {
            Endpoint::Url { url } => return Ok(url),
            Endpoint::Service {
                namespace,
                name,
                port,
                https,
            } => (namespace, name, port, https),
        };
        let context = self.cluster().await.map(|c| c.context).unwrap_or_default();
        let scheme = if https { "https" } else { "http" };
        let running = self.port_forwards().into_iter().find(|f| {
            f.context == context
                && f.target.resource.group.is_empty()
                && f.target.resource.plural == "services"
                && f.target.namespace.as_deref() == Some(namespace.as_str())
                && f.target.name == name
                && f.remote_port == port
        });
        let forward = match running {
            Some(forward) => forward,
            None => {
                self.start_port_forward(crate::PortForwardRequest {
                    target: ObjectRef {
                        context: None,
                        resource: crate::ResourceRef {
                            group: String::new(),
                            version: "v1".into(),
                            plural: "services".into(),
                        },
                        namespace: Some(namespace),
                        name,
                    },
                    remote_port: port,
                    local_port: None,
                    address: None,
                })
                .await?
            }
        };
        Ok(format!("{scheme}://127.0.0.1:{}", forward.local_port))
    }

    /// Log history of a pod, workload or Service from Loki.
    pub async fn loki_logs(&self, request: &loki::LokiRequest) -> Result<loki::LokiLogs> {
        let session = self.object_session(&request.target).await?;
        let endpoint = self
            .integrations(false)
            .await?
            .endpoint("loki")
            .cloned()
            .ok_or_else(|| {
                Error::Unsupported("no Loki found; give its URL on the integrations page".into())
            })?;
        let query = match request.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            Some(query) => query.to_owned(),
            None => self.loki_query(&session, request).await?,
        };
        let limit = request.limit.unwrap_or(loki::MAX_LINES).clamp(1, loki::MAX_LINES);
        let seconds = request.seconds.clamp(60, 30 * 24 * 3600);
        loki::query(&session.proxy_client, &endpoint, &query, seconds, limit).await
    }

    /// The LogQL that finds an object's lines: by pod name, or by a Service's selector.
    async fn loki_query(&self, session: &crate::Session, request: &loki::LokiRequest) -> Result<String> {
        let target = &request.target;
        let kind = session.info_for(&target.resource)?.kind.clone();
        let namespace = target.namespace.clone().unwrap_or_default();
        let selector = if target.resource.group.is_empty() && kind == "Service" {
            let api: Api<Service> = Api::namespaced(session.client.clone(), &namespace);
            api.get(&target.name).await?.spec.and_then(|s| s.selector)
        } else {
            None
        };
        if kind == "Service" && selector.as_ref().is_none_or(|s| s.is_empty()) {
            return Err(Error::Unsupported(format!(
                "Service {} selects no pods",
                target.name
            )));
        }
        Ok(loki::default_query(
            &kind,
            &namespace,
            &target.name,
            selector.as_ref(),
            request.container.as_deref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(
        namespace: &str,
        name: &str,
        labels: &[(&str, &str)],
        ports: &[(&str, i32)],
        headless: bool,
    ) -> Service {
        serde_json::from_value(serde_json::json!({
            "metadata": {
                "namespace": namespace,
                "name": name,
                "labels": labels.iter().map(|(k, v)| (k.to_string(), serde_json::Value::from(*v))).collect::<serde_json::Map<_, _>>(),
            },
            "spec": {
                "clusterIP": if headless { "None" } else { "10.43.0.10" },
                "ports": ports.iter().map(|(n, p)| serde_json::json!({ "name": n, "port": p })).collect::<Vec<_>>(),
            }
        }))
        .unwrap()
    }

    #[test]
    fn finds_kube_prometheus_stack() {
        let services = vec![
            service(
                "monitoring",
                "prometheus-operated",
                &[("app.kubernetes.io/name", "prometheus")],
                &[("web", 9090)],
                true,
            ),
            service(
                "monitoring",
                "kps-prometheus",
                &[("app", "kube-prometheus-stack-prometheus")],
                &[("http-web", 9090), ("reloader-web", 8080)],
                false,
            ),
            service(
                "monitoring",
                "kps-alertmanager",
                &[],
                &[("http-web", 9093)],
                false,
            ),
            service("monitoring", "kps-operator", &[], &[("https", 443)], false),
            service(
                "monitoring",
                "kps-prometheus-node-exporter",
                &[],
                &[("http-metrics", 9100)],
                false,
            ),
            service(
                "monitoring",
                "kps-grafana",
                &[("app.kubernetes.io/name", "grafana")],
                &[("http-web", 80)],
                false,
            ),
            service(
                "logging",
                "loki-memberlist",
                &[("app.kubernetes.io/name", "loki")],
                &[("tcp", 7946)],
                true,
            ),
            service(
                "logging",
                "loki",
                &[("app.kubernetes.io/name", "loki")],
                &[("http-metrics", 3100), ("grpc", 9095)],
                false,
            ),
            service(
                "logging",
                "loki-gateway",
                &[
                    ("app.kubernetes.io/name", "loki"),
                    ("app.kubernetes.io/component", "gateway"),
                ],
                &[("http-metrics", 80)],
                false,
            ),
        ];
        let svc = |namespace: &str, name: &str, port| Endpoint::Service {
            namespace: namespace.into(),
            name: name.into(),
            port,
            https: false,
        };
        assert_eq!(
            best(Tool::Prometheus, &services),
            Some(svc("monitoring", "kps-prometheus", 9090))
        );
        assert_eq!(
            best(Tool::Grafana, &services),
            Some(svc("monitoring", "kps-grafana", 80))
        );
        assert_eq!(
            best(Tool::Loki, &services),
            Some(svc("logging", "loki-gateway", 80))
        );
        assert_eq!(
            best(Tool::Loki, &services[..8]),
            Some(svc("logging", "loki", 3100))
        );
        // The prometheus chart.
        let chart = [service(
            "monitoring",
            "prometheus-server",
            &[],
            &[("http", 80)],
            false,
        )];
        assert_eq!(
            best(Tool::Prometheus, &chart),
            Some(svc("monitoring", "prometheus-server", 80))
        );
        assert_eq!(best(Tool::Grafana, &chart), None);
    }

    #[test]
    fn urls_naming_services_use_the_proxy() {
        assert_eq!(
            Endpoint::from_url("http://prometheus.monitoring.svc:9090").unwrap(),
            Endpoint::Service {
                namespace: "monitoring".into(),
                name: "prometheus".into(),
                port: 9090,
                https: false
            }
        );
        assert_eq!(
            Endpoint::from_url("https://grafana.ops.svc.cluster.local")
                .unwrap()
                .label(),
            "ops/grafana:443"
        );
        assert_eq!(
            Endpoint::from_url("https://grafana.example.com/").unwrap(),
            Endpoint::Url {
                url: "https://grafana.example.com".into()
            }
        );
        assert!(Endpoint::from_url("grafana.example.com").is_err());
        assert!(Endpoint::from_url("ftp://x").is_err());
        let settings = IntegrationSettings {
            prometheus: " OFF ".into(),
            grafana: "https://g.example.com/".into(),
            ..Default::default()
        }
        .normalized()
        .unwrap();
        assert_eq!(settings.prometheus, OFF);
        assert_eq!(settings.grafana, "https://g.example.com");
    }
}
