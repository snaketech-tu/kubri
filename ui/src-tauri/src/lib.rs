//! The desktop app: the shared UI in a native window, backed by kubri-core through Tauri commands.

mod agents;
mod explain;
#[cfg(unix)]
mod shell_env;
mod update;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::{Stream, StreamExt};
use kubri_core::{
    ClusterInfo, ContextList, Kubri, LogEvent, LogOptions, ObjectRef, PluginRun, ResourceRef, TableEvent,
    TableScope, Terminal, TerminalInput, TerminalOptions, TerminalOutput,
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

/// Every call in the core's call table, named by `call` in `request`.
#[tauri::command]
async fn call(
    kubri: State<'_, Arc<Kubri>>,
    agents: State<'_, Arc<agents::Agents>>,
    request: kubri_core::Request,
) -> CommandResult<Value> {
    let agent_settings = matches!(request, kubri_core::Request::SetAgentSettings { .. });
    let reply = kubri.call(request).await.map_err(|e| e.to_string())?;
    // The agents' server starts, stops or moves with its settings.
    if agent_settings {
        agents.follow_settings().await;
    }
    Ok(reply)
}

#[tauri::command]
async fn add_kubeconfig_files(
    kubri: State<'_, Arc<Kubri>>,
    paths: Vec<String>,
) -> CommandResult<ContextList> {
    kubri
        .add_kubeconfig_files(&paths)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_kubeconfig_file(kubri: State<'_, Arc<Kubri>>, path: String) -> CommandResult<ContextList> {
    kubri
        .remove_kubeconfig_file(&path)
        .await
        .map_err(|e| e.to_string())
}

/// Asks for kubeconfig files in the system's file dialog; empty when the user cancels.
#[tauri::command]
async fn choose_kubeconfig_files(app: tauri::AppHandle) -> Vec<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file().set_title("Load kubeconfig files");
    if let Some(kube) = dirs::home_dir().map(|h| h.join(".kube")).filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(kube);
    }
    dialog.pick_files(move |files| {
        let _ = tx.send(files.unwrap_or_default());
    });
    rx.await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
        .collect()
}

/// Saves a namespace, or the whole cluster, as a snapshot file in the Downloads folder.
/// Returns the path.
#[tauri::command]
async fn take_snapshot(kubri: State<'_, Arc<Kubri>>, namespace: Option<String>) -> CommandResult<String> {
    let (name, bytes) = kubri
        .take_snapshot(namespace.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or("there is no Downloads folder")?;
    tauri::async_runtime::spawn_blocking(move || save_new(&dir, &file_name(&name), &bytes))
        .await
        .map_err(|e| e.to_string())?
        .map(|path| path.display().to_string())
}

/// Opens a snapshot file sent as the raw request body, its name in the `x-name` header.
#[tauri::command]
async fn open_snapshot(
    kubri: State<'_, Arc<Kubri>>,
    request: tauri::ipc::Request<'_>,
) -> CommandResult<ClusterInfo> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("the snapshot file did not arrive".into());
    };
    let name = request
        .headers()
        .get("x-name")
        .and_then(|n| n.to_str().ok())
        .map(percent_decode);
    kubri.open_snapshot(bytes, name).await.map_err(|e| e.to_string())
}

/// Undoes `encodeURIComponent`.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[i + 1..i + 3]).ok())
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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

/// Opens a shell on this computer; it is written to and closed like [`terminal_open`].
#[tauri::command]
async fn local_terminal_open(
    kubri: State<'_, Arc<Kubri>>,
    terminals: State<'_, Arc<Terminals>>,
    options: kubri_core::LocalTerminalOptions,
    on_event: Channel<InvokeResponseBody>,
) -> CommandResult<u32> {
    let terminal = kubri
        .open_local_terminal(&options)
        .await
        .map_err(|e| e.to_string())?;
    Ok(register(&terminals, terminal, on_event))
}

/// Makes a cluster with kind or k3d in a terminal; it is written to and closed like
/// [`terminal_open`].
#[tauri::command]
async fn local_cluster_create(
    kubri: State<'_, Arc<Kubri>>,
    terminals: State<'_, Arc<Terminals>>,
    request: kubri_core::LocalClusterRequest,
    on_event: Channel<InvokeResponseBody>,
) -> CommandResult<u32> {
    let terminal = kubri
        .create_local_cluster(&request)
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
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // `kubri lint …` and `kubri snapshot …` run in the terminal, without a window.
    let command = kubri_cli::requested(&args);
    #[cfg(windows)]
    if command {
        attach_console();
    }
    // Over stdio, stdout belongs to the protocol. A command only says what went wrong.
    let filter = if command {
        "warn,kube_client=error"
    } else {
        "kubri_desktop_lib=info,kubri_core=info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .with_writer(std::io::stderr)
        .init();
    // Before Kubri reads KUBECONFIG, and before Tauri starts any threads.
    #[cfg(unix)]
    shell_env::load();

    if command {
        std::process::exit(kubri_cli::run(args).into());
    }
    if args.iter().skip(1).any(|a| a == "--mcp") {
        agents::serve_stdio();
        return;
    }

    let kubri = Arc::new(Kubri::load());
    // `kubri FILE.kubri` or `kubri open FILE`: start with that snapshot open.
    if let Some(path) = kubri_cli::file_to_open(&args) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let opened = match std::fs::read(&path) {
            Ok(bytes) => {
                tauri::async_runtime::block_on(kubri.open_snapshot(&bytes, name)).map_err(|e| e.to_string())
            }
            Err(err) => Err(err.to_string()),
        };
        if let Err(err) = opened {
            tracing::error!("cannot open the snapshot {}: {err}", path.display());
        }
    }
    kubri.record_changes();
    kubri.allow_local_terminal();
    let read_only = read_only_requested();
    if read_only {
        kubri.set_read_only();
        tracing::info!("read-only: Kubri does not change clusters");
    }
    let agents = agents::Agents::new(kubri.clone(), read_only);
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
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
        .manage(update::Pending::default())
        .invoke_handler(tauri::generate_handler![
            call,
            take_snapshot,
            open_snapshot,
            local_terminal_open,
            local_cluster_create,
            agents::agents_connection,
            agents::agents_new_token,
            agents::agents_pending,
            agents::agents_answer,
            agents::agents_calls,
            explain::explain_key,
            explain::set_explain_key,
            explain::explain_prompt,
            explain::explain,
            watch_table,
            logs,
            unwatch,
            terminal_open,
            plugin_open,
            terminal_write,
            terminal_resize,
            terminal_close,
            add_kubeconfig_files,
            remove_kubeconfig_file,
            choose_kubeconfig_files,
            update::update_check,
            update::update_install,
            open_url,
            save_text,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Kubri");
}

/// A release build on Windows has no console of its own; a command writes to the one it was
/// started from.
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: AttachConsole only takes a process id; failing leaves the process as it was.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
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
