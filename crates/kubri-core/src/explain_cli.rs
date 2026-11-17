//! Explain through Claude Code or Codex, the command-line tools a person may already have
//! installed and signed in to: Kubri runs `claude -p` or `codex exec` with the prompt on stdin,
//! so no API key is needed.
//!
//! The tools run on the computer Kubri runs on. In the web app that is the server, so like the
//! local terminal they are only used there when it is started with `KUBRI_LOCAL_TERMINAL=true`,
//! and never on a Kubri shared by a team. They get no tools of their own: Claude Code runs with
//! none, Codex in its read-only sandbox, both in an empty folder.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::explain::{ExplainAnswer, Provider, SYSTEM_PROMPT};
use crate::local_terminal::{find_program, path_var};
use crate::{Error, Kubri, Result};

/// How long a tool may take to answer.
const TIMEOUT: Duration = Duration::from_secs(300);
/// How long `--version` may take; both start a runtime first.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Why Claude Code and Codex are not used in a web app started without the local terminal.
pub(crate) const OFF_ON_SERVER: &str = "Claude Code and Codex would run on the server here. Start the server with KUBRI_LOCAL_TERMINAL=true to allow it.";
/// Why a Kubri shared by a team does not use them.
pub(crate) const OFF_FOR_TEAMS: &str = "A Kubri shared by a team cannot use Claude Code or Codex: they would run on the server, with its own sign-in.";
const NO_CLAUDE: &str =
    "Kubri finds no `claude` command. Install Claude Code, sign in by running `claude` once, then try again.";
const NO_CODEX: &str =
    "Kubri finds no `codex` command. Install Codex, sign in with `codex login`, then try again.";

/// A command-line tool Explain can use, as found on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExplainCli {
    pub provider: Provider,
    /// The program Kubri would run, or `None` when it finds none.
    pub path: Option<String>,
    /// What `--version` printed, when it did.
    pub version: Option<String>,
}

/// Whether Explain can use Claude Code and Codex here, and where they are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExplainClis {
    /// Why they cannot be used here, when they cannot; then `clis` is empty.
    pub reason: Option<String>,
    pub clis: Vec<ExplainCli>,
}

/// The name of the command a provider runs, for providers that run one.
pub(crate) fn command(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::Claude => Some("claude"),
        Provider::Codex => Some("codex"),
        _ => None,
    }
}

/// The program to run for `provider`: the first on the `PATH`, or else one in a folder their
/// installers use, as an app started from a launcher often has a short `PATH`.
pub(crate) fn program(provider: Provider) -> Option<PathBuf> {
    let name = command(provider)?;
    find_program(name, &path_var())
        .or_else(|| find_program(name, &std::env::join_paths(install_dirs()).unwrap_or_default()))
}

/// Folders the installers of Claude Code and Codex, npm and the usual package managers put
/// commands in.
fn install_dirs() -> Vec<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let mut dirs = Vec::new();
    if cfg!(windows) {
        if let Some(home) = var("USERPROFILE") {
            dirs.push(home.join(".local").join("bin"));
        }
        if let Some(data) = var("APPDATA") {
            dirs.push(data.join("npm"));
        }
    } else if let Some(home) = var("HOME") {
        for dir in [".claude/local", ".npm-global/bin", ".bun/bin", ".volta/bin"] {
            dirs.push(home.join(dir));
        }
        // Node versions of nvm, newest first.
        let mut nodes: Vec<PathBuf> = std::fs::read_dir(home.join(".nvm/versions/node"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("bin"))
            .collect();
        nodes.sort_by_key(|p| std::cmp::Reverse(node_version(p)));
        dirs.extend(nodes);
    }
    dirs.extend(crate::helm::install_dirs());
    dirs
}

/// `v22.11.0/bin` as `[22, 11, 0]`, to sort nvm's folders.
fn node_version(bin: &Path) -> Vec<u64> {
    bin.parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(|n| {
            n.trim_start_matches('v')
                .split('.')
                .map(|part| part.parse().unwrap_or(0))
                .collect()
        })
        .unwrap_or_default()
}

