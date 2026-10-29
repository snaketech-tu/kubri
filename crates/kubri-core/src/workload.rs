//! What k9s does with workloads beyond editing them: where Enter leads from an object
//! (`jumps.yaml` first), running a CronJob now, suspending it, setting container images and
//! rolling a Deployment back to an older ReplicaSet.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::k9s::Jump;
use crate::{Error, ObjectRef, ResourceInfo, ResourceRef, Result, TableScope, selector, template};

/// Where Enter leads from an object: a list of what belongs to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drill {
    pub resource: ResourceRef,
    /// `None` lists all namespaces.
    pub namespace: Option<String>,
    pub scope: TableScope,
    /// Keeps only what this UID owns, like the Jobs of a CronJob.
    pub owner_uid: Option<String>,
    /// What the list shows, such as `Pods of Deployment web`.
    pub title: String,
}

/// Kubri's own drill-downs, as k9s has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    /// The pods a workload or Service selects.
    SelectedPods,
    /// The Jobs a CronJob started.
    OwnedJobs,
    /// The pods that run on a node.
    PodsOnNode,
}

impl Builtin {
    pub fn of(resource: &ResourceInfo) -> Option<Self> {
        match (resource.reference.group.as_str(), resource.kind.as_str()) {
            ("apps", "Deployment" | "ReplicaSet" | "StatefulSet" | "DaemonSet")
            | ("batch", "Job")
            | ("", "Service" | "ReplicationController") => Some(Self::SelectedPods),
            ("batch", "CronJob") => Some(Self::OwnedJobs),
            ("", "Node") => Some(Self::PodsOnNode),
            _ => None,
        }
    }

    /// The list to show for `object`. `pods` and `jobs` are the cluster's resources.
    pub fn drill(
        self,
        source: &ResourceInfo,
        target: &ObjectRef,
        object: &Value,
        pods: Option<ResourceRef>,
        jobs: Option<ResourceRef>,
    ) -> Result<Drill> {
        let missing = |what: &str| Error::UnknownResource(what.to_owned());
        let title = |what: &str| format!("{what} of {} {}", source.kind, target.name);
        Ok(match self {
            Self::SelectedPods => Drill {
                resource: pods.ok_or_else(|| missing("v1/pods"))?,
                namespace: target.namespace.clone(),
                scope: TableScope {
                    label_selector: Some(selector::pod_selector(object).ok_or_else(|| {
                        Error::Unsupported(format!("{} {} does not select pods", source.kind, target.name))
                    })?),
                    field_selector: None,
                    paths: None,
                },
                owner_uid: None,
                title: title("Pods"),
            },
            Self::OwnedJobs => Drill {
                resource: jobs.ok_or_else(|| missing("batch/v1/jobs"))?,
                namespace: target.namespace.clone(),
                scope: TableScope::default(),
                owner_uid: Some(text(object, "/metadata/uid").to_owned()).filter(|u| !u.is_empty()),
                title: title("Jobs"),
            },
            Self::PodsOnNode => Drill {
                resource: pods.ok_or_else(|| missing("v1/pods"))?,
                namespace: None,
                scope: TableScope {
                    label_selector: None,
                    field_selector: Some(format!("spec.nodeName={}", target.name)),
                    paths: None,
                },
                owner_uid: None,
                title: format!("Pods on {}", target.name),
            },
        })
    }
}

/// The `jumps.yaml` rule for `resource`, keyed like k9s by `group/version/resource`
/// (`v1/pods` for core types). Another version of the same resource counts too.
pub(crate) fn find_jump<'a>(jumps: &'a [Jump], resource: &ResourceRef) -> Option<&'a Jump> {
    let exact = gvr(resource);
    jumps.iter().find(|j| j.source == exact).or_else(|| {
        jumps.iter().find(|j| {
            let (group, _, plural) = split_gvr(&j.source);
            group == resource.group && plural == resource.plural
        })
    })
}

fn gvr(resource: &ResourceRef) -> String {
    if resource.group.is_empty() {
        format!("{}/{}", resource.version, resource.plural)
    } else {
        format!("{}/{}/{}", resource.group, resource.version, resource.plural)
    }
}

