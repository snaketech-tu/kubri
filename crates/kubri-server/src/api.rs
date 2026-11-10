use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Extension, Json, Router};
use futures::{SinkExt, Stream, StreamExt};
use kubri_core::{
    ClusterInfo, ExplainAnswer, ExplainPrompt, ExplainSubject, Kubri, LogEvent, LogOptions, ObjectRef,
    PluginRun, ResourceRef, TableScope, Terminal, TerminalOptions, TerminalOutput,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audit::{self, Recorder};

pub fn router(kubri: Arc<Kubri>) -> Router {
    Router::new()
        .route("/call", post(call))
        .route(
            "/snapshot",
            post(take_snapshot)
                .put(open_snapshot)
                .layer(DefaultBodyLimit::max(MAX_SNAPSHOT)),
        )
        // The key is kept where only the server reads it; the desktop app keeps its own.
        .route("/explain/key", get(explain_key).put(set_explain_key))
        .route("/explain/prompt", post(explain_prompt))
        .route("/explain", post(explain))
        .route("/table", any(table))
        .route("/logs", any(logs))
        .route("/terminal", any(terminal))
        .route("/plugins/terminal", any(plugin_terminal))
        .route("/terminal/local/open", any(local_terminal))
        .route("/local-cluster/create", any(create_local_cluster))
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
            NoKubeconfig(_) | NotConnected | ContextChanged(_) | Cancelled(_) => StatusCode::CONFLICT,
            // A kubeconfig kubectl refuses too, such as one pasted into Add clusters.
            Unsupported(_) | InvalidObject(_) | InvalidKubeconfig(_) => StatusCode::BAD_REQUEST,
            Timeout => StatusCode::GATEWAY_TIMEOUT,
            ReadOnly(_) | Forbidden(_) => StatusCode::FORBIDDEN,
            Kubeconfig(_) | Kube(_) | Model(_) | Integration(_) => StatusCode::BAD_GATEWAY,
            Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(serde_json::json!({ "error": self.0.to_string() }));
        (status, body).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

/// Every call in the core's call table, named by `call` in the body.
async fn call(State(kubri): State<Arc<Kubri>>, Json(request): Json<kubri_core::Request>) -> ApiResult<Value> {
    Ok(Json(kubri.call(request).await?))
}

/// The largest snapshot file Kubri opens.
const MAX_SNAPSHOT: usize = 1 << 30;

#[derive(Deserialize)]
struct SnapshotQuery {
    namespace: Option<String>,
}

/// A snapshot file of a namespace or the whole cluster, as a download.
async fn take_snapshot(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<SnapshotQuery>,
) -> Result<Response, ApiError> {
    let (name, bytes) = kubri.take_snapshot(query.namespace.as_deref()).await?;
    let disposition = format!("attachment; filename=\"{name}\"");
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/gzip".to_owned()),
            (axum::http::header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
struct SnapshotFile {
    name: Option<String>,
}

/// Opens a snapshot file sent as the request body.
async fn open_snapshot(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<SnapshotFile>,
    body: Bytes,
) -> ApiResult<ClusterInfo> {
    Ok(Json(kubri.open_snapshot(&body, query.name).await?))
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
) -> Response {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    let events = match kubri
        .watch_table(&query.resource, namespace.as_deref(), query.scope)
        .await
    {
        Ok(events) => events.boxed(),
        // A browser cannot read why a WebSocket was turned away, so the reason goes over
        // the socket, which then stays open, as the desktop app's channel does.
        Err(err) => refused(err).boxed(),
    };
    ws.on_upgrade(move |socket| forward(socket, events))
}

/// A list that could not start: its error, then nothing until the client goes.
fn refused(err: kubri_core::Error) -> impl Stream<Item = kubri_core::TableEvent> + Send + 'static {
    let error = kubri_core::TableEvent::Error {
        message: err.to_string(),
    };
    futures::stream::once(futures::future::ready(error)).chain(futures::stream::pending())
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
async fn terminal(
    State(kubri): State<Arc<Kubri>>,
    recorder: Option<Extension<Recorder>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let opened = match first_message::<TerminalRequest>(&mut socket).await {
            Ok(req) => {
                let opened = kubri
                    .open_terminal(&req.target, req.options)
                    .await
                    .map_err(|e| e.to_string());
                if let Some(Extension(recorder)) = &recorder {
                    let result = opened.as_ref().map(|_| ()).map_err(Clone::clone);
                    recorder
                        .record("shell", Some(audit::Target::of(&req.target)), result)
                        .await;
                }
                opened
            }
            Err(err) => Err(err),
        };
        run_terminal(socket, opened).await;
    })
}

/// A plugin in a terminal: the client sends a [`PluginRun`] first, then it works like
/// [`terminal`].
async fn plugin_terminal(
    State(kubri): State<Arc<Kubri>>,
    recorder: Option<Extension<Recorder>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let opened = match first_message::<PluginRun>(&mut socket).await {
            Ok(run) => {
                let opened = kubri.open_plugin(&run).await.map_err(|e| e.to_string());
                if let Some(Extension(recorder)) = &recorder {
                    let result = opened.as_ref().map(|_| ()).map_err(Clone::clone);
                    let action = format!("plugin {}", run.plugin);
                    recorder
                        .record(&action, run.target.as_ref().map(audit::Target::of), result)
                        .await;
                }
                opened
            }
            Err(err) => Err(err),
        };
        run_terminal(socket, opened).await;
    })
}

