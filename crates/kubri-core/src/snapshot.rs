//! Snapshots: a namespace or a whole cluster saved to one file and opened offline.
//!
//! Taking one lists every readable resource the way the resource lists do (as the API server's
//! Tables, so the columns stay), plus the recent log lines of the pods. Opening one puts a small
//! API server in front of a [`Client`] that answers from the file: lists, objects, logs and
//! discovery come from the snapshot, watches stay quiet, and every change is refused. So the
//! whole UI, XRay, Pulses and the sanitizer included, works on a snapshot without knowing it.

use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::io::Read;
use std::sync::Arc;

use bytes::Bytes;
use futures::StreamExt;
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full, StreamBody, combinators::BoxBody};
use kube::Client;
use kube::client::Body;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{ClusterInfo, Error, ResourceInfo, ResourceRef, Result};

const FORMAT: &str = "kubri-snapshot";
const VERSION: u32 = 1;
/// The file name extension; the file is gzipped JSON.
pub const EXTENSION: &str = "kubri";

const TABLE_ACCEPT: &str = "application/json;as=Table;v=v1;g=meta.k8s.io,\
                            application/json;as=Table;v=v1beta1;g=meta.k8s.io,application/json";
/// Objects kept per resource type; the rest are left out and the snapshot says so.
const MAX_OBJECTS: usize = 5000;
/// Events kept, newest first: busy clusters have many and they repeat.
const MAX_EVENTS: usize = 2000;
/// Containers whose recent log lines are kept.
const MAX_LOGS: usize = 300;
const LOG_LINES: u32 = 500;
const LOG_BYTES: u32 = 256 * 1024;
/// Requests to the cluster at once while taking a snapshot.
const PARALLEL: usize = 8;
/// A snapshot larger than this when unpacked is refused, as a damaged or hostile file.
const MAX_UNPACKED: u64 = 2 << 30;
const READ_VERBS: [&str; 3] = ["get", "list", "watch"];
/// Cluster-wide types a namespace snapshot keeps too, since its objects refer to them.
const CLUSTER_KINDS: [(&str, &str); 10] = [
    ("", "nodes"),
    ("", "persistentvolumes"),
    ("storage.k8s.io", "storageclasses"),
    ("networking.k8s.io", "ingressclasses"),
    ("scheduling.k8s.io", "priorityclasses"),
    ("node.k8s.io", "runtimeclasses"),
    ("apiextensions.k8s.io", "customresourcedefinitions"),
    ("rbac.authorization.k8s.io", "clusterroles"),
    ("rbac.authorization.k8s.io", "clusterrolebindings"),
    ("metrics.k8s.io", "nodes"),
];

/// What a snapshot holds, shown wherever Kubri shows the cluster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    /// The context it was taken from.
    pub context: String,
    pub server: String,
    /// The cluster's Kubernetes version.
    pub kubernetes: String,
    /// The namespace saved; `None` for the whole cluster.
    pub namespace: Option<String>,
    /// When it was taken, in milliseconds since 1970.
    pub taken: i64,
    /// The Kubri version that took it.
    pub kubri: String,
    pub objects: usize,
    /// Containers whose recent log lines it holds.
    pub logs: usize,
    /// What it does not hold and why, such as `secrets: forbidden`.
    pub left_out: Vec<String>,
    /// The file it was opened from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// The file's contents.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    format: String,
    version: u32,
    pub(crate) info: SnapshotInfo,
    pub(crate) resources: Vec<ResourceInfo>,
    /// By resource, as [`ResourceRef`] shows it.
    lists: BTreeMap<String, Stored>,
    /// Log text with timestamps, by `namespace/pod/container`.
    logs: BTreeMap<String, String>,
}

/// One resource type's objects, with the server's table columns and cells where it had them.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    columns: Option<Value>,
    items: Vec<Item>,
    /// There were more than [`MAX_OBJECTS`].
    #[serde(skip)]
    cut: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Item {
    object: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cells: Option<Value>,
}

/// A file name for a snapshot: `prod-shop-20261029-1403.kubri`.
pub(crate) fn file_name(context: &str, namespace: Option<&str>, taken: i64) -> String {
    let stamp = k8s_openapi::jiff::Timestamp::from_millisecond(taken)
        .map(|t| t.strftime("%Y%m%d-%H%M").to_string())
        .unwrap_or_default();
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    format!(
        "{}-{}-{stamp}.{EXTENSION}",
        clean(context),
        clean(namespace.unwrap_or("cluster"))
    )
}

