//! Kubernetes access shared by the Kubri desktop app and the Kubri web server.
//!
//! [`Kubri`] owns the merged kubeconfig and the connection to the selected context.
//! Both frontends talk to the cluster only through it.

mod bench;
mod context;
mod discovery;
mod error;
mod health;
mod helm;
mod k9s;
mod logs;
mod metrics;
mod node;
mod objects;
mod plugin;
mod portforward;
mod pulse;
mod rbac;
mod resource;
mod sanitizer;
mod selector;
mod table;
mod terminal;
#[cfg(test)]
mod testing;
mod typed;
mod xray;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::{Stream, StreamExt};
use k8s_openapi::api::core::v1::{Event, Node, Pod, Service};
use kube::api::{DynamicObject, ListParams, Patch, PatchParams, PostParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;

pub use bench::{
    BenchInfo, BenchRequest, BenchResults, BenchState, Bucket, ErrorCount, Percentile, StatusCount,
};
pub use context::{ContextInfo, write_in_cluster_kubeconfig};
pub use error::{Error, Result};
pub use health::Health;
pub use helm::{HelmRelease, HelmReleaseDetails};
pub use k9s::{Alias, ConfigFile, Hotkey, K9sConfig, Plugin};
pub use logs::{LogEvent, LogLine, LogOptions, LogSource};
pub use metrics::{MetricsKind, Usage, parse_quantity};
pub use node::{DrainOptions, DrainReport, PodIssue};
pub use objects::{DeleteOptions, EventInfo, ObjectRef, Propagation};
pub use plugin::{Cell, PluginRun, Release};
pub use portforward::{PortForwardInfo, PortForwardRequest};
pub use pulse::{EventPulse, KindPulse, Pulse, RecentEvent, Utilization};
pub use rbac::{Grant, Permissions, Rbac, RoleState, Rule, SubjectInfo};
pub use resource::{ResourceInfo, ResourceRef};
pub use sanitizer::{Finding, Issue, Level, SanitizerReport, SanitizerSection};
pub use table::{Column, Row, TableEvent};
pub use terminal::{Terminal, TerminalInput, TerminalMode, TerminalOptions, TerminalOutput};
pub use xray::XrayNode;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// The cluster Kubri is currently connected to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterInfo {
    pub context: String,
    pub server: String,
    pub version: String,
    /// The context's namespace, or `default`.
    pub default_namespace: String,
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
    resources: Vec<ResourceInfo>,
    by_ref: HashMap<ResourceRef, usize>,
}

pub struct Kubri {
    kubeconfig: RwLock<Result<Kubeconfig, String>>,
    session: RwLock<Option<Arc<Session>>>,
    forwards: std::sync::Mutex<portforward::Forwards>,
    benchmarks: std::sync::Mutex<bench::Benchmarks>,
}

