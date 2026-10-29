//! What changed: a record of changes in the clusters Kubri watched, kept in SQLite.
//!
//! The recorder ([`crate::recorder`]) watches a few kinds in the connected context and writes
//! what changed here: rollouts, image and spec changes, scaling, ConfigMap and Secret edits (by
//! key, never values), nodes and warning events. Each object's last known state is kept too, so
//! a change made while Kubri was closed is found when it starts again. Watched spans are kept as
//! sessions, so the timeline can show when Kubri was not watching.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{Error, ResourceRef, Result};

/// How long a recorder's claim on a context lasts without being renewed.
pub(crate) const LEASE_MS: i64 = 90_000;
/// Watched spans closer than this count as one.
const JOIN_SPANS_MS: i64 = 120_000;
const DEFAULT_LIMIT: u32 = 200;
const MAX_LIMIT: u32 = 2000;

/// What the recorder keeps and for how long.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistorySettings {
    /// Record changes in the connected context while Kubri runs.
    pub record: bool,
    /// Changes older than this many days are removed.
    pub keep_days: u32,
    /// The database is kept below this many megabytes by removing the oldest changes.
    pub max_mb: u32,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            record: true,
            keep_days: 14,
            max_mb: 200,
        }
    }
}

/// What kind of change an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Created,
    Deleted,
    /// The number of replicas.
    Scaled,
    /// A container's image.
    Image,
    /// `kubectl rollout restart`.
    Restarted,
    /// Another change to the pod template, which starts a rollout.
    Rollout,
    /// Keys of a ConfigMap or Secret.
    Data,
    /// The spec otherwise.
    Spec,
    /// Labels or annotations only.
    Metadata,
    /// A node's readiness, scheduling or taints.
    Node,
    /// A warning event.
    Warning,
}

impl ChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Created => "created",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Scaled => "scaled",
            ChangeKind::Image => "image",
            ChangeKind::Restarted => "restarted",
            ChangeKind::Rollout => "rollout",
            ChangeKind::Data => "data",
            ChangeKind::Spec => "spec",
            ChangeKind::Metadata => "metadata",
            ChangeKind::Node => "node",
            ChangeKind::Warning => "warning",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        serde_json::from_value(Value::String(text.to_owned())).ok()
    }

    /// Changes that start new pods, worth a line across charts and log histograms.
    pub fn is_rollout(self) -> bool {
        matches!(
            self,
            ChangeKind::Image
                | ChangeKind::Restarted
                | ChangeKind::Rollout
                | ChangeKind::Scaled
                | ChangeKind::Data
        )
    }
}

/// One change, without the object's states before and after.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub context: String,
    /// Unix milliseconds.
    pub at: i64,
    pub resource: ResourceRef,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
    pub uid: String,
    pub change: ChangeKind,
    pub summary: String,
    /// Found when Kubri started again: it happened at some time before `at`, while Kubri was
    /// not watching.
    pub unwatched: bool,
    /// The states before and after are kept, so the entry has a diff.
    pub has_diff: bool,
}

/// An entry with the object as it was before and after, for its diff.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDetail {
    #[serde(flatten)]
    pub entry: HistoryEntry,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// A time Kubri was watching, in Unix milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub from: i64,
    pub to: i64,
}

/// Which changes to read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryQuery {
    /// The context; the connected one when left out.
    pub context: Option<String>,
    pub namespace: Option<String>,
    /// A kind such as `Deployment`.
    pub kind: Option<String>,
    pub name: Option<String>,
    /// One object, by uid; warnings about it are included.
    pub uid: Option<String>,
    /// Unix milliseconds.
    pub since: Option<i64>,
    pub until: Option<i64>,
    /// Only these kinds of change; all when empty.
    pub changes: Vec<ChangeKind>,
    /// Part of the name, kind or summary.
    pub text: Option<String>,
    pub limit: Option<u32>,
    /// Entries older than this id, for the next page.
    pub before: Option<i64>,
}

/// Changes, newest first, with the times Kubri was watching.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub context: String,
    pub entries: Vec<HistoryEntry>,
    /// There are older entries; ask again with `before` set to the last id.
    pub more: bool,
    /// When Kubri watched this context, newest first, within the range asked for.
    pub watched: Vec<Span>,
    /// What the recorder is doing now.
    pub recorder: RecorderStatus,
}

/// Whether changes in a context are being recorded, and what is left out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderStatus {
    pub recording: bool,
    /// Why not, when not.
    pub reason: Option<String>,
    /// Kinds that cannot be watched, with why.
    pub skipped: Vec<String>,
    /// Unix milliseconds the recording started.
    pub since: Option<i64>,
}

