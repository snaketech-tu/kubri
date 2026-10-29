//! XRay as a graph: the objects of a namespace and how they connect, from Ingresses through
//! Services to workloads, their pods, the nodes they run on and the ConfigMaps, Secrets and
//! claims they use. Also the request path of each Ingress rule, Ingress → Service →
//! EndpointSlice → Pod, with the first broken link named.

use std::collections::{BTreeMap, HashMap, HashSet};

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{ConfigMap, Node, PersistentVolumeClaim, Pod, PodSpec, Secret, Service};
use k8s_openapi::api::discovery::v1::EndpointSlice;
use k8s_openapi::api::networking::v1::{Ingress, IngressServiceBackend};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::Api;
use kube::{Client, Resource};
use serde::Serialize;

use crate::health::{self, Health, Status};
use crate::typed::{list, names, resource_ref, scoped};
use crate::xray::{RefKind, is_ready, references, selects};
use crate::{ResourceRef, Result};

/// Pods shown per owner, those with problems first; the owner says how many more there are.
const PODS_PER_OWNER: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    /// `Kind/namespace/name`.
    pub id: String,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    /// Set for objects that can be opened; not for missing ones.
    pub resource: Option<ResourceRef>,
    pub health: Health,
    pub detail: String,
    /// The application it belongs to, from `app.kubernetes.io/name`, `app` or `k8s-app`, or
    /// from what it is connected to.
    pub app: Option<String>,
    /// The node a pod runs on, or the node of a workload's first pod.
    pub node: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgeKind {
    /// An Ingress sends traffic to a Service.
    Routes,
    /// A Service selects a pod.
    Selects,
    /// A workload owns a pod.
    Owns,
    /// A workload or pod uses a ConfigMap, Secret or claim.
    Uses,
    /// A pod runs on a node.
    RunsOn,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}

/// One step of a request path.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathStep {
    /// The graph node, if the step is one.
    pub id: Option<String>,
    pub kind: String,
    pub name: String,
    pub health: Health,
    pub detail: String,
}

/// How a request for one host and path of an Ingress reaches pods.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPath {
    pub ingress: String,
    pub namespace: Option<String>,
    pub host: Option<String>,
    pub path: String,
    /// Ingress, Service and EndpointSlice, in that order, as far as they exist.
    pub steps: Vec<PathStep>,
    /// The pods the endpoints lead to.
    pub pods: Vec<PathStep>,
    /// Why requests do not arrive, at the first broken step.
    pub broken: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub paths: Vec<RequestPath>,
    /// Kinds that could not be listed, so are missing from the graph.
    pub missing: Vec<String>,
}

/// What the graph is built from.
#[derive(Default)]
pub(crate) struct Inventory {
    pub pods: Vec<Pod>,
    pub replica_sets: Vec<ReplicaSet>,
    pub deployments: Vec<Deployment>,
    pub stateful_sets: Vec<StatefulSet>,
    pub daemon_sets: Vec<DaemonSet>,
    pub jobs: Vec<Job>,
    pub cron_jobs: Vec<CronJob>,
    pub services: Vec<Service>,
    pub ingresses: Vec<Ingress>,
    /// `None` when they could not be listed: then paths go by the pods a Service selects.
    pub slices: Option<Vec<EndpointSlice>>,
    pub config_maps: Option<HashSet<(String, String)>>,
    pub secrets: Option<HashSet<(String, String)>>,
    pub claims: Option<Vec<PersistentVolumeClaim>>,
    pub nodes: Option<Vec<Node>>,
    pub missing: Vec<String>,
}

