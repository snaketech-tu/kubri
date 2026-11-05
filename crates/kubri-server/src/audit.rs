//! The audit log: every change made through Kubri, who made it, in which context, to which
//! object, and whether it worked. Written as JSON lines to a file or to stdout with
//! `KUBRI_AUDIT_LOG`, and kept in memory for the Activity panel.

use std::collections::VecDeque;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::body::{Body, to_bytes};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use kubri_core::{Kubri, ObjectRef};
use serde::Serialize;
use serde_json::Value;

use crate::auth::Caller;

/// Changes kept for the Activity panel.
const KEEP: usize = 1000;
/// The largest request body read to find what a change is about; larger ones are refused
/// by the routes anyway.
const MAX_BODY: usize = 64 * 1024 * 1024;
/// Values of a request worth writing down, besides the object.
const DETAILS: [&str; 9] = [
    "replicas",
    "revision",
    "cordoned",
    "suspend",
    "install",
    "remotePort",
    "localPort",
    "image",
    "images",
];

pub struct AuditLog {
    out: Option<Mutex<Box<dyn Write + Send>>>,
    recent: Mutex<VecDeque<Entry>>,
    /// Groups whose members see everyone's changes on the Activity panel.
    readers: Vec<String>,
}

/// One change.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// RFC 3339, in UTC.
    pub time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    /// The browser's address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The object a change was made to.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Target {
    /// Plural, with the group: `deployments.apps`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Target {
    pub fn of(object: &ObjectRef) -> Self {
        Self {
            resource: Some(resource_name(&object.resource.plural, &object.resource.group)),
            namespace: object.namespace.clone(),
            name: Some(object.name.clone()),
        }
    }
}

fn resource_name(plural: &str, group: &str) -> String {
    if group.is_empty() {
        plural.to_owned()
    } else {
        format!("{plural}.{group}")
    }
}

impl AuditLog {
    /// Writes to `path`, appending, or to stdout for `-`; keeps entries only in memory
    /// without one.
    pub fn open(path: Option<&str>, readers: Vec<String>) -> Result<Self, String> {
        let out: Option<Box<dyn Write + Send>> = match path {
            None => None,
            Some("-") => Some(Box::new(std::io::stdout())),
            Some(path) => {
                let mut options = std::fs::OpenOptions::new();
                options.create(true).append(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let file = options
                    .open(path)
                    .map_err(|e| format!("cannot open the audit log {path}: {e}"))?;
                Some(Box::new(file))
            }
        };
        Ok(Self {
            out: out.map(Mutex::new),
            recent: Mutex::new(VecDeque::new()),
            readers,
        })
    }

    pub fn write(&self, entry: Entry) {
        if let Some(out) = &self.out
            && let Ok(line) = serde_json::to_string(&entry)
        {
            let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(err) = writeln!(out, "{line}").and_then(|()| out.flush()) {
                tracing::error!("cannot write the audit log: {err}");
            }
        }
        let mut recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        if recent.len() == KEEP {
            recent.pop_back();
        }
        recent.push_front(entry);
    }

    /// The changes `caller` may see, newest first: everyone's where nobody signs in by
    /// name or the caller is in a reader group, otherwise their own.
    pub fn visible_to(&self, caller: &Caller) -> Vec<Entry> {
        let recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        match &caller.person {
            None => recent.iter().cloned().collect(),
            Some(person) if person.groups.iter().any(|g| self.readers.contains(g)) => {
                recent.iter().cloned().collect()
            }
            Some(person) => recent
                .iter()
                .filter(|e| e.person.as_deref() == Some(person.name.as_str()))
                .cloned()
                .collect(),
        }
    }

    /// Whether `caller` sees everyone's changes.
    pub fn sees_everyone(&self, caller: &Caller) -> bool {
        caller
            .person
            .as_ref()
            .is_none_or(|p| p.groups.iter().any(|g| self.readers.contains(g)))
    }
}

/// What the audit middleware of one Kubri needs, and what handlers of sockets use to
/// write down what they opened.
#[derive(Clone)]
pub struct Recorder {
    log: Arc<AuditLog>,
    kubri: Arc<Kubri>,
    caller: Caller,
    client: Option<String>,
}

impl Recorder {
    /// Writes down that `action` was done to `target`.
    pub async fn record(&self, action: &str, target: Option<Target>, result: Result<(), String>) {
        let context = self.kubri.cluster().await.map(|c| c.context);
        self.log.write(self.entry(action, context, target, None, result));
    }

