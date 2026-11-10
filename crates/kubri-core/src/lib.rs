//! Kubernetes access shared by the Kubri desktop app and the Kubri web server.
//!
//! [`Kubri`] owns the merged kubeconfig and the connection to the selected context.
//! Both frontends talk to the cluster only through it.

mod access;
mod added;
mod apply;
mod bench;
mod capacity;
mod certmanager;
mod chosen;
mod context;
mod debug;
pub mod diagnose;
mod discovery;
mod error;
pub mod explain;
mod fixes;
mod fleet;
mod gitops;
mod grafana;
mod graph;
mod guard;
mod health;
mod helm;
mod history;
mod integrations;
mod jsonpath;
mod k9s;
mod kubeconfig;
mod local_cluster;
mod local_terminal;
mod logs;
mod loki;
mod metrics;
mod node;
mod objects;
mod plugin;
mod portforward;
mod preview;
mod prometheus;
mod proxy;
mod pulse;
mod rbac;
mod recorder;
pub mod redact;
mod references;
mod resource;
mod rightsizing;
mod sanitizer;
mod schema;
mod search;
mod selector;
mod session;
mod settings;
mod snapshot;
mod table;
mod team;
mod template;
mod terminal;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
mod typed;
mod update;
mod views;
mod workload;
mod xray;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use kube::api::ListParams;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Config};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;

use session::Session;

pub use access::{AccessCheck, Identity};
pub use added::NewContext;
pub use apply::{ApplyItem, ApplyOutcome, ApplyRequest, fetch_manifest};
pub use bench::{
    BenchInfo, BenchRequest, BenchResults, BenchState, Bucket, ErrorCount, Percentile, StatusCount,
};
pub use capacity::{Amounts, Capacity, NodeRoom, QuotaRoom};
pub use context::{ContextInfo, write_in_cluster_kubeconfig};
pub use debug::{DebugRequest, DebugSession, DebugSettings};
pub use error::{Error, Result};
pub use explain::{ExplainAnswer, ExplainPrompt, ExplainSettings, ExplainSubject, Provider};
pub use fixes::{MemoryFix, NearMiss, NodeFit, PodMatch, ServiceCheck};
pub use fleet::FleetCluster;
pub use gitops::{GitOpsAction, Manager};
pub use grafana::{DashboardLink, Dashboards};
pub use guard::{Guard, UNLOCK_FOR};
pub use health::Health;
pub use helm::{HelmChange, HelmChartRef, HelmOutcome, HelmRelease, HelmReleaseDetails, HubChart};
pub use history::{
    ChangeKind, HistoryDetail, HistoryEntry, HistoryPage, HistoryQuery, HistorySettings, RecorderStatus, Span,
};
pub use integrations::{Endpoint, Integration, IntegrationSettings, Integrations};
pub use k9s::{
    Alias, ConfigFile, Hotkey, Jump, K9sConfig, Logger, Plugin, PluginInput, Threshold, Thresholds,
};
pub use local_cluster::{LocalClusterOption, LocalClusterRequest, LocalClusterTool, LocalClusters};
pub use local_terminal::{LocalTerminalInfo, LocalTerminalOptions, TerminalSettings};
pub use logs::{LogEvent, LogLine, LogOptions, LogSource};
pub use loki::{LokiLogs, LokiRequest};
pub use metrics::{Limits, MetricsKind, Usage, parse_quantity};
pub use node::{DrainOptions, DrainReport, PodIssue};
pub use objects::{DeleteOptions, EventInfo, ObjectRef, Propagation};
pub use plugin::{Cell, PluginRun, Release};
pub use portforward::{FavoriteStart, PortForwardInfo, PortForwardRequest};
pub use preview::{Change, Preview};
pub use prometheus::{Chart, ChartLevel, Charts, Marker, Range, Series};
pub use pulse::{
    CertificateAlert, EventPulse, KindPulse, Pulse, RecentEvent, Utilization, WaitingPod, Workload,
};
pub use rbac::{Grant, Permissions, Rbac, RoleState, Rule, SubjectInfo};
pub use references::Reference;
pub use resource::{ResourceInfo, ResourceRef};
pub use rightsizing::{ContainerResources, RightSizing, SizeAdvice, SizeReason};
pub use sanitizer::{Finding, Issue, Level, SanitizerReport, SanitizerSection};
pub use search::{FieldTest, SearchHit, SearchProblem, SearchRequest, SearchResult};
pub use settings::{
    AgentSettings, ColumnSettings, ConnectionSettings, ContextSettings, CustomColumn, DiffSettings,
    FavoriteForward, LabelTemplate, SavedSearch, Settings,
};
pub use snapshot::SnapshotInfo;
pub use update::{LatestRelease, latest_release};
pub use views::{RenderedView, View, ViewSet};

