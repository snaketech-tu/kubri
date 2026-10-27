use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource;
use kube::discovery::ApiResource;
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
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    #[serde(flatten)]
    pub reference: ResourceRef,
    pub kind: String,
    pub singular: String,
    /// Aliases such as `po` or `deploy`, used by the command palette.
    pub short_names: Vec<String>,
    pub namespaced: bool,
    pub verbs: Vec<String>,
    /// Subresources such as `scale`, `log` or `exec`.
    pub subresources: Vec<String>,
}

impl ResourceInfo {
    /// Builds the entry for `resource`, collecting its subresources from the rest of the list.
    pub(crate) fn from_api_resource(
        group: &str,
        version: &str,
        resource: &APIResource,
        all: &[APIResource],
    ) -> Self {
        let prefix = format!("{}/", resource.name);
        let singular = if resource.singular_name.is_empty() {
            resource.kind.to_lowercase()
        } else {
            resource.singular_name.clone()
        };
        Self {
            reference: ResourceRef {
                group: group.to_owned(),
                version: version.to_owned(),
                plural: resource.name.clone(),
            },
            kind: resource.kind.clone(),
            singular,
            short_names: resource.short_names.clone().unwrap_or_default(),
            namespaced: resource.namespaced,
            verbs: resource.verbs.clone(),
            subresources: all
                .iter()
                .filter_map(|r| r.name.strip_prefix(&prefix).map(str::to_owned))
                .collect(),
        }
    }

    pub fn can(&self, verb: &str) -> bool {
        self.verbs.iter().any(|v| v == verb)
    }

    pub(crate) fn api_resource(&self) -> ApiResource {
        let r = &self.reference;
        ApiResource {
            group: r.group.clone(),
            version: r.version.clone(),
            api_version: if r.group.is_empty() {
                r.version.clone()
            } else {
                format!("{}/{}", r.group, r.version)
            },
            kind: self.kind.clone(),
            plural: r.plural.clone(),
        }
    }
}
