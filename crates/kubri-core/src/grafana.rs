//! Grafana dashboards for an object: those whose variables Kubri can fill with the object's
//! namespace, pod, workload or node, as links that open them already filtered.
//!
//! Dashboards come from Grafana's search API and from ConfigMaps labeled `grafana_dashboard`,
//! which is how kube-prometheus-stack and most charts ship them; the ConfigMaps need no
//! Grafana login.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::ListParams;
use kube::{Api, Client};
use serde::Serialize;
use serde_json::Value;

use crate::integrations::Endpoint;

/// Dashboards whose variables are fetched one by one from the API, at most.
const MAX_FETCHED: usize = 150;

/// A dashboard and the names of its variables.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Dashboard {
    pub uid: String,
    pub title: String,
    pub folder: String,
    pub variables: Vec<String>,
}

/// The dashboards read, when, and why Grafana's own list could not be.
pub(crate) type Cached = (std::time::Instant, Vec<Dashboard>, Option<String>);

/// A dashboard to open for an object.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLink {
    pub title: String,
    pub folder: String,
    /// The path and query on Grafana, like `/d/uid?var-namespace=shop`.
    pub path: String,
    /// The variables Kubri filled in.
    pub filled: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboards {
    pub links: Vec<DashboardLink>,
    /// Why Grafana's own list could not be read, when only ConfigMaps were.
    pub note: Option<String>,
}