/// A change about to be written.
#[derive(Debug, Clone)]
pub(crate) struct NewEntry {
    pub at: i64,
    pub resource: ResourceRef,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
    pub uid: String,
    pub change: ChangeKind,
    pub summary: String,
    pub unwatched: bool,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// An object's last known state.
#[derive(Debug, Clone)]
pub(crate) struct Known {
    pub uid: String,
    pub namespace: Option<String>,
    pub name: String,
    pub state: String,
}

pub struct Store {
    conn: Mutex<Connection>,
}

fn storage(err: rusqlite::Error) -> Error {
    Error::Unsupported(format!("the change history cannot be read or written: {err}"))
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Store {
    /// Opens the database at `path`, creating it readable only by the user.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let fresh = !path.exists();
        let conn = Connection::open(path).map_err(storage)?;
        #[cfg(unix)]
        if fresh {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        #[cfg(not(unix))]
        let _ = fresh;
        Self::setup(conn, true)
    }

    /// A database in memory, as in tests.
    pub fn in_memory() -> Result<Self> {
        Self::setup(Connection::open_in_memory().map_err(storage)?, false)
    }

    fn setup(conn: Connection, on_disk: bool) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(storage)?;
        // Set before the first table, so space freed by pruning can be given back.
        conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")
            .map_err(storage)?;
        if on_disk {
            conn.pragma_update(None, "journal_mode", "WAL").map_err(storage)?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY,
                context TEXT NOT NULL,
                started INTEGER NOT NULL,
                ended INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS sessions_context ON sessions (context, ended);
            CREATE TABLE IF NOT EXISTS objects (
                context TEXT NOT NULL,
                uid TEXT NOT NULL,
                api_group TEXT NOT NULL,
                kind TEXT NOT NULL,
                namespace TEXT,
                name TEXT NOT NULL,
                state TEXT NOT NULL,
                PRIMARY KEY (context, uid)
            );
            CREATE INDEX IF NOT EXISTS objects_kind ON objects (context, api_group, kind);
            CREATE TABLE IF NOT EXISTS changes (
                id INTEGER PRIMARY KEY,
                context TEXT NOT NULL,
                at INTEGER NOT NULL,
                api_group TEXT NOT NULL,
                version TEXT NOT NULL,
                plural TEXT NOT NULL,
                kind TEXT NOT NULL,
                namespace TEXT,
                name TEXT NOT NULL,
                uid TEXT NOT NULL,
                change TEXT NOT NULL,
                summary TEXT NOT NULL,
                unwatched INTEGER NOT NULL DEFAULT 0,
                before TEXT,
                after TEXT
            );
            CREATE INDEX IF NOT EXISTS changes_at ON changes (context, at);
            CREATE INDEX IF NOT EXISTS changes_uid ON changes (context, uid);
            CREATE TABLE IF NOT EXISTS leases (
                context TEXT PRIMARY KEY,
                owner TEXT NOT NULL,
                until INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value BLOB NOT NULL
            );",
        )
        .map_err(storage)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The key for fingerprints of ConfigMap and Secret values, made once per database.
    pub(crate) fn fingerprint_key(&self) -> Result<Vec<u8>> {
        let conn = self.conn();
        let key: Option<Vec<u8>> = conn
            .query_row("SELECT value FROM meta WHERE key = 'fingerprint'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(storage)?;
        if let Some(key) = key {
            return Ok(key);
        }
        let mut key = vec![0u8; 32];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut key)
            .map_err(|_| Error::Unsupported("no random numbers for the change history".into()))?;
        conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('fingerprint', ?1)",
            params![key],
        )
        .map_err(storage)?;
        conn.query_row("SELECT value FROM meta WHERE key = 'fingerprint'", [], |r| {
            r.get(0)
        })
        .map_err(storage)
    }

    /// Claims recording `context` for `owner`, or renews the claim. Fails while another Kubri
    /// records it, so two windows do not write everything twice.
    pub(crate) fn claim(&self, context: &str, owner: &str, now: i64) -> Result<bool> {
        let changed = self
            .conn()
            .execute(
                "INSERT INTO leases (context, owner, until) VALUES (?1, ?2, ?3)
                 ON CONFLICT (context) DO UPDATE SET owner = excluded.owner, until = excluded.until
                 WHERE leases.owner = excluded.owner OR leases.until < ?4",
                params![context, owner, now + LEASE_MS, now],
            )
            .map_err(storage)?;
        Ok(changed == 1)
    }

    pub(crate) fn release(&self, context: &str, owner: &str) -> Result<()> {
        self.conn()
            .execute(
                "DELETE FROM leases WHERE context = ?1 AND owner = ?2",
                params![context, owner],
            )
            .map_err(storage)?;
        Ok(())
    }

    /// When Kubri last watched `context`, if ever.
    pub(crate) fn last_watched(&self, context: &str) -> Result<Option<i64>> {
        self.conn()
            .query_row(
                "SELECT max(ended) FROM sessions WHERE context = ?1",
                params![context],
                |r| r.get(0),
            )
            .map_err(storage)
    }

    pub(crate) fn start_session(&self, context: &str, now: i64) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO sessions (context, started, ended) VALUES (?1, ?2, ?2)",
            params![context, now],
        )
        .map_err(storage)?;
        Ok(conn.last_insert_rowid())
    }

    pub(crate) fn beat(&self, session: i64, now: i64) -> Result<()> {
        self.conn()
            .execute(
                "UPDATE sessions SET ended = ?2 WHERE id = ?1",
                params![session, now],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub(crate) fn known(&self, context: &str, uid: &str) -> Result<Option<String>> {
        self.conn()
            .query_row(
                "SELECT state FROM objects WHERE context = ?1 AND uid = ?2",
                params![context, uid],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)
    }

    /// The objects of one kind known in `context`, in `namespace` or everywhere.
    pub(crate) fn known_of_kind(
        &self,
        context: &str,
        group: &str,
        kind: &str,
        namespace: Option<&str>,
    ) -> Result<Vec<Known>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT uid, namespace, name, state FROM objects
                 WHERE context = ?1 AND api_group = ?2 AND kind = ?3 AND (?4 IS NULL OR namespace = ?4)",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(params![context, group, kind, namespace], |r| {
                Ok(Known {
                    uid: r.get(0)?,
                    namespace: r.get(1)?,
                    name: r.get(2)?,
                    state: r.get(3)?,
                })
            })
            .map_err(storage)?;
        rows.collect::<std::result::Result<_, _>>().map_err(storage)
    }

    /// Keeps `known` as the last state of an object of `kind` in `group`.
    pub(crate) fn put_state(&self, context: &str, group: &str, kind: &str, known: &Known) -> Result<()> {
        self.conn()
            .execute(
                "INSERT INTO objects (context, uid, api_group, kind, namespace, name, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (context, uid) DO UPDATE SET namespace = excluded.namespace,
                     name = excluded.name, state = excluded.state",
                params![
                    context,
                    known.uid,
                    group,
                    kind,
                    known.namespace,
                    known.name,
                    known.state
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub(crate) fn forget(&self, context: &str, uid: &str) -> Result<()> {
        self.conn()
            .execute(
                "DELETE FROM objects WHERE context = ?1 AND uid = ?2",
                params![context, uid],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub(crate) fn record(&self, context: &str, entry: &NewEntry) -> Result<i64> {
        let text = |v: &Option<Value>| v.as_ref().map(|v| v.to_string());
        let conn = self.conn();
        conn.execute(
            "INSERT INTO changes (context, at, api_group, version, plural, kind, namespace, name, uid,
                 change, summary, unwatched, before, after)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                context,
                entry.at,
                entry.resource.group,
                entry.resource.version,
                entry.resource.plural,
                entry.kind,
                entry.namespace,
                entry.name,
                entry.uid,
                entry.change.as_str(),
                entry.summary,
                entry.unwatched,
                text(&entry.before),
                text(&entry.after),
            ],
        )
        .map_err(storage)?;
        Ok(conn.last_insert_rowid())
    }

    /// Changes matching `query` in `context`, newest first.
    pub fn query(&self, context: &str, query: &HistoryQuery) -> Result<(Vec<HistoryEntry>, bool)> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let mut sql = String::from(
            "SELECT id, context, at, api_group, version, plural, kind, namespace, name, uid, change,
                 summary, unwatched, before IS NOT NULL OR after IS NOT NULL
             FROM changes WHERE context = ?",
        );
        let mut values: Vec<rusqlite::types::Value> = vec![context.to_owned().into()];
        let mut add = |clause: &str, value: rusqlite::types::Value| {
            sql.push_str(" AND ");
            sql.push_str(clause);
            values.push(value);
        };
        if let Some(ns) = &query.namespace {
            add("namespace = ?", ns.clone().into());
        }
        if let Some(kind) = &query.kind {
            add("lower(kind) = lower(?)", kind.clone().into());
        }
        if let Some(name) = &query.name {
            add("name = ?", name.clone().into());
        }
        if let Some(uid) = &query.uid {
            add("uid = ?", uid.clone().into());
        }
        if let Some(since) = query.since {
            add("at >= ?", since.into());
        }
        if let Some(until) = query.until {
            add("at <= ?", until.into());
        }
        if let Some(before) = query.before {
            add("id < ?", before.into());
        }
        if let Some(text) = query.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            let like = format!(
                "%{}%",
                text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
            );
            sql.push_str(
                " AND (name LIKE ? ESCAPE '\\' OR kind LIKE ? ESCAPE '\\' OR summary LIKE ? ESCAPE '\\'
                    OR namespace LIKE ? ESCAPE '\\')",
            );
            for _ in 0..4 {
                values.push(like.clone().into());
            }
        }
        if !query.changes.is_empty() {
            let marks = vec!["?"; query.changes.len()].join(", ");
            sql.push_str(&format!(" AND change IN ({marks})"));
            values.extend(query.changes.iter().map(|c| c.as_str().to_owned().into()));
        }
        sql.push_str(" ORDER BY at DESC, id DESC LIMIT ?");
        values.push(i64::from(limit + 1).into());

        let conn = self.conn();
        let mut statement = conn.prepare(&sql).map_err(storage)?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(values), entry_of)
            .map_err(storage)?;
        let mut entries = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage)?;
        let more = entries.len() > limit as usize;
        entries.truncate(limit as usize);
        Ok((entries, more))
    }

    /// One change with the states before and after it.
    pub fn detail(&self, id: i64) -> Result<Option<HistoryDetail>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT id, context, at, api_group, version, plural, kind, namespace, name, uid, change,
                 summary, unwatched, before IS NOT NULL OR after IS NOT NULL, before, after
             FROM changes WHERE id = ?1",
            params![id],
            |r| {
                let parse = |text: Option<String>| text.and_then(|t| serde_json::from_str(&t).ok());
                Ok(HistoryDetail {
                    entry: entry_of(r)?,
                    before: parse(r.get(14)?),
                    after: parse(r.get(15)?),
                })
            },
        )
        .optional()
        .map_err(storage)
    }

    /// When Kubri watched `context` between `since` and `until`, newest first, with spans
    /// that nearly touch joined.
    pub fn watched(&self, context: &str, since: Option<i64>, until: Option<i64>) -> Result<Vec<Span>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT started, ended FROM sessions
                 WHERE context = ?1 AND ended >= ?2 AND started <= ?3 ORDER BY started",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(
                params![context, since.unwrap_or(i64::MIN), until.unwrap_or(i64::MAX)],
                |r| {
                    Ok(Span {
                        from: r.get(0)?,
                        to: r.get(1)?,
                    })
                },
            )
            .map_err(storage)?;
        let mut spans: Vec<Span> = Vec::new();
        for span in rows {
            let span = span.map_err(storage)?;
            match spans.last_mut() {
                Some(last) if span.from <= last.to + JOIN_SPANS_MS => last.to = last.to.max(span.to),
                _ => spans.push(span),
            }
        }
        spans.reverse();
        Ok(spans)
    }

    /// Removes changes older than the settings keep, then the oldest ones while the database
    /// is larger than they allow.
    pub(crate) fn prune(&self, settings: &HistorySettings, now: i64) -> Result<()> {
        let conn = self.conn();
        let oldest = now - i64::from(settings.keep_days.max(1)) * 24 * 3600 * 1000;
        conn.execute("DELETE FROM changes WHERE at < ?1", params![oldest])
            .map_err(storage)?;
        conn.execute("DELETE FROM sessions WHERE ended < ?1", params![oldest])
            .map_err(storage)?;
        let max = i64::from(settings.max_mb.max(1)) * 1024 * 1024;
        for _ in 0..20 {
            let size: i64 = conn
                .query_row(
                    "SELECT (page_count - freelist_count) * page_size FROM pragma_page_count(), pragma_freelist_count(), pragma_page_size()",
                    [],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            if size <= max {
                break;
            }
            let removed = conn
                .execute(
                    "DELETE FROM changes WHERE id IN (SELECT id FROM changes ORDER BY at LIMIT
                         (SELECT max(100, count(*) / 10) FROM changes))",
                    [],
                )
                .map_err(storage)?;
            if removed == 0 {
                break;
            }
        }
        conn.execute_batch("PRAGMA incremental_vacuum").map_err(storage)?;
        Ok(())
    }
}