/// Kubri's config folder, where its settings live.
pub fn config_dir() -> Option<std::path::PathBuf> {
    settings::dir()
}
pub use chosen::KubeconfigFile;
pub use graph::{EdgeKind, Graph, GraphEdge, GraphNode, PathStep, RequestPath};
pub use table::{Column, Owner, Row, TableEvent, TableScope};
pub use team::{ActAs, Person, write_private};
pub use terminal::{Terminal, TerminalInput, TerminalMode, TerminalOptions, TerminalOutput};
pub use workload::{ContainerImage, Drill};
pub use xray::XrayNode;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a connection to another context is kept for reading.
const OTHER_SESSION_FOR: Duration = Duration::from_secs(5 * 60);
/// How long a context left behind stays connected when it is not used again.
const WARM_FOR: Duration = Duration::from_secs(30 * 60);
/// How long a kept connection may take to answer before Kubri connects afresh.
const WARM_CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// The cluster Kubri is currently connected to.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClusterInfo {
    pub context: String,
    pub server: String,
    pub version: String,
    /// The context's namespace, or `default`.
    pub default_namespace: String,
    /// Set when this is a snapshot opened from a file, not a live cluster.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub snapshot: Option<SnapshotInfo>,
    /// Set when a switch took a connection kept from earlier instead of connecting afresh.
    /// Its resource types may be out of date; [`Kubri::refresh_resources`] checks them.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub resumed: bool,
}

/// The context switcher's data: every context, and the one Kubri is connected to.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContextList {
    pub contexts: Vec<ContextInfo>,
    pub cluster: Option<ClusterInfo>,
    /// Contexts kept connected in the background, most recent first.
    pub warm: Vec<String>,
}

pub struct Kubri {
    /// Shared with the Kubris made for people by [`Kubri::for_person`].
    kubeconfig: Arc<RwLock<Result<Kubeconfig, String>>>,
    session: RwLock<Option<Arc<Session>>>,
    /// Counts switches and cancels: a connection only becomes the current one if nothing came
    /// after the switch that made it.
    switches: AtomicU64,
    forwards: std::sync::Mutex<portforward::Forwards>,
    benchmarks: std::sync::Mutex<bench::Benchmarks>,
    /// Started read-only, whatever the k9s configuration says.
    read_only: AtomicBool,
    /// Acting for an AI agent: changes need the context's consent in the settings.
    agent: AtomicBool,
    /// Shared with the Kubris made for people by [`Kubri::for_person`].
    settings: team::SharedSettings,
    /// Locked contexts unlocked until then, by name. Never saved: a restart locks them again.
    unlocks: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    /// Connections to other contexts, for reading objects there, and when they were made.
    others: tokio::sync::Mutex<HashMap<String, (std::time::Instant, Arc<Session>)>>,
    /// Contexts switched away from, most recent first, and when each was left, so switching
    /// back is instant. At most `connections.keep_recent` of them.
    warm: tokio::sync::Mutex<Vec<(std::time::Instant, Arc<Session>)>>,
    /// Clients the Clusters page checks contexts with, by context.
    fleet: std::sync::Mutex<HashMap<String, fleet::Kept>>,
    /// Records what changes in the connected context, where this process records.
    recording: AtomicBool,
    /// Local terminals may be opened, and contexts added in Kubri may run commands and read
    /// files here: in the desktop app, or a server started to allow local terminals.
    local_terminal: AtomicBool,
    recorder: std::sync::Mutex<Option<recorder::Recorder>>,
    history: std::sync::OnceLock<std::result::Result<Arc<history::Store>, String>>,
    /// The person this Kubri acts for, on a server shared by a team.
    act_as: Option<team::ActAs>,
}

