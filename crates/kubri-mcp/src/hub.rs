//! What the Kubri window and the MCP server share: changes waiting for a person to allow them,
//! and the log of tool calls.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

/// How long a change waits for a person to allow it.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);
/// The window counts as open when it asked for pending changes this recently.
const WINDOW_SEEN: Duration = Duration::from_secs(10);
/// The log is cut to its newer half when it grows past this.
const LOG_LIMIT: u64 = 512 * 1024;
/// Calls the log shows at most.
const LOG_SHOWN: usize = 200;

/// A change an agent asked for, waiting for a person in the Kubri window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmRequest {
    pub id: u64,
    pub tool: String,
    pub context: Option<String>,
    /// What the change does, in a sentence.
    pub summary: String,
    /// The `kubectl` command that does the same.
    pub kubectl: String,
}

/// One tool call, as the log keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallRecord {
    /// Seconds since the Unix epoch.
    pub time: u64,
    pub transport: String,
    pub tool: String,
    pub context: Option<String>,
    pub arguments: Value,
    /// Why the call failed, if it did.
    pub error: Option<String>,
    pub millis: u64,
}

impl CallRecord {
    pub(crate) fn new(
        transport: &str,
        tool: &str,
        context: Option<String>,
        arguments: &Value,
        error: Option<String>,
        took: Duration,
    ) -> Self {
        Self {
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default(),
            transport: transport.to_owned(),
            tool: tool.to_owned(),
            context,
            arguments: arguments.clone(),
            error,
            millis: took.as_millis().try_into().unwrap_or(u64::MAX),
        }
    }
}

struct Pending {
    request: ConfirmRequest,
    reply: oneshot::Sender<bool>,
}

pub struct Hub {
    pending: Mutex<Vec<Pending>>,
    next_id: AtomicU64,
    window_seen: Mutex<Option<Instant>>,
    /// JSON lines, shared by every Kubri process: stdio servers log there too.
    log_path: Option<PathBuf>,
    log_lock: Mutex<()>,
}

impl Hub {
    /// A hub logging tool calls to `log_path`, or nowhere.
    pub fn new(log_path: Option<PathBuf>) -> Self {
        Self {
            pending: Default::default(),
            next_id: AtomicU64::new(1),
            window_seen: Default::default(),
            log_path,
            log_lock: Default::default(),
        }
    }

    /// The hub of this computer's Kubri: its log is `agent-calls.jsonl` in Kubri's config folder.
    pub fn for_this_computer() -> Self {
        Self::new(kubri_core::config_dir().map(|d| d.join("agent-calls.jsonl")))
    }

    /// Changes waiting for a person. Asking counts as the window being open.
    pub fn pending(&self) -> Vec<ConfirmRequest> {
        *self.window_seen.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        self.pending_list().iter().map(|p| p.request.clone()).collect()
    }

    /// Allows or refuses a waiting change; false when it is no longer waiting.
    pub fn answer(&self, id: u64, allow: bool) -> bool {
        let mut pending = self.pending_list();
        let Some(index) = pending.iter().position(|p| p.request.id == id) else {
            return false;
        };
        pending.remove(index).reply.send(allow).is_ok()
    }

    fn pending_list(&self) -> std::sync::MutexGuard<'_, Vec<Pending>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn window_open(&self) -> bool {
        self.window_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|seen| seen.elapsed() < WINDOW_SEEN)
    }

    /// Waits until a person allows the change in the Kubri window.
    pub(crate) async fn confirm(
        &self,
        tool: &str,
        context: Option<String>,
        summary: String,
        kubectl: String,
    ) -> Result<(), String> {
        if !self.window_open() {
            return Err("Kubri's window is not open, so nobody can allow this change. \
                        Ask the user to open Kubri and try again."
                .into());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        self.pending_list().push(Pending {
            request: ConfirmRequest {
                id,
                tool: tool.to_owned(),
                context,
                summary,
                kubectl,
            },
            reply,
        });
        match tokio::time::timeout(CONFIRM_TIMEOUT, answer).await {
            Ok(Ok(true)) => Ok(()),
            Ok(_) => Err("The user did not allow this change.".into()),
            Err(_) => {
                self.pending_list().retain(|p| p.request.id != id);
                Err("Nobody allowed this change within two minutes.".into())
            }
        }
    }

    /// Adds a call to the log.
    pub(crate) fn record(&self, record: CallRecord) {
        let Some(path) = &self.log_path else {
            return;
        };
        let _lock = self.log_lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(err) = append(path, &record) {
            tracing::warn!("cannot log the tool call: {err}");
        }
    }

    /// The latest tool calls, newest first.
    pub fn calls(&self) -> Vec<CallRecord> {
        let Some(path) = &self.log_path else {
            return Vec::new();
        };
        let text = std::fs::read_to_string(path).unwrap_or_default();
        text.lines()
            .rev()
            .filter_map(|line| serde_json::from_str(line).ok())
            .take(LOG_SHOWN)
            .collect()
    }
}

fn append(path: &std::path::Path, record: &CallRecord) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LOG_LIMIT) {
        let text = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = text.lines().collect();
        let kept = lines[lines.len() / 2..].join("\n") + "\n";
        std::fs::write(path, kept)?;
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(record).map_err(std::io::Error::other)?;
    writeln!(file, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn changes_need_an_open_window_and_an_answer() {
        let hub = std::sync::Arc::new(Hub::new(None));
        let refused = hub.confirm("scale", None, "Scale".into(), "kubectl".into()).await;
        assert!(refused.unwrap_err().contains("not open"));

        assert!(hub.pending().is_empty());
        let waiting = {
            let hub = hub.clone();
            tokio::spawn(async move {
                hub.confirm("scale", None, "Scale web".into(), "kubectl".into())
                    .await
            })
        };
        let id = loop {
            if let Some(request) = hub.pending().first() {
                break request.id;
            }
            tokio::task::yield_now().await;
        };
        assert!(hub.answer(id, true));
        assert!(waiting.await.unwrap().is_ok());
        assert!(!hub.answer(id, true));
    }

    #[test]
    fn keeps_a_log_newest_first() {
        let dir = std::env::temp_dir().join(format!("kubri-hub-{}", std::process::id()));
        let hub = Hub::new(Some(dir.join("calls.jsonl")));
        for tool in ["a", "b"] {
            hub.record(CallRecord::new(
                "stdio",
                tool,
                None,
                &Value::Null,
                None,
                Duration::ZERO,
            ));
        }
        let calls: Vec<_> = hub.calls().into_iter().map(|c| c.tool).collect();
        assert_eq!(calls, ["b", "a"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