pub(crate) async fn gather(client: Client, namespace: Option<&str>) -> Result<Graph> {
    let c = &client;
    let (pods, replica_sets, deployments, stateful_sets, daemon_sets, jobs, cron_jobs) = futures::join!(
        list(scoped::<Pod>(c, namespace)),
        list(scoped::<ReplicaSet>(c, namespace)),
        list(scoped::<Deployment>(c, namespace)),
        list(scoped::<StatefulSet>(c, namespace)),
        list(scoped::<DaemonSet>(c, namespace)),
        list(scoped::<Job>(c, namespace)),
        list(scoped::<CronJob>(c, namespace)),
    );
    let (services, ingresses, slices, config_maps, secrets, claims, nodes) = futures::join!(
        list(scoped::<Service>(c, namespace)),
        list(scoped::<Ingress>(c, namespace)),
        list(scoped::<EndpointSlice>(c, namespace)),
        names(scoped::<ConfigMap>(c, namespace)),
        names(scoped::<Secret>(c, namespace)),
        list(scoped::<PersistentVolumeClaim>(c, namespace)),
        list(Api::<Node>::all(client.clone())),
    );
    let mut missing = Vec::new();
    fn or_none<T>(missing: &mut Vec<String>, kind: &str, listed: Result<Vec<T>>) -> Vec<T> {
        listed.unwrap_or_else(|_| {
            missing.push(kind.to_owned());
            Vec::new()
        })
    }
    let inventory = Inventory {
        pods: pods?,
        replica_sets: or_none(&mut missing, "ReplicaSets", replica_sets),
        deployments: or_none(&mut missing, "Deployments", deployments),
        stateful_sets: or_none(&mut missing, "StatefulSets", stateful_sets),
        daemon_sets: or_none(&mut missing, "DaemonSets", daemon_sets),
        jobs: or_none(&mut missing, "Jobs", jobs),
        cron_jobs: or_none(&mut missing, "CronJobs", cron_jobs),
        services: or_none(&mut missing, "Services", services),
        ingresses: or_none(&mut missing, "Ingresses", ingresses),
        slices: slices.ok(),
        config_maps: config_maps.ok().map(HashSet::from_iter),
        secrets: secrets.ok().map(HashSet::from_iter),
        claims: claims.ok(),
        nodes: nodes.ok(),
        missing,
    };
    Ok(build(&inventory))
}

fn id(kind: &str, namespace: Option<&str>, name: &str) -> String {
    format!("{kind}/{}/{name}", namespace.unwrap_or_default())
}

fn meta_id<K: Resource<DynamicType = ()>>(meta: &ObjectMeta) -> String {
    id(
        &K::kind(&()),
        meta.namespace.as_deref(),
        meta.name.as_deref().unwrap_or_default(),
    )
}

/// The application of an object, from its labels.
fn app_of(labels: Option<&BTreeMap<String, String>>) -> Option<String> {
    let labels = labels?;
    [
        "app.kubernetes.io/name",
        "app",
        "k8s-app",
        "app.kubernetes.io/instance",
    ]
    .iter()
    .find_map(|key| labels.get(*key).cloned())
}

struct Builder {
    nodes: Vec<GraphNode>,
    index: HashMap<String, usize>,
    edges: Vec<GraphEdge>,
    seen_edges: HashSet<(String, String)>,
}

impl Builder {
    fn node<K: Resource<DynamicType = ()>>(
        &mut self,
        meta: &ObjectMeta,
        status: Status,
        app: Option<String>,
    ) -> String {
        let id = meta_id::<K>(meta);
        if !self.index.contains_key(&id) {
            self.index.insert(id.clone(), self.nodes.len());
            self.nodes.push(GraphNode {
                id: id.clone(),
                kind: K::kind(&()).into_owned(),
                name: meta.name.clone().unwrap_or_default(),
                namespace: meta.namespace.clone(),
                resource: Some(resource_ref::<K>()),
                health: status.health,
                detail: status.detail,
                app,
                node: None,
            });
        }
        id
    }

    /// An object something names that is not there.
    fn missing(&mut self, kind: &str, namespace: Option<&str>, name: &str) -> String {
        let id = id(kind, namespace, name);
        if !self.index.contains_key(&id) {
            self.index.insert(id.clone(), self.nodes.len());
            self.nodes.push(GraphNode {
                id: id.clone(),
                kind: kind.to_owned(),
                name: name.to_owned(),
                namespace: namespace.map(str::to_owned),
                resource: None,
                health: Health::Failed,
                detail: "Missing".to_owned(),
                app: None,
                node: None,
            });
        }
        id
    }

    fn edge(&mut self, from: &str, to: &str, kind: EdgeKind) {
        if self.seen_edges.insert((from.to_owned(), to.to_owned())) {
            self.edges.push(GraphEdge {
                from: from.to_owned(),
                to: to.to_owned(),
                kind,
            });
        }
    }

    fn get(&mut self, id: &str) -> Option<&mut GraphNode> {
        self.index.get(id).map(|&i| &mut self.nodes[i])
    }
}

/// The controller of an object, as `(kind, uid)`.
fn controller(meta: &ObjectMeta) -> Option<(&str, &str)> {
    meta.owner_references
        .iter()
        .flatten()
        .find(|o| o.controller == Some(true))
        .map(|o| (o.kind.as_str(), o.uid.as_str()))
}

