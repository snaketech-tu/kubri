//! AI agents: Kubri's MCP endpoint on localhost while the settings turn it on, and the
//! commands the settings page and the confirmation dialog use.

use std::sync::{Arc, Mutex};

use kubri_core::{AgentSettings, Kubri, Settings};
use kubri_mcp::http::HttpState;
use kubri_mcp::{CallRecord, ConfirmRequest, Connection, Hub, Mcp, Transport};
use tauri::State;
use tauri::async_runtime::JoinHandle;
use tokio::sync::oneshot;

type CommandResult<T> = Result<T, String>;

struct Running {
    port: u16,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

pub struct Agents {
    kubri: Arc<Kubri>,
    hub: Arc<Hub>,
    http: Arc<HttpState>,
    running: tokio::sync::Mutex<Option<Running>>,
    /// Why the endpoint is not running although it is turned on.
    error: Mutex<Option<String>>,
}

impl Agents {
    /// The MCP server of the desktop app: a Kubri of its own, read-only when `read_only`.
    pub fn new(kubri: Arc<Kubri>, read_only: bool) -> Arc<Self> {
        let hub = Arc::new(Hub::for_this_computer());
        let agent = Arc::new(Kubri::load());
        if read_only {
            agent.set_read_only();
        }
        let token = kubri_mcp::http::token().unwrap_or_else(|err| {
            tracing::warn!("cannot keep a token for agents: {err}");
            String::new()
        });
        let settings = kubri.clone();
        let http = Arc::new(HttpState {
            mcp: Arc::new(Mcp::new(agent, Transport::Http, hub.clone())),
            token: std::sync::RwLock::new(token),
            enabled: Box::new(move || settings.settings().agents.http),
        });
        Arc::new(Self {
            kubri,
            hub,
            http,
            running: Default::default(),
            error: Default::default(),
        })
    }

    /// Starts, moves or stops the endpoint to match the settings.
    pub async fn follow_settings(&self) {
        let wanted = self.kubri.settings().agents;
        let mut running = self.running.lock().await;
        let outdated = running
            .as_ref()
            .is_some_and(|r| !wanted.http || r.port != wanted.port);
        if outdated && let Some(old) = running.take() {
            let _ = old.stop.send(());
            let _ = old.task.await;
        }
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if wanted.http && running.is_none() {
            match kubri_mcp::http::serve(wanted.port, self.http.clone()).await {
                Ok((stop, task)) => {
                    tracing::info!(port = wanted.port, "serving MCP for agents");
                    *running = Some(Running {
                        port: wanted.port,
                        stop,
                        task: JoinHandle::Tokio(task),
                    });
                }
                Err(err) => {
                    let message = format!("cannot listen on port {}: {err}", wanted.port);
                    tracing::warn!("{message}");
                    *self.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(message);
                }
            }
        }
    }

    async fn connection(&self) -> Connection {
        let port = self.running.lock().await.as_ref().map(|r| r.port);
        let token = self.http.token.read().unwrap_or_else(|e| e.into_inner()).clone();
        Connection {
            url: port.map(|p| format!("http://127.0.0.1:{p}/mcp")),
            error: self.error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            token: (!token.is_empty()).then_some(token),
            command: kubri_mcp::stdio_command(),
        }
    }
}

/// MCP over stdio, when an agent starts the app with `--mcp`: no window, and only reading,
/// since no window can confirm a change.
pub fn serve_stdio() {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            tracing::error!("{err}");
            return;
        }
    };
    runtime.block_on(async {
        let kubri = Arc::new(Kubri::load());
        kubri.set_read_only();
        let hub = Arc::new(Hub::for_this_computer());
        let mcp = Arc::new(Mcp::new(kubri, Transport::Stdio, hub));
        if let Err(err) = kubri_mcp::stdio::serve(mcp).await {
            tracing::error!("{err}");
        }
    });
}

#[tauri::command]
pub async fn agents_connection(agents: State<'_, Arc<Agents>>) -> CommandResult<Connection> {
    Ok(agents.connection().await)
}

#[tauri::command]
pub async fn set_agent_settings(
    kubri: State<'_, Arc<Kubri>>,
    agents: State<'_, Arc<Agents>>,
    settings: AgentSettings,
) -> CommandResult<Settings> {
    let saved = kubri.set_agent_settings(settings).map_err(|e| e.to_string())?;
    agents.follow_settings().await;
    Ok(saved)
}

#[tauri::command]
pub async fn agents_new_token(agents: State<'_, Arc<Agents>>) -> CommandResult<Connection> {
    let token = kubri_mcp::http::new_token().map_err(|e| e.to_string())?;
    *agents.http.token.write().unwrap_or_else(|e| e.into_inner()) = token;
    Ok(agents.connection().await)
}

#[tauri::command]
pub fn agents_pending(agents: State<'_, Arc<Agents>>) -> Vec<ConfirmRequest> {
    agents.hub.pending()
}

#[tauri::command]
pub fn agents_answer(agents: State<'_, Arc<Agents>>, id: u64, allow: bool) -> bool {
    agents.hub.answer(id, allow)
}

#[tauri::command]
pub fn agents_calls(agents: State<'_, Arc<Agents>>) -> Vec<CallRecord> {
    agents.hub.calls()
}
