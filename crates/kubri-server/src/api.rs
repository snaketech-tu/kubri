use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post, put};
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use kubri_core::{ClusterInfo, ContextList, Kubri, ResourceInfo, ResourceRef};
use serde::Deserialize;

pub fn router(kubri: Arc<Kubri>) -> Router {
    Router::new()
        .route("/contexts", get(contexts))
        .route("/contexts/reload", post(reload))
        .route("/context", put(switch_context))
        .route("/resources", get(resources))
        .route("/watch", any(watch))
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
            UnknownContext(_) | UnknownResource(_) => StatusCode::NOT_FOUND,
            NoKubeconfig(_) | NotConnected => StatusCode::CONFLICT,
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
struct WatchQuery {
    #[serde(flatten)]
    resource: ResourceRef,
    namespace: Option<String>,
}

/// Streams [`kubri_core::WatchEvent`]s as JSON text frames until the client disconnects.
async fn watch(
    State(kubri): State<Arc<Kubri>>,
    Query(query): Query<WatchQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let namespace = query.namespace.filter(|ns| !ns.is_empty());
    let events = kubri.watch(&query.resource, namespace.as_deref()).await?;
    Ok(ws.on_upgrade(move |socket| forward(socket, events)))
}

async fn forward(
    socket: WebSocket,
    events: impl futures::Stream<Item = kubri_core::WatchEvent> + Send + 'static,
) {
    let (mut tx, mut rx) = socket.split();
    let mut events = std::pin::pin!(events);
    loop {
        tokio::select! {
            event = events.next() => {
                let Some(event) = event else { break };
                let text = match serde_json::to_string(&event) {
                    Ok(text) => text,
                    Err(err) => {
                        tracing::warn!(%err, "failed to serialize watch event");
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn get_json(router: Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = router
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn lists_contexts_from_kubeconfig() {
        let kubeconfig = kube::config::Kubeconfig::from_yaml(
            "contexts: [{name: kind, context: {cluster: kind, user: kind}}]\ncurrent-context: kind",
        )
        .unwrap();
        let router = router(Arc::new(Kubri::from_kubeconfig(kubeconfig)));
        let (status, body) = get_json(router, "/contexts").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["contexts"][0]["name"], "kind");
        assert_eq!(body["contexts"][0]["isDefault"], true);
        assert!(body["cluster"].is_null());
    }

    #[tokio::test]
    async fn resources_require_a_connection() {
        let router = router(Arc::new(Kubri::from_kubeconfig(Default::default())));
        let (status, body) = get_json(router, "/resources").await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("not connected"));
    }
}