/// Saves `namespace`, or the whole cluster, as the gzipped bytes of a snapshot file.
pub(crate) async fn take(
    client: &Client,
    cluster: &ClusterInfo,
    resources: &[ResourceInfo],
    namespace: Option<&str>,
) -> Result<Vec<u8>> {
    let taken = k8s_openapi::jiff::Timestamp::now().as_millisecond();
    let chosen: Vec<ResourceInfo> = resources
        .iter()
        .filter(|r| r.can("list") && wanted(r, namespace))
        .cloned()
        .collect();
    let lists: Vec<(ResourceInfo, std::result::Result<Stored, String>)> =
        futures::stream::iter(chosen.into_iter().map(|resource| {
            let client = client.clone();
            let namespace = namespace.map(str::to_owned);
            async move {
                let result = list(&client, &resource, namespace.as_deref()).await;
                (resource, result)
            }
        }))
        .buffer_unordered(PARALLEL)
        .collect()
        .await;

    let mut left_out = Vec::new();
    let mut stored = BTreeMap::new();
    let mut kept = Vec::new();
    for (resource, result) in lists {
        match result {
            Ok(list) => {
                if list.cut {
                    left_out.push(format!(
                        "{}: only the first {MAX_OBJECTS}",
                        resource.reference.plural
                    ));
                }
                stored.insert(resource.reference.to_string(), list);
                kept.push(resource);
            }
            Err(err) => left_out.push(format!("{}: {err}", resource.reference.plural)),
        }
    }
    if let Some(events) = stored.get_mut("v1/events")
        && events.items.len() > MAX_EVENTS
    {
        events
            .items
            .sort_by_key(|i| std::cmp::Reverse(event_time(&i.object)));
        events.items.truncate(MAX_EVENTS);
        left_out.push(format!("events: only the newest {MAX_EVENTS}"));
    }
    kept.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(a.reference.group.cmp(&b.reference.group))
    });

    let pods = stored
        .get("v1/pods")
        .map(|l| log_sources(&l.items))
        .unwrap_or_default();
    if pods.len() > MAX_LOGS {
        left_out.push(format!("logs: only those of {MAX_LOGS} containers"));
    }
    let logs: BTreeMap<String, String> =
        futures::stream::iter(pods.into_iter().take(MAX_LOGS).map(|(ns, pod, container)| {
            let client = client.clone();
            async move {
                let text = log(&client, &ns, &pod, &container).await;
                (format!("{ns}/{pod}/{container}"), text)
            }
        }))
        .buffer_unordered(PARALLEL)
        .filter_map(|(key, text)| async move { text.map(|t| (key, t)) })
        .collect()
        .await;

    left_out.sort();
    let snapshot = Snapshot {
        format: FORMAT.into(),
        version: VERSION,
        info: SnapshotInfo {
            context: cluster.context.clone(),
            server: cluster.server.clone(),
            kubernetes: cluster.version.clone(),
            namespace: namespace.map(str::to_owned),
            taken,
            kubri: env!("CARGO_PKG_VERSION").into(),
            objects: stored.values().map(|l| l.items.len()).sum(),
            logs: logs.len(),
            left_out,
            file: None,
        },
        resources: kept,
        lists: stored,
        logs,
    };
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    serde_json::to_writer(&mut gzip, &snapshot)?;
    Ok(gzip.finish()?)
}

/// Whether a namespace snapshot keeps this type; a cluster snapshot keeps every type.
fn wanted(resource: &ResourceInfo, namespace: Option<&str>) -> bool {
    let r = &resource.reference;
    // The same events as v1 events, twice the size.
    if r.group == "events.k8s.io" {
        return false;
    }
    match namespace {
        None => true,
        Some(_) => {
            resource.namespaced
                || (r.group.is_empty() && r.plural == "namespaces")
                || CLUSTER_KINDS.contains(&(r.group.as_str(), r.plural.as_str()))
        }
    }
}

fn collection_url(resource: &ResourceRef, namespace: Option<&str>) -> String {
    let mut url = if resource.group.is_empty() {
        format!("/api/{}", resource.version)
    } else {
        format!("/apis/{}/{}", resource.group, resource.version)
    };
    if let Some(ns) = namespace {
        url.push_str(&format!("/namespaces/{ns}"));
    }
    url.push('/');
    url.push_str(&resource.plural);
    url
}

/// Lists one type page by page, as a Table where the server renders one.
async fn list(
    client: &Client,
    resource: &ResourceInfo,
    namespace: Option<&str>,
) -> std::result::Result<Stored, String> {
    let r = &resource.reference;
    let is_namespaces = r.group.is_empty() && r.plural == "namespaces";
    let url = collection_url(r, namespace.filter(|_| resource.namespaced));
    let mut stored = Stored::default();
    let mut next = String::new();
    loop {
        let mut query = "limit=500&includeObject=Object".to_owned();
        if !next.is_empty() {
            query.push_str(&format!("&continue={}", encode(&next)));
        }
        // A namespace snapshot keeps its own namespace only.
        if let (true, Some(ns)) = (is_namespaces, namespace) {
            query.push_str(&format!(
                "&fieldSelector={}",
                encode(&format!("metadata.name={ns}"))
            ));
        }
        let request = http::Request::get(format!("{url}?{query}"))
            .header(http::header::ACCEPT, TABLE_ACCEPT)
            .body(Vec::new())
            .map_err(|e| e.to_string())?;
        let page: Value = client.request(request).await.map_err(|e| match e {
            kube::Error::Api(status) if status.code == 403 => "forbidden".to_owned(),
            kube::Error::Api(status) if status.code == 404 || status.code == 405 => "not listable".to_owned(),
            e => e.to_string(),
        })?;
        next = add_page(&mut stored, page, resource);
        if next.is_empty() {
            break;
        }
        if stored.items.len() >= MAX_OBJECTS {
            stored.cut = true;
            break;
        }
    }
    stored.cut |= stored.items.len() > MAX_OBJECTS;
    stored.items.truncate(MAX_OBJECTS);
    Ok(stored)
}

/// Adds a page of a list and returns the token for the next one, empty after the last.
fn add_page(stored: &mut Stored, mut page: Value, resource: &ResourceInfo) -> String {
    let api_version = resource.api_resource().api_version;
    let token = page
        .pointer("/metadata/continue")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let is_table = page.get("kind").and_then(Value::as_str) == Some("Table");
    let mut take = |key: &str| match page.get_mut(key).map(Value::take) {
        Some(Value::Array(values)) => values,
        _ => Vec::new(),
    };
    let mut items: Vec<Item> = if is_table {
        stored.columns = Some(Value::Array(take("columnDefinitions")));
        take("rows")
            .into_iter()
            .map(|mut row| Item {
                object: row.get_mut("object").map(Value::take).unwrap_or(Value::Null),
                cells: row.get_mut("cells").map(Value::take),
            })
            .collect()
    } else {
        take("items")
            .into_iter()
            .map(|object| Item { object, cells: None })
            .collect()
    };
    for item in &mut items {
        clean(&mut item.object, &resource.kind, &api_version);
    }
    stored.items.extend(items);
    token
}

