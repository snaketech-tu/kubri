//! The desktop app: the shared UI in a native window, backed by kubri-core through Tauri commands.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::{Stream, StreamExt};
use kubri_core::{
    BenchInfo, BenchRequest, ClusterInfo, ContextList, DeleteOptions, DrainOptions, DrainReport, EventInfo,
    HelmRelease, HelmReleaseDetails, K9sConfig, Kubri, LogEvent, LogOptions, MetricsKind, ObjectRef,
    Permissions, PluginRun, PortForwardInfo, PortForwardRequest, Pulse, Rbac, ResourceInfo, ResourceRef,
    SanitizerReport, TableEvent, Terminal, TerminalInput, TerminalOptions, TerminalOutput, Usage, XrayNode,
};
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
    on_event: Channel<TableEvent>,
) -> CommandResult<u32> {
    let events = kubri
        .watch_table(&resource, namespace.as_deref())
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
async fn restart(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<()> {
    kubri.restart(&target).await.map_err(|e| e.to_string())
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

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kubri_desktop_lib=info,kubri_core=info".into()),
        )
        .init();

    tauri::Builder::default()
        .manage(Arc::new(Kubri::load()))
        .manage(Arc::new(Streams::default()))
        .manage(Arc::new(Terminals::default()))
        .invoke_handler(tauri::generate_handler![
            contexts,
            reload_contexts,
            switch_context,
            resources,
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
            events,
            metrics,
            pulse,
            xray,
            sanitize,
            rbac,
            permissions,
            helm_releases,
            helm_release,
            helm_history,
            helm_version,
            helm_rollback,
            helm_uninstall
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Kubri");
}
