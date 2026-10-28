//! Who may do what: the subjects of all role bindings with the rules they are granted, and
//! the rules of the current user.

use std::collections::{BTreeMap, HashMap};

use k8s_openapi::api::authorization::v1::{SelfSubjectRulesReview, SelfSubjectRulesReviewSpec};
use k8s_openapi::api::rbac::v1::{
    ClusterRole, ClusterRoleBinding, PolicyRule, Role, RoleBinding, RoleRef, Subject,
};
use kube::api::PostParams;
use kube::{Api, Client};
use serde::Serialize;

use crate::typed::{list, scoped};
use crate::{Error, Result};

/// One rule of a role, as Kubernetes stores it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub api_groups: Vec<String>,
    pub resources: Vec<String>,
    pub resource_names: Vec<String>,
    pub non_resource_urls: Vec<String>,
    pub verbs: Vec<String>,
}

impl From<&PolicyRule> for Rule {
    fn from(rule: &PolicyRule) -> Self {
        Self {
            api_groups: rule.api_groups.clone().unwrap_or_default(),
            resources: rule.resources.clone().unwrap_or_default(),
            resource_names: rule.resource_names.clone().unwrap_or_default(),
            non_resource_urls: rule.non_resource_urls.clone().unwrap_or_default(),
            verbs: rule.verbs.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RoleState {
    Found,
    /// The binding points to a role that does not exist, so it grants nothing.
    Missing,
    /// Roles could not be listed, so the rules are unknown.
    Unreadable,
}

/// What one binding grants one subject.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    /// `RoleBinding` or `ClusterRoleBinding`.
    pub binding_kind: String,
    pub binding: String,
    /// `Role` or `ClusterRole`.
    pub role_kind: String,
    pub role: String,
    /// Where the rules apply: the binding's namespace, or `None` for the whole cluster.
    pub namespace: Option<String>,
    pub role_state: RoleState,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectInfo {
    /// `User`, `Group` or `ServiceAccount`.
    pub kind: String,
    pub name: String,
    /// Set for service accounts.
    pub namespace: Option<String>,
    pub grants: Vec<Grant>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rbac {
    pub subjects: Vec<SubjectInfo>,
    /// Lists that failed, for example because of missing permissions.
    pub errors: Vec<String>,
}

/// What the current user may do, as `kubectl auth can-i --list` shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub namespace: String,
    pub rules: Vec<Rule>,
    /// The API server could not evaluate every rule, for example with webhook authorization.
    pub incomplete: bool,
    pub error: Option<String>,
}

/// Lists bindings and roles and joins them. `namespace` limits role bindings to one
/// namespace; cluster role bindings are always included.
pub(crate) async fn gather(client: Client, namespace: Option<&str>) -> Rbac {
    let c = &client;
    let (bindings, cluster_bindings, roles, cluster_roles) = futures::join!(
        list(scoped::<RoleBinding>(c, namespace)),
        list(Api::<ClusterRoleBinding>::all(c.clone())),
        list(scoped::<Role>(c, namespace)),
        list(Api::<ClusterRole>::all(c.clone())),
    );
    let mut errors = Vec::new();
    let bindings = ok(bindings, "RoleBindings", &mut errors).unwrap_or_default();
    let cluster_bindings = ok(cluster_bindings, "ClusterRoleBindings", &mut errors).unwrap_or_default();
    let roles = ok(roles, "Roles", &mut errors);
    let cluster_roles = ok(cluster_roles, "ClusterRoles", &mut errors);
    Rbac {
        subjects: build(
            &bindings,
            &cluster_bindings,
            roles.as_deref(),
            cluster_roles.as_deref(),
        ),
        errors,
    }
}

fn ok<T>(result: Result<Vec<T>>, what: &str, errors: &mut Vec<String>) -> Option<Vec<T>> {
    result.map_err(|err| errors.push(format!("{what}: {err}"))).ok()
}

/// The current user's rules in `namespace`, including cluster-wide ones.
pub(crate) async fn permissions(client: Client, namespace: &str) -> Result<Permissions> {
    let review = SelfSubjectRulesReview {
        spec: SelfSubjectRulesReviewSpec {
            namespace: Some(namespace.to_owned()),
        },
        ..Default::default()
    };
    let api: Api<SelfSubjectRulesReview> = Api::all(client);
    let status = api
        .create(&PostParams::default(), &review)
        .await?
        .status
        .ok_or_else(|| Error::Unsupported("the API server returned no rules".into()))?;
    let mut rules: Vec<Rule> = status
        .resource_rules
        .iter()
        .map(|r| Rule {
            api_groups: r.api_groups.clone().unwrap_or_default(),
            resources: r.resources.clone().unwrap_or_default(),
            resource_names: r.resource_names.clone().unwrap_or_default(),
            non_resource_urls: Vec::new(),
            verbs: r.verbs.clone(),
        })
        .collect();
    rules.extend(status.non_resource_rules.iter().map(|r| Rule {
        non_resource_urls: r.non_resource_urls.clone().unwrap_or_default(),
        verbs: r.verbs.clone(),
        ..Default::default()
    }));
    Ok(Permissions {
        namespace: namespace.to_owned(),
        rules,
        incomplete: status.incomplete,
        error: status.evaluation_error.filter(|e| !e.is_empty()),
    })
}

type SubjectKey = (u8, String, Option<String>);

fn subject_key(subject: &Subject, binding_namespace: Option<&str>) -> SubjectKey {
    let order = match subject.kind.as_str() {
        "User" => 0,
        "Group" => 1,
        _ => 2,
    };
    // Service accounts belong to a namespace; it defaults to the binding's.
    let namespace = (subject.kind == "ServiceAccount").then(|| {
        subject
            .namespace
            .clone()
            .or_else(|| binding_namespace.map(str::to_owned))
            .unwrap_or_default()
    });
    (order, subject.name.clone(), namespace)
}

/// Joins bindings with their roles, per subject. `None` for roles means they could not be listed.
pub(crate) fn build(
    bindings: &[RoleBinding],
    cluster_bindings: &[ClusterRoleBinding],
    roles: Option<&[Role]>,
    cluster_roles: Option<&[ClusterRole]>,
) -> Vec<SubjectInfo> {
    let role_rules: Option<HashMap<(&str, &str), &[PolicyRule]>> = roles.map(|roles| {
        roles
            .iter()
            .map(|r| {
                let key = (
                    r.metadata.namespace.as_deref().unwrap_or_default(),
                    r.metadata.name.as_deref().unwrap_or_default(),
                );
                (key, r.rules.as_deref().unwrap_or_default())
            })
            .collect()
    });
    let cluster_rules: Option<HashMap<&str, &[PolicyRule]>> = cluster_roles.map(|roles| {
        roles
            .iter()
            .map(|r| {
                (
                    r.metadata.name.as_deref().unwrap_or_default(),
                    r.rules.as_deref().unwrap_or_default(),
                )
            })
            .collect()
    });
    let resolve = |role: &RoleRef, namespace: Option<&str>| -> (RoleState, Vec<Rule>) {
        let rules = if role.kind == "ClusterRole" {
            cluster_rules.as_ref().map(|m| m.get(role.name.as_str()).copied())
        } else {
            role_rules.as_ref().map(|m| {
                m.get(&(namespace.unwrap_or_default(), role.name.as_str()))
                    .copied()
            })
        };
        match rules {
            Some(Some(rules)) => (RoleState::Found, rules.iter().map(Rule::from).collect()),
            Some(None) => (RoleState::Missing, Vec::new()),
            None => (RoleState::Unreadable, Vec::new()),
        }
    };

    let mut subjects: BTreeMap<SubjectKey, SubjectInfo> = BTreeMap::new();
    let mut add = |subject: &Subject, grant: Grant, binding_namespace: Option<&str>| {
        let key = subject_key(subject, binding_namespace);
        subjects
            .entry(key.clone())
            .or_insert_with(|| SubjectInfo {
                kind: subject.kind.clone(),
                name: subject.name.clone(),
                namespace: key.2.clone(),
                grants: Vec::new(),
            })
            .grants
            .push(grant);
    };

    for binding in cluster_bindings {
        let (role_state, rules) = resolve(&binding.role_ref, None);
        for subject in binding.subjects.iter().flatten() {
            let grant = Grant {
                binding_kind: "ClusterRoleBinding".into(),
                binding: binding.metadata.name.clone().unwrap_or_default(),
                role_kind: binding.role_ref.kind.clone(),
                role: binding.role_ref.name.clone(),
                namespace: None,
                role_state,
                rules: rules.clone(),
            };
            add(subject, grant, None);
        }
    }
    for binding in bindings {
        let namespace = binding.metadata.namespace.as_deref();
        let (role_state, rules) = resolve(&binding.role_ref, namespace);
        for subject in binding.subjects.iter().flatten() {
            let grant = Grant {
                binding_kind: "RoleBinding".into(),
                binding: binding.metadata.name.clone().unwrap_or_default(),
                role_kind: binding.role_ref.kind.clone(),
                role: binding.role_ref.name.clone(),
                namespace: namespace.map(str::to_owned),
                role_state,
                rules: rules.clone(),
            };
            add(subject, grant, namespace);
        }
    }
    subjects.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn from<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn fixtures() -> (
        Vec<RoleBinding>,
        Vec<ClusterRoleBinding>,
        Vec<Role>,
        Vec<ClusterRole>,
    ) {
        let bindings = vec![from(json!({
            "metadata": { "name": "deployers", "namespace": "shop" },
            "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "Role", "name": "deploy" },
            "subjects": [
                { "kind": "ServiceAccount", "name": "ci" },
                { "kind": "User", "name": "ada", "apiGroup": "rbac.authorization.k8s.io" },
                { "kind": "Group", "name": "devs", "apiGroup": "rbac.authorization.k8s.io" }
            ]
        }))];
        let cluster_bindings = vec![
            from(json!({
                "metadata": { "name": "ada-view" },
                "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": "view" },
                "subjects": [{ "kind": "User", "name": "ada", "apiGroup": "rbac.authorization.k8s.io" }]
            })),
            from(json!({
                "metadata": { "name": "dangling" },
                "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": "gone" },
                "subjects": [{ "kind": "ServiceAccount", "name": "old", "namespace": "kube-system" }]
            })),
        ];
        let roles = vec![from(json!({
            "metadata": { "name": "deploy", "namespace": "shop" },
            "rules": [{ "apiGroups": ["apps"], "resources": ["deployments"], "verbs": ["get", "patch"] }]
        }))];
        let cluster_roles = vec![from(json!({
            "metadata": { "name": "view" },
            "rules": [
                { "apiGroups": [""], "resources": ["pods", "services"], "verbs": ["get", "list", "watch"] },
                { "nonResourceURLs": ["/healthz"], "verbs": ["get"] }
            ]
        }))];
        (bindings, cluster_bindings, roles, cluster_roles)
    }

    #[test]
    fn joins_bindings_with_their_roles_per_subject() {
        let (b, cb, r, cr) = fixtures();
        let subjects = build(&b, &cb, Some(&r), Some(&cr));
        let keys: Vec<(&str, &str, Option<&str>)> = subjects
            .iter()
            .map(|s| (s.kind.as_str(), s.name.as_str(), s.namespace.as_deref()))
            .collect();
        assert_eq!(
            keys,
            [
                ("User", "ada", None),
                ("Group", "devs", None),
                ("ServiceAccount", "ci", Some("shop")),
                ("ServiceAccount", "old", Some("kube-system")),
            ]
        );

        let ada = &subjects[0];
        assert_eq!(ada.grants.len(), 2);
        let cluster = &ada.grants[0];
        assert_eq!(
            (cluster.binding.as_str(), cluster.namespace.as_deref()),
            ("ada-view", None)
        );
        assert_eq!(cluster.rules.len(), 2);
        assert_eq!(cluster.rules[1].non_resource_urls, ["/healthz"]);
        let local = &ada.grants[1];
        assert_eq!(
            (local.role.as_str(), local.namespace.as_deref()),
            ("deploy", Some("shop"))
        );
        assert_eq!(local.rules[0].verbs, ["get", "patch"]);
    }

    #[test]
    fn marks_missing_and_unreadable_roles() {
        let (b, cb, _, cr) = fixtures();
        let subjects = build(&b, &cb, None, Some(&cr));
        let old = subjects.iter().find(|s| s.name == "old").unwrap();
        assert_eq!(old.grants[0].role_state, RoleState::Missing);
        let ci = subjects.iter().find(|s| s.name == "ci").unwrap();
        assert_eq!(ci.grants[0].role_state, RoleState::Unreadable);
        assert!(ci.grants[0].rules.is_empty());
    }
}