/// The settings and where they are kept.
pub(crate) struct SettingsFile {
    /// `None` keeps them in memory only, as in tests.
    path: Option<std::path::PathBuf>,
    settings: Settings,
    /// Why the file could not be read; Kubri then uses the defaults and does not overwrite it.
    error: Option<String>,
}

/// What decides how Kubri connects to `context`: its context, cluster and user entries.
fn context_entry(kubeconfig: &Kubeconfig, context: &str) -> Option<Value> {
    let named = kubeconfig.contexts.iter().find(|c| c.name == context)?;
    let inner = named.context.as_ref();
    let cluster = inner.and_then(|c| kubeconfig.clusters.iter().find(|n| n.name == c.cluster));
    let user = inner
        .and_then(|c| c.user.as_ref())
        .and_then(|u| kubeconfig.auth_infos.iter().find(|n| &n.name == u));
    serde_json::to_value((named, cluster, user)).ok()
}

/// The client configuration for `context`, read the way kubectl reads it.
/// With `act_as`, the client acts for that person instead of as the kubeconfig's user.
pub(crate) async fn client_config(
    kubeconfig: &Kubeconfig,
    context: &str,
    act_as: Option<&team::ActAs>,
) -> Result<Config> {
    let mut kubeconfig = kubeconfig.clone();
    kubeconfig::prepare(&mut kubeconfig, context)?;
    let options = KubeConfigOptions {
        context: Some(context.to_owned()),
        ..Default::default()
    };
    let mut config = Config::from_custom_kubeconfig(kubeconfig.clone(), &options).await?;
    if let Some(act_as) = act_as {
        team::apply(&mut config, act_as);
    }
    config
        .headers
        .extend(kubeconfig::impersonation_headers(&config.auth_info)?);
    // The headers above carry these two as kubectl sends them; kube-rs would send them again.
    config.auth_info.impersonate_uid = None;
    config.auth_info.impersonate_user_extra = None;
    proxy::proxy_like_kubectl(&mut config, &kubeconfig, context);
    Ok(config)
}

/// Why an AI agent may not change something in a namespace marked as production.
pub(crate) const PRODUCTION_NAMESPACE: &str = "AI agents never change a production namespace";

impl Kubri {
    /// Loads the kubeconfig from `KUBECONFIG` (all listed files, merged, missing ones skipped)
    /// or `~/.kube/config`, as kubectl does.
    /// A missing or broken kubeconfig is not fatal: it is reported by [`Kubri::contexts`].
    pub fn load() -> Self {
        let kubri = Self::with_kubeconfig(kubeconfig::read());
        if let Some(path) = settings::path() {
            kubri.settings_file().path = Some(path);
            kubri.reload_settings();
        }
        kubri
    }

    /// Acts for an AI agent from now on: a context can only be changed where its settings
    /// allow agents to, and never a production context.
    pub fn set_agent(&self) {
        self.agent.store(true, Ordering::Relaxed);
    }

    pub fn from_kubeconfig(kubeconfig: Kubeconfig) -> Self {
        Self::with_kubeconfig(Ok(kubeconfig))
    }

    fn with_kubeconfig(kubeconfig: Result<Kubeconfig, String>) -> Self {
        let settings = SettingsFile {
            path: None,
            settings: Settings::default(),
            error: None,
        };
        Self::sharing(
            Arc::new(RwLock::new(kubeconfig)),
            Arc::new(std::sync::RwLock::new(settings)),
        )
    }

    fn sharing(kubeconfig: Arc<RwLock<Result<Kubeconfig, String>>>, settings: team::SharedSettings) -> Self {
        Self {
            kubeconfig,
            session: RwLock::new(None),
            switches: AtomicU64::new(0),
            forwards: Default::default(),
            benchmarks: Default::default(),
            read_only: AtomicBool::new(false),
            agent: AtomicBool::new(false),
            settings,
            unlocks: Default::default(),
            others: Default::default(),
            warm: Default::default(),
            fleet: Default::default(),
            recording: AtomicBool::new(false),
            local_terminal: AtomicBool::new(false),
            recorder: Default::default(),
            history: Default::default(),
            act_as: None,
        }
    }

