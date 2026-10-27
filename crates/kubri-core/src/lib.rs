//! Kubernetes access shared by the Kubri desktop app and the Kubri web server.
//!
//! [`Kubri`] owns the merged kubeconfig and the connection to the selected context.
//! Both frontends talk to the cluster only through it.

mod context;
mod error;
mod resource;
mod watch;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use kube::api::DynamicObject;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::discovery::{ApiResource, Discovery};
use kube::{Api, Client, Config};
use serde::Serialize;
use tokio::sync::RwLock;

pub use context::ContextInfo;
pub use error::{Error, Result};
pub use resource::{ResourceInfo, ResourceRef};
pub use watch::WatchEvent;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// The cluster Kubri is currently connected to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterInfo {
    pub context: String,
    pub server: String,
    pub version: String,
    /// The context's namespace, or `default`.
    pub default_namespace: String,
}

/// The context switcher's data: every context, and the one Kubri is connected to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextList {
    pub contexts: Vec<ContextInfo>,
    pub cluster: Option<ClusterInfo>,
}

struct Session {
    info: ClusterInfo,
    client: Client,
    resources: Vec<ResourceInfo>,
    api_resources: HashMap<ResourceRef, ApiResource>,
}

pub struct Kubri {
    kubeconfig: RwLock<Result<Kubeconfig, String>>,
    session: RwLock<Option<Arc<Session>>>,
}

impl Kubri {
    /// Loads the kubeconfig from `KUBECONFIG` (all listed files, merged) or `~/.kube/config`.
    /// A missing or broken kubeconfig is not fatal: it is reported by [`Kubri::contexts`].
    pub fn load() -> Self {
        Self::with_kubeconfig(Kubeconfig::read().map_err(|e| e.to_string()))
    }

    pub fn from_kubeconfig(kubeconfig: Kubeconfig) -> Self {
        Self::with_kubeconfig(Ok(kubeconfig))
    }

    fn with_kubeconfig(kubeconfig: Result<Kubeconfig, String>) -> Self {
        Self {
            kubeconfig: RwLock::new(kubeconfig),
            session: RwLock::new(None),
        }
    }

    /// Re-reads the kubeconfig from disk, keeping the current connection.
    pub async fn reload(&self) -> Result<ContextList> {
        *self.kubeconfig.write().await = Kubeconfig::read().map_err(|e| e.to_string());
        self.contexts().await
    }

    pub async fn contexts(&self) -> Result<ContextList> {
        let contexts = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => context::list_contexts(kubeconfig),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        Ok(ContextList {
            contexts,
            cluster: self.cluster().await,
        })
    }

    pub async fn cluster(&self) -> Option<ClusterInfo> {
        self.session.read().await.as_ref().map(|s| s.info.clone())
    }

    /// Connects to `context` and discovers the resource types it serves.
    /// The previous connection stays active if this fails.
    pub async fn switch_context(&self, context: &str) -> Result<ClusterInfo> {
        let kubeconfig = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => kubeconfig.clone(),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        let named = kubeconfig
            .contexts
            .iter()
            .find(|c| c.name == context)
            .ok_or_else(|| Error::UnknownContext(context.to_owned()))?;
        let namespace = named
            .context
            .as_ref()
            .and_then(|c| c.namespace.clone())
            .unwrap_or_else(|| "default".to_owned());

        let options = KubeConfigOptions {
            context: Some(context.to_owned()),
            ..Default::default()
        };
        let session = tokio::time::timeout(CONNECT_TIMEOUT, async {
            let config = Config::from_custom_kubeconfig(kubeconfig, &options).await?;
            let server = config.cluster_url.to_string();
            let client = Client::try_from(config)?;
            let version = client.apiserver_version().await?.git_version;
            let discovery = Discovery::new(client.clone()).run().await?;
            Ok::<_, Error>(Session::new(
                ClusterInfo {
                    context: context.to_owned(),
                    server,
                    version,
                    default_namespace: namespace,
                },
                client,
                &discovery,
            ))
        })
        .await
        .map_err(|_| Error::Timeout)??;

        let info = session.info.clone();
        *self.session.write().await = Some(Arc::new(session));
        tracing::info!(context, version = %info.version, "connected");
        Ok(info)
    }

    /// All resource types served by the connected cluster, sorted by kind.
    pub async fn resources(&self) -> Result<Vec<ResourceInfo>> {
        Ok(self.session().await?.resources.clone())
    }

    /// Streams the objects of one resource type. `namespace` is ignored for cluster-scoped
    /// resources, and `None` watches all namespaces.
    ///
    /// The stream is bound to the context that was active when it was created.
    pub async fn watch(
        &self,
        resource: &ResourceRef,
        namespace: Option<&str>,
    ) -> Result<impl Stream<Item = WatchEvent> + Send + 'static> {
        let session = self.session().await?;
        let api_resource = session
            .api_resources
            .get(resource)
            .ok_or_else(|| Error::UnknownResource(resource.to_string()))?;
        let namespaced = session
            .resources
            .iter()
            .any(|r| r.reference == *resource && r.namespaced);
        let api: Api<DynamicObject> = match namespace {
            Some(ns) if namespaced => Api::namespaced_with(session.client.clone(), ns, api_resource),
            _ => Api::all_with(session.client.clone(), api_resource),
        };
        Ok(watch::watch_objects(api))
    }

    async fn session(&self) -> Result<Arc<Session>> {
        self.session.read().await.clone().ok_or(Error::NotConnected)
    }
}

impl Session {
    fn new(info: ClusterInfo, client: Client, discovery: &Discovery) -> Self {
        let mut resources = Vec::new();
        let mut api_resources = HashMap::new();
        for group in discovery.groups() {
            for (api_resource, caps) in group.recommended_resources() {
                // Subresources such as pods/log are reached through their parent.
                if api_resource.plural.contains('/') {
                    continue;
                }
                let info = ResourceInfo::new(&api_resource, &caps);
                api_resources.insert(info.reference.clone(), api_resource);
                resources.push(info);
            }
        }
        resources.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then(a.reference.group.cmp(&b.reference.group))
        });
        Self {
            info,
            client,
            resources,
            api_resources,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_kubeconfig_is_reported_not_fatal() {
        let kubri = Kubri::with_kubeconfig(Err("not found".into()));
        assert!(matches!(kubri.contexts().await, Err(Error::NoKubeconfig(_))));
        assert!(matches!(kubri.resources().await, Err(Error::NotConnected)));
        assert!(kubri.cluster().await.is_none());
    }

    #[tokio::test]
    async fn switching_to_unknown_context_fails() {
        let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
        assert!(matches!(
            kubri.switch_context("nope").await,
            Err(Error::UnknownContext(name)) if name == "nope"
        ));
    }
}