/// The variables of a dashboard's JSON, without data sources, constants and intervals.
fn variables(dashboard: &Value) -> Vec<String> {
    dashboard
        .pointer("/templating/list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|v| {
            !matches!(
                v.get("type").and_then(Value::as_str),
                Some("datasource" | "constant" | "interval" | "adhoc")
            )
        })
        .filter_map(|v| v.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn from_json(dashboard: &Value, folder: &str) -> Option<Dashboard> {
    Some(Dashboard {
        uid: dashboard.get("uid")?.as_str()?.to_owned(),
        title: dashboard
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        folder: folder.to_owned(),
        variables: variables(dashboard),
    })
}

/// Dashboards shipped in ConfigMaps for Grafana's sidecar.
pub(crate) async fn from_config_maps(client: &Client) -> Vec<Dashboard> {
    let api: Api<ConfigMap> = Api::all(client.clone());
    let Ok(list) = api.list(&ListParams::default().labels("grafana_dashboard")).await else {
        return Vec::new();
    };
    list.items
        .iter()
        .flat_map(|cm| {
            let folder = cm
                .metadata
                .annotations
                .as_ref()
                .and_then(|a| {
                    a.get("grafana_folder")
                        .or_else(|| a.get("k8s-sidecar-target-directory"))
                })
                .cloned()
                .unwrap_or_default();
            cm.data
                .iter()
                .flatten()
                .filter_map(move |(_, text)| from_json(&serde_json::from_str(text).ok()?, &folder))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Dashboards Grafana lists, with their variables.
pub(crate) async fn from_api(
    client: &Client,
    endpoint: &Endpoint,
    token: Option<&str>,
) -> crate::Result<Vec<Dashboard>> {
    let found = endpoint
        .get_json(client, "/api/search?type=dash-db&limit=5000", token)
        .await?;
    let items: Vec<Value> = found.as_array().cloned().unwrap_or_default();
    let fetches = items.iter().take(MAX_FETCHED).filter_map(|item| {
        let uid = item.get("uid")?.as_str()?.to_owned();
        let folder = item
            .get("folderTitle")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Some(async move {
            let answer = endpoint
                .get_json(client, &format!("/api/dashboards/uid/{uid}"), token)
                .await
                .ok()?;
            from_json(answer.get("dashboard")?, &folder)
        })
    });
    Ok(futures::future::join_all(fetches)
        .await
        .into_iter()
        .flatten()
        .collect())
}

/// What an object fills in: its kind decides which variables it can give.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Subject {
    Pod {
        namespace: String,
        name: String,
    },
    Workload {
        kind: String,
        namespace: String,
        name: String,
    },
    Node {
        name: String,
    },
    Namespace {
        name: String,
    },
}

/// The dashboards that show `subject`, best first: those that take its own variable, with
/// the variables it can fill in the link.
pub(crate) fn links(dashboards: &[Dashboard], subject: &Subject) -> Vec<DashboardLink> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(usize, DashboardLink)> = Vec::new();
    for d in dashboards {
        if !seen.insert(d.uid.clone()) {
            continue;
        }
        let has = |name: &str| d.variables.iter().any(|v| v == name);
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        let fits = match subject {
            Subject::Pod { namespace, name } => {
                values.insert("namespace".into(), namespace.clone());
                values.insert("pod".into(), name.clone());
                has("pod")
            }
            Subject::Workload {
                kind,
                namespace,
                name,
            } => {
                let kind = kind.to_ascii_lowercase();
                values.insert("namespace".into(), namespace.clone());
                if has("workload") {
                    values.insert("workload".into(), name.clone());
                    values.insert("type".into(), kind.clone());
                    values.insert("workload_type".into(), kind);
                    true
                } else if has(&kind) {
                    values.insert(kind, name.clone());
                    true
                } else {
                    false
                }
            }
            Subject::Node { name } => {
                values.insert("node".into(), name.clone());
                has("node")
            }
            // A namespace's own dashboards: none that also need a pod, workload or node.
            Subject::Namespace { name } => {
                values.insert("namespace".into(), name.clone());
                has("namespace")
                    && !["pod", "workload", "node", "deployment", "statefulset"]
                        .iter()
                        .any(|v| has(v))
            }
        };
        if !fits {
            continue;
        }
        let filled: Vec<String> = d
            .variables
            .iter()
            .filter(|v| values.contains_key(*v))
            .cloned()
            .collect();
        let query: Vec<String> = filled
            .iter()
            .map(|v| format!("var-{v}={}", encode(&values[v])))
            .collect();
        out.push((
            filled.len(),
            DashboardLink {
                title: d.title.clone(),
                folder: d.folder.clone(),
                path: format!("/d/{}?{}", encode(&d.uid), query.join("&")),
                filled,
            },
        ));
    }
    out.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.title.cmp(&y.title)));
    out.into_iter().map(|(_, link)| link).collect()
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dashboard(uid: &str, title: &str, vars: &[&str]) -> Dashboard {
        Dashboard {
            uid: uid.into(),
            title: title.into(),
            folder: "Kubernetes".into(),
            variables: vars.iter().map(|v| v.to_string()).collect(),
        }
    }

    #[test]
    fn reads_variables() {
        let json = serde_json::json!({
            "uid": "pod", "title": "Pod",
            "templating": { "list": [
                { "name": "datasource", "type": "datasource" },
                { "name": "cluster", "type": "query" },
                { "name": "namespace", "type": "query" },
                { "name": "pod", "type": "query" },
                { "name": "interval", "type": "interval" }
            ]}
        });
        assert_eq!(
            from_json(&json, "").unwrap().variables,
            ["cluster", "namespace", "pod"]
        );
    }

    #[test]
    fn links_fill_what_the_object_gives() {
        let dashboards = [
            dashboard(
                "ns",
                "Compute Resources / Namespace (Pods)",
                &["cluster", "namespace"],
            ),
            dashboard("pod", "Compute Resources / Pod", &["cluster", "namespace", "pod"]),
            dashboard(
                "wl",
                "Compute Resources / Workload",
                &["cluster", "namespace", "type", "workload"],
            ),
            dashboard("node", "Node (Pods)", &["cluster", "node"]),
            dashboard("pod", "Compute Resources / Pod", &["cluster", "namespace", "pod"]),
        ];
        let pod = links(
            &dashboards,
            &Subject::Pod {
                namespace: "shop".into(),
                name: "web-1".into(),
            },
        );
        assert_eq!(pod.len(), 1);
        assert_eq!(pod[0].path, "/d/pod?var-namespace=shop&var-pod=web-1");
        let workload = links(
            &dashboards,
            &Subject::Workload {
                kind: "Deployment".into(),
                namespace: "shop".into(),
                name: "web".into(),
            },
        );
        assert_eq!(
            workload[0].path,
            "/d/wl?var-namespace=shop&var-type=deployment&var-workload=web"
        );
        let namespace = links(&dashboards, &Subject::Namespace { name: "shop".into() });
        assert_eq!(
            namespace.iter().map(|l| l.title.as_str()).collect::<Vec<_>>(),
            ["Compute Resources / Namespace (Pods)"]
        );
        assert_eq!(
            links(&dashboards, &Subject::Node { name: "n 1".into() })[0].path,
            "/d/node?var-node=n%201"
        );
    }
}