/// A shell on this server, when it was started to allow it: the client sends
/// [`kubri_core::LocalTerminalOptions`] first, then it works like a container shell.
async fn local_terminal(
    State(kubri): State<Arc<Kubri>>,
    recorder: Option<Extension<Recorder>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let opened = match first_message::<kubri_core::LocalTerminalOptions>(&mut socket).await {
            Ok(options) => {
                let opened = kubri
                    .open_local_terminal(&options)
                    .await
                    .map_err(|e| e.to_string());
                if let Some(Extension(recorder)) = &recorder {
                    let result = opened.as_ref().map(|_| ()).map_err(Clone::clone);
                    recorder.record("local terminal", None, result).await;
                }
                opened
            }
            Err(err) => Err(err),
        };
        run_terminal(socket, opened).await;
    })
}

/// Makes a cluster with kind or k3d, when the server allows local terminals: the client
/// sends a [`kubri_core::LocalClusterRequest`] first, then it works like a terminal.
async fn create_local_cluster(State(kubri): State<Arc<Kubri>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let opened = match first_message::<kubri_core::LocalClusterRequest>(&mut socket).await {
            Ok(request) => kubri
                .create_local_cluster(&request)
                .await
                .map_err(|e| e.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn a_list_that_cannot_start_says_why_and_stays_open() {
        let mut events = std::pin::pin!(refused(kubri_core::Error::NotConnected));
        let first = serde_json::to_value(events.next().await.unwrap()).unwrap();
        assert_eq!(first["type"], "error");
        assert!(
            first["message"].as_str().unwrap().contains("not connected"),
            "{first}"
        );
        assert!(futures::FutureExt::now_or_never(events.next()).is_none());
    }

    async fn send(router: Router, request: Request<Body>) -> (StatusCode, Value) {
        let response = router.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    fn disconnected() -> Router {
        router(Arc::new(Kubri::from_kubeconfig(Default::default())))
    }

    fn post_json(uri: &str, body: &str) -> Request<Body> {
        Request::post(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    /// Makes `call` on `router` and answers with the status and the body.
    async fn call(router: Router, call: Value) -> (StatusCode, Value) {
        send(router, post_json("/call", &call.to_string())).await
    }

    fn target(plural: &str) -> Value {
        json!({ "resource": { "group": "", "version": "v1", "plural": plural }, "namespace": "shop", "name": "web" })
    }

    #[tokio::test]
    async fn answers_calls_of_the_call_table() {
        let kubeconfig = kube::config::Kubeconfig::from_yaml(
            "contexts: [{name: kind, context: {cluster: kind, user: kind}}]\ncurrent-context: kind",
        )
        .unwrap();
        let router = router(Arc::new(Kubri::from_kubeconfig(kubeconfig)));
        let (status, body) = call(router, json!({ "call": "contexts" })).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["contexts"][0]["name"], "kind");
        assert_eq!(body["contexts"][0]["isDefault"], true);
        assert!(body["cluster"].is_null());
        let (status, _) = call(disconnected(), json!({ "call": "nothing" })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        // A call without the arguments it needs is turned away before it runs.
        let (status, _) = call(disconnected(), json!({ "call": "helmRelease", "name": "shop" })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn calls_keep_the_status_of_the_core_s_errors() {
        let (status, body) = call(
            disconnected(),
            json!({ "call": "inspectKubeconfig", "text": "contexts: [nope" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            body["error"].as_str().unwrap().starts_with("invalid kubeconfig"),
            "{body}"
        );

        for request in [
            json!({ "call": "resources" }),
            json!({ "call": "getObject", "target": target("pods") }),
            json!({ "call": "metrics", "kind": "pods", "namespace": null }),
            json!({ "call": "pulse", "namespace": "" }),
            json!({ "call": "helmRelease", "namespace": "apps", "name": "shop", "revision": 2 }),
            json!({ "call": "cordon", "name": "worker-1", "cordoned": true }),
            json!({ "call": "startPortForward", "target": target("pods"), "remotePort": 8080 }),
        ] {
            let (status, body) = call(disconnected(), request.clone()).await;
            assert_eq!(status, StatusCode::CONFLICT, "{request}: {body}");
            assert!(
                body["error"].as_str().unwrap().contains("not connected"),
                "{body}"
            );
        }

        let (status, body) = call(disconnected(), json!({ "call": "stopPortForward", "id": 7 })).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "port-forward 7 does not exist");
        let (status, _) = call(disconnected(), json!({ "call": "k9sSkin", "name": "../config" })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, body) = call(disconnected(), json!({ "call": "portForwards" })).await;
        assert_eq!((status, body), (StatusCode::OK, json!([])));
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
    async fn read_only_turns_changes_away() {
        let kubri = Arc::new(Kubri::from_kubeconfig(Default::default()));
        kubri.set_read_only();
        let target = json!({ "resource": { "group": "batch", "version": "v1", "plural": "cronjobs" },
            "namespace": "ops", "name": "backup" });
        let changes = [
            json!({ "call": "restart", "target": target }),
            json!({ "call": "triggerCronJob", "target": target }),
            json!({ "call": "debug", "kind": "node", "node": "n1" }),
            json!({ "call": "apply", "objects": [] }),
            json!({ "call": "helmChange", "namespace": "web", "name": "shop", "values": "" }),
            json!({ "call": "setMemoryLimit", "target": target, "container": "app", "memory": "1Gi" }),
            json!({ "call": "suspendCronJob", "target": target, "suspend": true }),
            json!({ "call": "gitops", "target": target, "action": "reconcile" }),
            json!({ "call": "renewCertificate", "target": target }),
            json!({ "call": "setResources", "target": target,
                "containers": [{ "container": "app", "requests": { "cpu": "100m" }, "limits": {} }] }),
            json!({ "call": "rollback", "target": target }),
            json!({ "call": "setImages", "target": target, "images": [{ "name": "app", "image": "web:2" }] }),
        ];
        for change in changes {
            let (status, body) = call(router(kubri.clone()), change.clone()).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{change}");
            assert!(
                body["error"].as_str().unwrap().contains("read-only"),
                "{change}: {body}"
            );
        }
        // Looking around still works, until it needs the missing connection.
        for name in ["drill", "usedBy"] {
            let (status, _) = call(router(kubri.clone()), json!({ "call": name, "target": target })).await;
            assert_eq!(status, StatusCode::CONFLICT, "{name}");
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
}
