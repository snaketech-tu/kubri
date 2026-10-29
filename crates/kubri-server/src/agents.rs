//! AI agents: Kubri's MCP endpoint at `/mcp`, and the routes the settings page and the
//! confirmation dialog use.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use kubri_core::Kubri;
use kubri_mcp::http::HttpState;
use kubri_mcp::{CallRecord, ConfirmRequest, Connection, Hub, Mcp, Transport};
use serde::Deserialize;

pub struct Agents {
    kubri: Arc<Kubri>,
    hub: Arc<Hub>,
    http: Arc<HttpState>,
    url: String,
    token_error: Option<String>,
}

impl Agents {
    /// The MCP server of this web server: a Kubri of its own, read-only when `read_only`.
    pub fn new(kubri: Arc<Kubri>, addr: SocketAddr, read_only: bool) -> Arc<Self> {
        let hub = Arc::new(Hub::for_this_computer());
        let agent = Arc::new(Kubri::load());
        if read_only {
            agent.set_read_only();
        }
        let (token, token_error) = match kubri_mcp::http::token() {
            Ok(token) => (token, None),
            Err(err) => (String::new(), Some(format!("cannot keep a token: {err}"))),
        };
        let settings = kubri.clone();
        let http = Arc::new(HttpState {
            mcp: Arc::new(Mcp::new(agent, Transport::Http, hub.clone())),
            token: std::sync::RwLock::new(token),
            enabled: Box::new(move || settings.settings().agents.http),
        });
        let host = if addr.ip().is_unspecified() {
            SocketAddr::from(([127, 0, 0, 1], addr.port()))
        } else {
            addr
        };
        Arc::new(Self {
            kubri,
            hub,
            http,
            url: format!("http://{host}/mcp"),
            token_error,
        })
    }

    /// `/mcp`, for agents; it checks its own token.
    pub fn endpoint(&self) -> Router {
        kubri_mcp::http::router(self.http.clone())
    }

    /// The routes for the Kubri window, behind its sign-in.
    pub fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .route("/agents", get(connection))
            .route("/agents/token", post(new_token))
            .route("/agents/pending", get(pending))
            .route("/agents/answer", post(answer))
            .route("/agents/calls", get(calls))
            .with_state(self.clone())
    }

    fn connection(&self) -> Connection {
        let on = self.kubri.settings().agents.http;
        let token = self.http.token.read().unwrap_or_else(|e| e.into_inner()).clone();
        Connection {
            url: (on && self.token_error.is_none()).then(|| self.url.clone()),
            error: if on { self.token_error.clone() } else { None },
            token: (!token.is_empty()).then_some(token),
            command: kubri_mcp::stdio_command(),
        }
    }
}

async fn connection(State(agents): State<Arc<Agents>>) -> Json<Connection> {
    Json(agents.connection())
}

async fn new_token(State(agents): State<Arc<Agents>>) -> Result<Json<Connection>, (StatusCode, String)> {
    let token =
        kubri_mcp::http::new_token().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    *agents.http.token.write().unwrap_or_else(|e| e.into_inner()) = token;
    Ok(Json(agents.connection()))
}

async fn pending(State(agents): State<Arc<Agents>>) -> Json<Vec<ConfirmRequest>> {
    Json(agents.hub.pending())
}

#[derive(Deserialize)]
struct Answer {
    id: u64,
    allow: bool,
}

async fn answer(State(agents): State<Arc<Agents>>, Json(answer): Json<Answer>) -> Json<bool> {
    Json(agents.hub.answer(answer.id, answer.allow))
}

async fn calls(State(agents): State<Arc<Agents>>) -> Json<Vec<CallRecord>> {
    Json(agents.hub.calls())
}
