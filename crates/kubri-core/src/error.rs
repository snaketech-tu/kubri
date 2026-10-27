use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no kubeconfig loaded: {0}")]
    NoKubeconfig(String),
    #[error("context `{0}` does not exist in the kubeconfig")]
    UnknownContext(String),
    #[error("not connected to a cluster, select a context first")]
    NotConnected,
    #[error("resource `{0}` is not served by the cluster")]
    UnknownResource(String),
    #[error("failed to read kubeconfig: {0}")]
    Kubeconfig(#[from] kube::config::KubeconfigError),
    #[error("kubernetes API error: {0}")]
    Kube(#[from] kube::Error),
    #[error("timed out while connecting to the cluster")]
    Timeout,
}
