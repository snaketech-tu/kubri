//! The desktop app: the shared UI in a native window, backed by kubri-core through Tauri commands.

mod agents;
mod explain;
#[cfg(unix)]
mod shell_env;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::{Stream, StreamExt};
use kubri_core::{
    AccessCheck, BenchInfo, BenchRequest, Change, ClusterInfo, ContainerImage, ContextList, ContextSettings,
    DebugRequest, DebugSession, DeleteOptions, DiffSettings, DrainOptions, DrainReport, Drill, EventInfo,
    Guard, HelmRelease, HelmReleaseDetails, Identity, K9sConfig, Kubri, LogEvent, LogOptions, MetricsKind,
    ObjectRef, Permissions, PluginRun, PortForwardInfo, PortForwardRequest, Preview, Pulse, Rbac, Reference,
    ResourceInfo, ResourceRef, SanitizerReport, Settings, TableEvent, TableScope, Terminal, TerminalInput,
    TerminalOptions, TerminalOutput, Usage, XrayNode,
};
use kubri_core::{ApplyItem, ApplyRequest, MemoryFix, NodeFit, ServiceCheck};
use kubri_core::{HelmChange, HelmChartRef, HelmOutcome, HubChart};
use serde_json::Value;
use tauri::State;
use tauri::async_runtime::JoinHandle;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tracing_subscriber::EnvFilter;

/// Running table watches and log streams, so the UI can stop them when it navigates away.
#[derive(Default)]
struct Streams {
    next_id: AtomicU32,
    tasks: Mutex<HashMap<u32, JoinHandle<()>>>,
}

impl Streams {
    /// Forwards `events` to the UI until the stream ends, the UI goes away or [`unwatch`] is called.
    fn spawn<T>(self: &Arc<Self>, events: impl Stream<Item = T> + Send + 'static, channel: Channel<T>) -> u32
    where
        T: IpcResponse + Send + 'static,
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let registry = Arc::clone(self);
        // Hold the lock while spawning so the task cannot remove itself before it is added.
        let mut tasks = self.tasks.lock().unwrap();
        let task = tauri::async_runtime::spawn(async move {
            let mut events = std::pin::pin!(events);
            while let Some(event) = events.next().await {
                if channel.send(event).is_err() {
                    break;
                }
            }
            registry.tasks.lock().unwrap().remove(&id);
        });
        tasks.insert(id, task);
        id
    }
}

/// Open terminals: their input, and the task that sends their output to the UI.
#[derive(Default)]
struct Terminals {
    next_id: AtomicU32,
    sessions: Mutex<HashMap<u32, TerminalSession>>,
}

struct TerminalSession {
    input: Arc<tokio::sync::Mutex<TerminalInput>>,
    task: JoinHandle<()>,
}

impl Terminals {
    fn input(&self, id: u32) -> CommandResult<Arc<tokio::sync::Mutex<TerminalInput>>> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(&id).ok_or("the terminal is closed")?;
        Ok(Arc::clone(&session.input))
    }
}

type CommandResult<T> = Result<T, String>;