pub(crate) fn build(inv: &Inventory) -> Graph {
    let mut b = Builder {
        nodes: Vec::new(),
        index: HashMap::new(),
        edges: Vec::new(),
        seen_edges: HashSet::new(),
    };

    // Workloads, each with the pod template it stamps out.
    let mut by_uid: HashMap<&str, String> = HashMap::new();
    let mut templates: Vec<(String, Option<String>, &PodSpec)> = Vec::new();
    macro_rules! workloads {
        ($list:expr, $kind:ty, $health:expr, $w:ident => $spec:expr) => {
            for $w in &$list {
                let app = app_of($w.metadata.labels.as_ref());
                let id = b.node::<$kind>(&$w.metadata, $health($w), app);
                if let Some(uid) = $w.metadata.uid.as_deref() {
                    by_uid.insert(uid, id.clone());
                }
                if let Some(spec) = $spec {
                    templates.push((id, $w.metadata.namespace.clone(), spec));
                }
            }
        };
    }
    workloads!(inv.deployments, Deployment, health::deployment, w => w.spec.as_ref().and_then(|s| s.template.spec.as_ref()));
    workloads!(inv.stateful_sets, StatefulSet, health::stateful_set, w => w.spec.as_ref().and_then(|s| s.template.spec.as_ref()));
    workloads!(inv.daemon_sets, DaemonSet, health::daemon_set, w => w.spec.as_ref().and_then(|s| s.template.spec.as_ref()));
    workloads!(inv.cron_jobs, CronJob, health::cron_job, w => w.spec.job_template.spec.as_ref().and_then(|s| s.template.spec.as_ref()));
    // Jobs of a CronJob stand for it; others are workloads of their own.
    let cron_uids: HashMap<&str, String> = by_uid.clone();
    for job in &inv.jobs {
        match controller(&job.metadata) {
            Some(("CronJob", uid)) if cron_uids.contains_key(uid) => {
                if let Some(own) = job.metadata.uid.as_deref() {
                    by_uid.insert(own, cron_uids[uid].clone());
                }
            }
            _ => {
                let app = app_of(job.metadata.labels.as_ref());
                let id = b.node::<Job>(&job.metadata, health::job(job), app);
                if let Some(uid) = job.metadata.uid.as_deref() {
                    by_uid.insert(uid, id.clone());
                }
                if let Some(spec) = job.spec.as_ref().and_then(|s| s.template.spec.as_ref()) {
                    templates.push((id, job.metadata.namespace.clone(), spec));
                }
            }
        }
    }
    // ReplicaSets stand for their Deployment; one without is a workload of its own.
    for rs in &inv.replica_sets {
        let Some(own) = rs.metadata.uid.as_deref() else {
            continue;
        };
        match controller(&rs.metadata).and_then(|(_, uid)| by_uid.get(uid).cloned()) {
            Some(owner) => {
                by_uid.insert(own, owner);
            }
            None if rs.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0) > 0 => {
                let app = app_of(rs.metadata.labels.as_ref());
                let id = b.node::<ReplicaSet>(&rs.metadata, health::replica_set(rs), app);
                by_uid.insert(own, id.clone());
                if let Some(spec) = rs
                    .spec
                    .as_ref()
                    .and_then(|s| s.template.as_ref())
                    .and_then(|t| t.spec.as_ref())
                {
                    templates.push((id, rs.metadata.namespace.clone(), spec));
                }
            }
            None => {}
        }
    }

    // Pods, under their owner; bare pods use what their own spec names.
    let mut shown: HashMap<String, usize> = HashMap::new();
    let mut hidden: HashMap<String, usize> = HashMap::new();
    let mut pod_ids: HashMap<(String, String), String> = HashMap::new();
    let mut pods: Vec<(&Pod, Status)> = inv.pods.iter().map(|p| (p, health::pod(p))).collect();
    pods.sort_by_key(|(_, status)| std::cmp::Reverse(status.health));
    for (pod, status) in pods {
        let owner = controller(&pod.metadata).and_then(|(_, uid)| by_uid.get(uid).cloned());
        if let Some(owner) = &owner {
            let n = shown.entry(owner.clone()).or_default();
            if *n >= PODS_PER_OWNER {
                *hidden.entry(owner.clone()).or_default() += 1;
                continue;
            }
            *n += 1;
        }
        let app = app_of(pod.metadata.labels.as_ref());
        let pod_id = b.node::<Pod>(&pod.metadata, status, app);
        let node_name = pod.spec.as_ref().and_then(|s| s.node_name.clone());
        if let Some(n) = b.get(&pod_id) {
            n.node = node_name.clone();
        }
        pod_ids.insert(
            (
                pod.metadata.namespace.clone().unwrap_or_default(),
                pod.metadata.name.clone().unwrap_or_default(),
            ),
            pod_id.clone(),
        );
        match &owner {
            Some(owner) => {
                b.edge(owner, &pod_id, EdgeKind::Owns);
                let first = b.get(owner).is_some_and(|o| o.node.is_none());
                if first && let Some(o) = b.get(owner) {
                    o.node = node_name.clone();
                }
            }
            None => {
                if let Some(spec) = &pod.spec {
                    templates.push((pod_id.clone(), pod.metadata.namespace.clone(), spec));
                }
            }
        }
        if let Some(node) = &node_name {
            let target = match inv
                .nodes
                .as_ref()
                .and_then(|nodes| nodes.iter().find(|n| n.metadata.name.as_ref() == Some(node)))
            {
                Some(n) => b.node::<Node>(&n.metadata, health::node(n), None),
                None => {
                    let id = id("Node", None, node);
                    if !b.index.contains_key(&id) {
                        b.index.insert(id.clone(), b.nodes.len());
                        b.nodes.push(GraphNode {
                            id: id.clone(),
                            kind: "Node".into(),
                            name: node.clone(),
                            namespace: None,
                            resource: Some(resource_ref::<Node>()),
                            health: Health::Ok,
                            detail: String::new(),
                            app: None,
                            node: Some(node.clone()),
                        });
                    }
                    id
                }
            };
            if let Some(n) = b.get(&target) {
                n.node = Some(node.clone());
            }
            b.edge(&pod_id, &target, EdgeKind::RunsOn);
        }
    }
    for (owner, more) in hidden {
        if let Some(o) = b.get(&owner) {
            let shown = PODS_PER_OWNER;
            o.detail = format!("{} · {shown} of {} pods shown", o.detail, shown + more);
        }
    }

    // What the pod specs use.
    for (from, namespace, spec) in templates {
        let ns = namespace.as_deref();
        let key = |name: &str| (namespace.clone().unwrap_or_default(), name.to_owned());
        for r in references(spec) {
            let target = match r.kind {
                RefKind::ServiceAccount => continue,
                RefKind::ConfigMap => match &inv.config_maps {
                    Some(known) if !known.contains(&key(&r.name)) => {
                        if r.optional {
                            continue;
                        }
                        b.missing("ConfigMap", ns, &r.name)
                    }
                    _ => b.node::<ConfigMap>(&named(ns, &r.name), Status::new(Health::Ok, ""), None),
                },
                RefKind::Secret => match &inv.secrets {
                    Some(known) if !known.contains(&key(&r.name)) => {
                        if r.optional {
                            continue;
                        }
                        b.missing("Secret", ns, &r.name)
                    }
                    _ => b.node::<Secret>(&named(ns, &r.name), Status::new(Health::Ok, ""), None),
                },
                RefKind::Claim => {
                    let found = inv.claims.as_ref().map(|claims| {
                        claims.iter().find(|c| {
                            c.metadata.namespace == namespace && c.metadata.name.as_deref() == Some(&r.name)
                        })
                    });
                    match found {
                        Some(Some(claim)) => {
                            b.node::<PersistentVolumeClaim>(&claim.metadata, health::claim(claim), None)
                        }
                        Some(None) => b.missing("PersistentVolumeClaim", ns, &r.name),
                        None => b.node::<PersistentVolumeClaim>(
                            &named(ns, &r.name),
                            Status::new(Health::Ok, ""),
                            None,
                        ),
                    }
                }
            };
            b.edge(&from, &target, EdgeKind::Uses);
        }
    }

    // Services and the pods they select.
    let mut service_ids: HashMap<(String, String), String> = HashMap::new();
    for service in &inv.services {
        let selector = service
            .spec
            .as_ref()
            .and_then(|s| s.selector.clone())
            .unwrap_or_default();
        let selected: Vec<&Pod> = if selector.is_empty() {
            Vec::new()
        } else {
            inv.pods
                .iter()
                .filter(|p| {
                    p.metadata.namespace == service.metadata.namespace
                        && selects(&selector, p.metadata.labels.as_ref())
                })
                .collect()
        };
        let ready = selected.iter().filter(|p| is_ready(p)).count();
        let status = if selector.is_empty() {
            Status::new(Health::Ok, service_type(service))
        } else if selected.is_empty() {
            Status::new(Health::Failed, "Selects no pod")
        } else if ready == 0 {
            Status::new(Health::Failed, format!("None of {} pods ready", selected.len()))
        } else if ready < selected.len() {
            Status::new(
                Health::Warning,
                format!("{ready} of {} pods ready", selected.len()),
            )
        } else {
            Status::new(Health::Ok, format!("{} · {ready} ready", service_type(service)))
        };
        let app = app_of(service.metadata.labels.as_ref()).or_else(|| app_of(Some(&selector)));
        let service_id = b.node::<Service>(&service.metadata, status, app);
        service_ids.insert(
            (
                service.metadata.namespace.clone().unwrap_or_default(),
                service.metadata.name.clone().unwrap_or_default(),
            ),
            service_id.clone(),
        );
        for pod in selected {
            let key = (
                pod.metadata.namespace.clone().unwrap_or_default(),
                pod.metadata.name.clone().unwrap_or_default(),
            );
            if let Some(pod_id) = pod_ids.get(&key).cloned() {
                b.edge(&service_id, &pod_id, EdgeKind::Selects);
            }
        }
    }

    // Ingresses, their Services and the request paths through them.
    let mut paths = Vec::new();
    for ingress in &inv.ingresses {
        let namespace = ingress.metadata.namespace.clone();
        let name = ingress.metadata.name.clone().unwrap_or_default();
        let mut found = Vec::new();
        for (host, path, backend) in backends(ingress) {
            let path = request_path(inv, &name, namespace.as_deref(), host, path, backend, &pod_ids);
            found.push(path);
        }
        let broken = found.iter().filter(|p| p.broken.is_some()).count();
        let status = if found.is_empty() {
            Status::new(Health::Warning, "No backends")
        } else if broken == found.len() {
            Status::new(Health::Failed, "No path reaches a ready pod")
        } else if broken > 0 {
            Status::new(
                Health::Warning,
                format!("{broken} of {} paths broken", found.len()),
            )
        } else {
            Status::new(Health::Ok, hosts(ingress))
        };
        let app = app_of(ingress.metadata.labels.as_ref());
        let ingress_id = b.node::<Ingress>(&ingress.metadata, status, app);
        let mut tls: Vec<&str> = ingress
            .spec
            .iter()
            .flat_map(|s| s.tls.iter().flatten())
            .filter_map(|t| t.secret_name.as_deref())
            .collect();
        tls.dedup();
        for secret in tls {
            let key = (namespace.clone().unwrap_or_default(), secret.to_owned());
            let target = match &inv.secrets {
                Some(known) if !known.contains(&key) => b.missing("Secret", namespace.as_deref(), secret),
                _ => b.node::<Secret>(
                    &named(namespace.as_deref(), secret),
                    Status::new(Health::Ok, ""),
                    None,
                ),
            };
            b.edge(&ingress_id, &target, EdgeKind::Uses);
        }
        for p in &found {
            if let Some(service) = p.steps.get(1) {
                let target =
                    match service_ids.get(&(namespace.clone().unwrap_or_default(), service.name.clone())) {
                        Some(id) => id.clone(),
                        None => b.missing("Service", namespace.as_deref(), &service.name),
                    };
                b.edge(&ingress_id, &target, EdgeKind::Routes);
            }
        }
        for mut p in found {
            p.steps[0].id = Some(ingress_id.clone());
            p.steps[0].health = b.get(&ingress_id).map(|n| n.health).unwrap_or(Health::Ok);
            paths.push(p);
        }
    }

    inherit_apps(&mut b);
    Graph {
        nodes: b.nodes,
        edges: b.edges,
        paths,
        missing: inv.missing.clone(),
    }
}

