//! Kubernetes access shared by the Kubri desktop app and the Kubri web server.
//!
//! [`Kubri`] owns the merged kubeconfig and the connection to the selected context.
//! Both frontends talk to the cluster only through it.

mod context;
mod discovery;
mod error;
mod metrics;
mod objects;
mod resource;
mod table;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use k8s_openapi::api::core::v1::Event;
use kube::api::{DynamicObject, ListParams, Patch, PatchParams, PostParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;

pub use context::ContextInfo;
pub use error::{Error, Result};
pub use metrics::{MetricsKind, Usage, parse_quantity};
pub use objects::{DeleteOptions, EventInfo, ObjectRef, Propagation};
pub use resource::{ResourceInfo, ResourceRef};
pub use table::{Column, Row, TableEvent};

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
    by_ref: HashMap<ResourceRef, usize>,
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
            let resources = discovery::discover(&client).await?;
            Ok::<_, Error>(Session::new(
                ClusterInfo {
                    context: context.to_owned(),
                    server,
                    version,
                    default_namespace: namespace,
                },
                client,
                resources,
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

    /// Streams a resource list with the columns `kubectl get` shows. `namespace` is ignored
    /// for cluster-scoped resources, and `None` lists all namespaces.
    ///
    /// The stream is bound to the context that was active when it was created.
    pub async fn watch_table(
        &self,
        resource: &ResourceRef,
        namespace: Option<&str>,
    ) -> Result<impl Stream<Item = TableEvent> + Send + 'static> {
        let session = self.session().await?;
        let api = session.api(resource, namespace)?;
        Ok(table::watch_table(
            session.client.clone(),
            api.resource_url().to_owned(),
        ))
    }

    /// Fetches one object in full.
    pub async fn get(&self, obj: &ObjectRef) -> Result<Value> {
        let api = self
            .session()
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        Ok(serde_json::to_value(api.get(&obj.name).await?)?)
    }

    /// Replaces an object, as `kubectl edit` does. Fails with a conflict if it changed since
    /// `object` was read.
    pub async fn replace(&self, obj: &ObjectRef, object: Value) -> Result<Value> {
        let api = self
            .session()
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        let object: DynamicObject = serde_json::from_value(object)?;
        let saved = api.replace(&obj.name, &PostParams::default(), &object).await?;
        Ok(serde_json::to_value(saved)?)
    }

    /// Applies a JSON merge patch, for example to change labels or annotations.
    pub async fn patch(&self, obj: &ObjectRef, patch: Value) -> Result<Value> {
        let api = self
            .session()
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        let saved = api
            .patch(&obj.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(serde_json::to_value(saved)?)
    }

    pub async fn delete(&self, obj: &ObjectRef, options: &DeleteOptions) -> Result<()> {
        let api = self
            .session()
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        api.delete(&obj.name, &options.to_params()).await?;
        Ok(())
    }

    /// Sets the replica count through the `scale` subresource, so CRDs that support it work too.
    pub async fn scale(&self, obj: &ObjectRef, replicas: u32) -> Result<()> {
        let session = self.session().await?;
        let info = session.info_for(&obj.resource)?;
        if !info.subresources.iter().any(|s| s == "scale") {
            return Err(Error::Unsupported(format!("{} cannot be scaled", info.kind)));
        }
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let patch = serde_json::json!({ "spec": { "replicas": replicas } });
        api.patch_scale(&obj.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(())
    }

    /// Restarts the pods of a Deployment, StatefulSet or DaemonSet, like
    /// `kubectl rollout restart`.
    pub async fn restart(&self, obj: &ObjectRef) -> Result<()> {
        let session = self.session().await?;
        let info = session.info_for(&obj.resource)?;
        if !matches!(info.kind.as_str(), "Deployment" | "StatefulSet" | "DaemonSet") {
            return Err(Error::Unsupported(format!("{} cannot be restarted", info.kind)));
        }
        let now = k8s_openapi::jiff::Timestamp::now().to_string();
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        api.patch(
            &obj.name,
            &PatchParams::default(),
            &Patch::Merge(objects::restart_patch(&now)),
        )
        .await?;
        Ok(())
    }

    /// Events about the object with `uid`, newest first.
    pub async fn events(&self, namespace: Option<&str>, uid: &str) -> Result<Vec<EventInfo>> {
        let session = self.session().await?;
        let api: Api<Event> = match namespace {
            Some(ns) => Api::namespaced(session.client.clone(), ns),
            None => Api::all(session.client.clone()),
        };
        let params = ListParams::default().fields(&format!("involvedObject.uid={uid}"));
        let mut events: Vec<EventInfo> = api
            .list(&params)
            .await?
            .items
            .into_iter()
            .map(EventInfo::from_event)
            .collect();
        events.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        Ok(events)
    }

    /// Current CPU and memory usage from metrics-server. Fails with
    /// [`Error::UnknownResource`] when metrics-server is not installed.
    pub async fn metrics(&self, kind: MetricsKind, namespace: Option<&str>) -> Result<Vec<Usage>> {
        let session = self.session().await?;
        let plural = match kind {
            MetricsKind::Pods => "pods",
            MetricsKind::Nodes => "nodes",
        };
        let resource = session
            .resources
            .iter()
            .find(|r| r.reference.group == "metrics.k8s.io" && r.reference.plural == plural)
            .ok_or_else(|| Error::UnknownResource(format!("metrics.k8s.io/{plural}")))?;
        let api = session.api(&resource.reference, namespace)?;
        let request = kube::core::Request::new(api.resource_url())
            .list(&ListParams::default())
            .map_err(|e| Error::Unsupported(e.to_string()))?;
        let list: Value = session.client.request(request).await?;
        Ok(metrics::parse_usage(&list))
    }

    async fn session(&self) -> Result<Arc<Session>> {
        self.session.read().await.clone().ok_or(Error::NotConnected)
    }
}

impl Session {
    fn new(info: ClusterInfo, client: Client, resources: Vec<ResourceInfo>) -> Self {
        let by_ref = resources
            .iter()
            .enumerate()
            .map(|(i, r)| (r.reference.clone(), i))
            .collect();
        Self {
            info,
            client,
            resources,
            by_ref,
        }
    }

    fn info_for(&self, resource: &ResourceRef) -> Result<&ResourceInfo> {
        self.by_ref
            .get(resource)
            .map(|&i| &self.resources[i])
            .ok_or_else(|| Error::UnknownResource(resource.to_string()))
    }

    /// An API handle for `resource`. `namespace` is ignored for cluster-scoped resources.
    fn api(&self, resource: &ResourceRef, namespace: Option<&str>) -> Result<Api<DynamicObject>> {
        let info = self.info_for(resource)?;
        let api_resource = info.api_resource();
        Ok(match namespace {
            Some(ns) if info.namespaced => Api::namespaced_with(self.client.clone(), ns, &api_resource),
            _ => Api::all_with(self.client.clone(), &api_resource),
        })
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
