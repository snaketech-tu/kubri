use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, delete, get, post, put};
use axum::{Json, Router};
use futures::{SinkExt, Stream, StreamExt};
use kubri_core::{
    AccessCheck, AgentSettings, BenchInfo, BenchRequest, Change, ClusterInfo, ContainerImage, ContextList,
    ContextSettings, DebugRequest, DebugSession, DeleteOptions, DiffSettings, DrainOptions, DrainReport,
    Drill, EventInfo, ExplainAnswer, ExplainPrompt, ExplainSettings, ExplainSubject, Guard, HelmRelease,
    HelmReleaseDetails, Identity, K9sConfig, Kubri, LogEvent, LogOptions, MetricsKind, ObjectRef,
    Permissions, PluginRun, PortForwardInfo, PortForwardRequest, Preview, Pulse, Rbac, Reference,
    ResourceInfo, ResourceRef, SanitizerReport, Settings, TableScope, Terminal, TerminalOptions,
    TerminalOutput, Usage, XrayNode,
};
use kubri_core::{ApplyItem, ApplyRequest, MemoryFix, NodeFit, ServiceCheck};
use kubri_core::{Charts, Dashboards, Integrations, LokiLogs, LokiRequest, Range, Series};
use kubri_core::{FleetCluster, GitOpsAction, Manager, NewContext, RightSizing, SearchRequest, SearchResult};
use kubri_core::{HelmChange, HelmChartRef, HelmOutcome, HubChart};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn router(kubri: Arc<Kubri>) -> Router {
    Router::new()
        .route("/contexts", get(contexts))
        .route("/contexts/reload", post(reload))
        .route("/contexts/inspect", post(inspect_kubeconfig))
        .route("/contexts/added", post(add_contexts).delete(remove_context))
        .route("/context", put(switch_context))
        .route("/settings", get(settings))
        .route("/settings/context", put(set_context_settings))
        .route("/settings/diff", put(set_diff_settings))
        .route("/settings/searches", put(set_saved_searches))
        .route("/settings/columns", put(set_column_settings))
        .route("/settings/agents", put(set_agent_settings))
        .route("/settings/explain", put(set_explain_settings))
        .route("/explain/key", get(explain_key).put(set_explain_key))
        .route("/explain/prompt", post(explain_prompt))
        .route("/explain", post(explain))
        .route("/guard", get(guard))
        .route("/guard/unlock", post(unlock))
        .route("/guard/lock", post(lock))
        .route("/schema", get(schema))
        .route("/access", post(access))
        .route("/whoami", get(who_am_i))
        .route("/namespaces/readable", get(readable_namespaces))
        .route("/resources", get(resources))
        .route("/table", any(table))
        .route(
            "/object",
            get(get_object)
                .put(replace_object)
                .patch(patch_object)
                .delete(delete_object),
        )
        .route("/object/elsewhere", get(get_object_elsewhere))
        .route("/object/preview", post(preview))
        .route("/object/scale", post(scale))
        .route("/object/restart", post(restart))
        .route("/debug", post(debug))
        .route("/fix/memory", post(memory_fix))
        .route("/fix/memory/apply", post(set_memory_limit))
        .route("/fix/scheduling", post(scheduling))
        .route("/fix/service", post(service_check))
        .route("/apply", post(apply))
        .route("/apply/fetch", post(fetch_manifest))
        .route("/object/drill", post(drill))
        .route("/object/used-by", post(used_by))
        .route("/object/images", post(set_images))
        .route("/cronjob/trigger", post(trigger_cronjob))
        .route("/cronjob/suspend", post(suspend_cronjob))
        .route("/gitops", post(gitops))
        .route("/certificate/renew", post(renew_certificate))
        .route("/right-sizing", get(right_sizing))
        .route("/object/resources", post(set_resources))
        .route("/object/managed-by", post(managed_by))
        .route("/replicaset/rollback", post(rollback))
        .route("/events", get(events))
        .route("/metrics", get(metrics))
        .route("/pulse", get(pulse))
        .route("/fleet", get(fleet))
        .route("/search", post(search))
        .route("/integrations", get(integrations))
        .route("/charts", post(charts))
        .route("/usage-history", get(usage_history))
        .route("/prometheus/query", post(prometheus_query))
        .route("/grafana/dashboards", post(dashboards))
        .route("/integrations/url", post(integration_url))
        .route("/loki/logs", post(loki_logs))
        .route("/xray", get(xray))
        .route("/rbac", get(rbac))
        .route("/sanitize", get(sanitize))
        .route("/permissions", get(permissions))
        .route("/helm/releases", get(helm_releases))
        .route("/helm/release", get(helm_release))
        .route("/helm/history", get(helm_history))
        .route("/helm/version", get(helm_version))
        .route("/helm/rollback", post(helm_rollback))
        .route("/helm/uninstall", post(helm_uninstall))
        .route("/helm/change", post(helm_change))
        .route("/helm/chart-values", post(helm_chart_values))
        .route("/helm/search", get(helm_search))
        .route("/k9s", get(k9s_config))
        .route("/k9s/skins/{name}", get(k9s_skin))
        .route("/plugins/run", post(run_plugin))
        .route("/plugins/terminal", any(plugin_terminal))
        .route("/logs", any(logs))
        .route("/terminal", any(terminal))
        .route("/portforwards", get(port_forwards).post(start_port_forward))
        .route("/portforwards/{id}", delete(stop_port_forward))
        .route("/portforwards/favorites", post(start_favorite_forwards))
        .route("/benchmarks", get(benchmarks).post(start_benchmark))
        .route("/benchmarks/{id}", delete(remove_benchmark))
        .route("/benchmarks/{id}/stop", post(stop_benchmark))
        .route("/node/cordon", post(cordon))
        .route("/node/drain", post(drain))
        .with_state(kubri)
}

