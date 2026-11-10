use k8s_openapi::api::core::v1::Event;
use kube::Api;
use kube::api::{DynamicObject, ListParams, Patch, PatchParams, PostParams};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session::Session;
use crate::{
    AccessCheck, Change, ContainerImage, Drill, Error, Kubri, Preview, Reference, ResourceRef, Result, fixes,
    preview, references, rightsizing, workload,
};

/// Addresses one object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ObjectRef {
    /// When provided, refuse to resolve this target against a different context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub resource: ResourceRef,
    /// `None` for cluster-scoped objects.
    #[serde(default)]
    pub namespace: Option<String>,
    pub name: String,
}

impl ObjectRef {
    /// The namespace a change to this object happens in: its own, or for a Namespace the one it
    /// is, so guard rails for a namespace also keep it from being changed or deleted.
    pub fn scope(&self) -> Option<&str> {
        match &self.namespace {
            Some(ns) => Some(ns),
            None if self.resource.group.is_empty() && self.resource.plural == "namespaces" => {
                Some(&self.name)
            }
            None => None,
        }
    }
}

impl std::fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{} {}/{}", self.resource, ns, self.name),
            None => write!(f, "{} {}", self.resource, self.name),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub enum Propagation {
    #[default]
    Background,
    Foreground,
    Orphan,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, optional_fields = nullable)]
pub struct DeleteOptions {
    /// Seconds to wait before killing; `Some(0)` with `force` deletes immediately.
    pub grace_period_seconds: Option<u32>,
    /// Delete immediately without waiting for confirmation from the kubelet.
    #[ts(as = "Option<bool>", optional)]
    pub force: bool,
    #[ts(as = "Option<Propagation>", optional)]
    pub propagation: Propagation,
}

impl DeleteOptions {
    pub(crate) fn to_params(&self) -> kube::api::DeleteParams {
        let grace = if self.force {
            Some(0)
        } else {
            self.grace_period_seconds
        };
        kube::api::DeleteParams {
            grace_period_seconds: grace,
            propagation_policy: Some(match self.propagation {
                Propagation::Background => kube::api::PropagationPolicy::Background,
                Propagation::Foreground => kube::api::PropagationPolicy::Foreground,
                Propagation::Orphan => kube::api::PropagationPolicy::Orphan,
            }),
            ..Default::default()
        }
    }
}

/// The merge patch `kubectl rollout restart` sends.
pub(crate) fn restart_patch(now: &str) -> serde_json::Value {
    serde_json::json!({
        "spec": { "template": { "metadata": { "annotations": {
            "kubectl.kubernetes.io/restartedAt": now
        } } } }
    })
}

/// An event about an object, as shown on its details page.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EventInfo {
    /// `Normal` or `Warning`.
    pub kind: String,
    pub reason: String,
    pub message: String,
    pub count: i32,
    pub source: String,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
}

impl EventInfo {
    pub(crate) fn from_event(event: k8s_openapi::api::core::v1::Event) -> Self {
        let time =
            |t: Option<k8s_openapi::apimachinery::pkg::apis::meta::v1::Time>| t.map(|t| t.0.to_string());
        let event_time = event.event_time.map(|t| t.0.to_string());
        let source = event
            .reporting_component
            .filter(|c| !c.is_empty())
            .or_else(|| event.source.and_then(|s| s.component))
            .unwrap_or_default();
        let first_seen = time(event.first_timestamp).or_else(|| event_time.clone());
        let last_seen = event
            .series
            .and_then(|s| s.last_observed_time.map(|t| t.0.to_string()))
            .or_else(|| time(event.last_timestamp))
            .or(event_time)
            .or_else(|| first_seen.clone());
        Self {
            kind: event.type_.unwrap_or_else(|| "Normal".to_owned()),
            reason: event.reason.unwrap_or_default(),
            message: event.message.unwrap_or_default().trim().to_owned(),
            count: event.count.unwrap_or(1),
            source,
            first_seen,
            last_seen,
        }
    }
}