fn named(namespace: Option<&str>, name: &str) -> ObjectMeta {
    ObjectMeta {
        namespace: namespace.map(str::to_owned),
        name: Some(name.to_owned()),
        ..Default::default()
    }
}

fn service_type(service: &Service) -> String {
    service
        .spec
        .as_ref()
        .and_then(|s| s.type_.clone())
        .unwrap_or_else(|| "ClusterIP".to_owned())
}

fn hosts(ingress: &Ingress) -> String {
    let hosts: Vec<&str> = ingress
        .spec
        .iter()
        .flat_map(|s| s.rules.iter().flatten())
        .filter_map(|r| r.host.as_deref())
        .collect();
    if hosts.is_empty() {
        "Any host".to_owned()
    } else {
        hosts.join(", ")
    }
}

/// Each host and path of an Ingress with the Service it goes to; the default backend as `/*`.
fn backends(ingress: &Ingress) -> Vec<(Option<String>, String, &IngressServiceBackend)> {
    let Some(spec) = &ingress.spec else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for rule in spec.rules.iter().flatten() {
        for p in rule.http.iter().flat_map(|h| &h.paths) {
            if let Some(service) = &p.backend.service {
                found.push((
                    rule.host.clone(),
                    p.path.clone().unwrap_or_else(|| "/".into()),
                    service,
                ));
            }
        }
    }
    if let Some(service) = spec.default_backend.as_ref().and_then(|d| d.service.as_ref()) {
        found.push((None, "/* (default)".to_owned(), service));
    }
    found
}

