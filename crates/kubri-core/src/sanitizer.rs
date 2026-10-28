//! A cluster sanitizer like Popeye: checks objects for problems and risky settings, and
//! scores each kind and the whole cluster.
//!
//! Each object scores 1 when it has no warnings or errors, ½ with warnings and 0 with an
//! error. A kind scores the average of its objects, the cluster the average of its kinds.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;

use futures::Future;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Node, PersistentVolume, PersistentVolumeClaim, Pod, PodSpec, PodTemplateSpec, Secret, Service,
    ServiceAccount,
};
use k8s_openapi::api::networking::v1::{Ingress, IngressBackend, NetworkPolicy};
use k8s_openapi::api::policy::v1::PodDisruptionBudget;
use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, Role, RoleBinding, RoleRef, Subject};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{ListParams, PartialObjectMeta};
use kube::{Api, Client, Resource, ResourceExt};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::health::{self, Health};
use crate::metrics::{Usage, parse_quantity};
use crate::typed::{list, resource_ref, scoped};
use crate::xray::{RefKind, is_ready, references, selects};
use crate::{ResourceRef, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    /// Worth knowing, does not lower the score.
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub level: Level,
    /// A stable name for the check, such as `image-tag`.
    pub code: &'static str,
    pub message: String,
    pub container: Option<String>,
}

impl Issue {
    fn new(level: Level, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            level,
            code,
            message: message.into(),
            container: None,
        }
    }

    fn info(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(Level::Info, code, message)
    }

    fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(Level::Warning, code, message)
    }

    fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(Level::Error, code, message)
    }

    fn of_health(code: &'static str, status: health::Status) -> Option<Self> {
        match status.health {
            Health::Ok => None,
            Health::Warning => Some(Self::warning(code, status.detail)),
            Health::Failed => Some(Self::error(code, status.detail)),
        }
    }

    fn in_container(mut self, name: &str) -> Self {
        self.container = Some(name.to_owned());
        self
    }
}

