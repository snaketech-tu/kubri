use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post, put};
use axum::{Json, Router};
use futures::{SinkExt, Stream, StreamExt};
use kubri_core::{
    ClusterInfo, ContextList, DeleteOptions, EventInfo, Kubri, MetricsKind, ObjectRef, ResourceInfo,
    ResourceRef, Usage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn router(kubri: Arc<Kubri>) -> Router {
    Router::new()
        .route("/contexts", get(contexts))
        .route("/contexts/reload", post(reload))
        .route("/context", put(switch_context))
        .route("/resources", get(resources))
        .route("/table", any(table))
        .route(
            "/object",
            get(get_object)
                .put(replace_object)
                .patch(patch_object)
                .delete(delete_object),
        )
        .route("/object/scale", post(scale))
        .route("/object/restart", post(restart))
        .route("/events", get(events))
        .route("/metrics", get(metrics))
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
            // Pass on what the API server said: 403, 404, 409 conflict, 422 invalid…
            err @ Kube(_) if err.api_status().is_some() => err
                .api_status()
                .and_then(|code| StatusCode::from_u16(code).ok())
                .unwrap_or(StatusCode::BAD_GATEWAY),
            UnknownContext(_) | UnknownResource(_) => StatusCode::NOT_FOUND,
            NoKubeconfig(_) | NotConnected => StatusCode::CONFLICT,
            Unsupported(_) | InvalidObject(_) => StatusCode::BAD_REQUEST,
            Timeout => StatusCode::GATEWAY_TIMEOUT,
            Kubeconfig(_) | Kube(_) => StatusCode::BAD_GATEWAY,
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
struct SwitchContext {
    name: String,
}

async fn switch_context(
    State(kubri): State<Arc<Kubri>>,
    Json(req): Json<SwitchContext>,
) -> ApiResult<ClusterInfo> {
    Ok(Json(kubri.switch_context(&req.name).await?))
}

async fn resources(State(kubri): State<Arc<Kubri>>) -> ApiResult<Vec<ResourceInfo>> {
    Ok(Json(kubri.resources().await?))
}

#[derive(Deserialize)]
struct TableQuery {
    #[serde(flatten)]
    resource: ResourceRef,
    namespace: Option<String>,
}

/// Streams [`kubri_core::TableEvent`]s as JSON text frames until the client disconnects.
async fn table(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<TableQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    let events = kubri.watch_table(&query.resource, namespace.as_deref()).await?;
    Ok(ws.on_upgrade(move |socket| forward(socket, events)))
}

async fn forward<T: Serialize>(socket: WebSocket, events: impl Stream<Item = T> + Send + 'static) {
    let (mut tx, mut rx) = socket.split();
    let mut events = std::pin::pin!(events);
    loop {
        tokio::select! {
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
    #[serde(flatten)]
    resource: ResourceRef,
    namespace: Option<String>,
    name: String,
}

async fn get_object(State(kubri): State<Arc<Kubri>>, Query(query): Query<ObjectQuery>) -> ApiResult<Value> {
    let target = ObjectRef {
        resource: query.resource,
        namespace: query.namespace.filter(|ns| !ns.is_empty()),
        name: query.name,
    };
    Ok(Json(kubri.get(&target).await?))
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
struct Scale {
    target: ObjectRef,
    replicas: u32,
}

async fn scale(State(kubri): State<Arc<Kubri>>, Json(req): Json<Scale>) -> ApiResult<()> {
    Ok(Json(kubri.scale(&req.target, req.replicas).await?))
}

#[derive(Deserialize)]
struct Restart {
    target: ObjectRef,
}

async fn restart(State(kubri): State<Arc<Kubri>>, Json(req): Json<Restart>) -> ApiResult<()> {
    Ok(Json(kubri.restart(&req.target).await?))
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
}