fn entry_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    let change: String = r.get(10)?;
    Ok(HistoryEntry {
        id: r.get(0)?,
        context: r.get(1)?,
        at: r.get(2)?,
        resource: ResourceRef {
            group: r.get(3)?,
            version: r.get(4)?,
            plural: r.get(5)?,
        },
        kind: r.get(6)?,
        namespace: r.get(7)?,
        name: r.get(8)?,
        uid: r.get(9)?,
        change: ChangeKind::parse(&change).unwrap_or(ChangeKind::Spec),
        summary: r.get(11)?,
        unwatched: r.get(12)?,
        has_diff: r.get(13)?,
    })
}

/// Annotations that change on their own or hold a copy of the object.
const NOISY_ANNOTATIONS: &[&str] = &[
    "kubectl.kubernetes.io/last-applied-configuration",
    "deployment.kubernetes.io/revision",
    "deprecated.daemonset.template.generation",
    "control-plane.alpha.kubernetes.io/leader",
];
const RESTARTED_AT: &str = "kubectl.kubernetes.io/restartedAt";

/// The part of `object` worth keeping and comparing: no status, no fields the API server
/// changes on its own, and for ConfigMaps and Secrets a fingerprint of each value in place of
/// the value. Nodes keep their conditions, taints and whether they take new pods.
pub(crate) fn stored(kind: &str, object: &Value, key: &[u8]) -> Value {
    let mut out = Map::new();
    let meta = &object["metadata"];
    let mut metadata = Map::new();
    for field in ["name", "namespace"] {
        if let Some(v) = meta.get(field) {
            metadata.insert(field.into(), v.clone());
        }
    }
    if let Some(labels) = meta
        .get("labels")
        .filter(|l| l.as_object().is_some_and(|l| !l.is_empty()))
    {
        metadata.insert("labels".into(), labels.clone());
    }
    if kind != "Node"
        && let Some(annotations) = meta.get("annotations").and_then(Value::as_object)
    {
        let kept: Map<String, Value> = annotations
            .iter()
            .filter(|(k, _)| !NOISY_ANNOTATIONS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !kept.is_empty() {
            metadata.insert("annotations".into(), Value::Object(kept));
        }
    }
    out.insert("metadata".into(), Value::Object(metadata));

    match kind {
        "ConfigMap" | "Secret" => {
            for field in ["data", "binaryData", "stringData"] {
                if let Some(data) = object.get(field).and_then(Value::as_object) {
                    let prints = data
                        .iter()
                        .map(|(k, v)| {
                            // Secret data and binaryData are base64; the size is of what it holds.
                            let encoded = kind == "Secret" && field == "data" || field == "binaryData";
                            (k.clone(), Value::String(fingerprint(key, v, encoded)))
                        })
                        .collect();
                    out.insert(field.into(), Value::Object(prints));
                }
            }
            for field in ["type", "immutable"] {
                if let Some(v) = object.get(field) {
                    out.insert(field.into(), v.clone());
                }
            }
        }
        "Node" => {
            let spec = &object["spec"];
            let mut kept = Map::new();
            if spec["unschedulable"].as_bool() == Some(true) {
                kept.insert("unschedulable".into(), json!(true));
            }
            // Taints under node.kubernetes.io mirror cordoning and the conditions kept below.
            let taints: Vec<Value> = spec["taints"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|t| !t["key"].as_str().unwrap_or("").starts_with("node.kubernetes.io/"))
                .map(|t| json!({"key": t["key"], "value": t["value"], "effect": t["effect"]}))
                .collect();
            if !taints.is_empty() {
                kept.insert("taints".into(), Value::Array(taints));
            }
            out.insert("spec".into(), Value::Object(kept));
            let conditions: Map<String, Value> = object["status"]["conditions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| Some((c["type"].as_str()?.to_owned(), c["status"].clone())))
                .collect();
            let version = &object["status"]["nodeInfo"]["kubeletVersion"];
            out.insert(
                "status".into(),
                json!({"conditions": conditions, "kubeletVersion": version}),
            );
        }
        _ => {
            if let Some(spec) = object.get("spec") {
                out.insert("spec".into(), spec.clone());
            }
        }
    }
    Value::Object(out)
}

/// A short fingerprint of a ConfigMap or Secret value: enough to tell that it changed, without
/// keeping it.
fn fingerprint(key: &[u8], value: &Value, base64: bool) -> String {
    use base64::Engine as _;
    let bytes = match value {
        Value::String(s) if base64 => base64::engine::general_purpose::STANDARD
            .decode(s)
            .unwrap_or_else(|_| s.as_bytes().to_vec()),
        Value::String(s) => s.as_bytes().to_vec(),
        other => other.to_string().into_bytes(),
    };
    let tag = ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key), &bytes);
    let hex: String = tag.as_ref()[..6].iter().map(|b| format!("{b:02x}")).collect();
    format!("‹{} bytes, {hex}›", bytes.len())
}