struct ApiError(kubri_core::Error);

impl From<kubri_core::Error> for ApiError {
    fn from(err: kubri_core::Error) -> Self {
        Self(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        use kubri_core::Error::*;
        let status = match &self.0 {
            // The cluster turned down the kubeconfig's credentials. Not passed on as 401,
            // which the UI takes to mean that the browser's session with Kubri ended.
            err @ Kube(_) if err.api_status() == Some(401) => StatusCode::BAD_GATEWAY,
            // Pass on what the API server said: 403, 404, 409 conflict, 422 invalid…
            err @ Kube(_) if err.api_status().is_some() => err
                .api_status()
                .and_then(|code| StatusCode::from_u16(code).ok())
                .unwrap_or(StatusCode::BAD_GATEWAY),
            UnknownContext(_) | UnknownResource(_) | NotFound(_) => StatusCode::NOT_FOUND,
            NoKubeconfig(_) | NotConnected | ContextChanged(_) => StatusCode::CONFLICT,
            Unsupported(_) | InvalidObject(_) => StatusCode::BAD_REQUEST,
            Timeout => StatusCode::GATEWAY_TIMEOUT,
            ReadOnly(_) | Forbidden(_) => StatusCode::FORBIDDEN,
            InvalidKubeconfig(_) | Kubeconfig(_) | Kube(_) | Model(_) | Integration(_) => {
                StatusCode::BAD_GATEWAY
            }
            Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(serde_json::json!({ "error": self.0.to_string() }));
        (status, body).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

async fn contexts(State(kubri): State<Arc<Kubri>>) -> ApiResult<ContextList> {
    Ok(Json(kubri.contexts().await?))
}

async fn reload(State(kubri): State<Arc<Kubri>>) -> ApiResult<ContextList> {
    Ok(Json(kubri.reload().await?))
}

#[derive(Deserialize)]
struct KubeconfigText {
    text: String,
}

async fn inspect_kubeconfig(
    State(kubri): State<Arc<Kubri>>,
    Json(body): Json<KubeconfigText>,
) -> ApiResult<Vec<NewContext>> {
    Ok(Json(kubri.inspect_kubeconfig(&body.text).await?))
}

#[derive(Deserialize)]
struct AddContexts {
    text: String,
    names: Vec<String>,
}

async fn add_contexts(
    State(kubri): State<Arc<Kubri>>,
    Json(body): Json<AddContexts>,
) -> ApiResult<ContextList> {
    Ok(Json(kubri.add_contexts(&body.text, &body.names).await?))
}

#[derive(Deserialize)]
struct ContextName {
    name: String,
}

async fn remove_context(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<ContextName>,
) -> ApiResult<ContextList> {
    Ok(Json(kubri.remove_context(&query.name).await?))
}

#[derive(Deserialize)]
struct SwitchContext {
    name: String,
}

async fn switch_context(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<SwitchContext>,
) -> ApiResult<ClusterInfo> {
    Ok(Json(kubri.switch_context(&req.name).await?))
}

/// The settings, and why the file could not be read if it could not.
#[derive(Serialize)]
struct SettingsReply {
    settings: Settings,
    error: Option<String>,
}

async fn settings(State(kubri): State<Arc<Kubri>>) -> ApiResult<SettingsReply> {
    Ok(Json(SettingsReply {
        settings: kubri.settings(),
        error: kubri.settings_error(),
    }))
}

#[derive(Deserialize)]
struct SetContextSettings {
    name: String,
    settings: ContextSettings,
}

async fn set_context_settings(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<SetContextSettings>,
) -> ApiResult<Settings> {
    Ok(Json(kubri.set_context_settings(&req.name, req.settings)?))
}

async fn set_saved_searches(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<Vec<kubri_core::SavedSearch>>,
) -> ApiResult<Settings> {
    Ok(Json(kubri.set_saved_searches(req)?))
}

async fn set_diff_settings(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<DiffSettings>,
) -> ApiResult<Settings> {
    Ok(Json(kubri.set_diff_settings(req)?))
}

#[derive(Deserialize)]
struct SetColumnSettings {
    kind: String,
    columns: kubri_core::ColumnSettings,
}

async fn set_column_settings(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<SetColumnSettings>,
) -> ApiResult<Settings> {
    Ok(Json(kubri.set_column_settings(&req.kind, req.columns)?))
}

async fn set_agent_settings(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<AgentSettings>,
) -> ApiResult<Settings> {
    Ok(Json(kubri.set_agent_settings(req)?))
}

async fn set_explain_settings(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<ExplainSettings>,
) -> ApiResult<Settings> {
    Ok(Json(kubri.set_explain_settings(req)?))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExplainKey {
    /// Never the key itself: only whether one is stored.
    has_key: bool,
}

#[derive(Deserialize)]
struct SetExplainKey {
    /// `None` or empty removes the key.
    key: Option<String>,
}

/// The web server keeps the key in a file only its user can read; see [`kubri_core::explain`].
async fn explain_key() -> ApiResult<ExplainKey> {
    Ok(Json(ExplainKey {
        has_key: kubri_core::explain::read_key_file().is_some(),
    }))
}

async fn set_explain_key(Json(req): Json<SetExplainKey>) -> ApiResult<ExplainKey> {
    kubri_core::explain::write_key_file(req.key.as_deref())?;
    explain_key().await
}

async fn explain_prompt(
    State(kubri): State<Arc<Kubri>>,
    Json(subject): Json<ExplainSubject>,
) -> ApiResult<ExplainPrompt> {
    let mut prompt = kubri.explain_prompt(&subject).await?;
    prompt.has_key = kubri_core::explain::read_key_file().is_some();
    Ok(Json(prompt))
}

#[derive(Deserialize)]
struct Explain {
    /// The prompt's text, as the user saw it.
    text: String,
}

async fn explain(State(kubri): State<Arc<Kubri>>, Json(req): Json<Explain>) -> ApiResult<ExplainAnswer> {
    let key = kubri_core::explain::read_key_file();
    Ok(Json(kubri.explain(&req.text, key.as_deref()).await?))
}

async fn guard(State(kubri): State<Arc<Kubri>>) -> ApiResult<Guard> {
    Ok(Json(kubri.guard().await))
}

async fn unlock(State(kubri): State<Arc<Kubri>>) -> ApiResult<Guard> {
    Ok(Json(kubri.unlock().await?))
}

async fn lock(State(kubri): State<Arc<Kubri>>) -> ApiResult<Guard> {
    Ok(Json(kubri.lock().await?))
}

async fn schema(State(kubri): State<Arc<Kubri>>, Query(resource): Query<ResourceRef>) -> ApiResult<Value> {
    Ok(Json(
        kubri
            .schema(&resource)
            .await?
            .as_ref()
            .clone()
            .unwrap_or(Value::Null),
    ))
}

#[derive(Deserialize)]
struct AccessRequest {
    checks: Vec<AccessCheck>,
    #[serde(default)]
    fresh: bool,
}

async fn access(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<AccessRequest>,
) -> ApiResult<Vec<Option<bool>>> {
    Ok(Json(kubri.access(&req.checks, req.fresh).await?))
}

async fn who_am_i(State(kubri): State<Arc<Kubri>>) -> ApiResult<Option<Identity>> {
    Ok(Json(kubri.who_am_i().await?))
}

async fn readable_namespaces(State(kubri): State<Arc<Kubri>>) -> ApiResult<Vec<String>> {
    Ok(Json(kubri.readable_namespaces().await?))
}

async fn resources(State(kubri): State<Arc<Kubri>>) -> ApiResult<Vec<ResourceInfo>> {
    Ok(Json(kubri.resources().await?))
}

#[derive(Deserialize)]
struct TableQuery {
    #[serde(flatten)]
    resource: ResourceRef,
    namespace: Option<String>,
    #[serde(flatten)]
    scope: TableScope,
}

/// Streams [`kubri_core::TableEvent`]s as JSON text frames until the client disconnects.
async fn table(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<TableQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    let events = kubri
        .watch_table(&query.resource, namespace.as_deref(), query.scope)
        .await?;
    Ok(ws.on_upgrade(move |socket| forward(socket, events)))
}

/// Proxies close sockets that stay quiet, often after a minute; a ping every 30 seconds
/// keeps them open.
fn keepalive() -> tokio::time::Interval {
    const EVERY: Duration = Duration::from_secs(30);
    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + EVERY, EVERY);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval
}

async fn forward<T: Serialize>(socket: WebSocket, events: impl Stream<Item = T> + Send + 'static) {
    let (mut tx, mut rx) = socket.split();
    let mut events = std::pin::pin!(events);
    let mut ping = keepalive();
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if tx.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
            event = events.next() => {
                let Some(event) = event else { break };
                let text = match serde_json::to_string(&event) {
                    Ok(text) => text,
                    Err(err) => {
                        tracing::warn!(%err, "failed to serialize event");
                        continue;
                    }
                };
                if tx.send(Message::text(text)).await.is_err() {
                    break;
                }
            }
            msg = rx.next() => match msg {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

#[derive(Deserialize)]
struct ObjectQuery {
    context: Option<String>,
    #[serde(flatten)]
    resource: ResourceRef,
    namespace: Option<String>,
    name: String,
}

async fn get_object(State(kubri): State<Arc<Kubri>>, Query(query): Query<ObjectQuery>) -> ApiResult<Value> {
    let target = ObjectRef {
        context: query.context,
        resource: query.resource,
        namespace: query.namespace.filter(|ns| !ns.is_empty()),
        name: query.name,
    };
    Ok(Json(kubri.get(&target).await?))
}

async fn get_object_elsewhere(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<ObjectQuery>,
) -> ApiResult<Value> {
    let target = ObjectRef {
        context: query.context,
        resource: query.resource,
        namespace: query.namespace.filter(|ns| !ns.is_empty()),
        name: query.name,
    };
    Ok(Json(kubri.get_elsewhere(&target).await?))
}

#[derive(Deserialize)]
struct Replace {
    target: ObjectRef,
    value: Value,
}

async fn replace_object(State(kubri): State<Arc<Kubri>>, Json(req): Json<Replace>) -> ApiResult<Value> {
    Ok(Json(kubri.replace(&req.target, req.value).await?))
}

#[derive(Deserialize)]
struct PatchRequest {
    target: ObjectRef,
    patch: Value,
}

async fn patch_object(State(kubri): State<Arc<Kubri>>, Json(req): Json<PatchRequest>) -> ApiResult<Value> {
    Ok(Json(kubri.patch(&req.target, req.patch).await?))
}

#[derive(Deserialize)]
struct Delete {
    target: ObjectRef,
    #[serde(default)]
    options: DeleteOptions,
}

async fn delete_object(State(kubri): State<Arc<Kubri>>, Json(req): Json<Delete>) -> ApiResult<()> {
    Ok(Json(kubri.delete(&req.target, &req.options).await?))
}

#[derive(Deserialize)]
struct PreviewRequest {
    target: ObjectRef,
    change: Change,
}

async fn preview(State(kubri): State<Arc<Kubri>>, Json(req): Json<PreviewRequest>) -> ApiResult<Preview> {
    Ok(Json(kubri.preview(&req.target, req.change).await?))
}

#[derive(Deserialize)]
struct Scale {
    target: ObjectRef,
    replicas: u32,
}

async fn scale(State(kubri): State<Arc<Kubri>>, Json(req): Json<Scale>) -> ApiResult<()> {
    Ok(Json(kubri.scale(&req.target, req.replicas).await?))
}

#[derive(Deserialize)]
struct Target {
    target: ObjectRef,
}

async fn debug(State(kubri): State<Arc<Kubri>>, Json(req): Json<DebugRequest>) -> ApiResult<DebugSession> {
    Ok(Json(kubri.debug(req).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryRequest {
    target: ObjectRef,
    container: String,
    #[serde(default)]
    memory: String,
}

async fn memory_fix(State(kubri): State<Arc<Kubri>>, Json(req): Json<MemoryRequest>) -> ApiResult<MemoryFix> {
    Ok(Json(kubri.memory_fix(&req.target, &req.container).await?))
}

async fn set_memory_limit(State(kubri): State<Arc<Kubri>>, Json(req): Json<MemoryRequest>) -> ApiResult<()> {
    Ok(Json(
        kubri
            .set_memory_limit(&req.target, &req.container, &req.memory)
            .await?,
    ))
}

async fn scheduling(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<Vec<NodeFit>> {
    Ok(Json(kubri.scheduling(&req.target).await?))
}

async fn service_check(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<ServiceCheck> {
    Ok(Json(kubri.service_check(&req.target).await?))
}

async fn apply(State(kubri): State<Arc<Kubri>>, Json(req): Json<ApplyRequest>) -> ApiResult<Vec<ApplyItem>> {
    Ok(Json(kubri.apply(req).await?))
}

#[derive(Deserialize)]
struct UrlRequest {
    url: String,
}

async fn fetch_manifest(Json(req): Json<UrlRequest>) -> ApiResult<String> {
    Ok(Json(kubri_core::fetch_manifest(&req.url).await?))
}

async fn restart(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<()> {
    Ok(Json(kubri.restart(&req.target).await?))
}

async fn drill(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<Option<Drill>> {
    Ok(Json(kubri.drill(&req.target).await?))
}

async fn used_by(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<Vec<Reference>> {
    Ok(Json(kubri.used_by(&req.target).await?))
}

#[derive(Serialize)]
struct Created {
    name: String,
}

async fn trigger_cronjob(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<Created> {
    let name = kubri.trigger_cronjob(&req.target).await?;
    Ok(Json(Created { name }))
}

#[derive(Deserialize)]
struct Suspend {
    target: ObjectRef,
    suspend: bool,
}

async fn suspend_cronjob(State(kubri): State<Arc<Kubri>>, Json(req): Json<Suspend>) -> ApiResult<()> {
    Ok(Json(kubri.suspend_cronjob(&req.target, req.suspend).await?))
}

#[derive(Deserialize)]
struct GitOps {
    target: ObjectRef,
    action: GitOpsAction,
}

async fn gitops(State(kubri): State<Arc<Kubri>>, Json(req): Json<GitOps>) -> ApiResult<()> {
    Ok(Json(kubri.gitops(&req.target, req.action).await?))
}

async fn right_sizing(
    State(kubri): State<Arc<Kubri>>,
    Query(q): Query<NamespaceQuery>,
) -> ApiResult<RightSizing> {
    Ok(Json(kubri.right_sizing(q.namespace()).await?))
}

#[derive(Deserialize)]
struct SetResources {
    target: ObjectRef,
    container: String,
    #[serde(default)]
    requests: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    limits: std::collections::BTreeMap<String, String>,
}

async fn set_resources(State(kubri): State<Arc<Kubri>>, Json(req): Json<SetResources>) -> ApiResult<()> {
    Ok(Json(
        kubri
            .set_resources(&req.target, &req.container, &req.requests, &req.limits)
            .await?,
    ))
}

async fn renew_certificate(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<()> {
    Ok(Json(kubri.renew_certificate(&req.target).await?))
}

async fn managed_by(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<Option<Manager>> {
    Ok(Json(kubri.managed_by(&req.target).await?))
}

#[derive(Deserialize)]
struct SetImages {
    target: ObjectRef,
    images: Vec<ContainerImage>,
}

async fn set_images(State(kubri): State<Arc<Kubri>>, Json(req): Json<SetImages>) -> ApiResult<()> {
    Ok(Json(kubri.set_images(&req.target, &req.images).await?))
}

async fn rollback(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<Created> {
    let name = kubri.rollback(&req.target).await?;
    Ok(Json(Created { name }))
}

#[derive(Deserialize)]
struct EventsQuery {
    namespace: Option<String>,
    uid: String,
}

async fn events(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<EventsQuery>,
) -> ApiResult<Vec<EventInfo>> {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    Ok(Json(kubri.events(namespace.as_deref(), &query.uid).await?))
}

#[derive(Deserialize)]
struct MetricsQuery {
    kind: MetricsKind,
    namespace: Option<String>,
}

async fn metrics(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<MetricsQuery>,
) -> ApiResult<Vec<Usage>> {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    Ok(Json(kubri.metrics(query.kind, namespace.as_deref()).await?))
}

async fn search(
    State(kubri): State<Arc<Kubri>>,
    Json(request): Json<SearchRequest>,
) -> ApiResult<SearchResult> {
    Ok(Json(kubri.search(&request).await?))
}

#[derive(Deserialize)]
struct FleetQuery {
    context: String,
}

async fn fleet(State(kubri): State<Arc<Kubri>>, Query(query): Query<FleetQuery>) -> ApiResult<FleetCluster> {
    Ok(Json(kubri.fleet_cluster(&query.context).await?))
}

#[derive(Deserialize)]
struct PulseQuery {
    namespace: Option<String>,
}

async fn pulse(State(kubri): State<Arc<Kubri>>, Query(query): Query<PulseQuery>) -> ApiResult<Pulse> {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    Ok(Json(kubri.pulse(namespace.as_deref()).await?))
}

#[derive(Deserialize)]
struct IntegrationsQuery {
    #[serde(default)]
    fresh: bool,
}

async fn integrations(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<IntegrationsQuery>,
) -> ApiResult<Integrations> {
    Ok(Json(kubri.integrations(query.fresh).await?))
}

#[derive(Deserialize)]
struct ChartsRequest {
    target: ObjectRef,
    range: Range,
}

async fn charts(State(kubri): State<Arc<Kubri>>, Json(req): Json<ChartsRequest>) -> ApiResult<Charts> {
    Ok(Json(kubri.charts(&req.target, req.range).await?))
}

#[derive(Deserialize)]
struct HistoryQuery {
    namespace: Option<String>,
    seconds: u64,
}

async fn usage_history(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Charts> {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    let range = Range {
        seconds: query.seconds,
        end: None,
    };
    Ok(Json(kubri.usage_history(namespace.as_deref(), range).await?))
}

#[derive(Deserialize)]
struct PromQuery {
    query: String,
    range: Range,
}

async fn prometheus_query(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<PromQuery>,
) -> ApiResult<Vec<Series>> {
    Ok(Json(kubri.prometheus_query(&req.query, req.range).await?))
}

async fn dashboards(State(kubri): State<Arc<Kubri>>, Json(req): Json<Target>) -> ApiResult<Dashboards> {
    Ok(Json(kubri.dashboards(&req.target).await?))
}

#[derive(Deserialize)]
struct IntegrationId {
    id: String,
}

#[derive(Serialize)]
struct Url {
    url: String,
}

async fn integration_url(State(kubri): State<Arc<Kubri>>, Json(req): Json<IntegrationId>) -> ApiResult<Url> {
    Ok(Json(Url {
        url: kubri.integration_url(&req.id).await?,
    }))
}

async fn loki_logs(State(kubri): State<Arc<Kubri>>, Json(req): Json<LokiRequest>) -> ApiResult<LokiLogs> {
    Ok(Json(kubri.loki_logs(&req).await?))
}

#[derive(Deserialize)]
struct XrayQuery {
    #[serde(flatten)]
    resource: ResourceRef,
    namespace: Option<String>,
}

async fn xray(State(kubri): State<Arc<Kubri>>, Query(query): Query<XrayQuery>) -> ApiResult<Vec<XrayNode>> {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    Ok(Json(kubri.xray(&query.resource, namespace.as_deref()).await?))
}

#[derive(Deserialize)]
struct NamespaceQuery {
    namespace: Option<String>,
}

impl NamespaceQuery {
    fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref().filter(|ns| !ns.is_empty())
    }
}

async fn sanitize(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<NamespaceQuery>,
) -> ApiResult<SanitizerReport> {
    Ok(Json(kubri.sanitize(query.namespace()).await?))
}

async fn rbac(State(kubri): State<Arc<Kubri>>, Query(query): Query<NamespaceQuery>) -> ApiResult<Rbac> {
    Ok(Json(kubri.rbac(query.namespace()).await?))
}

async fn permissions(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<NamespaceQuery>,
) -> ApiResult<Permissions> {
    Ok(Json(kubri.permissions(query.namespace()).await?))
}

async fn helm_releases(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<NamespaceQuery>,
) -> ApiResult<Vec<HelmRelease>> {
    Ok(Json(kubri.helm_releases(query.namespace()).await?))
}

#[derive(Deserialize)]
struct ReleaseQuery {
    namespace: String,
    name: String,
    revision: Option<u32>,
}

async fn helm_release(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<ReleaseQuery>,
) -> ApiResult<HelmReleaseDetails> {
    Ok(Json(
        kubri
            .helm_release(&query.namespace, &query.name, query.revision)
            .await?,
    ))
}

async fn helm_history(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<ReleaseQuery>,
) -> ApiResult<Vec<HelmRelease>> {
    Ok(Json(kubri.helm_history(&query.namespace, &query.name).await?))
}

#[derive(Serialize)]
struct HelmVersion {
    version: Option<String>,
}

async fn helm_version(State(kubri): State<Arc<Kubri>>) -> Json<HelmVersion> {
    Json(HelmVersion {
        version: kubri.helm_version().await,
    })
}

async fn helm_change(
    State(kubri): State<Arc<Kubri>>,
    Json(change): Json<HelmChange>,
) -> ApiResult<HelmOutcome> {
    Ok(Json(kubri.helm_change(&change).await?))
}

async fn helm_chart_values(
    State(kubri): State<Arc<Kubri>>,
    Json(chart): Json<HelmChartRef>,
) -> ApiResult<String> {
    Ok(Json(kubri.helm_chart_values(&chart).await?))
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String,
}

async fn helm_search(
    State(kubri): State<Arc<Kubri>>,
    Query(search): Query<SearchQuery>,
) -> ApiResult<Vec<HubChart>> {
    Ok(Json(kubri.helm_search(&search.query).await?))
}

#[derive(Deserialize)]
struct ReleaseAction {
    namespace: String,
    name: String,
    revision: Option<u32>,
}

async fn helm_rollback(State(kubri): State<Arc<Kubri>>, Json(req): Json<ReleaseAction>) -> ApiResult<String> {
    let revision = req
        .revision
        .ok_or_else(|| kubri_core::Error::Unsupported("a revision is required".into()))?;
    Ok(Json(
        kubri.helm_rollback(&req.namespace, &req.name, revision).await?,
    ))
}

async fn helm_uninstall(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<ReleaseAction>,
) -> ApiResult<String> {
    Ok(Json(kubri.helm_uninstall(&req.namespace, &req.name).await?))
}

/// Reads the JSON request a streaming socket starts with. Parameters go in a message rather
/// than the query string because they nest and include numbers and flags.
async fn first_message<T: DeserializeOwned>(socket: &mut WebSocket) -> Result<T, String> {
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text).map_err(|e| format!("invalid request: {e}"));
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            _ => return Err("expected a request".to_owned()),
        }
    }
}

async fn send_json<T: Serialize>(socket: &mut WebSocket, value: &T) {
    if let Ok(text) = serde_json::to_string(value) {
        let _ = socket.send(Message::text(text)).await;
    }
}

#[derive(Deserialize)]
struct LogsRequest {
    target: ObjectRef,
    #[serde(default)]
    options: LogOptions,
}

/// Streams [`LogEvent`]s as JSON text frames. The client sends a [`LogsRequest`] first.
async fn logs(State(kubri): State<Arc<Kubri>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let events = match first_message::<LogsRequest>(&mut socket).await {
            Ok(req) => kubri
                .logs(&req.target, req.options)
                .await
                .map_err(|e| e.to_string()),
            Err(err) => Err(err),
        };
        match events {
            Ok(events) => forward(socket, events).await,
            Err(message) => {
                send_json(
                    &mut socket,
                    &LogEvent::Error {
                        source: None,
                        message,
                    },
                )
                .await;
                send_json(&mut socket, &LogEvent::End).await;
            }
        }
    })
}

#[derive(Deserialize)]
struct TerminalRequest {
    target: ObjectRef,
    #[serde(default)]
    options: TerminalOptions,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum TerminalControl {
    Resize { cols: u16, rows: u16 },
}

/// A terminal session. The client sends a [`TerminalRequest`] first, then keystrokes as
/// binary frames and `{"type":"resize"}` as text. The server answers `{"type":"ready"}`,
/// then output as binary frames, and `{"type":"exit"}` when the process ends.
async fn terminal(State(kubri): State<Arc<Kubri>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let opened = match first_message::<TerminalRequest>(&mut socket).await {
            Ok(req) => kubri
                .open_terminal(&req.target, req.options)
                .await
                .map_err(|e| e.to_string()),
            Err(err) => Err(err),
        };
        run_terminal(socket, opened).await;
    })
}

async fn k9s_config(State(kubri): State<Arc<Kubri>>) -> Json<K9sConfig> {
    Json(kubri.k9s_config().await)
}

async fn k9s_skin(State(kubri): State<Arc<Kubri>>, Path(name): Path<String>) -> ApiResult<Value> {
    Ok(Json(kubri.k9s_skin(&name).await?))
}

/// Starts a background plugin; Kubri does not wait for it to finish.
async fn run_plugin(State(kubri): State<Arc<Kubri>>, Json(run): Json<PluginRun>) -> ApiResult<()> {
    Ok(Json(kubri.run_plugin(&run).await?))
}

/// A plugin in a terminal: the client sends a [`PluginRun`] first, then it works like
/// [`terminal`].
async fn plugin_terminal(State(kubri): State<Arc<Kubri>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let opened = match first_message::<PluginRun>(&mut socket).await {
            Ok(run) => kubri.open_plugin(&run).await.map_err(|e| e.to_string()),
            Err(err) => Err(err),
        };
        run_terminal(socket, opened).await;
    })
}

/// Connects the socket to the terminal, or tells why it could not be opened.
async fn run_terminal(mut socket: WebSocket, opened: Result<Terminal, String>) {
    let terminal = match opened {
        Ok(terminal) => terminal,
        Err(message) => {
            let exit = TerminalOutput::Exit {
                message: Some(message),
            };
            send_json(&mut socket, &exit).await;
            return;
        }
    };
    let Terminal {
        container,
        mut input,
        mut output,
    } = terminal;
    send_json(
        &mut socket,
        &serde_json::json!({ "type": "ready", "container": container }),
    )
    .await;
    let (mut tx, mut rx) = socket.split();
    // Input and output run side by side, so a blocked stdin never stalls the output.
    let keys = async move {
        while let Some(Ok(msg)) = rx.next().await {
            match msg {
                Message::Binary(data) => {
                    if input.write(&data).await.is_err() {
                        break;
                    }
                }
                Message::Text(text) => match serde_json::from_str(&text) {
                    Ok(TerminalControl::Resize { cols, rows }) => input.resize(cols, rows),
                    Err(err) => tracing::debug!(%err, "ignoring terminal message"),
                },
                Message::Close(_) => break,
                _ => {}
            }
        }
    };
    let screen = async move {
        let mut ping = keepalive();
        loop {
            let out = tokio::select! {
                out = output.next() => out,
                _ = ping.tick() => {
                    if tx.send(Message::Ping(Bytes::new())).await.is_err() {
                        break;
                    }
                    continue;
                }
            };
            let Some(out) = out else { break };
            let msg = match &out {
                TerminalOutput::Data { data } => Message::binary(data.clone()),
                TerminalOutput::Exit { .. } => match serde_json::to_string(&out) {
                    Ok(text) => Message::text(text),
                    Err(_) => break,
                },
            };
            let exit = matches!(out, TerminalOutput::Exit { .. });
            if tx.send(msg).await.is_err() || exit {
                break;
            }
        }
        let _ = tx.close().await;
    };
    tokio::select! {
        _ = keys => {}
        _ = screen => {}
    }
}

async fn port_forwards(State(kubri): State<Arc<Kubri>>) -> Json<Vec<PortForwardInfo>> {
    Json(kubri.port_forwards())
}

async fn start_port_forward(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<PortForwardRequest>,
) -> ApiResult<PortForwardInfo> {
    Ok(Json(kubri.start_port_forward(req).await?))
}

async fn start_favorite_forwards(
    State(kubri): State<Arc<Kubri>>,
) -> ApiResult<Vec<kubri_core::FavoriteStart>> {
    Ok(Json(kubri.start_favorite_forwards().await?))
}

async fn stop_port_forward(State(kubri): State<Arc<Kubri>>, Path(id): Path<u32>) -> ApiResult<()> {
    Ok(Json(kubri.stop_port_forward(id)?))
}

async fn benchmarks(State(kubri): State<Arc<Kubri>>) -> Json<Vec<BenchInfo>> {
    Json(kubri.benchmarks())
}

async fn start_benchmark(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<BenchRequest>,
) -> ApiResult<BenchInfo> {
    Ok(Json(kubri.start_benchmark(&req)?))
}

async fn stop_benchmark(State(kubri): State<Arc<Kubri>>, Path(id): Path<u32>) -> ApiResult<()> {
    Ok(Json(kubri.stop_benchmark(id)?))
}

async fn remove_benchmark(State(kubri): State<Arc<Kubri>>, Path(id): Path<u32>) -> ApiResult<()> {
    Ok(Json(kubri.remove_benchmark(id)?))
}

#[derive(Deserialize)]
struct Cordon {
    name: String,
    cordoned: bool,
}

async fn cordon(State(kubri): State<Arc<Kubri>>, Json(req): Json<Cordon>) -> ApiResult<()> {
    Ok(Json(kubri.cordon(&req.name, req.cordoned).await?))
}

#[derive(Deserialize)]
struct Drain {
    name: String,
    #[serde(default)]
    options: DrainOptions,
}

async fn drain(State(kubri): State<Arc<Kubri>>, Json(req): Json<Drain>) -> ApiResult<DrainReport> {
    Ok(Json(kubri.drain(&req.name, req.options).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn send(router: Router, request: Request<Body>) -> (StatusCode, Value) {
        let response = router.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    fn get(uri: &str) -> Request<Body> {
        Request::get(uri).body(Body::empty()).unwrap()
    }

    fn disconnected() -> Router {
        router(Arc::new(Kubri::from_kubeconfig(Default::default())))
    }

    #[tokio::test]
    async fn lists_contexts_from_kubeconfig() {
        let kubeconfig = kube::config::Kubeconfig::from_yaml(
            "contexts: [{name: kind, context: {cluster: kind, user: kind}}]\ncurrent-context: kind",
        )
        .unwrap();
        let router = router(Arc::new(Kubri::from_kubeconfig(kubeconfig)));
        let (status, body) = send(router, get("/contexts")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["contexts"][0]["name"], "kind");
        assert_eq!(body["contexts"][0]["isDefault"], true);
        assert!(body["cluster"].is_null());
    }

    #[test]
    fn cluster_errors_keep_their_status_except_401() {
        let status = |code| {
            let status = kube::core::Status::failure("", "").with_code(code).boxed();
            ApiError(kube::Error::Api(status).into()).into_response().status()
        };
        assert_eq!(status(403), StatusCode::FORBIDDEN);
        assert_eq!(status(422), StatusCode::UNPROCESSABLE_ENTITY);
        // 401 tells the UI to sign in to Kubri again, which would not help.
        assert_eq!(status(401), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn resources_require_a_connection() {
        let (status, body) = send(disconnected(), get("/resources")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("not connected"));
    }

    #[tokio::test]
    async fn object_routes_parse_their_parameters() {
        let (status, _) = send(
            disconnected(),
            get("/object?group=apps&version=v1&plural=deployments&namespace=shop&name=web"),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);

        let scale = Request::post("/object/scale")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"target":{"resource":{"group":"apps","version":"v1","plural":"deployments"},
                    "namespace":"shop","name":"web"},"replicas":3}"#,
            ))
            .unwrap();
        let (status, _) = send(disconnected(), scale).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let (status, _) = send(disconnected(), get("/metrics?kind=pods")).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn rbac_and_helm_routes_parse_their_parameters() {
        for uri in [
            "/rbac?namespace=shop",
            "/permissions",
            "/helm/releases?namespace=",
            "/helm/release?namespace=apps&name=shop&revision=2",
            "/helm/history?namespace=apps&name=shop",
        ] {
            let (status, _) = send(disconnected(), get(uri)).await;
            assert_eq!(status, StatusCode::CONFLICT, "{uri}");
        }
        let (status, _) = send(disconnected(), get("/helm/release?name=shop")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let rollback = post_json(
            "/helm/rollback",
            r#"{"namespace":"apps","name":"shop","revision":1}"#,
        );
        let (status, _) = send(disconnected(), rollback).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, body) = send(disconnected(), get("/helm/version")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.get("version").is_some());
    }

    #[tokio::test]
    async fn overview_routes_parse_their_parameters() {
        let (status, _) = send(disconnected(), get("/pulse?namespace=")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, _) = send(
            disconnected(),
            get("/xray?group=apps&version=v1&plural=deployments&namespace=shop"),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, _) = send(disconnected(), get("/xray?namespace=shop")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    fn post_json(uri: &str, body: &str) -> Request<Body> {
        Request::post(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn read_only_turns_changes_away() {
        let kubri = Arc::new(Kubri::from_kubeconfig(Default::default()));
        kubri.set_read_only();
        let target = r#"{"target":{"resource":{"group":"batch","version":"v1","plural":"cronjobs"},
            "namespace":"ops","name":"backup"}"#;
        let changes = [
            ("/object/restart", format!("{target}}}")),
            ("/cronjob/trigger", format!("{target}}}")),
            ("/debug", r#"{"kind":"node","node":"n1"}"#.to_owned()),
            ("/apply", r#"{"objects":[]}"#.to_owned()),
            (
                "/helm/change",
                r#"{"namespace":"web","name":"shop","values":""}"#.to_owned(),
            ),
            (
                "/fix/memory/apply",
                format!(r#"{target},"container":"app","memory":"1Gi"}}"#),
            ),
            ("/cronjob/suspend", format!(r#"{target},"suspend":true}}"#)),
            ("/gitops", format!(r#"{target},"action":"reconcile"}}"#)),
            ("/certificate/renew", format!("{target}}}")),
            (
                "/object/resources",
                format!(r#"{target},"container":"app","requests":{{"cpu":"100m"}}}}"#),
            ),
            ("/replicaset/rollback", format!("{target}}}")),
            (
                "/object/images",
                format!(r#"{target},"images":[{{"name":"app","image":"web:2"}}]}}"#),
            ),
        ];
        for (uri, body) in changes {
            let (status, body) = send(router(kubri.clone()), post_json(uri, &body)).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
            assert!(
                body["error"].as_str().unwrap().contains("read-only"),
                "{uri}: {body}"
            );
        }
        // Looking around still works, until it needs the missing connection.
        for uri in ["/object/drill", "/object/used-by"] {
            let (status, _) = send(router(kubri.clone()), post_json(uri, &format!("{target}}}"))).await;
            assert_eq!(status, StatusCode::CONFLICT, "{uri}");
        }
    }

    #[test]
    fn table_queries_take_selectors() {
        let uri: axum::http::Uri =
            "/table?group=&version=v1&plural=pods&namespace=shop&labelSelector=app%3Dweb&fieldSelector=spec.nodeName%3Dn1"
                .parse()
                .unwrap();
        let Query(query) = Query::<TableQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.resource.plural, "pods");
        assert_eq!(query.scope.label_selector.as_deref(), Some("app=web"));
        assert_eq!(query.scope.field_selector.as_deref(), Some("spec.nodeName=n1"));
        let uri: axum::http::Uri = "/table?group=apps&version=v1&plural=deployments".parse().unwrap();
        let Query(query) = Query::<TableQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.scope, TableScope::default());
    }

    #[tokio::test]
    async fn port_forward_routes() {
        let (status, body) = send(disconnected(), get("/portforwards")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, serde_json::json!([]));

        let start = post_json(
            "/portforwards",
            r#"{"target":{"resource":{"group":"","version":"v1","plural":"pods"},
                "namespace":"shop","name":"web-1"},"remotePort":8080}"#,
        );
        let (status, _) = send(disconnected(), start).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let stop = Request::delete("/portforwards/7").body(Body::empty()).unwrap();
        let (status, body) = send(disconnected(), stop).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "port-forward 7 does not exist");
    }

    #[tokio::test]
    async fn sanitizer_and_benchmark_routes() {
        let (status, _) = send(disconnected(), get("/sanitize?namespace=shop")).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let (status, body) = send(disconnected(), get("/benchmarks")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, serde_json::json!([]));
        let start = post_json(
            "/benchmarks",
            r#"{"forwardId":3,"concurrency":2,"requests":100,"headers":{"Host":"shop.acme.io"}}"#,
        );
        let (status, body) = send(disconnected(), start).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "port-forward 3 does not exist");
        let stop = Request::post("/benchmarks/4/stop").body(Body::empty()).unwrap();
        let (status, _) = send(disconnected(), stop).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let remove = Request::delete("/benchmarks/4").body(Body::empty()).unwrap();
        let (status, _) = send(disconnected(), remove).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn k9s_and_plugin_routes() {
        let (status, body) = send(disconnected(), get("/k9s")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["plugins"].is_array());
        let (status, _) = send(disconnected(), get("/k9s/skins/..%2Fconfig")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let run = post_json(
            "/plugins/run",
            r#"{"plugin":"fred","target":{"resource":{"group":"","version":"v1","plural":"pods"},"namespace":"shop","name":"web"}}"#,
        );
        let (status, _) = send(disconnected(), run).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn node_routes_parse_their_parameters() {
        let cordon = post_json("/node/cordon", r#"{"name":"worker-1","cordoned":true}"#);
        let (status, _) = send(disconnected(), cordon).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let drain = post_json(
            "/node/drain",
            r#"{"name":"worker-1","options":{"force":true,"gracePeriodSeconds":30}}"#,
        );
        let (status, _) = send(disconnected(), drain).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }
}
