//! Records what changes in the connected context into the [`history`](crate::history) store.
//!
//! One task watches a few kinds: workloads, CronJobs, ConfigMaps, Secrets, Services, Ingresses,
//! autoscalers, nodes and warning events. When it starts, it compares what it lists with what
//! it knew, so a change made while Kubri was closed shows up marked as such.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use kube::api::{ApiResource, DynamicObject};
use kube::runtime::{WatchStreamExt, watcher};
use kube::{Api, Client};
use serde_json::Value;

use crate::history::{
    ChangeKind, HistorySettings, Known, NewEntry, RecorderStatus, Store, describe, now_ms, stored,
};
use crate::{ResourceInfo, ResourceRef};

const BEAT: Duration = Duration::from_secs(30);
const PRUNE_EVERY: Duration = Duration::from_secs(3600);
/// A warning that keeps repeating is recorded again at most this often.
const REPEAT_WARNING_MS: i64 = 10 * 60 * 1000;
const MAX_MESSAGE: usize = 400;

/// The kinds recorded: group, plural.
const WATCHED: &[(&str, &str)] = &[
    ("apps", "deployments"),
    ("apps", "statefulsets"),
    ("apps", "daemonsets"),
    ("batch", "cronjobs"),
    ("", "configmaps"),
    ("", "secrets"),
    ("", "services"),
    ("networking.k8s.io", "ingresses"),
    ("autoscaling", "horizontalpodautoscalers"),
    ("", "nodes"),
    ("", "events"),
];

/// Records changes while it lives; dropping it stops the recording.
pub(crate) struct Recorder {
    context: String,
    task: tokio::task::JoinHandle<()>,
    status: Arc<Mutex<RecorderStatus>>,
    shared: Arc<Shared>,
}

struct Shared {
    store: Arc<Store>,
    context: String,
    /// Tells this process's claim on the context from another's.
    owner: String,
    session: Mutex<Option<i64>>,
}

impl Recorder {
    pub(crate) fn start(
        store: Arc<Store>,
        client: Client,
        context: &str,
        resources: &[ResourceInfo],
        namespaces: Option<Vec<String>>,
        settings: HistorySettings,
    ) -> Self {
        let status = Arc::new(Mutex::new(RecorderStatus::default()));
        let shared = Arc::new(Shared {
            store,
            context: context.to_owned(),
            owner: format!("{}-{}", std::process::id(), now_ms()),
            session: Mutex::new(None),
        });
        let mut kinds = Vec::new();
        let mut skipped = Vec::new();
        for (group, plural) in WATCHED {
            match resources
                .iter()
                .find(|r| r.reference.group == *group && r.reference.plural == *plural)
            {
                Some(info) if info.verbs.iter().any(|v| v == "watch") => kinds.push(info.clone()),
                Some(info) => skipped.push(format!("{}: cannot be watched", info.kind)),
                None => {}
            }
        }
        status.lock().unwrap_or_else(|e| e.into_inner()).skipped = skipped;
        let run = Run {
            shared: shared.clone(),
            status: status.clone(),
            client,
            resources: resources.to_vec(),
            settings,
        };
        let task = tokio::spawn(run.run(kinds, namespaces));
        Self {
            context: context.to_owned(),
            task,
            status,
            shared,
        }
    }

    pub(crate) fn context(&self) -> &str {
        &self.context
    }

    pub(crate) fn status(&self) -> RecorderStatus {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.task.abort();
        let session = *self.shared.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = session {
            let _ = self.shared.store.beat(session, now_ms());
        }
        let _ = self
            .shared
            .store
            .release(&self.shared.context, &self.shared.owner);
    }
}

struct Run {
    shared: Arc<Shared>,
    status: Arc<Mutex<RecorderStatus>>,
    client: Client,
    resources: Vec<ResourceInfo>,
    settings: HistorySettings,
}

impl Run {
    fn set_status(&self, change: impl FnOnce(&mut RecorderStatus)) {
        change(&mut self.status.lock().unwrap_or_else(|e| e.into_inner()));
    }