fn step(id: Option<String>, kind: &str, name: &str, status: Status) -> PathStep {
    PathStep {
        id,
        kind: kind.to_owned(),
        name: name.to_owned(),
        health: status.health,
        detail: status.detail,
    }
}

fn request_path(
    inv: &Inventory,
    ingress: &str,
    namespace: Option<&str>,
    host: Option<String>,
    path: String,
    backend: &IngressServiceBackend,
    pod_ids: &HashMap<(String, String), String>,
) -> RequestPath {
    let ns = namespace.unwrap_or_default();
    let mut result = RequestPath {
        ingress: ingress.to_owned(),
        namespace: namespace.map(str::to_owned),
        host,
        path,
        steps: vec![step(None, "Ingress", ingress, Status::new(Health::Ok, ""))],
        pods: Vec::new(),
        broken: None,
    };
    let port = backend.port.as_ref();
    let wanted = port
        .and_then(|p| p.number.map(|n| n.to_string()).or_else(|| p.name.clone()))
        .unwrap_or_default();
    let service_id = Some(id("Service", namespace, &backend.name));
    let Some(service) = inv.services.iter().find(|s| {
        s.metadata.namespace.as_deref() == namespace && s.metadata.name.as_deref() == Some(&backend.name)
    }) else {
        result.steps.push(step(
            service_id,
            "Service",
            &backend.name,
            Status::new(Health::Failed, "Missing"),
        ));
        result.broken = Some(format!("Service {} does not exist", backend.name));
        return result;
    };
    let ports = service.spec.as_ref().and_then(|s| s.ports.as_ref());
    let has_port = ports.into_iter().flatten().any(|p| {
        port.and_then(|w| w.number) == Some(p.port)
            || (port.and_then(|w| w.name.as_deref()).is_some()
                && port.and_then(|w| w.name.as_deref()) == p.name.as_deref())
    });
    if !has_port {
        result.steps.push(step(
            service_id,
            "Service",
            &backend.name,
            Status::new(Health::Failed, format!("No port {wanted}")),
        ));
        result.broken = Some(format!("Service {} has no port {wanted}", backend.name));
        return result;
    }
    result.steps.push(step(
        service_id,
        "Service",
        &backend.name,
        Status::new(Health::Ok, format!("port {wanted}")),
    ));

    // The pods behind it: from its EndpointSlices, or else from its selector.
    let selector = service
        .spec
        .as_ref()
        .and_then(|s| s.selector.clone())
        .unwrap_or_default();
    let mut pods: Vec<(String, bool)> = Vec::new();
    let mut slice_names = Vec::new();
    match &inv.slices {
        Some(slices) => {
            for slice in slices.iter().filter(|s| {
                s.metadata.namespace.as_deref() == namespace
                    && s.metadata
                        .labels
                        .as_ref()
                        .and_then(|l| l.get("kubernetes.io/service-name"))
                        == Some(&backend.name)
            }) {
                slice_names.push(slice.metadata.name.clone().unwrap_or_default());
                for endpoint in slice.endpoints.iter().flatten() {
                    let ready = endpoint.conditions.as_ref().and_then(|c| c.ready).unwrap_or(true);
                    let name = endpoint
                        .target_ref
                        .as_ref()
                        .filter(|t| t.kind.as_deref() == Some("Pod"))
                        .and_then(|t| t.name.clone())
                        .unwrap_or_else(|| endpoint.addresses.join(", "));
                    pods.push((name, ready));
                }
            }
        }
        None => {
            for pod in inv.pods.iter().filter(|p| {
                !selector.is_empty()
                    && p.metadata.namespace.as_deref() == namespace
                    && selects(&selector, p.metadata.labels.as_ref())
            }) {
                pods.push((pod.metadata.name.clone().unwrap_or_default(), is_ready(pod)));
            }
        }
    }
    let ready = pods.iter().filter(|(_, r)| *r).count();
    let (status, broken) = if pods.is_empty() {
        let why = if selector.is_empty() {
            format!("Service {} has no selector and no endpoints", backend.name)
        } else {
            let labels: Vec<String> = selector.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!(
                "Service {} selects no pod: no pod has {}",
                backend.name,
                labels.join(",")
            )
        };
        (Status::new(Health::Failed, "No endpoints"), Some(why))
    } else if ready == 0 {
        (
            Status::new(Health::Failed, format!("0 of {} ready", pods.len())),
            Some(format!(
                "None of the {} pods behind Service {} is ready",
                pods.len(),
                backend.name
            )),
        )
    } else if ready < pods.len() {
        (
            Status::new(Health::Warning, format!("{ready} of {} ready", pods.len())),
            None,
        )
    } else {
        (Status::new(Health::Ok, format!("{ready} ready")), None)
    };
    let label = if inv.slices.is_some() {
        if slice_names.len() == 1 {
            slice_names[0].clone()
        } else {
            format!("{} slices", slice_names.len())
        }
    } else {
        "selected pods".to_owned()
    };
    result.steps.push(step(None, "EndpointSlice", &label, status));
    result.broken = broken;
    for (name, ready) in pods {
        let pod = inv.pods.iter().find(|p| {
            p.metadata.namespace.as_deref() == namespace && p.metadata.name.as_deref() == Some(&name)
        });
        let status = match pod {
            Some(p) => health::pod(p),
            None if ready => Status::new(Health::Ok, "Ready"),
            None => Status::new(Health::Warning, "Not ready"),
        };
        let pod_id = pod_ids.get(&(ns.to_owned(), name.clone())).cloned();
        result.pods.push(step(pod_id, "Pod", &name, status));
    }
    result
}