/// A command for `program` that opens no console window on Windows. Its own folder goes first
/// on the `PATH`: an npm install starts with `env node`, and the `node` beside it is the one it
/// was installed with, even when the app was started with a short `PATH`.
fn quiet(program: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    if let Some(dir) = program.parent().filter(|d| !d.as_os_str().is_empty()) {
        let path = path_var();
        let dirs = std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&path));
        if let Ok(path) = std::env::join_paths(dirs) {
            command.env("PATH", path);
        }
    }
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.kill_on_drop(true);
    command
}

async fn version(program: &Path) -> Option<String> {
    let output = quiet(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let output = tokio::time::timeout(VERSION_TIMEOUT, output).await.ok()?.ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    output.status.success().then(|| version_number(&text)).flatten()
}

/// The version number in what `--version` printed, such as `2.1.3` in `2.1.3 (Claude Code)` or
/// `codex-cli 0.168.0`; the first line when it has none.
fn version_number(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let number = line
        .split_whitespace()
        .map(|word| word.trim_start_matches('v'))
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()));
    Some(number.unwrap_or(line).to_owned())
}

impl Kubri {
    /// Why Explain cannot run Claude Code or Codex here, if it cannot.
    pub(crate) fn explain_cli_refusal(&self) -> Option<&'static str> {
        if self.act_as.is_some() {
            Some(OFF_FOR_TEAMS)
        } else if !self.local_terminal.load(Ordering::Relaxed) {
            Some(OFF_ON_SERVER)
        } else {
            None
        }
    }

    /// Where Claude Code and Codex are, for the settings.
    pub async fn explain_clis(&self) -> ExplainClis {
        if let Some(reason) = self.explain_cli_refusal() {
            return ExplainClis {
                reason: Some(reason.into()),
                clis: Vec::new(),
            };
        }
        let find = |provider| async move {
            let path = tokio::task::spawn_blocking(move || program(provider))
                .await
                .ok()
                .flatten();
            let version = match &path {
                Some(path) => version(path).await,
                None => None,
            };
            ExplainCli {
                provider,
                path: path.map(|p| p.display().to_string()),
                version,
            }
        };
        let (claude, codex) = tokio::join!(find(Provider::Claude), find(Provider::Codex));
        ExplainClis {
            reason: None,
            clis: vec![claude, codex],
        }
    }

    /// For a provider that runs a command: the program, or why Explain cannot run it.
    pub(crate) async fn explain_cli(&self, provider: Provider) -> Result<PathBuf> {
        if let Some(reason) = self.explain_cli_refusal() {
            return Err(Error::Unsupported(reason.into()));
        }
        let missing = match provider {
            Provider::Codex => NO_CODEX,
            _ => NO_CLAUDE,
        };
        tokio::task::spawn_blocking(move || program(provider))
            .await
            .ok()
            .flatten()
            .ok_or_else(|| Error::Unsupported(missing.into()))
    }
}

fn model_error(message: impl Into<String>) -> Error {
    Error::Model(message.into())
}

/// An empty folder for one run, so neither tool picks up instructions or files of a project,
/// removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kubri-explain-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The arguments for `provider`; the prompt goes to stdin.
fn args(provider: Provider, model: &str, scratch: &Path) -> Vec<String> {
    let mut args: Vec<String> = match provider {
        Provider::Codex => [
            "exec",
            "--skip-git-repo-check",
            "--ephemeral",
            "--sandbox",
            "read-only",
            "--color",
            "never",
        ]
        .map(String::from)
        .into(),
        _ => [
            "-p",
            "--output-format",
            "json",
            "--tools",
            "",
            "--strict-mcp-config",
            "--no-session-persistence",
            "--system-prompt",
            SYSTEM_PROMPT,
        ]
        .map(String::from)
        .into(),
    };
    if !model.is_empty() {
        // One argument, so a model name cannot pass for an option of its own.
        args.push(format!("--model={model}"));
    }
    if provider == Provider::Codex {
        args.extend([
            "--cd".to_owned(),
            scratch.display().to_string(),
            "--output-last-message".to_owned(),
            scratch.join("answer.md").display().to_string(),
            "-".to_owned(),
        ]);
    }
    args
}

