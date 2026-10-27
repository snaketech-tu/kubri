//! Interactive shells in containers and attaching to a container's main process.

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

/// Starts bash if the image has it and sh otherwise, like k9s.
const DEFAULT_SHELL: [&str; 3] = [
    "/bin/sh",
    "-c",
    "export TERM=xterm-256color; command -v bash >/dev/null 2>&1 && exec bash || exec sh",
];
const DEFAULT_CONTAINER_ANNOTATION: &str = "kubectl.kubernetes.io/default-container";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalMode {
    /// Run a new shell in the container (`kubectl exec -it`).
    #[default]
    Shell,
    /// Connect to the container's main process (`kubectl attach -it`).
    Attach,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminalOptions {
    /// Defaults to the pod's default container.
    pub container: Option<String>,
    pub mode: TerminalMode,
    /// Command to run in [`TerminalMode::Shell`] instead of the default shell.
    pub command: Option<Vec<String>>,
    pub cols: u16,
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

    let (mut process, tty) = match options.mode {
        TerminalMode::Shell => {
            let params = AttachParams::interactive_tty().container(&container);
            let command: Vec<String> = match &options.command {
                Some(command) if !command.is_empty() => command.clone(),
                _ => DEFAULT_SHELL.iter().map(|s| s.to_string()).collect(),
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
        output: output(process, tty),
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

/// Reads the process output until it exits or the stream is dropped.
fn output(mut process: AttachedProcess, tty: bool) -> BoxStream<'static, TerminalOutput> {
    let (tx, rx) = mpsc::channel(64);
    let stdout = process.stdout();
    let stderr = process.stderr();
    let status = process.take_status();
    tokio::spawn(async move {
        let pump = async {
            let readers = stdout
                .map(|r| Box::new(r) as Box<dyn AsyncRead + Send + Unpin>)
                .into_iter()
                .chain(stderr.map(|r| Box::new(r) as Box<dyn AsyncRead + Send + Unpin>))
                .map(|r| pump(r, tty, tx.clone()));
            futures::future::join_all(readers).await;
            let message = match status {
                Some(status) => status.await.and_then(|s| exit_message(&s)),
                None => None,
            };
            let _ = tx.send(TerminalOutput::Exit { message }).await;
        };
        tokio::select! {
            _ = pump => {}
            _ = tx.closed() => {}
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

async fn pump(mut reader: Box<dyn AsyncRead + Send + Unpin>, tty: bool, tx: mpsc::Sender<TerminalOutput>) {
    let mut buf = vec![0; 16 * 1024];
    loop {
        let data = match reader.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) if tty => buf[..n].to_vec(),
            // Without a TTY nothing turns \n into \r\n for the terminal emulator.
            Ok(n) => crlf(&buf[..n]),
        };
        if tx.send(TerminalOutput::Data { data }).await.is_err() {
            return;
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
}