/// An object with at least one issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub namespace: Option<String>,
    pub name: String,
    /// The worst level of its issues.
    pub level: Level,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizerSection {
    pub kind: String,
    pub resource: ResourceRef,
    pub total: u32,
    /// Objects without warnings or errors; those with only infos count here too.
    pub ok: u32,
    pub warning: u32,
    pub error: u32,
    /// 0 to 100; `None` when the kind could not be listed.
    pub score: Option<u8>,
    /// Worst first, then by namespace and name.
    pub findings: Vec<Finding>,
    /// Why the kind could not be listed.
    pub failed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizerReport {
    /// 0 to 100, the average of the sections that have a score.
    pub score: u8,
    /// `A` (90 and up) to `F` (below 50), like Popeye.
    pub grade: char,
    pub namespace: Option<String>,
    /// Worst score first.
    pub sections: Vec<SanitizerSection>,
}

type Listed<T> = std::result::Result<Vec<T>, String>;

/// Everything the checks look at. Cluster-wide kinds are only scanned for all namespaces.
pub(crate) struct Inventory {
    pub namespace: Option<String>,
    pub pods: Listed<Pod>,
    pub deployments: Listed<Deployment>,
    pub stateful_sets: Listed<StatefulSet>,
    pub daemon_sets: Listed<DaemonSet>,
    pub replica_sets: Listed<ReplicaSet>,
    pub jobs: Listed<Job>,
    pub cron_jobs: Listed<CronJob>,
    pub services: Listed<Service>,
    pub config_maps: Listed<PartialObjectMeta<ConfigMap>>,
    /// Only metadata, so Secret data never leaves the cluster.
    pub secrets: Listed<PartialObjectMeta<Secret>>,
    pub service_accounts: Listed<ServiceAccount>,
    pub claims: Listed<PersistentVolumeClaim>,
    pub autoscalers: Listed<HorizontalPodAutoscaler>,
    pub budgets: Listed<PodDisruptionBudget>,
    pub ingresses: Listed<Ingress>,
    pub policies: Listed<NetworkPolicy>,
    pub roles: Listed<Role>,
    pub role_bindings: Listed<RoleBinding>,
    pub cluster: Option<ClusterInventory>,
}

pub(crate) struct ClusterInventory {
    pub nodes: Listed<Node>,
    pub volumes: Listed<PersistentVolume>,
    pub cluster_roles: Listed<ClusterRole>,
    pub cluster_role_bindings: Listed<ClusterRoleBinding>,
    /// Node usage from metrics-server, empty when it is not installed.
    pub usage: Vec<Usage>,
}

#[cfg(test)]
impl Default for Inventory {
    fn default() -> Self {
        Self {
            namespace: None,
            pods: Ok(Vec::new()),
            deployments: Ok(Vec::new()),
            stateful_sets: Ok(Vec::new()),
            daemon_sets: Ok(Vec::new()),
            replica_sets: Ok(Vec::new()),
            jobs: Ok(Vec::new()),
            cron_jobs: Ok(Vec::new()),
            services: Ok(Vec::new()),
            config_maps: Ok(Vec::new()),
            secrets: Ok(Vec::new()),
            service_accounts: Ok(Vec::new()),
            claims: Ok(Vec::new()),
            autoscalers: Ok(Vec::new()),
            budgets: Ok(Vec::new()),
            ingresses: Ok(Vec::new()),
            policies: Ok(Vec::new()),
            roles: Ok(Vec::new()),
            role_bindings: Ok(Vec::new()),
            cluster: None,
        }
    }
}

async fn listed<K>(api: Api<K>) -> Listed<K>
where
    K: Resource + Clone + DeserializeOwned + Debug,
{
    list(api).await.map_err(|e| e.to_string())
}

async fn metadata<K>(api: Api<K>) -> Listed<PartialObjectMeta<K>>
where
    K: Resource + Clone + DeserializeOwned + Debug,
{
    api.list_metadata(&ListParams::default().match_any())
        .await
        .map(|l| l.items)
        .map_err(|e| crate::Error::from(e).to_string())
}

/// Lists everything in `namespace`, or in the whole cluster, and checks it.
pub(crate) async fn scan(
    client: Client,
    namespace: Option<&str>,
    node_usage: impl Future<Output = Result<Vec<Usage>>>,
) -> SanitizerReport {
    let c = &client;
    let ns = namespace;
    let (pods, deployments, stateful_sets, daemon_sets, replica_sets, jobs, cron_jobs, services, config_maps) = futures::join!(
        listed(scoped::<Pod>(c, ns)),
        listed(scoped::<Deployment>(c, ns)),
        listed(scoped::<StatefulSet>(c, ns)),
        listed(scoped::<DaemonSet>(c, ns)),
        listed(scoped::<ReplicaSet>(c, ns)),
        listed(scoped::<Job>(c, ns)),
        listed(scoped::<CronJob>(c, ns)),
        listed(scoped::<Service>(c, ns)),
        metadata(scoped::<ConfigMap>(c, ns)),
    );
    let (secrets, service_accounts, claims, autoscalers, budgets, ingresses, policies, roles, role_bindings) = futures::join!(
        metadata(scoped::<Secret>(c, ns)),
        listed(scoped::<ServiceAccount>(c, ns)),
        listed(scoped::<PersistentVolumeClaim>(c, ns)),
        listed(scoped::<HorizontalPodAutoscaler>(c, ns)),
        listed(scoped::<PodDisruptionBudget>(c, ns)),
        listed(scoped::<Ingress>(c, ns)),
        listed(scoped::<NetworkPolicy>(c, ns)),
        listed(scoped::<Role>(c, ns)),
        listed(scoped::<RoleBinding>(c, ns)),
    );
    let cluster = match namespace {
        Some(_) => None,
        None => {
            let (nodes, volumes, cluster_roles, cluster_role_bindings, usage) = futures::join!(
                listed(Api::<Node>::all(client.clone())),
                listed(Api::<PersistentVolume>::all(client.clone())),
                listed(Api::<ClusterRole>::all(client.clone())),
                listed(Api::<ClusterRoleBinding>::all(client.clone())),
                node_usage,
            );
            Some(ClusterInventory {
                nodes,
                volumes,
                cluster_roles,
                cluster_role_bindings,
                usage: usage.unwrap_or_default(),
            })
        }
    };
    report(&Inventory {
        namespace: namespace.map(str::to_owned),
        pods,
        deployments,
        stateful_sets,
        daemon_sets,
        replica_sets,
        jobs,
        cron_jobs,
        services,
        config_maps,
        secrets,
        service_accounts,
        claims,
        autoscalers,
        budgets,
        ingresses,
        policies,
        roles,
        role_bindings,
        cluster,
    })
}

/// Restarts from which a container is reported.
const RESTARTS: i32 = 5;
/// Node CPU or memory use, as a share of allocatable, from which a node is reported.
const NODE_USE: f64 = 0.8;

type Key = (String, String);

fn key<K: ResourceExt>(object: &K) -> Key {
    (object.namespace().unwrap_or_default(), object.name_any())
}

/// What the pods of the cluster use, to tell unused objects apart.
#[derive(Default)]
struct Uses {
    config_maps: HashSet<Key>,
    secrets: HashSet<Key>,
    claims: HashSet<Key>,
    service_accounts: HashSet<Key>,
}

impl Uses {
    fn of(inventory: &Inventory) -> Option<Self> {
        let pods = inventory.pods.as_ref().ok()?;
        let mut uses = Self::default();
        let mut add = |ns: String, spec: &PodSpec| {
            for reference in references(spec) {
                let k = (ns.clone(), reference.name);
                match reference.kind {
                    RefKind::ConfigMap => uses.config_maps.insert(k),
                    RefKind::Secret => uses.secrets.insert(k),
                    RefKind::Claim => uses.claims.insert(k),
                    RefKind::ServiceAccount => uses.service_accounts.insert(k),
                };
            }
        };
        for pod in pods {
            if let Some(spec) = &pod.spec {
                add(pod.namespace().unwrap_or_default(), spec);
            }
        }
        // Workloads use their config even while no pod runs: a CronJob between runs, a
        // Deployment scaled to 0.
        let templates = inventory
            .deployments
            .iter()
            .flatten()
            .map(|d| (d.namespace(), d.spec.as_ref().map(|s| &s.template)))
            .chain(
                (inventory.stateful_sets.iter().flatten())
                    .map(|s| (s.namespace(), s.spec.as_ref().map(|s| &s.template))),
            )
            .chain(
                (inventory.daemon_sets.iter().flatten())
                    .map(|d| (d.namespace(), d.spec.as_ref().map(|s| &s.template))),
            )
            .chain(
                (inventory.replica_sets.iter().flatten())
                    .map(|r| (r.namespace(), r.spec.as_ref().and_then(|s| s.template.as_ref()))),
            )
            .chain(
                (inventory.jobs.iter().flatten())
                    .map(|j| (j.namespace(), j.spec.as_ref().map(|s| &s.template))),
            )
            .chain((inventory.cron_jobs.iter().flatten()).map(|c| {
                let job = c.spec.job_template.spec.as_ref();
                (c.namespace(), job.map(|s| &s.template))
            }));
        for (ns, template) in templates {
            if let Some(spec) = template.and_then(|t| t.spec.as_ref()) {
                add(ns.unwrap_or_default(), spec);
            }
        }
        for account in inventory.service_accounts.iter().flatten() {
            let ns = account.namespace().unwrap_or_default();
            let pull = account
                .image_pull_secrets
                .iter()
                .flatten()
                .map(|s| s.name.clone());
            let mounted = account.secrets.iter().flatten().filter_map(|s| s.name.clone());
            uses.secrets.extend(pull.chain(mounted).map(|n| (ns.clone(), n)));
        }
        for ingress in inventory.ingresses.iter().flatten() {
            let ns = ingress.namespace().unwrap_or_default();
            let tls = ingress.spec.as_ref().and_then(|s| s.tls.as_ref());
            let names = tls.into_iter().flatten().filter_map(|t| t.secret_name.clone());
            uses.secrets.extend(names.map(|n| (ns.clone(), n)));
        }
        Some(uses)
    }
}

/// Indexes that several checks share.
struct Index<'a> {
    pods_by_ns: HashMap<String, Vec<&'a Pod>>,
    uses: Option<Uses>,
}

impl<'a> Index<'a> {
    fn new(inventory: &'a Inventory) -> Self {
        let mut pods_by_ns: HashMap<String, Vec<&Pod>> = HashMap::new();
        for pod in inventory.pods.iter().flatten() {
            pods_by_ns
                .entry(pod.namespace().unwrap_or_default())
                .or_default()
                .push(pod);
        }
        Self {
            pods_by_ns,
            uses: Uses::of(inventory),
        }
    }

    /// The running pods of a namespace, `None` when pods could not be listed.
    fn pods(&self, inventory: &Inventory, namespace: &str) -> Option<Vec<&'a Pod>> {
        inventory.pods.as_ref().ok()?;
        let pods = self
            .pods_by_ns
            .get(namespace)
            .map(Vec::as_slice)
            .unwrap_or_default();
        Some(pods.iter().copied().filter(|p| !finished(p)).collect())
    }
}

