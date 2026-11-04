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
use serde::{Deserialize, Serialize};
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
    /// A whole URL, from a link in the context's settings, opened as it is; `path` is then
    /// empty.
    pub url: Option<String>,
}

/// A link to a Grafana in a context's settings, with placeholders that an object fills in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GrafanaLink {
    pub title: String,
    /// A URL like `https://grafana.example.com/d/abc?var-pod={pod}`, or a path like
    /// `/d/abc?var-namespace={namespace}` on the Grafana Kubri found.
    pub url: String,
}

/// Grafana's dashboard search for `name`, which works even where Kubri cannot read
/// Grafana's list, as the browser signs in.
pub(crate) fn search_link(name: &str) -> DashboardLink {
    DashboardLink {
        title: format!("Search Grafana for {name}"),
        folder: String::new(),
        path: format!("/dashboards?query={}", encode(name)),
        filled: Vec::new(),
        url: None,
    }
}

/// The kinds a `{workload}` placeholder stands for.
const WORKLOADS: [&str; 6] = [
    "Deployment",
    "StatefulSet",
    "DaemonSet",
    "ReplicaSet",
    "Job",
    "CronJob",
];

/// The names of the placeholders in `template`, like `pod` for `{pod}`, in order.
pub(crate) fn placeholders(template: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else { break };
        let name = &rest[..end];
        if !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            out.push(name);
            rest = &rest[end + 1..];
        }
    }
    out
}

/// What an object fills in: `context`, `name` and `kind`, `namespace` for objects in one and
/// for a Namespace itself, the name under its kind, like `pod` or `node`, and `workload` for
/// Deployments, StatefulSets, DaemonSets, ReplicaSets, Jobs and CronJobs.
pub(crate) fn object_values(
    context: &str,
    kind: &str,
    namespace: Option<&str>,
    name: &str,
) -> BTreeMap<String, String> {
    // The kind's own first, so that a custom kind named like `Name` cannot replace the others.
    let mut values = BTreeMap::from([(kind.to_ascii_lowercase(), name.to_owned())]);
    values.insert("context".into(), context.to_owned());
    values.insert("name".into(), name.to_owned());
    values.insert("kind".into(), kind.to_ascii_lowercase());
    if let Some(namespace) = namespace.filter(|n| !n.is_empty()) {
        values.insert("namespace".into(), namespace.to_owned());
    } else if kind == "Namespace" {
        values.insert("namespace".into(), name.to_owned());
    }
    if WORKLOADS.contains(&kind) {
        values.insert("workload".into(), name.to_owned());
    }
    values
}

/// `template` with its placeholders filled, each value URL-encoded; `None` when the object
/// lacks one of them, so that a link for pods shows only for pods.
pub(crate) fn fill(template: &str, values: &BTreeMap<String, String>) -> Option<String> {
    let mut out = template.to_owned();
    for name in placeholders(template) {
        out = out.replace(&format!("{{{name}}}"), &encode(values.get(name)?));
    }
    Some(out)
}

/// Why a link from the settings cannot work; `None` when it can.
pub(crate) fn link_problem(link: &GrafanaLink) -> Option<String> {
    let url = link.url.as_str();
    if url.starts_with('/') {
        return None;
    }
    // Every placeholder filled with a plain word, to see that the rest is a URL.
    let sample: BTreeMap<String, String> = placeholders(url)
        .into_iter()
        .map(|p| (p.to_owned(), "x".to_owned()))
        .collect();
    match reqwest::Url::parse(&fill(url, &sample).unwrap_or_default()) {
        Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => None,
        _ => Some(format!(
            "the dashboard address `{url}` must start with http:// or https://, or with / for a path on the Grafana Kubri found"
        )),
    }
}

