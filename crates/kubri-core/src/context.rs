use kube::config::Kubeconfig;
use serde::Serialize;

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