pub(crate) fn report(inventory: &Inventory) -> SanitizerReport {
    let index = Index::new(inventory);
    let uses = index.uses.as_ref();
    let mut sections = Vec::new();
    let mut add = |section: Option<SanitizerSection>| sections.extend(section);

    add(section(&inventory.pods, |_| true, pod));
    add(section(
        &inventory.deployments,
        |_| true,
        |d| {
            let mut issues: Vec<Issue> = Issue::of_health("replicas", health::deployment(d))
                .into_iter()
                .collect();
            if d.spec.as_ref().and_then(|s| s.replicas) == Some(0) {
                issues.push(Issue::info("scaled-down", "Scaled to 0"));
            }
            issues.extend(template(d.spec.as_ref().map(|s| &s.template), false));
            issues
        },
    ));
    add(section(
        &inventory.stateful_sets,
        |_| true,
        |s| {
            let mut issues: Vec<Issue> = Issue::of_health("replicas", health::stateful_set(s))
                .into_iter()
                .collect();
            if s.spec.as_ref().and_then(|s| s.replicas) == Some(0) {
                issues.push(Issue::info("scaled-down", "Scaled to 0"));
            }
            issues.extend(template(s.spec.as_ref().map(|s| &s.template), false));
            issues
        },
    ));
    add(section(
        &inventory.daemon_sets,
        |_| true,
        |d| {
            let mut issues: Vec<Issue> = Issue::of_health("replicas", health::daemon_set(d))
                .into_iter()
                .collect();
            issues.extend(template(d.spec.as_ref().map(|s| &s.template), false));
            issues
        },
    ));
    // ReplicaSets of Deployments are checked through their Deployment.
    add(section(
        &inventory.replica_sets,
        |r| controller(r.meta()).is_none(),
        |r| {
            let mut issues: Vec<Issue> = Issue::of_health("replicas", health::replica_set(r))
                .into_iter()
                .collect();
            issues.extend(template(r.spec.as_ref().and_then(|s| s.template.as_ref()), false));
            issues
        },
    ));
    add(section(
        &inventory.jobs,
        |_| true,
        |j| {
            let mut issues: Vec<Issue> = Issue::of_health("job", health::job(j)).into_iter().collect();
            if controller(j.meta()).is_none() {
                issues.extend(template(j.spec.as_ref().map(|s| &s.template), true));
            }
            issues
        },
    ));
    add(section(
        &inventory.cron_jobs,
        |_| true,
        |c| {
            let mut issues: Vec<Issue> = Issue::of_health("last-run", health::cron_job(c))
                .into_iter()
                .collect();
            if c.spec.suspend == Some(true) {
                issues.push(Issue::info("suspended", "Suspended"));
            }
            issues.extend(template(
                c.spec.job_template.spec.as_ref().map(|s| &s.template),
                true,
            ));
            issues
        },
    ));
    add(section(
        &inventory.services,
        |_| true,
        |s| service(s, index.pods(inventory, &key(s).0)),
    ));
    add(section(
        &inventory.config_maps,
        |m| !system(m.meta()),
        |m| {
            let managed =
                m.name_any() == "kube-root-ca.crt" || m.labels().get("owner").is_some_and(|o| o == "helm");
            unused(
                uses.map(|u| &u.config_maps),
                m,
                managed,
                "Not used by any workload",
            )
        },
    ));
    add(section(
        &inventory.secrets,
        |m| !system(m.meta()),
        |m| {
            let managed = m.labels().get("owner").is_some_and(|o| o == "helm")
                || m.annotations().contains_key("kubernetes.io/service-account.name");
            unused(
                uses.map(|u| &u.secrets),
                m,
                managed,
                "Not used by any workload, service account or ingress",
            )
        },
    ));
    add(section(
        &inventory.service_accounts,
        |a| !system(a.meta()),
        |a| {
            unused(
                uses.map(|u| &u.service_accounts),
                a,
                a.name_any() == "default",
                "Not used by any workload",
            )
        },
    ));
    add(section(
        &inventory.claims,
        |_| true,
        |c| {
            let mut issues: Vec<Issue> = Issue::of_health("phase", health::claim(c)).into_iter().collect();
            let bound = c.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Bound");
            if bound {
                issues.extend(unused(
                    uses.map(|u| &u.claims),
                    c,
                    false,
                    "Not mounted by any pod",
                ));
            }
            issues
        },
    ));
    add(section(
        &inventory.autoscalers,
        |_| true,
        |h| autoscaler(h, inventory),
    ));
    add(section(
        &inventory.budgets,
        |_| true,
        |b| budget(b, index.pods(inventory, &key(b).0)),
    ));
    add(section(&inventory.ingresses, |_| true, |i| ingress(i, inventory)));
    add(section(
        &inventory.policies,
        |_| true,
        |p| policy(p, index.pods(inventory, &key(p).0)),
    ));

    let role_names = |kind: &str| -> Option<HashSet<Key>> {
        match kind {
            "Role" => inventory.roles.as_ref().ok().map(|r| r.iter().map(key).collect()),
            _ => {
                let cluster = inventory.cluster.as_ref()?;
                cluster
                    .cluster_roles
                    .as_ref()
                    .ok()
                    .map(|r| r.iter().map(key).collect())
            }
        }
    };
    let roles = role_names("Role");
    // Without the cluster inventory, cluster roles cannot be checked.
    let cluster_roles = role_names("ClusterRole");
    let accounts: Option<HashSet<Key>> = inventory
        .service_accounts
        .as_ref()
        .ok()
        .map(|a| a.iter().map(key).collect());
    let bound: HashSet<(String, Key)> = bound_roles(inventory);
    let scanned = inventory.namespace.as_deref();

    add(section(
        &inventory.roles,
        |_| true,
        |r| {
            let used = bound.contains(&("Role".to_owned(), key(r)));
            if used {
                Vec::new()
            } else {
                vec![Issue::info("unused", "Not bound by any role binding")]
            }
        },
    ));
    add(section(
        &inventory.role_bindings,
        |_| true,
        |b| {
            let ns = b.namespace().unwrap_or_default();
            binding(
                &b.role_ref,
                b.subjects.as_deref(),
                &ns,
                roles.as_ref(),
                cluster_roles.as_ref(),
                accounts.as_ref(),
                scanned,
            )
        },
    ));

    if let Some(cluster) = &inventory.cluster {
        let usage: HashMap<&str, &Usage> = cluster.usage.iter().map(|u| (u.name.as_str(), u)).collect();
        add(section(
            &cluster.nodes,
            |_| true,
            |n| node(n, usage.get(n.name_any().as_str()).copied()),
        ));
        add(section(
            &cluster.volumes,
            |_| true,
            |v| {
                let mut issues: Vec<Issue> =
                    Issue::of_health("phase", health::volume(v)).into_iter().collect();
                if v.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Available") {
                    issues.push(Issue::info("unclaimed", "Not claimed by any volume claim"));
                }
                issues
            },
        ));
        add(section(
            &cluster.cluster_roles,
            |r| !builtin_role(r),
            |r| {
                let used = bound.contains(&("ClusterRole".to_owned(), key(r)));
                if used {
                    Vec::new()
                } else {
                    vec![Issue::info(
                        "unused",
                        "Not bound by any role binding or cluster role binding",
                    )]
                }
            },
        ));
        add(section(
            &cluster.cluster_role_bindings,
            |_| true,
            |b| {
                binding(
                    &b.role_ref,
                    b.subjects.as_deref(),
                    "",
                    None,
                    cluster_roles.as_ref(),
                    accounts.as_ref(),
                    scanned,
                )
            },
        ));
    }

    sections.sort_by(|a, b| {
        let worst = |s: &SanitizerSection| s.score.map_or(u16::MAX, u16::from);
        worst(a).cmp(&worst(b)).then_with(|| a.kind.cmp(&b.kind))
    });
    let scores: Vec<u32> = sections.iter().filter_map(|s| s.score.map(u32::from)).collect();
    let score = if scores.is_empty() {
        100
    } else {
        (scores.iter().sum::<u32>() as f64 / scores.len() as f64).round() as u8
    };
    SanitizerReport {
        score,
        grade: grade(score),
        namespace: inventory.namespace.clone(),
        sections,
    }
}

