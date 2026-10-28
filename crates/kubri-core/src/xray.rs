//! XRay: a tree of how objects depend on each other, from a workload down to its pods,
//! containers and the ConfigMaps, Secrets and volumes they use.

use std::collections::{BTreeMap, HashMap, HashSet};

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, PersistentVolumeClaim, Pod, PodSpec, Secret, Service, ServiceAccount,
};
use k8s_openapi::api::networking::v1::Ingress;
use kube::Client;
use serde::Serialize;

use crate::health::{self, Health, Status};
use crate::typed::{list, names, resource_ref, scoped};
use crate::{ResourceRef, Result};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XrayNode {
    /// Unique in the tree: the path from the root.
    pub id: String,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    /// Set for objects that can be opened; not for containers or missing objects.
    pub resource: Option<ResourceRef>,
    pub health: Health,
    pub detail: String,
    pub children: Vec<XrayNode>,
}

/// The kinds a tree can start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Root {
    Deployment,
    StatefulSet,
    DaemonSet,
    ReplicaSet,
    Job,
    CronJob,
    Pod,
    Service,
    Ingress,
}

impl Root {
    pub(crate) const SUPPORTED: &str =
        "Deployments, StatefulSets, DaemonSets, ReplicaSets, Jobs, CronJobs, Pods, Services and Ingresses";

    pub(crate) fn of(group: &str, kind: &str) -> Option<Self> {
        Some(match (group, kind) {
            ("apps", "Deployment") => Root::Deployment,
            ("apps", "StatefulSet") => Root::StatefulSet,
            ("apps", "DaemonSet") => Root::DaemonSet,
            ("apps", "ReplicaSet") => Root::ReplicaSet,
            ("batch", "Job") => Root::Job,
            ("batch", "CronJob") => Root::CronJob,
            ("", "Pod") => Root::Pod,
            ("", "Service") => Root::Service,
            ("networking.k8s.io", "Ingress") => Root::Ingress,
            _ => return None,
        })
    }
}

/// Everything the tree is built from, from one namespace or all of them.
#[derive(Default)]
pub(crate) struct Inventory {
    pub pods: Vec<Pod>,
    pub replica_sets: Vec<ReplicaSet>,
    pub jobs: Vec<Job>,
    pub services: Vec<Service>,
    /// `None` when they could not be listed, so references cannot be checked.
    pub claims: Option<Vec<PersistentVolumeClaim>>,
    pub config_maps: Option<HashSet<(String, String)>>,
    pub secrets: Option<HashSet<(String, String)>>,
    pub service_accounts: Option<HashSet<(String, String)>>,
}

/// Lists what the tree needs and builds one tree per object of the root kind.
pub(crate) async fn gather(client: Client, root: Root, namespace: Option<&str>) -> Result<Vec<XrayNode>> {
    let c = &client;
    let set = |names: Result<Vec<(String, String)>>| names.ok().map(HashSet::from_iter);
    let (pods, claims, config_maps, secrets, service_accounts) = futures::join!(
        list(scoped::<Pod>(c, namespace)),
        list(scoped::<PersistentVolumeClaim>(c, namespace)),
        names(scoped::<ConfigMap>(c, namespace)),
        names(scoped::<Secret>(c, namespace)),
        names(scoped::<ServiceAccount>(c, namespace)),
    );
    let mut inventory = Inventory {
        pods: pods?,
        claims: claims.ok(),
        config_maps: set(config_maps),
        secrets: set(secrets),
        service_accounts: set(service_accounts),
        ..Default::default()
    };

    macro_rules! roots {
        ($kind:ty) => {
            list(scoped::<$kind>(c, namespace)).await?
        };
    }
    let mut nodes: Vec<XrayNode> = match root {
        Root::Deployment => {
            let deployments = roots!(Deployment);
            inventory.replica_sets = roots!(ReplicaSet);
            let tree = Tree::new(&inventory);
            deployments.iter().map(|d| tree.deployment(d)).collect()
        }
        Root::CronJob => {
            let crons = roots!(CronJob);
            inventory.jobs = roots!(Job);
            let tree = Tree::new(&inventory);
            crons.iter().map(|c| tree.cron_job(c)).collect()
        }
        Root::Ingress => {
            let ingresses = roots!(Ingress);
            inventory.services = roots!(Service);
            let tree = Tree::new(&inventory);
            ingresses.iter().map(|i| tree.ingress(i)).collect()
        }
        Root::StatefulSet => {
            let sets = roots!(StatefulSet);
            let tree = Tree::new(&inventory);
            sets.iter()
                .map(|s| tree.owner(s, health::stateful_set(s), None))
                .collect()
        }
        Root::DaemonSet => {
            let sets = roots!(DaemonSet);
            let tree = Tree::new(&inventory);
            sets.iter()
                .map(|s| tree.owner(s, health::daemon_set(s), None))
                .collect()
        }
        Root::ReplicaSet => {
            let sets = roots!(ReplicaSet);
            let tree = Tree::new(&inventory);
            sets.iter()
                .map(|s| tree.owner(s, health::replica_set(s), None))
                .collect()
        }
        Root::Job => {
            let jobs = roots!(Job);
            let tree = Tree::new(&inventory);
            jobs.iter().map(|j| tree.owner(j, health::job(j), None)).collect()
        }
        Root::Service => {
            let services = roots!(Service);
            let tree = Tree::new(&inventory);
            services.iter().map(|s| tree.service(s, None)).collect()
        }
        Root::Pod => {
            let tree = Tree::new(&inventory);
            inventory.pods.iter().map(|p| tree.pod(p, None)).collect()
        }
    };
    sort(&mut nodes);
    Ok(nodes)
}

