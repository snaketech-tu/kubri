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
    #[error("{0} does not exist")]
    NotFound(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("invalid object: {0}")]
    InvalidObject(#[from] serde_json::Error),
    /// A kubeconfig setting kubectl refuses too; the text is kubectl's.
    #[error("invalid kubeconfig: {0}")]
    InvalidKubeconfig(String),
    #[error("failed to read kubeconfig: {0}")]
    Kubeconfig(#[from] kube::config::KubeconfigError),
    #[error("{}", kube_message(.0))]
    Kube(#[from] kube::Error),
    /// The target was picked in another context than the one Kubri is connected to now.
    #[error("the target belongs to context `{0}`, not the current one; review it again")]
    ContextChanged(String),
    #[error("timed out while connecting to the cluster")]
    Timeout,
    /// Changes are turned off; the text says why.
    #[error("Kubri is read-only ({0}), so it does not change the cluster")]
    ReadOnly(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
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
/// Other errors name only their outer layer ("client error (Connect)"), so the causes
/// below it follow ("… tcp connect error: Connection refused").
fn kube_message(err: &kube::Error) -> String {
    match err {
        kube::Error::Api(status) if status.code == 401 => format!(
            "the cluster did not accept the credentials in the kubeconfig ({})",
            if status.message.is_empty() {
                "Unauthorized"
            } else {
                &status.message
            }
        ),
        kube::Error::Api(status) if !status.message.is_empty() => status.message.clone(),
        other => {
            let mut message = other.to_string();
            let mut source = std::error::Error::source(other);
            while let Some(cause) = source {
                let text = cause.to_string();
                if !message.contains(&text) {
                    message.push_str(": ");
                    message.push_str(&text);
                }
                source = cause.source();
            }
            message
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::core::Status;

    #[derive(Debug, Error)]
    #[error("client error (Connect)")]
    struct Outer(#[source] Inner);

    #[derive(Debug, Error)]
    #[error("tcp connect error: {0}")]
    struct Inner(#[source] std::io::Error);

    #[test]
    fn api_errors_use_the_server_message() {
        let err = Error::from(kube::Error::Api(
            Status::failure("pods \"web\" not found", "NotFound")
                .with_code(404)
                .boxed(),
        ));
        assert_eq!(err.to_string(), "pods \"web\" not found");
        assert_eq!(err.api_status(), Some(404));
    }

    #[test]
    fn rejected_credentials_say_so() {
        let err = Error::from(kube::Error::Api(
            Status::failure("Unauthorized", "Unauthorized")
                .with_code(401)
                .boxed(),
        ));
        assert_eq!(
            err.to_string(),
            "the cluster did not accept the credentials in the kubeconfig (Unauthorized)"
        );
    }

    #[test]
    fn other_errors_name_their_causes_once() {
        let io = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "Connection refused");
        let err = Error::from(kube::Error::Service(Box::new(Outer(Inner(io)))));
        assert_eq!(
            err.to_string(),
            "ServiceError: client error (Connect): tcp connect error: Connection refused"
        );
    }
}