/// Drops what a snapshot must not or need not hold: Secrets' values and managed fields.
fn clean(object: &mut Value, kind: &str, api_version: &str) {
    let Some(map) = object.as_object_mut() else {
        return;
    };
    map.entry("kind").or_insert_with(|| kind.into());
    map.entry("apiVersion").or_insert_with(|| api_version.into());
    if let Some(metadata) = map.get_mut("metadata").and_then(Value::as_object_mut) {
        metadata.remove("managedFields");
    }
    if kind == "Secret" && api_version == "v1" {
        // Helm keeps its releases in Secrets; their summary stays, so the Helm page works.
        let helm = map.get("type").and_then(Value::as_str) == Some("helm.sh/release.v1");
        let release = helm
            .then(|| {
                map.get("data")
                    .and_then(|d| d.get("release"))
                    .and_then(Value::as_str)
            })
            .flatten()
            .and_then(helm_summary);
        for field in ["data", "stringData"] {
            if let Some(values) = map.get_mut(field).and_then(Value::as_object_mut) {
                for value in values.values_mut() {
                    *value = Value::String(String::new());
                }
            }
        }
        // kubectl apply keeps the whole Secret, values included, in an annotation.
        if let Some(annotations) = map
            .get_mut("metadata")
            .and_then(|m| m.get_mut("annotations"))
            .and_then(Value::as_object_mut)
        {
            annotations.remove("kubectl.kubernetes.io/last-applied-configuration");
        }
        if let (Some(release), Some(data)) = (release, map.get_mut("data").and_then(Value::as_object_mut)) {
            data.insert("release".into(), Value::String(release));
        }
    }
}

/// A Helm release Secret's value with only what the release list shows: name, revision,
/// status, times and the chart's name and version. Values, notes and manifests, which can
/// hold secrets, are left out.
fn helm_summary(value: &str) -> Option<String> {
    use base64::Engine;
    let base64 = base64::engine::general_purpose::STANDARD;
    let stored = base64.decode(value).ok()?;
    let release: Value = crate::helm::decode_as(&stored).ok()?;
    let info = release.get("info").cloned().unwrap_or_default();
    let keep = |from: &Value, keys: &[&str]| -> Value {
        Value::Object(
            keys.iter()
                .filter_map(|k| from.get(*k).map(|v| ((*k).to_owned(), v.clone())))
                .collect(),
        )
    };
    let summary = json!({
        "name": release.get("name"),
        "namespace": release.get("namespace"),
        "version": release.get("version"),
        "info": keep(&info, &["status", "first_deployed", "last_deployed", "deleted", "description"]),
        "chart": { "metadata": release.pointer("/chart/metadata") },
    });
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gzip, summary.to_string().as_bytes()).ok()?;
    Some(base64.encode(base64.encode(gzip.finish().ok()?)))
}

fn event_time(event: &Value) -> String {
    [
        "/lastTimestamp",
        "/eventTime",
        "/series/lastObservedTime",
        "/metadata/creationTimestamp",
    ]
    .iter()
    .find_map(|p| event.pointer(p).and_then(Value::as_str))
    .unwrap_or_default()
    .to_owned()
}

/// The containers that have run, as `(namespace, pod, container)`.
fn log_sources(pods: &[Item]) -> Vec<(String, String, String)> {
    let mut sources = Vec::new();
    for pod in pods {
        let pod = &pod.object;
        let text = |p: &str| {
            pod.pointer(p)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let (ns, name) = (text("/metadata/namespace"), text("/metadata/name"));
        for field in ["initContainerStatuses", "containerStatuses"] {
            for status in pod
                .pointer(&format!("/status/{field}"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let ran = ["/state/running", "/state/terminated", "/lastState/terminated"]
                    .iter()
                    .any(|p| status.pointer(p).is_some());
                if let (true, Some(container)) = (ran, status.get("name").and_then(Value::as_str)) {
                    sources.push((ns.clone(), name.clone(), container.to_owned()));
                }
            }
        }
    }
    sources
}

async fn log(client: &Client, namespace: &str, pod: &str, container: &str) -> Option<String> {
    let url = format!(
        "/api/v1/namespaces/{namespace}/pods/{pod}/log?container={}&timestamps=true&tailLines={LOG_LINES}&limitBytes={LOG_BYTES}",
        encode(container)
    );
    let request = http::Request::get(url).body(Vec::new()).ok()?;
    let text = client.request_text(request).await.ok()?;
    // A cut-off last line is dropped rather than shown half.
    let complete = match text.rfind('\n') {
        Some(end) => &text[..=end],
        None => "",
    };
    (!complete.is_empty()).then(|| complete.to_owned())
}

/// Reads a snapshot file: gzipped JSON, or plain JSON someone unpacked.
pub(crate) fn read(bytes: &[u8]) -> Result<Snapshot> {
    let invalid = |why: String| Error::Unsupported(format!("this is not a Kubri snapshot: {why}"));
    let mut text = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bytes)
            .take(MAX_UNPACKED)
            .read_to_end(&mut text)
            .map_err(|e| invalid(e.to_string()))?;
    } else {
        text.extend_from_slice(bytes);
    }
    let head: Value = serde_json::from_slice::<Map<String, Value>>(&text)
        .map(|m| {
            Value::Object(
                m.into_iter()
                    .filter(|(k, _)| k == "format" || k == "version")
                    .collect(),
            )
        })
        .map_err(|e| invalid(e.to_string()))?;
    if head.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(invalid("it has no kubri-snapshot format field".into()));
    }
    let version = head.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version > u64::from(VERSION) {
        return Err(Error::Unsupported(format!(
            "this snapshot was taken by a newer Kubri (format {version}); update Kubri to open it"
        )));
    }
    serde_json::from_slice(&text).map_err(|e| invalid(e.to_string()))
}

impl Snapshot {
    /// The saved types as a snapshot serves them: readable, with no subresource that acts.
    pub(crate) fn served_resources(&self) -> Vec<ResourceInfo> {
        let mut resources = self.resources.clone();
        for r in &mut resources {
            r.verbs.retain(|v| READ_VERBS.contains(&v.as_str()));
            r.subresources.retain(|s| s == "log" || s == "status");
        }
        resources
    }
}

