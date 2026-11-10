//! Interactive shells in containers and attaching to a container's main process.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc as futures_mpsc;
use futures::stream::{self, BoxStream};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::{AttachParams, AttachedProcess, TerminalSize};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::{Error, Result};

/// Starts bash if the image has it and sh otherwise, like k9s. First it says its process id, in
/// an escape sequence that Kubri takes out of the output, so that closing the session can end it.
const DEFAULT_SHELL: [&str; 3] = [
    "/bin/sh",
    "-c",
    "printf '\\033]7777;kubri-shell=%s\\007' $$; export TERM=xterm-256color; \
     command -v bash >/dev/null 2>&1 && exec bash || exec sh",
];
/// How the shell's process id starts in its first output; a BEL ends it.
const SHELL_ID: &[u8] = b"\x1b]7777;kubri-shell=";
/// How long ending a closed shell may take.
const HANGUP_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_CONTAINER_ANNOTATION: &str = "kubectl.kubernetes.io/default-container";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TerminalMode {
    /// Run a new shell in the container (`kubectl exec -it`).
    #[default]
    Shell,
    /// Connect to the container's main process (`kubectl attach -it`).
    Attach,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, optional_fields = nullable)]
pub struct TerminalOptions {
    /// Defaults to the pod's default container.
    pub container: Option<String>,
    #[ts(as = "Option<TerminalMode>", optional)]
    pub mode: TerminalMode,
    /// Command to run in [`TerminalMode::Shell`] instead of the default shell.
    pub command: Option<Vec<String>>,
    #[ts(as = "Option<u16>", optional)]
    pub cols: u16,
    #[ts(as = "Option<u16>", optional)]
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalOutput {
    /// Raw terminal output.
    Data { data: Vec<u8> },
    /// The process ended. `message` explains a failure, like a missing shell.
    Exit { message: Option<String> },
}

/// A running terminal session. Dropping [`Terminal::output`] ends it.
pub struct Terminal {
    pub container: String,
    pub input: TerminalInput,
    pub output: BoxStream<'static, TerminalOutput>,
}

/// Sends keystrokes and size changes to a [`Terminal`].
pub struct TerminalInput {
    stdin: Option<Box<dyn AsyncWrite + Send + Unpin>>,
    resize: Option<futures_mpsc::Sender<TerminalSize>>,
}

impl TerminalInput {
    pub(crate) fn new(
        stdin: Option<Box<dyn AsyncWrite + Send + Unpin>>,
        resize: Option<futures_mpsc::Sender<TerminalSize>>,
    ) -> Self {
        Self { stdin, resize }
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Error::Unsupported("the container does not accept input".into()))?;
        stdin.write_all(data).await?;
        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if let Some(resize) = &mut self.resize
            && cols > 0
            && rows > 0
        {
            // Only the latest size matters, so a full queue can drop this one.
            let _ = resize.try_send(TerminalSize {
                width: cols,
                height: rows,
            });
        }
    }
}

/// The names of every container of a pod: init, regular and ephemeral.
pub(crate) fn containers(pod: &Pod) -> Vec<String> {
    let Some(spec) = &pod.spec else {
        return Vec::new();
    };
    let init = spec.init_containers.iter().flatten().map(|c| c.name.clone());
    let main = spec.containers.iter().map(|c| c.name.clone());
    let debug = spec.ephemeral_containers.iter().flatten().map(|c| c.name.clone());
    init.chain(main).chain(debug).collect()
}

/// The container a terminal opens in when none is chosen.
pub(crate) fn default_container(pod: &Pod) -> Option<String> {
    let annotated = pod
        .metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get(DEFAULT_CONTAINER_ANNOTATION))
        .cloned();
    annotated.or_else(|| pod.spec.as_ref()?.containers.first().map(|c| c.name.clone()))
}

