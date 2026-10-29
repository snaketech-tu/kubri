//! Creating and changing objects from YAML, as `kubectl apply --server-side` does: several
//! objects in dependency order, each as a server-side dry run first for a diff.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use kube::api::{ApiResource, DynamicObject, Patch, PatchParams, PostParams};
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Guard, Kubri, ObjectRef, ResourceRef, Result, Session, discovery, preview};

/// The field manager Kubri applies as, so later applies change what it set before.
const MANAGER: &str = "kubri";

/// Objects to create or change, in any order.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub objects: Vec<Value>,
    /// For namespaced objects that name none.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Only a server-side dry run: nothing changes.
    #[serde(default)]
    pub dry_run: bool,
    /// Refuse to apply to another context than the one the YAML was written for.
    #[serde(default)]
    pub context: Option<String>,
    /// Apply in this context instead of the connected one, as when cloning to another
    /// cluster. Its read-only and locked settings apply.
    #[serde(default)]
    pub into: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplyOutcome {
    Created,
    Changed,
    Unchanged,
    /// Depends on an object the dry run only pretends to create, such as a new namespace.
    Unchecked,
    Failed,
}

/// What happened, or would happen, to one object.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyItem {
    /// Its place in the request.
    pub index: usize,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub target: Option<ObjectRef>,
    pub outcome: ApplyOutcome,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub message: Option<String>,
}

/// Helm's install order: what others need comes first; kinds it does not know come last.
const ORDER: &[&str] = &[
    "Namespace",
    "NetworkPolicy",
    "ResourceQuota",
    "LimitRange",
    "PodSecurityPolicy",
    "PodDisruptionBudget",
    "ServiceAccount",
    "Secret",
    "ConfigMap",
    "StorageClass",
    "PersistentVolume",
    "PersistentVolumeClaim",
    "CustomResourceDefinition",
    "ClusterRole",
    "ClusterRoleBinding",
    "Role",
    "RoleBinding",
    "Service",
    "DaemonSet",
    "Pod",
    "ReplicationController",
    "ReplicaSet",
    "Deployment",
    "HorizontalPodAutoscaler",
    "StatefulSet",
    "Job",
    "CronJob",
    "IngressClass",
    "Ingress",
    "APIService",
];

fn rank(kind: &str) -> usize {
    ORDER.iter().position(|k| *k == kind).unwrap_or(ORDER.len())
}

/// The order to apply `objects` in, as indexes; the same kind keeps the order it came in.
pub(crate) fn order(objects: &[Value]) -> Vec<usize> {
    let mut indexes: Vec<usize> = (0..objects.len()).collect();
    indexes.sort_by_key(|&i| rank(objects[i]["kind"].as_str().unwrap_or("")));
    indexes
}

/// The parts of an object applying needs, or what is missing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Head {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    /// Set instead of a name: the object is created under a generated name.
    pub generate_name: bool,
    pub namespace: Option<String>,
}

