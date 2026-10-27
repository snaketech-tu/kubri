use futures::future::join_all;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResourceList;
use kube::Client;

use crate::{ResourceInfo, Result};

/// Lists every resource type at its preferred version, including CRDs and aggregated APIs.
///
/// API groups that fail to answer (a common case for broken aggregated APIs such as an
/// unhealthy metrics-server) are skipped instead of failing the whole connection.
pub(crate) async fn discover(client: &Client) -> Result<Vec<ResourceInfo>> {
    let core_versions = client.list_core_api_versions().await?;
    let groups = client.list_api_groups().await?;

    let mut group_versions: Vec<String> = core_versions.versions.into_iter().take(1).collect();
    for group in groups.groups {
        let preferred = group
            .preferred_version
            .map(|v| v.group_version)
            .or_else(|| group.versions.first().map(|v| v.group_version.clone()));
        group_versions.extend(preferred);
    }

    let lists = join_all(group_versions.iter().map(|gv| async move {
        let result = if gv.contains('/') {
            client.list_api_group_resources(gv).await
        } else {
            client.list_core_api_resources(gv).await
        };
        (gv, result)
    }))
    .await;

    let mut resources = Vec::new();
    for (gv, result) in lists {
        match result {
            Ok(list) => resources.extend(from_list(&list)),
            Err(err) => tracing::warn!(group_version = %gv, %err, "skipping API group"),
        }
    }
    resources.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(a.reference.group.cmp(&b.reference.group))
    });
    Ok(resources)
}

pub(crate) fn from_list(list: &APIResourceList) -> Vec<ResourceInfo> {
    let (group, version) = match list.group_version.split_once('/') {
        Some((group, version)) => (group, version),
        None => ("", list.group_version.as_str()),
    };
    list.resources
        .iter()
        // Subresources such as pods/log are reached through their parent.
        .filter(|r| !r.name.contains('/'))
        .map(|r| ResourceInfo::from_api_resource(group, version, r, &list.resources))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_resources_with_aliases_and_subresources() {
        let list: APIResourceList = serde_json::from_value(serde_json::json!({
            "groupVersion": "apps/v1",
            "resources": [
                { "name": "deployments", "singularName": "deployment", "namespaced": true,
                  "kind": "Deployment", "verbs": ["get", "list", "watch", "patch"],
                  "shortNames": ["deploy"] },
                { "name": "deployments/scale", "singularName": "", "namespaced": true,
                  "kind": "Scale", "verbs": ["get", "patch"] },
                { "name": "deployments/status", "singularName": "", "namespaced": true,
                  "kind": "Deployment", "verbs": ["get"] }
            ]
        }))
        .unwrap();
        let resources = from_list(&list);
        assert_eq!(resources.len(), 1);
        let deploy = &resources[0];
        assert_eq!(deploy.reference.group, "apps");
        assert_eq!(deploy.reference.version, "v1");
        assert_eq!(deploy.short_names, ["deploy"]);
        assert_eq!(deploy.subresources, ["scale", "status"]);
        assert!(deploy.can("watch"));
        assert_eq!(deploy.api_resource().api_version, "apps/v1");
    }

    #[test]
    fn core_group_has_empty_group_name() {
        let list: APIResourceList = serde_json::from_value(serde_json::json!({
            "groupVersion": "v1",
            "resources": [{ "name": "pods", "singularName": "", "namespaced": true,
                            "kind": "Pod", "verbs": ["list"] }]
        }))
        .unwrap();
        let pod = &from_list(&list)[0];
        assert_eq!(pod.reference.group, "");
        assert_eq!(pod.singular, "pod");
        assert_eq!(pod.api_resource().api_version, "v1");
    }
}