pub(crate) async fn open(api: Api<Pod>, pod: &Pod, options: TerminalOptions) -> Result<Terminal> {
    let name = pod.metadata.name.as_deref().unwrap_or_default();
    let container = options
        .container
        .clone()
        .or_else(|| default_container(pod))
        .ok_or_else(|| Error::Unsupported(format!("pod {name} has no containers")))?;
    if let Some(reason) = not_running(pod, &container) {
        return Err(Error::Unsupported(format!(
            "container {container} is not running ({reason}), so there is nothing to connect to"
        )));
    }

    let mut hangup = None;
    let (mut process, tty) = match options.mode {
        TerminalMode::Shell => {
            let params = AttachParams::interactive_tty().container(&container);
            let command: Vec<String> = match &options.command {
                Some(command) if !command.is_empty() => command.clone(),
                _ => {
                    hangup = Some(Hangup {
                        api: api.clone(),
                        pod: name.to_string(),
                        container: container.clone(),
                        pid: Arc::default(),
                    });
                    DEFAULT_SHELL.iter().map(|s| s.to_string()).collect()
                }
            };
            (api.exec(name, command, &params).await?, true)
        }
        TerminalMode::Attach => {
            // Attach can only offer what the container was started with.
            let (stdin, tty) = attach_flags(pod, &container);
            let params = AttachParams::default()
                .container(&container)
                .stdin(stdin)
                .stdout(true)
                .stderr(!tty)
                .tty(tty);
            (api.attach(name, &params).await?, tty)
        }
    };

    let mut input = TerminalInput {
        stdin: process
            .stdin()
            .map(|w| Box::new(w) as Box<dyn AsyncWrite + Send + Unpin>),
        resize: process.terminal_size(),
    };
    input.resize(options.cols, options.rows);
    Ok(Terminal {
        container,
        input,
        output: output(process, tty, hangup),
    })
}

/// Why `container` is not running, from the pod's status. `None` while it runs, or when the
/// status does not say, leaving the answer to the API server.
fn not_running(pod: &Pod, container: &str) -> Option<String> {
    let status = pod.status.as_ref()?;
    let state = [
        &status.init_container_statuses,
        &status.container_statuses,
        &status.ephemeral_container_statuses,
    ]
    .into_iter()
    .flatten()
    .flatten()
    .find(|s| s.name == container)?
    .state
    .as_ref()?;
    if state.running.is_some() {
        return None;
    }
    if let Some(waiting) = &state.waiting {
        return Some(match &waiting.reason {
            Some(reason) => format!("waiting: {reason}"),
            None => "waiting".into(),
        });
    }
    let terminated = state.terminated.as_ref()?;
    Some(match &terminated.reason {
        Some(reason) => format!("terminated: {reason}"),
        None => format!("exited with code {}", terminated.exit_code),
    })
}

/// Whether `container` was started with stdin open and a TTY.
fn attach_flags(pod: &Pod, container: &str) -> (bool, bool) {
    let Some(spec) = &pod.spec else {
        return (false, false);
    };
    let main = spec.containers.iter().find(|c| c.name == container);
    let debug = spec
        .ephemeral_containers
        .iter()
        .flatten()
        .find(|c| c.name == container);
    let (stdin, tty) = match (main, debug) {
        (Some(c), _) => (c.stdin, c.tty),
        (None, Some(c)) => (c.stdin, c.tty),
        (None, None) => (None, None),
    };
    (stdin.unwrap_or(false), tty.unwrap_or(false))
}

/// Ends a shell that still runs when its session is closed. Kubernetes keeps the process of a
/// terminal session running when the connection goes away, so without this every shell closed in
/// Kubri would stay behind in the container, like a `kubectl exec` that was killed.
struct Hangup {
    api: Api<Pod>,
    pod: String,
    container: String,
    /// The shell's process id, once it said it.
    pid: Arc<OnceLock<u32>>,
}

impl Hangup {
    /// Sends the shell SIGHUP, as closing a terminal window does: the shell ends and takes what
    /// runs in it along.
    async fn run(self) {
        let Some(pid) = self.pid.get().copied() else {
            return;
        };
        let params = AttachParams::default().container(&self.container).stdout(false);
        let command = [
            "/bin/sh".to_string(),
            "-c".to_string(),
            format!("kill -HUP {pid}"),
        ];
        let hangup = async {
            if let Ok(process) = self.api.exec(&self.pod, command, &params).await {
                let _ = process.join().await;
            }
        };
        if tokio::time::timeout(HANGUP_TIMEOUT, hangup).await.is_err() {
            tracing::debug!(pod = %self.pod, pid, "the closed shell did not end in time");
        }
    }
}

