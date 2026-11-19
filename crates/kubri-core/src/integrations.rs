//! Tools many clusters run, found without setup: Prometheus, Grafana and Loki by their
//! Services, Flux, Argo CD and cert-manager by their API groups.
//!
//! Kubri reaches a Service through the API server's service proxy, with the kubeconfig's
//! credentials, so nothing needs to be exposed or port-forwarded. A URL in the context's
//! settings takes the place of what was found.

use std::sync::LazyLock;
use std::time::Duration;

use k8s_openapi::api::core::v1::{Pod, Service, ServicePort};
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::ListParams;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::prometheus::{self, Charts, Limits, Marker, Range, Subject};
use crate::{ChangeKind, Error, ObjectRef, ResourceInfo, Result};
use crate::{grafana, loki};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Where a tool is reached: the turned-off value in the settings, a URL, or a Service.
pub const OFF: &str = "off";

/// The URLs a context's settings give for its tools; empty finds them, `off` turns them off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct IntegrationSettings {
    pub prometheus: String,
    pub grafana: String,
    /// A Grafana service account token, sent to a Grafana given by URL.
    pub grafana_token: String,
    pub loki: String,
    /// The Loki data source in Grafana for logs in Explore, by UID or name; found when empty.
    pub grafana_logs: String,
    /// The label template whose names the logs use, by name; empty for the usual names.
    pub labels: String,
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
        self.grafana_logs = self.grafana_logs.trim().to_owned();
        self.labels = self.labels.trim().to_owned();
        Ok(self)
    }
}