type Reply = Response<BoxBody<Bytes, Infallible>>;

/// A client whose API server is the snapshot.
pub(crate) fn client(snapshot: Snapshot, default_namespace: &str) -> Client {
    let server = Arc::new(Server::new(snapshot));
    let service = tower::service_fn(move |request: Request<Body>| {
        let server = Arc::clone(&server);
        async move { Ok::<_, Infallible>(server.answer(request).await) }
    });
    Client::new(service, default_namespace)
}

/// Answers API requests from a snapshot.
struct Server {
    info: SnapshotInfo,
    resources: Vec<ResourceInfo>,
    /// By group and plural: a client may ask for another version than the one saved.
    lists: HashMap<(String, String), (ResourceInfo, Stored)>,
    logs: BTreeMap<String, String>,
}

/// What a request path names.
#[derive(Debug, PartialEq)]
struct Target<'a> {
    group: &'a str,
    version: &'a str,
    namespace: Option<&'a str>,
    plural: Option<&'a str>,
    name: Option<&'a str>,
    subresource: Option<&'a str>,
}

impl Server {
    fn new(mut snapshot: Snapshot) -> Self {
        let resources = snapshot.served_resources();
        let mut lists = HashMap::new();
        for resource in &resources {
            let stored = snapshot
                .lists
                .remove(&resource.reference.to_string())
                .unwrap_or_default();
            let key = (
                resource.reference.group.clone(),
                resource.reference.plural.clone(),
            );
            lists.insert(key, (resource.clone(), stored));
        }
        Self {
            info: snapshot.info,
            resources,
            lists,
            logs: snapshot.logs,
        }
    }

