//! A shell on the user's own computer, with `kubectl` and `helm` set to the context and
//! namespace Kubri shows.
//!
//! `KUBECONFIG` puts a small overlay file in front of the user's own files. kubectl and helm
//! merge the list with the first file winning, so the overlay only needs `current-context` and
//! the context entry with the namespace: the cluster and user entries, and with them every
//! credential, still come from the user's files. Nothing secret is written to disk.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use futures::StreamExt;
use kube::config::{Context, Kubeconfig};
use serde::{Deserialize, Serialize};

use crate::plugin::{self, Prepared};
use crate::{Error, Guard, Kubri, Result, Terminal, TerminalOutput, team};

/// Overlay files older than this are left from a Kubri that did not exit cleanly.
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
/// How long `kubectl version --client` may take.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// Where the shell starts and what it runs, from the settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct TerminalSettings {
    /// The program to run; empty for the user's login shell, or PowerShell on Windows.
    pub shell: String,
    /// Its arguments, when `shell` is set.
    pub args: Vec<String>,
    /// The folder the shell starts in; empty for the home folder.
    pub start_dir: String,
    /// The terminal's font size in pixels.
    pub font_size: u32,
    /// Fonts to try before Kubri's own, separated by commas, like `MesloLGS NF`; empty for
    /// Kubri's.
    pub font_family: String,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: String::new(),
            args: Vec::new(),
            start_dir: String::new(),
            font_size: 13,
            font_family: String::new(),
        }
    }
}

/// What the UI asks for when it opens a local terminal.
#[derive(Debug, Clone, Default, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, optional_fields = nullable)]
pub struct LocalTerminalOptions {
    /// The namespace Kubri shows; `None` for all namespaces, which keeps the context's own.
    pub namespace: Option<String>,
    #[ts(as = "Option<u16>", optional)]
    pub cols: u16,
    #[ts(as = "Option<u16>", optional)]
    pub rows: u16,
}

/// What a local terminal would run with, shown in its tab before and while it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalTerminalInfo {
    /// Whether local terminals are on at all: always in the desktop app, in the web server
    /// only when started with `KUBRI_LOCAL_TERMINAL=true`.
    pub enabled: bool,
    /// Whether a terminal can be opened now.
    pub allowed: bool,
    /// Why not, when it cannot.
    pub reason: Option<String>,
    pub context: Option<String>,
    /// The shell's program.
    pub shell: String,
    /// `kubectl version --client`, or `None` when no `kubectl` is on the shell's `PATH`.
    pub kubectl: Option<String>,
    /// `helm version --short`, or `None` when no `helm` is on the `PATH`.
    pub helm: Option<String>,
    /// The API server's version, to compare with kubectl's.
    pub server: Option<String>,
    /// Set when kubectl's minor version is more than one away from the server's.
    pub skew: Option<String>,
}

/// The shell to run: the setting, or the user's own.
pub(crate) fn shell(settings: &TerminalSettings) -> (String, Vec<String>) {
    if !settings.shell.trim().is_empty() {
        return (settings.shell.trim().to_owned(), settings.args.clone());
    }
    if cfg!(windows) {
        let program = if find_program("pwsh", &path_var()).is_some() {
            "pwsh.exe"
        } else {
            "powershell.exe"
        };
        return (program.to_owned(), vec!["-NoLogo".to_owned()]);
    }
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if Path::new("/bin/bash").exists() {
                "/bin/bash".to_owned()
            } else {
                "/bin/sh".to_owned()
            }
        });
    // A login shell reads the profile, as a terminal app does.
    (shell, vec!["-l".to_owned()])
}

/// The overlay kubeconfig for `context` with `namespace`: only `current-context` and the
/// context entry.
pub(crate) fn overlay(name: &str, context: &Context, namespace: Option<&str>) -> String {
    let mut context = context.clone();
    if let Some(namespace) = namespace {
        context.namespace = Some(namespace.to_owned());
    }
    // JSON is YAML too, and kubectl and helm read it as a kubeconfig.
    serde_json::json!({
        "apiVersion": "v1",
        "kind": "Config",
        "current-context": name,
        "contexts": [{ "name": name, "context": context }],
    })
    .to_string()
}