fn grade(score: u8) -> char {
    match score {
        90.. => 'A',
        80..=89 => 'B',
        70..=79 => 'C',
        60..=69 => 'D',
        50..=59 => 'E',
        _ => 'F',
    }
}

/// Checks the objects of one kind; kinds without objects are left out.
fn section<K>(
    listed: &Listed<K>,
    keep: impl Fn(&K) -> bool,
    check: impl Fn(&K) -> Vec<Issue>,
) -> Option<SanitizerSection>
where
    K: Resource<DynamicType = ()>,
{
    let kind = K::kind(&()).into_owned();
    let resource = resource_ref::<K>();
    let objects = match listed {
        Ok(objects) => objects,
        Err(error) => {
            return Some(SanitizerSection {
                kind,
                resource,
                total: 0,
                ok: 0,
                warning: 0,
                error: 0,
                score: None,
                findings: Vec::new(),
                failed: Some(error.clone()),
            });
        }
    };
    let mut section = SanitizerSection {
        kind,
        resource,
        total: 0,
        ok: 0,
        warning: 0,
        error: 0,
        score: None,
        findings: Vec::new(),
        failed: None,
    };
    for object in objects.iter().filter(|o| keep(o)) {
        section.total += 1;
        let mut issues = check(object);
        let level = issues.iter().map(|i| i.level).max();
        match level {
            Some(Level::Error) => section.error += 1,
            Some(Level::Warning) => section.warning += 1,
            _ => section.ok += 1,
        }
        if let Some(level) = level {
            issues.sort_by_key(|i| std::cmp::Reverse(i.level));
            let meta = object.meta();
            section.findings.push(Finding {
                namespace: meta.namespace.clone(),
                name: meta.name.clone().unwrap_or_default(),
                level,
                issues,
            });
        }
    }
    if section.total == 0 {
        return None;
    }
    let points = f64::from(section.ok) + f64::from(section.warning) / 2.0;
    section.score = Some((100.0 * points / f64::from(section.total)).round() as u8);
    section.findings.sort_by(|a, b| {
        b.level
            .cmp(&a.level)
            .then_with(|| a.namespace.cmp(&b.namespace))
            .then_with(|| a.name.cmp(&b.name))
    });
    Some(section)
}

fn controller(meta: &ObjectMeta) -> Option<&str> {
    let owners = meta.owner_references.as_ref()?;
    owners
        .iter()
        .find(|o| o.controller == Some(true))
        .map(|o| o.kind.as_str())
}

/// Namespaces of the control plane, whose config is used in ways pods do not show.
fn system(meta: &ObjectMeta) -> bool {
    meta.namespace
        .as_deref()
        .is_some_and(|ns| ns.starts_with("kube-"))
}

fn finished(pod: &Pod) -> bool {
    matches!(
        pod.status.as_ref().and_then(|s| s.phase.as_deref()),
        Some("Succeeded" | "Failed")
    )
}

fn unused<K: ResourceExt>(
    uses: Option<&HashSet<Key>>,
    object: &K,
    exempt: bool,
    message: &str,
) -> Vec<Issue> {
    match uses {
        Some(uses) if !exempt && !uses.contains(&key(object)) => vec![Issue::info("unused", message)],
        _ => Vec::new(),
    }
}

fn pod(pod: &Pod) -> Vec<Issue> {
    if pod.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Succeeded") {
        return Vec::new();
    }
    let mut issues: Vec<Issue> = Issue::of_health("status", health::pod(pod)).into_iter().collect();
    let statuses = pod.status.as_ref().and_then(|s| s.container_statuses.as_ref());
    for status in statuses.into_iter().flatten() {
        if status.restart_count >= RESTARTS {
            issues.push(
                Issue::warning("restarts", format!("Restarted {} times", status.restart_count))
                    .in_container(&status.name),
            );
        }
    }
    let mirror = pod.annotations().contains_key("kubernetes.io/config.mirror");
    if controller(pod.meta()).is_none() && !mirror {
        issues.push(Issue::warning(
            "unmanaged",
            "Not managed by a controller, so nothing recreates it",
        ));
        if let Some(spec) = &pod.spec {
            issues.extend(pod_spec(spec, false));
        }
    }
    issues
}

fn template(template: Option<&PodTemplateSpec>, batch: bool) -> Vec<Issue> {
    template
        .and_then(|t| t.spec.as_ref())
        .map(|s| pod_spec(s, batch))
        .unwrap_or_default()
}

