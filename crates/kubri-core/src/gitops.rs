//! Flux and Argo CD: their actions as the `flux` and `argocd` CLIs take them, as patches on the
//! objects the controllers watch, and which of them manages an object.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, Kubri, ObjectRef, Result};

/// Where Argo CD keeps the auto-sync policy it turned off, to turn it on again as it was.
const SAVED_AUTO_SYNC: &str = "kubri.dev/auto-sync";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitOpsAction {
    /// Flux: reconcile now, like `flux reconcile`.
    Reconcile,
    /// Flux: stop reconciling, like `flux suspend`.
    Suspend,
    /// Flux: reconcile again, like `flux resume`.
    Resume,
    /// Argo CD: sync to the target revision, like `argocd app sync`.
    Sync,
    /// Argo CD: compare with Git again, like `argocd app get --refresh`.
    Refresh,
    /// Argo CD: turn automated sync off.
    AutoSyncOff,
    /// Argo CD: turn automated sync on, as it was before it was turned off.
    AutoSyncOn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Tool {
    Flux,
    #[serde(rename = "Argo CD")]
    ArgoCd,
}

pub(crate) fn tool_of(group: &str) -> Option<Tool> {
    if group.ends_with(".toolkit.fluxcd.io") {
        Some(Tool::Flux)
    } else if group == "argoproj.io" {
        Some(Tool::ArgoCd)
    } else {
        None
    }
}

/// The merge patch that does `action` on `object`, a Flux object or an Argo CD Application.
pub(crate) fn patch(
    group: &str,
    kind: &str,
    object: &Value,
    action: GitOpsAction,
    now: &str,
) -> Result<Value> {
    use GitOpsAction::*;
    let tool = tool_of(group);
    let unsupported = || Error::Unsupported(format!("{action:?} does not apply to {kind}"));
    match (tool, action) {
        (Some(Tool::Flux), Reconcile) => Ok(requested(now)),
        (Some(Tool::Flux), Suspend) => Ok(json!({ "spec": { "suspend": true } })),
        (Some(Tool::Flux), Resume) => {
            let mut patch = requested(now);
            patch["spec"] = json!({ "suspend": false });
            Ok(patch)
        }
        (Some(Tool::ArgoCd), _) if kind != "Application" => Err(unsupported()),
        (Some(Tool::ArgoCd), Sync) => {
            if object.get("operation").is_some_and(|o| !o.is_null()) {
                return Err(Error::Unsupported(
                    "Argo CD is already running an operation on this Application".into(),
                ));
            }
            Ok(json!({
                "operation": {
                    "initiatedBy": { "username": "kubri" },
                    "sync": { "syncStrategy": { "hook": {} } }
                }
            }))
        }
        (Some(Tool::ArgoCd), Refresh) => Ok(json!({
            "metadata": { "annotations": { "argocd.argoproj.io/refresh": "normal" } }
        })),
        (Some(Tool::ArgoCd), AutoSyncOff) => {
            let automated = object
                .pointer("/spec/syncPolicy/automated")
                .filter(|a| !a.is_null())
                .ok_or_else(|| Error::Unsupported("Auto-sync is already off".into()))?;
            Ok(json!({
                "metadata": { "annotations": { SAVED_AUTO_SYNC: automated.to_string() } },
                "spec": { "syncPolicy": { "automated": null } }
            }))
        }
        (Some(Tool::ArgoCd), AutoSyncOn) => {
            let saved = object
                .pointer(&format!(
                    "/metadata/annotations/{}",
                    SAVED_AUTO_SYNC.replace('/', "~1")
                ))
                .and_then(Value::as_str)
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({}));
            Ok(json!({
                "metadata": { "annotations": { SAVED_AUTO_SYNC: null } },
                "spec": { "syncPolicy": { "automated": saved } }
            }))
        }
        _ => Err(unsupported()),
    }
}

