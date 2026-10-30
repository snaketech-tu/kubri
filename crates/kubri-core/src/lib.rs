//! Kubernetes access shared by the Kubri desktop app and the Kubri web server.
//!
//! [`Kubri`] owns the merged kubeconfig and the connection to the selected context.
//! Both frontends talk to the cluster only through it.

mod access;
mod added;
mod apply;
mod bench;
mod certmanager;
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
mod health;
mod helm;
mod history;
mod integrations;
mod jsonpath;
mod k9s;
mod kubeconfig;
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
mod settings;
mod snapshot;
mod table;
mod template;
mod terminal;
#[cfg(test)]
mod testing;
mod typed;
mod update;
mod views;
mod workload;
mod xray;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::{Stream, StreamExt};
use k8s_openapi::api::core::v1::{Event, Node, Pod, Service};
use kube::api::{DynamicObject, ListParams, Patch, PatchParams, PostParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;

pub use access::{AccessCheck, Identity};
pub use added::NewContext;
pub use apply::{ApplyItem, ApplyOutcome, ApplyRequest, fetch_manifest};
pub use bench::{
    BenchInfo, BenchRequest, BenchResults, BenchState, Bucket, ErrorCount, Percentile, StatusCount,
};
pub use context::{ContextInfo, write_in_cluster_kubeconfig};
pub use debug::{DebugRequest, DebugSession, DebugSettings};
pub use error::{Error, Result};
pub use explain::{ExplainAnswer, ExplainPrompt, ExplainSettings, ExplainSubject, Provider};
pub use fixes::{MemoryFix, NearMiss, NodeFit, PodMatch, ServiceCheck};
pub use fleet::FleetCluster;
pub use gitops::{GitOpsAction, Manager};
pub use grafana::{DashboardLink, Dashboards};
pub use health::Health;
pub use helm::{HelmChange, HelmChartRef, HelmOutcome, HelmRelease, HelmReleaseDetails, HubChart};
pub use history::{
    ChangeKind, HistoryDetail, HistoryEntry, HistoryPage, HistoryQuery, HistorySettings, RecorderStatus, Span,
};
pub use integrations::{Endpoint, Integration, IntegrationSettings, Integrations};
pub use k9s::{
    Alias, ConfigFile, Hotkey, Jump, K9sConfig, Logger, Plugin, PluginInput, Threshold, Thresholds,
};
pub use logs::{LogEvent, LogLine, LogOptions, LogSource};
pub use loki::{LokiLogs, LokiRequest};
pub use metrics::{Limits, MetricsKind, Usage, parse_quantity};
pub use node::{DrainOptions, DrainReport, PodIssue};
pub use objects::{DeleteOptions, EventInfo, ObjectRef, Propagation};
pub use plugin::{Cell, PluginRun, Release};
pub use portforward::{FavoriteStart, PortForwardInfo, PortForwardRequest};
pub use preview::{Change, Preview};
pub use prometheus::{Chart, ChartLevel, Charts, Marker, Range, Series};
pub use pulse::{CertificateAlert, EventPulse, KindPulse, Pulse, RecentEvent, Utilization};
pub use rbac::{Grant, Permissions, Rbac, RoleState, Rule, SubjectInfo};
pub use references::Reference;
pub use resource::{ResourceInfo, ResourceRef};
pub use rightsizing::{RightSizing, SizeAdvice, SizeReason};
pub use sanitizer::{Finding, Issue, Level, SanitizerReport, SanitizerSection};
pub use search::{FieldTest, SearchHit, SearchProblem, SearchRequest, SearchResult};
pub use settings::{
    AgentSettings, ColumnSettings, ContextSettings, CustomColumn, DiffSettings, FavoriteForward, SavedSearch,
    Settings,
};
pub use snapshot::SnapshotInfo;
pub use update::{LatestRelease, latest_release};
pub use views::{RenderedView, View, ViewSet};

/// Kubri's config folder, where its settings live.
pub fn config_dir() -> Option<std::path::PathBuf> {
    settings::dir()
}
pub use graph::{EdgeKind, Graph, GraphEdge, GraphNode, PathStep, RequestPath};
pub use table::{Column, Owner, Row, TableEvent, TableScope};
pub use terminal::{Terminal, TerminalInput, TerminalMode, TerminalOptions, TerminalOutput};
pub use workload::{ContainerImage, Drill};
pub use xray::XrayNode;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a connection to another context is kept for reading.
const OTHER_SESSION_FOR: Duration = Duration::from_secs(5 * 60);

/// The cluster Kubri is currently connected to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterInfo {
    pub context: String,
    pub server: String,
    pub version: String,
    /// The context's namespace, or `default`.
    pub default_namespace: String,
    /// Set when this is a snapshot opened from a file, not a live cluster.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<SnapshotInfo>,
}

/// The context switcher's data: every context, and the one Kubri is connected to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextList {
    pub contexts: Vec<ContextInfo>,
    pub cluster: Option<ClusterInfo>,
}

struct Session {
    info: ClusterInfo,
    client: Client,
    /// The same connection without retries, for the service proxy: its 503 means a tool
    /// has no ready pod, which retrying for half a minute does not change.
    proxy_client: Client,
    resources: Vec<ResourceInfo>,
    by_ref: HashMap<ResourceRef, usize>,
    access: access::AccessCache,
    /// OpenAPI schemas by kind, fetched once per connection.
    schemas: std::sync::Mutex<HashMap<ResourceRef, Arc<Option<Value>>>>,
    /// The tools found in the cluster, once looked for.
    integrations: tokio::sync::Mutex<Option<(IntegrationSettings, Integrations)>>,
    /// Grafana's dashboards, when they were read and why Grafana's own list was not.
    grafana: tokio::sync::Mutex<Option<grafana::Cached>>,
}

pub struct Kubri {
    kubeconfig: RwLock<Result<Kubeconfig, String>>,
    session: RwLock<Option<Arc<Session>>>,
    forwards: std::sync::Mutex<portforward::Forwards>,
    benchmarks: std::sync::Mutex<bench::Benchmarks>,
    /// Started read-only, whatever the k9s configuration says.
    read_only: AtomicBool,
    /// Acting for an AI agent: changes need the context's consent in the settings.
    agent: AtomicBool,
    settings: std::sync::RwLock<SettingsFile>,
    /// Locked contexts unlocked until then, by name. Never saved: a restart locks them again.
    unlocks: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    /// Connections to other contexts, for reading objects there, and when they were made.
    others: tokio::sync::Mutex<HashMap<String, (std::time::Instant, Arc<Session>)>>,
    /// Records what changes in the connected context, where this process records.
    recording: AtomicBool,
    recorder: std::sync::Mutex<Option<recorder::Recorder>>,
    history: std::sync::OnceLock<std::result::Result<Arc<history::Store>, String>>,
}

/// The settings and where they are kept.
struct SettingsFile {
    /// `None` keeps them in memory only, as in tests.
    path: Option<std::path::PathBuf>,
    settings: Settings,
    /// Why the file could not be read; Kubri then uses the defaults and does not overwrite it.
    error: Option<String>,
}

/// How long unlocking a locked context lasts.
pub const UNLOCK_FOR: Duration = Duration::from_secs(15 * 60);

/// Whether Kubri may change the connected cluster, and why not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Guard {
    pub read_only: bool,
    /// Why changes are off, when they are.
    pub reason: Option<String>,
    /// The context is locked and nothing else keeps it read-only, so it can be unlocked.
    pub can_unlock: bool,
    /// Seconds until an unlocked context locks again.
    pub unlocked_for: Option<u64>,
}

