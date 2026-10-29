//! What the current user may do, asked the API server one action at a time
//! (`SelfSubjectAccessReview`, like `kubectl auth can-i`), and remembered for a minute.
//!
//! The UI greys out what is not allowed and says why; the core checks again before it changes
//! anything, so every caller gets the same answer.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use k8s_openapi::api::authentication::v1::SelfSubjectReview;
use k8s_openapi::api::authorization::v1::{
    ResourceAttributes, SelfSubjectAccessReview, SelfSubjectAccessReviewSpec,
};
use kube::api::PostParams;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};

use crate::ResourceRef;

/// How long an answer is trusted. Roles rarely change while someone works.
const TTL: Duration = Duration::from_secs(60);
/// Reviews asked at once.
const PARALLEL: usize = 16;
/// The most checks one request may ask for.
pub const MAX_CHECKS: usize = 200;

/// One action, as RBAC sees it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessCheck {
    pub verb: String,
    /// The API group; empty for the core group.
    #[serde(default)]
    pub group: String,
    /// The plural resource name, such as `pods`.
    pub resource: String,
    /// Such as `exec`, `log` or `scale`.
    #[serde(default)]
    pub subresource: String,
    /// `None` for cluster-scoped resources, or for all namespaces.
    #[serde(default)]
    pub namespace: Option<String>,
    /// One object; rules may name objects.
    #[serde(default)]
    pub name: Option<String>,
}

impl AccessCheck {
    pub fn new(verb: &str, resource: &ResourceRef, namespace: Option<&str>) -> Self {
        Self {
            verb: verb.to_owned(),
            group: resource.group.clone(),
            resource: resource.plural.clone(),
            subresource: String::new(),
            namespace: namespace.map(str::to_owned),
            name: None,
        }
    }

    pub fn on_subresource(mut self, subresource: &str) -> Self {
        self.subresource = subresource.to_owned();
        self
    }

    pub fn named(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    /// Such as "`delete` on `pods` in `shop`", for messages.
    pub fn describe(&self) -> String {
        let mut resource = self.resource.clone();
        if !self.group.is_empty() {
            resource = format!("{resource}.{}", self.group);
        }
        if !self.subresource.is_empty() {
            resource = format!("{resource}/{}", self.subresource);
        }
        match &self.namespace {
            Some(ns) => format!("`{}` on `{resource}` in `{ns}`", self.verb),
            None => format!("`{}` on `{resource}`", self.verb),
        }
    }
}

/// Answers per check, per connection.
#[derive(Default)]
pub(crate) struct AccessCache {
    entries: Mutex<HashMap<AccessCheck, (bool, Instant)>>,
}

impl AccessCache {
    fn get(&self, check: &AccessCheck) -> Option<bool> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries
            .get(check)
            .filter(|(_, at)| at.elapsed() < TTL)
            .map(|(allowed, _)| *allowed)
    }

    fn put(&self, check: AccessCheck, allowed: bool) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, (_, at)| at.elapsed() < TTL);
        entries.insert(check, (allowed, Instant::now()));
    }

    /// Forgets every answer, after a role may have changed.
    pub(crate) fn clear(&self) {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// Whether each check is allowed; `None` where the API server could not say, which callers
/// treat as allowed and leave to the API server itself.
pub(crate) async fn check_all(
    client: &Client,
    cache: &AccessCache,
    checks: &[AccessCheck],
) -> Vec<Option<bool>> {
    let mut answers = Vec::with_capacity(checks.len());
    for chunk in checks.chunks(PARALLEL) {
        let asked = chunk.iter().map(|check| answer(client, cache, check));
        answers.extend(futures::future::join_all(asked).await);
    }
    answers
}

async fn answer(client: &Client, cache: &AccessCache, check: &AccessCheck) -> Option<bool> {
    if let Some(allowed) = cache.get(check) {
        return Some(allowed);
    }
    let allowed = review(client, check).await?;
    cache.put(check.clone(), allowed);
    Some(allowed)
}

async fn review(client: &Client, check: &AccessCheck) -> Option<bool> {
    let review = SelfSubjectAccessReview {
        spec: SelfSubjectAccessReviewSpec {
            resource_attributes: Some(ResourceAttributes {
                verb: Some(check.verb.clone()),
                group: Some(check.group.clone()),
                resource: Some(check.resource.clone()),
                subresource: (!check.subresource.is_empty()).then(|| check.subresource.clone()),
                namespace: check.namespace.clone(),
                name: check.name.clone(),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let api: Api<SelfSubjectAccessReview> = Api::all(client.clone());
    match api.create(&PostParams::default(), &review).await {
        Ok(answer) => answer.status.map(|s| s.allowed),
        Err(err) => {
            tracing::debug!(%err, check = check.describe(), "access review failed");
            None
        }
    }
}

/// Who the API server takes the current user for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub username: String,
    pub groups: Vec<String>,
}

/// The current user, like `kubectl auth whoami`; `None` where the cluster does not say.
pub(crate) async fn who_am_i(client: &Client) -> Option<Identity> {
    let api: Api<SelfSubjectReview> = Api::all(client.clone());
    let review = api
        .create(&PostParams::default(), &SelfSubjectReview::default())
        .await
        .ok()?;
    let user = review.status?.user_info?;
    Some(Identity {
        username: user.username?,
        groups: user.groups.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pods() -> ResourceRef {
        ResourceRef {
            group: String::new(),
            version: "v1".into(),
            plural: "pods".into(),
        }
    }

    #[test]
    fn describes_checks() {
        let delete = AccessCheck::new("delete", &pods(), Some("shop"));
        assert_eq!(delete.describe(), "`delete` on `pods` in `shop`");
        let exec = AccessCheck::new("create", &pods(), Some("shop"))
            .on_subresource("exec")
            .named("web");
        assert_eq!(exec.describe(), "`create` on `pods/exec` in `shop`");
        let deployments = ResourceRef {
            group: "apps".into(),
            version: "v1".into(),
            plural: "deployments".into(),
        };
        let scale = AccessCheck::new("patch", &deployments, None).on_subresource("scale");
        assert_eq!(scale.describe(), "`patch` on `deployments.apps/scale`");
    }

    #[test]
    fn answers_expire() {
        let cache = AccessCache::default();
        let check = AccessCheck::new("get", &pods(), Some("shop"));
        assert_eq!(cache.get(&check), None);
        cache.put(check.clone(), true);
        assert_eq!(cache.get(&check), Some(true));
        cache.entries.lock().unwrap().get_mut(&check).unwrap().1 = Instant::now() - TTL;
        assert_eq!(cache.get(&check), None);
        cache.put(check.clone(), false);
        cache.clear();
        assert_eq!(cache.get(&check), None);
    }
}
