//! The connection to one context, and what Kubri learned about its cluster.

use std::collections::HashMap;
use std::sync::Arc;

use k8s_openapi::api::core::v1::{Pod, Service};
use kube::api::{DynamicObject, ListParams};
use kube::{Api, Client};
use serde_json::Value;

use crate::{
    AccessCheck, ClusterInfo, Error, IntegrationSettings, Integrations, MetricsKind, ObjectRef, ResourceInfo,
    ResourceRef, Result, Usage, access, grafana, integrations, metrics, portforward, selector, workload,
};

pub(crate) struct Session {
    pub(crate) info: ClusterInfo,
    pub(crate) client: Client,
    /// The same connection without retries, for the service proxy: its 503 means a tool
    /// has no ready pod, which retrying for half a minute does not change.
    pub(crate) proxy_client: Client,
    pub(crate) resources: Vec<ResourceInfo>,
    pub(crate) by_ref: HashMap<ResourceRef, usize>,
    pub(crate) access: access::AccessCache,
    /// OpenAPI schemas by kind, fetched once per connection.
    pub(crate) schemas: std::sync::Mutex<HashMap<ResourceRef, Arc<Option<Value>>>>,
    /// The tools found in the cluster, once looked for.
    pub(crate) integrations: tokio::sync::Mutex<Option<(IntegrationSettings, Integrations)>>,
    /// Grafana's dashboards, when they were read and why Grafana's own list was not.
    pub(crate) grafana: tokio::sync::Mutex<Option<grafana::Cached>>,
    /// The Loki data source a Grafana has, once looked up with a token.
    pub(crate) grafana_logs: tokio::sync::Mutex<Option<(integrations::Endpoint, String, Option<String>)>>,
}

impl Session {
    /// Fails with [`Error::Forbidden`] when RBAC denies `check`. Where the API server cannot
    /// say, the action goes ahead and the API server decides.
    pub(crate) async fn ensure_allowed(&self, check: AccessCheck) -> Result<()> {
        match self.access_of(&check).await {
            Some(false) => Err(Error::Forbidden(check.describe())),
            _ => Ok(()),
        }
    }

    /// Whether RBAC allows `check`; `None` where the API server cannot say.
    pub(crate) async fn access_of(&self, check: &AccessCheck) -> Option<bool> {
        access::check_all(&self.client, &self.access, std::slice::from_ref(check))
            .await
            .into_iter()
            .next()
            .flatten()
    }

    /// Checks `verb` on the object `obj`, or on its `subresource`.
    pub(crate) async fn ensure_may(&self, verb: &str, obj: &ObjectRef, subresource: &str) -> Result<()> {
        let check = AccessCheck::new(verb, &obj.resource, obj.namespace.as_deref())
            .on_subresource(subresource)
            .named(&obj.name);
        self.ensure_allowed(check).await
    }

    pub(crate) fn new(info: ClusterInfo, client: Client, resources: Vec<ResourceInfo>) -> Self {
        let by_ref = resources
            .iter()
            .enumerate()
            .map(|(i, r)| (r.reference.clone(), i))
            .collect();
        Self {
            info,
            proxy_client: client.clone(),
            client,
            resources,
            by_ref,
            access: Default::default(),
            schemas: Default::default(),
            integrations: Default::default(),
            grafana: Default::default(),
            grafana_logs: Default::default(),
        }
    }

    /// The time ages and "recent" count to: now, or when a snapshot was taken.
    pub(crate) fn now(&self) -> k8s_openapi::jiff::Timestamp {
        self.info
            .snapshot
            .as_ref()
            .and_then(|s| k8s_openapi::jiff::Timestamp::from_millisecond(s.taken).ok())
            .unwrap_or_else(k8s_openapi::jiff::Timestamp::now)
    }

    pub(crate) fn info_for(&self, resource: &ResourceRef) -> Result<&ResourceInfo> {
        self.by_ref
            .get(resource)
            .map(|&i| &self.resources[i])
            .ok_or_else(|| Error::UnknownResource(resource.to_string()))
    }

    /// Whether `resource` is the built-in core type `kind`, such as `Pod`.
    pub(crate) fn is_core(&self, resource: &ResourceRef, kind: &str) -> Result<bool> {
        Ok(resource.group.is_empty() && self.info_for(resource)?.kind == kind)
    }

    pub(crate) async fn metrics(&self, kind: MetricsKind, namespace: Option<&str>) -> Result<Vec<Usage>> {
        let plural = match kind {
            MetricsKind::Pods | MetricsKind::Namespaces => "pods",
            MetricsKind::Nodes => "nodes",
        };
        let resource = self
            .resources
            .iter()
            .find(|r| r.reference.group == "metrics.k8s.io" && r.reference.plural == plural)
            .ok_or_else(|| Error::UnknownResource(format!("metrics.k8s.io/{plural}")))?;
        let api = self.api(&resource.reference, namespace)?;
        let request = kube::core::Request::new(api.resource_url())
            .list(&ListParams::default())
            .map_err(|e| Error::Unsupported(e.to_string()))?;
        let list: Value = self.client.request(request).await?;
        Ok(metrics::parse_usage(&list))
    }