/// Reads the process output until it exits or the stream is dropped.
fn output(
    mut process: AttachedProcess,
    tty: bool,
    hangup: Option<Hangup>,
) -> BoxStream<'static, TerminalOutput> {
    let (tx, rx) = mpsc::channel(64);
    let stdout = process.stdout();
    let stderr = process.stderr();
    let status = process.take_status();
    let pid = hangup.as_ref().map(|h| Arc::clone(&h.pid));
    tokio::spawn(async move {
        let pump = async {
            let stdout = stdout.map(|r| pump(Box::new(r), tty, pid.map(ShellId::new), tx.clone()));
            let stderr = stderr.map(|r| pump(Box::new(r), tty, None, tx.clone()));
            futures::future::join_all(stdout.into_iter().chain(stderr)).await;
            let message = match status {
                Some(status) => status.await.and_then(|s| exit_message(&s)),
                None => None,
            };
            let _ = tx.send(TerminalOutput::Exit { message }).await;
        };
        tokio::select! {
            _ = pump => {}
            // Closed while the process still runs.
            _ = tx.closed() => {
                if let Some(hangup) = hangup {
                    hangup.run().await;
                }
            }
        }
        // Dropping the process aborts the connection.
        drop(process);
    });
    stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|event| (event, rx)) },
    )
    .boxed()
}

async fn pump(
    mut reader: Box<dyn AsyncRead + Send + Unpin>,
    tty: bool,
    mut shell_id: Option<ShellId>,
    tx: mpsc::Sender<TerminalOutput>,
) {
    let mut buf = vec![0; 16 * 1024];
    loop {
        let mut data = match reader.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) if tty => buf[..n].to_vec(),
            // Without a TTY nothing turns \n into \r\n for the terminal emulator.
            Ok(n) => crlf(&buf[..n]),
        };
        if let Some(id) = &mut shell_id {
            data = id.take(&data);
            if id.done {
                shell_id = None;
            }
            if data.is_empty() {
                continue;
            }
        }
        if tx.send(TerminalOutput::Data { data }).await.is_err() {
            return;
        }
    }
}

/// Takes the process id that [`DEFAULT_SHELL`] says first out of its output.
struct ShellId {
    pid: Arc<OnceLock<u32>>,
    /// The start of the output, while it may still be the process id.
    held: Vec<u8>,
    done: bool,
}

impl ShellId {
    fn new(pid: Arc<OnceLock<u32>>) -> Self {
        Self {
            pid,
            held: Vec::new(),
            done: false,
        }
    }

    /// The part of `data` to show. Holds back what may be the start of the process id, and
    /// gives everything back as it was when the output starts with something else.
    fn take(&mut self, data: &[u8]) -> Vec<u8> {
        if self.done {
            return data.to_vec();
        }
        self.held.extend_from_slice(data);
        let held = &self.held;
        let prefix = held.len().min(SHELL_ID.len());
        if held[..prefix] != SHELL_ID[..prefix] {
            self.done = true;
            return std::mem::take(&mut self.held);
        }
        if held.len() <= SHELL_ID.len() {
            return Vec::new();
        }
        let rest = &held[SHELL_ID.len()..];
        match rest.iter().position(|&b| b == 0x07) {
            Some(end) => {
                if let Some(pid) = std::str::from_utf8(&rest[..end])
                    .ok()
                    .and_then(|s| s.parse().ok())
                {
                    let _ = self.pid.set(pid);
                }
                self.done = true;
                rest[end + 1..].to_vec()
            }
            // A process id has at most ten digits; this is something else.
            None if rest.len() > 10 => {
                self.done = true;
                std::mem::take(&mut self.held)
            }
            None => Vec::new(),
        }
    }
}

fn crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 16);
    let mut prev = 0;
    for &b in data {
        if b == b'\n' && prev != b'\r' {
            out.push(b'\r');
        }
        out.push(b);
        prev = b;
    }
    out
}