impl Kubri {
    /// Loads the kubeconfig from `KUBECONFIG` (all listed files, merged) or `~/.kube/config`.
    /// A missing or broken kubeconfig is not fatal: it is reported by [`Kubri::contexts`].
    pub fn load() -> Self {
        Self::with_kubeconfig(Kubeconfig::read().map_err(|e| e.to_string()))
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
        }
    }

    /// Re-reads the kubeconfig from disk, keeping the current connection.
    pub async fn reload(&self) -> Result<ContextList> {
        *self.kubeconfig.write().await = Kubeconfig::read().map_err(|e| e.to_string());
        self.contexts().await
    }

    pub async fn contexts(&self) -> Result<ContextList> {
        let contexts = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => context::list_contexts(kubeconfig),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        Ok(ContextList {
            contexts,
            cluster: self.cluster().await,
        })
    }

    pub async fn cluster(&self) -> Option<ClusterInfo> {
        self.session.read().await.as_ref().map(|s| s.info.clone())
    }

    /// Connects to `context` and discovers the resource types it serves.
    /// The previous connection stays active if this fails.
    pub async fn switch_context(&self, context: &str) -> Result<ClusterInfo> {
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

        let options = KubeConfigOptions {
            context: Some(context.to_owned()),
            ..Default::default()
        };
        let session = tokio::time::timeout(CONNECT_TIMEOUT, async {
            let config = Config::from_custom_kubeconfig(kubeconfig, &options).await?;
            let server = config.cluster_url.to_string();
            let client = Client::try_from(config)?;
            let version = client.apiserver_version().await?.git_version;
            let resources = discovery::discover(&client).await?;
            Ok::<_, Error>(Session::new(
                ClusterInfo {
                    context: context.to_owned(),
                    server,
                    version,
                    default_namespace: namespace,
                },
                client,
                resources,
            ))
        })
        .await
        .map_err(|_| Error::Timeout)??;

        let info = session.info.clone();
        *self.session.write().await = Some(Arc::new(session));
        tracing::info!(context, version = %info.version, "connected");
        Ok(info)
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
    ) -> Result<impl Stream<Item = TableEvent> + Send + 'static> {
        let session = self.session().await?;
        let api = session.api(resource, namespace)?;
        Ok(table::watch_table(
            session.client.clone(),
            api.resource_url().to_owned(),
        ))
    }

    /// Fetches one object in full.
    pub async fn get(&self, obj: &ObjectRef) -> Result<Value> {
        let api = self
            .session()
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        Ok(serde_json::to_value(api.get(&obj.name).await?)?)
    }

    /// Replaces an object, as `kubectl edit` does. Fails with a conflict if it changed since
    /// `object` was read.
    pub async fn replace(&self, obj: &ObjectRef, object: Value) -> Result<Value> {
        let api = self
            .session()
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        let object: DynamicObject = serde_json::from_value(object)?;
        let saved = api.replace(&obj.name, &PostParams::default(), &object).await?;
        Ok(serde_json::to_value(saved)?)
    }

    /// Applies a JSON merge patch, for example to change labels or annotations.
    pub async fn patch(&self, obj: &ObjectRef, patch: Value) -> Result<Value> {
        let api = self
            .session()
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        let saved = api
            .patch(&obj.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(serde_json::to_value(saved)?)
    }

    pub async fn delete(&self, obj: &ObjectRef, options: &DeleteOptions) -> Result<()> {
        let api = self
            .session()
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        api.delete(&obj.name, &options.to_params()).await?;
        Ok(())
    }

    /// Sets the replica count through the `scale` subresource, so CRDs that support it work too.
    pub async fn scale(&self, obj: &ObjectRef, replicas: u32) -> Result<()> {
        let session = self.session().await?;
        let info = session.info_for(&obj.resource)?;
        if !info.subresources.iter().any(|s| s == "scale") {
            return Err(Error::Unsupported(format!("{} cannot be scaled", info.kind)));
        }
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let patch = serde_json::json!({ "spec": { "replicas": replicas } });
        api.patch_scale(&obj.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(())
    }

    /// Restarts the pods of a Deployment, StatefulSet or DaemonSet, like
    /// `kubectl rollout restart`.
    pub async fn restart(&self, obj: &ObjectRef) -> Result<()> {
        let session = self.session().await?;
        let info = session.info_for(&obj.resource)?;
        if !matches!(info.kind.as_str(), "Deployment" | "StatefulSet" | "DaemonSet") {
            return Err(Error::Unsupported(format!("{} cannot be restarted", info.kind)));
        }
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

    /// Current CPU and memory usage from metrics-server. Fails with
    /// [`Error::UnknownResource`] when metrics-server is not installed.
    pub async fn metrics(&self, kind: MetricsKind, namespace: Option<&str>) -> Result<Vec<Usage>> {
        self.session().await?.metrics(kind, namespace).await
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
        Ok(pulse::gather(session.client.clone(), namespace, metrics, resolve).await)
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

    /// Checks the objects of `namespace`, or of the whole cluster with nodes, volumes and
    /// cluster roles, for problems and risky settings, like Popeye.
    pub async fn sanitize(&self, namespace: Option<&str>) -> Result<SanitizerReport> {
        let session = self.session().await?;
        let usage = session.metrics(MetricsKind::Nodes, None);
        Ok(sanitizer::scan(session.client.clone(), namespace, usage).await)
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
        let context = self.session().await?.info.context.clone();
        helm::rollback(&context, namespace, name, revision).await
    }

    /// Uninstalls a release with `helm uninstall`, on the connected context.
    pub async fn helm_uninstall(&self, namespace: &str, name: &str) -> Result<String> {
        let context = self.session().await?.info.context.clone();
        helm::uninstall(&context, namespace, name).await
    }

    /// The k9s aliases, hotkeys, plugins and skins, with those of the connected context.
    pub async fn k9s_config(&self) -> K9sConfig {
        let Some(dirs) = k9s::Dirs::from_env() else {
            return K9sConfig::default();
        };
        let context = self.context_info().await;
        let key = context.as_ref().map(|c| k9s::ContextKey {
            cluster: c.cluster.clone().unwrap_or_default(),
            context: c.name.clone(),
        });
        let env_skin = std::env::var("K9S_SKIN").ok();
        tokio::task::spawn_blocking(move || k9s::load(&dirs, key.as_ref(), env_skin))
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
        if let (Some(target), Some(info)) = (&run.target, resource)
            && plugin::is_pods(info)
        {
            let object = session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?;
            let pod: Pod = serde_json::from_value(serde_json::to_value(object)?)?;
            container = match &run.container {
                Some(name) if terminal::containers(&pod).contains(name) => Some(name.clone()),
                Some(name) => return Err(Error::NotFound(format!("container {name}"))),
                None => terminal::default_container(&pod),
            };
        }
        let kubeconfig = std::env::var("KUBECONFIG")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|h| h.join(".kube").join("config").display().to_string())
                    .unwrap_or_default()
            });
        let variables = plugin::variables(&context, kubeconfig, run, resource, container);
        let prepared = plugin::prepare(&plugin, &variables, &run.columns)?;
        Ok((plugin, prepared))
    }

    /// The kubeconfig entry of the connected context.
    async fn context_info(&self) -> Option<ContextInfo> {
        let name = self.cluster().await?.context;
        match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => context::list_contexts(kubeconfig)
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
        let session = self.session().await?;
        let pods = session.pods_of(target).await?;
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
        let events = logs::stream_logs(session.client.clone(), sources, options);
        // The notice goes after the sources, which always come first.
        Ok(events.enumerate().flat_map(move |(i, event)| {
            let notice = if i == 0 { notice.clone() } else { None };
            futures::stream::iter(std::iter::once(event).chain(notice))
        }))
    }

    /// Opens a shell in a pod's container, or attaches to its main process.
    pub async fn open_terminal(&self, target: &ObjectRef, options: TerminalOptions) -> Result<Terminal> {
        let session = self.session().await?;
        if !session.is_core(&target.resource, "Pod")? {
            return Err(Error::Unsupported("terminals open in pods".into()));
        }
        let api = session.pod_api(target.namespace.as_deref())?;
        let pod = api.get(&target.name).await?;
        terminal::open(api, &pod, options).await
    }

    /// Running port-forwards, of every context.
    pub fn port_forwards(&self) -> Vec<PortForwardInfo> {
        self.forwards.lock().unwrap().list()
    }

    /// Forwards a local port to a pod. For a Service or workload a ready pod is picked, and a
    /// Service port is mapped to the pod port it targets.
    pub async fn start_port_forward(&self, request: PortForwardRequest) -> Result<PortForwardInfo> {
        portforward::validate_address(request.address.as_deref())?;
        let session = self.session().await?;
        let target = &request.target;
        let api = session.pod_api(target.namespace.as_deref())?;
        let (pod, port) = if session.is_core(&target.resource, "Pod")? {
            (api.get(&target.name).await?, request.remote_port)
        } else {
            let pods = session.pods_of(target).await?;
            let pod = portforward::pick_pod(pods)
                .ok_or_else(|| Error::Unsupported(format!("{} has no ready pod", target.name)))?;
            let port = if session.is_core(&target.resource, "Service")? {
                let services: Api<Service> =
                    Api::namespaced(session.client.clone(), api.namespace().unwrap_or_default());
                let service = services.get(&target.name).await?;
                portforward::service_target_port(&service, request.remote_port, &pod)?
            } else {
                request.remote_port
            };
            (pod, port)
        };
        let destination = portforward::Destination {
            namespace: pod.metadata.namespace.clone().unwrap_or_default(),
            pod: pod.metadata.name.clone().unwrap_or_default(),
            port,
            api,
        };
        portforward::start(&self.forwards, session.info.context.clone(), request, destination).await
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
        let session = self.session().await?;
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
    fn new(info: ClusterInfo, client: Client, resources: Vec<ResourceInfo>) -> Self {
        let by_ref = resources
            .iter()
            .enumerate()
            .map(|(i, r)| (r.reference.clone(), i))
            .collect();
        Self {
            info,
            client,
            resources,
            by_ref,
        }
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

    fn pod_api(&self, namespace: Option<&str>) -> Result<Api<Pod>> {
        let namespace = namespace.ok_or_else(|| Error::Unsupported("a namespace is required".into()))?;
        Ok(Api::namespaced(self.client.clone(), namespace))
    }

    /// The pods behind `target`: the pod itself, or the pods a workload or Service selects.
    async fn pods_of(&self, target: &ObjectRef) -> Result<Vec<Pod>> {
        let info = self.info_for(&target.resource)?;
        if self.is_core(&target.resource, "Pod")? {
            let pod = self
                .pod_api(target.namespace.as_deref())?
                .get(&target.name)
                .await?;
            return Ok(vec![pod]);
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
        Ok(pods)
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
        };
        *kubri.session.write().await = Some(Arc::new(Session::new(info, client, resources)));
        kubri
    }

    fn list(kind: &str, items: Value) -> String {
        serde_json::json!({ "apiVersion": "v1", "kind": format!("{kind}List"), "metadata": {}, "items": items })
            .to_string()
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
}