impl Kubri {
    /// Fetches one object in full.
    pub async fn get(&self, obj: &ObjectRef) -> Result<Value> {
        let api = self
            .object_session(obj)
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        Ok(serde_json::to_value(api.get(&obj.name).await?)?)
    }

    /// Fetches one object from its own context, also one Kubri is not connected to, as Compare
    /// does. [`Kubri::get`] refuses those, so a view left from the last context cannot show
    /// another cluster's object as this one's.
    pub async fn get_elsewhere(&self, obj: &ObjectRef) -> Result<Value> {
        let api = self
            .read_session(obj.context.as_deref())
            .await?
            .api(&obj.resource, obj.namespace.as_deref())?;
        Ok(serde_json::to_value(api.get(&obj.name).await?)?)
    }

    /// Replaces an object, as `kubectl edit` does. Fails with a conflict if it changed since
    /// `object` was read.
    pub async fn replace(&self, obj: &ObjectRef, object: Value) -> Result<Value> {
        self.ensure_writable_in(obj.scope()).await?;
        let session = self.object_session(obj).await?;
        session.ensure_may("update", obj, "").await?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let object: DynamicObject = serde_json::from_value(object)?;
        let saved = api.replace(&obj.name, &PostParams::default(), &object).await?;
        Ok(serde_json::to_value(saved)?)
    }

    /// Runs `change` as a server-side dry run (`dryRun=All`) and returns the object as it is
    /// and as it would be, for a diff. Nothing changes in the cluster.
    pub async fn preview(&self, obj: &ObjectRef, change: Change) -> Result<Preview> {
        let session = self.object_session(obj).await?;
        let info = session.info_for(&obj.resource)?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let dry = PatchParams {
            dry_run: true,
            ..Default::default()
        };
        let (before, after) = match change {
            Change::Replace { value } => {
                let before = api.get(&obj.name).await?;
                let object: DynamicObject = serde_json::from_value(value)?;
                let post = PostParams {
                    dry_run: true,
                    ..Default::default()
                };
                (before, api.replace(&obj.name, &post, &object).await?)
            }
            Change::Patch { patch } => (
                api.get(&obj.name).await?,
                api.patch(&obj.name, &dry, &Patch::Merge(patch)).await?,
            ),
            Change::Restart => (
                api.get(&obj.name).await?,
                api.patch(
                    &obj.name,
                    &dry,
                    &Patch::Merge(restart_patch(&k8s_openapi::jiff::Timestamp::now().to_string())),
                )
                .await?,
            ),
            Change::Images { images } => {
                let patch = workload::image_patch(info.kind == "Pod", &images)?;
                (
                    api.get(&obj.name).await?,
                    api.patch(&obj.name, &dry, &Patch::Strategic(patch)).await?,
                )
            }
            Change::Resources { containers } => {
                let patch = rightsizing::resources_patch(&containers)?;
                (
                    api.get(&obj.name).await?,
                    api.patch(&obj.name, &dry, &Patch::Strategic(patch)).await?,
                )
            }
            Change::Memory { container, memory } => {
                let patch = fixes::memory_patch(&container, &memory)?;
                (
                    api.get(&obj.name).await?,
                    api.patch(&obj.name, &dry, &Patch::Strategic(patch)).await?,
                )
            }
            Change::Scale { replicas } => {
                if !info.subresources.iter().any(|s| s == "scale") {
                    return Err(Error::Unsupported(format!("{} cannot be scaled", info.kind)));
                }
                let patch = serde_json::json!({ "spec": { "replicas": replicas } });
                let before = serde_json::to_value(api.get_scale(&obj.name).await?)?;
                let after =
                    serde_json::to_value(api.patch_scale(&obj.name, &dry, &Patch::Merge(patch)).await?)?;
                return Ok(Preview {
                    before: preview::comparable(before),
                    after: preview::comparable(after),
                });
            }
        };
        Ok(Preview {
            before: preview::comparable(serde_json::to_value(before)?),
            after: preview::comparable(serde_json::to_value(after)?),
        })
    }