/// What goes to stdin: Codex has no option for instructions, so they come first.
fn stdin(provider: Provider, text: &str) -> String {
    match provider {
        Provider::Codex => format!("{SYSTEM_PROMPT}\n\n---\n\n{text}"),
        _ => text.to_owned(),
    }
}

/// Why a tool failed, from what it printed on stderr: its last line about an error, or else its
/// last lines.
fn tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = match lines
        .iter()
        .rev()
        .find(|l| l.to_ascii_lowercase().contains("error"))
    {
        Some(line) => (*line).to_owned(),
        None => lines[lines.len().saturating_sub(3)..].join("\n"),
    };
    let mut start = tail.len().saturating_sub(600);
    while !tail.is_char_boundary(start) {
        start += 1;
    }
    tail[start..].to_owned()
}

/// Runs `program` for `provider` with `text`, as Explain sends it, and returns the answer.
pub(crate) async fn ask(
    provider: Provider,
    program: &Path,
    model: &str,
    text: &str,
) -> Result<ExplainAnswer> {
    use tokio::io::AsyncWriteExt;
    let name = command(provider).unwrap_or("the tool");
    let scratch = Scratch::new()?;
    let mut child = quiet(program)
        .args(args(provider, model, &scratch.0))
        .current_dir(&scratch.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| model_error(format!("cannot run {}: {e}", program.display())))?;
    let mut input = child.stdin.take().expect("stdin is piped");
    let prompt = stdin(provider, text);
    // Written beside the wait, so a tool that prints before it has read everything cannot block.
    let write = async move {
        let _ = input.write_all(prompt.as_bytes()).await;
        // Dropping stdin closes it, which tells the tool that the prompt is complete.
    };
    let (_, output) = tokio::time::timeout(TIMEOUT, async { tokio::join!(write, child.wait_with_output()) })
        .await
        .map_err(|_| model_error(format!("{name} did not answer within 5 minutes")))?;
    let output = output.map_err(|e| model_error(format!("{name} broke off: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    match provider {
        Provider::Codex => {
            let answer = std::fs::read_to_string(scratch.0.join("answer.md")).unwrap_or_default();
            codex_answer(output.status.success(), &answer, &stdout, &stderr, model)
        }
        _ => claude_answer(&stdout, &stderr, model),
    }
}

/// Claude Code's answer, from what `--output-format json` printed.
fn claude_answer(stdout: &str, stderr: &str, asked: &str) -> Result<ExplainAnswer> {
    let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) else {
        let why = tail(stderr);
        let why = if why.is_empty() { tail(stdout) } else { why };
        return Err(model_error(if why.is_empty() {
            "claude ended without an answer".to_owned()
        } else {
            format!("claude ended without an answer: {why}")
        }));
    };
    let text = value.get("result").and_then(Value::as_str).unwrap_or("").trim();
    if value.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Err(model_error(format!(
            "claude answered with an error: {}",
            if text.is_empty() { "no message" } else { text }
        )));
    }
    if text.is_empty() {
        return Err(model_error("claude gave no answer"));
    }
    // The model that wrote the most, in case Claude Code asked a smaller one on the side.
    let model = value
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|usage| {
            usage
                .iter()
                .max_by_key(|(_, u)| u.get("outputTokens").and_then(Value::as_u64).unwrap_or(0))
        })
        .map(|(model, _)| model.clone());
    Ok(ExplainAnswer {
        text: text.to_owned(),
        model: model
            .or_else(|| (!asked.is_empty()).then(|| asked.to_owned()))
            .unwrap_or_else(|| "Claude Code".to_owned()),
        cut_short: value.get("stop_reason").and_then(Value::as_str) == Some("max_tokens"),
    })
}