/// How Kubri reaches a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", tag = "via")]
#[ts(export)]
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
    #[serde(rename_all = "camelCase")]
    Url {
        url: String,
        /// The Service the cluster's Ingress for the URL's host sends it to. Kubri asks it,
        /// through the API server, when the URL does not answer, such as when the Ingress wants
        /// a client certificate. The browser still opens the URL.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        behind: Option<Box<Endpoint>>,
    },
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
                behind: None,
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
            Endpoint::Url { url, .. } => url.clone(),
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
        self.fetch(client, path, token).await.map(|(text, _)| text)
    }

    /// Like `get`, and for a Service the scheme that answered when it was not the one expected:
    /// `Some(true)` for HTTPS, `Some(false)` for plain HTTP.
    pub(crate) async fn fetch(
        &self,
        client: &Client,
        path: &str,
        token: Option<&str>,
    ) -> Result<(String, Option<bool>)> {
        match self {
            Endpoint::Service { https, .. } => self
                .through_proxy_either(client, path, *https)
                .await
                .map_err(|failure| failure.error),
            Endpoint::Url { url, behind } => match direct(url, path, token).await {
                Ok(text) => Ok((text, None)),
                Err(err) => match behind.as_deref() {
                    Some(service @ Endpoint::Service { https, .. }) => service
                        .through_proxy_either(client, path, *https)
                        .await
                        .map(|(text, _)| (text, None))
                        // The URL's own error says best why it does not answer.
                        .map_err(|_| err),
                    _ => Err(err),
                },
            },
        }
    }

    /// GETs `path` of a Service through the API server's service proxy over HTTPS or not, and
    /// over the other scheme when the answer shows that the first was wrong: a server that
    /// speaks TLS answers plain HTTP with 400 ("Client sent an HTTP request to an HTTPS
    /// server"), and the API server cannot do a TLS handshake with one that speaks plain HTTP.
    /// With the scheme that answered when it was the other one.
    async fn through_proxy_either(
        &self,
        client: &Client,
        path: &str,
        https: bool,
    ) -> std::result::Result<(String, Option<bool>), ProxyFailure> {
        let first = match self.through_proxy(client, path, https).await {
            Ok(text) => return Ok((text, None)),
            Err(first) => first,
        };
        let wrong = if https {
            first.plain
        } else {
            first.status == Some(400)
        };
        if !wrong {
            return Err(first);
        }
        match self.through_proxy(client, path, !https).await {
            Ok(text) => Ok((text, Some(!https))),
            // It answered over the other scheme too, such as that it wants a client
            // certificate, which says more than the first answer.
            Err(again) if again.status.is_some() && !again.plain => Err(again),
            // A 400 of its own, from a server that speaks plain HTTP.
            Err(_) => Err(first),
        }
    }

    /// GETs `path` of a Service through the API server's service proxy.
    async fn through_proxy(
        &self,
        client: &Client,
        path: &str,
        https: bool,
    ) -> std::result::Result<String, ProxyFailure> {
        let fail = |status: Option<u16>, message: String| ProxyFailure {
            status,
            plain: false,
            error: Error::Integration(message),
        };
        let Endpoint::Service {
            namespace,
            name,
            port,
            ..
        } = self
        else {
            return Err(ProxyFailure {
                status: None,
                plain: false,
                error: Error::Unsupported(format!("{} is not a Service", self.label())),
            });
        };
        let uri = Self::proxy_path(namespace, name, *port, https, path);
        let request = http::Request::get(uri)
            .header(http::header::ACCEPT, "application/json")
            .body(Vec::new())
            .map_err(|e| ProxyFailure {
                status: None,
                plain: false,
                error: Error::Unsupported(e.to_string()),
            })?;
        let call = client.request_text(request);
        match tokio::time::timeout(TIMEOUT, call).await {
            Ok(Ok(text)) => Ok(text),
            Ok(Err(kube::Error::Api(status))) if status.message.contains("no endpoints available") => {
                Err(fail(None, format!("{} has no ready pod to answer", self.label())))
            }
            Ok(Err(kube::Error::Api(status))) if wants_certificate(&status.message) => {
                Err(fail(Some(status.code), no_certificate(&self.label())))
            }
            Ok(Err(kube::Error::Api(status))) => Err(ProxyFailure {
                plain: https && speaks_plain(&status.message),
                ..fail(
                    Some(status.code),
                    format!(
                        "{} answered {} through the API server: {}",
                        self.label(),
                        status.code,
                        short(&status.message)
                    ),
                )
            }),
            Ok(Err(err)) => Err(fail(None, format!("{}: {}", self.label(), Error::from(err)))),
            Err(_) => Err(fail(None, format!("{} did not answer in time", self.label()))),
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

/// GETs `path` of a URL, with `token` if there is one.
async fn direct(url: &str, path: &str, token: Option<&str>) -> Result<String> {
    let http = CLIENT.as_ref().map_err(|e| Error::Integration(e.clone()))?;
    let mut request = http
        .get(format!("{url}{path}"))
        .header("Accept", "application/json");
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        request = request.bearer_auth(token);
    }
    let failed = |e: reqwest::Error| {
        let message = reqwest_message(&e);
        Error::Integration(if wants_certificate(&message) {
            no_certificate(url)
        } else {
            format!("{url}: {message}")
        })
    };
    let response = request.send().await.map_err(failed)?;
    let status = response.status();
    let text = response.text().await.map_err(failed)?;
    if !status.is_success() {
        if wants_certificate(&text) {
            return Err(Error::Integration(no_certificate(url)));
        }
        return Err(Error::Integration(format!(
            "{url} answered {}: {}",
            status.as_u16(),
            short(&text)
        )));
    }
    Ok(text)
}

/// Whether an answer or error says that a client certificate was missing: nginx's "400 No
/// required SSL certificate was sent", or the TLS alert "certificate required".
fn wants_certificate(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("no required ssl certificate")
        || text.contains("certificaterequired")
        || text.contains("certificate required")
}

/// Why a tool behind a client certificate does not answer Kubri.
fn no_certificate(label: &str) -> String {
    format!(
        "{label} asks for a client certificate, which Kubri does not send; your browser can still open it"
    )
}

/// Why a GET through the API server's service proxy failed.
struct ProxyFailure {
    /// The status the API server answered with, if it answered.
    status: Option<u16>,
    /// The API server tried HTTPS, and the Service answered in plain HTTP.
    plain: bool,
    error: Error,
}

/// Whether the API server's error says that it tried TLS with a server that speaks plain HTTP:
/// "error trying to reach service: tls: first record does not look like a TLS handshake".
fn speaks_plain(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("first record does not look like a tls handshake")
        || message.contains("server gave http response to https client")
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
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Integration {
    /// `prometheus`, `grafana`, `loki`, `flux`, `argocd` or `certManager`.
    #[ts(type = "'prometheus' | 'grafana' | 'loki' | 'flux' | 'argocd' | 'certManager'")]
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

/// Whether a Service's port speaks TLS, by its name, its app protocol or its number.
fn speaks_tls(port: &ServicePort) -> bool {
    let name = port.name.as_deref().unwrap_or_default();
    let protocol = port
        .app_protocol
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    name == "https"
        || name == "tls"
        || name.starts_with("https-")
        || protocol == "https"
        || protocol.ends_with("/https")
        || matches!(port.port, 443 | 8443)
}

/// How well a Service fits a tool: higher is better, `None` is not it.
fn score(tool: Tool, service: &Service) -> Option<(i32, ServicePort)> {
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
            .filter(|p| u16::try_from(p.port).is_ok())
            .cloned()
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
                // kube-prometheus-stack names all of its Services after itself.
                "grafana",
                "kubelet",
                "coredns",
                "kube-proxy",
                "kube-etcd",
                "kube-scheduler",
                "kube-controller-manager",
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
            port: port.port as u16,
            https: speaks_tls(&port),
        })
}

/// The Service an Ingress of the cluster sends `url` to: the backend of the root path of the
/// rule for its host. Only for a URL without a path, as an Ingress may rewrite one.
fn behind_ingress(url: &str, ingresses: &[Ingress], services: &[Service]) -> Option<Endpoint> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if !matches!(parsed.path(), "" | "/") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    let fits = |rule: &str| {
        let rule = rule.to_ascii_lowercase();
        match rule.strip_prefix("*.") {
            // A wildcard stands for exactly one label.
            Some(domain) => host
                .split_once('.')
                .is_some_and(|(label, rest)| !label.is_empty() && rest == domain),
            None => rule == host,
        }
    };
    // An exact host wins over a wildcard, as in Ingress controllers.
    let mut found: Option<(bool, Endpoint)> = None;
    for ingress in ingresses {
        let namespace = ingress.metadata.namespace.clone().unwrap_or_default();
        let annotations = ingress.metadata.annotations.clone().unwrap_or_default();
        let backend_tls = annotations
            .get("nginx.ingress.kubernetes.io/backend-protocol")
            .is_some_and(|p| p.eq_ignore_ascii_case("https"))
            || annotations
                .get("traefik.ingress.kubernetes.io/service.serversscheme")
                .is_some_and(|p| p.eq_ignore_ascii_case("https"));
        let Some(spec) = ingress.spec.as_ref() else {
            continue;
        };
        for rule in spec.rules.iter().flatten() {
            let Some(rule_host) = rule.host.as_deref().filter(|h| fits(h)) else {
                continue;
            };
            let exact = !rule_host.starts_with("*.");
            if found.as_ref().is_some_and(|(was_exact, _)| *was_exact || !exact) {
                continue;
            }
            let backend = match rule.http.as_ref() {
                Some(http) => http
                    .paths
                    .iter()
                    .find(|p| matches!(p.path.as_deref(), None | Some("" | "/" | "/*")))
                    .and_then(|p| p.backend.service.clone()),
                None => spec.default_backend.as_ref().and_then(|b| b.service.clone()),
            };
            let Some(backend) = backend else {
                continue;
            };
            let service = services.iter().find(|s| {
                s.metadata.namespace.as_deref() == Some(namespace.as_str())
                    && s.metadata.name.as_deref() == Some(backend.name.as_str())
            });
            let wanted = backend.port.as_ref();
            let port = service
                .and_then(|s| s.spec.as_ref())
                .and_then(|spec| spec.ports.as_ref())
                .and_then(|ports| {
                    ports.iter().find(|p| match wanted {
                        Some(w) if w.number.is_some() => w.number == Some(p.port),
                        Some(w) => w.name.is_some() && w.name == p.name,
                        None => false,
                    })
                })
                .cloned()
                .or_else(|| {
                    // A Service Kubri may not read: the number in the Ingress is enough.
                    let number = wanted?.number?;
                    Some(ServicePort {
                        port: number,
                        ..Default::default()
                    })
                });
            let Some(port) = port.filter(|p| u16::try_from(p.port).is_ok()) else {
                continue;
            };
            found = Some((
                exact,
                Endpoint::Service {
                    namespace: namespace.clone(),
                    name: backend.name.clone(),
                    port: port.port as u16,
                    https: backend_tls || speaks_tls(&port),
                },
            ));
        }
    }
    found.map(|(_, endpoint)| endpoint)
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
    // The Ingresses only matter for a tool given by URL.
    let urls = [&settings.prometheus, &settings.grafana, &settings.loki]
        .into_iter()
        .any(|s| s.starts_with("http"));
    let ingresses = if urls {
        crate::typed::list(Api::<Ingress>::all(client.clone()))
            .await
            .unwrap_or_else(|err| {
                tracing::debug!("cannot list Ingresses to find the Services behind URLs: {err}");
                Vec::new()
            })
    } else {
        Vec::new()
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
            let endpoint = Endpoint::from_url(setting).ok().map(|endpoint| match endpoint {
                Endpoint::Url { url, .. } => {
                    let behind = behind_ingress(&url, &ingresses, &services).map(Box::new);
                    Endpoint::Url { url, behind }
                }
                service => service,
            });
            (endpoint, "settings")
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

/// Where a tool tells its version, which also shows that it answers.
fn version_path(id: &str) -> Option<&'static str> {
    match id {
        "prometheus" => Some("/api/v1/status/buildinfo"),
        "grafana" => Some("/api/health"),
        "loki" => Some("/loki/api/v1/status/buildinfo"),
        _ => None,
    }
}

/// Asks each tool Kubri talks to for its version, which also shows that it answers. A Service
/// that answers only over the other scheme than the one expected is marked so, for the
/// browser's port-forward too.
pub(crate) async fn probe(client: &Client, integrations: &mut Integrations, grafana_token: Option<&str>) {
    let checks = integrations.items.iter().map(|item| {
        let path = version_path(&item.id);
        let token = (item.id == "grafana").then_some(grafana_token).flatten();
        let endpoint = item.endpoint.clone();
        async move {
            let (endpoint, path) = endpoint.zip(path)?;
            Some(match endpoint.fetch(client, path, token).await {
                Ok((text, scheme)) => {
                    let answer: Value = serde_json::from_str(&text).unwrap_or_default();
                    let version = answer
                        .pointer("/data/version")
                        .or_else(|| answer.get("version"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    Ok((version, scheme))
                }
                Err(err) => Err(err.to_string()),
            })
        }
    });
    let answers = futures::future::join_all(checks).await;
    for (item, answer) in integrations.items.iter_mut().zip(answers) {
        match answer {
            Some(Ok((version, scheme))) => {
                item.version = version;
                if let (Some(tls), Some(Endpoint::Service { https, .. })) = (scheme, item.endpoint.as_mut()) {
                    *https = tls;
                }
            }
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
        self.recorded_markers(&session.info.context, &kind, &object, &range, &mut markers);
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

    /// Changes from the change history in `range`: for a workload or node its own, for a
    /// namespace those that start new pods. Rollouts already marked from ReplicaSets are not
    /// marked twice.
    fn recorded_markers(
        &self,
        context: &str,
        kind: &str,
        object: &Value,
        range: &Range,
        markers: &mut Vec<Marker>,
    ) {
        let Ok(store) = self.history_store() else {
            return;
        };
        let (start, end, _) = range.resolve();
        let text = |path: &str| object.pointer(path).and_then(Value::as_str).map(str::to_owned);
        let mut query = crate::HistoryQuery {
            context: Some(context.to_owned()),
            since: Some((start * 1000.0) as i64),
            until: Some((end * 1000.0) as i64),
            limit: Some(200),
            ..Default::default()
        };
        match kind {
            "Pod" => return,
            "Namespace" => {
                query.namespace = text("/metadata/name");
                query.changes = vec![
                    ChangeKind::Image,
                    ChangeKind::Restarted,
                    ChangeKind::Rollout,
                    ChangeKind::Scaled,
                    ChangeKind::Data,
                ];
            }
            _ => query.uid = text("/metadata/uid"),
        }
        let Ok((entries, _)) = store.query(context, &query) else {
            return;
        };
        let from_replica_sets = markers.clone();
        for entry in entries.iter().filter(|e| e.change != ChangeKind::Warning) {
            let at = entry.at as f64 / 1000.0;
            let starts_pods = matches!(
                entry.change,
                ChangeKind::Image | ChangeKind::Restarted | ChangeKind::Rollout
            );
            if starts_pods && from_replica_sets.iter().any(|m| (m.at - at).abs() < 120.0) {
                continue;
            }
            let label = if kind == "Namespace" {
                format!("{} {}: {}", entry.kind, entry.name, entry.summary)
            } else {
                entry.summary.clone()
            };
            markers.push(Marker { at, label });
        }
        markers.sort_by(|a, b| a.at.total_cmp(&b.at));
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

    /// The Grafana dashboards that show `target`, as links with its variables filled in: its
    /// logs in Explore first, then the dashboards Grafana lists, then a search of Grafana for
    /// the object's name.
    pub async fn dashboards(&self, target: &ObjectRef) -> Result<grafana::Dashboards> {
        let session = self.object_session(target).await?;
        let kind = session.info_for(&target.resource)?.kind.clone();
        let namespace = target.namespace.clone().unwrap_or_default();
        let subject = match (target.resource.group.as_str(), kind.as_str()) {
            ("", "Pod") => Some(grafana::Subject::Pod {
                namespace,
                name: target.name.clone(),
            }),
            ("", "Node") => Some(grafana::Subject::Node {
                name: target.name.clone(),
            }),
            ("", "Namespace") => Some(grafana::Subject::Namespace {
                name: target.name.clone(),
            }),
            ("apps", "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet")
            | ("batch", "Job" | "CronJob") => Some(grafana::Subject::Workload {
                kind,
                namespace,
                name: target.name.clone(),
            }),
            _ => None,
        };
        let found = self
            .integrations(false)
            .await
            .is_ok_and(|i| i.endpoint("grafana").is_some());
        if !found {
            return Err(Error::Unsupported(
                "no Grafana found; give its URL on the integrations page".into(),
            ));
        }
        let listed = async {
            match &subject {
                Some(_) => Some(self.grafana_dashboards(&session).await),
                None => None,
            }
        };
        let (logs, listed) = futures::join!(self.grafana_logs(target), listed);
        // Objects outside a namespace, like nodes, have no logs of their own.
        let mut links: Vec<_> = logs.ok().into_iter().collect();
        let mut note = None;
        if let (Some(subject), Some(listed)) = (subject, listed) {
            match listed {
                Ok((dashboards, why)) => {
                    let listed = grafana::links(&dashboards, &subject);
                    // Why Grafana's list could not be read matters only when nothing matched.
                    note = why.filter(|_| listed.is_empty());
                    links.extend(listed);
                }
                // Such as a Grafana behind a client certificate, which the browser can still open.
                Err(err) => note = Some(err.to_string()),
            }
        }
        links.push(grafana::search_link(&target.name));
        Ok(grafana::Dashboards { links, note })
    }

    /// The logs of `target` in Grafana's Explore, from Loki: those of a pod, a workload's pods,
    /// a Service's pods, a namespace, or for any other object those of its namespace.
    pub async fn grafana_logs(&self, target: &ObjectRef) -> Result<grafana::DashboardLink> {
        let session = self.object_session(target).await?;
        let kind = session.info_for(&target.resource)?.kind.clone();
        let integrations = self.integrations(false).await?;
        let endpoint = integrations.endpoint("grafana").cloned().ok_or_else(|| {
            Error::Unsupported("no Grafana found; give its URL on the integrations page".into())
        })?;
        let own = (target.resource.group.is_empty() && matches!(kind.as_str(), "Pod" | "Service"))
            || matches!(
                (target.resource.group.as_str(), kind.as_str()),
                ("apps", "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet")
                    | ("batch", "Job" | "CronJob")
            );
        let (title, query) = match target.namespace.as_deref().filter(|n| !n.is_empty()) {
            Some(_) if own => (
                format!("Logs of {} {}", kind.to_ascii_lowercase(), target.name),
                self.loki_query(&session, target, None).await?,
            ),
            Some(namespace) => (
                format!("Logs of namespace {namespace}"),
                loki::namespace_query(namespace, &self.loki_labels(&session.info.context)),
            ),
            None if target.resource.group.is_empty() && kind == "Namespace" => (
                format!("Logs of namespace {}", target.name),
                loki::namespace_query(&target.name, &self.loki_labels(&session.info.context)),
            ),
            None => {
                return Err(Error::Unsupported(format!(
                    "{kind} {} has no logs of its own; open a namespace, pod or workload",
                    target.name
                )));
            }
        };
        let source = self.grafana_loki_source(&session, &endpoint).await;
        Ok(grafana::DashboardLink {
            title,
            folder: "Explore".into(),
            path: grafana::explore_path(source.as_deref(), &query),
            filled: Vec::new(),
        })
    }

    /// The Loki data source to show logs from: the one in the settings, else the one Grafana
    /// says it has, looked up once; `None` leaves it to Grafana's default.
    async fn grafana_loki_source(&self, session: &crate::Session, endpoint: &Endpoint) -> Option<String> {
        let settings = self.settings().context(&session.info.context).integrations;
        if !settings.grafana_logs.is_empty() {
            return Some(settings.grafana_logs);
        }
        let mut cached = session.grafana_logs.lock().await;
        // Asked again once the Grafana or its token changes, as a token may be what it lacked.
        if let Some((asked, with, found)) = cached.as_ref()
            && asked == endpoint
            && *with == settings.grafana_token
        {
            return found.clone();
        }
        let token = Some(settings.grafana_token.as_str()).filter(|t| !t.is_empty());
        // A Grafana that needs a login or a client certificate answers no one but the browser.
        let found = tokio::time::timeout(
            Duration::from_secs(5),
            endpoint.get_json(&session.proxy_client, "/api/frontend/settings", token),
        )
        .await
        .ok()
        .and_then(|answer| answer.ok())
        .and_then(|settings| grafana::loki_source(&settings));
        *cached = Some((endpoint.clone(), settings.grafana_token.clone(), found.clone()));
        found
    }

    /// Where the browser reaches a tool: its URL, or for one on a Service a port-forward to
    /// it, started once and used again.
    pub async fn integration_url(&self, id: &str) -> Result<String> {
        let integrations = self.integrations(false).await?;
        let endpoint = integrations
            .endpoint(id)
            .cloned()
            .ok_or_else(|| Error::Unsupported(format!("{id} is not found in this cluster")))?;
        // A Service may answer only over the other scheme than the one expected, and the
        // browser needs the one it answers.
        let scheme = match (&endpoint, version_path(id)) {
            (Endpoint::Service { .. }, Some(path)) => {
                let session = self.session().await?;
                endpoint
                    .fetch(&session.proxy_client, path, None)
                    .await
                    .ok()
                    .and_then(|(_, scheme)| scheme)
            }
            _ => None,
        };
        let (namespace, name, port, https) = match endpoint {
            Endpoint::Url { url, .. } => return Ok(url),
            Endpoint::Service {
                namespace,
                name,
                port,
                https,
            } => (namespace, name, port, scheme.unwrap_or(https)),
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
            None => {
                self.loki_query(&session, &request.target, request.container.as_deref())
                    .await?
            }
        };
        let limit = request.limit.unwrap_or(loki::MAX_LINES).clamp(1, loki::MAX_LINES);
        let seconds = request.seconds.clamp(60, 30 * 24 * 3600);
        let labels = self.loki_labels(&session.info.context);
        loki::query(&session.proxy_client, &endpoint, &query, seconds, limit, &labels).await
    }

    /// The LogQL that finds an object's lines: by the names of its pods.
    async fn loki_query(
        &self,
        session: &crate::Session,
        target: &ObjectRef,
        container: Option<&str>,
    ) -> Result<String> {
        let kind = session.info_for(&target.resource)?.kind.clone();
        let namespace = target.namespace.clone().unwrap_or_default();
        let mut selected = None;
        if target.resource.group.is_empty() && kind == "Service" {
            let api: Api<Service> = Api::namespaced(session.client.clone(), &namespace);
            let selector = api.get(&target.name).await?.spec.and_then(|s| s.selector);
            let selector = selector
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::Unsupported(format!("Service {} selects no pods", target.name)))?;
            let labels: Vec<String> = selector.iter().map(|(k, v)| format!("{k}={v}")).collect();
            let pods: Api<Pod> = Api::namespaced(session.client.clone(), &namespace);
            let names: Vec<String> = pods
                .list_metadata(&ListParams::default().labels(&labels.join(",")))
                .await?
                .items
                .into_iter()
                .filter_map(|p| p.metadata.name)
                .collect();
            if names.is_empty() {
                return Err(Error::Unsupported(format!(
                    "Service {} selects no pods right now",
                    target.name
                )));
            }
            selected = Some(names);
        }
        Ok(loki::default_query(
            &kind,
            &namespace,
            &target.name,
            selected.as_deref(),
            container,
            &self.loki_labels(&session.info.context),
        ))
    }

    /// The label names the logs of `context` use: those of the template its settings pick.
    fn loki_labels(&self, context: &str) -> loki::Labels {
        let settings = self.settings();
        let picked = settings.context(context).integrations.labels;
        let template = settings.label_templates.iter().find(|t| t.name == picked);
        loki::Labels::from_template(template.filter(|_| !picked.is_empty()))
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
        // kube-prometheus-stack with its Prometheus turned off, as with Prometheus elsewhere.
        let without = [
            service(
                "monitoring",
                "kube-prometheus-stack-grafana",
                &[("app.kubernetes.io/name", "grafana")],
                &[("http-web", 80)],
                false,
            ),
            service(
                "kube-system",
                "kube-prometheus-stack-coredns",
                &[],
                &[("http-metrics", 9153)],
                true,
            ),
            service(
                "kube-system",
                "kube-prometheus-stack-kubelet",
                &[],
                &[("https-metrics", 10250), ("http-metrics", 10255)],
                true,
            ),
        ];
        assert_eq!(best(Tool::Prometheus, &without), None);
        assert_eq!(
            best(Tool::Grafana, &without),
            Some(svc("monitoring", "kube-prometheus-stack-grafana", 80))
        );
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
                url: "https://grafana.example.com".into(),
                behind: None,
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

    #[test]
    fn finds_services_that_speak_tls() {
        let grafana = |ports: &[(&str, i32)]| {
            [service(
                "ops",
                "grafana",
                &[("app.kubernetes.io/name", "grafana")],
                ports,
                false,
            )]
        };
        let https = |services: &[Service]| match best(Tool::Grafana, services) {
            Some(Endpoint::Service { https, port, .. }) => (port, https),
            other => panic!("{other:?}"),
        };
        assert_eq!(https(&grafana(&[("https", 443)])), (443, true));
        assert_eq!(https(&grafana(&[("web", 8443)])), (8443, true));
        assert_eq!(https(&grafana(&[("http-web", 80)])), (80, false));
        // Plain HTTP first, where the Service has both.
        assert_eq!(https(&grafana(&[("https", 443), ("http-web", 80)])), (80, false));
    }

    fn ingress(namespace: &str, host: &str, service: &str, port: Value, annotations: Value) -> Ingress {
        serde_json::from_value(serde_json::json!({
            "metadata": { "namespace": namespace, "name": service, "annotations": annotations },
            "spec": { "rules": [{ "host": host, "http": { "paths": [
                { "path": "/api", "pathType": "Prefix", "backend": { "service": { "name": "other", "port": { "number": 1 } } } },
                { "path": "/", "pathType": "Prefix", "backend": { "service": { "name": service, "port": port } } }
            ] } }] }
        }))
        .unwrap()
    }

    #[test]
    fn finds_the_service_behind_an_ingress() {
        let services = [service(
            "ops",
            "grafana",
            &[],
            &[("service", 80), ("tls", 3443)],
            false,
        )];
        let ingresses = [
            ingress(
                "ops",
                "*.example.com",
                "wild",
                serde_json::json!({ "number": 8080 }),
                serde_json::json!({}),
            ),
            ingress(
                "ops",
                "grafana.example.com",
                "grafana",
                serde_json::json!({ "name": "service" }),
                serde_json::json!({}),
            ),
        ];
        let svc = |name: &str, port, https| Endpoint::Service {
            namespace: "ops".into(),
            name: name.into(),
            port,
            https,
        };
        // The exact host wins over the wildcard, whichever comes first; a port by its name.
        assert_eq!(
            behind_ingress("https://Grafana.example.com", &ingresses, &services),
            Some(svc("grafana", 80, false))
        );
        // A Service Kubri cannot read still has the Ingress's port number.
        assert_eq!(
            behind_ingress("https://loki.example.com", &ingresses, &services),
            Some(svc("wild", 8080, false))
        );
        assert_eq!(
            behind_ingress("https://a.b.example.com", &ingresses, &services),
            None
        );
        assert_eq!(
            behind_ingress("https://grafana.example.com/grafana", &ingresses, &services),
            None
        );
        assert_eq!(
            behind_ingress("https://grafana.example.org", &ingresses, &services),
            None
        );
        let tls = [ingress(
            "ops",
            "grafana.example.com",
            "grafana",
            serde_json::json!({ "name": "tls" }),
            serde_json::json!({}),
        )];
        assert_eq!(
            behind_ingress("https://grafana.example.com", &tls, &services),
            Some(svc("grafana", 3443, true))
        );
        let annotated = [ingress(
            "ops",
            "grafana.example.com",
            "grafana",
            serde_json::json!({ "number": 80 }),
            serde_json::json!({ "nginx.ingress.kubernetes.io/backend-protocol": "HTTPS" }),
        )];
        assert_eq!(
            behind_ingress("https://grafana.example.com", &annotated, &services),
            Some(svc("grafana", 80, true))
        );
    }

    #[tokio::test]
    async fn asks_a_service_over_https_when_plain_http_fails() {
        let (client, log) = crate::testing::client(|r| {
            if r.uri.contains("/services/https:grafana:80/") {
                (200, r#"{"version":"11.3.0"}"#.into())
            } else {
                (400, "Client sent an HTTP request to an HTTPS server.".into())
            }
        });
        let endpoint = Endpoint::Service {
            namespace: "ops".into(),
            name: "grafana".into(),
            port: 80,
            https: false,
        };
        let (text, scheme) = endpoint.fetch(&client, "/api/health", None).await.unwrap();
        assert_eq!(scheme, Some(true));
        assert!(text.contains("11.3.0"));
        assert_eq!(log.lock().unwrap().len(), 2);

        let mut found = Integrations {
            items: vec![Integration {
                id: "grafana".into(),
                name: "Grafana".into(),
                endpoint: Some(endpoint),
                source: "found".into(),
                version: None,
                error: None,
            }],
        };
        probe(&client, &mut found, None).await;
        let item = &found.items[0];
        assert_eq!(item.version.as_deref(), Some("11.3.0"));
        assert!(matches!(
            item.endpoint,
            Some(Endpoint::Service { https: true, .. })
        ));
    }

    /// What the API server answers when it tries TLS with a Service that speaks plain HTTP.
    const NOT_TLS: &str = r#"{"kind":"Status","apiVersion":"v1","metadata":{},"status":"Failure","message":"error trying to reach service: tls: first record does not look like a TLS handshake","reason":"ServiceUnavailable","code":503}"#;

    #[tokio::test]
    async fn asks_a_service_over_plain_http_when_it_speaks_no_tls() {
        // Port 443 is taken for HTTPS, but the pod behind it speaks plain HTTP.
        let (client, log) = crate::testing::client(|r| {
            if r.uri.contains("/services/https:") {
                (503, NOT_TLS.into())
            } else {
                (200, r#"{"version":"11.3.0"}"#.into())
            }
        });
        let endpoint = Endpoint::Service {
            namespace: "ops".into(),
            name: "grafana".into(),
            port: 443,
            https: true,
        };
        let (text, scheme) = endpoint.fetch(&client, "/api/health", None).await.unwrap();
        assert_eq!(scheme, Some(false));
        assert!(text.contains("11.3.0"));
        assert_eq!(log.lock().unwrap().len(), 2);

        let mut found = Integrations {
            items: vec![Integration {
                id: "grafana".into(),
                name: "Grafana".into(),
                endpoint: Some(endpoint),
                source: "found".into(),
                version: None,
                error: None,
            }],
        };
        probe(&client, &mut found, None).await;
        assert!(matches!(
            found.items[0].endpoint,
            Some(Endpoint::Service { https: false, .. })
        ));
    }

    #[tokio::test]
    async fn keeps_the_400_of_a_service_that_speaks_plain_http() {
        // Prometheus refusing a query: the answer over HTTPS fails the handshake, so the
        // query's own error is the one that counts.
        let (client, log) = crate::testing::client(|r| {
            if r.uri.contains("/services/https:") {
                (503, NOT_TLS.into())
            } else {
                (
                    400,
                    r#"{"status":"error","errorType":"bad_data","error":"parse error"}"#.into(),
                )
            }
        });
        let endpoint = Endpoint::Service {
            namespace: "ops".into(),
            name: "prometheus".into(),
            port: 9090,
            https: false,
        };
        let err = endpoint
            .get(&client, "/api/v1/query?query=up(", None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("parse error"), "{err}");
        assert!(!err.to_string().contains("TLS"), "{err}");
        assert_eq!(log.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn says_when_a_client_certificate_is_wanted() {
        // A TLS sidecar that wants a client certificate, on a port taken for plain HTTP.
        let (client, log) = crate::testing::client(|r| {
            let body = if r.uri.contains("/services/https:") {
                "<html>\n<head><title>400 No required SSL certificate was sent</title></head>"
            } else {
                "<html>\n<head><title>400 The plain HTTP request was sent to HTTPS port</title></head>"
            };
            (400, body.into())
        });
        let endpoint = Endpoint::Service {
            namespace: "ops".into(),
            name: "grafana".into(),
            port: 80,
            https: false,
        };
        let err = endpoint.get(&client, "/api/health", None).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "ops/grafana:80 asks for a client certificate, which Kubri does not send; your browser can still open it"
        );
        assert_eq!(log.lock().unwrap().len(), 2);
    }

    /// A server that answers every request as nginx does without a client certificate.
    async fn nginx_wanting_a_certificate() -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buffer = [0u8; 4096];
                    let _ = socket.read(&mut buffer).await;
                    let body =
                        "<html><head><title>400 No required SSL certificate was sent</title></head></html>";
                    let answer = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(answer.as_bytes()).await;
                });
            }
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn asks_the_service_behind_the_ingress_when_the_url_wants_a_certificate() {
        let url = nginx_wanting_a_certificate().await;
        let (client, log) = crate::testing::client(|r| {
            assert!(
                r.uri
                    .starts_with("/api/v1/namespaces/ops/services/grafana:80/proxy/api/health")
            );
            (200, r#"{"version":"11.3.0"}"#.into())
        });
        let alone = Endpoint::Url {
            url: url.clone(),
            behind: None,
        };
        let err = alone.get(&client, "/api/health", None).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "{url} asks for a client certificate, which Kubri does not send; your browser can still open it"
            )
        );
        let behind = Endpoint::Url {
            url,
            behind: Some(Box::new(Endpoint::Service {
                namespace: "ops".into(),
                name: "grafana".into(),
                port: 80,
                https: false,
            })),
        };
        let text = behind.get(&client, "/api/health", None).await.unwrap();
        assert!(text.contains("11.3.0"));
        assert_eq!(log.lock().unwrap().len(), 1);
    }
}