/// What kind of change turns `before` into `after`, both as [`stored`] keeps them, and a line
/// saying what changed. `None` when nothing worth recording did.
pub(crate) fn describe(kind: &str, before: &Value, after: &Value) -> Option<(ChangeKind, String)> {
    if before == after {
        return None;
    }
    if kind == "Node" {
        return Some((ChangeKind::Node, describe_node(before, after)));
    }
    if kind == "ConfigMap" || kind == "Secret" {
        let keys = |v: &Value| -> BTreeMap<String, Value> {
            ["data", "binaryData", "stringData"]
                .iter()
                .filter_map(|f| v.get(*f).and_then(Value::as_object))
                .flatten()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let (old, new) = (keys(before), keys(after));
        if old != new {
            let added: Vec<&str> = new
                .keys()
                .filter(|k| !old.contains_key(*k))
                .map(String::as_str)
                .collect();
            let removed: Vec<&str> = old
                .keys()
                .filter(|k| !new.contains_key(*k))
                .map(String::as_str)
                .collect();
            let changed: Vec<&str> = new
                .iter()
                .filter(|(k, v)| old.get(*k).is_some_and(|o| o != *v))
                .map(|(k, _)| k.as_str())
                .collect();
            let mut parts = Vec::new();
            for (label, keys) in [("Changed", changed), ("added", added), ("removed", removed)] {
                if !keys.is_empty() {
                    parts.push(format!("{label} {}", listed(&keys)));
                }
            }
            let mut text = parts.join("; ");
            if let Some(first) = text.get(..1) {
                text = first.to_uppercase() + &text[1..];
            }
            return Some((ChangeKind::Data, text));
        }
    }

    let spec_before = &before["spec"];
    let spec_after = &after["spec"];
    if spec_before == spec_after {
        return Some((ChangeKind::Metadata, describe_metadata(before, after)));
    }
    let template_before = template_of(spec_before);
    let template_after = template_of(spec_after);
    if template_before != template_after {
        let images_before = images(template_before);
        let images_after = images(template_after);
        if images_before != images_after {
            let mut parts = Vec::new();
            for (name, image) in &images_after {
                match images_before.get(name) {
                    Some(old) if old != image => parts.push(format!("{name}: {old} → {image}")),
                    None => parts.push(format!("{name} added: {image}")),
                    _ => {}
                }
            }
            for name in images_before.keys().filter(|n| !images_after.contains_key(*n)) {
                parts.push(format!("{name} removed"));
            }
            return Some((ChangeKind::Image, format!("Image {}", parts.join("; "))));
        }
        let restarted = |t: &Value| t["metadata"]["annotations"][RESTARTED_AT].clone();
        if restarted(template_before) != restarted(template_after) {
            let mut b = template_before.clone();
            let mut a = template_after.clone();
            for t in [&mut b, &mut a] {
                if let Some(metadata) = t.get_mut("metadata").and_then(Value::as_object_mut) {
                    if let Some(annotations) = metadata.get_mut("annotations").and_then(Value::as_object_mut)
                    {
                        annotations.remove(RESTARTED_AT);
                        if annotations.is_empty() {
                            metadata.remove("annotations");
                        }
                    }
                    if metadata.is_empty()
                        && let Some(t) = t.as_object_mut()
                    {
                        t.remove("metadata");
                    }
                }
            }
            if a == b {
                return Some((ChangeKind::Restarted, "Restarted".into()));
            }
        }
        let mut paths = Vec::new();
        changed_paths(template_before, template_after, "template", &mut paths);
        return Some((ChangeKind::Rollout, format!("Changed {}", listed_paths(&paths))));
    }
    let replicas = |s: &Value| s.get("replicas").cloned();
    let mut rest_before = spec_before.clone();
    let mut rest_after = spec_after.clone();
    for s in [&mut rest_before, &mut rest_after] {
        if let Some(s) = s.as_object_mut() {
            s.remove("replicas");
        }
    }
    if replicas(spec_before) != replicas(spec_after) && rest_before == rest_after {
        let count = |v: Option<Value>| v.map(|v| v.to_string()).unwrap_or_else(|| "1".into());
        return Some((
            ChangeKind::Scaled,
            format!(
                "Scaled from {} to {}",
                count(replicas(spec_before)),
                count(replicas(spec_after))
            ),
        ));
    }
    let mut paths = Vec::new();
    changed_paths(spec_before, spec_after, "", &mut paths);
    Some((ChangeKind::Spec, format!("Changed {}", listed_paths(&paths))))
}

fn template_of(spec: &Value) -> &Value {
    // CronJobs keep their pod template inside the job template.
    let direct = &spec["template"];
    if direct.is_null() {
        &spec["jobTemplate"]["spec"]["template"]
    } else {
        direct
    }
}

/// Container name to image, for init containers too.
fn images(template: &Value) -> BTreeMap<String, String> {
    ["initContainers", "containers"]
        .iter()
        .filter_map(|f| template["spec"][*f].as_array())
        .flatten()
        .filter_map(|c| Some((c["name"].as_str()?.to_owned(), c["image"].as_str()?.to_owned())))
        .collect()
}

fn describe_node(before: &Value, after: &Value) -> String {
    let mut parts = Vec::new();
    let ready = |v: &Value| v["status"]["conditions"]["Ready"].as_str().map(str::to_owned);
    if ready(before) != ready(after) {
        parts.push(match ready(after).as_deref() {
            Some("True") => "Ready".to_owned(),
            Some("False") => "Not ready".to_owned(),
            _ => "Readiness unknown".to_owned(),
        });
    }
    let cordoned = |v: &Value| v["spec"]["unschedulable"].as_bool() == Some(true);
    if cordoned(before) != cordoned(after) {
        parts.push(if cordoned(after) { "Cordoned" } else { "Uncordoned" }.to_owned());
    }
    let pressure = |v: &Value| -> BTreeSet<String> {
        v["status"]["conditions"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, s)| k.as_str() != "Ready" && s.as_str() == Some("True"))
            .map(|(k, _)| k.clone())
            .collect()
    };
    let (old, new) = (pressure(before), pressure(after));
    for started in new.difference(&old) {
        parts.push(started.clone());
    }
    for ended in old.difference(&new) {
        parts.push(format!("{ended} ended"));
    }
    if before["spec"]["taints"] != after["spec"]["taints"] {
        parts.push("Taints changed".into());
    }
    let version = |v: &Value| v["status"]["kubeletVersion"].as_str().map(str::to_owned);
    if version(before) != version(after) {
        parts.push(format!(
            "Kubelet {} → {}",
            version(before).unwrap_or_default(),
            version(after).unwrap_or_default()
        ));
    }
    if parts.is_empty() {
        parts.push(describe_metadata(before, after));
    }
    parts.join("; ")
}