    fn entry(
        &self,
        action: &str,
        context: Option<String>,
        target: Option<Target>,
        detail: Option<Value>,
        result: Result<(), String>,
    ) -> Entry {
        let person = self.caller.person.as_ref();
        Entry {
            time: jiff::Timestamp::now().to_string(),
            person: person.map(|p| p.name.clone()),
            groups: person.map(|p| p.groups.clone()).unwrap_or_default(),
            client: self.client.clone(),
            context,
            action: action.to_owned(),
            target,
            detail,
            ok: result.is_ok(),
            error: result.err(),
        }
    }
}

/// The state of [`record`] for one Kubri.
#[derive(Clone)]
pub struct Audit {
    pub log: Arc<AuditLog>,
    pub kubri: Arc<Kubri>,
}

/// Writes every change down after it is made, and gives socket handlers a [`Recorder`].
pub async fn record(State(audit): State<Audit>, mut request: Request, next: Next) -> Response {
    let recorder = Recorder {
        log: audit.log.clone(),
        kubri: audit.kubri.clone(),
        caller: request.extensions().get::<Caller>().cloned().unwrap_or_default(),
        client: request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string()),
    };
    let Some(action) = action(request.method(), request.uri().path()) else {
        request.extensions_mut().insert(recorder);
        return next.run(request).await;
    };
    let (parts, body) = request.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_BODY).await else {
        return (StatusCode::PAYLOAD_TOO_LARGE, "the request is too large").into_response();
    };
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    // A dry run changes nothing.
    if json.get("dryRun") == Some(&Value::Bool(true)) {
        return next.run(Request::from_parts(parts, Body::from(bytes))).await;
    }
    let target = target(&json, action);
    let context = match json.pointer("/target/context").and_then(Value::as_str) {
        Some(context) => Some(context.to_owned()),
        None => audit.kubri.cluster().await.map(|c| c.context),
    };
    let detail = detail(&json);
    let response = next.run(Request::from_parts(parts, Body::from(bytes))).await;
    let (response, result) = outcome(response).await;
    if let Err(err) = &result {
        tracing::info!(action, error = err, "change failed");
    }
    audit
        .log
        .write(recorder.entry(action, context, target, detail, result));
    response
}

/// The changes a route makes, by method and path. Sockets that open a shell or plugin are
/// written down by their handlers, which know the target.
fn action(method: &Method, path: &str) -> Option<&'static str> {
    Some(match (method.as_str(), path) {
        ("PUT", "/object") => "edit",
        ("PATCH", "/object") => "patch",
        ("DELETE", "/object") => "delete",
        ("POST", "/object/scale") => "scale",
        ("POST", "/object/restart") => "restart",
        ("POST", "/object/images") => "set image",
        ("POST", "/object/resources") => "set resources",
        ("POST", "/fix/memory/apply") => "set memory limit",
        ("POST", "/debug") => "debug",
        ("POST", "/apply") => "apply",
        ("POST", "/cronjob/trigger") => "trigger",
        ("POST", "/cronjob/suspend") => "suspend",
        ("POST", "/gitops") => "gitops",
        ("POST", "/certificate/renew") => "renew certificate",
        ("POST", "/replicaset/rollback") => "rollback",
        ("POST", "/helm/rollback") => "helm rollback",
        ("POST", "/helm/uninstall") => "helm uninstall",
        ("POST", "/helm/change") => "helm install or upgrade",
        ("POST", "/plugins/run") => "plugin",
        ("POST", "/portforwards") => "port-forward",
        ("POST", "/portforwards/favorites") => "port-forward favorites",
        ("DELETE", p) if p.starts_with("/portforwards/") => "stop port-forward",
        ("POST", "/benchmarks") => "benchmark",
        ("POST", "/node/cordon") => "cordon",
        ("POST", "/node/drain") => "drain",
        ("POST", "/guard/unlock") => "unlock context",
        ("POST", "/guard/lock") => "lock context",
        ("PUT", "/explain/key") => "change settings",
        ("PUT", p) if p.starts_with("/settings/") => "change settings",
        ("POST" | "DELETE", "/contexts/added") => "change contexts",
        ("GET", "/terminal/local/open") => "local terminal",
        _ => return None,
    })
}