/// Checks a pod spec, as written in a workload. `batch` pods run to completion, so they
/// need no probes.
fn pod_spec(spec: &PodSpec, batch: bool) -> Vec<Issue> {
    let mut issues = Vec::new();
    let account = spec.service_account_name.as_deref().unwrap_or("default");
    if account == "default" && spec.automount_service_account_token != Some(false) {
        issues.push(Issue::info("default-account", "Uses the default service account"));
    }
    let pod_security = spec.security_context.as_ref();
    let pod_non_root = pod_security.and_then(|s| s.run_as_non_root) == Some(true)
        || pod_security.and_then(|s| s.run_as_user).is_some_and(|u| u > 0);
    for container in &spec.containers {
        let name = container.name.as_str();
        if let Some(issue) = image(container.image.as_deref()) {
            issues.push(issue.in_container(name));
        }
        if !batch {
            match (
                container.liveness_probe.is_some(),
                container.readiness_probe.is_some(),
            ) {
                (false, false) => {
                    issues
                        .push(Issue::warning("probes", "No liveness or readiness probe").in_container(name));
                }
                (true, false) => issues.push(Issue::info("probes", "No readiness probe").in_container(name)),
                (false, true) => issues.push(Issue::info("probes", "No liveness probe").in_container(name)),
                (true, true) => {}
            }
        }
        let resources = container.resources.as_ref();
        let requests = resources.and_then(|r| r.requests.as_ref());
        let limits = resources.and_then(|r| r.limits.as_ref());
        let requested = |r: &str| {
            requests.is_some_and(|m| m.contains_key(r)) || limits.is_some_and(|m| m.contains_key(r))
        };
        match (requested("cpu"), requested("memory")) {
            (false, false) => {
                issues.push(Issue::warning("resources", "No CPU or memory requests").in_container(name));
            }
            (true, false) => issues.push(Issue::warning("resources", "No memory request").in_container(name)),
            (false, true) => issues.push(Issue::info("resources", "No CPU request").in_container(name)),
            (true, true) => {}
        }
        if !limits.is_some_and(|m| m.contains_key("memory")) {
            issues.push(Issue::info("limits", "No memory limit").in_container(name));
        }
        if container.ports.iter().flatten().any(|p| p.name.is_none()) {
            issues.push(Issue::info("port-name", "Has a port without a name").in_container(name));
        }
        let security = container.security_context.as_ref();
        let non_root = security.and_then(|s| s.run_as_non_root).unwrap_or(pod_non_root)
            || security.and_then(|s| s.run_as_user).is_some_and(|u| u > 0);
        if !non_root {
            issues.push(Issue::info("root", "May run as root").in_container(name));
        }
    }
    issues
}

/// Images without a tag or with `latest` change under a running workload.
fn image(image: Option<&str>) -> Option<Issue> {
    let image = image.unwrap_or_default();
    if image.is_empty() || image.contains('@') {
        return None;
    }
    // The tag follows the last colon after the last slash; a colon before it is a registry port.
    let name = image.rsplit('/').next().unwrap_or(image);
    match name.rsplit_once(':') {
        None => Some(Issue::warning(
            "image-tag",
            format!("Image {image} has no tag, so it means latest"),
        )),
        Some((_, "latest")) => Some(Issue::warning(
            "image-tag",
            format!("Image {image} uses the latest tag"),
        )),
        Some(_) => None,
    }
}

fn service(service: &Service, pods: Option<Vec<&Pod>>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(spec) = &service.spec else {
        return issues;
    };
    match spec.type_.as_deref() {
        Some("NodePort") => issues.push(Issue::info(
            "node-port",
            "Type NodePort opens a port on every node",
        )),
        Some("LoadBalancer") => {
            let ingress = service
                .status
                .as_ref()
                .and_then(|s| s.load_balancer.as_ref())
                .and_then(|l| l.ingress.as_ref());
            if ingress.is_none_or(|i| i.is_empty()) {
                issues.push(Issue::warning(
                    "load-balancer",
                    "Waiting for a load balancer address",
                ));
            }
        }
        _ => {}
    }
    let selector = spec.selector.as_ref().filter(|s| !s.is_empty());
    let (Some(selector), Some(pods)) = (selector, pods) else {
        return issues;
    };
    let selected: Vec<&Pod> = pods
        .into_iter()
        .filter(|p| selects(selector, p.metadata.labels.as_ref()))
        .collect();
    if selected.is_empty() {
        issues.push(Issue::warning("endpoints", "Selects no pods"));
        return issues;
    }
    if !selected.iter().any(|p| is_ready(p)) {
        issues.push(Issue::error("endpoints", "None of its pods is ready"));
    }
    for port in spec.ports.iter().flatten() {
        let Some(IntOrString::String(target)) = &port.target_port else {
            continue;
        };
        let named = |p: &&Pod| {
            let Some(spec) = &p.spec else { return false };
            let sidecars = spec.init_containers.iter().flatten();
            sidecars
                .chain(&spec.containers)
                .flat_map(|c| c.ports.iter().flatten())
                .any(|cp| cp.name.as_deref() == Some(target))
        };
        if !selected.iter().any(named) {
            issues.push(Issue::error(
                "target-port",
                format!(
                    "Port {} targets {target}, which none of its pods names",
                    port.port
                ),
            ));
        }
    }
    issues
}

fn autoscaler(hpa: &HorizontalPodAutoscaler, inventory: &Inventory) -> Vec<Issue> {
    let mut issues: Vec<Issue> = Issue::of_health("replicas", health::autoscaler(hpa))
        .into_iter()
        .collect();
    let target = &hpa.spec.scale_target_ref;
    let wanted = (hpa.namespace().unwrap_or_default(), target.name.clone());
    let exists = match target.kind.as_str() {
        "Deployment" => inventory
            .deployments
            .as_ref()
            .ok()
            .map(|d| d.iter().any(|x| key(x) == wanted)),
        "StatefulSet" => inventory
            .stateful_sets
            .as_ref()
            .ok()
            .map(|d| d.iter().any(|x| key(x) == wanted)),
        "ReplicaSet" => inventory
            .replica_sets
            .as_ref()
            .ok()
            .map(|d| d.iter().any(|x| key(x) == wanted)),
        _ => None,
    };
    if exists == Some(false) {
        issues.push(Issue::error(
            "target",
            format!("Scales {} {}, which does not exist", target.kind, target.name),
        ));
    }
    issues
}

fn budget(budget: &PodDisruptionBudget, pods: Option<Vec<&Pod>>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let selector = budget.spec.as_ref().and_then(|s| s.selector.as_ref());
    if let (Some(selector), Some(pods)) = (selector, pods)
        && !pods.iter().any(|p| matches(selector, p.metadata.labels.as_ref()))
    {
        issues.push(Issue::warning("selector", "Selects no pods"));
        return issues;
    }
    if let Some(status) = &budget.status
        && status.expected_pods.unwrap_or(0) > 0
        && status.disruptions_allowed == Some(0)
    {
        issues.push(Issue::warning(
            "disruptions",
            "Allows no disruptions right now, so node drains stop at its pods",
        ));
    }
    issues
}