fn requested(now: &str) -> Value {
    json!({ "metadata": { "annotations": { "reconcile.fluxcd.io/requestedAt": now } } })
}

/// The Flux or Argo CD object that manages an object, as the object's own labels and
/// annotations name it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hint {
    pub tool: Tool,
    pub api_version: &'static str,
    pub kind: &'static str,
    /// Unknown for an Argo CD Application, which can live in any namespace.
    pub namespace: Option<String>,
    pub name: String,
    /// Only the `app.kubernetes.io/instance` label names it, which Helm sets too: the
    /// Application must list the object to count.
    pub weak: bool,
}

pub(crate) fn hint(object: &Value) -> Option<Hint> {
    let label = |key: &str| {
        object
            .pointer("/metadata/labels")
            .and_then(|l| l.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let annotation = |key: &str| {
        object
            .pointer("/metadata/annotations")
            .and_then(|a| a.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    // `app:group/Kind:namespace/name`, the app as `namespace_app` outside Argo CD's namespace.
    if let Some(id) = annotation("argocd.argoproj.io/tracking-id") {
        let app = id.split(':').next().unwrap_or_default();
        let (namespace, name) = match app.split_once('_') {
            Some((ns, name)) => (Some(ns.to_owned()), name.to_owned()),
            None => (None, app.to_owned()),
        };
        if !name.is_empty() {
            return Some(argo(namespace, name, false));
        }
    }
    for (group, api_version, kind) in [
        ("kustomize", "kustomize.toolkit.fluxcd.io/v1", "Kustomization"),
        ("helm", "helm.toolkit.fluxcd.io/v2", "HelmRelease"),
    ] {
        if let (Some(name), Some(namespace)) = (
            label(&format!("{group}.toolkit.fluxcd.io/name")),
            label(&format!("{group}.toolkit.fluxcd.io/namespace")),
        ) {
            return Some(Hint {
                tool: Tool::Flux,
                api_version,
                kind,
                namespace: Some(namespace),
                name,
                weak: false,
            });
        }
    }
    label("app.kubernetes.io/instance").map(|name| argo(None, name, true))
}

fn argo(namespace: Option<String>, name: String, weak: bool) -> Hint {
    Hint {
        tool: Tool::ArgoCd,
        api_version: "argoproj.io/v1alpha1",
        kind: "Application",
        namespace,
        name,
        weak,
    }
}

/// The object that manages another, and whether it puts changes made by hand back.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manager {
    pub tool: Tool,
    pub owner: ObjectRef,
    pub kind: String,
    /// Changes by hand are put back on its next reconcile or right away.
    pub reverts: bool,
    /// It does not reconcile now: suspended, or auto-sync off.
    pub paused: bool,
}

/// Whether `manager` lists the object `target`, for an Application named only by a label.
pub(crate) fn lists(manager: &Value, kind: &str, target: &ObjectRef) -> bool {
    manager
        .pointer("/status/resources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|r| {
            let field = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default();
            field("kind") == kind
                && field("group") == target.resource.group
                && field("name") == target.name
                && field("namespace") == target.namespace.as_deref().unwrap_or_default()
        })
}

/// Whether `manager` puts changes back, and whether it is paused.
pub(crate) fn behavior(tool: Tool, kind: &str, manager: &Value) -> (bool, bool) {
    match tool {
        Tool::ArgoCd => {
            let automated = manager
                .pointer("/spec/syncPolicy/automated")
                .filter(|a| !a.is_null() && a.get("enabled") != Some(&Value::Bool(false)));
            let self_heal = automated
                .and_then(|a| a.get("selfHeal"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            (self_heal, automated.is_none())
        }
        Tool::Flux => {
            let suspended = manager
                .pointer("/spec/suspend")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            // A Kustomization applies everything again on each reconcile; a HelmRelease only
            // when it detects drift.
            let corrects = kind == "Kustomization"
                || manager
                    .pointer("/spec/driftDetection/mode")
                    .and_then(Value::as_str)
                    == Some("enabled");
            (corrects && !suspended, suspended)
        }
    }
}

impl Kubri {
    /// Runs a Flux or Argo CD action on the object `target`, the way their CLIs do.
    pub async fn gitops(&self, target: &ObjectRef, action: GitOpsAction) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        session.ensure_may("patch", target, "").await?;
        let kind = session.info_for(&target.resource)?.kind.clone();
        let api = session.api(&target.resource, target.namespace.as_deref())?;
        let object = serde_json::to_value(api.get(&target.name).await?)?;
        let now = k8s_openapi::jiff::Timestamp::now().to_string();
        let patch = patch(&target.resource.group, &kind, &object, action, &now)?;
        api.patch(
            &target.name,
            &kube::api::PatchParams::default(),
            &kube::api::Patch::Merge(patch),
        )
        .await?;
        Ok(())
    }

    /// The Flux Kustomization or HelmRelease, or the Argo CD Application, that manages the
    /// object `target`, when it exists.
    pub async fn managed_by(&self, target: &ObjectRef) -> Result<Option<Manager>> {
        let session = self.object_session(target).await?;
        let kind = session.info_for(&target.resource)?.kind.clone();
        let api = session.api(&target.resource, target.namespace.as_deref())?;
        let object = serde_json::to_value(api.get(&target.name).await?)?;
        let Some(hint) = hint(&object) else {
            return Ok(None);
        };
        let Some(resource) = session.resolve(hint.api_version, hint.kind) else {
            return Ok(None);
        };
        let manager = match &hint.namespace {
            Some(namespace) => session
                .api(&resource, Some(namespace))?
                .get_opt(&hint.name)
                .await?
                .map(serde_json::to_value)
                .transpose()?,
            // An Application without its namespace: the one of that name that lists the object.
            None => session
                .api(&resource, None)?
                .list(&Default::default())
                .await?
                .items
                .into_iter()
                .filter(|a| a.metadata.name.as_deref() == Some(hint.name.as_str()))
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .find(|a| !hint.weak || lists(a, &kind, target)),
        };
        let Some(manager) = manager else {
            return Ok(None);
        };
        if hint.weak && !lists(&manager, &kind, target) {
            return Ok(None);
        }
        let (reverts, paused) = behavior(hint.tool, hint.kind, &manager);
        let namespace = manager
            .pointer("/metadata/namespace")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(Some(Manager {
            tool: hint.tool,
            owner: ObjectRef {
                context: Some(session.info.context.clone()),
                resource,
                namespace,
                name: hint.name,
            },
            kind: hint.kind.to_owned(),
            reverts,
            paused,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-10-29T20:00:00Z";

    #[test]
    fn flux_patches() {
        let k = json!({});
        assert_eq!(
            patch(
                "kustomize.toolkit.fluxcd.io",
                "Kustomization",
                &k,
                GitOpsAction::Reconcile,
                NOW
            )
            .unwrap(),
            json!({ "metadata": { "annotations": { "reconcile.fluxcd.io/requestedAt": NOW } } })
        );
        assert_eq!(
            patch(
                "helm.toolkit.fluxcd.io",
                "HelmRelease",
                &k,
                GitOpsAction::Resume,
                NOW
            )
            .unwrap(),
            json!({
                "metadata": { "annotations": { "reconcile.fluxcd.io/requestedAt": NOW } },
                "spec": { "suspend": false }
            })
        );
        assert!(
            patch(
                "kustomize.toolkit.fluxcd.io",
                "Kustomization",
                &k,
                GitOpsAction::Sync,
                NOW
            )
            .is_err()
        );
        assert!(patch("apps", "Deployment", &k, GitOpsAction::Reconcile, NOW).is_err());
    }

    #[test]
    fn argo_auto_sync_round_trip() {
        let app = json!({ "spec": { "syncPolicy": { "automated": { "prune": true, "selfHeal": true } } } });
        let off = patch("argoproj.io", "Application", &app, GitOpsAction::AutoSyncOff, NOW).unwrap();
        assert_eq!(off["spec"]["syncPolicy"]["automated"], Value::Null);
        let saved = off["metadata"]["annotations"][SAVED_AUTO_SYNC].clone();
        let paused = json!({ "metadata": { "annotations": { SAVED_AUTO_SYNC: saved } }, "spec": { "syncPolicy": {} } });
        let on = patch(
            "argoproj.io",
            "Application",
            &paused,
            GitOpsAction::AutoSyncOn,
            NOW,
        )
        .unwrap();
        assert_eq!(
            on["spec"]["syncPolicy"]["automated"],
            json!({ "prune": true, "selfHeal": true })
        );
        assert_eq!(on["metadata"]["annotations"][SAVED_AUTO_SYNC], Value::Null);
        assert!(
            patch(
                "argoproj.io",
                "Application",
                &paused,
                GitOpsAction::AutoSyncOff,
                NOW
            )
            .is_err()
        );
        let busy = json!({ "operation": { "sync": {} } });
        assert!(patch("argoproj.io", "Application", &busy, GitOpsAction::Sync, NOW).is_err());
        assert!(patch("argoproj.io", "AppProject", &app, GitOpsAction::Sync, NOW).is_err());
    }

    #[test]
    fn finds_managers() {
        let tracked = json!({ "metadata": { "annotations": {
            "argocd.argoproj.io/tracking-id": "team_payments:apps/Deployment:shop/worker"
        } } });
        let found = hint(&tracked).unwrap();
        assert_eq!(
            (found.tool, found.namespace.as_deref(), found.name.as_str()),
            (Tool::ArgoCd, Some("team"), "payments")
        );
        let flux = json!({ "metadata": { "labels": {
            "kustomize.toolkit.fluxcd.io/name": "shop", "kustomize.toolkit.fluxcd.io/namespace": "flux-system"
        } } });
        let found = hint(&flux).unwrap();
        assert_eq!(
            (found.kind, found.namespace.as_deref()),
            ("Kustomization", Some("flux-system"))
        );
        let labeled = json!({ "metadata": { "labels": { "app.kubernetes.io/instance": "web" } } });
        assert!(hint(&labeled).unwrap().weak);
        assert_eq!(hint(&json!({ "metadata": {} })), None);
    }

    #[test]
    fn behaviors() {
        let healing = json!({ "spec": { "syncPolicy": { "automated": { "selfHeal": true } } } });
        assert_eq!(behavior(Tool::ArgoCd, "Application", &healing), (true, false));
        assert_eq!(
            behavior(Tool::ArgoCd, "Application", &json!({ "spec": {} })),
            (false, true)
        );
        let disabled =
            json!({ "spec": { "syncPolicy": { "automated": { "enabled": false, "selfHeal": true } } } });
        assert_eq!(behavior(Tool::ArgoCd, "Application", &disabled), (false, true));
        assert_eq!(
            behavior(Tool::Flux, "Kustomization", &json!({ "spec": {} })),
            (true, false)
        );
        assert_eq!(
            behavior(
                Tool::Flux,
                "Kustomization",
                &json!({ "spec": { "suspend": true } })
            ),
            (false, true)
        );
        assert_eq!(
            behavior(Tool::Flux, "HelmRelease", &json!({ "spec": {} })),
            (false, false)
        );
    }

    #[test]
    fn listed_resources() {
        let app = json!({ "status": { "resources": [
            { "group": "apps", "kind": "Deployment", "namespace": "shop", "name": "worker" }
        ] } });
        let target = ObjectRef {
            context: None,
            resource: crate::ResourceRef {
                group: "apps".into(),
                version: "v1".into(),
                plural: "deployments".into(),
            },
            namespace: Some("shop".into()),
            name: "worker".into(),
        };
        assert!(lists(&app, "Deployment", &target));
        assert!(!lists(&app, "StatefulSet", &target));
    }
}