/// `group/version/resource`, `version/resource` or a bare resource name, as k9s writes them.
pub(crate) fn split_gvr(text: &str) -> (&str, &str, &str) {
    let parts: Vec<&str> = text.trim().split('/').collect();
    match parts.as_slice() {
        [group, version, plural] => (group, version, plural),
        [version, plural] => ("", version, plural),
        _ => ("", "", text.trim()),
    }
}

/// Follows a `jumps.yaml` rule from `object`. `resolve` finds the target resource.
pub(crate) fn jump(
    rule: &Jump,
    source: &ResourceInfo,
    target: &ObjectRef,
    object: &Value,
    resolve: impl Fn(&str) -> Option<ResourceInfo>,
) -> Result<Drill> {
    let invalid =
        |what: &str, err: String| Error::Unsupported(format!("jumps.yaml, {}: {what}: {err}", rule.source));
    let to = resolve(&rule.target)
        .ok_or_else(|| invalid("targetGVR", format!("the cluster does not serve {}", rule.target)))?;
    let render = |what: &str, text: &str| -> Result<Option<String>> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        template::render(text, object)
            .map(Some)
            .map_err(|e| invalid(what, e))
    };
    let scope = TableScope {
        label_selector: render("labelSelector", &rule.label_selector)?,
        field_selector: render("fieldSelector", &rule.field_selector)?,
        paths: None,
    };
    if scope.label_selector.is_none() && scope.field_selector.is_none() {
        return Err(invalid("selectors", "set labelSelector or fieldSelector".into()));
    }
    let namespace = match rule.target_namespace.trim() {
        "" => target.namespace.clone(),
        "all" => None,
        other => render("targetNamespace", other)?,
    };
    Ok(Drill {
        namespace: namespace.filter(|_| to.namespaced),
        resource: to.reference.clone(),
        scope,
        owner_uid: None,
        title: format!("{} of {} {}", plural_kind(&to), source.kind, target.name),
    })
}

/// The kind in the plural, as the resource name spells it: `Pods`, `NetworkPolicies`. Falls
/// back to the kind when the name is not a form of it.
fn plural_kind(resource: &ResourceInfo) -> String {
    let kind = resource.kind.as_str();
    let plural = resource.reference.plural.as_str();
    let stem = kind.len().saturating_sub(1);
    match (kind.get(..stem), plural.get(..stem), plural.get(stem..)) {
        (Some(a), Some(b), Some(ending)) if stem > 0 && a.eq_ignore_ascii_case(b) && plural.len() > stem => {
            format!("{a}{ending}")
        }
        _ => kind.to_owned(),
    }
}

fn text<'a>(object: &'a Value, pointer: &str) -> &'a str {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// A Job from a CronJob's template, as `kubectl create job --from=cronjob/…` makes it. The
/// API server adds a random suffix to the name.
pub(crate) fn job_from_cronjob(cronjob: &Value) -> Result<Value> {
    let template = cronjob
        .pointer("/spec/jobTemplate")
        .ok_or_else(|| Error::Unsupported("the CronJob has no jobTemplate".into()))?;
    let name = text(cronjob, "/metadata/name");
    // Job names end up in pod labels, which allow 63 characters.
    let prefix: String = name.chars().take(42).collect();
    let mut annotations = template
        .pointer("/metadata/annotations")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    annotations.insert("cronjob.kubernetes.io/instantiate".into(), "manual".into());
    Ok(json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "generateName": format!("{prefix}-manual-"),
            "namespace": text(cronjob, "/metadata/namespace"),
            "labels": template.pointer("/metadata/labels").cloned().unwrap_or_else(|| json!({})),
            "annotations": annotations,
            "ownerReferences": [{
                "apiVersion": cronjob.get("apiVersion").cloned().unwrap_or_else(|| "batch/v1".into()),
                "kind": "CronJob",
                "name": name,
                "uid": text(cronjob, "/metadata/uid"),
                "controller": true,
                "blockOwnerDeletion": true
            }]
        },
        "spec": template.get("spec").cloned().unwrap_or_else(|| json!({}))
    }))
}

