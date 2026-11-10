//! A Kubernetes cluster on this computer, made with kind or k3d, for people who have none yet.
//!
//! Kubri runs the tool's own command in a terminal, so its progress shows as it would in a
//! shell. The tool adds the new context to the kubeconfig and makes it the current one; the
//! UI then reads the kubeconfig again and connects to it.

use std::ffi::OsString;

use serde::{Deserialize, Serialize};

use crate::local_terminal::{find_program, path_var};
use crate::plugin::{self, Prepared};
use crate::{Error, Result, Terminal};

/// The name of the cluster Kubri makes.
const NAME: &str = "kubri";

/// A tool that makes a cluster in containers on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LocalClusterTool {
    Kind,
    K3d,
}

impl LocalClusterTool {
    const ALL: [Self; 2] = [Self::Kind, Self::K3d];

    fn program(self) -> &'static str {
        match self {
            Self::Kind => "kind",
            Self::K3d => "k3d",
        }
    }

    fn args(self) -> Vec<String> {
        let args: &[&str] = match self {
            // Waits for the control plane, so that the first connection works.
            Self::Kind => &["create", "cluster", "--name", NAME, "--wait", "2m"],
            Self::K3d => &["cluster", "create", NAME, "--wait"],
        };
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    /// The context the tool adds to the kubeconfig.
    fn context(self) -> String {
        format!("{}-{NAME}", self.program())
    }

    fn install(self) -> &'static str {
        match self {
            Self::Kind => "https://kind.sigs.k8s.io/docs/user/quick-start/#installation",
            Self::K3d => "https://k3d.io/#installation",
        }
    }
}

/// A tool that can make a local cluster, and whether it is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalClusterOption {
    pub tool: LocalClusterTool,
    pub found: bool,
    /// The command Kubri runs, as it would be typed.
    pub command: String,
    /// The context the tool adds to the kubeconfig.
    pub context: String,
    /// Where to get the tool.
    pub install: String,
}

/// What Kubri can use to make a cluster on this computer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalClusters {
    /// Whether Kubri may run the tools here: in the desktop app, and in the web server only
    /// when it allows local terminals, as the cluster would run on the server.
    pub allowed: bool,
    /// Why not, when it may not.
    pub reason: Option<String>,
    pub tools: Vec<LocalClusterOption>,
    /// Whether Docker or Podman is installed: both tools run the cluster in containers.
    pub containers: bool,
}

/// Which tool to run, and the size of the terminal it runs in.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalClusterRequest {
    pub tool: LocalClusterTool,
    #[serde(default)]
    pub cols: u16,
    #[serde(default)]
    pub rows: u16,
}

/// The tools on `path`, and whether a container engine is there for them.
pub(crate) fn options_on(path: &OsString) -> (Vec<LocalClusterOption>, bool) {
    let tools = LocalClusterTool::ALL
        .into_iter()
        .map(|tool| LocalClusterOption {
            tool,
            found: find_program(tool.program(), path).is_some(),
            command: format!("{} {}", tool.program(), tool.args().join(" ")),
            context: tool.context(),
            install: tool.install().to_owned(),
        })
        .collect();
    let containers = ["docker", "podman"]
        .iter()
        .any(|p| find_program(p, path).is_some());
    (tools, containers)
}

/// The tools on this computer's `PATH`.
pub(crate) fn options() -> (Vec<LocalClusterOption>, bool) {
    options_on(&path_var())
}

/// Runs the tool in a terminal of its own; it ends when the cluster is ready or making it
/// failed.
pub(crate) fn create(request: &LocalClusterRequest) -> Result<Terminal> {
    let tool = request.tool;
    let program = find_program(tool.program(), &path_var()).ok_or_else(|| {
        Error::Unsupported(format!(
            "{} is not installed, get it from {}",
            tool.program(),
            tool.install()
        ))
    })?;
    let prepared = Prepared {
        program: program.to_string_lossy().into_owned(),
        args: tool.args(),
        env: Vec::new(),
        cwd: None,
    };
    plugin::open(
        &prepared,
        format!("{} cluster {NAME}", tool.program()),
        request.cols,
        request.rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_command_and_the_context_it_adds() {
        let dir = std::env::temp_dir().join(format!("kubri-local-cluster-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = if cfg!(windows) { "kind.exe" } else { "kind" };
        std::fs::write(dir.join(program), "").unwrap();

        let (tools, containers) = options_on(&dir.clone().into_os_string());
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(!containers);
        let kind = &tools[0];
        assert!(kind.found);
        assert_eq!(kind.command, "kind create cluster --name kubri --wait 2m");
        assert_eq!(kind.context, "kind-kubri");
        let k3d = &tools[1];
        assert!(!k3d.found);
        assert_eq!(k3d.command, "k3d cluster create kubri --wait");
        assert_eq!(k3d.context, "k3d-kubri");
        assert!(k3d.install.starts_with("https://"));
    }

    #[test]
    fn reads_the_tool_by_its_name() {
        let request: LocalClusterRequest =
            serde_json::from_str(r#"{"tool":"k3d","cols":100,"rows":30}"#).unwrap();
        assert_eq!(request.tool, LocalClusterTool::K3d);
        assert!(serde_json::from_str::<LocalClusterRequest>(r#"{"tool":"minikube"}"#).is_err());
    }
}