    /// All resource types served by the connected cluster, sorted by kind.
    pub async fn resources(&self) -> Result<Vec<ResourceInfo>> {
        Ok(self.session().await?.resources.clone())
    }

    /// The namespaces to show: all of them where the user may list namespaces, otherwise the
    /// ones the kubeconfig and Kubri's settings name for this context.
    pub async fn readable_namespaces(&self) -> Result<Vec<String>> {
        let session = self.session().await?;
        if let Some(allowed) = self.allowed_namespaces(&session).await? {
            return Ok(allowed);
        }
        if let Some(namespaces) = session.resolve("v1", "Namespace") {
            let check = AccessCheck::new("list", &namespaces, None);
            if session.access_of(&check).await != Some(false) {
                let api: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(session.client.clone());
                match api.list(&ListParams::default()).await {
                    Ok(list) => {
                        let mut names: Vec<String> =
                            list.items.into_iter().filter_map(|n| n.metadata.name).collect();
                        names.sort();
                        return Ok(names);
                    }
                    Err(kube::Error::Api(status)) if status.code == 403 => {}
                    Err(err) => return Err(err.into()),
                }
            }
        }
        let settings = self.settings().context(&session.info.context);
        let mut names = vec![session.info.default_namespace.clone()];
        if !settings.namespace.is_empty() && settings.namespace != names[0] {
            names.push(settings.namespace);
        }
        Ok(names)
    }

    /// The session to act on `target` in; refuses a target picked in another context.
    async fn object_session(&self, target: &ObjectRef) -> Result<Arc<Session>> {
        let session = self.session().await?;
        match &target.context {
            Some(context) if context != &session.info.context => Err(Error::ContextChanged(context.clone())),
            _ => Ok(session),
        }
    }

    /// The namespaces the context's settings limit Kubri to, sorted; `None` when they do not.
    async fn allowed_namespaces(&self, session: &Session) -> Result<Option<Vec<String>>> {
        let settings = self.settings().context(&session.info.context);
        let mut names: Vec<String> = settings
            .namespaces
            .iter()
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .collect();
        let selector = settings.namespace_selector.trim();
        if names.is_empty() && !selector.is_empty() {
            let api: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(session.client.clone());
            let list = api.list(&ListParams::default().labels(selector)).await?;
            names = list.items.into_iter().filter_map(|n| n.metadata.name).collect();
        } else if names.is_empty() {
            return Ok(None);
        }
        names.sort();
        names.dedup();
        Ok(Some(names))
    }

    /// The connection to `context` for reading: the current one, or one kept for a few
    /// minutes after it was made.
    async fn read_session(&self, context: Option<&str>) -> Result<Arc<Session>> {
        let session = self.session().await?;
        let Some(context) = context.filter(|c| *c != session.info.context) else {
            return Ok(session);
        };
        if let Some((_, kept)) = self
            .warm
            .lock()
            .await
            .iter()
            .find(|(_, s)| s.info.context == context)
        {
            return Ok(kept.clone());
        }
        let mut others = self.others.lock().await;
        others.retain(|_, (made, _)| made.elapsed() < OTHER_SESSION_FOR);
        if let Some((_, other)) = others.get(context) {
            return Ok(other.clone());
        }
        let other = Arc::new(self.connect(context).await?);
        others.insert(context.to_owned(), (std::time::Instant::now(), other.clone()));
        Ok(other)
    }

    async fn session(&self) -> Result<Arc<Session>> {
        self.session.read().await.clone().ok_or(Error::NotConnected)
    }
}

impl Drop for Kubri {
    fn drop(&mut self) {
        if let Ok(forwards) = self.forwards.get_mut() {
            forwards.stop_all();
        }
        if let Ok(benchmarks) = self.benchmarks.get_mut() {
            benchmarks.stop_all();
        }
    }
}