/// Objects without an application of their own take the one of what they connect to.
fn inherit_apps(b: &mut Builder) {
    for _ in 0..3 {
        let apps: HashMap<String, String> = b
            .nodes
            .iter()
            .filter_map(|n| n.app.clone().map(|a| (n.id.clone(), a)))
            .collect();
        let mut changed = false;
        for edge in &b.edges {
            let (from, to) = (&edge.from, &edge.to);
            for (have, want) in [(from, to), (to, from)] {
                if edge.kind == EdgeKind::RunsOn {
                    continue;
                }
                if let Some(app) = apps.get(have)
                    && let Some(&i) = b.index.get(want)
                    && b.nodes[i].app.is_none()
                    && b.nodes[i].kind != "Node"
                {
                    b.nodes[i].app = Some(app.clone());
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn ready_pod(name: &str, rs_uid: &str, ready: bool) -> Pod {
        from(json!({
            "metadata": { "name": name, "namespace": "shop", "uid": name, "labels": { "app": "web" },
                "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "web-1", "uid": rs_uid, "controller": true }] },
            "spec": { "nodeName": "node-a", "containers": [{ "name": "app", "image": "web",
                "envFrom": [{ "secretRef": { "name": "web-env" } }] }] },
            "status": { "phase": "Running", "conditions": [{ "type": "Ready", "status": if ready { "True" } else { "False" } }] }
        }))
    }

    fn inventory(ready: bool) -> Inventory {
        Inventory {
            deployments: vec![from(json!({
                "metadata": { "name": "web", "namespace": "shop", "uid": "d1", "labels": { "app": "web" } },
                "spec": { "selector": {}, "template": { "spec": { "containers": [{ "name": "app",
                    "envFrom": [{ "secretRef": { "name": "web-env" } }, { "configMapRef": { "name": "gone" } }] }] } } },
                "status": { "replicas": 1, "readyReplicas": 1, "availableReplicas": 1, "updatedReplicas": 1 }
            }))],
            replica_sets: vec![from(json!({
                "metadata": { "name": "web-1", "namespace": "shop", "uid": "rs1",
                    "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "Deployment", "name": "web", "uid": "d1", "controller": true }] },
                "spec": { "selector": {}, "replicas": 1 }
            }))],
            pods: vec![ready_pod("web-1-a", "rs1", ready)],
            services: vec![from(json!({
                "metadata": { "name": "web", "namespace": "shop" },
                "spec": { "selector": { "app": "web" }, "ports": [{ "port": 80 }] }
            }))],
            ingresses: vec![from(json!({
                "metadata": { "name": "shop", "namespace": "shop" },
                "spec": {
                    "tls": [{ "secretName": "shop-tls" }],
                    "rules": [{ "host": "shop.example", "http": { "paths": [
                        { "path": "/", "pathType": "Prefix", "backend": { "service": { "name": "web", "port": { "number": 80 } } } },
                        { "path": "/api", "pathType": "Prefix", "backend": { "service": { "name": "api", "port": { "number": 80 } } } },
                        { "path": "/admin", "pathType": "Prefix", "backend": { "service": { "name": "web", "port": { "number": 8080 } } } }
                    ] } }]
                }
            }))],
            slices: Some(vec![from(json!({
                "metadata": { "name": "web-abc", "namespace": "shop", "labels": { "kubernetes.io/service-name": "web" } },
                "addressType": "IPv4",
                "endpoints": [{ "addresses": ["10.42.0.5"], "conditions": { "ready": ready },
                    "targetRef": { "kind": "Pod", "name": "web-1-a", "namespace": "shop" } }]
            }))]),
            config_maps: Some(HashSet::new()),
            secrets: Some(
                [
                    ("shop".to_owned(), "web-env".to_owned()),
                    ("shop".to_owned(), "shop-tls".to_owned()),
                ]
                .into(),
            ),
            ..Default::default()
        }
    }

    #[test]
    fn connects_ingress_service_workload_pod_and_node() {
        let graph = build(&inventory(true));
        let ids: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
        for id in [
            "Deployment/shop/web",
            "Pod/shop/web-1-a",
            "Node//node-a",
            "Service/shop/web",
            "Ingress/shop/shop",
            "Secret/shop/web-env",
            "Secret/shop/shop-tls",
            "ConfigMap/shop/gone",
            "Service/shop/api",
        ] {
            assert!(ids.contains(&id), "{id} in {ids:?}");
        }
        let edge = |from: &str, to: &str| {
            graph
                .edges
                .iter()
                .find(|e| e.from == from && e.to == to)
                .map(|e| e.kind)
        };
        assert_eq!(
            edge("Deployment/shop/web", "Pod/shop/web-1-a"),
            Some(EdgeKind::Owns)
        );
        assert_eq!(
            edge("Service/shop/web", "Pod/shop/web-1-a"),
            Some(EdgeKind::Selects)
        );
        assert_eq!(
            edge("Ingress/shop/shop", "Service/shop/web"),
            Some(EdgeKind::Routes)
        );
        assert_eq!(edge("Pod/shop/web-1-a", "Node//node-a"), Some(EdgeKind::RunsOn));
        assert_eq!(
            edge("Deployment/shop/web", "Secret/shop/web-env"),
            Some(EdgeKind::Uses)
        );
        assert_eq!(
            edge("Ingress/shop/shop", "Secret/shop/shop-tls"),
            Some(EdgeKind::Uses)
        );
        // The ReplicaSet stands for its Deployment and is not shown.
        assert!(!ids.iter().any(|id| id.starts_with("ReplicaSet/")));
        let node = |id: &str| graph.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(node("ConfigMap/shop/gone").health, Health::Failed);
        assert_eq!(node("Service/shop/api").detail, "Missing");
        // The Ingress takes the application of its Service, and its TLS Secret that of the Ingress.
        assert_eq!(node("Secret/shop/shop-tls").app.as_deref(), Some("web"));
        assert_eq!(node("Secret/shop/web-env").app.as_deref(), Some("web"));
        assert_eq!(node("Deployment/shop/web").node.as_deref(), Some("node-a"));
    }

    #[test]
    fn names_the_first_broken_link_of_each_path() {
        let graph = build(&inventory(true));
        let path = |p: &str| graph.paths.iter().find(|x| x.path == p).unwrap();
        let root = path("/");
        assert_eq!(root.broken, None);
        assert_eq!(
            root.steps.iter().map(|s| s.kind.as_str()).collect::<Vec<_>>(),
            ["Ingress", "Service", "EndpointSlice"]
        );
        assert_eq!(root.steps[2].name, "web-abc");
        assert_eq!(root.pods[0].id.as_deref(), Some("Pod/shop/web-1-a"));
        assert_eq!(path("/api").broken.as_deref(), Some("Service api does not exist"));
        assert_eq!(
            path("/admin").broken.as_deref(),
            Some("Service web has no port 8080")
        );
        let ingress = graph.nodes.iter().find(|n| n.kind == "Ingress").unwrap();
        assert_eq!(ingress.health, Health::Warning);

        let down = build(&inventory(false));
        let root = down.paths.iter().find(|x| x.path == "/").unwrap();
        assert_eq!(
            root.broken.as_deref(),
            Some("None of the 1 pods behind Service web is ready")
        );
        assert_eq!(root.steps[2].health, Health::Failed);

        let mut gone = inventory(true);
        gone.pods.clear();
        gone.slices = None;
        let graph = build(&gone);
        let root = graph.paths.iter().find(|x| x.path == "/").unwrap();
        assert_eq!(
            root.broken.as_deref(),
            Some("Service web selects no pod: no pod has app=web")
        );
        let service = graph.nodes.iter().find(|n| n.id == "Service/shop/web").unwrap();
        assert_eq!(service.detail, "Selects no pod");
    }
}
