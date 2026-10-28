use std::path::{Path, PathBuf};

use kube::config::Kubeconfig;
use serde::Serialize;

/// Where Kubernetes mounts a pod's service account.
const SERVICE_ACCOUNT: &str = "/var/run/secrets/kubernetes.io/serviceaccount";

/// A context entry from the merged kubeconfig, as shown in the context switcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextInfo {
    pub name: String,
    pub cluster: Option<String>,
    pub user: Option<String>,
    pub namespace: Option<String>,
    /// The kubeconfig's `current-context`.
    pub is_default: bool,
}

/// Inside a pod without a kubeconfig, writes one for the pod's service account, with the
/// context `in-cluster`, and returns its path to put in `KUBECONFIG`. Kubri, `helm`, `kubectl`
/// and k9s plugins then all use it. `None` outside a pod, or when there is a kubeconfig.
pub fn write_in_cluster_kubeconfig() -> Option<std::io::Result<PathBuf>> {
    let has_kubeconfig = std::env::var_os("KUBECONFIG").is_some()
        || dirs::home_dir().is_some_and(|home| home.join(".kube").join("config").exists());
    if has_kubeconfig {
        return None;
    }
    let host = std::env::var("KUBERNETES_SERVICE_HOST").ok()?;
    let port = std::env::var("KUBERNETES_SERVICE_PORT").ok()?;
    let kubeconfig = in_cluster(&host, &port, Path::new(SERVICE_ACCOUNT))?;
    let path = std::env::temp_dir().join("kubri-in-cluster.kubeconfig");
    // JSON is YAML too. The file holds only paths: the token stays in its own file, which
    // Kubernetes renews.
    Some(write_new(&path, kubeconfig.to_string().as_bytes()).map(|()| path))
}

/// Replaces `path` with a new file, never writing through a link someone put there.
fn write_new(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
        _ => {}
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(contents)
}

/// A kubeconfig for the service account mounted in `dir`.
fn in_cluster(host: &str, port: &str, dir: &Path) -> Option<serde_json::Value> {
    let token = dir.join("token");
    if !token.exists() {
        return None;
    }
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let namespace = std::fs::read_to_string(dir.join("namespace"))
        .ok()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    Some(serde_json::json!({
        "apiVersion": "v1",
        "kind": "Config",
        "clusters": [{
            "name": "in-cluster",
            "cluster": {
                "server": format!("https://{host}:{port}"),
                "certificate-authority": dir.join("ca.crt"),
            },
        }],
        "users": [{ "name": "service-account", "user": { "tokenFile": token } }],
        "contexts": [{
            "name": "in-cluster",
            "context": { "cluster": "in-cluster", "user": "service-account", "namespace": namespace },
        }],
        "current-context": "in-cluster",
    }))
}

pub(crate) fn list_contexts(kubeconfig: &Kubeconfig) -> Vec<ContextInfo> {
    let default = kubeconfig.current_context.as_deref();
    let mut contexts: Vec<ContextInfo> = kubeconfig
        .contexts
        .iter()
        .map(|named| {
            let ctx = named.context.as_ref();
            ContextInfo {
                name: named.name.clone(),
                cluster: ctx.map(|c| c.cluster.clone()).filter(|c| !c.is_empty()),
                user: ctx.and_then(|c| c.user.clone()),
                namespace: ctx.and_then(|c| c.namespace.clone()),
                is_default: Some(named.name.as_str()) == default,
            }
        })
        .collect();
    contexts.sort_by(|a, b| a.name.cmp(&b.name));
    contexts
}

#[cfg(test)]
mod tests {
    use super::*;

    const KUBECONFIG: &str = r#"
apiVersion: v1
kind: Config
current-context: prod
clusters:
  - name: prod-cluster
    cluster: { server: "https://prod.example.com" }
  - name: dev-cluster
    cluster: { server: "https://dev.example.com" }
users:
  - name: admin
    user: { token: "secret" }
contexts:
  - name: prod
    context: { cluster: prod-cluster, user: admin, namespace: web }
  - name: dev
    context: { cluster: dev-cluster, user: admin }
"#;

    #[test]
    fn uses_the_service_account_in_a_pod() {
        let dir = crate::k9s::tests::scratch("in-cluster");
        assert!(in_cluster("10.96.0.1", "443", &dir).is_none());
        std::fs::write(dir.join("token"), "token").unwrap();
        std::fs::write(dir.join("namespace"), "kubri\n").unwrap();

        let json = in_cluster("10.96.0.1", "443", &dir).unwrap().to_string();
        let kubeconfig = Kubeconfig::from_yaml(&json).unwrap();
        let contexts = list_contexts(&kubeconfig);
        assert_eq!(contexts[0].name, "in-cluster");
        assert_eq!(contexts[0].namespace.as_deref(), Some("kubri"));
        assert!(contexts[0].is_default);
        let cluster = kubeconfig.clusters[0].cluster.as_ref().unwrap();
        assert_eq!(cluster.server.as_deref(), Some("https://10.96.0.1:443"));
        assert!(
            cluster
                .certificate_authority
                .as_deref()
                .unwrap()
                .ends_with("ca.crt")
        );
        let user = kubeconfig.auth_infos[0].auth_info.as_ref().unwrap();
        assert!(user.token_file.as_deref().unwrap().ends_with("token"));

        // Written again on every start, as a new file.
        let path = dir.join("kubeconfig");
        write_new(&path, b"old").unwrap();
        write_new(&path, json.as_bytes()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), json);

        let ipv6 = in_cluster("fd00::1", "6443", &dir).unwrap();
        assert_eq!(ipv6["clusters"][0]["cluster"]["server"], "https://[fd00::1]:6443");
    }

    #[test]
    fn lists_contexts_sorted_with_default_marked() {
        let kubeconfig = Kubeconfig::from_yaml(KUBECONFIG).unwrap();
        let contexts = list_contexts(&kubeconfig);
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].name, "dev");
        assert!(!contexts[0].is_default);
        assert_eq!(contexts[0].namespace, None);
        assert_eq!(contexts[1].name, "prod");
        assert!(contexts[1].is_default);
        assert_eq!(contexts[1].cluster.as_deref(), Some("prod-cluster"));
        assert_eq!(contexts[1].namespace.as_deref(), Some("web"));
    }
}