    async fn answer(&self, request: Request<Body>) -> Reply {
        let (parts, body) = request.into_parts();
        let path = parts.uri.path().to_owned();
        let query = parse_query(parts.uri.query().unwrap_or_default());
        let accept = parts
            .headers
            .get(http::header::ACCEPT)
            .and_then(|a| a.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        if parts.method == Method::POST && path.starts_with("/apis/authorization.k8s.io/") {
            let body = body.collect().await.map(|b| b.to_bytes()).unwrap_or_default();
            return self.review(&path, &body);
        }
        if parts.method != Method::GET {
            return status(
                StatusCode::METHOD_NOT_ALLOWED,
                "this is a snapshot: it shows what was saved and cannot change anything",
            );
        }
        if path == "/version" {
            return self.version();
        }
        let Some(target) = parse_path(&path) else {
            return not_saved();
        };
        let Some(plural) = target.plural else {
            return self.discovery(&target);
        };
        let Some((resource, stored)) = self.lists.get(&(target.group.to_owned(), plural.to_owned())) else {
            return not_saved();
        };
        match (target.name, target.subresource) {
            (None, _) if query.get("watch").is_some_and(|w| w == "true" || w == "1") => quiet(),
            (None, _) => list_reply(resource, stored, target.namespace, &query, &accept),
            (Some(name), None | Some("status")) => match find(stored, target.namespace, name) {
                Some(item) if accept.contains("as=PartialObjectMetadata") => {
                    json_reply(StatusCode::OK, &partial(&item.object))
                }
                Some(item) => json_reply(StatusCode::OK, &item.object),
                None => status(
                    StatusCode::NOT_FOUND,
                    &format!("{plural} \"{name}\" is not in the snapshot"),
                ),
            },
            (Some(pod), Some("log")) if target.group.is_empty() && plural == "pods" => {
                self.log(target.namespace.unwrap_or_default(), pod, &query)
            }
            _ => not_saved(),
        }
    }

    /// Answers permission reviews: a snapshot may be read, never changed.
    fn review(&self, path: &str, body: &[u8]) -> Reply {
        let review: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let spec = review.get("spec").cloned().unwrap_or(Value::Null);
        let reason = "a snapshot can be read but not changed";
        let answer = if path.ends_with("/selfsubjectaccessreviews") {
            let verb = spec
                .pointer("/resourceAttributes/verb")
                .or_else(|| spec.pointer("/nonResourceAttributes/verb"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            json!({
                "apiVersion": "authorization.k8s.io/v1", "kind": "SelfSubjectAccessReview",
                "metadata": {}, "spec": spec,
                "status": { "allowed": READ_VERBS.contains(&verb), "reason": reason }
            })
        } else if path.ends_with("/selfsubjectrulesreviews") {
            json!({
                "apiVersion": "authorization.k8s.io/v1", "kind": "SelfSubjectRulesReview",
                "metadata": {}, "spec": spec,
                "status": {
                    "resourceRules": [{ "verbs": READ_VERBS, "apiGroups": ["*"], "resources": ["*"] }],
                    "nonResourceRules": [], "incomplete": false
                }
            })
        } else {
            return not_saved();
        };
        json_reply(StatusCode::CREATED, &answer)
    }

    fn version(&self) -> Reply {
        let v = self.info.kubernetes.trim_start_matches('v');
        let mut parts = v.split('.');
        let major = parts.next().unwrap_or_default();
        let minor: String = parts
            .next()
            .unwrap_or_default()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        json_reply(
            StatusCode::OK,
            &json!({
                "major": major, "minor": minor, "gitVersion": self.info.kubernetes,
                "gitCommit": "", "gitTreeState": "", "buildDate": "", "goVersion": "",
                "compiler": "", "platform": "snapshot"
            }),
        )
    }

    /// `/api`, `/apis` and the resource lists of each group-version.
    fn discovery(&self, target: &Target) -> Reply {
        match (target.group, target.version) {
            ("", "") => json_reply(
                StatusCode::OK,
                &json!({ "kind": "APIVersions", "versions": ["v1"], "serverAddressByClientCIDRs": [] }),
            ),
            ("*", "") => {
                let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
                for r in self
                    .resources
                    .iter()
                    .map(|r| &r.reference)
                    .filter(|r| !r.group.is_empty())
                {
                    let versions = groups.entry(&r.group).or_default();
                    if !versions.contains(&r.version.as_str()) {
                        versions.push(&r.version);
                    }
                }
                let groups: Vec<Value> = groups
                    .into_iter()
                    .map(|(name, versions)| {
                        let versions: Vec<Value> = versions
                            .iter()
                            .map(|v| json!({ "groupVersion": format!("{name}/{v}"), "version": v }))
                            .collect();
                        json!({ "name": name, "versions": versions, "preferredVersion": versions[0] })
                    })
                    .collect();
                json_reply(
                    StatusCode::OK,
                    &json!({ "kind": "APIGroupList", "apiVersion": "v1", "groups": groups }),
                )
            }
            (group, version) => {
                let mut resources = Vec::new();
                for r in self
                    .resources
                    .iter()
                    .filter(|r| r.reference.group == group && r.reference.version == version)
                {
                    resources.push(json!({
                        "name": r.reference.plural, "singularName": r.singular, "namespaced": r.namespaced,
                        "kind": r.kind, "verbs": r.verbs, "shortNames": r.short_names
                    }));
                    for sub in &r.subresources {
                        resources.push(json!({
                            "name": format!("{}/{sub}", r.reference.plural), "singularName": "",
                            "namespaced": r.namespaced, "kind": r.kind, "verbs": ["get"]
                        }));
                    }
                }
                if resources.is_empty() {
                    return not_saved();
                }
                let group_version = if group.is_empty() {
                    version.to_owned()
                } else {
                    format!("{group}/{version}")
                };
                json_reply(
                    StatusCode::OK,
                    &json!({ "kind": "APIResourceList", "apiVersion": "v1", "groupVersion": group_version, "resources": resources }),
                )
            }
        }
    }

    /// The saved log lines of a container; a follow stays open without new lines.
    fn log(&self, namespace: &str, pod: &str, query: &HashMap<String, String>) -> Reply {
        if query.get("previous").is_some_and(|p| p == "true") {
            return status(
                StatusCode::BAD_REQUEST,
                "a snapshot holds no logs of previous containers",
            );
        }
        let prefix = format!("{namespace}/{pod}/");
        let text = match query.get("container") {
            Some(container) => self.logs.get(&format!("{prefix}{container}")),
            None => self
                .logs
                .range(prefix.clone()..)
                .next()
                .filter(|(k, _)| k.starts_with(&prefix))
                .map(|(_, t)| t),
        };
        let Some(text) = text else {
            return status(
                StatusCode::BAD_REQUEST,
                &format!("the snapshot holds no log lines of {pod}"),
            );
        };
        let since = query
            .get("sinceSeconds")
            .and_then(|s| s.parse::<i64>().ok())
            .map(|s| self.info.taken - s * 1000);
        let mut lines: Vec<&str> = text
            .lines()
            .filter(|line| match since {
                Some(since) => line_time(line).is_none_or(|t| t >= since),
                None => true,
            })
            .collect();
        if let Some(tail) = query.get("tailLines").and_then(|t| t.parse::<usize>().ok()) {
            lines.drain(..lines.len().saturating_sub(tail));
        }
        let timestamps = query.get("timestamps").is_some_and(|t| t == "true");
        let mut out = String::new();
        for line in lines {
            let line = if timestamps {
                line
            } else {
                line.split_once(' ').map_or(line, |(_, rest)| rest)
            };
            out.push_str(line);
            out.push('\n');
        }
        let body = Bytes::from(out);
        if query.get("follow").is_some_and(|f| f == "true") {
            let stream = futures::stream::once(async move { Ok(http_body::Frame::data(body)) })
                .chain(futures::stream::pending());
            return reply(
                StatusCode::OK,
                "text/plain",
                BodyExt::boxed(StreamBody::new(stream)),
            );
        }
        reply(StatusCode::OK, "text/plain", Full::new(body).boxed())
    }
}

fn line_time(line: &str) -> Option<i64> {
    let (stamp, _) = line.split_once(' ')?;
    stamp
        .parse::<k8s_openapi::jiff::Timestamp>()
        .ok()
        .map(|t| t.as_millisecond())
}

fn parse_path(path: &str) -> Option<Target<'_>> {
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let (group, version, rest) = match parts.as_slice() {
        ["api"] => ("", "", &[][..]),
        ["apis"] => ("*", "", &[][..]),
        ["api", version, rest @ ..] => ("", *version, rest),
        ["apis", group, version, rest @ ..] => (*group, *version, rest),
        _ => return None,
    };
    let (namespace, rest) = match rest {
        ["namespaces", ns, rest @ ..] if !rest.is_empty() => (Some(*ns), rest),
        rest => (None, rest),
    };
    if rest.len() > 3 {
        return None;
    }
    Some(Target {
        group,
        version,
        namespace,
        plural: rest.first().copied(),
        name: rest.get(1).copied(),
        subresource: rest.get(2).copied(),
    })
}

fn find<'a>(stored: &'a Stored, namespace: Option<&str>, name: &str) -> Option<&'a Item> {
    stored.items.iter().find(|item| {
        let metadata = item.object.get("metadata");
        let text = |key: &str| metadata.and_then(|m| m.get(key)).and_then(Value::as_str);
        text("name") == Some(name) && text("namespace") == namespace
    })
}