fn sort(nodes: &mut [XrayNode]) {
    nodes.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
}

fn id(parent: Option<&str>, kind: &str, namespace: Option<&str>, name: &str) -> String {
    match parent {
        Some(parent) => format!("{parent}/{kind}:{name}"),
        None => format!("{kind}:{}/{name}", namespace.unwrap_or_default()),
    }
}

/// Builds trees from an [`Inventory`], with its objects indexed by owner.
struct Tree<'a> {
    inventory: &'a Inventory,
    owned: HashMap<&'a str, Vec<Owned<'a>>>,
}

enum Owned<'a> {
    Pod(&'a Pod),
    ReplicaSet(&'a ReplicaSet),
    Job(&'a Job),
}

impl<'a> Tree<'a> {
    fn new(inventory: &'a Inventory) -> Self {
        let mut owned: HashMap<&str, Vec<Owned>> = HashMap::new();
        let mut add = |meta: &'a kube::api::ObjectMeta, item: Owned<'a>| {
            if let Some(owner) = meta
                .owner_references
                .iter()
                .flatten()
                .find(|o| o.controller == Some(true))
            {
                owned.entry(owner.uid.as_str()).or_default().push(item);
            }
        };
        for pod in &inventory.pods {
            add(&pod.metadata, Owned::Pod(pod));
        }
        for set in &inventory.replica_sets {
            add(&set.metadata, Owned::ReplicaSet(set));
        }
        for job in &inventory.jobs {
            add(&job.metadata, Owned::Job(job));
        }
        Self { inventory, owned }
    }

    fn owned_by(&self, meta: &kube::api::ObjectMeta) -> &[Owned<'a>] {
        meta.uid
            .as_deref()
            .and_then(|uid| self.owned.get(uid))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// A node for `object`, with `children` built from its id.
    fn node<K: kube::Resource<DynamicType = ()>>(
        &self,
        object: &K,
        parent: Option<&str>,
        status: Status,
        children: impl FnOnce(&str) -> Vec<XrayNode>,
    ) -> XrayNode {
        let kind = K::kind(&()).into_owned();
        let meta = object.meta();
        let name = meta.name.clone().unwrap_or_default();
        let id = id(parent, &kind, meta.namespace.as_deref(), &name);
        let children = children(&id);
        XrayNode {
            id,
            kind,
            name,
            namespace: meta.namespace.clone(),
            resource: Some(resource_ref::<K>()),
            health: status.health,
            detail: status.detail,
            children,
        }
    }

    /// A workload whose children are the pods it controls.
    fn owner<K: kube::Resource<DynamicType = ()>>(
        &self,
        object: &K,
        status: Status,
        parent: Option<&str>,
    ) -> XrayNode {
        self.node(object, parent, status, |id| self.pods_of(object.meta(), id))
    }

    fn pods_of(&self, meta: &kube::api::ObjectMeta, parent: &str) -> Vec<XrayNode> {
        let mut pods: Vec<XrayNode> = self
            .owned_by(meta)
            .iter()
            .filter_map(|o| match o {
                Owned::Pod(pod) => Some(self.pod(pod, Some(parent))),
                _ => None,
            })
            .collect();
        sort(&mut pods);
        pods
    }

    fn deployment(&self, deployment: &Deployment) -> XrayNode {
        self.node(deployment, None, health::deployment(deployment), |id| {
            let mut sets: Vec<XrayNode> = self
                .owned_by(&deployment.metadata)
                .iter()
                .filter_map(|o| match o {
                    Owned::ReplicaSet(set) => Some(*set),
                    _ => None,
                })
                // Old revisions are kept scaled to 0; only the ones with pods matter.
                .filter(|set| {
                    set.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1) > 0
                        || self
                            .owned_by(&set.metadata)
                            .iter()
                            .any(|o| matches!(o, Owned::Pod(_)))
                })
                .map(|set| self.owner(set, health::replica_set(set), Some(id)))
                .collect();
            sort(&mut sets);
            sets
        })
    }

    fn cron_job(&self, cron: &CronJob) -> XrayNode {
        self.node(cron, None, health::cron_job(cron), |id| {
            let mut jobs: Vec<XrayNode> = self
                .owned_by(&cron.metadata)
                .iter()
                .filter_map(|o| match o {
                    Owned::Job(job) => Some(self.owner(*job, health::job(job), Some(id))),
                    _ => None,
                })
                .collect();
            sort(&mut jobs);
            jobs
        })
    }

    fn service(&self, service: &Service, parent: Option<&str>) -> XrayNode {
        let spec = service.spec.as_ref();
        let kind = spec.and_then(|s| s.type_.as_deref()).unwrap_or("ClusterIP");
        let address = spec
            .and_then(|s| s.cluster_ip.as_deref())
            .filter(|ip| *ip != "None")
            .or(spec.and_then(|s| s.external_name.as_deref()))
            .map(|a| format!(" · {a}"))
            .unwrap_or_default();
        let selector = spec.and_then(|s| s.selector.as_ref()).filter(|s| !s.is_empty());
        let pods: Vec<&Pod> = match selector {
            Some(selector) => self
                .inventory
                .pods
                .iter()
                .filter(|p| p.metadata.namespace == service.metadata.namespace)
                .filter(|p| selects(selector, p.metadata.labels.as_ref()))
                .collect(),
            None => Vec::new(),
        };
        let ready = pods.iter().filter(|p| is_ready(p)).count();
        let status = if selector.is_none() {
            Status {
                health: Health::Ok,
                detail: format!("{kind}{address} · no selector"),
            }
        } else if pods.is_empty() {
            Status {
                health: Health::Warning,
                detail: format!("{kind}{address} · selects no pods"),
            }
        } else if ready == 0 {
            Status {
                health: Health::Failed,
                detail: format!("{kind}{address} · no ready pods"),
            }
        } else {
            Status {
                health: Health::Ok,
                detail: format!("{kind}{address}"),
            }
        };
        self.node(service, parent, status, |id| {
            let mut nodes: Vec<XrayNode> = pods.iter().map(|p| self.pod(p, Some(id))).collect();
            sort(&mut nodes);
            nodes
        })
    }

    fn ingress(&self, ingress: &Ingress) -> XrayNode {
        let spec = ingress.spec.as_ref();
        let mut backends: Vec<&str> = spec
            .and_then(|s| s.default_backend.as_ref())
            .and_then(|b| b.service.as_ref())
            .map(|s| s.name.as_str())
            .into_iter()
            .collect();
        let rules = spec
            .and_then(|s| s.rules.as_ref())
            .map(Vec::as_slice)
            .unwrap_or_default();
        backends.extend(
            rules
                .iter()
                .filter_map(|r| r.http.as_ref())
                .flat_map(|h| &h.paths)
                .filter_map(|p| p.backend.service.as_ref())
                .map(|s| s.name.as_str()),
        );
        backends.sort();
        backends.dedup();
        let hosts: Vec<&str> = rules.iter().filter_map(|r| r.host.as_deref()).collect();
        let hosts = if hosts.is_empty() {
            "*".to_owned()
        } else {
            hosts.join(", ")
        };
        let namespace = ingress.metadata.namespace.as_deref();
        let find = |name: &str| {
            self.inventory.services.iter().find(|s| {
                s.metadata.namespace.as_deref() == namespace && s.metadata.name.as_deref() == Some(name)
            })
        };
        let missing = backends.iter().filter(|b| find(b).is_none()).count();
        let status = if missing > 0 {
            Status {
                health: Health::Warning,
                detail: match missing {
                    1 => format!("{hosts} · 1 backend missing"),
                    n => format!("{hosts} · {n} backends missing"),
                },
            }
        } else {
            Status {
                health: Health::Ok,
                detail: hosts,
            }
        };
        self.node(ingress, None, status, |id| {
            backends
                .iter()
                .map(|name| match find(name) {
                    Some(service) => self.service(service, Some(id)),
                    None => missing_node(id, "Service", namespace, name, false),
                })
                .collect()
        })
    }

    fn pod(&self, pod: &Pod, parent: Option<&str>) -> XrayNode {
        self.node(pod, parent, health::pod(pod), |id| {
            let mut children = containers(pod, id);
            if let Some(spec) = &pod.spec {
                let namespace = pod.metadata.namespace.as_deref();
                children.extend(
                    references(spec)
                        .into_iter()
                        .map(|r| self.reference(&r, namespace, id)),
                );
            }
            children
        })
    }

    fn reference(&self, reference: &Reference, namespace: Option<&str>, parent: &str) -> XrayNode {
        let name = reference.name.as_str();
        let key = (namespace.unwrap_or_default().to_owned(), name.to_owned());
        let (kind, resource, exists) = match reference.kind {
            RefKind::ServiceAccount => (
                "ServiceAccount",
                resource_ref::<ServiceAccount>(),
                self.inventory.service_accounts.as_ref().map(|s| s.contains(&key)),
            ),
            RefKind::ConfigMap => (
                "ConfigMap",
                resource_ref::<ConfigMap>(),
                self.inventory.config_maps.as_ref().map(|s| s.contains(&key)),
            ),
            RefKind::Secret => (
                "Secret",
                resource_ref::<Secret>(),
                self.inventory.secrets.as_ref().map(|s| s.contains(&key)),
            ),
            RefKind::Claim => {
                let claim = self.inventory.claims.as_ref().map(|claims| {
                    claims.iter().find(|c| {
                        c.metadata.namespace.as_deref() == namespace
                            && c.metadata.name.as_deref() == Some(name)
                    })
                });
                return match claim {
                    Some(Some(claim)) => self.node(claim, Some(parent), health::claim(claim), |_| Vec::new()),
                    Some(None) => missing_node(
                        parent,
                        "PersistentVolumeClaim",
                        namespace,
                        name,
                        reference.optional,
                    ),
                    None => unchecked_node(
                        parent,
                        "PersistentVolumeClaim",
                        resource_ref::<PersistentVolumeClaim>(),
                        namespace,
                        name,
                    ),
                };
            }
        };
        match exists {
            Some(true) => XrayNode {
                id: id(Some(parent), kind, namespace, name),
                kind: kind.to_owned(),
                name: name.to_owned(),
                namespace: namespace.map(str::to_owned),
                resource: Some(resource),
                health: Health::Ok,
                detail: String::new(),
                children: Vec::new(),
            },
            Some(false) => missing_node(parent, kind, namespace, name, reference.optional),
            None => unchecked_node(parent, kind, resource, namespace, name),
        }
    }
}

fn missing_node(parent: &str, kind: &str, namespace: Option<&str>, name: &str, optional: bool) -> XrayNode {
    let (health, detail) = if optional {
        (Health::Ok, "optional · not found")
    } else {
        (Health::Failed, "not found")
    };
    XrayNode {
        id: id(Some(parent), kind, namespace, name),
        kind: kind.to_owned(),
        name: name.to_owned(),
        namespace: namespace.map(str::to_owned),
        resource: None,
        health,
        detail: detail.to_owned(),
        children: Vec::new(),
    }
}

/// A reference whose kind could not be listed, so it is not known whether it exists.
fn unchecked_node(
    parent: &str,
    kind: &str,
    resource: ResourceRef,
    namespace: Option<&str>,
    name: &str,
) -> XrayNode {
    XrayNode {
        id: id(Some(parent), kind, namespace, name),
        kind: kind.to_owned(),
        name: name.to_owned(),
        namespace: namespace.map(str::to_owned),
        resource: Some(resource),
        health: Health::Ok,
        detail: "not checked".to_owned(),
        children: Vec::new(),
    }
}

fn containers(pod: &Pod, parent: &str) -> Vec<XrayNode> {
    let Some(spec) = &pod.spec else {
        return Vec::new();
    };
    let status = pod.status.as_ref();
    let init_statuses = status
        .and_then(|s| s.init_container_statuses.as_deref())
        .unwrap_or_default();
    let statuses = status
        .and_then(|s| s.container_statuses.as_deref())
        .unwrap_or_default();
    let init = spec
        .init_containers
        .iter()
        .flatten()
        .map(|c| ("InitContainer", c, init_statuses));
    let regular = spec.containers.iter().map(|c| ("Container", c, statuses));
    init.chain(regular)
        .map(|(kind, container, statuses)| {
            let status = statuses
                .iter()
                .find(|s| s.name == container.name)
                .map(health::container)
                .unwrap_or(Status {
                    health: Health::Warning,
                    detail: "Pending".to_owned(),
                });
            let image = container.image.as_deref().unwrap_or_default();
            XrayNode {
                id: id(Some(parent), kind, None, &container.name),
                kind: kind.to_owned(),
                name: container.name.clone(),
                namespace: pod.metadata.namespace.clone(),
                resource: None,
                health: status.health,
                detail: format!("{} · {image}", status.detail),
                children: Vec::new(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RefKind {
    ServiceAccount,
    ConfigMap,
    Secret,
    Claim,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    pub kind: RefKind,
    pub name: String,
    /// Every use is marked optional, so the pod starts without it.
    pub optional: bool,
}

/// The objects a pod spec uses, each once, sorted by kind and name.
pub(crate) fn references(spec: &PodSpec) -> Vec<Reference> {
    let mut found: BTreeMap<(RefKind, String), bool> = BTreeMap::new();
    let mut add = |kind: RefKind, name: &str, optional: Option<bool>| {
        if name.is_empty() {
            return;
        }
        let optional = optional.unwrap_or(false);
        found
            .entry((kind, name.to_owned()))
            .and_modify(|o| *o &= optional)
            .or_insert(optional);
    };

    add(
        RefKind::ServiceAccount,
        spec.service_account_name.as_deref().unwrap_or("default"),
        None,
    );
    for secret in spec.image_pull_secrets.iter().flatten() {
        add(RefKind::Secret, &secret.name, None);
    }
    for volume in spec.volumes.iter().flatten() {
        if let Some(c) = &volume.config_map {
            add(RefKind::ConfigMap, &c.name, c.optional);
        }
        if let Some(s) = &volume.secret {
            add(
                RefKind::Secret,
                s.secret_name.as_deref().unwrap_or_default(),
                s.optional,
            );
        }
        if let Some(c) = &volume.persistent_volume_claim {
            add(RefKind::Claim, &c.claim_name, None);
        }
        let sources = volume.projected.as_ref().and_then(|p| p.sources.as_ref());
        for source in sources.into_iter().flatten() {
            if let Some(c) = &source.config_map {
                add(RefKind::ConfigMap, &c.name, c.optional);
            }
            if let Some(s) = &source.secret {
                add(RefKind::Secret, &s.name, s.optional);
            }
        }
    }
    let containers = spec.init_containers.iter().flatten().chain(&spec.containers);
    for container in containers {
        for env in container.env.iter().flatten() {
            let from = env.value_from.as_ref();
            if let Some(c) = from.and_then(|f| f.config_map_key_ref.as_ref()) {
                add(RefKind::ConfigMap, &c.name, c.optional);
            }
            if let Some(s) = from.and_then(|f| f.secret_key_ref.as_ref()) {
                add(RefKind::Secret, &s.name, s.optional);
            }
        }
        for from in container.env_from.iter().flatten() {
            if let Some(c) = &from.config_map_ref {
                add(RefKind::ConfigMap, &c.name, c.optional);
            }
            if let Some(s) = &from.secret_ref {
                add(RefKind::Secret, &s.name, s.optional);
            }
        }
    }
    found
        .into_iter()
        .map(|((kind, name), optional)| Reference { kind, name, optional })
        .collect()
}

/// Whether a Service's selector matches a pod's labels.
pub(crate) fn selects(
    selector: &BTreeMap<String, String>,
    labels: Option<&BTreeMap<String, String>>,
) -> bool {
    let Some(labels) = labels else {
        return false;
    };
    selector.iter().all(|(k, v)| labels.get(k) == Some(v))
}

pub(crate) fn is_ready(pod: &Pod) -> bool {
    pod.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .into_iter()
        .flatten()
        .any(|c| c.type_ == "Ready" && c.status == "True")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn owner(kind: &str, uid: &str) -> Value {
        json!([{ "apiVersion": "apps/v1", "kind": kind, "name": "x", "uid": uid, "controller": true }])
    }

    fn pod(name: &str, owner_refs: Value, labels: Value, spec: Value) -> Pod {
        from(json!({
            "metadata": { "name": name, "namespace": "shop", "uid": format!("pod-{name}"),
                          "ownerReferences": owner_refs, "labels": labels },
            "spec": spec,
            "status": { "phase": "Running",
                        "conditions": [{ "type": "Ready", "status": "True" }],
                        "containerStatuses": [{ "name": "app", "image": "web:1", "imageID": "",
                                                "ready": true, "restartCount": 0, "state": { "running": {} } }] }
        }))
    }

    fn app_spec() -> Value {
        json!({
            "serviceAccountName": "web",
            "containers": [{
                "name": "app", "image": "web:1",
                "envFrom": [{ "configMapRef": { "name": "settings" } }],
                "env": [
                    { "name": "TOKEN", "valueFrom": { "secretKeyRef": { "name": "token", "key": "t" } } },
                    { "name": "EXTRA", "valueFrom": { "configMapKeyRef": { "name": "extra", "key": "e", "optional": true } } }
                ]
            }],
            "volumes": [
                { "name": "data", "persistentVolumeClaim": { "claimName": "data" } },
                { "name": "tls", "secret": { "secretName": "token" } }
            ]
        })
    }

    fn keys(items: &[&str]) -> Option<HashSet<(String, String)>> {
        Some(
            items
                .iter()
                .map(|n| ("shop".to_owned(), (*n).to_owned()))
                .collect(),
        )
    }

    fn inventory() -> Inventory {
        Inventory {
            pods: vec![
                pod(
                    "web-abc-1",
                    owner("ReplicaSet", "rs-new"),
                    json!({ "app": "web" }),
                    app_spec(),
                ),
                pod(
                    "web-abc-2",
                    owner("ReplicaSet", "rs-new"),
                    json!({ "app": "web" }),
                    app_spec(),
                ),
                pod(
                    "lonely",
                    json!([]),
                    json!({ "app": "other" }),
                    json!({ "containers": [{ "name": "app" }] }),
                ),
            ],
            replica_sets: vec![
                from(
                    json!({ "metadata": { "name": "web-abc", "namespace": "shop", "uid": "rs-new",
                                           "ownerReferences": owner("Deployment", "dep") },
                             "spec": { "replicas": 2, "selector": {} }, "status": { "replicas": 2, "readyReplicas": 2 } }),
                ),
                from(
                    json!({ "metadata": { "name": "web-old", "namespace": "shop", "uid": "rs-old",
                                           "ownerReferences": owner("Deployment", "dep") },
                             "spec": { "replicas": 0, "selector": {} }, "status": { "replicas": 0 } }),
                ),
            ],
            services: vec![from(json!({ "metadata": { "name": "web", "namespace": "shop" },
                                        "spec": { "selector": { "app": "web" }, "clusterIP": "10.0.0.7" } }))],
            claims: Some(vec![from(
                json!({ "metadata": { "name": "data", "namespace": "shop" },
                                           "status": { "phase": "Bound" } }),
            )]),
            config_maps: keys(&["settings"]),
            secrets: keys(&[]),
            service_accounts: keys(&["web", "default"]),
            ..Default::default()
        }
    }

    fn deployment() -> Deployment {
        from(
            json!({ "metadata": { "name": "web", "namespace": "shop", "uid": "dep" },
                     "spec": { "replicas": 2, "selector": {}, "template": {} },
                     "status": { "readyReplicas": 2 } }),
        )
    }

    fn child<'a>(node: &'a XrayNode, kind: &str, name: &str) -> &'a XrayNode {
        node.children
            .iter()
            .find(|c| c.kind == kind && c.name == name)
            .unwrap_or_else(|| panic!("{} has no {kind} {name}", node.id))
    }

    #[test]
    fn follows_deployments_down_to_pod_references() {
        let inventory = inventory();
        let tree = Tree::new(&inventory);
        let root = tree.deployment(&deployment());
        assert_eq!(root.id, "Deployment:shop/web");
        assert_eq!(root.health, Health::Ok);
        // The old revision has no pods and is left out.
        assert_eq!(root.children.len(), 1);
        let set = child(&root, "ReplicaSet", "web-abc");
        assert_eq!(set.children.len(), 2);
        let pod = child(set, "Pod", "web-abc-1");
        assert_eq!(pod.id, "Deployment:shop/web/ReplicaSet:web-abc/Pod:web-abc-1");
        assert_eq!(pod.resource.as_ref().unwrap().plural, "pods");

        let container = child(pod, "Container", "app");
        assert_eq!(container.detail, "Running · web:1");
        assert_eq!(container.resource, None);
        assert_eq!(child(pod, "ServiceAccount", "web").health, Health::Ok);
        assert_eq!(child(pod, "ConfigMap", "settings").health, Health::Ok);
        assert_eq!(child(pod, "PersistentVolumeClaim", "data").detail, "Bound");

        let token = child(pod, "Secret", "token");
        assert_eq!(
            (token.health, token.detail.as_str()),
            (Health::Failed, "not found")
        );
        assert_eq!(token.resource, None);
        let extra = child(pod, "ConfigMap", "extra");
        assert_eq!(
            (extra.health, extra.detail.as_str()),
            (Health::Ok, "optional · not found")
        );
    }

    #[test]
    fn unlistable_references_are_not_reported_missing() {
        let mut inventory = inventory();
        inventory.secrets = None;
        let tree = Tree::new(&inventory);
        let pod = tree.pod(&inventory.pods[0], None);
        let token = child(&pod, "Secret", "token");
        assert_eq!((token.health, token.detail.as_str()), (Health::Ok, "not checked"));
    }

    #[test]
    fn services_list_the_pods_they_select() {
        let inventory = inventory();
        let tree = Tree::new(&inventory);
        let service = tree.service(&inventory.services[0], None);
        assert_eq!(service.detail, "ClusterIP · 10.0.0.7");
        let names: Vec<&str> = service.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["web-abc-1", "web-abc-2"]);

        let empty: Service = from(json!({ "metadata": { "name": "none", "namespace": "shop" },
                                          "spec": { "selector": { "app": "nothing" } } }));
        assert_eq!(tree.service(&empty, None).health, Health::Warning);
    }

    #[test]
    fn ingresses_flag_missing_backends() {
        let mut inventory = inventory();
        inventory.services.truncate(1);
        let tree = Tree::new(&inventory);
        let ingress: Ingress = from(
            json!({ "metadata": { "name": "shop", "namespace": "shop" }, "spec": {
                "rules": [{ "host": "shop.example.com", "http": { "paths": [
                    { "path": "/", "pathType": "Prefix", "backend": { "service": { "name": "web", "port": { "number": 80 } } } },
                    { "path": "/api", "pathType": "Prefix", "backend": { "service": { "name": "api", "port": { "number": 80 } } } }
                ] } }]
            }}),
        );
        let node = tree.ingress(&ingress);
        assert_eq!(node.health, Health::Warning);
        assert_eq!(node.detail, "shop.example.com · 1 backend missing");
        assert_eq!(child(&node, "Service", "api").health, Health::Failed);
        assert_eq!(child(&node, "Service", "web").children.len(), 2);
    }

    #[test]
    fn collects_each_reference_once() {
        let spec: PodSpec = from(app_spec());
        let refs = references(&spec);
        let names: Vec<(RefKind, &str, bool)> = refs
            .iter()
            .map(|r| (r.kind, r.name.as_str(), r.optional))
            .collect();
        assert_eq!(
            names,
            [
                (RefKind::ServiceAccount, "web", false),
                (RefKind::ConfigMap, "extra", true),
                (RefKind::ConfigMap, "settings", false),
                (RefKind::Secret, "token", false),
                (RefKind::Claim, "data", false),
            ]
        );
    }

    #[test]
    fn recognizes_root_kinds() {
        assert_eq!(Root::of("apps", "Deployment"), Some(Root::Deployment));
        assert_eq!(Root::of("", "Pod"), Some(Root::Pod));
        assert_eq!(Root::of("", "ConfigMap"), None);
        assert_eq!(Root::of("example.com", "Deployment"), None);
    }
}