fn ingress(ingress: &Ingress, inventory: &Inventory) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(spec) = &ingress.spec else {
        return issues;
    };
    let ns = ingress.namespace().unwrap_or_default();
    let services: Option<HashMap<&str, &Service>> = inventory.services.as_ref().ok().map(|services| {
        services
            .iter()
            .filter(|s| s.metadata.namespace.as_deref() == Some(ns.as_str()))
            .map(|s| (s.metadata.name.as_deref().unwrap_or_default(), s))
            .collect()
    });
    let rules = spec.rules.iter().flatten();
    let paths = rules.flat_map(|r| r.http.iter().flat_map(|h| h.paths.iter()));
    let backends = spec.default_backend.iter().chain(paths.map(|p| &p.backend));
    let mut seen = HashSet::new();
    for backend in backends {
        let Some(service) = &backend.service else { continue };
        if !seen.insert((service.name.clone(), port_label(backend))) {
            continue;
        }
        let Some(services) = &services else { continue };
        let Some(found) = services.get(service.name.as_str()) else {
            issues.push(Issue::error(
                "backend",
                format!("Sends traffic to service {}, which does not exist", service.name),
            ));
            continue;
        };
        let ports = found.spec.as_ref().and_then(|s| s.ports.as_ref());
        let wanted = service.port.as_ref();
        let has_port = ports.into_iter().flatten().any(|p| match wanted {
            Some(w) => w.number.is_some_and(|n| n == p.port) || (w.name.is_some() && w.name == p.name),
            None => true,
        });
        if !has_port {
            issues.push(Issue::error(
                "backend",
                format!("Service {} has no port {}", service.name, port_label(backend)),
            ));
        }
    }
    if let Ok(secrets) = &inventory.secrets {
        let names: HashSet<&str> = secrets
            .iter()
            .filter(|s| s.metadata.namespace.as_deref() == Some(ns.as_str()))
            .filter_map(|s| s.metadata.name.as_deref())
            .collect();
        for tls in spec.tls.iter().flatten() {
            if let Some(name) = &tls.secret_name
                && !names.contains(name.as_str())
            {
                issues.push(Issue::error(
                    "tls",
                    format!("Its TLS secret {name} does not exist"),
                ));
            }
        }
    }
    issues
}

fn port_label(backend: &IngressBackend) -> String {
    let port = backend.service.as_ref().and_then(|s| s.port.as_ref());
    match port {
        Some(p) => p
            .number
            .map(|n| n.to_string())
            .or_else(|| p.name.clone())
            .unwrap_or_default(),
        None => String::new(),
    }
}

fn policy(policy: &NetworkPolicy, pods: Option<Vec<&Pod>>) -> Vec<Issue> {
    let selector = policy.spec.as_ref().and_then(|s| s.pod_selector.as_ref());
    // An empty selector means every pod of the namespace, which is deliberate.
    let Some(selector) = selector.filter(|s| !is_empty(s)) else {
        return Vec::new();
    };
    match pods {
        Some(pods) if !pods.iter().any(|p| matches(selector, p.metadata.labels.as_ref())) => {
            vec![Issue::warning("selector", "Selects no pods")]
        }
        _ => Vec::new(),
    }
}

fn is_empty(selector: &LabelSelector) -> bool {
    selector.match_labels.as_ref().is_none_or(|m| m.is_empty())
        && selector.match_expressions.as_ref().is_none_or(|e| e.is_empty())
}

/// Whether a label selector with match expressions selects `labels`.
pub(crate) fn matches(selector: &LabelSelector, labels: Option<&BTreeMap<String, String>>) -> bool {
    let empty = BTreeMap::new();
    let labels = labels.unwrap_or(&empty);
    let wanted = selector.match_labels.iter().flatten();
    if !wanted.into_iter().all(|(k, v)| labels.get(k) == Some(v)) {
        return false;
    }
    selector.match_expressions.iter().flatten().all(|e| {
        let values = e.values.as_deref().unwrap_or_default();
        let value = labels.get(&e.key);
        match e.operator.as_str() {
            "In" => value.is_some_and(|v| values.contains(v)),
            "NotIn" => value.is_none_or(|v| !values.contains(v)),
            "Exists" => value.is_some(),
            "DoesNotExist" => value.is_none(),
            _ => false,
        }
    })
}

/// The roles that some binding refers to, as (kind, (namespace, name)).
fn bound_roles(inventory: &Inventory) -> HashSet<(String, Key)> {
    let mut bound = HashSet::new();
    for binding in inventory.role_bindings.iter().flatten() {
        let r = &binding.role_ref;
        let ns = if r.kind == "Role" {
            binding.namespace().unwrap_or_default()
        } else {
            String::new()
        };
        bound.insert((r.kind.clone(), (ns, r.name.clone())));
    }
    if let Some(cluster) = &inventory.cluster {
        for binding in cluster.cluster_role_bindings.iter().flatten() {
            let r = &binding.role_ref;
            bound.insert((r.kind.clone(), (String::new(), r.name.clone())));
        }
    }
    bound
}

/// Roles that come with Kubernetes or are aggregated into others.
fn builtin_role(role: &ClusterRole) -> bool {
    role.name_any().starts_with("system:")
        || role.aggregation_rule.is_some()
        || role.labels().keys().any(|k| {
            k == "kubernetes.io/bootstrapping" || k.starts_with("rbac.authorization.k8s.io/aggregate-to-")
        })
}

fn binding(
    role: &RoleRef,
    subjects: Option<&[Subject]>,
    namespace: &str,
    roles: Option<&HashSet<Key>>,
    cluster_roles: Option<&HashSet<Key>>,
    accounts: Option<&HashSet<Key>>,
    scanned: Option<&str>,
) -> Vec<Issue> {
    let mut issues = Vec::new();
    let known = match role.kind.as_str() {
        "Role" => roles.map(|r| r.contains(&(namespace.to_owned(), role.name.clone()))),
        _ => cluster_roles.map(|r| r.contains(&(String::new(), role.name.clone()))),
    };
    if known == Some(false) {
        issues.push(Issue::error(
            "role",
            format!("Refers to {} {}, which does not exist", role.kind, role.name),
        ));
    }
    if let Some(accounts) = accounts {
        for subject in subjects
            .into_iter()
            .flatten()
            .filter(|s| s.kind == "ServiceAccount")
        {
            let ns = subject.namespace.clone().unwrap_or_else(|| namespace.to_owned());
            // Accounts of other namespaces are only known when scanning all of them.
            if scanned.is_some_and(|scanned| scanned != ns) {
                continue;
            }
            if !accounts.contains(&(ns.clone(), subject.name.clone())) {
                issues.push(Issue::warning(
                    "subject",
                    format!(
                        "Binds service account {ns}/{}, which does not exist",
                        subject.name
                    ),
                ));
            }
        }
    }
    issues
}