    /// The resource type of an object's `apiVersion` and kind, preferring the same version.
    pub(crate) fn resolve(&self, api_version: &str, kind: &str) -> Option<ResourceRef> {
        let (group, version) = api_version.rsplit_once('/').unwrap_or(("", api_version));
        let candidates = self
            .resources
            .iter()
            .filter(|r| r.reference.group == group && r.kind == kind && !r.reference.plural.contains('/'));
        let mut best = None;
        for r in candidates {
            if r.reference.version == version {
                return Some(r.reference.clone());
            }
            best.get_or_insert(r.reference.clone());
        }
        best
    }

    /// The served resource of `group/version/resource`, `version/resource` or a resource name,
    /// as `jumps.yaml` names it; another version of the same resource if need be.
    pub(crate) fn find_path(&self, path: &str) -> Option<&ResourceInfo> {
        let (group, version, name) = workload::split_gvr(path);
        let name = name.to_ascii_lowercase();
        let matches = |r: &&ResourceInfo| {
            let plural = r.reference.plural.as_str();
            if version.is_empty() {
                // A bare name: the plural, singular or a short name, core types first.
                plural == name || r.singular == name || r.short_names.contains(&name)
            } else {
                r.reference.group == group && plural == name
            }
        };
        let mut candidates: Vec<&ResourceInfo> = self.resources.iter().filter(matches).collect();
        candidates.sort_by_key(|r| (!r.reference.group.is_empty(), r.reference.version != version));
        candidates.into_iter().next()
    }

    pub(crate) fn pod_api(&self, namespace: Option<&str>) -> Result<Api<Pod>> {
        let namespace = namespace.ok_or_else(|| Error::Unsupported("a namespace is required".into()))?;
        Ok(Api::namespaced(self.client.clone(), namespace))
    }

    /// The pods behind `target`: the pod itself, or the pods a workload or Service selects.
    /// The pod a forward to `target` reaches, and the port on it: the pod itself, or for a
    /// Service or workload a ready pod, with a Service port mapped to the pod port it targets.
    pub(crate) async fn forward_destination(
        &self,
        target: &ObjectRef,
        remote_port: u16,
    ) -> Result<(Pod, u16)> {
        let api = self.pod_api(target.namespace.as_deref())?;
        if self.is_core(&target.resource, "Pod")? {
            return Ok((api.get(&target.name).await?, remote_port));
        }
        let pods = self.pods_of(target).await?;
        let pod = portforward::pick_pod(pods)
            .ok_or_else(|| Error::Unsupported(format!("{} has no ready pod", target.name)))?;
        let port = if self.is_core(&target.resource, "Service")? {
            let services: Api<Service> =
                Api::namespaced(self.client.clone(), api.namespace().unwrap_or_default());
            let service = services.get(&target.name).await?;
            portforward::service_target_port(&service, remote_port, &pod)?
        } else {
            remote_port
        };
        Ok((pod, port))
    }

    pub(crate) async fn pods_of(&self, target: &ObjectRef) -> Result<Vec<Pod>> {
        Ok(self.pods_and_selector(target).await?.0)
    }

    /// The pods behind `target`, and for a workload or Service the label selector that picks
    /// them.
    pub(crate) async fn pods_and_selector(&self, target: &ObjectRef) -> Result<(Vec<Pod>, Option<String>)> {
        let info = self.info_for(&target.resource)?;
        if self.is_core(&target.resource, "Pod")? {
            let pod = self
                .pod_api(target.namespace.as_deref())?
                .get(&target.name)
                .await?;
            return Ok((vec![pod], None));
        }
        if !info.namespaced {
            return Err(Error::Unsupported(format!(
                "a {} has no pods of its own",
                info.kind
            )));
        }
        let object = self
            .api(&target.resource, target.namespace.as_deref())?
            .get(&target.name)
            .await?;
        let selector = selector::pod_selector(&serde_json::to_value(object)?).ok_or_else(|| {
            Error::Unsupported(format!("{} {} does not select pods", info.kind, target.name))
        })?;
        let pods = self.pod_api(target.namespace.as_deref())?;
        let mut pods = pods.list(&ListParams::default().labels(&selector)).await?.items;
        pods.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
        Ok((pods, Some(selector)))
    }

    /// An API handle for `resource`. `namespace` is ignored for cluster-scoped resources.
    pub(crate) fn api(&self, resource: &ResourceRef, namespace: Option<&str>) -> Result<Api<DynamicObject>> {
        let info = self.info_for(resource)?;
        let api_resource = info.api_resource();
        Ok(match namespace {
            Some(ns) if info.namespaced => Api::namespaced_with(self.client.clone(), ns, &api_resource),
            _ => Api::all_with(self.client.clone(), &api_resource),
        })
    }
}