/// The client configuration for `context`, read the way kubectl reads it.
pub(crate) async fn client_config(kubeconfig: &Kubeconfig, context: &str) -> Result<Config> {
    let mut kubeconfig = kubeconfig.clone();
    kubeconfig::prepare(&mut kubeconfig, context)?;
    let options = KubeConfigOptions {
        context: Some(context.to_owned()),
        ..Default::default()
    };
    let mut config = Config::from_custom_kubeconfig(kubeconfig.clone(), &options).await?;
    config
        .headers
        .extend(kubeconfig::impersonation_headers(&config.auth_info)?);
    proxy::skip_proxy_like_kubectl(&mut config, &kubeconfig, context);
    Ok(config)
}

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

    /// Reads the settings file again, as when another Kubri process changed it.
    pub fn reload_settings(&self) {
        let mut file = self.settings_file();
        let Some(path) = file.path.clone() else {
            return;
        };
        match settings::read(&path) {
            Ok(settings) => {
                file.settings = settings;
                file.error = None;
            }
            Err(err) => {
                tracing::warn!("cannot read the settings: {err}");
                file.error = Some(err);
            }
        }
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
        Self {
            kubeconfig: RwLock::new(kubeconfig),
            session: RwLock::new(None),
            forwards: Default::default(),
            benchmarks: Default::default(),
            read_only: AtomicBool::new(false),
            agent: AtomicBool::new(false),
            settings: std::sync::RwLock::new(SettingsFile {
                path: None,
                settings: Settings::default(),
                error: None,
            }),
            unlocks: Default::default(),
            others: Default::default(),
            recording: AtomicBool::new(false),
            recorder: Default::default(),
            history: Default::default(),
        }
    }

    fn settings_file(&self) -> std::sync::RwLockWriteGuard<'_, SettingsFile> {
        self.settings.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Kubri's own settings.
    pub fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .settings
            .clone()
    }

    /// Why the settings file could not be read, if it could not.
    pub fn settings_error(&self) -> Option<String> {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .error
            .clone()
    }

    /// Changes the settings and saves them.
    fn update_settings(&self, change: impl FnOnce(&mut Settings) -> Result<()>) -> Result<Settings> {
        let mut file = self.settings_file();
        if let Some(err) = &file.error {
            return Err(Error::Unsupported(format!(
                "the settings file is broken, so Kubri does not overwrite it: {err}"
            )));
        }
        let mut settings = file.settings.clone();
        change(&mut settings)?;
        if let Some(path) = &file.path {
            settings::write(path, &settings)?;
        }
        file.settings = settings.clone();
        Ok(settings)
    }

    /// Sets what Kubri remembers about a context. Turning locking on ends an unlock.
    pub fn set_context_settings(&self, context: &str, value: ContextSettings) -> Result<Settings> {
        let lock = value.locked && !self.settings().context(context).locked;
        let settings = self.update_settings(|s| s.set_context(context, value))?;
        if lock {
            self.unlocks_map().remove(context);
        }
        Ok(settings)
    }

    /// Sets what AI agents may do.
    pub fn set_agent_settings(&self, value: AgentSettings) -> Result<Settings> {
        self.update_settings(|s| {
            s.agents = value;
            Ok(())
        })
    }

    /// Sets how lists show a kind, by `group/Kind`.
    pub fn set_column_settings(&self, kind: &str, value: ColumnSettings) -> Result<Settings> {
        self.update_settings(|s| s.set_columns(kind, value))
    }

    /// Replaces the searches kept in the sidebar.
    pub fn set_saved_searches(&self, value: Vec<SavedSearch>) -> Result<Settings> {
        let value: Vec<SavedSearch> = value
            .into_iter()
            .filter(|s| !s.name.trim().is_empty() && !s.query.trim().is_empty())
            .collect();
        self.update_settings(|s| {
            s.searches = value;
            Ok(())
        })
    }

    /// Sets which actions show a dry-run diff.
    pub fn set_diff_settings(&self, value: DiffSettings) -> Result<Settings> {
        self.update_settings(|s| {
            s.diff = value;
            Ok(())
        })
    }

    /// Sets what the change history keeps; recording starts or stops at once.
    pub async fn set_history_settings(&self, value: HistorySettings) -> Result<Settings> {
        let settings = self.update_settings(|s| {
            s.history = value;
            Ok(())
        })?;
        if let Ok(session) = self.session().await {
            self.restart_recorder(&session).await;
        }
        Ok(settings)
    }

    /// Records what changes in each context Kubri connects to from now on, as the desktop app
    /// and the web server do. Tools that only read, like the MCP server on its own, do not.
    pub fn record_changes(&self) {
        self.recording.store(true, Ordering::Relaxed);
    }

    /// The change history, opened on first use: `history.db` in Kubri's config folder, or a
    /// database in memory when there is no settings file, as in tests.
    fn history_store(&self) -> Result<Arc<history::Store>> {
        let path = self.settings_file().path.clone();
        self.history
            .get_or_init(|| {
                let store = match path.as_ref().and_then(|p| p.parent()) {
                    Some(dir) => history::Store::open(&dir.join("history.db")),
                    None => history::Store::in_memory(),
                };
                store.map(Arc::new).map_err(|e| e.to_string())
            })
            .clone()
            .map_err(Error::Unsupported)
    }

    /// Stops recording the previous context and starts recording `session`'s, if Kubri records.
    async fn restart_recorder(&self, session: &Session) {
        let settings = self.settings().history;
        let mut start = None;
        if self.recording.load(Ordering::Relaxed) && settings.record && session.info.snapshot.is_none() {
            match self.history_store() {
                Ok(store) => {
                    let namespaces = self.allowed_namespaces(session).await.unwrap_or_default();
                    start = Some((store, namespaces));
                }
                Err(err) => tracing::warn!("cannot record changes: {err}"),
            }
        }
        let mut recorder = self.recorder.lock().unwrap_or_else(|e| e.into_inner());
        // Stop first, so the new recorder can claim the context at once.
        *recorder = None;
        if let Some((store, namespaces)) = start {
            *recorder = Some(recorder::Recorder::start(
                store,
                session.client.clone(),
                &session.info.context,
                &session.resources,
                namespaces,
                settings,
            ));
        }
    }

    /// What changed in a context, newest first, with the times Kubri was watching it.
    pub async fn history(&self, query: &HistoryQuery) -> Result<HistoryPage> {
        let context = match &query.context {
            Some(context) => context.clone(),
            None => self.session().await?.info.context.clone(),
        };
        let store = self.history_store()?;
        let (entries, more) = store.query(&context, query)?;
        let watched = store.watched(&context, query.since, query.until)?;
        let snapshot = self
            .cluster()
            .await
            .is_some_and(|c| c.snapshot.is_some() && c.context == context);
        let recorder = self
            .recorder
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .filter(|r| r.context() == context)
            .map(|r| r.status())
            .unwrap_or_else(|| RecorderStatus {
                reason: Some(if snapshot {
                    "A snapshot does not change, so there is nothing to record".into()
                } else if !self.recording.load(Ordering::Relaxed) {
                    "This Kubri does not record changes".into()
                } else if !self.settings().history.record {
                    "Recording is turned off in the settings".into()
                } else {
                    "Kubri records only the context it is connected to".into()
                }),
                ..Default::default()
            });
        Ok(HistoryPage {
            context,
            entries,
            more,
            watched,
            recorder,
        })
    }

    /// One change with the object before and after it.
    pub fn history_detail(&self, id: i64) -> Result<HistoryDetail> {
        self.history_store()?
            .detail(id)?
            .ok_or_else(|| Error::NotFound(format!("change {id}")))
    }

    fn unlocks_map(&self) -> std::sync::MutexGuard<'_, HashMap<String, std::time::Instant>> {
        self.unlocks.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Seconds until `context` locks again, if it is unlocked now.
    fn unlocked_for(&self, context: &str) -> Option<u64> {
        let mut unlocks = self.unlocks_map();
        let until = *unlocks.get(context)?;
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            unlocks.remove(context);
            return None;
        }
        Some(left.as_secs().max(1))
    }

    /// Whether Kubri may change the connected cluster now, and why not.
    pub async fn guard(&self) -> Guard {
        let cluster = self.cluster().await;
        if cluster.as_ref().is_some_and(|c| c.snapshot.is_some()) {
            return Guard {
                read_only: true,
                reason: Some("this is a snapshot: it shows what was saved and cannot change anything".into()),
                ..Default::default()
            };
        }
        self.guard_for(cluster.map(|c| c.context).as_deref()).await
    }

    async fn guard_for(&self, context: Option<&str>) -> Guard {
        let fixed = if self.read_only.load(Ordering::Relaxed) {
            Some("it was started read-only")
        } else if self.k9s_files().await.read_only {
            Some("readOnly is set in the k9s configuration")
        } else {
            None
        };
        let settings = context.map(|c| self.settings().context(c)).unwrap_or_default();
        if let Some(reason) = fixed {
            return Guard {
                read_only: true,
                reason: Some(reason.into()),
                ..Default::default()
            };
        }
        if settings.read_only {
            return Guard {
                read_only: true,
                reason: Some("this context is set read-only in Kubri".into()),
                ..Default::default()
            };
        }
        if self.agent.load(Ordering::Relaxed) {
            let reason = if settings.production {
                Some("AI agents never change a production context")
            } else if !settings.agent_writes {
                Some("AI agents may not change this context; allow it in Kubri's settings")
            } else if settings.locked {
                Some("this context is locked; AI agents cannot unlock it")
            } else {
                None
            };
            return Guard {
                read_only: reason.is_some(),
                reason: reason.map(Into::into),
                ..Default::default()
            };
        }
        if !settings.locked {
            return Guard::default();
        }
        match context.and_then(|c| self.unlocked_for(c)) {
            Some(left) => Guard {
                can_unlock: true,
                unlocked_for: Some(left),
                ..Default::default()
            },
            None => Guard {
                read_only: true,
                reason: Some("this context is locked; unlock it to make changes".into()),
                can_unlock: true,
                unlocked_for: None,
            },
        }
    }

    /// Unlocks the connected context for [`UNLOCK_FOR`] if it is locked.
    pub async fn unlock(&self) -> Result<Guard> {
        let context = self.cluster().await.ok_or(Error::NotConnected)?.context;
        let guard = self.guard_for(Some(&context)).await;
        if !guard.can_unlock {
            return Err(Error::ReadOnly(
                guard
                    .reason
                    .unwrap_or_else(|| "this context is not locked".into()),
            ));
        }
        self.unlocks_map()
            .insert(context.clone(), std::time::Instant::now() + UNLOCK_FOR);
        tracing::info!(context, "unlocked for 15 minutes");
        Ok(self.guard_for(Some(&context)).await)
    }

    /// Locks the connected context again before its unlock runs out.
    pub async fn lock(&self) -> Result<Guard> {
        let context = self.cluster().await.ok_or(Error::NotConnected)?.context;
        self.unlocks_map().remove(&context);
        tracing::info!(context, "locked again");
        Ok(self.guard_for(Some(&context)).await)
    }

    /// Turns changes to clusters off for good, like k9s `--readonly`. The k9s `readOnly`
    /// setting can turn them off per context too; see [`K9sConfig::read_only`].
    pub fn set_read_only(&self) {
        self.read_only.store(true, Ordering::Relaxed);
    }

    /// Fails with [`Error::ReadOnly`] when changes are turned off. Port-forwards, logs and
    /// benchmarks only read, so they stay allowed, as in k9s.
    async fn ensure_writable(&self) -> Result<()> {
        match self.guard().await {
            Guard {
                read_only: true,
                reason,
                ..
            } => Err(Error::ReadOnly(reason.unwrap_or_default())),
            _ => Ok(()),
        }
    }

    /// Re-reads the kubeconfig from disk, keeping the current connection.
    pub async fn reload(&self) -> Result<ContextList> {
        *self.kubeconfig.write().await = kubeconfig::read();
        self.others.lock().await.clear();
        self.contexts().await
    }

    pub async fn contexts(&self) -> Result<ContextList> {
        let contexts = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => context::list_contexts(kubeconfig, &added::names()),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        Ok(ContextList {
            contexts,
            cluster: self.cluster().await,
        })
    }

    /// The contexts of a kubeconfig the user picked, and why any cannot be added.
    pub async fn inspect_kubeconfig(&self, text: &str) -> Result<Vec<NewContext>> {
        let new = added::parse(text)?;
        Ok(added::inspect(&new, &self.context_names().await))
    }

    /// Adds `names` from a kubeconfig the user picked, in Kubri's own folder.
    pub async fn add_contexts(&self, text: &str, names: &[String]) -> Result<ContextList> {
        let dir = added::dir()
            .ok_or_else(|| Error::Unsupported("Kubri has no config folder to keep contexts in".into()))?;
        let new = added::parse(text)?;
        added::add(&dir, &new, names, &self.context_names().await)?;
        self.reload().await
    }

    /// Removes a context added in Kubri. Contexts from the user's kubeconfig stay.
    pub async fn remove_context(&self, name: &str) -> Result<ContextList> {
        if self.cluster().await.is_some_and(|c| c.context == name) {
            return Err(Error::Unsupported(format!(
                "Kubri is connected to {name}; switch to another context first"
            )));
        }
        let removed = match added::dir() {
            Some(dir) => added::remove(&dir, name)?,
            None => false,
        };
        if !removed {
            return Err(Error::Unsupported(format!(
                "{name} is in your kubeconfig, which Kubri does not change"
            )));
        }
        self.reload().await
    }

    async fn context_names(&self) -> std::collections::HashSet<String> {
        match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => kubeconfig.contexts.iter().map(|c| c.name.clone()).collect(),
            Err(_) => Default::default(),
        }
    }

    pub async fn cluster(&self) -> Option<ClusterInfo> {
        self.session.read().await.as_ref().map(|s| s.info.clone())
    }

    /// Connects to `context` and discovers the resource types it serves.
    /// The previous connection stays active if this fails.
    pub async fn switch_context(&self, context: &str) -> Result<ClusterInfo> {
        let session = self.connect(context).await?;
        let info = session.info.clone();
        let session = Arc::new(session);
        *self.session.write().await = Some(session.clone());
        tracing::info!(context, version = %info.version, "connected");
        self.restart_recorder(&session).await;
        Ok(info)
    }

    /// Saves `namespace`, or the whole cluster, to a snapshot file: its name and gzipped bytes.
    pub async fn take_snapshot(&self, namespace: Option<&str>) -> Result<(String, Vec<u8>)> {
        let session = self.session().await?;
        if session.info.snapshot.is_some() {
            return Err(Error::Unsupported("this is a snapshot already".into()));
        }
        let bytes = snapshot::take(&session.client, &session.info, &session.resources, namespace).await?;
        let name = snapshot::file_name(
            &session.info.context,
            namespace,
            k8s_openapi::jiff::Timestamp::now().as_millisecond(),
        );
        tracing::info!(
            context = session.info.context,
            namespace,
            bytes = bytes.len(),
            "took a snapshot"
        );
        Ok((name, bytes))
    }

    /// Opens a snapshot file in place of a cluster, read-only. Switching to a context leaves it.
    pub async fn open_snapshot(&self, bytes: &[u8], file: Option<String>) -> Result<ClusterInfo> {
        let mut snapshot = snapshot::read(bytes)?;
        snapshot.info.file = file;
        let saved = snapshot.info.clone();
        let namespace = saved.namespace.clone().unwrap_or_else(|| "default".to_owned());
        let info = ClusterInfo {
            context: format!("{} (snapshot)", saved.context),
            server: saved.server.clone(),
            version: saved.kubernetes.clone(),
            default_namespace: namespace.clone(),
            snapshot: Some(saved),
        };
        let resources = snapshot.served_resources();
        let client = snapshot::client(snapshot, &namespace);
        let session = Session::new(info.clone(), client, resources);
        self.restart_recorder(&session).await;
        *self.session.write().await = Some(Arc::new(session));
        tracing::info!(context = info.context, "opened a snapshot");
        Ok(info)
    }

    /// A connection to `context`, without making it the current one.
    async fn connect(&self, context: &str) -> Result<Session> {
        let kubeconfig = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => kubeconfig.clone(),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        let named = kubeconfig
            .contexts
            .iter()
            .find(|c| c.name == context)
            .ok_or_else(|| Error::UnknownContext(context.to_owned()))?;
        let namespace = named
            .context
            .as_ref()
            .and_then(|c| c.namespace.clone())
            .unwrap_or_else(|| "default".to_owned());

        let session = tokio::time::timeout(CONNECT_TIMEOUT, async {
            let config = client_config(&kubeconfig, context).await?;
            let server = config.cluster_url.to_string().trim_end_matches('/').to_owned();
            let mut once = config.clone();
            once.default_retry = false;
            let proxy_client = Client::try_from(once)?;
            let client = Client::try_from(config)?;
            let version = client.apiserver_version().await?.git_version;
            let resources = discovery::discover(&client).await?;
            let mut session = Session::new(
                ClusterInfo {
                    context: context.to_owned(),
                    server,
                    version,
                    default_namespace: namespace,
                    snapshot: None,
                },
                client,
                resources,
            );
            session.proxy_client = proxy_client;
            Ok::<_, Error>(session)
        })
        .await
        .map_err(|_| Error::Timeout)??;
        Ok(session)
    }

    /// Whether the current user may do each of `checks`, like `kubectl auth can-i`. `None`
    /// where the API server could not say. `fresh` forgets earlier answers first, after a
    /// role may have changed.
    pub async fn access(&self, checks: &[AccessCheck], fresh: bool) -> Result<Vec<Option<bool>>> {
        if checks.len() > access::MAX_CHECKS {
            return Err(Error::Unsupported(format!(
                "at most {} checks at once",
                access::MAX_CHECKS
            )));
        }
        let session = self.session().await?;
        if fresh {
            session.access.clear();
        }
        Ok(access::check_all(&session.client, &session.access, checks).await)
    }

    /// The OpenAPI v3 schema of a kind: `{ root, schemas }`, with the schemas it refers to by
    /// name. `None` where the API server publishes none for it.
    pub async fn schema(&self, resource: &ResourceRef) -> Result<Arc<Option<Value>>> {
        let session = self.session().await?;
        if let Some(found) = session
            .schemas
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(resource)
        {
            return Ok(found.clone());
        }
        let info = session.info_for(resource)?;
        let document =
            schema::group_version_document(&session.client, &resource.group, &resource.version).await?;
        let found = Arc::new(schema::kind_schema(
            &document,
            &resource.group,
            &resource.version,
            &info.kind,
        ));
        session
            .schemas
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(resource.clone(), found.clone());
        Ok(found)
    }

    /// Who the cluster takes the current user for, like `kubectl auth whoami`.
    pub async fn who_am_i(&self) -> Result<Option<Identity>> {
        Ok(access::who_am_i(&self.session().await?.client).await)
    }

    /// All resource types served by the connected cluster, sorted by kind.
    pub async fn resources(&self) -> Result<Vec<ResourceInfo>> {
        Ok(self.session().await?.resources.clone())
    }

    /// Streams a resource list with the columns `kubectl get` shows. `namespace` is ignored
    /// for cluster-scoped resources, and `None` lists all namespaces.
    ///
    /// The stream is bound to the context that was active when it was created.
    pub async fn watch_table(
        &self,
        resource: &ResourceRef,
        namespace: Option<&str>,
        mut scope: TableScope,
    ) -> Result<impl Stream<Item = TableEvent> + Send + 'static> {
        let session = self.session().await?;
        let api = session.api(resource, namespace)?;
        let info = session.info_for(resource)?;
        scope.view = self.view_of(info).await.map(Arc::new);
        if namespace.is_none()
            && info.namespaced
            && let Some(allowed) = self.allowed_namespaces(&session).await?
        {
            if allowed.is_empty() {
                return Err(Error::Unsupported(
                    "no namespace matches the label selector in this context's settings".into(),
                ));
            }
            let shown = match allowed.len() {
                0..=4 => allowed.join(", "),
                n => format!("{} and {} more", allowed[..3].join(", "), n - 3),
            };
            let notice = format!("Showing the namespaces this context's settings name: {shown}");
            let urls = allowed
                .iter()
                .map(|ns| Ok(session.api(resource, Some(ns))?.resource_url().to_owned()))
                .collect::<Result<Vec<_>>>()?;
            return Ok(table::watch_tables(session.client.clone(), urls, scope, notice).boxed());
        }
        if namespace.is_none() && info.namespaced {
            // Someone who may not list all namespaces at once sees the namespaces they may read.
            let everywhere = AccessCheck::new("list", resource, None);
            if session.access_of(&everywhere).await == Some(false) {
                let mut readable = Vec::new();
                for ns in self.readable_namespaces().await? {
                    let here = AccessCheck::new("list", resource, Some(&ns));
                    if session.access_of(&here).await != Some(false) {
                        readable.push(ns);
                    }
                }
                if !readable.is_empty() {
                    let notice = format!(
                        "You may not list {} in all namespaces; showing {}",
                        info.reference.plural,
                        readable.join(", ")
                    );
                    let urls = readable
                        .iter()
                        .map(|ns| Ok(session.api(resource, Some(ns))?.resource_url().to_owned()))
                        .collect::<Result<Vec<_>>>()?;
                    return Ok(table::watch_tables(session.client.clone(), urls, scope, notice).boxed());
                }
            }
        }
        Ok(table::watch_table(session.client.clone(), api.resource_url().to_owned(), scope).boxed())
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

    /// Fetches one object in full.
    pub async fn get(&self, obj: &ObjectRef) -> Result<Value> {
        let api = self
            .object_session(obj)
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        Ok(serde_json::to_value(api.get(&obj.name).await?)?)
    }

    /// Fetches one object from its own context, also one Kubri is not connected to, as Compare
    /// does. [`Kubri::get`] refuses those, so a view left from the last context cannot show
    /// another cluster's object as this one's.
    pub async fn get_elsewhere(&self, obj: &ObjectRef) -> Result<Value> {
        let api = self
            .read_session(obj.context.as_deref())
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        Ok(serde_json::to_value(api.get(&obj.name).await?)?)
    }

    /// Replaces an object, as `kubectl edit` does. Fails with a conflict if it changed since
    /// `object` was read.
    pub async fn replace(&self, obj: &ObjectRef, object: Value) -> Result<Value> {
        self.ensure_writable().await?;
        let session = self.object_session(obj).await?;
        session.ensure_may("update", obj, "").await?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let object: DynamicObject = serde_json::from_value(object)?;
        let saved = api.replace(&obj.name, &PostParams::default(), &object).await?;
        Ok(serde_json::to_value(saved)?)
    }

    /// Runs `change` as a server-side dry run (`dryRun=All`) and returns the object as it is
    /// and as it would be, for a diff. Nothing changes in the cluster.
    pub async fn preview(&self, obj: &ObjectRef, change: Change) -> Result<Preview> {
        let session = self.object_session(obj).await?;
        let info = session.info_for(&obj.resource)?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let dry = PatchParams {
            dry_run: true,
            ..Default::default()
        };
        let (before, after) = match change {
            Change::Replace { value } => {
                let before = api.get(&obj.name).await?;
                let object: DynamicObject = serde_json::from_value(value)?;
                let post = PostParams {
                    dry_run: true,
                    ..Default::default()
                };
                (before, api.replace(&obj.name, &post, &object).await?)
            }
            Change::Patch { patch } => (
                api.get(&obj.name).await?,
                api.patch(&obj.name, &dry, &Patch::Merge(patch)).await?,
            ),
            Change::Restart => (
                api.get(&obj.name).await?,
                api.patch(
                    &obj.name,
                    &dry,
                    &Patch::Merge(objects::restart_patch(
                        &k8s_openapi::jiff::Timestamp::now().to_string(),
                    )),
                )
                .await?,
            ),
            Change::Images { images } => {
                let patch = workload::image_patch(info.kind == "Pod", &images)?;
                (
                    api.get(&obj.name).await?,
                    api.patch(&obj.name, &dry, &Patch::Strategic(patch)).await?,
                )
            }
            Change::Resources {
                container,
                requests,
                limits,
            } => {
                let patch = rightsizing::resources_patch(&container, &requests, &limits)?;
                (
                    api.get(&obj.name).await?,
                    api.patch(&obj.name, &dry, &Patch::Strategic(patch)).await?,
                )
            }
            Change::Memory { container, memory } => {
                let patch = fixes::memory_patch(&container, &memory)?;
                (
                    api.get(&obj.name).await?,
                    api.patch(&obj.name, &dry, &Patch::Strategic(patch)).await?,
                )
            }
            Change::Scale { replicas } => {
                if !info.subresources.iter().any(|s| s == "scale") {
                    return Err(Error::Unsupported(format!("{} cannot be scaled", info.kind)));
                }
                let patch = serde_json::json!({ "spec": { "replicas": replicas } });
                let before = serde_json::to_value(api.get_scale(&obj.name).await?)?;
                let after =
                    serde_json::to_value(api.patch_scale(&obj.name, &dry, &Patch::Merge(patch)).await?)?;
                return Ok(Preview {
                    before: preview::comparable(before),
                    after: preview::comparable(after),
                });
            }
        };
        Ok(Preview {
            before: preview::comparable(serde_json::to_value(before)?),
            after: preview::comparable(serde_json::to_value(after)?),
        })
    }

    /// Applies a JSON merge patch, for example to change labels or annotations.
    pub async fn patch(&self, obj: &ObjectRef, patch: Value) -> Result<Value> {
        self.ensure_writable().await?;
        let session = self.object_session(obj).await?;
        session.ensure_may("patch", obj, "").await?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let saved = api
            .patch(&obj.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(serde_json::to_value(saved)?)
    }

    pub async fn delete(&self, obj: &ObjectRef, options: &DeleteOptions) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(obj).await?;
        session.ensure_may("delete", obj, "").await?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        api.delete(&obj.name, &options.to_params()).await?;
        Ok(())
    }

    /// Sets the replica count through the `scale` subresource, so CRDs that support it work too.
    pub async fn scale(&self, obj: &ObjectRef, replicas: u32) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(obj).await?;
        let info = session.info_for(&obj.resource)?;
        if !info.subresources.iter().any(|s| s == "scale") {
            return Err(Error::Unsupported(format!("{} cannot be scaled", info.kind)));
        }
        session.ensure_may("patch", obj, "scale").await?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let patch = serde_json::json!({ "spec": { "replicas": replicas } });
        api.patch_scale(&obj.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(())
    }

    /// Restarts the pods of a Deployment, StatefulSet or DaemonSet, like
    /// `kubectl rollout restart`.
    pub async fn restart(&self, obj: &ObjectRef) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(obj).await?;
        let info = session.info_for(&obj.resource)?;
        if !matches!(info.kind.as_str(), "Deployment" | "StatefulSet" | "DaemonSet") {
            return Err(Error::Unsupported(format!("{} cannot be restarted", info.kind)));
        }
        session.ensure_may("patch", obj, "").await?;
        let now = k8s_openapi::jiff::Timestamp::now().to_string();
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        api.patch(
            &obj.name,
            &PatchParams::default(),
            &Patch::Merge(objects::restart_patch(&now)),
        )
        .await?;
        Ok(())
    }

    /// Where Enter leads from an object: the rule for its resource in `jumps.yaml`, or the
    /// pods of a workload, Service or node, or the Jobs of a CronJob. `None` for the rest,
    /// which Enter describes.
    pub async fn drill(&self, target: &ObjectRef) -> Result<Option<Drill>> {
        let session = self.object_session(target).await?;
        let info = session.info_for(&target.resource)?;
        let config = self.k9s_config().await;
        let rule = workload::find_jump(&config.jumps, &info.reference);
        let builtin = workload::Builtin::of(info);
        if rule.is_none() && builtin.is_none() {
            return Ok(None);
        }
        let object = serde_json::to_value(
            session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?,
        )?;
        if let Some(rule) = rule {
            let resolve = |path: &str| session.find_path(path).cloned();
            return workload::jump(rule, info, target, &object, resolve).map(Some);
        }
        let pods = session.resolve("v1", "Pod");
        let jobs = session.resolve("batch/v1", "Job");
        builtin
            .map(|b| b.drill(info, target, &object, pods, jobs))
            .transpose()
    }

    /// Runs a CronJob now: creates a Job from its template, like `kubectl create job
    /// --from=cronjob/…`. Returns the Job's name.
    pub async fn trigger_cronjob(&self, target: &ObjectRef) -> Result<String> {
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        if session.info_for(&target.resource)?.kind != "CronJob" {
            return Err(Error::Unsupported("only CronJobs can be triggered".into()));
        }
        if let Some(jobs) = session.resolve("batch/v1", "Job") {
            session
                .ensure_allowed(AccessCheck::new("create", &jobs, target.namespace.as_deref()))
                .await?;
        }
        let cronjob = serde_json::to_value(
            session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?,
        )?;
        let job: DynamicObject = serde_json::from_value(workload::job_from_cronjob(&cronjob)?)?;
        let jobs = session
            .resolve("batch/v1", "Job")
            .ok_or_else(|| Error::UnknownResource("batch/v1/jobs".into()))?;
        let created = session
            .api(&jobs, target.namespace.as_deref())?
            .create(&PostParams::default(), &job)
            .await?;
        Ok(created.metadata.name.unwrap_or_default())
    }

    /// Suspends a CronJob, so it starts no more Jobs, or resumes it.
    pub async fn suspend_cronjob(&self, target: &ObjectRef, suspend: bool) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        if session.info_for(&target.resource)?.kind != "CronJob" {
            return Err(Error::Unsupported("only CronJobs can be suspended".into()));
        }
        session.ensure_may("patch", target, "").await?;
        let patch = serde_json::json!({ "spec": { "suspend": suspend } });
        session
            .api(&target.resource, target.namespace.as_deref())?
            .patch(&target.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(())
    }

    /// What uses a ConfigMap, Secret, PersistentVolumeClaim, ServiceAccount or
    /// PriorityClass, like k9s `u`: the workloads, CronJobs and bare pods whose pod spec names
    /// it, in its namespace (in all of them for a PriorityClass).
    pub async fn used_by(&self, target: &ObjectRef) -> Result<Vec<Reference>> {
        let session = self.object_session(target).await?;
        let info = session.info_for(&target.resource)?;
        let used = references::Used::of(info)
            .ok_or_else(|| Error::Unsupported(format!("Kubri does not look up what uses a {}", info.kind)))?;
        let users = used.users();
        let lists = users.iter().filter_map(|&(version, kind)| {
            let resource = session.resolve(version, kind)?;
            let api = session.api(&resource, target.namespace.as_deref()).ok()?;
            Some(async move { (kind, resource, api.list(&ListParams::default()).await) })
        });
        let mut found = Vec::new();
        for (kind, resource, list) in futures::future::join_all(lists).await {
            let list = match list {
                Ok(list) => list,
                // Extras: a user who may not list them still sees the rest.
                Err(_) if kind == "HTTPRoute" || (kind == "Ingress" && used == references::Used::Secret) => {
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            for object in list.items {
                let value = serde_json::to_value(&object)?;
                if used.by(kind, &value, &target.name) {
                    found.push(Reference {
                        resource: resource.clone(),
                        kind: kind.to_owned(),
                        namespace: object.metadata.namespace.clone(),
                        name: object.metadata.name.clone().unwrap_or_default(),
                        how: used.how(kind).map(str::to_owned),
                    });
                }
            }
        }
        Ok(found)
    }

    /// Sets container images of a pod, Deployment, StatefulSet or DaemonSet, like
    /// `kubectl set image`.
    pub async fn set_images(&self, target: &ObjectRef, images: &[ContainerImage]) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        let info = session.info_for(&target.resource)?;
        if !workload::has_images(info) {
            return Err(Error::Unsupported(format!("{} has no images to set", info.kind)));
        }
        session.ensure_may("patch", target, "").await?;
        let patch = workload::image_patch(info.kind == "Pod", images)?;
        session
            .api(&target.resource, target.namespace.as_deref())?
            .patch(&target.name, &PatchParams::default(), &Patch::Strategic(patch))
            .await?;
        Ok(())
    }

    /// Rolls the Deployment of a ReplicaSet back to the ReplicaSet's pod template, like
    /// `kubectl rollout undo --to-revision`. Returns the Deployment's name.
    pub async fn rollback(&self, target: &ObjectRef) -> Result<String> {
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        let info = session.info_for(&target.resource)?;
        if (info.reference.group.as_str(), info.kind.as_str()) != ("apps", "ReplicaSet") {
            return Err(Error::Unsupported("rollbacks start from a ReplicaSet".into()));
        }
        let replicaset = serde_json::to_value(
            session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?,
        )?;
        let name = workload::deployment_of(&replicaset)?;
        let deployments = session
            .resolve("apps/v1", "Deployment")
            .ok_or_else(|| Error::UnknownResource("apps/v1/deployments".into()))?;
        let deployment_ref = ObjectRef {
            context: None,
            resource: deployments.clone(),
            namespace: target.namespace.clone(),
            name: name.clone(),
        };
        session.ensure_may("update", &deployment_ref, "").await?;
        let api = session.api(&deployments, target.namespace.as_deref())?;
        let deployment = serde_json::to_value(api.get(&name).await?)?;
        let updated: DynamicObject = serde_json::from_value(workload::rolled_back(deployment, &replicaset)?)?;
        // Fails with a conflict if the Deployment changed since it was read.
        api.replace(&name, &PostParams::default(), &updated).await?;
        Ok(name)
    }

    /// Events about the object with `uid`, newest first.
    pub async fn events(&self, namespace: Option<&str>, uid: &str) -> Result<Vec<EventInfo>> {
        let session = self.session().await?;
        let api: Api<Event> = match namespace {
            Some(ns) => Api::namespaced(session.client.clone(), ns),
            None => Api::all(session.client.clone()),
        };
        let params = ListParams::default().fields(&format!("involvedObject.uid={uid}"));
        let mut events: Vec<EventInfo> = api
            .list(&params)
            .await?
            .items
            .into_iter()
            .map(EventInfo::from_event)
            .collect();
        events.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        Ok(events)
    }

    /// Current CPU and memory usage from metrics-server, with the requests and limits of pods
    /// and what nodes can give. Fails with [`Error::UnknownResource`] when metrics-server is
    /// not installed.
    pub async fn metrics(&self, kind: MetricsKind, namespace: Option<&str>) -> Result<Vec<Usage>> {
        let session = self.session().await?;
        let client = session.client.clone();
        let mut usage = session.metrics(kind, namespace).await?;
        // Without them the percentages stay empty; the usage itself is still worth showing.
        match kind {
            MetricsKind::Pods => {
                let api: Api<Pod> = match namespace {
                    Some(ns) => Api::namespaced(client, ns),
                    None => Api::all(client),
                };
                if let Ok(pods) = typed::list(api).await {
                    metrics::add_pod_limits(&mut usage, &pods);
                }
            }
            MetricsKind::Nodes => {
                if let Ok(nodes) = typed::list(Api::<Node>::all(client)).await {
                    metrics::add_node_allocatable(&mut usage, &nodes);
                }
            }
        }
        Ok(usage)
    }

    /// The cluster overview: health counts per kind, recent warnings and CPU and memory use.
    /// `None` covers all namespaces; nodes and persistent volumes are always cluster-wide.
    pub async fn pulse(&self, namespace: Option<&str>) -> Result<Pulse> {
        let session = self.session().await?;
        // Usage of the whole cluster comes from the nodes, of one namespace from its pods.
        let metrics = match namespace {
            Some(_) => session.metrics(MetricsKind::Pods, namespace),
            None => session.metrics(MetricsKind::Nodes, None),
        };
        let resolve = |api_version: &str, kind: &str| session.resolve(api_version, kind);
        Ok(pulse::gather(session.client.clone(), namespace, metrics, resolve, session.now()).await)
    }

    /// The XRay tree of every object of `resource` in `namespace`, or in all namespaces.
    pub async fn xray(&self, resource: &ResourceRef, namespace: Option<&str>) -> Result<Vec<XrayNode>> {
        let session = self.session().await?;
        let info = session.info_for(resource)?;
        let root = xray::Root::of(&resource.group, &info.kind).ok_or_else(|| {
            Error::Unsupported(format!(
                "XRay cannot show {}; it supports {}",
                info.kind,
                xray::Root::SUPPORTED
            ))
        })?;
        xray::gather(session.client.clone(), root, namespace).await
    }

    /// The objects of `namespace`, or of all namespaces, as a graph of how they connect, with
    /// the request path of every Ingress rule.
    pub async fn graph(&self, namespace: Option<&str>) -> Result<Graph> {
        let session = self.session().await?;
        let mut graph = graph::gather(session.client.clone(), namespace).await?;
        let views = self.views().await.views;
        graph::add_views(&mut graph, &session.client, &session.resources, &views, namespace).await;
        Ok(graph)
    }

    /// The views in use: Kubri's own and those in the config folder, read again each time so
    /// that edits apply at once.
    pub async fn views(&self) -> ViewSet {
        let dir = views::dir();
        tokio::task::spawn_blocking(move || views::load(dir.as_deref()))
            .await
            .unwrap_or_default()
    }

    async fn view_of(&self, info: &ResourceInfo) -> Option<views::View> {
        let views = self.views().await.views;
        views
            .into_iter()
            .find(|v| v.is_for(&info.reference.group, &info.kind))
    }

    /// The health and detail sections the kind's view gives an object; `None` without a view.
    pub async fn render_view(&self, resource: &ResourceRef, object: &Value) -> Result<Option<RenderedView>> {
        let session = self.session().await?;
        let info = session.info_for(resource)?;
        Ok(self.view_of(info).await.map(|v| v.render(object)))
    }

    /// Checks the objects of `namespace`, or of the whole cluster with nodes, volumes and
    /// cluster roles, for problems and risky settings, like Popeye.
    pub async fn sanitize(&self, namespace: Option<&str>) -> Result<SanitizerReport> {
        let session = self.session().await?;
        let usage = session.metrics(MetricsKind::Nodes, None);
        Ok(sanitizer::scan(session.client.clone(), namespace, usage, session.now()).await)
    }

    /// Every subject of a role binding with the rules it is granted. `namespace` limits role
    /// bindings to one namespace; cluster role bindings are always included.
    pub async fn rbac(&self, namespace: Option<&str>) -> Result<Rbac> {
        let session = self.session().await?;
        Ok(rbac::gather(session.client.clone(), namespace).await)
    }

    /// What the current user may do in `namespace` (by default the context's), like
    /// `kubectl auth can-i --list`.
    pub async fn permissions(&self, namespace: Option<&str>) -> Result<Permissions> {
        let session = self.session().await?;
        let namespace = namespace.unwrap_or(&session.info.default_namespace);
        rbac::permissions(session.client.clone(), namespace).await
    }

    /// The latest revision of every Helm release, like `helm list --all`.
    pub async fn helm_releases(&self, namespace: Option<&str>) -> Result<Vec<HelmRelease>> {
        helm::releases(self.session().await?.client.clone(), namespace).await
    }

    /// A Helm release with its values, notes and manifest; the latest revision by default.
    pub async fn helm_release(
        &self,
        namespace: &str,
        name: &str,
        revision: Option<u32>,
    ) -> Result<HelmReleaseDetails> {
        helm::release(self.session().await?.client.clone(), namespace, name, revision).await
    }

    /// All revisions of a Helm release, newest first.
    pub async fn helm_history(&self, namespace: &str, name: &str) -> Result<Vec<HelmRelease>> {
        helm::history(self.session().await?.client.clone(), namespace, name).await
    }

    /// The version of the `helm` command that rollbacks and uninstalls run, if it is installed.
    pub async fn helm_version(&self) -> Option<String> {
        helm::cli_version().await
    }

    /// Rolls a release back to `revision` with `helm rollback`, on the connected context.
    pub async fn helm_rollback(&self, namespace: &str, name: &str, revision: u32) -> Result<String> {
        self.ensure_writable().await?;
        let context = self.session().await?.info.context.clone();
        helm::rollback(&context, namespace, name, revision).await
    }

    /// Installs or upgrades a release, on the connected context. A dry run renders it against
    /// the cluster without changing anything, so it is allowed when Kubri may not change it.
    pub async fn helm_change(&self, change: &HelmChange) -> Result<HelmOutcome> {
        if !change.dry_run {
            self.ensure_writable().await?;
        }
        let session = self.session().await?;
        helm::change(session.client.clone(), &session.info.context, change).await
    }

    /// A chart's default values as YAML, like `helm show values`.
    pub async fn helm_chart_values(&self, chart: &HelmChartRef) -> Result<String> {
        let context = self.session().await?.info.context.clone();
        helm::show_values(&context, chart).await
    }

    /// Charts on Artifact Hub matching `query`, like `helm search hub`.
    pub async fn helm_search(&self, query: &str) -> Result<Vec<HubChart>> {
        let context = self.session().await?.info.context.clone();
        helm::search_hub(&context, query).await
    }

    /// Uninstalls a release with `helm uninstall`, on the connected context.
    pub async fn helm_uninstall(&self, namespace: &str, name: &str) -> Result<String> {
        self.ensure_writable().await?;
        let context = self.session().await?.info.context.clone();
        helm::uninstall(&context, namespace, name).await
    }

    /// The k9s aliases, hotkeys, plugins, skins and settings, with those of the connected
    /// context. [`K9sConfig::read_only`] is also set when Kubri was started read-only.
    pub async fn k9s_config(&self) -> K9sConfig {
        let mut config = self.k9s_files().await;
        config.read_only |= self.read_only.load(Ordering::Relaxed);
        // Views' actions run as plugins of their kind.
        if let Ok(session) = self.session().await {
            for view in self.views().await.views {
                let found = session
                    .resources
                    .iter()
                    .find(|r| view.is_for(&r.reference.group, &r.kind));
                if let Some(resource) = found {
                    config.plugins.extend(view.plugins(&resource.reference.plural));
                }
            }
        }
        config
    }

    /// The k9s configuration as the files have it.
    async fn k9s_files(&self) -> K9sConfig {
        let Some(dirs) = k9s::Dirs::from_env() else {
            return K9sConfig::default();
        };
        let context = self.context_info().await;
        let key = context.as_ref().map(|c| k9s::ContextKey {
            cluster: c.cluster.clone().unwrap_or_default(),
            context: c.name.clone(),
        });
        let env = k9s::Env::read();
        tokio::task::spawn_blocking(move || k9s::load(&dirs, key.as_ref(), &env))
            .await
            .unwrap_or_default()
    }

    /// A k9s skin by name, as JSON.
    pub async fn k9s_skin(&self, name: &str) -> Result<Value> {
        let dirs = k9s::Dirs::from_env().ok_or_else(|| Error::NotFound("the k9s config directory".into()))?;
        let name = name.to_owned();
        tokio::task::spawn_blocking(move || k9s::skin(&dirs, &name))
            .await
            .map_err(|e| Error::Unsupported(e.to_string()))?
    }

    /// Runs a k9s plugin in a terminal, for interactive commands.
    pub async fn open_plugin(&self, run: &PluginRun) -> Result<Terminal> {
        let (plugin, prepared) = self.prepare_plugin(run).await?;
        if plugin.background {
            return Err(Error::Unsupported(format!(
                "plugin {} runs in the background",
                plugin.name
            )));
        }
        let title = if plugin.description.is_empty() {
            plugin.name.clone()
        } else {
            plugin.description.clone()
        };
        tracing::info!(plugin = plugin.name, program = prepared.program, "running plugin");
        plugin::open(&prepared, title, run.cols, run.rows)
    }

    /// Starts a background k9s plugin without waiting for it.
    pub async fn run_plugin(&self, run: &PluginRun) -> Result<()> {
        let (plugin, prepared) = self.prepare_plugin(run).await?;
        if !plugin.background {
            return Err(Error::Unsupported(format!(
                "plugin {} needs a terminal",
                plugin.name
            )));
        }
        tracing::info!(
            plugin = plugin.name,
            program = prepared.program,
            "starting background plugin"
        );
        plugin::spawn_background(&prepared)
    }

    /// Finds the plugin in the configuration on disk, never trusting a command from the
    /// caller, and checks that the target exists and is in the plugin's scope.
    async fn prepare_plugin(&self, run: &PluginRun) -> Result<(Plugin, plugin::Prepared)> {
        let session = self.session().await?;
        let context = self.context_info().await.ok_or(Error::NotConnected)?;
        let config = self.k9s_config().await;
        let plugin = config
            .plugins
            .into_iter()
            .find(|p| p.name == run.plugin)
            .ok_or_else(|| Error::NotFound(format!("plugin {}", run.plugin)))?;
        // Read-only, k9s offers no dangerous plugins.
        if plugin.dangerous {
            self.ensure_writable().await?;
        }
        let inputs = plugin::input_values(&plugin, &run.inputs)?;
        // The selected object's resource, or the view's when nothing is selected.
        let reference = run.target.as_ref().map(|t| &t.resource).or(run.resource.as_ref());
        let resource = match reference {
            Some(reference) => {
                let info = session.info_for(reference)?;
                if !plugin::applies(&plugin, info) {
                    return Err(Error::Unsupported(format!(
                        "plugin {} is not for {}",
                        plugin.name, info.kind
                    )));
                }
                Some(info)
            }
            None if run.release.is_some() => {
                if !plugin::applies_to_helm(&plugin) {
                    return Err(Error::Unsupported(format!(
                        "plugin {} is not for Helm releases",
                        plugin.name
                    )));
                }
                None
            }
            None => None,
        };
        let mut container = None;
        if let (Some(target), Some(info)) = (&run.target, resource) {
            // Fetching it also makes sure `$NAME` and `$NAMESPACE` name a real object.
            let object = session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?;
            if plugin::is_pods(info) {
                let pod: Pod = serde_json::from_value(serde_json::to_value(object)?)?;
                container = match &run.container {
                    Some(name) if terminal::containers(&pod).contains(name) => Some(name.clone()),
                    Some(name) => return Err(Error::NotFound(format!("container {name}"))),
                    None => terminal::default_container(&pod),
                };
            }
        }
        if let Some(release) = &run.release {
            helm::validate_name(&release.namespace)?;
            helm::validate_name(&release.name)?;
        }
        let kubeconfig = kubeconfig::env_value().to_string_lossy().into_owned();
        let mut variables = plugin::variables(&context, kubeconfig, run, resource, container);
        variables.0.extend(inputs);
        let prepared = plugin::prepare(&plugin, &variables, &run.columns)?;
        Ok((plugin, prepared))
    }

    /// The kubeconfig entry of the connected context.
    async fn context_info(&self) -> Option<ContextInfo> {
        let name = self.cluster().await?.context;
        match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => context::list_contexts(kubeconfig, &Default::default())
                .into_iter()
                .find(|c| c.name == name),
            Err(_) => None,
        }
    }

    /// Streams container logs of a pod, or of every pod a workload or Service selects.
    pub async fn logs(
        &self,
        target: &ObjectRef,
        options: LogOptions,
    ) -> Result<impl Stream<Item = LogEvent> + Send + 'static> {
        let session = self.object_session(target).await?;
        let (pods, selector) = session.pods_and_selector(target).await?;
        let mut sources = logs::sources(&pods, options.container.as_deref());
        if sources.is_empty() {
            return Err(Error::Unsupported(format!("{} has no pods", target.name)));
        }
        let notice = (sources.len() > logs::MAX_STREAMS).then(|| LogEvent::Error {
            source: None,
            message: format!(
                "showing the first {} of {} containers",
                logs::MAX_STREAMS,
                sources.len()
            ),
        });
        sources.truncate(logs::MAX_STREAMS);
        let joiner = match (selector, target.namespace.as_deref()) {
            (Some(selector), Some(namespace)) => Some(logs::Joiner {
                api: Api::namespaced(session.client.clone(), namespace),
                selector,
                started: pods
                    .iter()
                    .filter(|p| logs::pod_started(p))
                    .filter_map(|p| p.metadata.name.clone())
                    .collect(),
            }),
            _ => None,
        };
        let events = logs::stream_logs(session.client.clone(), sources, options, joiner);
        // The notice goes after the sources, which always come first.
        Ok(events.enumerate().flat_map(move |(i, event)| {
            let notice = if i == 0 { notice.clone() } else { None };
            futures::stream::iter(std::iter::once(event).chain(notice))
        }))
    }

    /// Opens a shell in a pod's container, or attaches to its main process.
    pub async fn open_terminal(&self, target: &ObjectRef, options: TerminalOptions) -> Result<Terminal> {
        // A shell can change anything the container can.
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        if !session.is_core(&target.resource, "Pod")? {
            return Err(Error::Unsupported("terminals open in pods".into()));
        }
        let subresource = match options.mode {
            TerminalMode::Attach => "attach",
            _ => "exec",
        };
        session.ensure_may("create", target, subresource).await?;
        let api = session.pod_api(target.namespace.as_deref())?;
        let pod = api.get(&target.name).await?;
        let terminal = terminal::open(api.clone(), &pod, options).await?;
        Ok(debug::end_with_terminal(api, &pod, terminal))
    }

    /// Running port-forwards, of every context.
    pub fn port_forwards(&self) -> Vec<PortForwardInfo> {
        self.forwards.lock().unwrap().list()
    }

    /// Forwards a local port to a pod. For a Service or workload a ready pod is picked, and a
    /// Service port is mapped to the pod port it targets; when that pod goes away, the forward
    /// picks another.
    pub async fn start_port_forward(&self, request: PortForwardRequest) -> Result<PortForwardInfo> {
        portforward::validate_address(request.address.as_deref())?;
        let session = self.session().await?;
        let target = &request.target;
        let api = session.pod_api(target.namespace.as_deref())?;
        let (pod, port) = session.forward_destination(target, request.remote_port).await?;
        let repick: Option<portforward::Repick> = if session.is_core(&target.resource, "Pod")? {
            None
        } else {
            let session = Arc::clone(&session);
            let target = target.clone();
            let remote_port = request.remote_port;
            Some(Arc::new(move || {
                let session = Arc::clone(&session);
                let target = target.clone();
                Box::pin(async move {
                    let (pod, port) = session.forward_destination(&target, remote_port).await?;
                    Ok((pod.metadata.name.unwrap_or_default(), port))
                })
            }))
        };
        let destination = portforward::Destination {
            namespace: pod.metadata.namespace.clone().unwrap_or_default(),
            pod: pod.metadata.name.clone().unwrap_or_default(),
            port,
            api,
            repick,
        };
        portforward::start(&self.forwards, session.info.context.clone(), request, destination).await
    }

    /// Starts the connected context's favorite port-forwards that are not running yet, and
    /// says which could not start.
    pub async fn start_favorite_forwards(&self) -> Result<Vec<FavoriteStart>> {
        let context = self.session().await?.info.context.clone();
        let running = self.port_forwards();
        let mut out = Vec::new();
        for favorite in self.settings().context(&context).forwards {
            let already = running
                .iter()
                .any(|f| f.context == context && favorite.matches(f));
            if already {
                continue;
            }
            let request = PortForwardRequest {
                target: ObjectRef {
                    context: Some(context.clone()),
                    ..favorite.target.clone()
                },
                remote_port: favorite.remote_port,
                local_port: Some(favorite.local_port),
                address: Some(favorite.address.clone()).filter(|a| !a.is_empty()),
            };
            let error = self
                .start_port_forward(request)
                .await
                .err()
                .map(|e| e.to_string());
            out.push(FavoriteStart { favorite, error });
        }
        Ok(out)
    }

    pub fn stop_port_forward(&self, id: u32) -> Result<()> {
        self.forwards.lock().unwrap().stop(id)
    }

    /// HTTP benchmarks, newest first, with live progress while they run.
    pub fn benchmarks(&self) -> Vec<BenchInfo> {
        self.benchmarks.lock().unwrap().list()
    }

    /// Starts an HTTP benchmark through a running port-forward, like k9s does with `hey`.
    pub fn start_benchmark(&self, request: &BenchRequest) -> Result<BenchInfo> {
        let forward = self
            .port_forwards()
            .into_iter()
            .find(|f| f.id == request.forward_id)
            .ok_or_else(|| Error::NotFound(format!("port-forward {}", request.forward_id)))?;
        bench::start(&self.benchmarks, request, &forward)
    }

    /// Stops a running benchmark early; it keeps the results of what finished.
    pub fn stop_benchmark(&self, id: u32) -> Result<()> {
        self.benchmarks.lock().unwrap().stop(id)
    }

    pub fn remove_benchmark(&self, id: u32) -> Result<()> {
        self.benchmarks.lock().unwrap().remove(id)
    }

    /// Marks a node (un)schedulable, like `kubectl cordon` and `kubectl uncordon`.
    pub async fn cordon(&self, node: &str, cordoned: bool) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.session().await?;
        if let Some(resource) = session.resolve("v1", "Node") {
            session
                .ensure_allowed(AccessCheck::new("patch", &resource, None).named(node))
                .await?;
        }
        let nodes: Api<Node> = Api::all(session.client.clone());
        let patch = serde_json::json!({ "spec": { "unschedulable": cordoned } });
        nodes
            .patch(node, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(())
    }

    /// Cordons a node and evicts its pods, like `kubectl drain`. Pods the options do not
    /// allow to evict stop the drain before anything is evicted; see [`DrainReport::failed`].
    pub async fn drain(&self, node: &str, options: DrainOptions) -> Result<DrainReport> {
        self.cordon(node, true).await?;
        let session = self.session().await?;
        let pods: Api<Pod> = Api::all(session.client.clone());
        let on_node = ListParams::default().fields(&format!("spec.nodeName={node}"));
        let mut report = DrainReport::default();
        let mut evict = Vec::new();
        for pod in pods.list(&on_node).await?.items {
            match node::plan(&pod, &options) {
                node::Plan::Evict => evict.push(pod),
                node::Plan::Skip(reason) => report.skipped.push(PodIssue {
                    pod: node::pod_name(&pod),
                    reason,
                }),
                node::Plan::Block(reason) => report.failed.push(PodIssue {
                    pod: node::pod_name(&pod),
                    reason,
                }),
            }
        }
        if report.failed.is_empty() {
            node::evict_all(session.client.clone(), evict, &options, &mut report).await;
        }
        tracing::info!(
            node,
            evicted = report.evicted.len(),
            failed = report.failed.len(),
            "drained"
        );
        Ok(report)
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

impl Session {
    /// Fails with [`Error::Forbidden`] when RBAC denies `check`. Where the API server cannot
    /// say, the action goes ahead and the API server decides.
    async fn ensure_allowed(&self, check: AccessCheck) -> Result<()> {
        match self.access_of(&check).await {
            Some(false) => Err(Error::Forbidden(check.describe())),
            _ => Ok(()),
        }
    }

    /// Whether RBAC allows `check`; `None` where the API server cannot say.
    async fn access_of(&self, check: &AccessCheck) -> Option<bool> {
        access::check_all(&self.client, &self.access, std::slice::from_ref(check))
            .await
            .into_iter()
            .next()
            .flatten()
    }

    /// Checks `verb` on the object `obj`, or on its `subresource`.
    async fn ensure_may(&self, verb: &str, obj: &ObjectRef, subresource: &str) -> Result<()> {
        let check = AccessCheck::new(verb, &obj.resource, obj.namespace.as_deref())
            .on_subresource(subresource)
            .named(&obj.name);
        self.ensure_allowed(check).await
    }

    fn new(info: ClusterInfo, client: Client, resources: Vec<ResourceInfo>) -> Self {
        let by_ref = resources
            .iter()
            .enumerate()
            .map(|(i, r)| (r.reference.clone(), i))
            .collect();
        Self {
            info,
            proxy_client: client.clone(),
            client,
            resources,
            by_ref,
            access: Default::default(),
            schemas: Default::default(),
            integrations: Default::default(),
            grafana: Default::default(),
        }
    }

    /// The time ages and "recent" count to: now, or when a snapshot was taken.
    fn now(&self) -> k8s_openapi::jiff::Timestamp {
        self.info
            .snapshot
            .as_ref()
            .and_then(|s| k8s_openapi::jiff::Timestamp::from_millisecond(s.taken).ok())
            .unwrap_or_else(k8s_openapi::jiff::Timestamp::now)
    }

    fn info_for(&self, resource: &ResourceRef) -> Result<&ResourceInfo> {
        self.by_ref
            .get(resource)
            .map(|&i| &self.resources[i])
            .ok_or_else(|| Error::UnknownResource(resource.to_string()))
    }

    /// Whether `resource` is the built-in core type `kind`, such as `Pod`.
    fn is_core(&self, resource: &ResourceRef, kind: &str) -> Result<bool> {
        Ok(resource.group.is_empty() && self.info_for(resource)?.kind == kind)
    }

    async fn metrics(&self, kind: MetricsKind, namespace: Option<&str>) -> Result<Vec<Usage>> {
        let plural = match kind {
            MetricsKind::Pods => "pods",
            MetricsKind::Nodes => "nodes",
        };
        let resource = self
            .resources
            .iter()
            .find(|r| r.reference.group == "metrics.k8s.io" && r.reference.plural == plural)
            .ok_or_else(|| Error::UnknownResource(format!("metrics.k8s.io/{plural}")))?;
        let api = self.api(&resource.reference, namespace)?;
        let request = kube::core::Request::new(api.resource_url())
            .list(&ListParams::default())
            .map_err(|e| Error::Unsupported(e.to_string()))?;
        let list: Value = self.client.request(request).await?;
        Ok(metrics::parse_usage(&list))
    }

    /// The resource type of an object's `apiVersion` and kind, preferring the same version.
    fn resolve(&self, api_version: &str, kind: &str) -> Option<ResourceRef> {
        let (group, version) = api_version.rsplit_once('/').unwrap_or(("", api_version));
        let candidates = self
            .resources
            .iter()
            .filter(|r| r.reference.group == group && r.kind == kind && !r.reference.plural.contains('/'));
        let mut best = None;
        for r in candidates {
            if r.reference.version == version {
                return Some(r.reference.clone());
            }
            best.get_or_insert(r.reference.clone());
        }
        best
    }

    /// The served resource of `group/version/resource`, `version/resource` or a resource name,
    /// as `jumps.yaml` names it; another version of the same resource if need be.
    fn find_path(&self, path: &str) -> Option<&ResourceInfo> {
        let (group, version, name) = workload::split_gvr(path);
        let name = name.to_ascii_lowercase();
        let matches = |r: &&ResourceInfo| {
            let plural = r.reference.plural.as_str();
            if version.is_empty() {
                // A bare name: the plural, singular or a short name, core types first.
                plural == name || r.singular == name || r.short_names.contains(&name)
            } else {
                r.reference.group == group && plural == name
            }
        };
        let mut candidates: Vec<&ResourceInfo> = self.resources.iter().filter(matches).collect();
        candidates.sort_by_key(|r| (!r.reference.group.is_empty(), r.reference.version != version));
        candidates.into_iter().next()
    }

    fn pod_api(&self, namespace: Option<&str>) -> Result<Api<Pod>> {
        let namespace = namespace.ok_or_else(|| Error::Unsupported("a namespace is required".into()))?;
        Ok(Api::namespaced(self.client.clone(), namespace))
    }

    /// The pods behind `target`: the pod itself, or the pods a workload or Service selects.
    /// The pod a forward to `target` reaches, and the port on it: the pod itself, or for a
    /// Service or workload a ready pod, with a Service port mapped to the pod port it targets.
    async fn forward_destination(&self, target: &ObjectRef, remote_port: u16) -> Result<(Pod, u16)> {
        let api = self.pod_api(target.namespace.as_deref())?;
        if self.is_core(&target.resource, "Pod")? {
            return Ok((api.get(&target.name).await?, remote_port));
        }
        let pods = self.pods_of(target).await?;
        let pod = portforward::pick_pod(pods)
            .ok_or_else(|| Error::Unsupported(format!("{} has no ready pod", target.name)))?;
        let port = if self.is_core(&target.resource, "Service")? {
            let services: Api<Service> =
                Api::namespaced(self.client.clone(), api.namespace().unwrap_or_default());
            let service = services.get(&target.name).await?;
            portforward::service_target_port(&service, remote_port, &pod)?
        } else {
            remote_port
        };
        Ok((pod, port))
    }

    async fn pods_of(&self, target: &ObjectRef) -> Result<Vec<Pod>> {
        Ok(self.pods_and_selector(target).await?.0)
    }

    /// The pods behind `target`, and for a workload or Service the label selector that picks
    /// them.
    async fn pods_and_selector(&self, target: &ObjectRef) -> Result<(Vec<Pod>, Option<String>)> {
        let info = self.info_for(&target.resource)?;
        if self.is_core(&target.resource, "Pod")? {
            let pod = self
                .pod_api(target.namespace.as_deref())?
                .get(&target.name)
                .await?;
            return Ok((vec![pod], None));
        }
        if !info.namespaced {
            return Err(Error::Unsupported(format!(
                "a {} has no pods of its own",
                info.kind
            )));
        }
        let object = self
            .api(&target.resource, target.namespace.as_deref())?
            .get(&target.name)
            .await?;
        let selector = selector::pod_selector(&serde_json::to_value(object)?).ok_or_else(|| {
            Error::Unsupported(format!("{} {} does not select pods", info.kind, target.name))
        })?;
        let pods = self.pod_api(target.namespace.as_deref())?;
        let mut pods = pods.list(&ListParams::default().labels(&selector)).await?.items;
        pods.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
        Ok((pods, Some(selector)))
    }

    /// An API handle for `resource`. `namespace` is ignored for cluster-scoped resources.
    fn api(&self, resource: &ResourceRef, namespace: Option<&str>) -> Result<Api<DynamicObject>> {
        let info = self.info_for(resource)?;
        let api_resource = info.api_resource();
        Ok(match namespace {
            Some(ns) if info.namespaced => Api::namespaced_with(self.client.clone(), ns, &api_resource),
            _ => Api::all_with(self.client.clone(), &api_resource),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_kubeconfig_is_reported_not_fatal() {
        let kubri = Kubri::with_kubeconfig(Err("not found".into()));
        assert!(matches!(kubri.contexts().await, Err(Error::NoKubeconfig(_))));
        assert!(matches!(kubri.resources().await, Err(Error::NotConnected)));
        assert!(kubri.cluster().await.is_none());
    }

    #[tokio::test]
    async fn history_says_why_nothing_is_recorded() {
        let kubri = Kubri::with_kubeconfig(Err("not found".into()));
        assert!(matches!(
            kubri.history(&HistoryQuery::default()).await,
            Err(Error::NotConnected)
        ));
        let query = HistoryQuery {
            context: Some("lab".into()),
            ..Default::default()
        };
        let page = kubri.history(&query).await.unwrap();
        assert!(page.entries.is_empty() && page.watched.is_empty());
        assert_eq!(
            page.recorder.reason.as_deref(),
            Some("This Kubri does not record changes")
        );
        kubri.record_changes();
        let page = kubri.history(&query).await.unwrap();
        assert_eq!(
            page.recorder.reason.as_deref(),
            Some("Kubri records only the context it is connected to")
        );
        assert!(matches!(kubri.history_detail(1), Err(Error::NotFound(_))));
    }

    #[tokio::test]
    async fn agents_change_only_contexts_that_allow_it() {
        let kubri = Kubri::with_kubeconfig(Err("not found".into()));
        let allow = |production: bool, locked: bool| ContextSettings {
            agent_writes: true,
            production,
            locked,
            ..Default::default()
        };
        kubri.set_context_settings("dev", allow(false, false)).unwrap();
        kubri.set_context_settings("prod", allow(true, false)).unwrap();
        kubri.set_context_settings("locked", allow(false, true)).unwrap();
        // A person may change all of them, the locked one after unlocking.
        assert!(!kubri.guard_for(Some("dev")).await.read_only);
        assert!(!kubri.guard_for(Some("prod")).await.read_only);

        kubri.set_agent();
        assert!(!kubri.guard_for(Some("dev")).await.read_only);
        for context in ["prod", "locked", "other"] {
            let guard = kubri.guard_for(Some(context)).await;
            assert!(guard.read_only, "{context}");
            assert!(!guard.can_unlock, "{context}");
        }
        assert!(
            kubri
                .guard_for(Some("prod"))
                .await
                .reason
                .unwrap()
                .contains("production")
        );
    }

    fn resource(group: &str, version: &str, plural: &str, kind: &str, namespaced: bool) -> ResourceInfo {
        ResourceInfo {
            reference: ResourceRef {
                group: group.into(),
                version: version.into(),
                plural: plural.into(),
            },
            kind: kind.into(),
            singular: kind.to_lowercase(),
            short_names: Vec::new(),
            namespaced,
            verbs: vec!["list".into(), "watch".into()],
            subresources: Vec::new(),
        }
    }

    /// A Kubri connected to a fake API server.
    async fn connected(client: Client) -> Kubri {
        let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
        let resources = vec![
            resource("", "v1", "pods", "Pod", true),
            resource("", "v1", "configmaps", "ConfigMap", true),
            resource("apps", "v1", "deployments", "Deployment", true),
        ];
        let info = ClusterInfo {
            context: "test".into(),
            server: "https://fake".into(),
            version: "v1.36.0".into(),
            default_namespace: "default".into(),
            snapshot: None,
        };
        *kubri.session.write().await = Some(Arc::new(Session::new(info, client, resources)));
        kubri
    }

    fn list(kind: &str, items: Value) -> String {
        serde_json::json!({ "apiVersion": "v1", "kind": format!("{kind}List"), "metadata": {}, "items": items })
            .to_string()
    }

    #[tokio::test]
    async fn rejects_objects_bound_to_a_different_context() {
        let (client, requests) = testing::client(|_| panic!("stale target must not reach the cluster"));
        let kubri = connected(client).await;
        let target = ObjectRef {
            context: Some("previous-cluster".into()),
            resource: ResourceRef {
                group: String::new(),
                version: "v1".into(),
                plural: "pods".into(),
            },
            namespace: Some("shop".into()),
            name: "worker".into(),
        };
        assert!(matches!(kubri.get(&target).await, Err(Error::ContextChanged(_))));
        assert!(matches!(
            kubri.delete(&target, &DeleteOptions::default()).await,
            Err(Error::ContextChanged(_))
        ));
        assert!(requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn xray_builds_trees_from_listed_objects() {
        let (client, requests) = testing::client(|req| {
            let path = req.uri.split('?').next().unwrap_or_default();
            let body = match path {
                "/apis/apps/v1/namespaces/shop/deployments" => list(
                    "Deployment",
                    serde_json::json!([{
                        "metadata": { "name": "web", "namespace": "shop", "uid": "dep" },
                        "spec": { "replicas": 1, "selector": {}, "template": {} },
                        "status": { "readyReplicas": 1 }
                    }]),
                ),
                "/apis/apps/v1/namespaces/shop/replicasets" => list(
                    "ReplicaSet",
                    serde_json::json!([{
                        "metadata": { "name": "web-1", "namespace": "shop", "uid": "rs",
                                      "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "Deployment",
                                                            "name": "web", "uid": "dep", "controller": true }] },
                        "spec": { "replicas": 1, "selector": {} }, "status": { "replicas": 1, "readyReplicas": 1 }
                    }]),
                ),
                "/api/v1/namespaces/shop/pods" => list(
                    "Pod",
                    serde_json::json!([{
                        "metadata": { "name": "web-1-a", "namespace": "shop", "uid": "pod",
                                      "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet",
                                                            "name": "web-1", "uid": "rs", "controller": true }] },
                        "spec": { "containers": [{ "name": "app", "image": "web:1",
                                  "envFrom": [{ "secretRef": { "name": "web" } }] }] },
                        "status": { "phase": "Running" }
                    }]),
                ),
                "/api/v1/namespaces/shop/secrets" => serde_json::json!({
                    "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadataList", "metadata": {},
                    "items": [{ "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata",
                                "metadata": { "name": "web", "namespace": "shop" } }]
                })
                .to_string(),
                // Not allowed to list these: references stay unchecked instead of failing.
                _ => return (403, testing::status(403, "forbidden")),
            };
            (200, body)
        });
        let kubri = connected(client).await;
        let deployments = ResourceRef {
            group: "apps".into(),
            version: "v1".into(),
            plural: "deployments".into(),
        };
        let trees = kubri.xray(&deployments, Some("shop")).await.unwrap();
        assert_eq!(trees.len(), 1);
        let pod = &trees[0].children[0].children[0];
        assert_eq!(pod.id, "Deployment:shop/web/ReplicaSet:web-1/Pod:web-1-a");
        let names: Vec<(&str, &str)> = pod
            .children
            .iter()
            .map(|c| (c.kind.as_str(), c.detail.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("Container", "Pending · web:1"),
                ("ServiceAccount", "not checked"),
                ("Secret", "")
            ]
        );
        // Everything is listed from the API server's cache.
        let cached = requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.uri.contains("resourceVersion=0"));
        assert!(cached);

        let configmaps = ResourceRef {
            group: String::new(),
            version: "v1".into(),
            plural: "configmaps".into(),
        };
        let err = kubri.xray(&configmaps, None).await.unwrap_err().to_string();
        assert!(err.starts_with("XRay cannot show ConfigMap"), "{err}");
    }

    #[tokio::test]
    async fn pulse_reports_kinds_it_cannot_list() {
        let (client, _) = testing::client(|req| {
            let path = req.uri.split('?').next().unwrap_or_default();
            match path {
                "/api/v1/pods" => (
                    200,
                    list(
                        "Pod",
                        serde_json::json!([
                            { "metadata": { "name": "a" }, "status": { "phase": "Running" },
                              "spec": { "containers": [{ "name": "c", "resources": { "requests": { "cpu": "250m" } } }] } },
                            { "metadata": { "name": "b" }, "status": { "phase": "Failed" }, "spec": { "containers": [] } }
                        ]),
                    ),
                ),
                "/api/v1/nodes" => (
                    200,
                    list(
                        "Node",
                        serde_json::json!([{ "metadata": { "name": "n" },
                        "status": { "allocatable": { "cpu": "2", "memory": "4Gi" },
                                    "conditions": [{ "type": "Ready", "status": "True" }] } }]),
                    ),
                ),
                "/apis/autoscaling/v2/horizontalpodautoscalers" => {
                    (403, testing::status(403, "no autoscalers for you"))
                }
                _ => (200, list("Thing", serde_json::json!([]))),
            }
        });
        let pulse = connected(client).await.pulse(None).await.unwrap();
        let kind = |name: &str| pulse.kinds.iter().find(|k| k.kind == name).unwrap();
        assert_eq!((kind("Pod").total, kind("Pod").ok, kind("Pod").failed), (2, 1, 1));
        assert_eq!(kind("Node").ok, 1);
        assert_eq!(
            kind("HorizontalPodAutoscaler").error.as_deref(),
            Some("no autoscalers for you")
        );
        // No metrics-server: requests and capacity are still known.
        assert_eq!(pulse.cpu.used, None);
        assert_eq!(pulse.cpu.requested, Some(250.0));
        assert_eq!(pulse.cpu.capacity, Some(2000.0));
    }

    #[tokio::test]
    async fn switching_to_unknown_context_fails() {
        let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
        assert!(matches!(
            kubri.switch_context("nope").await,
            Err(Error::UnknownContext(name)) if name == "nope"
        ));
    }

    #[tokio::test]
    async fn a_snapshot_is_read_only_and_says_what_it_is() {
        let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
        let file = serde_json::json!({
            "format": "kubri-snapshot", "version": 1,
            "info": { "context": "prod", "server": "https://prod", "kubernetes": "v1.33.5",
                      "namespace": "shop", "taken": 1_793_000_000_000_i64, "kubri": "0.9.0",
                      "objects": 0, "logs": 0, "leftOut": [] },
            "resources": [], "lists": {}, "logs": {}
        });
        let info = kubri
            .open_snapshot(file.to_string().as_bytes(), Some("prod.kubri".into()))
            .await
            .unwrap();
        assert_eq!(info.context, "prod (snapshot)");
        assert_eq!(info.default_namespace, "shop");
        assert_eq!(
            info.snapshot.as_ref().and_then(|s| s.file.as_deref()),
            Some("prod.kubri")
        );
        let guard = kubri.guard().await;
        assert!(guard.read_only && !guard.can_unlock, "{guard:?}");
        assert!(matches!(
            kubri.take_snapshot(None).await,
            Err(Error::Unsupported(_))
        ));
        assert!(kubri.open_snapshot(b"not a snapshot", None).await.is_err());
        // A file that does not open leaves the snapshot open.
        assert_eq!(
            kubri.cluster().await.map(|c| c.context).as_deref(),
            Some("prod (snapshot)")
        );
    }
}