    /// Applies a JSON merge patch, for example to change labels or annotations.
    pub async fn patch(&self, obj: &ObjectRef, patch: Value) -> Result<Value> {
        self.ensure_writable_in(obj.scope()).await?;
        let session = self.object_session(obj).await?;
        session.ensure_may("patch", obj, "").await?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let saved = api
            .patch(&obj.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(serde_json::to_value(saved)?)
    }

    pub async fn delete(&self, obj: &ObjectRef, options: &DeleteOptions) -> Result<()> {
        self.ensure_writable_in(obj.scope()).await?;
        let session = self.object_session(obj).await?;
        session.ensure_may("delete", obj, "").await?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        api.delete(&obj.name, &options.to_params()).await?;
        Ok(())
    }

    /// Sets the replica count through the `scale` subresource, so CRDs that support it work too.
    pub async fn scale(&self, obj: &ObjectRef, replicas: u32) -> Result<()> {
        self.ensure_writable_in(obj.scope()).await?;
        let session = self.object_session(obj).await?;
        let info = session.info_for(&obj.resource)?;
        if !info.subresources.iter().any(|s| s == "scale") {
            return Err(Error::Unsupported(format!("{} cannot be scaled", info.kind)));
        }
        session.ensure_may("patch", obj, "scale").await?;
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        let patch = serde_json::json!({ "spec": { "replicas": replicas } });
        api.patch_scale(&obj.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(())
    }

    /// Restarts the pods of a Deployment, StatefulSet or DaemonSet, like
    /// `kubectl rollout restart`.
    pub async fn restart(&self, obj: &ObjectRef) -> Result<()> {
        self.ensure_writable_in(obj.scope()).await?;
        let session = self.object_session(obj).await?;
        let info = session.info_for(&obj.resource)?;
        if !matches!(info.kind.as_str(), "Deployment" | "StatefulSet" | "DaemonSet") {
            return Err(Error::Unsupported(format!("{} cannot be restarted", info.kind)));
        }
        session.ensure_may("patch", obj, "").await?;
        let now = k8s_openapi::jiff::Timestamp::now().to_string();
        let api = session.api(&obj.resource, obj.namespace.as_deref())?;
        api.patch(
            &obj.name,
            &PatchParams::default(),
            &Patch::Merge(restart_patch(&now)),
        )
        .await?;
        Ok(())
    }

    /// Where Enter leads from an object: the rule for its resource in `jumps.yaml`, or the
    /// pods of a workload, Service or node, or the Jobs of a CronJob. `None` for the rest,
    /// which Enter describes.
    pub async fn drill(&self, target: &ObjectRef) -> Result<Option<Drill>> {
        let session = self.object_session(target).await?;
        let info = session.info_for(&target.resource)?;
        let config = self.k9s_config().await;
        let rule = workload::find_jump(&config.jumps, &info.reference);
        let builtin = workload::Builtin::of(info);
        if rule.is_none() && builtin.is_none() {
            return Ok(None);
        }
        let object = serde_json::to_value(
            session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?,
        )?;
        if let Some(rule) = rule {
            let resolve = |path: &str| session.find_path(path).cloned();
            return workload::jump(rule, info, target, &object, resolve).map(Some);
        }
        let pods = session.resolve("v1", "Pod");
        let jobs = session.resolve("batch/v1", "Job");
        builtin
            .map(|b| b.drill(info, target, &object, pods, jobs))
            .transpose()
    }

    /// Runs a CronJob now: creates a Job from its template, like `kubectl create job
    /// --from=cronjob/…`. Returns the Job's name.
    pub async fn trigger_cronjob(&self, target: &ObjectRef) -> Result<String> {
        self.ensure_writable_in(target.scope()).await?;
        let session = self.object_session(target).await?;
        if session.info_for(&target.resource)?.kind != "CronJob" {
            return Err(Error::Unsupported("only CronJobs can be triggered".into()));
        }
        if let Some(jobs) = session.resolve("batch/v1", "Job") {
            session
                .ensure_allowed(AccessCheck::new("create", &jobs, target.namespace.as_deref()))
                .await?;
        }
        let cronjob = serde_json::to_value(
            session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?,
        )?;
        let job: DynamicObject = serde_json::from_value(workload::job_from_cronjob(&cronjob)?)?;
        let jobs = session
            .resolve("batch/v1", "Job")
            .ok_or_else(|| Error::UnknownResource("batch/v1/jobs".into()))?;
        let created = session
            .api(&jobs, target.namespace.as_deref())?
            .create(&PostParams::default(), &job)
            .await?;
        Ok(created.metadata.name.unwrap_or_default())
    }

    /// Suspends a CronJob, so it starts no more Jobs, or resumes it.
    pub async fn suspend_cronjob(&self, target: &ObjectRef, suspend: bool) -> Result<()> {
        self.ensure_writable_in(target.scope()).await?;
        let session = self.object_session(target).await?;
        if session.info_for(&target.resource)?.kind != "CronJob" {
            return Err(Error::Unsupported("only CronJobs can be suspended".into()));
        }
        session.ensure_may("patch", target, "").await?;
        let patch = serde_json::json!({ "spec": { "suspend": suspend } });
        session
            .api(&target.resource, target.namespace.as_deref())?
            .patch(&target.name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;
        Ok(())
    }

    /// What uses a ConfigMap, Secret, PersistentVolumeClaim, ServiceAccount or
    /// PriorityClass, like k9s `u`: the workloads, CronJobs and bare pods whose pod spec names
    /// it, in its namespace (in all of them for a PriorityClass).
    pub async fn used_by(&self, target: &ObjectRef) -> Result<Vec<Reference>> {
        let session = self.object_session(target).await?;
        let info = session.info_for(&target.resource)?;
        let used = references::Used::of(info)
            .ok_or_else(|| Error::Unsupported(format!("Kubri does not look up what uses a {}", info.kind)))?;
        let users = used.users();
        let lists = users.iter().filter_map(|&(version, kind)| {
            let resource = session.resolve(version, kind)?;
            let api = session.api(&resource, target.namespace.as_deref()).ok()?;
            Some(async move { (kind, resource, api.list(&ListParams::default()).await) })
        });
        let mut found = Vec::new();
        for (kind, resource, list) in futures::future::join_all(lists).await {
            let list = match list {
                Ok(list) => list,
                // Extras: a user who may not list them still sees the rest.
                Err(_) if kind == "HTTPRoute" || (kind == "Ingress" && used == references::Used::Secret) => {
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            for object in list.items {
                let value = serde_json::to_value(&object)?;
                if used.by(kind, &value, &target.name) {
                    found.push(Reference {
                        resource: resource.clone(),
                        kind: kind.to_owned(),
                        namespace: object.metadata.namespace.clone(),
                        name: object.metadata.name.clone().unwrap_or_default(),
                        how: used.how(kind).map(str::to_owned),
                    });
                }
            }
        }
        Ok(found)
    }

    /// Sets container images of a pod, Deployment, StatefulSet or DaemonSet, like
    /// `kubectl set image`.
    pub async fn set_images(&self, target: &ObjectRef, images: &[ContainerImage]) -> Result<()> {
        self.ensure_writable_in(target.scope()).await?;
        let session = self.object_session(target).await?;
        let info = session.info_for(&target.resource)?;
        if !workload::has_images(info) {
            return Err(Error::Unsupported(format!("{} has no images to set", info.kind)));
        }
        session.ensure_may("patch", target, "").await?;
        let patch = workload::image_patch(info.kind == "Pod", images)?;
        session
            .api(&target.resource, target.namespace.as_deref())?
            .patch(&target.name, &PatchParams::default(), &Patch::Strategic(patch))
            .await?;
        Ok(())
    }

    /// Rolls the Deployment of a ReplicaSet back to the ReplicaSet's pod template, or the
    /// StatefulSet or DaemonSet of a ControllerRevision back to the revision's, like
    /// `kubectl rollout undo --to-revision`. Returns the kind and name of what was rolled back,
    /// like `Deployment web`.
    pub async fn rollback(&self, target: &ObjectRef) -> Result<String> {
        self.ensure_writable_in(target.scope()).await?;
        let session = self.object_session(target).await?;
        let info = session.info_for(&target.resource)?;
        match (info.reference.group.as_str(), info.kind.as_str()) {
            ("apps", "ReplicaSet") => {}
            ("apps", "ControllerRevision") => return self.rollback_to_revision(&session, target).await,
            _ => {
                return Err(Error::Unsupported(
                    "rollbacks start from a ReplicaSet or a ControllerRevision".into(),
                ));
            }
        }
        let replicaset = serde_json::to_value(
            session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?,
        )?;
        let name = workload::deployment_of(&replicaset)?;
        let deployments = session
            .resolve("apps/v1", "Deployment")
            .ok_or_else(|| Error::UnknownResource("apps/v1/deployments".into()))?;
        let deployment_ref = ObjectRef {
            context: None,
            resource: deployments.clone(),
            namespace: target.namespace.clone(),
            name: name.clone(),
        };
        session.ensure_may("update", &deployment_ref, "").await?;
        let api = session.api(&deployments, target.namespace.as_deref())?;
        let deployment = serde_json::to_value(api.get(&name).await?)?;
        let updated: DynamicObject = serde_json::from_value(workload::rolled_back(deployment, &replicaset)?)?;
        // Fails with a conflict if the Deployment changed since it was read.
        api.replace(&name, &PostParams::default(), &updated).await?;
        Ok(format!("Deployment {name}"))
    }

    /// Patches the pod template saved in a ControllerRevision into its StatefulSet or DaemonSet.
    pub(crate) async fn rollback_to_revision(&self, session: &Session, target: &ObjectRef) -> Result<String> {
        let revision = serde_json::to_value(
            session
                .api(&target.resource, target.namespace.as_deref())?
                .get(&target.name)
                .await?,
        )?;
        let (kind, name) = workload::revision_owner(&revision)?;
        let owners = session
            .resolve("apps/v1", &kind)
            .ok_or_else(|| Error::UnknownResource(format!("apps/v1/{kind}")))?;
        let owner_ref = ObjectRef {
            context: None,
            resource: owners.clone(),
            namespace: target.namespace.clone(),
            name: name.clone(),
        };
        session.ensure_may("patch", &owner_ref, "").await?;
        let api = session.api(&owners, target.namespace.as_deref())?;
        let mut owner = serde_json::to_value(api.get(&name).await?)?;
        owner["kind"] = kind.clone().into();
        let patch = workload::revision_patch(&owner, &revision)?;
        api.patch(&name, &PatchParams::default(), &Patch::Strategic(patch))
            .await?;
        Ok(format!("{kind} {name}"))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_delete_uses_zero_grace_period() {
        let params = DeleteOptions {
            grace_period_seconds: Some(30),
            force: true,
            ..Default::default()
        }
        .to_params();
        assert_eq!(params.grace_period_seconds, Some(0));
    }

    #[test]
    fn restart_patch_sets_restarted_at_annotation() {
        let patch = restart_patch("2026-10-27T22:00:00Z");
        assert_eq!(
            patch.pointer("/spec/template/metadata/annotations/kubectl.kubernetes.io~1restartedAt"),
            Some(&serde_json::json!("2026-10-27T22:00:00Z"))
        );
    }

    #[test]
    fn object_ref_deserializes_without_namespace() {
        let obj: ObjectRef = serde_json::from_value(serde_json::json!({
            "resource": { "group": "", "version": "v1", "plural": "nodes" },
            "name": "worker-1"
        }))
        .unwrap();
        assert_eq!(obj.namespace, None);
        assert_eq!(obj.to_string(), "v1/nodes worker-1");
    }
}