fn list_reply(
    resource: &ResourceInfo,
    stored: &Stored,
    namespace: Option<&str>,
    query: &HashMap<String, String>,
    accept: &str,
) -> Reply {
    let labels = query.get("labelSelector").map(String::as_str).unwrap_or_default();
    let fields = query.get("fieldSelector").map(String::as_str).unwrap_or_default();
    let items = stored.items.iter().filter(|item| {
        let object = &item.object;
        (namespace.is_none() || object.pointer("/metadata/namespace").and_then(Value::as_str) == namespace)
            && labels_match(labels, object.pointer("/metadata/labels"))
            && fields_match(fields, object)
    });
    let metadata = json!({ "resourceVersion": "1" });
    let api_version = resource.api_resource().api_version;
    let body = if accept.contains("as=Table") {
        let full = query.get("includeObject").is_some_and(|i| i == "Object");
        match &stored.columns {
            Some(definitions) => {
                let rows: Vec<Value> = items
                    .map(|item| {
                        let object = if full {
                            item.object.clone()
                        } else {
                            partial(&item.object)
                        };
                        json!({ "cells": item.cells.clone().unwrap_or_default(), "object": object })
                    })
                    .collect();
                json!({
                    "kind": "Table", "apiVersion": "meta.k8s.io/v1", "metadata": metadata,
                    "columnDefinitions": definitions, "rows": rows
                })
            }
            // The server had no table for this type; the UI falls back to Name and Age.
            None => plain_list(resource, &api_version, metadata, items),
        }
    } else if accept.contains("as=PartialObjectMetadataList") {
        let items: Vec<Value> = items.map(|i| partial(&i.object)).collect();
        json!({ "kind": "PartialObjectMetadataList", "apiVersion": "meta.k8s.io/v1", "metadata": metadata, "items": items })
    } else {
        plain_list(resource, &api_version, metadata, items)
    };
    json_reply(StatusCode::OK, &body)
}

fn plain_list<'a>(
    resource: &ResourceInfo,
    api_version: &str,
    metadata: Value,
    items: impl Iterator<Item = &'a Item>,
) -> Value {
    let items: Vec<&Value> = items.map(|i| &i.object).collect();
    json!({ "kind": format!("{}List", resource.kind), "apiVersion": api_version, "metadata": metadata, "items": items })
}

fn partial(object: &Value) -> Value {
    json!({
        "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata",
        "metadata": object.get("metadata").cloned().unwrap_or_else(|| json!({}))
    })
}

/// Whether labels match a `labelSelector`: `a=b`, `a!=b`, `a in (b,c)`, `a notin (b)`, `a`, `!a`.
fn labels_match(selector: &str, labels: Option<&Value>) -> bool {
    let label = |key: &str| labels.and_then(|l| l.get(key)).and_then(Value::as_str);
    split_terms(selector).into_iter().all(|term| {
        let term = term.trim();
        if term.is_empty() {
            return true;
        }
        if let Some(key) = term.strip_prefix('!') {
            return label(key.trim()).is_none();
        }
        for (op, negate) in [(" notin ", true), (" in ", false)] {
            if let Some((key, set)) = term.split_once(op) {
                let set: Vec<&str> = set
                    .trim()
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .split(',')
                    .map(str::trim)
                    .collect();
                let found = label(key.trim()).is_some_and(|v| set.contains(&v));
                return found != negate;
            }
        }
        if let Some((key, value)) = term.split_once("!=") {
            return label(key.trim()) != Some(value.trim());
        }
        if let Some((key, value)) = term.split_once("==").or_else(|| term.split_once('=')) {
            return label(key.trim()) == Some(value.trim());
        }
        label(term).is_some()
    })
}

/// Splits a selector at the commas outside `( )`.
fn split_terms(selector: &str) -> Vec<&str> {
    let mut terms = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (i, c) in selector.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                terms.push(&selector[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    terms.push(&selector[start..]);
    terms
}

/// Whether an object matches a `fieldSelector` such as `spec.nodeName=n1,status.phase!=Running`.
fn fields_match(selector: &str, object: &Value) -> bool {
    selector
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .all(|term| {
            let (path, value, equal) = match term.split_once("!=") {
                Some((path, value)) => (path, value, false),
                None => match term.split_once("==").or_else(|| term.split_once('=')) {
                    Some((path, value)) => (path, value, true),
                    None => return true,
                },
            };
            let pointer = format!("/{}", path.trim().replace('.', "/"));
            let actual = match object.pointer(&pointer) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            (actual == value.trim()) == equal
        })
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (!key.is_empty()).then(|| (decode(key), decode(value)))
        })
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[i + 1..i + 3]).ok())
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[i], escaped) {
            (_, Some(b)) => {
                out.push(b);
                i += 2;
            }
            (b'+', _) => out.push(b' '),
            (b, _) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect()
}

fn reply(code: StatusCode, content_type: &'static str, body: BoxBody<Bytes, Infallible>) -> Reply {
    let mut response = Response::new(body);
    *response.status_mut() = code;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(content_type),
    );
    response
}

fn json_reply(code: StatusCode, value: &Value) -> Reply {
    reply(
        code,
        "application/json",
        Full::new(Bytes::from(value.to_string())).boxed(),
    )
}

fn status(code: StatusCode, message: &str) -> Reply {
    json_reply(
        code,
        &json!({
            "kind": "Status", "apiVersion": "v1", "metadata": {}, "status": "Failure",
            "message": message, "reason": "", "code": code.as_u16()
        }),
    )
}

fn not_saved() -> Reply {
    status(StatusCode::NOT_FOUND, "this is not in the snapshot")
}

