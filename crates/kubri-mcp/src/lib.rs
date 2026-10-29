//! Kubri as an MCP server: AI agents such as Claude Desktop, Claude Code or IDE assistants get
//! a window into the cluster through the same checks a person gets in Kubri.
//!
//! - Tools read by default: contexts, lists with `kubectl get` columns, YAML, events, logs, a
//!   pod's likely problem, the sanitizer, XRay, permission checks and cluster health.
//! - Changes (scale, restart, set image, delete) are offered only over HTTP from a running Kubri,
//!   only on contexts whose settings allow agents to change them (never production ones), and
//!   each one waits for a person to allow it in the Kubri window.
//! - Secrets are redacted unless the settings allow agents to read them, and results are
//!   size-limited.
//!
//! The protocol is JSON-RPC 2.0 as MCP specifies it, over stdio ([`stdio`]) or streamable
//! HTTP ([`http`]).

mod diagnose;
pub mod http;
mod hub;
mod redact;
mod render;
pub mod stdio;
mod tools;

use std::sync::Arc;

use kubri_core::Kubri;
use serde_json::{Value, json};

pub use hub::{CallRecord, ConfirmRequest, Hub};

/// How to connect an agent, for the settings page.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// The HTTP endpoint while it is on, such as `http://127.0.0.1:7879/mcp`.
    pub url: Option<String>,
    /// Why the endpoint is not running although it is turned on.
    pub error: Option<String>,
    pub token: Option<String>,
    /// The command an agent runs for MCP over stdio: this program with `--mcp`.
    pub command: Vec<String>,
}

/// This program started with `--mcp`, as an agent's configuration names it.
pub fn stdio_command() -> Vec<String> {
    let program = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "kubri".into());
    vec![program, "--mcp".into()]
}

/// The protocol versions Kubri speaks, newest first.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// How an agent reaches Kubri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// A process the agent starts. No Kubri window can confirm changes, so it only reads.
    Stdio,
    /// The endpoint of a running Kubri, whose window confirms changes.
    Http,
}

impl Transport {
    fn name(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
        }
    }
}

/// One MCP server: a Kubri of its own, so the agent's context never moves the person's.
pub struct Mcp {
    kubri: Arc<Kubri>,
    transport: Transport,
    hub: Arc<Hub>,
    /// Tool calls run one at a time: a call may switch this Kubri's context.
    busy: tokio::sync::Mutex<()>,
}

impl Mcp {
    /// Serves `kubri`, which should be a Kubri of its own: it is put in agent mode, where
    /// changes need the context's consent in the settings.
    pub fn new(kubri: Arc<Kubri>, transport: Transport, hub: Arc<Hub>) -> Self {
        kubri.set_agent();
        Self {
            kubri,
            transport,
            hub,
            busy: Default::default(),
        }
    }

    /// Answers one JSON-RPC message, or a batch of them; `None` for notifications.
    pub async fn handle(&self, message: Value) -> Option<Value> {
        match message {
            Value::Array(batch) => {
                let mut answers = Vec::new();
                for message in batch {
                    if let Some(answer) = Box::pin(self.handle(message)).await {
                        answers.push(answer);
                    }
                }
                (!answers.is_empty()).then_some(Value::Array(answers))
            }
            message => self.handle_one(message).await,
        }
    }

