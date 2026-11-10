//! The calls both apps offer the UI, in one table: each is a variant of [`Request`] with the
//! value it answers, and [`Kubri::call`] runs it. The web server takes them at `POST /api/call`
//! and the desktop app as its `call` command, so a new call is written once, here. Calls that
//! stream (tables, logs, terminals) keep their own sockets and channels.
//!
//! The UI's types come from the same table: ts-rs writes `Request` and `Responses`, the
//! answer of each call by its name.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

/// The settings, and why the file could not be read if it could not.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct SettingsReply {
    pub settings: Settings,
    /// Why the settings file could not be read.
    pub error: Option<String>,
}

/// A namespace from the UI, where none and an empty one both mean all namespaces.
fn namespace(namespace: &Option<String>) -> Option<&str> {
    namespace.as_deref().filter(|ns| !ns.is_empty())
}

/// Each line is `name { arguments } -> answer => |kubri| body;`, where the body may use the
/// arguments by name. Names are camelCase on the wire: `AddContexts` is `addContexts`. An
/// argument marked `#[serde(flatten)]` sends its fields next to `call`, where the audit log
/// finds the target and details of a change.
macro_rules! calls {
    ($(
        $(#[doc = $doc:literal])*
        $name:ident { $($(#[$attr:meta])* $field:ident : $ty:ty),* $(,)? } -> $reply:ty => |$kubri:ident| $body:expr;
    )*) => {
        /// A call from the UI, named by `call`, with its arguments next to it.
        #[derive(Debug, Clone, Deserialize, ts_rs::TS)]
        #[serde(tag = "call", rename_all = "camelCase", rename_all_fields = "camelCase")]
        #[ts(export)]
        pub enum Request {
            $(
                $(#[doc = $doc])*
                $name { $($(#[$attr])* $field: $ty),* },
            )*
        }

        /// What each call answers, by its name. Only its type is used, by the UI.
        #[allow(dead_code, non_snake_case)]
        #[derive(ts_rs::TS)]
        #[ts(export, rename_all = "camelCase")]
        pub struct Responses {
            $(pub $name: $reply,)*
        }

        impl Request {
            /// Every call's name, as the UI sends it.
            pub fn names() -> Vec<String> {
                vec![$(camel(stringify!($name)),)*]
            }
        }

        impl Kubri {
            /// Runs `request` and answers with its value as JSON.
            pub async fn call(&self, request: Request) -> Result<Value> {
                match request {
                    $(
                        Request::$name { $($field),* } => {
                            let $kubri = self;
                            let reply: $reply = $body;
                            Ok(serde_json::to_value(reply)?)
                        }
                    )*
                }
            }
        }
    };
}

calls! {
    // Contexts and kubeconfig files.

    /// Every context, and the one Kubri is connected to.
    Contexts {} -> ContextList => |kubri| kubri.contexts().await?;
    /// Reads the kubeconfig files again.
    ReloadContexts {} -> ContextList => |kubri| kubri.reload().await?;
    /// The kubeconfig files Kubri reads.
    KubeconfigFiles {} -> Vec<KubeconfigFile> => |kubri| kubri.kubeconfig_files();
    /// The contexts of a pasted kubeconfig, before adding them.
    InspectKubeconfig { text: String } -> Vec<NewContext> => |kubri| kubri.inspect_kubeconfig(&text).await?;
    /// Adds the contexts `names` of a pasted kubeconfig.
    AddContexts { text: String, names: Vec<String> } -> ContextList =>
        |kubri| kubri.add_contexts(&text, &names).await?;
    /// Removes a context added in Kubri.
    RemoveContext { name: String } -> ContextList => |kubri| kubri.remove_context(&name).await?;
    /// Connects to another context.
    SwitchContext { name: String } -> ClusterInfo => |kubri| kubri.switch_context(&name).await?;
    /// Stops a switch that is still connecting.
    CancelSwitch {} -> () => |kubri| kubri.cancel_switch().await;
    /// Leaves the open snapshot for no context, when there is none to go back to.
    CloseSnapshot {} -> () => |kubri| kubri.close_snapshot().await;

    // The cluster's kinds, and what the user may do.

    /// The kinds of the cluster.
    Resources {} -> Vec<ResourceInfo> => |kubri| kubri.resources().await?;
    /// Asks the cluster for its kinds again.
    RefreshResources {} -> Vec<ResourceInfo> => |kubri| kubri.refresh_resources().await?;
    /// The OpenAPI schema of a kind, or null when the cluster has none.
    Schema { resource: ResourceRef } -> Value =>
        |kubri| kubri.schema(&resource).await?.as_ref().clone().unwrap_or(Value::Null);
    /// Whether the user may do each of `checks`, null where the cluster did not say.
    Access { checks: Vec<AccessCheck>, fresh: bool } -> Vec<Option<bool>> =>
        |kubri| kubri.access(&checks, fresh).await?;
    /// Who the cluster takes the user for.
    WhoAmI {} -> Option<Identity> => |kubri| kubri.who_am_i().await?;
    /// The namespaces the user may list.
    ReadableNamespaces {} -> Vec<String> => |kubri| kubri.readable_namespaces().await?;
    /// Whether the context may be changed, and why not.
    Guard {} -> Guard => |kubri| kubri.guard().await;
    /// Allows changes to a locked context for a while.
    Unlock {} -> Guard => |kubri| kubri.unlock().await?;
    /// Locks the context again.
    Lock {} -> Guard => |kubri| kubri.lock().await?;

    // Settings.

    /// The settings, and why the file could not be read if it could not.
    Settings {} -> SettingsReply =>
        |kubri| SettingsReply { settings: kubri.settings(), error: kubri.settings_error() };
    /// Sets the settings of one context.
    SetContextSettings { name: String, settings: ContextSettings } -> Settings =>
        |kubri| kubri.set_context_settings(&name, settings)?;
    SetDiffSettings { settings: DiffSettings } -> Settings => |kubri| kubri.set_diff_settings(settings)?;
    SetHistorySettings { settings: HistorySettings } -> Settings =>
        |kubri| kubri.set_history_settings(settings).await?;
    SetConnectionSettings { settings: ConnectionSettings } -> Settings =>
        |kubri| kubri.set_connection_settings(settings).await?;
    SetTerminalSettings { settings: TerminalSettings } -> Settings =>
        |kubri| kubri.set_terminal_settings(settings)?;
    SetLabelTemplates { templates: Vec<LabelTemplate> } -> Settings =>
        |kubri| kubri.set_label_templates(templates)?;
    SetColumnSettings { kind: String, columns: ColumnSettings } -> Settings =>
        |kubri| kubri.set_column_settings(&kind, columns)?;
    SetAgentSettings { settings: AgentSettings } -> Settings => |kubri| kubri.set_agent_settings(settings)?;
    SetExplainSettings { settings: ExplainSettings } -> Settings =>
        |kubri| kubri.set_explain_settings(settings)?;
    SetSavedSearches { searches: Vec<SavedSearch> } -> Settings =>
        |kubri| kubri.set_saved_searches(searches)?;

    // What changed.

    /// A page of the recorded changes.
    History { #[serde(flatten)] query: HistoryQuery } -> HistoryPage => |kubri| kubri.history(&query).await?;
    /// One recorded change, with the object before and after.
    HistoryDetail { id: i64 } -> HistoryDetail => |kubri| kubri.history_detail(id)?;

    // Objects.

    /// An object.
    GetObject { target: ObjectRef } -> Value => |kubri| kubri.get(&target).await?;
    /// An object as another context has it.
    GetObjectElsewhere { target: ObjectRef } -> Value => |kubri| kubri.get_elsewhere(&target).await?;
    /// Replaces an object with `value`, as edited in YAML.
    ReplaceObject { target: ObjectRef, value: Value } -> Value =>
        |kubri| kubri.replace(&target, value).await?;
    /// Patches an object.
    PatchObject { target: ObjectRef, patch: Value } -> Value => |kubri| kubri.patch(&target, patch).await?;
    /// Deletes an object.
    DeleteObject { target: ObjectRef, options: Option<DeleteOptions> } -> () =>
        |kubri| kubri.delete(&target, &options.unwrap_or_default()).await?;
    /// What a change would do, before it is made.
    Preview { target: ObjectRef, change: Change } -> Preview => |kubri| kubri.preview(&target, change).await?;
    Scale { target: ObjectRef, replicas: u32 } -> () => |kubri| kubri.scale(&target, replicas).await?;
    Restart { target: ObjectRef } -> () => |kubri| kubri.restart(&target).await?;
    /// Starts a debug container.
    Debug { #[serde(flatten)] request: DebugRequest } -> DebugSession => |kubri| kubri.debug(request).await?;
    /// Applies manifests, or tries them with `dryRun`.
    Apply { #[serde(flatten)] request: ApplyRequest } -> Vec<ApplyItem> => |kubri| kubri.apply(request).await?;
    /// Fetches a manifest from an address, to apply it.
    FetchManifest { url: String } -> String => |_kubri| fetch_manifest(&url).await?;
    /// Where to go from an object: its pods, its nodes and so on.
    Drill { target: ObjectRef } -> Option<Drill> => |kubri| kubri.drill(&target).await?;
    /// The objects that use an object.
    UsedBy { target: ObjectRef } -> Vec<Reference> => |kubri| kubri.used_by(&target).await?;
    /// Who manages an object, such as Helm or a GitOps tool.
    ManagedBy { target: ObjectRef } -> Option<Manager> => |kubri| kubri.managed_by(&target).await?;
    /// The events of an object.
    Events { namespace: Option<String>, uid: String } -> Vec<EventInfo> =>
        |kubri| kubri.events(self::namespace(&namespace), &uid).await?;
    /// Starts a job from a CronJob and answers with its name.
    TriggerCronJob { target: ObjectRef } -> String => |kubri| kubri.trigger_cronjob(&target).await?;
    SuspendCronJob { target: ObjectRef, suspend: bool } -> () =>
        |kubri| kubri.suspend_cronjob(&target, suspend).await?;
    /// Syncs, suspends or resumes an object of a GitOps tool.
    Gitops { target: ObjectRef, action: GitOpsAction } -> () => |kubri| kubri.gitops(&target, action).await?;
    RenewCertificate { target: ObjectRef } -> () => |kubri| kubri.renew_certificate(&target).await?;
    SetImages { target: ObjectRef, images: Vec<ContainerImage> } -> () =>
        |kubri| kubri.set_images(&target, &images).await?;
    /// Rolls a Deployment back to a ReplicaSet and answers with the Deployment's name.
    Rollback { target: ObjectRef } -> String => |kubri| kubri.rollback(&target).await?;

    // Fixes and resources.

    /// A memory limit that would stop a container being killed.
    MemoryFix { target: ObjectRef, container: String } -> MemoryFix =>
        |kubri| kubri.memory_fix(&target, &container).await?;
    SetMemoryLimit { target: ObjectRef, container: String, memory: String } -> () =>
        |kubri| kubri.set_memory_limit(&target, &container, &memory).await?;
    /// Why a pod is not scheduled, node by node.
    Scheduling { target: ObjectRef } -> Vec<NodeFit> => |kubri| kubri.scheduling(&target).await?;
    /// Why a Service reaches no pods.
    ServiceCheck { target: ObjectRef } -> ServiceCheck => |kubri| kubri.service_check(&target).await?;
    /// Room for an object's requests on the nodes and in the namespace's quota.
    Capacity { target: ObjectRef } -> Capacity => |kubri| kubri.capacity(&target).await?;
    /// Requests and limits next to what the containers use.
    RightSizing { namespace: Option<String> } -> RightSizing =>
        |kubri| kubri.right_sizing(self::namespace(&namespace)).await?;
    SetResources { target: ObjectRef, containers: Vec<ContainerResources> } -> () =>
        |kubri| kubri.set_resources(&target, &containers).await?;

    // Overviews.

    /// Objects of every kind that match a query.
    Search { #[serde(flatten)] request: SearchRequest } -> SearchResult => |kubri| kubri.search(&request).await?;
    /// The health of another context, for the Clusters page.
    FleetCluster { context: String } -> FleetCluster => |kubri| kubri.fleet_cluster(&context).await?;
    /// The views of custom resources.
    Views {} -> ViewSet => |kubri| kubri.views().await;
    /// An object as its kind's view shows it.
    RenderView { resource: ResourceRef, object: Value } -> Option<RenderedView> =>
        |kubri| kubri.render_view(&resource, &object).await?;
    /// What pods or nodes use.
    Metrics { kind: MetricsKind, namespace: Option<String> } -> Vec<Usage> =>
        |kubri| kubri.metrics(kind, self::namespace(&namespace)).await?;
    /// The overview's numbers.
    Pulse { namespace: Option<String> } -> Pulse => |kubri| kubri.pulse(self::namespace(&namespace)).await?;
    /// An object and what it is made of, as a tree.
    Xray { resource: ResourceRef, namespace: Option<String> } -> Vec<XrayNode> =>
        |kubri| kubri.xray(&resource, self::namespace(&namespace)).await?;
    /// How the objects of a namespace connect.
    Graph { namespace: Option<String> } -> Graph => |kubri| kubri.graph(self::namespace(&namespace)).await?;
    /// The checks of objects for common mistakes.
    Sanitize { namespace: Option<String> } -> SanitizerReport =>
        |kubri| kubri.sanitize(self::namespace(&namespace)).await?;
    /// Who may do what, by role.
    Rbac { namespace: Option<String> } -> Rbac => |kubri| kubri.rbac(self::namespace(&namespace)).await?;
    /// What the user may do.
    Permissions { namespace: Option<String> } -> Permissions =>
        |kubri| kubri.permissions(self::namespace(&namespace)).await?;

    // Integrations.

    /// Prometheus, Grafana and Loki, found or set.
    Integrations { fresh: bool } -> Integrations => |kubri| kubri.integrations(fresh).await?;
    /// An object's charts from Prometheus.
    Charts { target: ObjectRef, range: Range } -> Charts => |kubri| kubri.charts(&target, range).await?;
    /// What a namespace or the cluster used over time.
    UsageHistory { namespace: Option<String>, seconds: u64 } -> Charts =>
        |kubri| kubri.usage_history(self::namespace(&namespace), Range { seconds, end: None }).await?;
    /// Grafana dashboards for an object.
    Dashboards { target: ObjectRef } -> Dashboards => |kubri| kubri.dashboards(&target).await?;
    /// An object's logs in Grafana.
    GrafanaLogs { target: ObjectRef } -> DashboardLink => |kubri| kubri.grafana_logs(&target).await?;
    /// The address that opens an integration.
    IntegrationUrl { id: String } -> String => |kubri| kubri.integration_url(&id).await?;
    /// Log lines from Loki.
    LokiLogs { #[serde(flatten)] request: LokiRequest } -> LokiLogs => |kubri| kubri.loki_logs(&request).await?;

    // Port-forwards, benchmarks and nodes.

    /// Running port-forwards, of every context.
    PortForwards {} -> Vec<PortForwardInfo> => |kubri| kubri.port_forwards();
    StartPortForward { #[serde(flatten)] request: PortForwardRequest } -> PortForwardInfo =>
        |kubri| kubri.start_port_forward(request).await?;
    /// Starts the context's favorite port-forwards that start on connect.
    StartFavoriteForwards {} -> Vec<FavoriteStart> => |kubri| kubri.start_favorite_forwards().await?;
    StopPortForward { id: u32 } -> () => |kubri| kubri.stop_port_forward(id)?;
    Benchmarks {} -> Vec<BenchInfo> => |kubri| kubri.benchmarks();
    StartBenchmark { #[serde(flatten)] request: BenchRequest } -> BenchInfo => |kubri| kubri.start_benchmark(&request)?;
    StopBenchmark { id: u32 } -> () => |kubri| kubri.stop_benchmark(id)?;
    RemoveBenchmark { id: u32 } -> () => |kubri| kubri.remove_benchmark(id)?;
    Cordon { name: String, cordoned: bool } -> () => |kubri| kubri.cordon(&name, cordoned).await?;
    Drain { name: String, options: Option<DrainOptions> } -> DrainReport =>
        |kubri| kubri.drain(&name, options.unwrap_or_default()).await?;

    // Helm.

    HelmReleases { namespace: Option<String> } -> Vec<HelmRelease> =>
        |kubri| kubri.helm_releases(self::namespace(&namespace)).await?;
    /// A release, at its latest revision or at `revision`.
    HelmRelease { namespace: String, name: String, revision: Option<u32> } -> HelmReleaseDetails =>
        |kubri| kubri.helm_release(&namespace, &name, revision).await?;
    HelmHistory { namespace: String, name: String } -> Vec<HelmRelease> =>
        |kubri| kubri.helm_history(&namespace, &name).await?;
    /// The version of the helm command, or null without one.
    HelmVersion {} -> Option<String> => |kubri| kubri.helm_version().await;
    HelmRollback { namespace: String, name: String, revision: u32 } -> String =>
        |kubri| kubri.helm_rollback(&namespace, &name, revision).await?;
    HelmUninstall { namespace: String, name: String } -> String =>
        |kubri| kubri.helm_uninstall(&namespace, &name).await?;
    /// Installs or upgrades a release.
    HelmChange { #[serde(flatten)] change: HelmChange } -> HelmOutcome => |kubri| kubri.helm_change(&change).await?;
    /// A chart's default values.
    HelmChartValues { #[serde(flatten)] chart: HelmChartRef } -> String =>
        |kubri| kubri.helm_chart_values(&chart).await?;
    /// Charts on Artifact Hub.
    HelmSearch { query: String } -> Vec<HubChart> => |kubri| kubri.helm_search(&query).await?;
    /// The newest release of Kubri.
    LatestRelease {} -> LatestRelease => |_kubri| latest_release().await?;

    // k9s, plugins and local tools.

    /// The k9s configuration Kubri follows.
    K9sConfig {} -> K9sConfig => |kubri| kubri.k9s_config().await;
    /// A k9s skin, by name.
    K9sSkin { name: String } -> Value => |kubri| kubri.k9s_skin(&name).await?;
    /// Starts a background plugin; Kubri does not wait for it to finish.
    RunPlugin { #[serde(flatten)] run: PluginRun } -> () => |kubri| kubri.run_plugin(&run).await?;
    /// Whether a local terminal can be opened, and what it would run with.
    LocalTerminalInfo {} -> LocalTerminalInfo => |kubri| kubri.local_terminal_info().await;
    /// The tools that can make a cluster on this computer.
    LocalClusters {} -> LocalClusters => |kubri| kubri.local_clusters().await;
}

/// `AddContexts` as `addContexts`, as serde names the calls.
fn camel(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_the_call_and_its_arguments_by_name() {
        let request: Request =
            serde_json::from_value(json!({ "call": "addContexts", "text": "x", "names": ["a"] })).unwrap();
        assert!(matches!(request, Request::AddContexts { text, names } if text == "x" && names == ["a"]));
        let request: Request = serde_json::from_value(json!({ "call": "contexts" })).unwrap();
        assert!(matches!(request, Request::Contexts {}));
        assert!(serde_json::from_value::<Request>(json!({ "call": "switchContext" })).is_err());
        assert!(serde_json::from_value::<Request>(json!({ "call": "nothing" })).is_err());
    }

    #[test]
    fn reads_requests_whose_fields_sit_next_to_the_call() {
        let target = json!({ "resource": { "group": "", "version": "v1", "plural": "pods" }, "name": "web" });
        let request: Request = serde_json::from_value(
            json!({ "call": "startPortForward", "target": target, "remotePort": 80, "localPort": null }),
        )
        .unwrap();
        assert!(matches!(request, Request::StartPortForward { request } if request.remote_port == 80));
        let request: Request = serde_json::from_value(
            json!({ "call": "debug", "kind": "copy", "target": target, "container": null }),
        )
        .unwrap();
        assert!(matches!(
            request,
            Request::Debug {
                request: DebugRequest::Copy { .. }
            }
        ));
        let request: Request = serde_json::from_value(json!({ "call": "history", "kind": "Pod" })).unwrap();
        assert!(matches!(request, Request::History { query } if query.kind.as_deref() == Some("Pod")));
    }

    #[test]
    fn names_the_calls_as_the_ui_sends_them() {
        let names = Request::names();
        assert!(names.iter().any(|n| n == "whoAmI"));
        assert!(names.iter().any(|n| n == "k9sConfig"));
        for name in &names {
            // Every name reads back as a call, so the list and serde agree.
            let err = serde_json::from_value::<Request>(json!({ "call": name })).err();
            assert!(
                err.as_ref()
                    .is_none_or(|e| !e.to_string().contains("unknown variant")),
                "{name}: {err:?}"
            );
        }
    }

    #[test]
    fn takes_an_empty_namespace_for_all_namespaces() {
        assert_eq!(namespace(&Some(String::new())), None);
        assert_eq!(namespace(&None), None);
        assert_eq!(namespace(&Some("shop".into())), Some("shop"));
    }

    #[tokio::test]
    async fn runs_a_call_and_answers_with_its_value() {
        let kubeconfig: kube::config::Kubeconfig = serde_saphyr::from_str(
            "contexts:\n- name: one\n  context: {cluster: c, user: u}\nclusters:\n- name: c\n  cluster: {server: 'https://127.0.0.1:1'}\nusers:\n- name: u\n  user: {}\n",
        )
        .unwrap();
        let kubri = Kubri::from_kubeconfig(kubeconfig);
        let reply = kubri.call(Request::Contexts {}).await.unwrap();
        assert_eq!(reply["contexts"][0]["name"], "one");
        assert_eq!(reply["cluster"], Value::Null);
        assert_eq!(kubri.call(Request::CancelSwitch {}).await.unwrap(), Value::Null);
    }
}
