use kube::discovery::{ApiCapabilities, ApiResource, Scope, verbs};
use serde::{Deserialize, Serialize};

/// Identifies a resource type independently of its kind, the way the API server addresses it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRef {
    /// Empty for the core group.
    #[serde(default)]
    pub group: String,
    pub version: String,
    pub plural: String,
}

impl std::fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.group.is_empty() {
            write!(f, "{}/{}", self.version, self.plural)
        } else {
            write!(f, "{}/{}/{}", self.group, self.version, self.plural)
        }
    }
}

/// A resource type served by the connected cluster, including CRDs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    #[serde(flatten)]
    pub reference: ResourceRef,
    pub kind: String,
    pub namespaced: bool,
    pub verbs: Vec<String>,
}

impl ResourceInfo {
    pub(crate) fn new(resource: &ApiResource, caps: &ApiCapabilities) -> Self {
        Self {
            reference: ResourceRef {
                group: resource.group.clone(),
                version: resource.version.clone(),
                plural: resource.plural.clone(),
            },
            kind: resource.kind.clone(),
            namespaced: caps.scope == Scope::Namespaced,
            verbs: caps.operations.clone(),
        }
    }

    pub fn can_watch(&self) -> bool {
        self.verbs.iter().any(|v| v == verbs::WATCH)
    }
}