fn node(node: &Node, usage: Option<&Usage>) -> Vec<Issue> {
    let mut issues: Vec<Issue> = Issue::of_health("status", health::node(node))
        .into_iter()
        .collect();
    if node.spec.as_ref().and_then(|s| s.unschedulable) == Some(true) {
        // Cordoned on purpose: say so, without the warning health gives it. Pressure stays.
        issues.retain(|i| !(i.code == "status" && i.message.ends_with("SchedulingDisabled")));
        issues.push(Issue::info("cordoned", "Cordoned, so no new pods are scheduled"));
    }
    let Some(usage) = usage else {
        return issues;
    };
    let allocatable = node.status.as_ref().and_then(|s| s.allocatable.as_ref());
    let amount = |r: &str| {
        allocatable
            .and_then(|a| a.get(r))
            .and_then(|q| parse_quantity(&q.0))
    };
    for (resource, used, total) in [
        ("CPU", usage.cpu, amount("cpu").map(|c| c * 1000.0)),
        ("Memory", usage.memory, amount("memory")),
    ] {
        if let Some(total) = total.filter(|t| *t > 0.0)
            && used / total >= NODE_USE
        {
            issues.push(Issue::warning(
                "usage",
                format!(
                    "{resource} use is at {:.0}% of what is allocatable",
                    100.0 * used / total
                ),
            ));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn from<T: DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn container(extra: Value) -> Value {
        let mut c = json!({
            "name": "app",
            "image": "ghcr.io/acme/app:1.2.3",
            "livenessProbe": {"httpGet": {"path": "/", "port": 80}},
            "readinessProbe": {"httpGet": {"path": "/", "port": 80}},
            "resources": {"requests": {"cpu": "100m", "memory": "64Mi"}, "limits": {"memory": "128Mi"}},
            "ports": [{"name": "http", "containerPort": 80}],
            "securityContext": {"runAsNonRoot": true}
        });
        c.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        c
    }

    fn spec(extra: Value) -> PodSpec {
        from(json!({"serviceAccountName": "app", "containers": [container(extra)]}))
    }

    fn codes(issues: &[Issue]) -> Vec<(&'static str, Level)> {
        issues.iter().map(|i| (i.code, i.level)).collect()
    }

    fn pod(name: &str, labels: Value, ready: bool) -> Pod {
        from(json!({
            "metadata": {"name": name, "namespace": "shop", "labels": labels,
                "ownerReferences": [{"apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "rs", "uid": "u", "controller": true}]},
            "spec": {"serviceAccountName": "app", "containers": [container(json!({}))], "volumes": [{"name": "c", "configMap": {"name": "used"}}]},
            "status": {"phase": "Running", "conditions": [{"type": "Ready", "status": if ready { "True" } else { "False" }}],
                "containerStatuses": [{"name": "app", "ready": ready, "restartCount": 0, "image": "", "imageID": "", "state": {"running": {}}}]}
        }))
    }

    #[test]
    fn a_careful_spec_has_no_issues() {
        assert_eq!(pod_spec(&spec(json!({})), false), vec![]);
    }

    #[test]
    fn flags_risky_container_settings() {
        let issues = pod_spec(
            &spec(json!({
                "image": "nginx",
                "livenessProbe": null,
                "readinessProbe": null,
                "resources": {},
                "ports": [{"containerPort": 80}],
                "securityContext": null
            })),
            false,
        );
        assert_eq!(
            codes(&issues),
            vec![
                ("image-tag", Level::Warning),
                ("probes", Level::Warning),
                ("resources", Level::Warning),
                ("limits", Level::Info),
                ("port-name", Level::Info),
                ("root", Level::Info),
            ]
        );
        assert!(issues.iter().all(|i| i.container.as_deref() == Some("app")));
        // Batch pods need no probes.
        let batch = pod_spec(
            &spec(json!({"livenessProbe": null, "readinessProbe": null})),
            true,
        );
        assert_eq!(batch, vec![]);
    }

    #[test]
    fn reads_image_tags() {
        assert!(image(Some("nginx:1.27")).is_none());
        assert!(image(Some("registry:5000/team/app@sha256:abc")).is_none());
        assert!(image(Some("registry:5000/team/app:2")).is_none());
        assert_eq!(image(Some("registry:5000/team/app")).unwrap().code, "image-tag");
        assert!(image(Some("app:latest")).unwrap().message.contains("latest tag"));
    }

    #[test]
    fn services_need_ready_pods_and_named_ports() {
        let svc: Service = from(json!({
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"selector": {"app": "web"}, "ports": [{"port": 80, "targetPort": "http"}, {"port": 9090, "targetPort": "metrics"}]}
        }));
        let ready = pod("web-1", json!({"app": "web"}), true);
        let issues = service(&svc, Some(vec![&ready]));
        assert_eq!(codes(&issues), vec![("target-port", Level::Error)]);
        assert!(issues[0].message.contains("9090"));

        let unready = pod("web-2", json!({"app": "web"}), false);
        assert!(codes(&service(&svc, Some(vec![&unready]))).contains(&("endpoints", Level::Error)));
        let other = pod("db", json!({"app": "db"}), true);
        assert_eq!(
            codes(&service(&svc, Some(vec![&other]))),
            vec![("endpoints", Level::Warning)]
        );
        // Without pods, nothing is claimed about endpoints.
        assert_eq!(service(&svc, None), vec![]);
    }

    #[test]
    fn matches_label_selectors_with_expressions() {
        let selector: LabelSelector = from(json!({
            "matchLabels": {"app": "web"},
            "matchExpressions": [
                {"key": "tier", "operator": "In", "values": ["front", "edge"]},
                {"key": "canary", "operator": "DoesNotExist"}
            ]
        }));
        let labels = |v: Value| -> BTreeMap<String, String> { from(v) };
        assert!(matches(
            &selector,
            Some(&labels(json!({"app": "web", "tier": "edge"})))
        ));
        assert!(!matches(
            &selector,
            Some(&labels(json!({"app": "web", "tier": "back"})))
        ));
        assert!(!matches(
            &selector,
            Some(&labels(json!({"app": "web", "tier": "front", "canary": "1"})))
        ));
        assert!(!matches(&selector, None));
    }

    #[test]
    fn scores_kinds_and_grades_the_cluster() {
        let healthy = pod("web-1", json!({"app": "web"}), true);
        let mut crashing = pod("web-2", json!({"app": "web"}), false);
        crashing
            .status
            .as_mut()
            .unwrap()
            .container_statuses
            .as_mut()
            .unwrap()[0]
            .state = Some(from(json!({"waiting": {"reason": "CrashLoopBackOff"}})));
        let config_maps: Vec<PartialObjectMeta<ConfigMap>> = vec![
            from(
                json!({"apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata", "metadata": {"name": "used", "namespace": "shop"}}),
            ),
            from(
                json!({"apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata", "metadata": {"name": "stale", "namespace": "shop"}}),
            ),
            from(
                json!({"apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata", "metadata": {"name": "kube-root-ca.crt", "namespace": "shop"}}),
            ),
            from(
                json!({"apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata", "metadata": {"name": "coredns", "namespace": "kube-system"}}),
            ),
        ];
        let inventory = Inventory {
            namespace: None,
            pods: Ok(vec![healthy, crashing]),
            config_maps: Ok(config_maps),
            deployments: Err("deployments.apps is forbidden".into()),
            ..Default::default()
        };
        let report = report(&inventory);
        let kinds: Vec<(&str, Option<u8>)> = report
            .sections
            .iter()
            .map(|s| (s.kind.as_str(), s.score))
            .collect();
        // Pods: one fine, one failed. ConfigMaps: infos do not cost points, kube-* is skipped.
        assert_eq!(
            kinds,
            vec![("Pod", Some(50)), ("ConfigMap", Some(100)), ("Deployment", None)]
        );
        assert_eq!(report.score, 75);
        assert_eq!(report.grade, 'C');
        let maps = &report.sections[1];
        assert_eq!(maps.total, 3);
        assert_eq!(maps.findings.len(), 1);
        assert_eq!(maps.findings[0].name, "stale");
        assert_eq!(report.sections[0].findings[0].level, Level::Error);
        assert!(
            report.sections[2]
                .failed
                .as_deref()
                .unwrap()
                .contains("forbidden")
        );
    }

    #[test]
    fn bindings_must_point_at_roles_and_accounts_that_exist() {
        let rb: RoleBinding = from(json!({
            "metadata": {"name": "devs", "namespace": "shop"},
            "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "Role", "name": "gone"},
            "subjects": [{"kind": "ServiceAccount", "name": "ghost"}, {"kind": "ServiceAccount", "name": "app"}, {"kind": "User", "name": "bob"}]
        }));
        let roles = HashSet::new();
        let accounts = HashSet::from([("shop".to_owned(), "app".to_owned())]);
        let issues = binding(
            &rb.role_ref,
            rb.subjects.as_deref(),
            "shop",
            Some(&roles),
            None,
            Some(&accounts),
            Some("shop"),
        );
        assert_eq!(
            codes(&issues),
            vec![("role", Level::Error), ("subject", Level::Warning)]
        );
        assert!(issues[1].message.contains("shop/ghost"));
        // Unknown lists check nothing.
        assert_eq!(
            binding(&rb.role_ref, None, "shop", None, None, None, None),
            vec![]
        );

        // Accounts of a namespace that was not scanned are unknown; with all namespaces
        // scanned, an account of a deleted namespace is missing.
        let elsewhere: Vec<Subject> =
            from(json!([{"kind": "ServiceAccount", "name": "ci", "namespace": "gone"}]));
        let check = |scanned| {
            binding(
                &rb.role_ref,
                Some(&elsewhere),
                "shop",
                None,
                None,
                Some(&accounts),
                scanned,
            )
        };
        assert_eq!(check(Some("shop")), vec![]);
        assert_eq!(codes(&check(None)), vec![("subject", Level::Warning)]);
    }

    #[test]
    fn workload_templates_count_as_uses() {
        let meta = |name: &str| -> PartialObjectMeta<ConfigMap> {
            from(
                json!({"apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata",
                "metadata": {"name": name, "namespace": "shop"}}),
            )
        };
        // No job runs right now, but the CronJob uses the ConfigMap on every run.
        let nightly: CronJob = from(json!({
            "metadata": {"name": "report", "namespace": "shop"},
            "spec": {"schedule": "0 3 * * *", "jobTemplate": {"spec": {"template": {"spec": {
                "serviceAccountName": "reporter", "restartPolicy": "OnFailure",
                "containers": [container(json!({"envFrom": [{"configMapRef": {"name": "nightly"}}]}))]}}}}}
        }));
        let account: ServiceAccount = from(json!({"metadata": {"name": "reporter", "namespace": "shop"}}));
        let inventory = Inventory {
            namespace: Some("shop".into()),
            cron_jobs: Ok(vec![nightly]),
            config_maps: Ok(vec![meta("nightly"), meta("stale")]),
            service_accounts: Ok(vec![account]),
            ..Default::default()
        };
        let report = report(&inventory);
        let findings = |kind: &str| -> Vec<String> {
            let section = report.sections.iter().find(|s| s.kind == kind).unwrap();
            section.findings.iter().map(|f| f.name.clone()).collect()
        };
        assert_eq!(findings("ConfigMap"), vec!["stale"]);
        assert_eq!(findings("ServiceAccount"), Vec::<String>::new());
    }

    #[test]
    fn nodes_report_pressure_and_heavy_use() {
        let n: Node = from(json!({
            "metadata": {"name": "worker-1"},
            "spec": {"unschedulable": true},
            "status": {"allocatable": {"cpu": "4", "memory": "8Gi"}, "conditions": [{"type": "Ready", "status": "True"}]}
        }));
        let usage = Usage {
            namespace: None,
            name: "worker-1".into(),
            cpu: 3600.0,
            memory: 1024.0,
        };
        let issues = node(&n, Some(&usage));
        assert_eq!(
            codes(&issues),
            vec![("cordoned", Level::Info), ("usage", Level::Warning)]
        );
        assert!(issues[1].message.contains("90%"));

        let pressured: Node = from(json!({
            "metadata": {"name": "worker-2"},
            "spec": {"unschedulable": true},
            "status": {"conditions": [
                {"type": "Ready", "status": "True"},
                {"type": "MemoryPressure", "status": "True"}
            ]}
        }));
        let issues = node(&pressured, None);
        assert_eq!(
            codes(&issues),
            vec![("status", Level::Warning), ("cordoned", Level::Info)]
        );
        assert_eq!(issues[0].message, "MemoryPressure");
    }
}