/// The links from the settings that `values` can fill, in their order.
pub(crate) fn settings_links(links: &[GrafanaLink], values: &BTreeMap<String, String>) -> Vec<DashboardLink> {
    links
        .iter()
        .filter_map(|link| {
            let filled = fill(&link.url, values)?;
            let mut names: Vec<String> = Vec::new();
            for name in placeholders(&link.url) {
                if !names.iter().any(|n| n == name) {
                    names.push(name.to_owned());
                }
            }
            let (path, url) = if filled.starts_with('/') {
                (filled, None)
            } else {
                (String::new(), Some(filled))
            };
            Some(DashboardLink {
                title: link.title.clone(),
                folder: String::new(),
                path,
                filled: names,
                url,
            })
        })
        .collect()
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
                url: None,
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

    fn link(title: &str, url: &str) -> GrafanaLink {
        GrafanaLink {
            title: title.into(),
            url: url.into(),
        }
    }

    #[test]
    fn finds_placeholders() {
        assert_eq!(
            placeholders("https://g/d/a?var-ns={namespace}&var-pod={pod}&x={Pod}&y={}&z={pod}"),
            ["namespace", "pod", "pod"]
        );
        assert_eq!(placeholders("https://g/{namespace"), Vec::<&str>::new());
        assert_eq!(placeholders("https://g/{{pod}"), ["pod"]);
    }

    #[test]
    fn settings_links_show_where_the_object_fills_them() {
        let links = [
            link(
                "Pod",
                "https://grafana.example.com/d/pod?var-namespace={namespace}&var-pod={pod}&from={namespace}",
            ),
            link(
                "Namespace",
                "https://grafana.example.com/d/ns?var-namespace={namespace}&ctx={context}",
            ),
            link("Workload", "/d/wl?var-workload={workload}&var-type={kind}"),
            link("Anything", "https://logs.example.com/search?q={kind}%2F{name}"),
        ];
        let titles = |values: &BTreeMap<String, String>| {
            settings_links(&links, values)
                .into_iter()
                .map(|l| l.title)
                .collect::<Vec<_>>()
        };
        let pod = object_values("prod eu", "Pod", Some("shop"), "web-1");
        assert_eq!(titles(&pod), ["Pod", "Namespace", "Anything"]);
        let found = settings_links(&links, &pod);
        assert_eq!(
            found[0].url.as_deref(),
            Some("https://grafana.example.com/d/pod?var-namespace=shop&var-pod=web-1&from=shop")
        );
        assert_eq!(found[0].filled, ["namespace", "pod"]);
        assert_eq!(
            found[1].url.as_deref(),
            Some("https://grafana.example.com/d/ns?var-namespace=shop&ctx=prod%20eu")
        );
        let deployment = object_values("c", "Deployment", Some("shop"), "web");
        assert_eq!(titles(&deployment), ["Namespace", "Workload", "Anything"]);
        let path = &settings_links(&links, &deployment)[1];
        assert_eq!(
            (path.path.as_str(), path.url.as_deref()),
            ("/d/wl?var-workload=web&var-type=deployment", None)
        );
        // A node has no namespace; a Namespace fills its own.
        assert_eq!(titles(&object_values("c", "Node", None, "n1")), ["Anything"]);
        assert_eq!(
            titles(&object_values("c", "Namespace", None, "shop")),
            ["Namespace", "Anything"]
        );
    }

    #[test]
    fn searches_grafana_for_the_name() {
        let link = search_link("web 1");
        assert_eq!(link.path, "/dashboards?query=web%201");
        assert_eq!(link.url, None);
    }

    #[test]
    fn settings_links_must_be_urls() {
        assert_eq!(
            link_problem(&link("", "https://g.example.com/d/a?var-pod={pod}")),
            None
        );
        assert_eq!(link_problem(&link("", "/d/a?var-pod={pod}")), None);
        assert_eq!(link_problem(&link("", "http://{name}.example.com/")), None);
        for bad in [
            "grafana.example.com/d/a",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "{name}",
        ] {
            assert!(link_problem(&link("", bad)).is_some(), "{bad}");
        }
    }
}