pub(crate) fn head(object: &Value) -> std::result::Result<Head, String> {
    if !object.is_object() {
        return Err("is not a mapping".into());
    }
    let text = |v: &Value| {
        v.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    let api_version = text(&object["apiVersion"]).ok_or("has no apiVersion")?;
    let kind = text(&object["kind"]).ok_or("has no kind")?;
    let meta = &object["metadata"];
    let (name, generate_name) = match (text(&meta["name"]), text(&meta["generateName"])) {
        (Some(name), _) => (name, false),
        (None, Some(prefix)) => (prefix, true),
        (None, None) => return Err("has no metadata.name".into()),
    };
    Ok(Head {
        api_version,
        kind,
        name,
        generate_name,
        namespace: text(&meta["namespace"]),
    })
}

/// Created when there was nothing, unchanged when the dry run or apply changed nothing that
/// `kubectl diff` would show.
pub(crate) fn outcome(before: Option<&Value>, after: &Value) -> ApplyOutcome {
    match before {
        None => ApplyOutcome::Created,
        Some(before) if preview::comparable(before.clone()) == preview::comparable(after.clone()) => {
            ApplyOutcome::Unchanged
        }
        Some(_) => ApplyOutcome::Changed,
    }
}

/// A kind served by a CustomResourceDefinition in the same request, before it exists.
#[derive(Debug, Clone)]
struct Defined {
    api_resource: ApiResource,
    namespaced: bool,
    crd: String,
}

/// The kinds the CustomResourceDefinitions among `objects` serve, one per served version.
fn defined_kinds(objects: &[Value]) -> Vec<Defined> {
    let mut out = Vec::new();
    for crd in objects.iter().filter(|o| o["kind"] == "CustomResourceDefinition") {
        let spec = &crd["spec"];
        let (Some(group), Some(kind), Some(plural)) = (
            spec["group"].as_str(),
            spec["names"]["kind"].as_str(),
            spec["names"]["plural"].as_str(),
        ) else {
            continue;
        };
        for version in spec["versions"].as_array().into_iter().flatten() {
            let Some(v) = version["name"].as_str() else {
                continue;
            };
            out.push(Defined {
                api_resource: ApiResource {
                    group: group.into(),
                    version: v.into(),
                    api_version: format!("{group}/{v}"),
                    kind: kind.into(),
                    plural: plural.into(),
                },
                namespaced: spec["scope"] != "Cluster",
                crd: crd["metadata"]["name"].as_str().unwrap_or_default().into(),
            });
        }
    }
    out
}

/// Waits until the API server serves a new CustomResourceDefinition, as `kubectl wait
/// --for condition=established` does.
async fn established(client: &Client, name: &str) -> bool {
    let crds: Api<DynamicObject> = Api::all_with(
        client.clone(),
        &ApiResource {
            group: "apiextensions.k8s.io".into(),
            version: "v1".into(),
            api_version: "apiextensions.k8s.io/v1".into(),
            kind: "CustomResourceDefinition".into(),
            plural: "customresourcedefinitions".into(),
        },
    );
    for _ in 0..60 {
        if let Ok(crd) = crds.get(name).await {
            let ready = crd.data["status"]["conditions"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| c["type"] == "Established" && c["status"] == "True");
            if ready {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

impl Kubri {
    /// Applies `request.objects` in dependency order, or with `dry_run` shows what would
    /// change. Each object gets its own result; one that fails does not stop the others.
    pub async fn apply(&self, request: ApplyRequest) -> Result<Vec<ApplyItem>> {
        let current = self.cluster().await.map(|c| c.context);
        let elsewhere = request
            .into
            .as_ref()
            .filter(|into| Some(*into) != current.as_ref());
        if !request.dry_run {
            match elsewhere {
                Some(into) => {
                    if let Guard {
                        read_only: true,
                        reason,
                        ..
                    } = self.guard_for(Some(into)).await
                    {
                        return Err(Error::ReadOnly(format!("{into}: {}", reason.unwrap_or_default())));
                    }
                }
                None => self.ensure_writable().await?,
            }
        }
        let session = self.session().await?;
        if let Some(context) = &request.context
            && context != &session.info.context
        {
            return Err(Error::ContextChanged(context.clone()));
        }
        let session = match elsewhere {
            Some(into) => Arc::new(self.connect(into).await?),
            None => session,
        };
        let default_namespace = request
            .namespace
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| session.info.default_namespace.clone());
        let defined = defined_kinds(&request.objects);
        // Namespaces the dry run pretends to create: objects in them cannot be checked.
        let mut pretended: Vec<String> = Vec::new();
        let mut items = Vec::with_capacity(request.objects.len());
        for index in order(&request.objects) {
            let mut object = request.objects[index].clone();
            let head = match head(&object) {
                Ok(head) => head,
                Err(message) => {
                    items.push(ApplyItem {
                        index,
                        kind: object["kind"].as_str().unwrap_or("Object").into(),
                        name: String::new(),
                        namespace: None,
                        target: None,
                        outcome: ApplyOutcome::Failed,
                        before: None,
                        after: None,
                        message: Some(format!("Document {} {message}", index + 1)),
                    });
                    continue;
                }
            };
            let mut item = ApplyItem {
                index,
                kind: head.kind.clone(),
                name: head.name.clone(),
                namespace: None,
                target: None,
                outcome: ApplyOutcome::Failed,
                before: None,
                after: None,
                message: None,
            };
            // A kind the cluster serves, or one a CustomResourceDefinition in this request adds.
            let served = session.resolve(&head.api_version, &head.kind).and_then(|r| {
                session
                    .info_for(&r)
                    .ok()
                    .map(|i| (i.api_resource(), i.namespaced))
            });
            let (api_resource, namespaced) = match served {
                Some(found) => found,
                None => match defined.iter().find(|d| {
                    d.api_resource.api_version == head.api_version && d.api_resource.kind == head.kind
                }) {
                    Some(d) if request.dry_run => {
                        item.namespace = d.namespaced.then(|| {
                            head.namespace
                                .clone()
                                .unwrap_or_else(|| default_namespace.clone())
                        });
                        item.outcome = ApplyOutcome::Unchecked;
                        item.after = Some(object);
                        item.message = Some(format!(
                            "Checked once the CustomResourceDefinition {} exists",
                            d.crd
                        ));
                        items.push(item);
                        continue;
                    }
                    Some(d) => {
                        if !established(&session.client, &d.crd).await {
                            item.message = Some(format!(
                                "The CustomResourceDefinition {} was not ready in time; apply again",
                                d.crd
                            ));
                            items.push(item);
                            continue;
                        }
                        (d.api_resource.clone(), d.namespaced)
                    }
                    None => {
                        item.message = Some(format!(
                            "The cluster does not serve {} {}",
                            head.api_version, head.kind
                        ));
                        items.push(item);
                        continue;
                    }
                },
            };
            let namespace = namespaced.then(|| {
                head.namespace
                    .clone()
                    .unwrap_or_else(|| default_namespace.clone())
            });
            if let Some(meta) = object["metadata"].as_object_mut() {
                match &namespace {
                    Some(ns) => meta.insert("namespace".into(), Value::String(ns.clone())),
                    None => meta.remove("namespace"),
                };
            }
            item.namespace = namespace.clone();
            let target = ObjectRef {
                context: Some(session.info.context.clone()),
                resource: ResourceRef {
                    group: api_resource.group.clone(),
                    version: api_resource.version.clone(),
                    plural: api_resource.plural.clone(),
                },
                namespace: namespace.clone(),
                name: head.name.clone(),
            };
            if request.dry_run
                && let Some(ns) = &namespace
                && pretended.contains(ns)
            {
                item.outcome = ApplyOutcome::Unchecked;
                item.after = Some(object);
                item.message = Some(format!("Checked once the namespace {ns} exists"));
                items.push(item);
                continue;
            }
            let api: Api<DynamicObject> = match &namespace {
                Some(ns) => Api::namespaced_with(session.client.clone(), ns, &api_resource),
                None => Api::all_with(session.client.clone(), &api_resource),
            };
            let result = async {
                let before = if head.generate_name {
                    None
                } else {
                    api.get_opt(&head.name).await?
                };
                let check = ObjectRef {
                    name: if head.generate_name {
                        String::new()
                    } else {
                        head.name.clone()
                    },
                    ..target.clone()
                };
                session
                    .ensure_may(if before.is_some() { "patch" } else { "create" }, &check, "")
                    .await?;
                let value: DynamicObject = serde_json::from_value(object.clone())?;
                let after = if head.generate_name {
                    let post = PostParams {
                        dry_run: request.dry_run,
                        field_manager: Some(MANAGER.into()),
                    };
                    api.create(&post, &value).await?
                } else {
                    let params = PatchParams {
                        dry_run: request.dry_run,
                        force: true,
                        field_manager: Some(MANAGER.into()),
                        ..Default::default()
                    };
                    api.patch(&head.name, &params, &Patch::Apply(&value)).await?
                };
                Ok::<_, Error>((
                    before.map(serde_json::to_value).transpose()?,
                    serde_json::to_value(after)?,
                ))
            }
            .await;
            match result {
                Ok((before, after)) => {
                    item.outcome = outcome(before.as_ref(), &after);
                    if item.outcome == ApplyOutcome::Created && head.kind == "Namespace" && request.dry_run {
                        pretended.push(head.name.clone());
                    }
                    if let Some(name) = after["metadata"]["name"].as_str() {
                        item.name = name.into();
                    }
                    item.target = Some(ObjectRef {
                        name: item.name.clone(),
                        ..target
                    });
                    item.before = before.map(preview::comparable);
                    item.after = Some(preview::comparable(after));
                }
                Err(err) => item.message = Some(err.to_string()),
            }
            items.push(item);
        }
        let defined = items.iter().any(|i| {
            i.kind == "CustomResourceDefinition"
                && matches!(i.outcome, ApplyOutcome::Created | ApplyOutcome::Changed)
        });
        if defined && !request.dry_run && elsewhere.is_none() {
            self.rediscover(&session).await;
        }
        Ok(items)
    }

    /// Learns the kinds a new CustomResourceDefinition added, unless the context changed
    /// meanwhile. Lists that are open keep their connection.
    async fn rediscover(&self, session: &Arc<Session>) {
        let Ok(resources) = discovery::discover(&session.client).await else {
            return;
        };
        let mut current = self.session.write().await;
        if current.as_ref().is_some_and(|c| Arc::ptr_eq(c, session)) {
            *current = Some(Arc::new(Session::new(
                session.info.clone(),
                session.client.clone(),
                resources,
            )));
        }
    }
}

/// The largest manifest Kubri reads from a URL.
const MAX_MANIFEST: usize = 5 << 20;

static HTTP: LazyLock<std::result::Result<reqwest::Client, String>> = LazyLock::new(|| {
    // kube brings ring; reqwest uses whatever provider the process has.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
});

/// GitHub's page of a file, as people copy it from the browser, means the file itself.
pub(crate) fn raw_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://github.com/")
        && let Some((repo, path)) = rest.split_once("/blob/")
    {
        return format!("https://raw.githubusercontent.com/{repo}/{path}");
    }
    url.to_owned()
}

/// Reads YAML from an http(s) address, as `kubectl apply -f https://…` does.
pub async fn fetch_manifest(url: &str) -> Result<String> {
    let url = raw_url(url.trim());
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(Error::Unsupported(
            "Give an address that starts with https://".into(),
        ));
    }
    let client = HTTP.as_ref().map_err(|e| Error::Unsupported(e.clone()))?;
    let fail = |e: reqwest::Error| Error::Unsupported(format!("Could not read {url}: {e}"));
    let response = client.get(&url).send().await.map_err(fail)?;
    if !response.status().is_success() {
        return Err(Error::Unsupported(format!(
            "{url} answered {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|n| n as usize > MAX_MANIFEST)
    {
        return Err(Error::Unsupported(format!("{url} is larger than 5 MB")));
    }
    let bytes = response.bytes().await.map_err(fail)?;
    if bytes.len() > MAX_MANIFEST {
        return Err(Error::Unsupported(format!("{url} is larger than 5 MB")));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| Error::Unsupported(format!("{url} is not text")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_github_pages_as_files() {
        assert_eq!(
            raw_url("https://github.com/org/repo/blob/main/deploy/app.yaml"),
            "https://raw.githubusercontent.com/org/repo/main/deploy/app.yaml"
        );
        assert_eq!(
            raw_url("https://example.com/a.yaml"),
            "https://example.com/a.yaml"
        );
    }

    #[test]
    fn orders_what_others_need_first() {
        let objects = vec![
            json!({ "kind": "Deployment" }),
            json!({ "kind": "Widget" }),
            json!({ "kind": "Service" }),
            json!({ "kind": "Namespace" }),
            json!({ "kind": "ConfigMap" }),
            json!({ "kind": "CustomResourceDefinition" }),
            json!({ "kind": "Service" }),
        ];
        assert_eq!(order(&objects), vec![3, 4, 5, 2, 6, 0, 1]);
    }

    #[test]
    fn reads_what_applying_needs() {
        let ok = head(
            &json!({ "apiVersion": "v1", "kind": "ConfigMap", "metadata": { "name": "a", "namespace": "shop" } }),
        );
        assert_eq!(
            ok.unwrap(),
            Head {
                api_version: "v1".into(),
                kind: "ConfigMap".into(),
                name: "a".into(),
                generate_name: false,
                namespace: Some("shop".into()),
            }
        );
        let generated =
            head(&json!({ "apiVersion": "batch/v1", "kind": "Job", "metadata": { "generateName": "run-" } }));
        assert!(generated.unwrap().generate_name);
        assert_eq!(head(&json!({ "kind": "Pod" })).unwrap_err(), "has no apiVersion");
        assert_eq!(
            head(&json!({ "apiVersion": "v1", "kind": "Pod", "metadata": {} })).unwrap_err(),
            "has no metadata.name"
        );
        assert_eq!(head(&json!("text")).unwrap_err(), "is not a mapping");
    }

    #[test]
    fn tells_created_changed_and_unchanged() {
        let before = json!({ "metadata": { "name": "a", "resourceVersion": "1" }, "data": { "k": "v" } });
        let same = json!({ "metadata": { "name": "a", "resourceVersion": "2", "managedFields": [{}] }, "data": { "k": "v" } });
        let changed = json!({ "metadata": { "name": "a" }, "data": { "k": "w" } });
        assert_eq!(outcome(None, &same), ApplyOutcome::Created);
        assert_eq!(outcome(Some(&before), &same), ApplyOutcome::Unchanged);
        assert_eq!(outcome(Some(&before), &changed), ApplyOutcome::Changed);
    }

    #[test]
    fn finds_kinds_a_definition_adds() {
        let objects = vec![json!({
            "apiVersion": "apiextensions.k8s.io/v1",
            "kind": "CustomResourceDefinition",
            "metadata": { "name": "widgets.example.com" },
            "spec": {
                "group": "example.com",
                "scope": "Namespaced",
                "names": { "kind": "Widget", "plural": "widgets" },
                "versions": [{ "name": "v1" }, { "name": "v2" }]
            }
        })];
        let kinds = defined_kinds(&objects);
        assert_eq!(kinds.len(), 2);
        assert_eq!(kinds[1].api_resource.api_version, "example.com/v2");
        assert!(kinds[0].namespaced);
        assert_eq!(kinds[0].crd, "widgets.example.com");
    }
}