/// A new image for one container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerImage {
    pub name: String,
    pub image: String,
    /// An init container rather than a regular one.
    #[serde(default)]
    pub init: bool,
}

/// Kinds whose images Kubri sets, as k9s does.
pub(crate) fn has_images(resource: &ResourceInfo) -> bool {
    matches!(
        (resource.reference.group.as_str(), resource.kind.as_str()),
        ("", "Pod") | ("apps", "Deployment" | "StatefulSet" | "DaemonSet")
    )
}

/// A strategic merge patch setting the images, which merges containers by name like
/// `kubectl set image`.
pub(crate) fn image_patch(pod: bool, images: &[ContainerImage]) -> Result<Value> {
    if images.is_empty() {
        return Err(Error::Unsupported("no image to set".into()));
    }
    let mut containers = Vec::new();
    let mut init = Vec::new();
    for c in images {
        let image = c.image.trim();
        if image.is_empty() || image.contains(char::is_whitespace) {
            return Err(Error::Unsupported(format!("`{}` is not an image", c.image)));
        }
        let entry = json!({ "name": c.name, "image": image });
        if c.init {
            init.push(entry)
        } else {
            containers.push(entry)
        }
    }
    let mut spec = Map::new();
    if !containers.is_empty() {
        spec.insert("containers".into(), containers.into());
    }
    if !init.is_empty() {
        spec.insert("initContainers".into(), init.into());
    }
    Ok(if pod {
        json!({ "spec": spec })
    } else {
        json!({ "spec": { "template": { "spec": spec } } })
    })
}

/// The Deployment that controls a ReplicaSet.
pub(crate) fn deployment_of(replicaset: &Value) -> Result<String> {
    replicaset
        .pointer("/metadata/ownerReferences")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|o| {
            o.get("kind").and_then(Value::as_str) == Some("Deployment")
                && o.get("controller") == Some(&json!(true))
        })
        .and_then(|o| o.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| {
            Error::Unsupported(format!(
                "ReplicaSet {} belongs to no Deployment",
                text(replicaset, "/metadata/name")
            ))
        })
}

/// Annotations a rollback keeps from the Deployment instead of taking them from the
/// ReplicaSet, as in `kubectl rollout undo`.
const DEPLOYMENT_ANNOTATIONS: [&str; 6] = [
    "kubectl.kubernetes.io/last-applied-configuration",
    "deployment.kubernetes.io/revision",
    "deployment.kubernetes.io/revision-history",
    "deployment.kubernetes.io/desired-replicas",
    "deployment.kubernetes.io/max-replicas",
    "deprecated.deployment.rollback.to",
];