/// The object a request names: an object reference as `target`, a whole object, or a
/// node or Helm release by name.
fn target(json: &Value, action: &str) -> Option<Target> {
    let text = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    if let Some(target) = json.get("target").filter(|t| t.is_object()) {
        let resource = target.get("resource").map(|r| {
            resource_name(
                r.get("plural").and_then(Value::as_str).unwrap_or_default(),
                r.get("group").and_then(Value::as_str).unwrap_or_default(),
            )
        });
        return Some(Target {
            resource,
            namespace: text(target, "namespace"),
            name: text(target, "name"),
        });
    }
    let name = text(json, "name")?;
    let resource = match action {
        "cordon" | "drain" => Some("nodes".to_owned()),
        a if a.starts_with("helm") => Some("helm releases".to_owned()),
        _ => None,
    };
    Some(Target {
        resource,
        namespace: text(json, "namespace"),
        name: Some(name),
    })
}

fn detail(json: &Value) -> Option<Value> {
    let found: serde_json::Map<String, Value> = DETAILS
        .iter()
        .filter_map(|key| json.get(*key).map(|v| ((*key).to_owned(), v.clone())))
        .collect();
    (!found.is_empty()).then_some(Value::Object(found))
}

/// The response again, and the error it carries when it is not a success.
async fn outcome(response: Response) -> (Response, Result<(), String>) {
    let status = response.status();
    if status.is_success() {
        return (response, Ok(()));
    }
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, 1024 * 1024).await.unwrap_or_default();
    let message = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| {
            let text = String::from_utf8_lossy(&bytes).trim().to_owned();
            if text.is_empty() { status.to_string() } else { text }
        });
    (Response::from_parts(parts, Body::from(bytes)), Err(message))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Activity {
    /// Whether the entries are everyone's, or only the caller's own.
    everyone: bool,
    entries: Vec<Entry>,
}