    async fn run(self, kinds: Vec<ResourceInfo>, namespaces: Option<Vec<String>>) {
        let store = &self.shared.store;
        let context = &self.shared.context;
        // Another Kubri records this context: wait for it to stop.
        loop {
            match store.claim(context, &self.shared.owner, now_ms()) {
                Ok(true) => break,
                Ok(false) => self.set_status(|s| {
                    s.reason = Some("Another Kubri window records this context".into());
                }),
                Err(err) => {
                    self.set_status(|s| s.reason = Some(err.to_string()));
                    return;
                }
            }
            tokio::time::sleep(BEAT * 2).await;
        }
        let started = now_ms();
        let prepared = (|| {
            let last = store.last_watched(context)?;
            let key = store.fingerprint_key()?;
            store.prune(&self.settings, started)?;
            Ok::<_, crate::Error>((last, key))
        })();
        let (last, key) = match prepared {
            Ok(prepared) => prepared,
            Err(err) => {
                tracing::warn!("cannot record changes: {err}");
                self.set_status(|s| s.reason = Some(err.to_string()));
                return;
            }
        };
        self.set_status(|s| {
            s.recording = true;
            s.reason = None;
            s.since = Some(started);
        });
        tracing::info!(context, "recording changes");

        let mut watches = Vec::new();
        for info in kinds {
            match (&namespaces, info.namespaced) {
                (Some(names), true) => {
                    for ns in names {
                        watches.push((info.clone(), Some(ns.clone())));
                    }
                }
                _ => watches.push((info, None)),
            }
        }
        let this = Arc::new(Watching {
            run: self,
            last,
            key,
            started,
            listing: AtomicUsize::new(watches.len()),
            warnings: Mutex::new(HashMap::new()),
        });
        if watches.is_empty() {
            this.listed();
        }
        let mut tasks: Vec<BoxFuture<'static, ()>> = vec![this.clone().beat().boxed()];
        for (info, namespace) in watches {
            tasks.push(this.clone().watch(info, namespace).boxed());
        }
        futures::future::join_all(tasks).await;
    }
}

struct Watching {
    run: Run,
    /// When Kubri last watched this context before now.
    last: Option<i64>,
    key: Vec<u8>,
    started: i64,
    /// Watches still making their first list. The watched span is written once all have, so a
    /// recording stopped before it compared everything does not count as watched.
    listing: AtomicUsize,
    /// Warning events recorded in this run: how often they had happened, and when recorded.
    warnings: Mutex<HashMap<String, (i64, i64)>>,
}

fn text_at<'a>(object: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(object, |v, key| v.get(key))?.as_str()
}

fn time_ms(text: Option<&str>) -> Option<i64> {
    let at: k8s_openapi::jiff::Timestamp = text?.parse().ok()?;
    Some(at.as_millisecond())
}

impl Watching {
    fn store(&self) -> &Store {
        &self.run.shared.store
    }

    fn context(&self) -> &str {
        &self.run.shared.context
    }

    /// One more watch has made its first list, or given up.
    fn listed(&self) {
        if self.listing.fetch_sub(1, Ordering::AcqRel) > 1 {
            return;
        }
        match self.store().start_session(self.context(), self.started) {
            Ok(session) => {
                if let Err(err) = self.store().beat(session, now_ms()) {
                    tracing::warn!("cannot record changes: {err}");
                }
                *self.run.shared.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
            }
            Err(err) => tracing::warn!("cannot record changes: {err}"),
        }
    }

    /// Keeps the claim on the context and the watched span up to date, and prunes old changes.
    async fn beat(self: Arc<Self>) {
        let mut pruned = std::time::Instant::now();
        loop {
            tokio::time::sleep(BEAT).await;
            let now = now_ms();
            let session = *self.run.shared.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(session) = session
                && let Err(err) = self.store().beat(session, now)
            {
                tracing::warn!("cannot record changes: {err}");
            }
            if !matches!(
                self.store().claim(self.context(), &self.run.shared.owner, now),
                Ok(true)
            ) {
                tracing::warn!("another Kubri took over recording {}", self.context());
            }
            if pruned.elapsed() >= PRUNE_EVERY {
                pruned = std::time::Instant::now();
                if let Err(err) = self.store().prune(&self.run.settings, now) {
                    tracing::warn!("cannot prune the change history: {err}");
                }
            }
        }
    }

