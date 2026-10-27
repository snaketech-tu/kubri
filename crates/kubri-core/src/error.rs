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
    #[error("{0}")]
    Unsupported(String),
    #[error("invalid object: {0}")]
    InvalidObject(#[from] serde_json::Error),
    #[error("failed to read kubeconfig: {0}")]
    Kubeconfig(#[from] kube::config::KubeconfigError),
    #[error("{}", kube_message(.0))]
    Kube(#[from] kube::Error),
    #[error("timed out while connecting to the cluster")]
    Timeout,
}

impl Error {
    /// The HTTP status the API server answered with, if this is an API error.
    pub fn api_status(&self) -> Option<u16> {
        match self {
            Error::Kube(kube::Error::Api(status)) => Some(status.code),
            _ => None,
        }
    }
}

/// API errors carry a readable message from the server; prefer it over the debug form.
fn kube_message(err: &kube::Error) -> String {
    match err {
        kube::Error::Api(status) if !status.message.is_empty() => status.message.clone(),
        other => other.to_string(),
    }
}