fn exit_message(status: &k8s_openapi::apimachinery::pkg::apis::meta::v1::Status) -> Option<String> {
    if status.status.as_deref() == Some("Success") {
        return None;
    }
    status.message.clone().filter(|m| !m.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pod(value: serde_json::Value) -> Pod {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn default_container_prefers_the_annotation() {
        let annotated = pod(json!({
            "metadata": { "annotations": { DEFAULT_CONTAINER_ANNOTATION: "app" } },
            "spec": { "containers": [{ "name": "istio-proxy" }, { "name": "app" }] }
        }));
        assert_eq!(default_container(&annotated).as_deref(), Some("app"));
        let plain = pod(json!({ "spec": { "containers": [{ "name": "web" }] } }));
        assert_eq!(default_container(&plain).as_deref(), Some("web"));
    }

    #[test]
    fn attach_uses_the_containers_stdin_and_tty() {
        let pod = pod(json!({ "spec": {
            "containers": [{ "name": "repl", "stdin": true, "tty": true }, { "name": "app" }],
            "ephemeralContainers": [{ "name": "debugger", "stdin": true }]
        } }));
        assert_eq!(attach_flags(&pod, "repl"), (true, true));
        assert_eq!(attach_flags(&pod, "app"), (false, false));
        assert_eq!(attach_flags(&pod, "debugger"), (true, false));
    }

    #[test]
    fn only_running_containers_take_a_terminal() {
        let pod = pod(json!({ "status": {
            "containerStatuses": [
                { "name": "web", "image": "", "imageID": "", "ready": true, "restartCount": 0,
                  "state": { "running": {} } },
                { "name": "app", "image": "", "imageID": "", "ready": false, "restartCount": 5,
                  "state": { "waiting": { "reason": "CrashLoopBackOff" } } },
                { "name": "job", "image": "", "imageID": "", "ready": false, "restartCount": 0,
                  "state": { "terminated": { "exitCode": 0, "reason": "Completed" } } },
                { "name": "odd", "image": "", "imageID": "", "ready": false, "restartCount": 0,
                  "state": { "terminated": { "exitCode": 3 } } }
            ],
            "initContainerStatuses": [
                { "name": "init", "image": "", "imageID": "", "ready": false, "restartCount": 0,
                  "state": { "terminated": { "exitCode": 0, "reason": "Completed" } } }
            ]
        } }));
        assert_eq!(not_running(&pod, "web"), None);
        assert_eq!(
            not_running(&pod, "app").as_deref(),
            Some("waiting: CrashLoopBackOff")
        );
        assert_eq!(not_running(&pod, "job").as_deref(), Some("terminated: Completed"));
        assert_eq!(not_running(&pod, "odd").as_deref(), Some("exited with code 3"));
        assert_eq!(
            not_running(&pod, "init").as_deref(),
            Some("terminated: Completed")
        );
        // Unknown: the API server decides.
        assert_eq!(not_running(&pod, "sidecar"), None);
    }

    #[test]
    fn non_tty_output_gets_carriage_returns() {
        assert_eq!(crlf(b"a\nb\r\nc\n"), b"a\r\nb\r\nc\r\n");
    }

    #[test]
    fn successful_exits_have_no_message() {
        let ok = serde_json::from_value(json!({ "status": "Success" })).unwrap();
        assert_eq!(exit_message(&ok), None);
        let failed = serde_json::from_value(json!({
            "status": "Failure", "message": "exec: \"/bin/sh\": not found"
        }))
        .unwrap();
        assert_eq!(
            exit_message(&failed).as_deref(),
            Some("exec: \"/bin/sh\": not found")
        );
    }

    /// Feeds `chunks` through a [`ShellId`] and returns what shows and the process id.
    fn shell_id(chunks: &[&[u8]]) -> (Vec<u8>, Option<u32>) {
        let pid = Arc::new(OnceLock::new());
        let mut id = ShellId::new(Arc::clone(&pid));
        let shown = chunks.iter().flat_map(|c| id.take(c)).collect();
        (shown, pid.get().copied())
    }

    #[test]
    fn the_shell_says_its_process_id_and_it_does_not_show() {
        let (shown, pid) = shell_id(&[b"\x1b]7777;kubri-shell=4711\x07/ # "]);
        assert_eq!((shown.as_slice(), pid), (&b"/ # "[..], Some(4711)));
        // Split anywhere, also inside the escape sequence.
        let (shown, pid) = shell_id(&[b"\x1b]77", b"77;kubri-shell=47", b"11\x07", b"$ "]);
        assert_eq!((shown.as_slice(), pid), (&b"$ "[..], Some(4711)));
        // Only the first output can be the process id.
        let (shown, _) = shell_id(&[b"\x1b]7777;kubri-shell=1\x07", b"\x1b]7777;kubri-shell=2\x07"]);
        assert_eq!(shown, b"\x1b]7777;kubri-shell=2\x07");
    }

    #[test]
    fn other_output_shows_as_it_is() {
        let (shown, pid) = shell_id(&[b"\x1b]0;title\x07", b"$ "]);
        assert_eq!((shown.as_slice(), pid), (&b"\x1b]0;title\x07$ "[..], None));
        let (shown, pid) = shell_id(&[b"sh: printf: not found\r\n"]);
        assert_eq!((shown.as_slice(), pid), (&b"sh: printf: not found\r\n"[..], None));
        // A sequence that never ends is given back.
        let long = b"\x1b]7777;kubri-shell=123456789012345";
        let (shown, pid) = shell_id(&[long]);
        assert_eq!((shown.as_slice(), pid), (&long[..], None));
    }
}
