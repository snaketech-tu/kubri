//! MCP over streamable HTTP, served by the running Kubri on localhost: `POST /mcp` with one
//! JSON-RPC message, answered with JSON. Every request needs the bearer token from Kubri's
//! settings, and requests from web pages on other sites are refused.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::{Mcp, parse_error};

pub struct HttpState {
    pub mcp: Arc<Mcp>,
    /// The bearer token; [`new_token`] makes another.
    pub token: std::sync::RwLock<String>,
    /// Whether the endpoint is on now; the settings can turn it off at any time.
    pub enabled: Box<dyn Fn() -> bool + Send + Sync>,
}

/// The `/mcp` route.
pub fn router(state: Arc<HttpState>) -> Router {
    Router::new()
        .route("/mcp", post(message).get(no_stream).delete(no_stream))
        .with_state(state)
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(json!({ "error": message }))).into_response()
}

/// Whether a request may come from where its `Origin` says: no origin (not a browser), or a
/// page on this computer. This stops other sites from reaching the endpoint through DNS
/// rebinding.
fn local_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let host = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin)
        .trim_end_matches('/');
    let host = match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => host.split(':').next().unwrap_or(""),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Compares in constant time, so the token cannot be guessed from response times.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|given| same(given.trim().as_bytes(), token.as_bytes()))
}

fn checked(state: &HttpState, headers: &HeaderMap) -> Result<(), Box<Response>> {
    if !(state.enabled)() {
        return Err(Box::new(refuse(
            StatusCode::NOT_FOUND,
            "MCP over HTTP is off; turn it on under Settings → AI agents in Kubri",
        )));
    }
    if !local_origin(headers) {
        return Err(Box::new(refuse(
            StatusCode::FORBIDDEN,
            "requests from other sites are refused",
        )));
    }
    let token = state.token.read().unwrap_or_else(|e| e.into_inner()).clone();
    if token.is_empty() || !authorized(headers, &token) {
        return Err(Box::new(refuse(
            StatusCode::UNAUTHORIZED,
            "missing or wrong token; Kubri's settings show the right one",
        )));
    }
    Ok(())
}

async fn message(State(state): State<Arc<HttpState>>, headers: HeaderMap, body: Bytes) -> Response {
    if let Err(refused) = checked(&state, &headers) {
        return *refused;
    }
    let message: Value = match serde_json::from_slice(&body) {
        Ok(message) => message,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, axum::Json(parse_error(&err.to_string()))).into_response();
        }
    };
    match state.mcp.handle(message).await {
        Some(answer) => axum::Json(answer).into_response(),
        // Notifications and responses are accepted without a body.
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// Kubri sends nothing on its own, so it offers no event stream and no sessions to end.
async fn no_stream(State(state): State<Arc<HttpState>>, headers: HeaderMap) -> Response {
    if let Err(refused) = checked(&state, &headers) {
        return *refused;
    }
    StatusCode::METHOD_NOT_ALLOWED.into_response()
}

fn token_path() -> Option<PathBuf> {
    kubri_core::config_dir().map(|d| d.join("mcp-token"))
}

/// The token agents send, made on first use and kept in Kubri's config folder, readable only
/// by the user.
pub fn token() -> std::io::Result<String> {
    let path = token_path().ok_or_else(|| std::io::Error::other("there is no config folder"))?;
    match std::fs::read_to_string(&path) {
        Ok(token) if !token.trim().is_empty() => Ok(token.trim().to_owned()),
        _ => new_token(),
    }
}

/// Makes a new token; agents using the old one are refused from now on.
pub fn new_token() -> std::io::Result<String> {
    let path = token_path().ok_or_else(|| std::io::Error::other("there is no config folder"))?;
    let mut bytes = [0u8; 24];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
        .map_err(|_| std::io::Error::other("no random numbers"))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    std::io::Write::write_all(&mut options.open(&path)?, token.as_bytes())?;
    Ok(token)
}

/// An endpoint of its own on `127.0.0.1:port`, for the desktop app. Dropping the sender stops it.
pub async fn serve(
    port: u16,
    state: Arc<HttpState>,
) -> std::io::Result<(oneshot::Sender<()>, JoinHandle<()>)> {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let serving = axum::serve(listener, router(state)).with_graceful_shutdown(async {
            let _ = stopped.await;
        });
        if let Err(err) = serving.await {
            tracing::warn!("the MCP endpoint stopped: {err}");
        }
    });
    Ok((stop, task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Hub, Transport};
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app(enabled: bool) -> Router {
        let kubri = Arc::new(kubri_core::Kubri::from_kubeconfig(Default::default()));
        router(Arc::new(HttpState {
            mcp: Arc::new(Mcp::new(kubri, Transport::Http, Arc::new(Hub::new(None)))),
            token: std::sync::RwLock::new("secret".into()),
            enabled: Box::new(move || enabled),
        }))
    }

    fn request(token: Option<&str>, origin: Option<&str>, body: &str) -> Request<Body> {
        let mut builder = Request::post("/mcp").header("content-type", "application/json");
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }

    const PING: &str = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

    #[tokio::test]
    async fn needs_the_token_a_local_origin_and_the_setting() {
        let status = |r: Response| r.status();
        assert_eq!(
            status(app(true).oneshot(request(None, None, PING)).await.unwrap()),
            401
        );
        assert_eq!(
            status(
                app(true)
                    .oneshot(request(Some("nope"), None, PING))
                    .await
                    .unwrap()
            ),
            401
        );
        assert_eq!(
            status(
                app(true)
                    .oneshot(request(Some("secret"), Some("https://evil.example"), PING))
                    .await
                    .unwrap()
            ),
            403
        );
        assert_eq!(
            status(
                app(false)
                    .oneshot(request(Some("secret"), None, PING))
                    .await
                    .unwrap()
            ),
            404
        );

        let answer = app(true)
            .oneshot(request(Some("secret"), Some("http://localhost:6274"), PING))
            .await
            .unwrap();
        assert_eq!(answer.status(), 200);
        let body = answer.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["result"], json!({}));
    }

    #[tokio::test]
    async fn accepts_notifications_without_a_body() {
        let notification = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let answer = app(true)
            .oneshot(request(Some("secret"), None, notification))
            .await
            .unwrap();
        assert_eq!(answer.status(), 202);
    }

    #[test]
    fn tells_local_origins() {
        let origin = |o: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::ORIGIN, o.parse().unwrap());
            local_origin(&headers)
        };
        assert!(origin("http://localhost:3000"));
        assert!(origin("http://127.0.0.1"));
        assert!(origin("http://[::1]:8080"));
        assert!(!origin("http://localhost.evil.example"));
        assert!(!origin("null"));
    }
}