    async fn watch(self: Arc<Self>, info: ResourceInfo, namespace: Option<String>) {
        let api_resource: ApiResource = info.api_resource();
        let api: Api<DynamicObject> = match &namespace {
            Some(ns) => Api::namespaced_with(self.run.client.clone(), ns, &api_resource),
            None => Api::all_with(self.run.client.clone(), &api_resource),
        };
        let events = info.kind == "Event";
        let mut config = watcher::Config::default();
        if events {
            config = config.fields("type=Warning");
        }
        let mut stream = watcher(api, config).default_backoff().boxed();
        let mut seen = HashSet::new();
        let mut first = true;
        let mut failures = 0;
        while let Some(event) = stream.next().await {
            let result = match event {
                Ok(watcher::Event::Init) => {
                    seen.clear();
                    Ok(())
                }
                Ok(watcher::Event::InitApply(object)) => {
                    let object = serde_json::to_value(object).unwrap_or_default();
                    if let Some(uid) = text_at(&object, &["metadata", "uid"]) {
                        seen.insert(uid.to_owned());
                    }
                    self.apply(&info, &object, first, events)
                }
                Ok(watcher::Event::InitDone) => {
                    let done = if events {
                        Ok(())
                    } else {
                        self.deleted_meanwhile(&info, namespace.as_deref(), &seen, first)
                    };
                    if first {
                        self.listed();
                    }
                    first = false;
                    failures = 0;
                    done
                }
                Ok(watcher::Event::Apply(object)) => {
                    let object = serde_json::to_value(object).unwrap_or_default();
                    self.apply(&info, &object, false, events)
                }
                Ok(watcher::Event::Delete(object)) => {
                    let object = serde_json::to_value(object).unwrap_or_default();
                    if events {
                        Ok(())
                    } else {
                        self.delete(&info, &object)
                    }
                }
                Err(err) => {
                    let code = match &err {
                        watcher::Error::InitialListFailed(kube::Error::Api(s))
                        | watcher::Error::WatchStartFailed(kube::Error::Api(s))
                        | watcher::Error::WatchFailed(kube::Error::Api(s)) => Some(s.code),
                        watcher::Error::WatchError(s) => Some(s.code),
                        _ => None,
                    };
                    failures += 1;
                    if matches!(code, Some(403..=405)) || (first && failures >= 5) {
                        let place = namespace
                            .as_deref()
                            .map(|ns| format!(" in {ns}"))
                            .unwrap_or_default();
                        let why = if code == Some(403) {
                            "not allowed to watch".into()
                        } else {
                            err.to_string()
                        };
                        self.run
                            .set_status(|s| s.skipped.push(format!("{}{place}: {why}", info.kind)));
                        if first {
                            self.listed();
                        }
                        return;
                    }
                    tracing::debug!(kind = %info.kind, "recorder watch: {err}");
                    Ok(())
                }
            };
            if let Err(err) = result {
                tracing::warn!("cannot record a change: {err}");
            }
        }
    }

    fn resource_of(&self, api_version: &str, kind: &str) -> ResourceRef {
        let (group, version) = api_version.rsplit_once('/').unwrap_or(("", api_version));
        self.run
            .resources
            .iter()
            .find(|r| r.reference.group == group && r.kind == kind)
            .map(|r| r.reference.clone())
            .unwrap_or_else(|| ResourceRef {
                group: group.to_owned(),
                version: version.to_owned(),
                plural: format!("{}s", kind.to_lowercase()),
            })
    }

    fn apply(&self, info: &ResourceInfo, object: &Value, first: bool, event: bool) -> crate::Result<()> {
        if event {
            return self.warning(object, first);
        }
        let Some(uid) = text_at(object, &["metadata", "uid"]) else {
            return Ok(());
        };
        let name = text_at(object, &["metadata", "name"]).unwrap_or_default();
        let namespace = text_at(object, &["metadata", "namespace"]);
        let state = stored(&info.kind, object, &self.key);
        let text = state.to_string();
        let now = now_ms();
        let entry = |at, change, summary: String, unwatched, before| NewEntry {
            at,
            resource: info.reference.clone(),
            kind: info.kind.clone(),
            namespace: namespace.map(str::to_owned),
            name: name.to_owned(),
            uid: uid.to_owned(),
            change,
            summary,
            unwatched,
            before,
            after: Some(state.clone()),
        };
        match self.store().known(self.context(), uid)? {
            None => {
                let created = time_ms(text_at(object, &["metadata", "creationTimestamp"]));
                // Objects found on the first list are new only if made since Kubri last watched;
                // the rest existed before anyone recorded them.
                let new = if first {
                    matches!((self.last, created), (Some(last), Some(created)) if created + 1000 > last)
                } else {
                    true
                };
                if new {
                    let at = match created {
                        Some(c) if first || (now - c).abs() < 600_000 => c,
                        _ => now,
                    };
                    self.store().record(
                        self.context(),
                        &entry(at, ChangeKind::Created, "Created".into(), false, None),
                    )?;
                }
            }
            Some(old) if old != text => {
                let before: Value = serde_json::from_str(&old).unwrap_or_default();
                if let Some((change, summary)) = describe(&info.kind, &before, &state) {
                    self.store()
                        .record(self.context(), &entry(now, change, summary, first, Some(before)))?;
                }
            }
            Some(_) => return Ok(()),
        }
        let known = Known {
            uid: uid.to_owned(),
            namespace: namespace.map(str::to_owned),
            name: name.to_owned(),
            state: text,
        };
        self.store()
            .put_state(self.context(), &info.reference.group, &info.kind, &known)
    }