/// The folder for overlay files, private to the user.
fn overlay_dir() -> Result<PathBuf> {
    let dir = crate::config_dir()
        .ok_or_else(|| Error::Unsupported("Kubri has no config folder for the terminal's kubeconfig".into()))?
        .join("terminals");
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

/// Writes `text` to a new overlay file that only the user can read.
fn write_overlay(dir: &Path, text: &str) -> Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = dir.join(format!(
        "kubeconfig-{}-{}.yaml",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options.open(&path)?.write_all(text.as_bytes())?;
    Ok(path)
}

/// Removes overlay files a Kubri left behind when it did not exit cleanly.
pub(crate) fn remove_stale_overlays() {
    let Some(dir) = crate::config_dir().map(|d| d.join("terminals")) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > STALE_AFTER);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Deletes the overlay file when the terminal's output is dropped.
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `KUBECONFIG` with `overlay` first, then the files kubectl would read, then the contexts
/// added in Kubri.
pub(crate) fn kubeconfig_value(overlay: &Path) -> OsString {
    let mut paths = vec![overlay.to_path_buf()];
    paths.extend(std::env::split_paths(&crate::kubeconfig::env_value()));
    std::env::join_paths(paths).unwrap_or_default()
}

/// Opens the shell in a PTY. `warning` is written first, as the terminal's own line.
pub(crate) fn open(
    settings: &TerminalSettings,
    kubeconfig: &Kubeconfig,
    name: &str,
    options: &LocalTerminalOptions,
    warning: Option<String>,
) -> Result<Terminal> {
    let entry = kubeconfig
        .contexts
        .iter()
        .find(|c| c.name == name)
        .and_then(|c| c.context.as_ref())
        .ok_or_else(|| Error::UnknownContext(name.to_owned()))?;
    let path = write_overlay(
        &overlay_dir()?,
        &overlay(name, entry, options.namespace.as_deref()),
    )?;
    let guard = RemoveOnDrop(path.clone());
    let namespace = options
        .namespace
        .clone()
        .or_else(|| entry.namespace.clone())
        .unwrap_or_else(|| "default".to_owned());
    let (program, args) = shell(settings);
    let cwd = Some(settings.start_dir.trim())
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    let prepared = Prepared {
        program,
        args,
        env: vec![
            (
                "KUBECONFIG".into(),
                kubeconfig_value(&path).to_string_lossy().into_owned(),
            ),
            ("KUBRI_CONTEXT".into(), name.to_owned()),
            ("KUBRI_NAMESPACE".into(), namespace),
        ],
        cwd,
    };
    let title = format!("{name} terminal");
    let terminal = plugin::open(&prepared, title, options.cols, options.rows)?;
    let first = warning.map(|text| TerminalOutput::Data {
        data: format!("\x1b[1;31m{text}\x1b[0m\r\n").into_bytes(),
    });
    let output = futures::stream::iter(first)
        .chain(terminal.output)
        .map(move |event| {
            let _keep = &guard;
            event
        })
        .boxed();
    Ok(Terminal { output, ..terminal })
}

/// The `PATH` the shell gets: this process's, which the desktop app filled from the login
/// shell at startup.
pub(crate) fn path_var() -> OsString {
    std::env::var_os("PATH").unwrap_or_default()
}

/// The first `program` on `path`, as the shell would find it.
pub(crate) fn find_program(program: &str, path: &OsString) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        vec![
            format!("{program}.exe"),
            format!("{program}.cmd"),
            program.to_owned(),
        ]
    } else {
        vec![program.to_owned()]
    };
    std::env::split_paths(path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// Runs `program args` and returns its output, or `None` when it fails or takes too long.
fn run(program: &Path, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < VERSION_TIMEOUT => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The versions of `kubectl` and `helm` on the `PATH`.
pub(crate) fn tools() -> (Option<String>, Option<String>) {
    let path = path_var();
    let kubectl = find_program("kubectl", &path).and_then(|p| {
        let json = run(&p, &["version", "--client", "-o", "json"])?;
        let value: serde_json::Value = serde_json::from_str(&json).ok()?;
        value["clientVersion"]["gitVersion"].as_str().map(str::to_owned)
    });
    let helm = find_program("helm", &path).and_then(|p| run(&p, &["version", "--short"]));
    (kubectl, helm)
}

/// The minor version of a Kubernetes version such as `v1.33.5+k3s1`.
fn minor(version: &str) -> Option<u32> {
    let mut parts = version.trim_start_matches('v').split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor = parts.next()?;
    let digits: String = minor.chars().take_while(char::is_ascii_digit).collect();
    (major == 1).then(|| digits.parse().ok()).flatten()
}

/// A note when kubectl is outside the version skew Kubernetes supports: one minor version
/// older or newer than the API server.
pub(crate) fn skew(kubectl: &str, server: &str) -> Option<String> {
    let (client, cluster) = (minor(kubectl)?, minor(server)?);
    (client.abs_diff(cluster) > 1).then(|| {
        format!(
            "kubectl {kubectl} is more than one minor version away from the cluster's {server}; some commands may not work"
        )
    })
}

impl Kubri {
    /// Lets local terminals be opened, and contexts that run a command to sign in or name
    /// files be added: the desktop app calls this, the web server only when started with
    /// `KUBRI_LOCAL_TERMINAL=true`, as both run on the server for everyone who signs in.
    pub fn allow_local_terminal(&self) {
        self.local_terminal.store(true, Ordering::Relaxed);
        remove_stale_overlays();
    }

    /// Why a local terminal cannot be opened for the connected context now, if it cannot.
    pub(crate) async fn local_terminal_refusal(&self) -> Option<String> {
        if self.act_as.is_some() {
            return Some(team::NO_LOCAL_TERMINAL.into());
        }
        if !self.local_terminal.load(Ordering::Relaxed) {
            return Some(
                "The local terminal is off here: it would be a shell on the server. Start the server with KUBRI_LOCAL_TERMINAL=true to allow it."
                    .into(),
            );
        }
        if self.agent.load(Ordering::Relaxed) {
            return Some("AI agents cannot open a local terminal.".into());
        }
        let Some(cluster) = self.cluster().await else {
            return Some("Kubri is not connected to a context.".into());
        };
        if cluster.snapshot.is_some() {
            return Some("A snapshot has no cluster for kubectl to talk to.".into());
        }
        match self.guard().await {
            Guard {
                read_only: true,
                reason,
                ..
            } => {
                let reason = reason.unwrap_or_else(|| "changes are off for this context".into());
                let reason = reason.trim_end_matches('.');
                let mut chars = reason.chars();
                let first = chars.next().map(|c| c.to_uppercase().collect::<String>());
                Some(format!(
                    "{}{}. A local terminal would get around that, so it stays closed.",
                    first.unwrap_or_default(),
                    chars.as_str()
                ))
            }
            _ => None,
        }
    }

    /// What a local terminal would run with, and whether one can be opened now.
    pub async fn local_terminal_info(&self) -> LocalTerminalInfo {
        let reason = self.local_terminal_refusal().await;
        let cluster = self.cluster().await.filter(|c| c.snapshot.is_none());
        let settings = self.settings().terminal;
        let (kubectl, helm) = if reason.is_none() {
            tokio::task::spawn_blocking(tools).await.unwrap_or_default()
        } else {
            (None, None)
        };
        let server = cluster.as_ref().map(|c| c.version.clone());
        let skew = kubectl
            .as_deref()
            .zip(server.as_deref())
            .and_then(|(k, s)| skew(k, s));
        LocalTerminalInfo {
            enabled: self.local_terminal.load(Ordering::Relaxed),
            allowed: reason.is_none(),
            reason,
            context: cluster.map(|c| c.context),
            shell: shell(&settings).0,
            kubectl,
            helm,
            server,
            skew,
        }
    }

    /// Opens a shell on this computer with `kubectl` and `helm` pointing at the connected
    /// context and `options.namespace`.
    pub async fn open_local_terminal(&self, options: &LocalTerminalOptions) -> Result<Terminal> {
        if let Some(reason) = self.local_terminal_refusal().await {
            return Err(Error::Unsupported(reason));
        }
        let cluster = self.cluster().await.ok_or(Error::NotConnected)?;
        let kubeconfig = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => kubeconfig.clone(),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        let settings = self.settings();
        let warning = settings.context(&cluster.context).production.then(|| {
            format!(
                "Production: commands here run against {} without Kubri's checks.",
                cluster.context
            )
        });
        tracing::info!(context = cluster.context, "opening a local terminal");
        open(
            &settings.terminal,
            &kubeconfig,
            &cluster.context,
            options,
            warning,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(namespace: Option<&str>) -> Context {
        Context {
            cluster: "lab".into(),
            user: Some("admin".into()),
            namespace: namespace.map(str::to_owned),
            extensions: None,
            other: Default::default(),
        }
    }

    #[test]
    fn the_overlay_holds_only_the_context() {
        let text = overlay("prod", &context(Some("default")), Some("shop"));
        let kubeconfig = Kubeconfig::from_yaml(&text).unwrap();
        assert_eq!(kubeconfig.current_context.as_deref(), Some("prod"));
        assert_eq!(kubeconfig.contexts.len(), 1);
        let entry = kubeconfig.contexts[0].context.as_ref().unwrap();
        assert_eq!(entry.namespace.as_deref(), Some("shop"));
        assert_eq!(
            (entry.cluster.as_str(), entry.user.as_deref()),
            ("lab", Some("admin"))
        );
        assert!(kubeconfig.clusters.is_empty() && kubeconfig.auth_infos.is_empty());
        // All namespaces keep the context's own.
        let text = overlay("prod", &context(Some("default")), None);
        let kubeconfig = Kubeconfig::from_yaml(&text).unwrap();
        assert_eq!(
            kubeconfig.contexts[0]
                .context
                .as_ref()
                .unwrap()
                .namespace
                .as_deref(),
            Some("default")
        );
    }

    #[test]
    fn overlay_files_are_private_and_removed() {
        let dir = std::env::temp_dir().join(format!("kubri-overlay-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_overlay(&dir, "x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(RemoveOnDrop(path.clone()));
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn version_skew() {
        assert_eq!(minor("v1.33.5+k3s1"), Some(33));
        assert_eq!(minor("v1.30.0-eks-1"), Some(30));
        assert_eq!(minor("nonsense"), None);
        assert!(skew("v1.33.0", "v1.34.2").is_none());
        assert!(skew("v1.32.0", "v1.33.1+k3s1").is_none());
        assert!(
            skew("v1.30.0", "v1.33.1")
                .unwrap()
                .contains("more than one minor version")
        );
        assert!(skew("v1.36.0", "v1.33.1").is_some());
    }

    #[test]
    fn settings_without_a_font_family_still_load() {
        let settings: TerminalSettings = serde_json::from_str(r#"{"shell":"zsh","fontSize":14}"#).unwrap();
        assert_eq!(settings.font_size, 14);
        assert_eq!(settings.font_family, "");
    }

    #[test]
    fn a_shell_from_the_settings_wins() {
        let settings = TerminalSettings {
            shell: "wsl.exe".into(),
            args: vec!["-d".into(), "Ubuntu".into()],
            ..Default::default()
        };
        assert_eq!(
            shell(&settings),
            ("wsl.exe".to_owned(), vec!["-d".to_owned(), "Ubuntu".to_owned()])
        );
        let (program, _) = shell(&TerminalSettings::default());
        assert!(!program.is_empty());
    }
}