    async fn handle_one(&self, message: Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response to a request of ours; Kubri sends none.
            return id.map(|id| error(id, INVALID_REQUEST, "expected a request"));
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        // Notifications get no answer, whatever they are.
        let id = id?;
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::list(self.writes_offered()) })),
            "tools/call" => self.call(&params).await,
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err((METHOD_NOT_FOUND, format!("unknown method {method}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error(id, code, &message),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": "kubri",
                "title": "Kubri",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": INSTRUCTIONS,
        })
    }

    /// Whether the change tools are listed: over HTTP, when some context allows agents to
    /// change it.
    fn writes_offered(&self) -> bool {
        if self.transport != Transport::Http {
            return false;
        }
        self.kubri.reload_settings();
        self.kubri
            .settings()
            .contexts
            .values()
            .any(|c| c.agent_writes && !c.production && !c.read_only)
    }

    async fn call(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "missing the tool name".to_owned()))?;
        let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let tool = tools::find(name, self.writes_offered())
            .ok_or_else(|| (INVALID_PARAMS, format!("unknown tool {name}")))?;
        let _busy = self.busy.lock().await;
        // The person may have changed the settings in the Kubri window since the last call.
        self.kubri.reload_settings();
        let started = std::time::Instant::now();
        let outcome = tools::call(tool, &self.kubri, &self.hub, &arguments).await;
        let context = self.kubri.cluster().await.map(|c| c.context);
        self.hub.record(CallRecord::new(
            self.transport.name(),
            name,
            context,
            &arguments,
            outcome.as_ref().err().cloned(),
            started.elapsed(),
        ));
        Ok(match outcome {
            Ok(text) => {
                json!({ "content": [{ "type": "text", "text": render::limit(text) }], "isError": false })
            }
            // Tool failures are results the agent can read and act on, not protocol errors.
            Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
        })
    }
}

const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const PARSE_ERROR: i64 = -32700;

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The answer to a message that is not JSON.
fn parse_error(message: &str) -> Value {
    error(Value::Null, PARSE_ERROR, message)
}

const INSTRUCTIONS: &str = "Kubri gives read access to the Kubernetes clusters in the user's \
kubeconfig, with the same permission checks as the user. Tools act on the user's current \
context unless you pass `context` (see list_contexts). Resources can be named as kubectl names \
them: `pods`, `po`, `deploy`, `deployments.apps` or a kind such as `Deployment`. To find out \
why a pod fails, start with diagnose_pod. Secret values are hidden unless the user allows them. \
Changes are only possible where the user allowed them, and the user confirms each one in Kubri.";

#[cfg(test)]
mod tests {
    use super::*;

    fn mcp(transport: Transport) -> Mcp {
        let kubeconfig = kubri_core::Kubri::from_kubeconfig(Default::default());
        Mcp::new(Arc::new(kubeconfig), transport, Arc::new(Hub::new(None)))
    }

    #[tokio::test]
    async fn initializes_with_a_version_it_speaks() {
        let server = mcp(Transport::Stdio);
        let answer = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } }))
            .await
            .unwrap();
        assert_eq!(answer["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(answer["result"]["serverInfo"]["name"], "kubri");
        let answer = server
            .handle(json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }))
            .await
            .unwrap();
        assert_eq!(answer["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[tokio::test]
    async fn notifications_get_no_answer_and_unknown_methods_an_error() {
        let server = mcp(Transport::Stdio);
        assert!(
            server
                .handle(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .await
                .is_none()
        );
        let answer = server
            .handle(json!({ "jsonrpc": "2.0", "id": "a", "method": "nope" }))
            .await
            .unwrap();
        assert_eq!(answer["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(answer["id"], "a");
    }

    #[tokio::test]
    async fn stdio_lists_no_change_tools() {
        let server = mcp(Transport::Stdio);
        let answer = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .await
            .unwrap();
        let tools = answer["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "diagnose_pod"));
        assert!(tools.iter().all(|t| t["annotations"]["readOnlyHint"] == true));
        let answer = server
            .handle(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "scale", "arguments": {} } }))
            .await
            .unwrap();
        assert_eq!(answer["error"]["code"], INVALID_PARAMS);
    }

    #[tokio::test]
    async fn answers_batches() {
        let server = mcp(Transport::Stdio);
        let answer = server
            .handle(json!([
                { "jsonrpc": "2.0", "id": 1, "method": "ping" },
                { "jsonrpc": "2.0", "method": "notifications/initialized" },
                { "jsonrpc": "2.0", "id": 2, "method": "ping" }
            ]))
            .await
            .unwrap();
        assert_eq!(answer.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn tool_failures_are_results() {
        let server = mcp(Transport::Stdio);
        let answer = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": "get_object", "arguments": { "resource": "pods", "name": "x" } } }))
            .await
            .unwrap();
        assert_eq!(answer["result"]["isError"], true);
    }
}