/// Codex's answer: the last message it wrote to the file, or else what it printed.
fn codex_answer(ok: bool, answer: &str, stdout: &str, stderr: &str, asked: &str) -> Result<ExplainAnswer> {
    let text = if answer.trim().is_empty() { stdout } else { answer }.trim();
    if !ok || text.is_empty() {
        let why = tail(stderr);
        return Err(model_error(if why.is_empty() {
            "codex ended without an answer".to_owned()
        } else {
            format!("codex ended without an answer: {why}")
        }));
    }
    // Codex names the model in the header it prints to stderr.
    let model = stderr
        .lines()
        .find_map(|line| line.trim().strip_prefix("model:"))
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty());
    Ok(ExplainAnswer {
        text: text.to_owned(),
        model: model
            .or_else(|| (!asked.is_empty()).then(|| asked.to_owned()))
            .unwrap_or_else(|| "Codex".to_owned()),
        cut_short: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_runs_without_tools_and_reads_the_prompt_from_stdin() {
        let args = args(Provider::Claude, "sonnet", Path::new("/tmp/x"));
        assert_eq!(args[..3], ["-p", "--output-format", "json"]);
        let tools = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[tools + 1], "");
        assert!(args.contains(&"--strict-mcp-config".to_owned()));
        assert_eq!(args.last().unwrap(), "--model=sonnet");
        assert_eq!(stdin(Provider::Claude, "Why?"), "Why?");
        let default = super::args(Provider::Claude, "", Path::new("/tmp/x"));
        assert!(!default.iter().any(|a| a.starts_with("--model")));
    }

    #[test]
    fn codex_runs_read_only_in_the_scratch_folder() {
        let args = args(Provider::Codex, "", Path::new("/tmp/x"));
        assert_eq!(args[0], "exec");
        let sandbox = args.iter().position(|a| a == "--sandbox").unwrap();
        assert_eq!(args[sandbox + 1], "read-only");
        assert!(!args.iter().any(|a| a.starts_with("--model")));
        assert_eq!(args.last().unwrap(), "-");
        let args = super::args(Provider::Codex, "gpt-6", Path::new("/tmp/x"));
        assert!(args.contains(&"--model=gpt-6".to_owned()));
        assert!(stdin(Provider::Codex, "Why?").starts_with(SYSTEM_PROMPT));
        assert!(stdin(Provider::Codex, "Why?").ends_with("\n\nWhy?"));
    }

    #[test]
    fn reads_claudes_answer() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":" OOMKilled. ",
            "stop_reason":"end_turn","modelUsage":{"claude-haiku-5-5":{"outputTokens":3},"claude-opus-5-5":{"outputTokens":300}}}"#;
        let answer = claude_answer(ok, "", "").unwrap();
        assert_eq!(answer.text, "OOMKilled.");
        assert_eq!(answer.model, "claude-opus-5-5");
        assert!(!answer.cut_short);

        let login = r#"{"type":"result","is_error":true,"result":"Not logged in · Please run /login"}"#;
        let err = claude_answer(login, "", "").unwrap_err().to_string();
        assert_eq!(
            err,
            "Explain: claude answered with an error: Not logged in · Please run /login"
        );

        let err = claude_answer("", "error: unknown option '--tools'\n", "").unwrap_err();
        assert!(err.to_string().ends_with("unknown option '--tools'"), "{err}");
        let no_usage = r#"{"is_error":false,"result":"Fine."}"#;
        assert_eq!(claude_answer(no_usage, "", "").unwrap().model, "Claude Code");
        assert_eq!(claude_answer(no_usage, "", "sonnet").unwrap().model, "sonnet");
    }

    #[test]
    fn reads_codexs_answer() {
        let header = "OpenAI Codex v0.168.0\n--------\nworkdir: /tmp/x\nmodel: gpt-6\nprovider: openai\n";
        let answer = codex_answer(true, "The probe fails.\n", "", header, "").unwrap();
        assert_eq!(answer.text, "The probe fails.");
        assert_eq!(answer.model, "gpt-6");
        assert_eq!(
            codex_answer(true, "", "From stdout", "", "").unwrap().model,
            "Codex"
        );
        let err = codex_answer(false, "", "", "a\nERROR: Not logged in\n  at main.js:1\n", "").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Explain: codex ended without an answer: ERROR: Not logged in"
        );
        let err = codex_answer(false, "", "", "a\nb\nc\nd\n", "").unwrap_err();
        assert_eq!(err.to_string(), "Explain: codex ended without an answer: b\nc\nd");
    }

    #[test]
    fn finds_version_numbers() {
        assert_eq!(version_number("2.1.326 (Claude Code)\n").unwrap(), "2.1.326");
        assert_eq!(version_number("codex-cli 0.168.0").unwrap(), "0.168.0");
        assert_eq!(version_number("tool v1.2").unwrap(), "1.2");
        assert_eq!(version_number("dev build").unwrap(), "dev build");
        assert_eq!(version_number("  \n"), None);
    }

    #[test]
    fn sorts_node_versions() {
        assert_eq!(
            node_version(Path::new("/h/.nvm/versions/node/v22.11.0/bin")),
            [22, 11, 0]
        );
        assert!(node_version(Path::new("/v9.0.0/bin")) < node_version(Path::new("/v22.1.0/bin")));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_a_cli_with_the_prompt_on_stdin() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new().unwrap();
        let fake = dir.0.join("claude");
        // Answers with what it read, and the folder it ran in.
        std::fs::write(
            &fake,
            "#!/bin/sh\nread -r line\nprintf '{\"is_error\":false,\"result\":\"%s in %s\"}' \"$line\" \"$(pwd)\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let answer = ask(Provider::Claude, &fake, "", "Why?").await.unwrap();
        assert!(answer.text.starts_with("Why? in "), "{}", answer.text);
        assert!(answer.text.contains("kubri-explain-"), "{}", answer.text);
        let scratch = answer.text.trim_start_matches("Why? in ");
        assert!(!Path::new(scratch).exists(), "the scratch folder is removed");

        let codex = dir.0.join("codex");
        // Writes its answer to the file after --output-last-message.
        std::fs::write(
            &codex,
            "#!/bin/sh\nwhile [ \"$1\" != --output-last-message ]; do shift; done\ncat > /dev/null\necho 'model: gpt-6' >&2\necho 'Fine.' > \"$2\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o755)).unwrap();
        let answer = ask(Provider::Codex, &codex, "", "Why?").await.unwrap();
        assert_eq!((answer.text.as_str(), answer.model.as_str()), ("Fine.", "gpt-6"));
    }

    #[tokio::test]
    async fn runs_only_where_local_terminals_are_allowed() {
        let kubri = Kubri::from_kubeconfig(kube::config::Kubeconfig::default());
        assert_eq!(kubri.explain_clis().await.reason.as_deref(), Some(OFF_ON_SERVER));
        let err = kubri.explain_cli(Provider::Claude).await.unwrap_err();
        assert_eq!(err.to_string(), OFF_ON_SERVER);
        let claude = crate::ExplainSettings {
            provider: Provider::Codex,
            ..Default::default()
        };
        let err = kubri.set_explain_settings(claude).unwrap_err();
        assert_eq!(err.to_string(), OFF_ON_SERVER);
        kubri.local_terminal.store(true, Ordering::Relaxed);
        let clis = kubri.explain_clis().await;
        assert_eq!(clis.reason, None);
        assert_eq!(clis.clis.len(), 2);
    }
}