fn describe_metadata(before: &Value, after: &Value) -> String {
    let labels = before["metadata"]["labels"] != after["metadata"]["labels"];
    let annotations = before["metadata"]["annotations"] != after["metadata"]["annotations"];
    match (labels, annotations) {
        (true, true) => "Labels and annotations changed",
        (true, false) => "Labels changed",
        (false, true) => "Annotations changed",
        (false, false) => "Changed",
    }
    .into()
}

/// The paths where `a` and `b` differ, with list items named by their `name` where they have
/// one: `template.spec.containers[app].env`.
pub(crate) fn changed_paths(a: &Value, b: &Value, at: &str, out: &mut Vec<String>) {
    if a == b || out.len() > 20 {
        return;
    }
    let join = |key: &str| {
        if at.is_empty() {
            key.to_owned()
        } else {
            format!("{at}.{key}")
        }
    };
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for key in keys {
                changed_paths(
                    x.get(key).unwrap_or(&Value::Null),
                    y.get(key).unwrap_or(&Value::Null),
                    &join(key),
                    out,
                );
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            let named = |v: &Vec<Value>| -> Option<BTreeMap<String, Value>> {
                v.iter()
                    .map(|i| Some((i["name"].as_str()?.to_owned(), i.clone())))
                    .collect()
            };
            match (named(x), named(y)) {
                (Some(x), Some(y)) if !x.is_empty() || !y.is_empty() => {
                    let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
                    for key in keys {
                        changed_paths(
                            x.get(key).unwrap_or(&Value::Null),
                            y.get(key).unwrap_or(&Value::Null),
                            &format!("{at}[{key}]"),
                            out,
                        );
                    }
                }
                _ => out.push(at.to_owned()),
            }
        }
        _ => out.push(at.to_owned()),
    }
}