    fn delete(&self, info: &ResourceInfo, object: &Value) -> crate::Result<()> {
        let Some(uid) = text_at(object, &["metadata", "uid"]) else {
            return Ok(());
        };
        let before = self
            .store()
            .known(self.context(), uid)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .or_else(|| Some(stored(&info.kind, object, &self.key)));
        self.store().record(
            self.context(),
            &NewEntry {
                at: now_ms(),
                resource: info.reference.clone(),
                kind: info.kind.clone(),
                namespace: text_at(object, &["metadata", "namespace"]).map(str::to_owned),
                name: text_at(object, &["metadata", "name"])
                    .unwrap_or_default()
                    .to_owned(),
                uid: uid.to_owned(),
                change: ChangeKind::Deleted,
                summary: "Deleted".into(),
                unwatched: false,
                before,
                after: None,
            },
        )?;
        self.store().forget(self.context(), uid)
    }

    /// Objects Kubri knew that a fresh list no longer has were deleted meanwhile.
    fn deleted_meanwhile(
        &self,
        info: &ResourceInfo,
        namespace: Option<&str>,
        seen: &HashSet<String>,
        first: bool,
    ) -> crate::Result<()> {
        let known =
            self.store()
                .known_of_kind(self.context(), &info.reference.group, &info.kind, namespace)?;
        for gone in known.into_iter().filter(|k| !seen.contains(&k.uid)) {
            self.store().record(
                self.context(),
                &NewEntry {
                    at: now_ms(),
                    resource: info.reference.clone(),
                    kind: info.kind.clone(),
                    namespace: gone.namespace.clone(),
                    name: gone.name.clone(),
                    uid: gone.uid.clone(),
                    change: ChangeKind::Deleted,
                    summary: "Deleted".into(),
                    unwatched: first,
                    before: serde_json::from_str(&gone.state).ok(),
                    after: None,
                },
            )?;
            self.store().forget(self.context(), &gone.uid)?;
        }
        Ok(())
    }

    /// Records a warning event once, and again when it keeps happening.
    fn warning(&self, event: &Value, first: bool) -> crate::Result<()> {
        let Some(uid) = text_at(event, &["metadata", "uid"]) else {
            return Ok(());
        };
        let count = event["count"]
            .as_i64()
            .or_else(|| event["series"]["count"].as_i64())
            .unwrap_or(1);
        let at = time_ms(text_at(event, &["lastTimestamp"]))
            .or_else(|| time_ms(text_at(event, &["series", "lastObservedTime"])))
            .or_else(|| time_ms(text_at(event, &["eventTime"])))
            .or_else(|| time_ms(text_at(event, &["metadata", "creationTimestamp"])))
            .unwrap_or_else(now_ms);
        let now = now_ms();
        {
            let mut warnings = self.warnings.lock().unwrap_or_else(|e| e.into_inner());
            match warnings.get(uid) {
                Some(&(seen, recorded)) if count <= seen || now - recorded < REPEAT_WARNING_MS => {
                    warnings.insert(uid.to_owned(), (count.max(seen), recorded));
                    return Ok(());
                }
                _ => {}
            }
            // On the first list, only what happened since Kubri last watched is new.
            let old = first && self.last.is_none_or(|last| at <= last);
            warnings.insert(uid.to_owned(), (count, if old { 0 } else { now }));
            if old {
                return Ok(());
            }
        }
        let involved = &event["involvedObject"];
        let kind = involved["kind"].as_str().unwrap_or("Object");
        let reason = event["reason"].as_str().unwrap_or("Warning");
        let mut message = event["message"].as_str().unwrap_or_default().trim().to_owned();
        if message.chars().count() > MAX_MESSAGE {
            message = message.chars().take(MAX_MESSAGE).collect::<String>() + "…";
        }
        let mut summary = if message.is_empty() {
            reason.to_owned()
        } else {
            format!("{reason}: {message}")
        };
        if count > 1 {
            summary.push_str(&format!(" (×{count})"));
        }
        self.store().record(
            self.context(),
            &NewEntry {
                at,
                resource: self.resource_of(involved["apiVersion"].as_str().unwrap_or("v1"), kind),
                kind: kind.to_owned(),
                namespace: involved["namespace"]
                    .as_str()
                    .or_else(|| text_at(event, &["metadata", "namespace"]))
                    .map(str::to_owned),
                name: involved["name"].as_str().unwrap_or_default().to_owned(),
                uid: involved["uid"].as_str().unwrap_or_default().to_owned(),
                change: ChangeKind::Warning,
                summary,
                unwatched: false,
                before: None,
                after: None,
            },
        )?;
        Ok(())
    }
}