/// `GET /activity`: the changes the caller may see.
pub async fn activity(State(log): State<Arc<AuditLog>>, Extension(caller): Extension<Caller>) -> Response {
    Json(Activity {
        everyone: log.sees_everyone(&caller),
        entries: log.visible_to(&caller),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::{get, post, put};
    use kubri_core::Person;
    use tower::ServiceExt;

    fn caller(name: &str, groups: &[&str]) -> Caller {
        Caller {
            session: Some(format!("{name}-session")),
            person: Some(Person {
                name: name.to_owned(),
                groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            }),
            token: None,
        }
    }

    fn app(log: Arc<AuditLog>) -> Router {
        let kubri = Arc::new(Kubri::from_kubeconfig(Default::default()));
        Router::new()
            .route("/object/scale", post(|| async { Json(()) }))
            .route(
                "/node/drain",
                post(|| async {
                    (
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({ "error": "nodes \"n1\" is forbidden" })),
                    )
                }),
            )
            .route("/helm/change", post(|| async { Json(()) }))
            .route("/explain/key", put(|| async { Json(()) }))
            .route("/pods", get(|| async { Json(()) }))
            .layer(axum::middleware::from_fn_with_state(Audit { log, kubri }, record))
    }

    async fn send(app: &Router, method: &str, path: &str, who: &Caller, body: Value) -> StatusCode {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        request.extensions_mut().insert(who.clone());
        app.clone().oneshot(request).await.unwrap().status()
    }

    #[tokio::test]
    async fn changes_are_written_with_who_what_and_how_it_went() {
        let path = std::env::temp_dir().join(format!("kubri-audit-test-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let log = Arc::new(AuditLog::open(path.to_str(), vec!["admins".into()]).unwrap());
        let app = app(log.clone());
        let alice = caller("alice", &["devs"]);
        let bob = caller("bob", &["admins"]);
        let target = serde_json::json!({
            "target": {
                "context": "prod",
                "resource": {"group": "apps", "version": "v1", "plural": "deployments"},
                "namespace": "shop",
                "name": "web"
            },
            "replicas": 3
        });
        assert_eq!(send(&app, "POST", "/object/scale", &alice, target).await, StatusCode::OK);
        let drain = serde_json::json!({ "name": "n1" });
        assert_eq!(send(&app, "POST", "/node/drain", &bob, drain).await, StatusCode::FORBIDDEN);
        // Reading, dry runs and the key itself are not written down.
        assert_eq!(send(&app, "GET", "/pods", &alice, Value::Null).await, StatusCode::OK);
        let dry = serde_json::json!({ "name": "x", "namespace": "shop", "dryRun": true });
        send(&app, "POST", "/helm/change", &alice, dry).await;
        let key = serde_json::json!({ "key": "sk-secret" });
        send(&app, "PUT", "/explain/key", &alice, key).await;

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(!written.contains("sk-secret"));
        let lines: Vec<Value> = written.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 3, "{written}");
        assert_eq!(lines[0]["person"], "alice");
        assert_eq!(lines[0]["groups"], serde_json::json!(["devs"]));
        assert_eq!(lines[0]["context"], "prod");
        assert_eq!(lines[0]["action"], "scale");
        assert_eq!(
            lines[0]["target"],
            serde_json::json!({"resource": "deployments.apps", "namespace": "shop", "name": "web"})
        );
        assert_eq!(lines[0]["detail"], serde_json::json!({"replicas": 3}));
        assert_eq!(lines[0]["ok"], true);
        assert_eq!(lines[1]["action"], "drain");
        assert_eq!(lines[1]["target"], serde_json::json!({"resource": "nodes", "name": "n1"}));
        assert_eq!(lines[1]["ok"], false);
        assert_eq!(lines[1]["error"], "nodes \"n1\" is forbidden");
        assert_eq!(lines[2]["action"], "change settings");
        assert!(lines[0]["time"].as_str().unwrap().ends_with('Z'));

        // Alice sees her own changes; Bob, in a reader group, sees everyone's.
        let mine = log.visible_to(&alice);
        assert_eq!(mine.len(), 2);
        assert!(mine.iter().all(|e| e.person.as_deref() == Some("alice")));
        assert!(!log.sees_everyone(&alice));
        assert_eq!(log.visible_to(&bob).len(), 3);
        assert_eq!(log.visible_to(&bob)[0].action, "change settings");
        // Without people signing in by name, everyone shares one identity.
        assert_eq!(log.visible_to(&Caller::default()).len(), 3);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn only_the_newest_changes_are_kept() {
        let log = AuditLog::open(None, vec![]).unwrap();
        let recorder = Recorder {
            log: Arc::new(AuditLog::open(None, vec![]).unwrap()),
            kubri: Arc::new(Kubri::from_kubeconfig(Default::default())),
            caller: Caller::default(),
            client: None,
        };
        for i in 0..KEEP + 5 {
            log.write(recorder.entry(&format!("change {i}"), None, None, None, Ok(())));
        }
        let entries = log.visible_to(&Caller::default());
        assert_eq!(entries.len(), KEEP);
        assert_eq!(entries[0].action, format!("change {}", KEEP + 4));
    }
}
