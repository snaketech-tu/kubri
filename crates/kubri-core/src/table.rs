//! Live resource lists with the columns `kubectl get` shows.
//!
//! The API server renders these as `meta.k8s.io/v1` Tables, including the printer columns of
//! CRDs. Servers that cannot (some aggregated APIs) get a plain Name / Age fallback.

use std::time::Duration;

use futures::{Stream, StreamExt};
use kube::Client;
use kube::api::{ListParams, WatchParams};
use kube::core::{Request, WatchEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::jsonpath::JsonPath;

const TABLE_ACCEPT: &str = "application/json;as=Table;v=v1;g=meta.k8s.io,\
                            application/json;as=Table;v=v1beta1;g=meta.k8s.io,application/json";
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub description: String,
    /// 0 for the default view, higher for `-o wide`.
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// Stable key: the object's UID, or namespace/name if the server sends none.
    pub key: String,
    pub name: String,
    pub namespace: Option<String>,
    pub created: Option<String>,
    pub labels: serde_json::Map<String, Value>,
    /// The objects this one belongs to, such as the ReplicaSet of a pod.
    pub owners: Vec<Owner>,
    /// One value per column, in column order.
    pub cells: Vec<Value>,
}

/// An owner reference of a row's object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub uid: String,
    /// The owner that manages the object, as opposed to one it only belongs to.
    #[serde(default)]
    pub controller: bool,
}

/// Narrows a list on the server, like `kubectl get -l` and `--field-selector`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TableScope {
    pub label_selector: Option<String>,
    pub field_selector: Option<String>,
    /// Extra columns, one JSONPath per line, like `kubectl get -o custom-columns`. Their cells
    /// follow the server's, and their columns are named by the path with the format `jsonpath`.
    pub paths: Option<String>,
}

impl TableScope {
    /// The extra columns; a path that does not parse is an error.
    fn extra(&self) -> Result<Extra, String> {
        let names: Vec<String> = self
            .paths
            .as_deref()
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        let parsed = names
            .iter()
            .map(|n| JsonPath::parse(n))
            .collect::<Result<_, _>>()?;
        Ok(Extra { names, parsed })
    }

    fn list_params(&self) -> ListParams {
        let mut params = ListParams::default();
        if let Some(labels) = self.label_selector.as_deref().filter(|s| !s.is_empty()) {
            params = params.labels(labels);
        }
        if let Some(fields) = self.field_selector.as_deref().filter(|s| !s.is_empty()) {
            params = params.fields(fields);
        }
        params
    }

    fn watch_params(&self) -> WatchParams {
        let mut params = WatchParams::default().disable_bookmarks().timeout(290);
        if let Some(labels) = self.label_selector.as_deref().filter(|s| !s.is_empty()) {
            params = params.labels(labels);
        }
        if let Some(fields) = self.field_selector.as_deref().filter(|s| !s.is_empty()) {
            params = params.fields(fields);
        }
        params
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TableEvent {
    /// The complete current table. Replaces everything the client knows.
    Reset {
        columns: Vec<Column>,
        rows: Vec<Row>,
    },
    Upsert {
        row: Row,
    },
    Delete {
        row: Row,
    },
    /// Listing or watching failed; Kubri retries with backoff.
    Error {
        message: String,
    },
    /// Something to know about the list, such as which namespaces it covers.
    Notice {
        message: String,
    },
}

/// Columns read from each object with a JSONPath, beside the server's.
#[derive(Debug, Clone, Default)]
struct Extra {
    names: Vec<String>,
    parsed: Vec<JsonPath>,
}

impl Extra {
    fn columns(&self) -> impl Iterator<Item = Column> + '_ {
        self.names.iter().map(|name| Column {
            name: name.clone(),
            kind: "string".to_owned(),
            format: "jsonpath".to_owned(),
            description: format!("{name} in each object"),
            priority: 0,
        })
    }

    fn cells<'a>(&'a self, object: &'a Value) -> impl Iterator<Item = Value> + 'a {
        self.parsed.iter().map(move |p| p.cell(object))
    }
}

/// Streams one table made of the collections at `urls`, such as the same resource in several
/// namespaces, starting with `notice`.
pub(crate) fn watch_tables(
    client: Client,
    urls: Vec<String>,
    scope: TableScope,
    notice: String,
) -> impl Stream<Item = TableEvent> + Send + 'static {
    let streams: Vec<_> = urls
        .into_iter()
        .enumerate()
        .map(|(i, url)| {
            watch_table(client.clone(), url, scope.clone())
                .map(move |event| (i, event))
                .boxed()
        })
        .collect();
    let mut parts: Vec<Option<Vec<Row>>> = vec![None; streams.len()];
    let mut columns = Vec::new();
    let merged = futures::stream::select_all(streams).filter_map(move |(i, event)| {
        let out = match event {
            TableEvent::Reset { columns: c, rows } => {
                parts[i] = Some(rows);
                if columns.is_empty() {
                    columns = c;
                }
                let rows = parts.iter().flatten().flatten().cloned().collect();
                Some(TableEvent::Reset {
                    columns: columns.clone(),
                    rows,
                })
            }
            other => Some(other),
        };
        futures::future::ready(out)
    });
    futures::stream::once(futures::future::ready(TableEvent::Notice { message: notice })).chain(merged)
}