/// The Deployment with the pod template of `replicaset`, like `kubectl rollout undo
/// --to-revision`: the template without its hash label, and the ReplicaSet's annotations
/// (such as `kubernetes.io/change-cause`).
pub(crate) fn rolled_back(mut deployment: Value, replicaset: &Value) -> Result<Value> {
    let name = text(&deployment, "/metadata/name").to_owned();
    if deployment.pointer("/spec/paused") == Some(&json!(true)) {
        return Err(Error::Unsupported(format!(
            "Deployment {name} is paused; resume it before rolling back"
        )));
    }
    let mut template = replicaset
        .pointer("/spec/template")
        .cloned()
        .ok_or_else(|| Error::Unsupported("the ReplicaSet has no pod template".into()))?;
    if let Some(labels) = template
        .pointer_mut("/metadata/labels")
        .and_then(Value::as_object_mut)
    {
        labels.remove("pod-template-hash");
    }
    let mut current = deployment.pointer("/spec/template").cloned().unwrap_or_default();
    if let Some(labels) = current
        .pointer_mut("/metadata/labels")
        .and_then(Value::as_object_mut)
    {
        labels.remove("pod-template-hash");
    }
    if current == template {
        return Err(Error::Unsupported(format!(
            "Deployment {name} already runs the template of ReplicaSet {}",
            text(replicaset, "/metadata/name")
        )));
    }
    let annotations = |object: &Value| {
        object
            .pointer("/metadata/annotations")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    let mut merged: Map<String, Value> = annotations(&deployment)
        .into_iter()
        .filter(|(k, _)| DEPLOYMENT_ANNOTATIONS.contains(&k.as_str()))
        .collect();
    merged.extend(
        annotations(replicaset)
            .into_iter()
            .filter(|(k, _)| !DEPLOYMENT_ANNOTATIONS.contains(&k.as_str())),
    );
    deployment["spec"]["template"] = template;
    deployment["metadata"]["annotations"] = merged.into();
    Ok(deployment)
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_kinds_in_the_plural() {
        let named = |kind: &str, plural: &str| super::plural_kind(&info("", kind, plural, true));
        assert_eq!(named("Pod", "pods"), "Pods");
        assert_eq!(named("NetworkPolicy", "networkpolicies"), "NetworkPolicies");
        assert_eq!(named("Ingress", "ingresses"), "Ingresses");
        assert_eq!(named("Endpoints", "endpoints"), "Endpoints");
        assert_eq!(named("Thing", "stuff"), "Thing");
    }

    use super::*;

    fn info(group: &str, kind: &str, plural: &str, namespaced: bool) -> ResourceInfo {
        ResourceInfo {
            reference: ResourceRef {
                group: group.into(),
                version: "v1".into(),
                plural: plural.into(),
            },
            kind: kind.into(),
            singular: kind.to_lowercase(),
            short_names: Vec::new(),
            namespaced,
            verbs: Vec::new(),
            subresources: Vec::new(),
        }
    }

    fn target(resource: &ResourceInfo, namespace: Option<&str>, name: &str) -> ObjectRef {
        ObjectRef {
            context: None,
            resource: resource.reference.clone(),
            namespace: namespace.map(str::to_owned),
            name: name.into(),
        }
    }

    #[test]
    fn workloads_lead_to_their_pods() {
        let deploy = info("apps", "Deployment", "deployments", true);
        let pods = info("", "Pod", "pods", true).reference;
        let object = json!({ "spec": { "selector": { "matchLabels": { "app": "web" } } } });
        let drill = Builtin::of(&deploy)
            .unwrap()
            .drill(
                &deploy,
                &target(&deploy, Some("shop"), "web"),
                &object,
                Some(pods.clone()),
                None,
            )
            .unwrap();
        assert_eq!(drill.resource, pods);
        assert_eq!(drill.namespace.as_deref(), Some("shop"));
        assert_eq!(drill.scope.label_selector.as_deref(), Some("app=web"));
        assert_eq!(drill.title, "Pods of Deployment web");

        let svc = info("", "Service", "services", true);
        let err = Builtin::of(&svc)
            .unwrap()
            .drill(
                &svc,
                &target(&svc, Some("shop"), "db"),
                &json!({ "spec": {} }),
                Some(pods),
                None,
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "Service db does not select pods");
        assert_eq!(Builtin::of(&info("", "ConfigMap", "configmaps", true)), None);
        // Another group's Deployment is not the built-in one.
        assert_eq!(
            Builtin::of(&info("example.com", "Deployment", "deployments", true)),
            None
        );
    }

    #[test]
    fn cronjobs_lead_to_their_jobs_and_nodes_to_their_pods() {
        let cronjob = info("batch", "CronJob", "cronjobs", true);
        let jobs = info("batch", "Job", "jobs", true).reference;
        let drill = Builtin::of(&cronjob)
            .unwrap()
            .drill(
                &cronjob,
                &target(&cronjob, Some("ops"), "backup"),
                &json!({ "metadata": { "uid": "cj-1" } }),
                None,
                Some(jobs.clone()),
            )
            .unwrap();
        assert_eq!((drill.resource, drill.owner_uid.as_deref()), (jobs, Some("cj-1")));

        let node = info("", "Node", "nodes", false);
        let pods = info("", "Pod", "pods", true).reference;
        let drill = Builtin::of(&node)
            .unwrap()
            .drill(
                &node,
                &target(&node, None, "worker-1"),
                &json!({}),
                Some(pods),
                None,
            )
            .unwrap();
        assert_eq!(drill.namespace, None);
        assert_eq!(
            drill.scope.field_selector.as_deref(),
            Some("spec.nodeName=worker-1")
        );
        assert_eq!(drill.title, "Pods on worker-1");
    }

    fn rule(source: &str, target: &str, labels: &str, namespace: &str) -> Jump {
        Jump {
            source: source.into(),
            target: target.into(),
            label_selector: labels.into(),
            field_selector: String::new(),
            target_namespace: namespace.into(),
        }
    }

    #[test]
    fn jumps_match_by_resource_and_fill_in_templates() {
        let patch = info("example.com", "Patch", "patches", true);
        let jobs = info("example.com", "PatchJob", "patchjobs", true);
        let rules = vec![
            rule("v1/pods", "v1/configmaps", "app=x", ""),
            rule(
                "example.com/v1beta1/patches",
                "example.com/v1/patchjobs",
                "patch={{.metadata.name}}",
                "",
            ),
        ];
        // The rule names v1beta1; the cluster serves v1.
        let found = find_jump(&rules, &patch.reference).unwrap();
        assert_eq!(found.target, "example.com/v1/patchjobs");
        let object = json!({ "metadata": { "name": "p1", "namespace": "ops" } });
        let resolve = |path: &str| (path == "example.com/v1/patchjobs").then(|| jobs.clone());
        let drill = jump(
            found,
            &patch,
            &target(&patch, Some("ops"), "p1"),
            &object,
            resolve,
        )
        .unwrap();
        assert_eq!(drill.resource, jobs.reference);
        assert_eq!(drill.namespace.as_deref(), Some("ops"));
        assert_eq!(drill.scope.label_selector.as_deref(), Some("patch=p1"));
        assert_eq!(drill.title, "PatchJobs of Patch p1");

        let all = rule("x", "example.com/v1/patchjobs", "a=b", "all");
        let drill = jump(&all, &patch, &target(&patch, Some("ops"), "p1"), &object, resolve).unwrap();
        assert_eq!(drill.namespace, None);
        let templated = rule(
            "x",
            "example.com/v1/patchjobs",
            "a=b",
            "{{.metadata.labels.team}}",
        );
        let err = jump(
            &templated,
            &patch,
            &target(&patch, Some("ops"), "p1"),
            &object,
            resolve,
        )
        .unwrap_err();
        assert!(err.to_string().contains("targetNamespace"), "{err}");
        let nowhere = rule("x", "example.com/v1/nothing", "a=b", "");
        assert!(
            jump(
                &nowhere,
                &patch,
                &target(&patch, Some("ops"), "p1"),
                &object,
                resolve
            )
            .is_err()
        );
        let unselective = rule("x", "example.com/v1/patchjobs", "", "");
        assert!(
            jump(
                &unselective,
                &patch,
                &target(&patch, Some("ops"), "p1"),
                &object,
                resolve
            )
            .is_err()
        );
        assert_eq!(split_gvr("apps/v1/deployments"), ("apps", "v1", "deployments"));
        assert_eq!(split_gvr("v1/pods"), ("", "v1", "pods"));
        assert_eq!(split_gvr("pods"), ("", "", "pods"));
    }

    #[test]
    fn jobs_from_cronjobs_point_back_to_them() {
        let cronjob = json!({
            "apiVersion": "batch/v1",
            "kind": "CronJob",
            "metadata": { "name": "nightly-backup", "namespace": "ops", "uid": "cj-1" },
            "spec": { "schedule": "0 3 * * *", "jobTemplate": {
                "metadata": { "labels": { "app": "backup" } },
                "spec": { "template": { "spec": { "restartPolicy": "Never", "containers": [{ "name": "b", "image": "backup:1" }] } } }
            } }
        });
        let job = job_from_cronjob(&cronjob).unwrap();
        assert_eq!(job["metadata"]["generateName"], "nightly-backup-manual-");
        assert_eq!(job["metadata"]["namespace"], "ops");
        assert_eq!(job["metadata"]["labels"]["app"], "backup");
        assert_eq!(
            job["metadata"]["annotations"]["cronjob.kubernetes.io/instantiate"],
            "manual"
        );
        assert_eq!(job["metadata"]["ownerReferences"][0]["uid"], "cj-1");
        assert_eq!(
            job["spec"]["template"]["spec"]["containers"][0]["image"],
            "backup:1"
        );

        let long =
            json!({ "metadata": { "name": "x".repeat(60) }, "spec": { "jobTemplate": { "spec": {} } } });
        let name = job_from_cronjob(&long).unwrap()["metadata"]["generateName"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(name.len(), 42 + "-manual-".len());
        assert!(job_from_cronjob(&json!({ "spec": {} })).is_err());
    }

    #[test]
    fn image_patches_merge_by_container_name() {
        let images = [
            ContainerImage {
                name: "app".into(),
                image: "web:2".into(),
                init: false,
            },
            ContainerImage {
                name: "migrate".into(),
                image: " web:2 ".into(),
                init: true,
            },
        ];
        assert_eq!(
            image_patch(false, &images).unwrap(),
            json!({ "spec": { "template": { "spec": {
                "containers": [{ "name": "app", "image": "web:2" }],
                "initContainers": [{ "name": "migrate", "image": "web:2" }]
            } } } })
        );
        assert_eq!(
            image_patch(true, &images[..1]).unwrap(),
            json!({ "spec": { "containers": [{ "name": "app", "image": "web:2" }] } })
        );
        let bad = ContainerImage {
            name: "app".into(),
            image: "web 2".into(),
            init: false,
        };
        assert!(image_patch(true, &[bad]).is_err());
        assert!(image_patch(true, &[]).is_err());
    }

    #[test]
    fn rollbacks_take_the_replicasets_template() {
        let deployment = json!({
            "metadata": { "name": "web", "resourceVersion": "7", "annotations": {
                "deployment.kubernetes.io/revision": "3",
                "kubernetes.io/change-cause": "image web:3"
            } },
            "spec": { "template": {
                "metadata": { "labels": { "app": "web" } },
                "spec": { "containers": [{ "name": "app", "image": "web:3" }] }
            } }
        });
        let old = json!({
            "metadata": { "name": "web-5d4c", "annotations": {
                "deployment.kubernetes.io/revision": "1",
                "kubernetes.io/change-cause": "image web:1"
            }, "ownerReferences": [{ "kind": "Deployment", "name": "web", "controller": true }] },
            "spec": { "template": {
                "metadata": { "labels": { "app": "web", "pod-template-hash": "5d4c" } },
                "spec": { "containers": [{ "name": "app", "image": "web:1" }] }
            } }
        });
        assert_eq!(deployment_of(&old).unwrap(), "web");
        let rolled = rolled_back(deployment.clone(), &old).unwrap();
        assert_eq!(
            rolled["spec"]["template"]["spec"]["containers"][0]["image"],
            "web:1"
        );
        assert_eq!(
            rolled["spec"]["template"]["metadata"]["labels"],
            json!({ "app": "web" })
        );
        assert_eq!(rolled["metadata"]["resourceVersion"], "7");
        assert_eq!(
            rolled["metadata"]["annotations"],
            json!({ "deployment.kubernetes.io/revision": "3", "kubernetes.io/change-cause": "image web:1" })
        );

        // The current ReplicaSet, and a paused Deployment, are turned down.
        let mut current = old.clone();
        current["spec"]["template"]["spec"]["containers"][0]["image"] = "web:3".into();
        assert!(
            rolled_back(deployment.clone(), &current)
                .unwrap_err()
                .to_string()
                .contains("already runs")
        );
        let mut paused = deployment;
        paused["spec"]["paused"] = true.into();
        assert!(
            rolled_back(paused, &old)
                .unwrap_err()
                .to_string()
                .contains("paused")
        );
        assert!(deployment_of(&json!({ "metadata": { "name": "lonely" } })).is_err());
    }
}