/// A watch that never reports a change: nothing changes in a snapshot.
fn quiet() -> Reply {
    let stream = futures::stream::pending::<std::result::Result<http_body::Frame<Bytes>, Infallible>>();
    reply(
        StatusCode::OK,
        "application/json",
        BodyExt::boxed(StreamBody::new(stream)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::Pod;
    use kube::api::{Api, ListParams, PostParams};

    fn resource(group: &str, version: &str, plural: &str, kind: &str, namespaced: bool) -> ResourceInfo {
        ResourceInfo {
            reference: ResourceRef {
                group: group.into(),
                version: version.into(),
                plural: plural.into(),
            },
            kind: kind.into(),
            singular: kind.to_lowercase(),
            short_names: Vec::new(),
            namespaced,
            verbs: ["get", "list", "watch", "create", "delete"]
                .map(String::from)
                .to_vec(),
            subresources: ["log", "exec", "status"].map(String::from).to_vec(),
        }
    }

    fn pod(namespace: &str, name: &str, app: &str, node: &str) -> Item {
        Item {
            object: json!({
                "apiVersion": "v1", "kind": "Pod",
                "metadata": { "name": name, "namespace": namespace, "labels": { "app": app } },
                "spec": { "nodeName": node, "containers": [{ "name": "main", "image": "nginx" }] },
                "status": { "phase": "Running" }
            }),
            cells: Some(json!([name, "1/1", "Running"])),
        }
    }

    fn snapshot() -> Snapshot {
        let mut lists = BTreeMap::new();
        lists.insert(
            "v1/pods".to_owned(),
            Stored {
                columns: Some(json!([{ "name": "Name", "type": "string", "format": "name" }, { "name": "Ready" }, { "name": "Status" }])),
                items: vec![pod("shop", "web-1", "web", "n1"), pod("shop", "db-1", "db", "n2"), pod("other", "web-2", "web", "n1")],
                cut: false,
            },
        );
        let mut logs = BTreeMap::new();
        logs.insert(
            "shop/web-1/main".to_owned(),
            "2026-10-29T10:00:00Z first\n2026-10-29T10:59:00Z second\n2026-10-29T11:00:00Z third\n"
                .to_owned(),
        );
        Snapshot {
            format: FORMAT.into(),
            version: VERSION,
            info: SnapshotInfo {
                context: "prod".into(),
                server: "https://prod".into(),
                kubernetes: "v1.33.5".into(),
                namespace: None,
                taken: "2026-10-29T11:00:00Z"
                    .parse::<k8s_openapi::jiff::Timestamp>()
                    .unwrap()
                    .as_millisecond(),
                kubri: "0.9.0".into(),
                objects: 3,
                logs: 1,
                left_out: Vec::new(),
                file: None,
            },
            resources: vec![resource("", "v1", "pods", "Pod", true)],
            lists,
            logs,
        }
    }

    fn pack(snapshot: &Snapshot) -> Vec<u8> {
        use std::io::Write;
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&serde_json::to_vec(snapshot).unwrap()).unwrap();
        gzip.finish().unwrap()
    }

    #[test]
    fn reads_its_own_files_and_refuses_others() {
        let read = read(&pack(&snapshot())).unwrap();
        assert_eq!(read.info.context, "prod");
        assert_eq!(read.lists["v1/pods"].items.len(), 3);
        // Unpacked JSON opens too.
        assert!(super::read(&serde_json::to_vec(&snapshot()).unwrap()).is_ok());

        let err = super::read(b"{\"kind\": \"Pod\"}").err().unwrap().to_string();
        assert!(err.contains("not a Kubri snapshot"), "{err}");
        let mut newer = serde_json::to_value(snapshot()).unwrap();
        newer["version"] = json!(VERSION + 1);
        let err = super::read(newer.to_string().as_bytes())
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("newer Kubri"), "{err}");
    }

    #[test]
    fn serves_only_reading() {
        let served = snapshot().served_resources();
        assert_eq!(served[0].verbs, ["get", "list", "watch"]);
        assert_eq!(served[0].subresources, ["log", "status"]);
    }

    #[test]
    fn keeps_secret_keys_but_not_values() {
        let mut secret = json!({
            "metadata": { "name": "db", "managedFields": [{}], "annotations": {
                "kubectl.kubernetes.io/last-applied-configuration": "{\"data\":{\"password\":\"aHVudGVyMg==\"}}",
                "team": "shop"
            } },
            "type": "Opaque",
            "data": { "password": "aHVudGVyMg==" },
            "stringData": { "user": "admin" }
        });
        clean(&mut secret, "Secret", "v1");
        assert_eq!(secret["kind"], "Secret");
        assert_eq!(secret["apiVersion"], "v1");
        assert_eq!(secret["data"], json!({ "password": "" }));
        assert_eq!(secret["stringData"], json!({ "user": "" }));
        assert_eq!(secret["metadata"]["annotations"], json!({ "team": "shop" }));
        assert!(secret["metadata"].get("managedFields").is_none());

        // A ConfigMap keeps its data.
        let mut map = json!({ "metadata": { "name": "app" }, "data": { "mode": "fast" } });
        clean(&mut map, "ConfigMap", "v1");
        assert_eq!(map["data"]["mode"], "fast");
    }

    #[test]
    fn keeps_only_the_summary_of_helm_releases() {
        use base64::Engine;
        let base64 = base64::engine::general_purpose::STANDARD;
        let release = json!({
            "name": "shop", "namespace": "apps", "version": 3,
            "info": { "status": "deployed", "last_deployed": "2026-10-28T07:00:00Z", "notes": "password: hunter2" },
            "chart": { "metadata": { "name": "shop", "version": "1.2.0" }, "values": { "password": "default" } },
            "config": { "password": "hunter2" },
            "manifest": "kind: Secret\ndata:\n  password: aHVudGVyMg==\n"
        });
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gzip, release.to_string().as_bytes()).unwrap();
        let stored = base64.encode(base64.encode(gzip.finish().unwrap()));
        let mut secret = json!({
            "metadata": { "name": "sh.helm.release.v1.shop.v3" },
            "type": "helm.sh/release.v1",
            "data": { "release": stored }
        });
        clean(&mut secret, "Secret", "v1");
        let kept = secret["data"]["release"].as_str().unwrap();
        let summary: Value = crate::helm::decode_as(&base64.decode(kept).unwrap()).unwrap();
        assert_eq!(summary["version"], 3);
        assert_eq!(summary["info"]["status"], "deployed");
        assert_eq!(summary["chart"]["metadata"]["version"], "1.2.0");
        assert!(!summary.to_string().contains("hunter2"), "{summary}");
        assert!(!summary.to_string().contains("default"), "{summary}");
    }

    #[test]
    fn matches_selectors_like_the_api_server() {
        let labels = json!({ "app": "web", "tier": "front" });
        let l = Some(&labels);
        assert!(labels_match("", l));
        assert!(labels_match("app=web", l));
        assert!(labels_match("app==web,tier", l));
        assert!(labels_match("app in (web, api),!missing", l));
        assert!(labels_match("tier notin (back)", l));
        assert!(labels_match("missing!=x", l));
        assert!(!labels_match("app=api", l));
        assert!(!labels_match("app in (api,db)", l));
        assert!(!labels_match("!app", l));
        assert!(!labels_match("app,missing", l));

        let event = json!({ "type": "Warning", "involvedObject": { "uid": "u1" }, "count": 3 });
        assert!(fields_match("involvedObject.uid=u1,type=Warning", &event));
        assert!(fields_match("type!=Normal", &event));
        assert!(fields_match("count=3", &event));
        assert!(!fields_match("type=Normal", &event));
    }

    #[test]
    fn parses_api_paths() {
        let t = parse_path("/api/v1/namespaces/shop/pods/web-1/log").unwrap();
        assert_eq!(
            t,
            Target {
                group: "",
                version: "v1",
                namespace: Some("shop"),
                plural: Some("pods"),
                name: Some("web-1"),
                subresource: Some("log")
            }
        );
        let t = parse_path("/apis/apps/v1/deployments").unwrap();
        assert_eq!(
            (t.group, t.plural, t.namespace),
            ("apps", Some("deployments"), None)
        );
        // The namespaces type itself, not a namespace's objects.
        let t = parse_path("/api/v1/namespaces/shop").unwrap();
        assert_eq!(
            (t.plural, t.name, t.namespace),
            (Some("namespaces"), Some("shop"), None)
        );
        assert_eq!(parse_path("/apis").unwrap().group, "*");
        assert!(parse_path("/openapi/v3").is_none());
        assert_eq!(decode("app%3Dweb%2Ctier+x%zz"), "app=web,tier x%zz");
        assert_eq!(encode("a=b c"), "a%3Db%20c");
    }

    #[tokio::test]
    async fn answers_like_an_api_server() {
        let client = client(snapshot(), "shop");
        let pods: Api<Pod> = Api::namespaced(client.clone(), "shop");
        let names = |list: kube::core::ObjectList<Pod>| -> Vec<String> {
            list.items.into_iter().filter_map(|p| p.metadata.name).collect()
        };
        assert_eq!(
            names(pods.list(&ListParams::default()).await.unwrap()),
            ["web-1", "db-1"]
        );
        let all: Api<Pod> = Api::all(client.clone());
        assert_eq!(
            names(
                all.list(&ListParams::default().labels("app=web").fields("spec.nodeName=n1"))
                    .await
                    .unwrap()
            ),
            ["web-1", "web-2"]
        );
        assert_eq!(
            pods.get("db-1").await.unwrap().metadata.name.as_deref(),
            Some("db-1")
        );
        let missing = pods.get("gone").await.unwrap_err();
        assert!(
            matches!(missing, kube::Error::Api(ref s) if s.code == 404),
            "{missing}"
        );

        // Tables, with whole objects or metadata only.
        let table = |query: &str| {
            http::Request::get(format!("/api/v1/namespaces/shop/pods?{query}"))
                .header(http::header::ACCEPT, TABLE_ACCEPT)
                .body(Vec::new())
                .unwrap()
        };
        let full: Value = client.request(table("includeObject=Object")).await.unwrap();
        assert_eq!(full["kind"], "Table");
        assert_eq!(full["rows"][0]["cells"][2], "Running");
        assert_eq!(full["rows"][0]["object"]["spec"]["nodeName"], "n1");
        let partial: Value = client.request(table("")).await.unwrap();
        assert_eq!(partial["rows"][0]["object"]["kind"], "PartialObjectMetadata");
        assert!(partial["rows"][0]["object"].get("spec").is_none());

        // Logs: tail and since count back from when the snapshot was taken.
        let log = |query: &str| {
            http::Request::get(format!("/api/v1/namespaces/shop/pods/web-1/log?{query}"))
                .body(Vec::new())
                .unwrap()
        };
        assert_eq!(
            client
                .request_text(log("container=main&tailLines=1"))
                .await
                .unwrap(),
            "third\n"
        );
        assert_eq!(
            client.request_text(log("sinceSeconds=120")).await.unwrap(),
            "second\nthird\n"
        );
        let text = client
            .request_text(log("timestamps=true&tailLines=1"))
            .await
            .unwrap();
        assert_eq!(text, "2026-10-29T11:00:00Z third\n");
        assert!(client.request_text(log("previous=true")).await.is_err());

        // Nothing changes, and watches stay quiet.
        let created = pods
            .create(&PostParams::default(), &Pod::default())
            .await
            .unwrap_err();
        assert!(
            matches!(created, kube::Error::Api(ref s) if s.code == 405),
            "{created}"
        );
        let watch = pods.watch(&kube::api::WatchParams::default(), "1").await.unwrap();
        let mut watch = std::pin::pin!(watch);
        let next = tokio::time::timeout(std::time::Duration::from_millis(50), watch.next()).await;
        assert!(next.is_err(), "a watch should report nothing");

        // Reading is allowed, changing is not.
        let check = |verb: &str| {
            crate::AccessCheck::new(
                verb,
                &ResourceRef {
                    group: String::new(),
                    version: "v1".into(),
                    plural: "pods".into(),
                },
                Some("shop"),
            )
        };
        let cache = crate::access::AccessCache::default();
        let answers = crate::access::check_all(&client, &cache, &[check("list"), check("delete")]).await;
        assert_eq!(answers, [Some(true), Some(false)]);

        // Discovery and the version come from the file.
        assert_eq!(client.apiserver_version().await.unwrap().git_version, "v1.33.5");
        let core = client.list_core_api_resources("v1").await.unwrap();
        assert!(core.resources.iter().any(|r| r.name == "pods/log"));
        assert!(!core.resources.iter().any(|r| r.name == "pods/exec"));
    }
}