/// Streams table updates for the collection at `url` until the stream is dropped.
pub(crate) fn watch_table(
    client: Client,
    url: String,
    scope: TableScope,
) -> impl Stream<Item = TableEvent> + Send + 'static {
    let (tx, rx) = mpsc::channel(256);
    tokio::spawn(async move {
        tokio::select! {
            _ = run(&client, &url, &scope, &tx) => {}
            _ = tx.closed() => {}
        }
    });
    futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|event| (event, rx)) },
    )
}

enum Failure {
    /// The watch's resource version expired (410 Gone): relist right away.
    Expired,
    Error(String),
}

impl From<kube::Error> for Failure {
    fn from(err: kube::Error) -> Self {
        // The API server may also answer a watch from an old version with an HTTP 410.
        match crate::Error::from(err) {
            err if err.api_status() == Some(410) => Failure::Expired,
            err => Failure::Error(err.to_string()),
        }
    }
}

async fn run(client: &Client, url: &str, scope: &TableScope, tx: &mpsc::Sender<TableEvent>) {
    let extra = match scope.extra() {
        Ok(extra) => extra,
        Err(message) => {
            let _ = tx.send(TableEvent::Error { message }).await;
            return;
        }
    };
    let mut backoff = Duration::from_secs(1);
    loop {
        let mut listed = false;
        let message = match list_and_watch(client, url, scope, &extra, tx, &mut listed).await {
            Ok(()) => return,
            Err(Failure::Expired) => continue,
            Err(Failure::Error(message)) => message,
        };
        if listed {
            backoff = Duration::from_secs(1);
        }
        if tx.send(TableEvent::Error { message }).await.is_err() {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// Lists once, then watches from that version. Returns `Ok` only when the receiver is gone.
async fn list_and_watch(
    client: &Client,
    url: &str,
    scope: &TableScope,
    extra: &Extra,
    tx: &mpsc::Sender<TableEvent>,
    listed: &mut bool,
) -> Result<(), Failure> {
    let full = !extra.parsed.is_empty();
    let request = with_table_accept(Request::new(url).list(&scope.list_params()), full);
    let list: Value = client.request(request).await?;
    let mut version = list
        .pointer("/metadata/resourceVersion")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let (columns, rows) = parse_list(list, extra);
    if tx.send(TableEvent::Reset { columns, rows }).await.is_err() {
        return Ok(());
    }
    *listed = true;

    loop {
        let request = with_table_accept(Request::new(url).watch(&scope.watch_params(), &version), full);
        let events = client.request_events::<Value>(request).await?;
        let mut events = std::pin::pin!(events);
        while let Some(event) = events.next().await {
            let (deleted, object) = match event? {
                WatchEvent::Added(obj) | WatchEvent::Modified(obj) => (false, obj),
                WatchEvent::Deleted(obj) => (true, obj),
                WatchEvent::Bookmark(bookmark) => {
                    version = bookmark.metadata.resource_version;
                    continue;
                }
                WatchEvent::Error(status) if status.code == 410 => return Err(Failure::Expired),
                WatchEvent::Error(status) => return Err(Failure::Error(status.message)),
            };
            for row in parse_event(object, &mut version, extra) {
                let event = if deleted {
                    TableEvent::Delete { row }
                } else {
                    TableEvent::Upsert { row }
                };
                if tx.send(event).await.is_err() {
                    return Ok(());
                }
            }
        }
        // The server closed the watch after its timeout; resume from the last version.
    }
}

/// Asks for a table; `full` also asks for whole objects in its rows, for JSONPath columns,
/// instead of only their metadata.
fn with_table_accept(
    request: Result<http::Request<Vec<u8>>, kube::core::request::Error>,
    full: bool,
) -> http::Request<Vec<u8>> {
    // Building a GET for a valid path cannot fail.
    let mut request = request.expect("valid list/watch request");
    if full {
        let uri = request.uri().to_string();
        let joined = format!(
            "{uri}{}includeObject=Object",
            if uri.contains('?') { '&' } else { '?' }
        );
        if let Ok(uri) = joined.parse() {
            *request.uri_mut() = uri;
        }
    }
    request
        .headers_mut()
        .insert(http::header::ACCEPT, http::HeaderValue::from_static(TABLE_ACCEPT));
    request
}

fn is_table(value: &Value) -> bool {
    value.get("kind").and_then(Value::as_str) == Some("Table")
}

fn parse_list(list: Value, extra: &Extra) -> (Vec<Column>, Vec<Row>) {
    let (mut columns, rows) = if is_table(&list) {
        let columns: Vec<Column> = list
            .get("columnDefinitions")
            .cloned()
            .and_then(|c| serde_json::from_value(c).ok())
            .unwrap_or_default();
        let rows = table_rows(&list, extra).collect();
        (columns, rows)
    } else {
        let rows = list
            .get("items")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(|o| fallback_row(o, extra)).collect())
            .unwrap_or_default();
        (fallback_columns(), rows)
    };
    columns.extend(extra.columns());
    (columns, rows)
}

fn parse_event(object: Value, version: &mut String, extra: &Extra) -> Vec<Row> {
    let rows: Vec<Row> = if is_table(&object) {
        table_rows(&object, extra).collect()
    } else {
        vec![fallback_row(&object, extra)]
    };
    let latest = if is_table(&object) {
        object.pointer("/rows/0/object/metadata/resourceVersion")
    } else {
        object.pointer("/metadata/resourceVersion")
    };
    if let Some(v) = latest.and_then(Value::as_str) {
        *version = v.to_owned();
    }
    rows
}

fn table_rows<'a>(table: &'a Value, extra: &'a Extra) -> impl Iterator<Item = Row> + 'a {
    table
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(move |row| {
            let mut cells = row
                .get("cells")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let object = row.get("object").unwrap_or(&Value::Null);
            cells.extend(extra.cells(object));
            let metadata = object.get("metadata").unwrap_or(&Value::Null);
            build_row(metadata, cells)
        })
}

fn fallback_columns() -> Vec<Column> {
    ["Name", "Age"]
        .into_iter()
        .map(|name| Column {
            name: name.to_owned(),
            kind: "string".to_owned(),
            format: if name == "Name" {
                "name".to_owned()
            } else {
                String::new()
            },
            description: String::new(),
            priority: 0,
        })
        .collect()
}

fn fallback_row(object: &Value, extra: &Extra) -> Row {
    let metadata = object.get("metadata").unwrap_or(&Value::Null);
    let name = metadata.get("name").cloned().unwrap_or(Value::Null);
    let created = metadata.get("creationTimestamp").cloned().unwrap_or(Value::Null);
    let mut cells = vec![name, created];
    cells.extend(extra.cells(object));
    build_row(metadata, cells)
}

fn build_row(metadata: &Value, cells: Vec<Value>) -> Row {
    let text = |key: &str| metadata.get(key).and_then(Value::as_str).map(str::to_owned);
    let name = text("name")
        .or_else(|| cells.first().and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default();
    let namespace = text("namespace");
    let key = text("uid").unwrap_or_else(|| format!("{}/{}", namespace.as_deref().unwrap_or(""), name));
    Row {
        key,
        name,
        namespace,
        created: text("creationTimestamp"),
        labels: metadata
            .get("labels")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        owners: metadata
            .get("ownerReferences")
            .cloned()
            .and_then(|o| serde_json::from_value(o).ok())
            .unwrap_or_default(),
        cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pod_table() -> Value {
        json!({
            "kind": "Table",
            "apiVersion": "meta.k8s.io/v1",
            "metadata": { "resourceVersion": "100" },
            "columnDefinitions": [
                { "name": "Name", "type": "string", "format": "name", "description": "", "priority": 0 },
                { "name": "Ready", "type": "string", "format": "", "description": "", "priority": 0 },
                { "name": "IP", "type": "string", "format": "", "description": "", "priority": 1 }
            ],
            "rows": [{
                "cells": ["web-1", "1/1", "10.0.0.7"],
                "object": { "kind": "PartialObjectMetadata", "metadata": {
                    "name": "web-1", "namespace": "shop", "uid": "u-1", "resourceVersion": "99",
                    "creationTimestamp": "2026-10-01T10:00:00Z", "labels": { "app": "web" },
                    "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet",
                                          "name": "web-7d4b9", "uid": "rs-1", "controller": true }] } }
            }]
        })
    }

    #[test]
    fn adds_jsonpath_columns() {
        let scope = TableScope {
            paths: Some(".spec.nodeName\n\n .metadata.labels.app \n".into()),
            ..Default::default()
        };
        let extra = scope.extra().unwrap();
        let mut table = pod_table();
        table["rows"][0]["object"]["spec"] = json!({ "nodeName": "n1" });
        let (columns, rows) = parse_list(table, &extra);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            ["Name", "Ready", "IP", ".spec.nodeName", ".metadata.labels.app"]
        );
        assert_eq!(columns[3].format, "jsonpath");
        assert_eq!(rows[0].cells[3..], [json!("n1"), json!("web")]);
        // Objects the server sends without a table get them too.
        let (_, rows) = parse_list(
            json!({ "items": [{ "metadata": { "name": "a" }, "spec": { "nodeName": "n2" } }] }),
            &extra,
        );
        assert_eq!(rows[0].cells[2..], [json!("n2"), Value::Null]);
        let bad = TableScope {
            paths: Some(".a[".into()),
            ..Default::default()
        };
        assert!(bad.extra().is_err());
    }

    #[test]
    fn parses_server_side_tables() {
        let (columns, rows) = parse_list(pod_table(), &Extra::default());
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[2].priority, 1);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.key, "u-1");
        assert_eq!(row.name, "web-1");
        assert_eq!(row.namespace.as_deref(), Some("shop"));
        assert_eq!(row.created.as_deref(), Some("2026-10-01T10:00:00Z"));
        assert_eq!(row.labels["app"], "web");
        assert_eq!(row.cells, vec![json!("web-1"), json!("1/1"), json!("10.0.0.7")]);
        assert_eq!(
            row.owners,
            vec![Owner {
                api_version: "apps/v1".into(),
                kind: "ReplicaSet".into(),
                name: "web-7d4b9".into(),
                uid: "rs-1".into(),
                controller: true
            }]
        );
    }

    #[test]
    fn scopes_narrow_lists_and_watches() {
        let scope = TableScope {
            label_selector: Some("app=web".into()),
            field_selector: Some("spec.nodeName=n1".into()),
            paths: None,
        };
        let list = scope.list_params();
        assert_eq!(list.label_selector.as_deref(), Some("app=web"));
        assert_eq!(list.field_selector.as_deref(), Some("spec.nodeName=n1"));
        let watch = scope.watch_params();
        assert_eq!(watch.label_selector.as_deref(), Some("app=web"));
        assert_eq!(watch.field_selector.as_deref(), Some("spec.nodeName=n1"));
        let empty = TableScope {
            label_selector: Some(String::new()),
            field_selector: None,
            paths: None,
        };
        assert_eq!(empty.list_params().label_selector, None);
    }

    #[test]
    fn watch_events_advance_the_resource_version() {
        let mut version = "1".to_owned();
        let rows = parse_event(pod_table(), &mut version, &Extra::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(version, "99");
    }

    #[test]
    fn falls_back_to_name_and_age_for_plain_lists() {
        let list = json!({
            "kind": "PodMetricsList",
            "items": [{ "metadata": { "name": "a", "namespace": "x",
                                      "creationTimestamp": "2026-10-01T10:00:00Z" } }]
        });
        let (columns, rows) = parse_list(list, &Extra::default());
        assert_eq!(
            columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["Name", "Age"]
        );
        assert_eq!(rows[0].key, "x/a");
        assert_eq!(rows[0].cells, vec![json!("a"), json!("2026-10-01T10:00:00Z")]);
    }
}