#[tauri::command]
async fn contexts(kubri: State<'_, Arc<Kubri>>) -> CommandResult<ContextList> {
    kubri.contexts().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn reload_contexts(kubri: State<'_, Arc<Kubri>>) -> CommandResult<ContextList> {
    kubri.reload().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn switch_context(kubri: State<'_, Arc<Kubri>>, name: String) -> CommandResult<ClusterInfo> {
    kubri.switch_context(&name).await.map_err(|e| e.to_string())
}

/// The settings, and why the file could not be read if it could not.
#[derive(serde::Serialize)]
struct SettingsReply {
    settings: Settings,
    error: Option<String>,
}

#[tauri::command]
fn settings(kubri: State<'_, Arc<Kubri>>) -> SettingsReply {
    SettingsReply {
        settings: kubri.settings(),
        error: kubri.settings_error(),
    }
}

#[tauri::command]
fn set_context_settings(
    kubri: State<'_, Arc<Kubri>>,
    name: String,
    settings: ContextSettings,
) -> CommandResult<Settings> {
    kubri
        .set_context_settings(&name, settings)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_diff_settings(kubri: State<'_, Arc<Kubri>>, settings: DiffSettings) -> CommandResult<Settings> {
    kubri.set_diff_settings(settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_column_settings(
    kubri: State<'_, Arc<Kubri>>,
    kind: String,
    columns: kubri_core::ColumnSettings,
) -> CommandResult<Settings> {
    kubri
        .set_column_settings(&kind, columns)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn guard(kubri: State<'_, Arc<Kubri>>) -> CommandResult<Guard> {
    Ok(kubri.guard().await)
}

#[tauri::command]
async fn unlock(kubri: State<'_, Arc<Kubri>>) -> CommandResult<Guard> {
    kubri.unlock().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn lock(kubri: State<'_, Arc<Kubri>>) -> CommandResult<Guard> {
    kubri.lock().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn preview(kubri: State<'_, Arc<Kubri>>, target: ObjectRef, change: Change) -> CommandResult<Preview> {
    kubri.preview(&target, change).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn schema(kubri: State<'_, Arc<Kubri>>, resource: ResourceRef) -> CommandResult<Value> {
    let found = kubri.schema(&resource).await.map_err(|e| e.to_string())?;
    Ok(found.as_ref().clone().unwrap_or(Value::Null))
}

#[tauri::command]
async fn access(
    kubri: State<'_, Arc<Kubri>>,
    checks: Vec<AccessCheck>,
    fresh: bool,
) -> CommandResult<Vec<Option<bool>>> {
    kubri.access(&checks, fresh).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn who_am_i(kubri: State<'_, Arc<Kubri>>) -> CommandResult<Option<Identity>> {
    kubri.who_am_i().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn readable_namespaces(kubri: State<'_, Arc<Kubri>>) -> CommandResult<Vec<String>> {
    kubri.readable_namespaces().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn resources(kubri: State<'_, Arc<Kubri>>) -> CommandResult<Vec<ResourceInfo>> {
    kubri.resources().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn watch_table(
    kubri: State<'_, Arc<Kubri>>,
    streams: State<'_, Arc<Streams>>,
    resource: ResourceRef,
    namespace: Option<String>,
    scope: Option<TableScope>,
    on_event: Channel<TableEvent>,
) -> CommandResult<u32> {
    let events = kubri
        .watch_table(&resource, namespace.as_deref(), scope.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())?;
    Ok(streams.spawn(events, on_event))
}

#[tauri::command]
async fn logs(
    kubri: State<'_, Arc<Kubri>>,
    streams: State<'_, Arc<Streams>>,
    target: ObjectRef,
    options: Option<LogOptions>,
    on_event: Channel<LogEvent>,
) -> CommandResult<u32> {
    let events = kubri
        .logs(&target, options.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())?;
    Ok(streams.spawn(events, on_event))
}

/// Stops a table watch or log stream.
#[tauri::command]
fn unwatch(streams: State<'_, Arc<Streams>>, id: u32) {
    if let Some(task) = streams.tasks.lock().unwrap().remove(&id) {
        task.abort();
    }
}

/// Opens a terminal. The channel receives `{"type":"ready"}`, then output as raw bytes,
/// then `{"type":"exit"}`.
#[tauri::command]
async fn terminal_open(
    kubri: State<'_, Arc<Kubri>>,
    terminals: State<'_, Arc<Terminals>>,
    target: ObjectRef,
    options: Option<TerminalOptions>,
    on_event: Channel<InvokeResponseBody>,
) -> CommandResult<u32> {
    let terminal = kubri
        .open_terminal(&target, options.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())?;
    Ok(register(&terminals, terminal, on_event))
}

/// Runs a k9s plugin in a terminal; it is written to and closed like [`terminal_open`].
#[tauri::command]
async fn plugin_open(
    kubri: State<'_, Arc<Kubri>>,
    terminals: State<'_, Arc<Terminals>>,
    run: PluginRun,
    on_event: Channel<InvokeResponseBody>,
) -> CommandResult<u32> {
    let terminal = kubri.open_plugin(&run).await.map_err(|e| e.to_string())?;
    Ok(register(&terminals, terminal, on_event))
}

/// Starts a background k9s plugin without waiting for it.
#[tauri::command]
async fn plugin_run(kubri: State<'_, Arc<Kubri>>, run: PluginRun) -> CommandResult<()> {
    kubri.run_plugin(&run).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn k9s_config(kubri: State<'_, Arc<Kubri>>) -> CommandResult<K9sConfig> {
    Ok(kubri.k9s_config().await)
}

#[tauri::command]
async fn k9s_skin(kubri: State<'_, Arc<Kubri>>, name: String) -> CommandResult<Value> {
    kubri.k9s_skin(&name).await.map_err(|e| e.to_string())
}

/// Sends a terminal's output to the channel and keeps its input for `terminal_write`.
fn register(terminals: &Arc<Terminals>, terminal: Terminal, on_event: Channel<InvokeResponseBody>) -> u32 {
    let Terminal {
        container,
        input,
        mut output,
    } = terminal;
    let id = terminals.next_id.fetch_add(1, Ordering::Relaxed);
    let registry = Arc::clone(terminals);
    let mut sessions = terminals.sessions.lock().unwrap();
    let task = tauri::async_runtime::spawn(async move {
        let ready = serde_json::json!({ "type": "ready", "container": container });
        let _ = on_event.send(InvokeResponseBody::Json(ready.to_string()));
        while let Some(out) = output.next().await {
            let body = match out {
                TerminalOutput::Data { data } => InvokeResponseBody::Raw(data),
                exit @ TerminalOutput::Exit { .. } => {
                    InvokeResponseBody::Json(serde_json::to_string(&exit).unwrap_or_default())
                }
            };
            if on_event.send(body).is_err() {
                break;
            }
        }
        registry.sessions.lock().unwrap().remove(&id);
    });
    sessions.insert(
        id,
        TerminalSession {
            input: Arc::new(tokio::sync::Mutex::new(input)),
            task,
        },
    );
    id
}

#[tauri::command]
async fn terminal_write(terminals: State<'_, Arc<Terminals>>, id: u32, data: String) -> CommandResult<()> {
    let input = terminals.input(id)?;
    let mut input = input.lock().await;
    input.write(data.as_bytes()).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn terminal_resize(
    terminals: State<'_, Arc<Terminals>>,
    id: u32,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    let input = terminals.input(id)?;
    input.lock().await.resize(cols, rows);
    Ok(())
}

#[tauri::command]
fn terminal_close(terminals: State<'_, Arc<Terminals>>, id: u32) {
    if let Some(session) = terminals.sessions.lock().unwrap().remove(&id) {
        session.task.abort();
    }
}

#[tauri::command]
fn port_forwards(kubri: State<'_, Arc<Kubri>>) -> Vec<PortForwardInfo> {
    kubri.port_forwards()
}

#[tauri::command]
async fn start_port_forward(
    kubri: State<'_, Arc<Kubri>>,
    request: PortForwardRequest,
) -> CommandResult<PortForwardInfo> {
    kubri.start_port_forward(request).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_favorite_forwards(
    kubri: State<'_, Arc<Kubri>>,
) -> CommandResult<Vec<kubri_core::FavoriteStart>> {
    kubri.start_favorite_forwards().await.map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_port_forward(kubri: State<'_, Arc<Kubri>>, id: u32) -> CommandResult<()> {
    kubri.stop_port_forward(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn benchmarks(kubri: State<'_, Arc<Kubri>>) -> Vec<BenchInfo> {
    kubri.benchmarks()
}

// Async, so the benchmark task starts on the async runtime.
#[tauri::command]
async fn start_benchmark(kubri: State<'_, Arc<Kubri>>, request: BenchRequest) -> CommandResult<BenchInfo> {
    kubri.start_benchmark(&request).map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_benchmark(kubri: State<'_, Arc<Kubri>>, id: u32) -> CommandResult<()> {
    kubri.stop_benchmark(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_benchmark(kubri: State<'_, Arc<Kubri>>, id: u32) -> CommandResult<()> {
    kubri.remove_benchmark(id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn cordon(kubri: State<'_, Arc<Kubri>>, name: String, cordoned: bool) -> CommandResult<()> {
    kubri.cordon(&name, cordoned).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn drain(
    kubri: State<'_, Arc<Kubri>>,
    name: String,
    options: Option<DrainOptions>,
) -> CommandResult<DrainReport> {
    kubri
        .drain(&name, options.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_object(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<Value> {
    kubri.get(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn replace_object(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    value: Value,
) -> CommandResult<Value> {
    kubri.replace(&target, value).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn patch_object(kubri: State<'_, Arc<Kubri>>, target: ObjectRef, patch: Value) -> CommandResult<Value> {
    kubri.patch(&target, patch).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_object(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    options: Option<DeleteOptions>,
) -> CommandResult<()> {
    kubri
        .delete(&target, &options.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn scale(kubri: State<'_, Arc<Kubri>>, target: ObjectRef, replicas: u32) -> CommandResult<()> {
    kubri.scale(&target, replicas).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn debug(kubri: State<'_, Arc<Kubri>>, request: DebugRequest) -> CommandResult<DebugSession> {
    kubri.debug(request).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn memory_fix(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    container: String,
) -> CommandResult<MemoryFix> {
    kubri
        .memory_fix(&target, &container)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_memory_limit(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    container: String,
    memory: String,
) -> CommandResult<()> {
    kubri
        .set_memory_limit(&target, &container, &memory)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn scheduling(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<Vec<NodeFit>> {
    kubri.scheduling(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn apply(kubri: State<'_, Arc<Kubri>>, request: ApplyRequest) -> CommandResult<Vec<ApplyItem>> {
    kubri.apply(request).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_manifest(url: String) -> CommandResult<String> {
    kubri_core::fetch_manifest(&url).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn service_check(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<ServiceCheck> {
    kubri.service_check(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn restart(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<()> {
    kubri.restart(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn drill(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<Option<Drill>> {
    kubri.drill(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn used_by(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<Vec<Reference>> {
    kubri.used_by(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn trigger_cronjob(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<String> {
    kubri.trigger_cronjob(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn suspend_cronjob(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    suspend: bool,
) -> CommandResult<()> {
    kubri
        .suspend_cronjob(&target, suspend)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_images(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    images: Vec<ContainerImage>,
) -> CommandResult<()> {
    kubri
        .set_images(&target, &images)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn rollback(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<String> {
    kubri.rollback(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn events(
    kubri: State<'_, Arc<Kubri>>,
    namespace: Option<String>,
    uid: String,
) -> CommandResult<Vec<EventInfo>> {
    kubri
        .events(namespace.as_deref(), &uid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn metrics(
    kubri: State<'_, Arc<Kubri>>,
    kind: MetricsKind,
    namespace: Option<String>,
) -> CommandResult<Vec<Usage>> {
    kubri
        .metrics(kind, namespace.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn pulse(kubri: State<'_, Arc<Kubri>>, namespace: Option<String>) -> CommandResult<Pulse> {
    kubri.pulse(namespace.as_deref()).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn integrations(kubri: State<'_, Arc<Kubri>>, fresh: bool) -> CommandResult<kubri_core::Integrations> {
    kubri.integrations(fresh).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn charts(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    range: kubri_core::Range,
) -> CommandResult<kubri_core::Charts> {
    kubri.charts(&target, range).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn usage_history(
    kubri: State<'_, Arc<Kubri>>,
    namespace: Option<String>,
    seconds: u64,
) -> CommandResult<kubri_core::Charts> {
    let range = kubri_core::Range { seconds, end: None };
    let namespace = namespace.filter(|ns| !ns.is_empty());
    kubri
        .usage_history(namespace.as_deref(), range)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn prometheus_query(
    kubri: State<'_, Arc<Kubri>>,
    query: String,
    range: kubri_core::Range,
) -> CommandResult<Vec<kubri_core::Series>> {
    kubri
        .prometheus_query(&query, range)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn xray(
    kubri: State<'_, Arc<Kubri>>,
    resource: ResourceRef,
    namespace: Option<String>,
) -> CommandResult<Vec<XrayNode>> {
    kubri
        .xray(&resource, namespace.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn sanitize(kubri: State<'_, Arc<Kubri>>, namespace: Option<String>) -> CommandResult<SanitizerReport> {
    kubri
        .sanitize(namespace.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn rbac(kubri: State<'_, Arc<Kubri>>, namespace: Option<String>) -> CommandResult<Rbac> {
    kubri.rbac(namespace.as_deref()).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn permissions(kubri: State<'_, Arc<Kubri>>, namespace: Option<String>) -> CommandResult<Permissions> {
    kubri
        .permissions(namespace.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn helm_releases(
    kubri: State<'_, Arc<Kubri>>,
    namespace: Option<String>,
) -> CommandResult<Vec<HelmRelease>> {
    kubri
        .helm_releases(namespace.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn helm_release(
    kubri: State<'_, Arc<Kubri>>,
    namespace: String,
    name: String,
    revision: Option<u32>,
) -> CommandResult<HelmReleaseDetails> {
    kubri
        .helm_release(&namespace, &name, revision)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn helm_history(
    kubri: State<'_, Arc<Kubri>>,
    namespace: String,
    name: String,
) -> CommandResult<Vec<HelmRelease>> {
    kubri
        .helm_history(&namespace, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn helm_version(kubri: State<'_, Arc<Kubri>>) -> CommandResult<Option<String>> {
    Ok(kubri.helm_version().await)
}

#[tauri::command]
async fn helm_rollback(
    kubri: State<'_, Arc<Kubri>>,
    namespace: String,
    name: String,
    revision: u32,
) -> CommandResult<String> {
    kubri
        .helm_rollback(&namespace, &name, revision)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn helm_change(kubri: State<'_, Arc<Kubri>>, change: HelmChange) -> CommandResult<HelmOutcome> {
    kubri.helm_change(&change).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn helm_chart_values(kubri: State<'_, Arc<Kubri>>, chart: HelmChartRef) -> CommandResult<String> {
    kubri.helm_chart_values(&chart).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn helm_search(kubri: State<'_, Arc<Kubri>>, query: String) -> CommandResult<Vec<HubChart>> {
    kubri.helm_search(&query).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn helm_uninstall(
    kubri: State<'_, Arc<Kubri>>,
    namespace: String,
    name: String,
) -> CommandResult<String> {
    kubri
        .helm_uninstall(&namespace, &name)
        .await
        .map_err(|e| e.to_string())
}

/// Opens a web address in the default browser, such as a port-forward's. Only http and
/// https, so the UI cannot start other programs through it.
#[tauri::command]
fn open_url(url: String) -> CommandResult<()> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("{url} is not a web address"));
    }
    open::that_detached(&url).map_err(|e| format!("cannot open {url}: {e}"))
}

/// Saves text, such as the shown log lines, in the Downloads folder, since the window cannot
/// download files. Never replaces a file: `web.log` becomes `web (1).log`. Returns the path.
#[tauri::command]
async fn save_text(name: String, text: String) -> CommandResult<String> {
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or("there is no Downloads folder")?;
    tauri::async_runtime::spawn_blocking(move || save_new(&dir, &file_name(&name), text.as_bytes()))
        .await
        .map_err(|e| e.to_string())?
        .map(|path| path.display().to_string())
}

/// `name` with anything but letters, digits, `.`, `-` and `_` replaced, so it stays one file
/// in the folder.
fn file_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    match name.trim_start_matches('.') {
        "" => "kubri.txt".to_owned(),
        name => name.to_owned(),
    }
}

fn save_new(dir: &std::path::Path, name: &str, contents: &[u8]) -> CommandResult<std::path::PathBuf> {
    use std::io::Write;
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
        _ => (name, String::new()),
    };
    for n in 0..1000 {
        let path = match n {
            0 => dir.join(name),
            n => dir.join(format!("{stem} ({n}){extension}")),
        };
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                return file
                    .write_all(contents)
                    .map(|()| path.clone())
                    .map_err(|e| format!("cannot save {}: {e}", path.display()));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot save {}: {e}", path.display())),
        }
    }
    Err(format!("{name} exists too many times in {}", dir.display()))
}

/// `KUBRI_READ_ONLY=true` (or 1, yes, on), as for kubri-server.
fn read_only_requested() -> bool {
    std::env::var("KUBRI_READ_ONLY").is_ok_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "y"
        )
    })
}

pub fn run() {
    #[cfg(unix)]
    if shell_env::print_requested() {
        shell_env::print();
        return;
    }
    // Over stdio, stdout belongs to the protocol.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kubri_desktop_lib=info,kubri_core=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    // Before Kubri reads KUBECONFIG, and before Tauri starts any threads.
    #[cfg(unix)]
    shell_env::load();

    if std::env::args().skip(1).any(|a| a == "--mcp") {
        agents::serve_stdio();
        return;
    }

    let kubri = Arc::new(Kubri::load());
    let read_only = read_only_requested();
    if read_only {
        kubri.set_read_only();
        tracing::info!("read-only: Kubri does not change clusters");
    }
    let agents = agents::Agents::new(kubri.clone(), read_only);
    tauri::Builder::default()
        .setup({
            let agents = agents.clone();
            move |_| {
                tauri::async_runtime::spawn(async move { agents.follow_settings().await });
                Ok(())
            }
        })
        .manage(kubri)
        .manage(agents)
        .manage(Arc::new(Streams::default()))
        .manage(Arc::new(Terminals::default()))
        .invoke_handler(tauri::generate_handler![
            contexts,
            reload_contexts,
            switch_context,
            resources,
            preview,
            schema,
            access,
            who_am_i,
            readable_namespaces,
            settings,
            set_context_settings,
            set_diff_settings,
            set_column_settings,
            agents::set_agent_settings,
            agents::agents_connection,
            agents::agents_new_token,
            agents::agents_pending,
            agents::agents_answer,
            agents::agents_calls,
            explain::set_explain_settings,
            explain::explain_key,
            explain::set_explain_key,
            explain::explain_prompt,
            explain::explain,
            guard,
            unlock,
            lock,
            watch_table,
            logs,
            unwatch,
            terminal_open,
            plugin_open,
            plugin_run,
            k9s_config,
            k9s_skin,
            terminal_write,
            terminal_resize,
            terminal_close,
            port_forwards,
            start_port_forward,
            stop_port_forward,
            start_favorite_forwards,
            benchmarks,
            start_benchmark,
            stop_benchmark,
            remove_benchmark,
            cordon,
            drain,
            get_object,
            replace_object,
            patch_object,
            delete_object,
            scale,
            restart,
            debug,
            memory_fix,
            set_memory_limit,
            scheduling,
            service_check,
            apply,
            fetch_manifest,
            events,
            metrics,
            pulse,
            integrations,
            charts,
            usage_history,
            prometheus_query,
            xray,
            sanitize,
            rbac,
            permissions,
            helm_releases,
            helm_release,
            helm_history,
            helm_version,
            helm_rollback,
            helm_uninstall,
            helm_change,
            helm_chart_values,
            helm_search,
            open_url,
            save_text,
            drill,
            used_by,
            trigger_cronjob,
            suspend_cronjob,
            set_images,
            rollback
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Kubri");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_stay_in_the_folder() {
        assert_eq!(file_name("web-7d4b9-app.log"), "web-7d4b9-app.log");
        assert_eq!(file_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(file_name("a b:c\\d.log"), "a_b_c_d.log");
        assert_eq!(file_name(".hidden"), "hidden");
        assert_eq!(file_name(""), "kubri.txt");
    }

    #[test]
    fn saving_never_replaces_a_file() {
        let dir = std::env::temp_dir().join(format!("kubri-save-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = save_new(&dir, "web.log", b"one").unwrap();
        let second = save_new(&dir, "web.log", b"two").unwrap();
        let bare = save_new(&dir, "notes", b"three").unwrap();
        let again = save_new(&dir, "notes", b"four").unwrap();
        assert_eq!(first, dir.join("web.log"));
        assert_eq!(second, dir.join("web (1).log"));
        assert_eq!(std::fs::read(&first).unwrap(), b"one");
        assert_eq!(std::fs::read(&second).unwrap(), b"two");
        assert_eq!(bare, dir.join("notes"));
        assert_eq!(again, dir.join("notes (1)"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