fn listed(items: &[&str]) -> String {
    match items.len() {
        0..=3 => items.join(", "),
        n => format!("{} and {} more", items[..3].join(", "), n - 3),
    }
}

fn listed_paths(paths: &[String]) -> String {
    if paths.is_empty() {
        return "the spec".into();
    }
    let items: Vec<&str> = paths.iter().map(String::as_str).collect();
    listed(&items)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"test key";

    fn deployment(image: &str, replicas: u32) -> Value {
        json!({
            "metadata": {"name": "web", "namespace": "shop", "uid": "u1", "resourceVersion": "9",
                "annotations": {"deployment.kubernetes.io/revision": "3"}},
            "spec": {"replicas": replicas, "template": {"spec": {"containers": [
                {"name": "app", "image": image, "env": [{"name": "A", "value": "1"}]}
            ]}}},
            "status": {"readyReplicas": 1}
        })
    }

    fn change(kind: &str, a: &Value, b: &Value) -> Option<(ChangeKind, String)> {
        describe(kind, &stored(kind, a, KEY), &stored(kind, b, KEY))
    }

    #[test]
    fn status_and_bookkeeping_are_not_changes() {
        let a = deployment("nginx:1", 2);
        let mut b = a.clone();
        b["status"]["readyReplicas"] = json!(2);
        b["metadata"]["resourceVersion"] = json!("10");
        b["metadata"]["annotations"]["deployment.kubernetes.io/revision"] = json!("4");
        assert_eq!(change("Deployment", &a, &b), None);
    }

    #[test]
    fn workload_changes_are_named() {
        let a = deployment("nginx:1", 2);
        assert_eq!(
            change("Deployment", &a, &deployment("nginx:1", 3)),
            Some((ChangeKind::Scaled, "Scaled from 2 to 3".into()))
        );
        assert_eq!(
            change("Deployment", &a, &deployment("nginx:2", 2)),
            Some((ChangeKind::Image, "Image app: nginx:1 → nginx:2".into()))
        );
        let mut restarted = a.clone();
        restarted["spec"]["template"]["metadata"] =
            json!({"annotations": {RESTARTED_AT: "2026-10-29T10:00:00Z"}});
        assert_eq!(
            change("Deployment", &a, &restarted),
            Some((ChangeKind::Restarted, "Restarted".into()))
        );
        let mut env = a.clone();
        env["spec"]["template"]["spec"]["containers"][0]["env"][0]["value"] = json!("2");
        assert_eq!(
            change("Deployment", &a, &env),
            Some((
                ChangeKind::Rollout,
                "Changed template.spec.containers[app].env[A].value".into()
            ))
        );
        let mut labels = a.clone();
        labels["metadata"]["labels"] = json!({"team": "shop"});
        assert_eq!(
            change("Deployment", &a, &labels),
            Some((ChangeKind::Metadata, "Labels changed".into()))
        );
    }

    #[test]
    fn secrets_keep_no_values() {
        let a = json!({"metadata": {"name": "s"}, "data": {"password": "aHVudGVyMg==", "user": "YQ=="}});
        let mut b = a.clone();
        b["data"]["password"] = json!("c2VjcmV0");
        b["data"]["token"] = json!("eA==");
        let kept = stored("Secret", &b, KEY).to_string();
        assert!(!kept.contains("c2VjcmV0") && !kept.contains("eA=="));
        assert!(kept.contains("‹6 bytes, "));
        assert_eq!(
            change("Secret", &a, &b),
            Some((ChangeKind::Data, "Changed password; added token".into()))
        );
        // Another database has another key, so the same value has another fingerprint.
        assert_ne!(stored("Secret", &a, KEY), stored("Secret", &a, b"other"));
    }

    #[test]
    fn nodes_say_what_happened() {
        let node = |ready: &str, cordoned: bool| {
            json!({"metadata": {"name": "n1", "annotations": {"heartbeat": "x"}},
                "spec": {"unschedulable": cordoned},
                "status": {"conditions": [
                    {"type": "Ready", "status": ready, "lastHeartbeatTime": "now"},
                    {"type": "MemoryPressure", "status": "False"}
                ], "nodeInfo": {"kubeletVersion": "v1.37.0"}}})
        };
        assert_eq!(change("Node", &node("True", false), &node("True", false)), None);
        assert_eq!(
            change("Node", &node("True", false), &node("False", true)),
            Some((ChangeKind::Node, "Not ready; Cordoned".into()))
        );
    }

    #[test]
    fn store_keeps_changes_and_watched_spans() {
        let store = Store::in_memory().unwrap();
        let resource = ResourceRef {
            group: "apps".into(),
            version: "v1".into(),
            plural: "deployments".into(),
        };
        assert!(store.claim("ctx", "a", 1000).unwrap());
        assert!(!store.claim("ctx", "b", 2000).unwrap());
        assert!(store.claim("ctx", "a", 2000).unwrap());
        assert!(store.claim("ctx", "b", 1000 + LEASE_MS + 2000).unwrap());

        let s1 = store.start_session("ctx", 1_000).unwrap();
        store.beat(s1, 60_000).unwrap();
        let s2 = store.start_session("ctx", 100_000).unwrap();
        store.beat(s2, 200_000).unwrap();
        let s3 = store.start_session("ctx", 900_000).unwrap();
        store.beat(s3, 950_000).unwrap();
        assert_eq!(
            store.watched("ctx", None, None).unwrap(),
            vec![
                Span {
                    from: 900_000,
                    to: 950_000
                },
                Span {
                    from: 1_000,
                    to: 200_000
                }
            ]
        );
        assert_eq!(store.last_watched("ctx").unwrap(), Some(950_000));

        let entry = |at: i64, name: &str, change: ChangeKind| NewEntry {
            at,
            resource: resource.clone(),
            kind: "Deployment".into(),
            namespace: Some("shop".into()),
            name: name.into(),
            uid: format!("uid-{name}"),
            change,
            summary: format!("{name} 50%_done"),
            unwatched: false,
            before: Some(json!({"spec": 1})),
            after: None,
        };
        for i in 0..5 {
            store
                .record("ctx", &entry(10_000 + i, &format!("web{i}"), ChangeKind::Image))
                .unwrap();
        }
        store
            .record("ctx", &entry(20_000, "api", ChangeKind::Scaled))
            .unwrap();
        store
            .record("other", &entry(20_000, "api", ChangeKind::Scaled))
            .unwrap();

        let (all, more) = store
            .query(
                "ctx",
                &HistoryQuery {
                    limit: Some(4),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(all.len(), 4);
        assert!(more);
        assert_eq!(all[0].name, "api");
        let (rest, more) = store
            .query(
                "ctx",
                &HistoryQuery {
                    before: Some(all[3].id),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!((rest.len(), more), (2, false));

        let only = |q: HistoryQuery| store.query("ctx", &q).unwrap().0.len();
        assert_eq!(
            only(HistoryQuery {
                changes: vec![ChangeKind::Scaled],
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            only(HistoryQuery {
                text: Some("%_".into()),
                ..Default::default()
            }),
            6
        );
        assert_eq!(
            only(HistoryQuery {
                text: Some("x%".into()),
                ..Default::default()
            }),
            0
        );
        assert_eq!(
            only(HistoryQuery {
                kind: Some("deployment".into()),
                ..Default::default()
            }),
            6
        );
        assert_eq!(
            only(HistoryQuery {
                since: Some(15_000),
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            only(HistoryQuery {
                uid: Some("uid-web2".into()),
                ..Default::default()
            }),
            1
        );

        let detail = store.detail(all[0].id).unwrap().unwrap();
        assert_eq!(detail.before, Some(json!({"spec": 1})));
        assert!(detail.entry.has_diff);

        store
            .prune(
                &HistorySettings {
                    keep_days: 1,
                    ..Default::default()
                },
                24 * 3600 * 1000 + 15_000,
            )
            .unwrap();
        assert_eq!(only(HistoryQuery::default()), 1);

        let key = store.fingerprint_key().unwrap();
        assert_eq!(key.len(), 32);
        assert_eq!(store.fingerprint_key().unwrap(), key);
    }
}
