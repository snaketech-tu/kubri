//! Live container logs, merged across the containers of a pod or the pods of a workload.
//!
//! Each line comes with what Kubri recognizes in it: the time the kubelet received it, its
//! level, and its fields when it is JSON or logfmt. Pods that join a workload while its logs are
//! followed, such as during a rollout, join the stream.

use std::collections::{HashMap, HashSet};
use std::pin::Pin;

use futures::io::AsyncBufRead;
use futures::stream::{self, BoxStream, SelectAll};
use futures::{AsyncBufReadExt, FutureExt, Stream, StreamExt};
use k8s_openapi::api::core::v1::{ContainerStatus, Pod};
use kube::api::LogParams;
use kube::runtime::watcher;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::mpsc;

/// More streams than this would flood the UI; `kubectl logs -l` stops at 5 by default.
pub(crate) const MAX_STREAMS: usize = 50;
/// Lines sent in one event at most, so a burst does not become one huge message.
const MAX_BATCH: usize = 500;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LogOptions {
    /// Only this container. `None` streams every container, init containers included.
    pub container: Option<String>,
    /// Keep streaming new lines.
    pub follow: bool,
    /// Logs of the previous, crashed instance of the container.
    pub previous: bool,
    /// Prefix the text of every line with the time the kubelet received it. Lines always carry
    /// the time apart, in [`LogLine::time`]; this is for readers of the text alone.
    pub timestamps: bool,
    /// Start with the last n lines.
    pub tail_lines: Option<i64>,
    /// Only lines newer than this.
    pub since_seconds: Option<i64>,
}

impl LogOptions {
    fn params(&self, container: &str) -> LogParams {
        LogParams {
            container: Some(container.to_owned()),
            follow: self.follow,
            previous: self.previous,
            // Always: the time is split off into its own field.
            timestamps: true,
            tail_lines: self.tail_lines,
            since_seconds: self.since_seconds,
            ..Default::default()
        }
    }
}

/// One container whose log is being streamed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSource {
    #[serde(skip)]
    pub namespace: String,
    pub pod: String,
    pub container: String,
}

/// How severe a log line is, as far as its text tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    /// Index into the list of [`LogEvent::Sources`].
    pub source: usize,
    /// The line without the kubelet's timestamp.
    pub text: String,
    /// When the kubelet received the line, as RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// The level the line names. Lines that continue the one before, such as the frames of a
    /// stack trace, take its level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    /// The fields of a JSON or logfmt line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Map<String, Value>>,
    /// The line continues the one before it from the same container, like a stack frame.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub continued: bool,
}

impl LogLine {
    /// A line as the kubelet sent it, with a timestamp in front.
    pub(crate) fn parse(source: usize, raw: &str) -> Self {
        let (time, text) = split_time(raw);
        let fields = json_fields(text).or_else(|| logfmt_fields(text));
        let level = fields.as_ref().and_then(field_level).or_else(|| text_level(text));
        Self {
            source,
            text: text.to_owned(),
            time: time.map(str::to_owned),
            level,
            fields,
            continued: is_continuation(text),
        }
    }

    /// The text with the time in front, as `kubectl logs --timestamps` shows it.
    pub fn with_time(&self) -> String {
        match &self.time {
            Some(time) => format!("{time} {}", self.text),
            None => self.text.clone(),
        }
    }
}

/// Splits the kubelet's RFC 3339 timestamp off a line.
fn split_time(raw: &str) -> (Option<&str>, &str) {
    if let Some((time, text)) = raw.split_once(' ') {
        let b = time.as_bytes();
        let looks_like_time =
            b.len() >= 20 && b[..4].iter().all(u8::is_ascii_digit) && b[4] == b'-' && b[10] == b'T';
        if looks_like_time {
            return (Some(time), text);
        }
    }
    // An empty line comes as the timestamp alone.
    let b = raw.as_bytes();
    if b.len() >= 20
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[10] == b'T'
        && !raw.contains(' ')
    {
        return (Some(raw), "");
    }
    (None, raw)
}

fn json_fields(text: &str) -> Option<Map<String, Value>> {
    let trimmed = text.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }
    match serde_json::from_str(trimmed) {
        Ok(Value::Object(map)) if !map.is_empty() => Some(map),
        _ => None,
    }
}

/// `key=value key2="quoted value"` lines, with at least two pairs and nothing else.
fn logfmt_fields(text: &str) -> Option<Map<String, Value>> {
    let mut map = Map::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let eq = rest.find('=')?;
        let key = &rest[..eq];
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/' | '@'))
        {
            return None;
        }
        rest = &rest[eq + 1..];
        let value = if let Some(quoted) = rest.strip_prefix('"') {
            let mut value = String::new();
            let mut chars = quoted.char_indices();
            let mut end = None;
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => {
                        if let Some((_, next)) = chars.next() {
                            value.push(match next {
                                'n' => '\n',
                                't' => '\t',
                                other => other,
                            });
                        }
                    }
                    '"' => {
                        end = Some(i);
                        break;
                    }
                    c => value.push(c),
                }
            }
            rest = &quoted[end? + 1..];
            value
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let value = rest[..end].to_owned();
            rest = &rest[end..];
            value
        };
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return None;
        }
        rest = rest.trim_start();
        map.insert(key.to_owned(), Value::String(value));
    }
    (map.len() >= 2).then_some(map)
}

/// The level a word names: `error`, `WARN`, `Information`, `dbg`…
fn level_word(word: &str) -> Option<Level> {
    let word = word.to_ascii_lowercase();
    Some(match word.as_str() {
        "error" | "err" | "eror" | "fatal" | "panic" | "critical" | "crit" | "alert" | "emerg"
        | "emergency" | "severe" | "dpanic" => Level::Error,
        "warn" | "warning" | "wrn" => Level::Warn,
        "info" | "information" | "inf" | "notice" => Level::Info,
        "debug" | "dbg" | "trace" | "trc" | "verbose" | "fine" => Level::Debug,
        _ => return None,
    })
}

/// The level in a structured line's fields: `level`, `severity`, `lvl`…, or pino's numbers.
fn field_level(fields: &Map<String, Value>) -> Option<Level> {
    const KEYS: [&str; 8] = [
        "level",
        "lvl",
        "severity",
        "loglevel",
        "log.level",
        "levelname",
        "@l",
        "log_level",
    ];
    let value = KEYS.iter().find_map(|k| {
        fields
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(k))
            .map(|(_, v)| v)
    })?;
    match value {
        Value::String(s) => level_word(s),
        Value::Number(n) => n.as_u64().map(|n| match n {
            0..=29 => Level::Debug,
            30..=39 => Level::Info,
            40..=49 => Level::Warn,
            _ => Level::Error,
        }),
        _ => None,
    }
}

/// The level in a plain line: klog's `E1029`, or a level word near the start.
fn text_level(text: &str) -> Option<Level> {
    let b = text.as_bytes();
    // klog: `E1029 17:09:31.123456 …`
    if b.len() > 6 && b[1..5].iter().all(u8::is_ascii_digit) && b[5] == b' ' {
        match b[0] {
            b'E' | b'F' => return Some(Level::Error),
            b'W' => return Some(Level::Warn),
            b'I' => return Some(Level::Info),
            _ => {}
        }
    }
    // The first line of a stack trace: `Exception in thread…`, `Traceback (most recent…`,
    // `panic: …`, `java.lang.IllegalStateException: …`.
    let first = text.split_whitespace().next().unwrap_or("");
    let first = first.trim_end_matches(':');
    if matches!(first, "Exception" | "Traceback" | "panic" | "Unhandled")
        || first.ends_with("Exception")
        || (first.ends_with("Error") && first.contains('.'))
    {
        return Some(Level::Error);
    }
    let head: String = text.chars().take(120).collect();
    // A level word near the start, alone or in brackets: `[ERROR]`, `level=warn`, `WARN:`.
    for word in head
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
    {
        let upper = word.chars().all(|c| c.is_ascii_uppercase());
        if (upper || word.len() >= 4)
            && let Some(level) = level_word(word)
            && (upper || matches!(level, Level::Error | Level::Warn))
        {
            return Some(level);
        }
    }
    None
}

/// Lines that continue the one before: stack frames, `Caused by:`, indented text.
fn is_continuation(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let trimmed = text.trim_start();
    text.starts_with([' ', '\t'])
        || trimmed.starts_with("at ")
        || trimmed.starts_with("Caused by:")
        || trimmed.starts_with("... ")
        || trimmed.starts_with("File \"")
        || trimmed.starts_with("goroutine ")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LogEvent {
    /// Sent first: the containers being streamed.
    Sources {
        sources: Vec<LogSource>,
    },
    Lines {
        lines: Vec<LogLine>,
    },
    /// A container's log could not be read, for example because it has not started yet.
    /// The other containers keep streaming.
    Error {
        source: Option<usize>,
        message: String,
    },
    /// Something happened to the stream, such as a pod that joined during a rollout or a
    /// container whose log ended. Shown as a line across the log.
    Marker {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        time: Option<String>,
    },
    /// Every stream has ended.
    End,
}

/// The containers to stream for `pods`, in pod order.
pub(crate) fn sources(pods: &[Pod], container: Option<&str>) -> Vec<LogSource> {
    let mut sources = Vec::new();
    for pod in pods {
        let namespace = pod.metadata.namespace.clone().unwrap_or_default();
        let name = pod.metadata.name.clone().unwrap_or_default();
        let mut add = |container: &str| {
            sources.push(LogSource {
                namespace: namespace.clone(),
                pod: name.clone(),
                container: container.to_owned(),
            })
        };
        if let Some(container) = container {
            add(container);
            continue;
        }
        let (Some(spec), status) = (&pod.spec, pod.status.as_ref()) else {
            continue;
        };
        let started = |statuses: Option<&Vec<ContainerStatus>>, name: &str| {
            statuses
                .into_iter()
                .flatten()
                .any(|s| s.name == name && has_started(s))
        };
        // Init and debug containers only have logs once they ran, unlike the main containers,
        // whose "waiting to start" error is worth showing.
        for init in spec.init_containers.iter().flatten() {
            if started(
                status.and_then(|s| s.init_container_statuses.as_ref()),
                &init.name,
            ) {
                add(&init.name);
            }
        }
        for main in &spec.containers {
            add(&main.name);
        }
        for debug in spec.ephemeral_containers.iter().flatten() {
            if started(
                status.and_then(|s| s.ephemeral_container_statuses.as_ref()),
                &debug.name,
            ) {
                add(&debug.name);
            }
        }
    }
    sources
}

fn has_started(status: &ContainerStatus) -> bool {
    let state = status.state.as_ref();
    state.is_some_and(|s| s.running.is_some() || s.terminated.is_some())
        || status.last_state.as_ref().is_some_and(|s| s.terminated.is_some())
}

/// Pods that may join a workload's log while it is followed: those `selector` picks.
pub(crate) struct Joiner {
    pub api: Api<Pod>,
    pub selector: String,
    /// Pods whose containers had started when the stream began; the others join once they do.
    pub started: HashSet<String>,
}

/// Whether a container of `pod` has run, so that it has a log.
pub(crate) fn pod_started(pod: &Pod) -> bool {
    pod.status
        .as_ref()
        .and_then(|s| s.container_statuses.as_ref())
        .is_some_and(|statuses| statuses.iter().any(has_started))
}

/// Streams the logs of `sources` until every stream ends or the returned stream is dropped.
/// With `joiner`, pods that start later join, and the stream lasts while the watch does.
pub(crate) fn stream_logs(
    client: Client,
    sources: Vec<LogSource>,
    options: LogOptions,
    joiner: Option<Joiner>,
) -> impl Stream<Item = LogEvent> + Send + 'static {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        tokio::select! {
            _ = run(client, sources, options, joiner, &tx) => {}
            _ = tx.closed() => {}
        }
    });
    stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|event| (event, rx)) },
    )
}

/// What one container's log stream gives.
enum Item {
    Line(usize, String),
    Failed(usize, String),
    Ended(usize),
}

type Lines = BoxStream<'static, Item>;

async fn open(client: &Client, index: usize, source: &LogSource, params: LogParams) -> Lines {
    let api: Api<Pod> = Api::namespaced(client.clone(), &source.namespace);
    match api.log_stream(&source.pod, &params).await {
        Ok(reader) => lines(Box::pin(reader))
            .map(move |line| match line {
                Ok(text) => Item::Line(index, text),
                Err(err) => Item::Failed(index, err),
            })
            .chain(stream::once(async move { Item::Ended(index) }))
            .boxed(),
        Err(err) => {
            stream::once(async move { Item::Failed(index, crate::Error::from(err).to_string()) }).boxed()
        }
    }
}

async fn run(
    client: Client,
    mut sources: Vec<LogSource>,
    options: LogOptions,
    joiner: Option<Joiner>,
    tx: &mpsc::Sender<LogEvent>,
) {
    let opened = sources
        .iter()
        .enumerate()
        .map(|(index, source)| open(&client, index, source, options.params(&source.container)));
    let mut streams: SelectAll<Lines> = SelectAll::new();
    for s in futures::future::join_all(opened).await {
        streams.push(s);
    }
    if tx
        .send(LogEvent::Sources {
            sources: sources.clone(),
        })
        .await
        .is_err()
    {
        return;
    }
    let several_pods = sources.iter().map(|s| &s.pod).collect::<HashSet<_>>().len() > 1;
    let joiner = joiner.filter(|_| options.follow && !options.previous);
    let mut known: HashSet<String> = match &joiner {
        Some(j) => j.started.clone(),
        None => sources.iter().map(|s| s.pod.clone()).collect(),
    };
    let mut pods = joiner.map(|j| watcher(j.api, watcher::Config::default().labels(&j.selector)).boxed());
    // The level of each container's last line, for the lines that continue it.
    let mut last_level: HashMap<usize, Option<Level>> = HashMap::new();

    loop {
        let item = if streams.is_empty() {
            None
        } else if let Some(item) = streams.next().now_or_never() {
            item
        } else {
            tokio::select! {
                item = streams.next() => item,
                Some(event) = async { match pods.as_mut() { Some(p) => p.next().await, None => None } } => {
                    if let Ok(event) = event {
                        joined(&client, &options, event, &mut sources, &mut known, &mut streams, tx).await;
                    }
                    continue;
                }
            }
        };
        let Some(first) = item else {
            // Every stream ended. While pods may still join, wait for them.
            match pods.as_mut() {
                Some(p) => match p.next().await {
                    Some(Ok(event)) => {
                        joined(
                            &client,
                            &options,
                            event,
                            &mut sources,
                            &mut known,
                            &mut streams,
                            tx,
                        )
                        .await;
                        continue;
                    }
                    Some(Err(_)) => continue,
                    None => break,
                },
                None => break,
            }
        };
        let mut batch = vec![first];
        while batch.len() < MAX_BATCH {
            match streams.next().now_or_never() {
                Some(Some(item)) => batch.push(item),
                _ => break,
            }
        }
        let mut lines = Vec::with_capacity(batch.len());
        for item in batch {
            let event = match item {
                Item::Line(source, raw) => {
                    let mut line = LogLine::parse(source, &raw);
                    let last = last_level.entry(source).or_default();
                    if line.continued && line.level.is_none() {
                        line.level = *last;
                    } else {
                        *last = line.level;
                    }
                    lines.push(line);
                    continue;
                }
                Item::Failed(source, message) => LogEvent::Error {
                    source: Some(source),
                    message,
                },
                // Without follow every log ends; that is not news.
                Item::Ended(source) if options.follow && (several_pods || pods.is_some()) => {
                    let s = &sources[source];
                    LogEvent::Marker {
                        text: format!(
                            "{}/{}: the log ended (the container stopped or restarted)",
                            s.pod, s.container
                        ),
                        time: Some(now()),
                    }
                }
                Item::Ended(_) => continue,
            };
            if !lines.is_empty()
                && tx
                    .send(LogEvent::Lines {
                        lines: std::mem::take(&mut lines),
                    })
                    .await
                    .is_err()
            {
                return;
            }
            if tx.send(event).await.is_err() {
                return;
            }
        }
        if !lines.is_empty() && tx.send(LogEvent::Lines { lines }).await.is_err() {
            return;
        }
    }
    let _ = tx.send(LogEvent::End).await;
}

/// Adds the containers of a pod that started since the stream began, and says so.
async fn joined(
    client: &Client,
    options: &LogOptions,
    event: watcher::Event<Pod>,
    sources: &mut Vec<LogSource>,
    known: &mut HashSet<String>,
    streams: &mut SelectAll<Lines>,
    tx: &mpsc::Sender<LogEvent>,
) {
    let pod = match event {
        watcher::Event::Apply(pod) | watcher::Event::InitApply(pod) => pod,
        _ => return,
    };
    let name = pod.metadata.name.clone().unwrap_or_default();
    if known.contains(&name)
        || !pod_started(&pod)
        || pod.metadata.deletion_timestamp.is_some()
        || sources.len() >= MAX_STREAMS
    {
        return;
    }
    known.insert(name.clone());
    let added = self::sources(std::slice::from_ref(&pod), options.container.as_deref());
    for source in added {
        let index = sources.len();
        // From its first line: the pod is new.
        let params = LogParams {
            container: Some(source.container.clone()),
            follow: true,
            timestamps: true,
            ..Default::default()
        };
        streams.push(open(client, index, &source, params).await);
        sources.push(source);
    }
    let _ = tx
        .send(LogEvent::Sources {
            sources: sources.clone(),
        })
        .await;
    let _ = tx
        .send(LogEvent::Marker {
            text: format!("Pod {name} joined"),
            time: Some(now()),
        })
        .await;
}

fn now() -> String {
    k8s_openapi::jiff::Timestamp::now().to_string()
}

/// Splits a log into lines. Unlike `AsyncBufReadExt::lines` this keeps lines that are not
/// valid UTF-8, and it stops after the first read error.
fn lines<R>(reader: Pin<Box<R>>) -> impl Stream<Item = Result<String, String>> + Send + 'static
where
    R: AsyncBufRead + Send + ?Sized + 'static,
{
    stream::unfold(Some(reader), |reader| async move {
        let mut reader = reader?;
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) => None,
            Ok(_) => Some((Ok(decode_line(buf)), Some(reader))),
            Err(err) => Some((Err(err.to_string()), None)),
        }
    })
}

fn decode_line(mut buf: Vec<u8>) -> String {
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
    String::from_utf8(buf).unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pod(value: serde_json::Value) -> Pod {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn streams_main_containers_and_init_containers_that_ran() {
        let pod = pod(json!({
            "metadata": { "name": "web-1", "namespace": "shop" },
            "spec": {
                "initContainers": [{ "name": "migrate" }, { "name": "seed" }],
                "containers": [{ "name": "app" }, { "name": "proxy" }]
            },
            "status": {
                "initContainerStatuses": [
                    { "name": "migrate", "ready": false, "restartCount": 0, "image": "", "imageID": "",
                      "state": { "terminated": { "exitCode": 0 } } },
                    { "name": "seed", "ready": false, "restartCount": 0, "image": "", "imageID": "",
                      "state": { "waiting": { "reason": "PodInitializing" } } }
                ]
            }
        }));
        let names: Vec<_> = sources(&[pod], None).into_iter().map(|s| s.container).collect();
        assert_eq!(names, ["migrate", "app", "proxy"]);
    }

    #[test]
    fn a_chosen_container_is_streamed_from_every_pod() {
        let pods = [
            pod(json!({ "metadata": { "name": "a", "namespace": "ns" }, "spec": { "containers": [] } })),
            pod(json!({ "metadata": { "name": "b", "namespace": "ns" }, "spec": { "containers": [] } })),
        ];
        let sources = sources(&pods, Some("app"));
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[1].pod, "b");
        assert_eq!(sources[1].namespace, "ns");
        assert_eq!(sources[1].container, "app");
    }

    #[test]
    fn options_map_to_log_params() {
        let options: LogOptions =
            serde_json::from_value(json!({ "follow": true, "tailLines": 100, "timestamps": true })).unwrap();
        let params = options.params("app");
        assert_eq!(params.container.as_deref(), Some("app"));
        assert!(params.follow && params.timestamps && !params.previous);
        assert_eq!(params.tail_lines, Some(100));
    }

    #[tokio::test]
    async fn splits_lines_and_keeps_invalid_utf8() {
        let input: &[u8] = b"one\r\ntwo\n\xffthree";
        let lines: Vec<_> = lines(Box::pin(input)).collect().await;
        assert_eq!(
            lines,
            [
                Ok("one".to_owned()),
                Ok("two".to_owned()),
                Ok("\u{fffd}three".to_owned())
            ]
        );
    }

    #[tokio::test]
    async fn merges_containers_and_reports_failures_per_container() {
        let (client, requests) = crate::testing::client(|req| {
            if req.uri.contains("/pods/web-1/log") {
                (
                    200,
                    "2026-10-29T10:00:00.000000001Z first\n2026-10-29T10:00:01.000000001Z ERROR second\n    at main\n"
                        .to_owned(),
                )
            } else {
                (
                    400,
                    crate::testing::status(400, "container \"app\" is waiting to start"),
                )
            }
        });
        let source = |pod: &str| LogSource {
            namespace: "shop".into(),
            pod: pod.into(),
            container: "app".into(),
        };
        let options = LogOptions {
            follow: true,
            tail_lines: Some(10),
            ..Default::default()
        };
        let events: Vec<LogEvent> =
            stream_logs(client, vec![source("web-1"), source("web-2")], options, None)
                .collect()
                .await;

        assert!(matches!(&events[0], LogEvent::Sources { sources } if sources.len() == 2));
        assert_eq!(events.last(), Some(&LogEvent::End));
        let lines: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                LogEvent::Lines { lines } => Some(lines.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(
            lines,
            [
                LogLine {
                    source: 0,
                    text: "first".into(),
                    time: Some("2026-10-29T10:00:00.000000001Z".into()),
                    level: None,
                    fields: None,
                    continued: false
                },
                LogLine {
                    source: 0,
                    text: "ERROR second".into(),
                    time: Some("2026-10-29T10:00:01.000000001Z".into()),
                    level: Some(Level::Error),
                    fields: None,
                    continued: false
                },
                LogLine {
                    source: 0,
                    text: "    at main".into(),
                    time: None,
                    level: Some(Level::Error),
                    fields: None,
                    continued: true
                }
            ]
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, LogEvent::Marker { text, .. } if text.starts_with("web-1/app")))
        );
        assert!(events.contains(&LogEvent::Error {
            source: Some(1),
            message: "container \"app\" is waiting to start".into()
        }));

        let uris: Vec<_> = requests.lock().unwrap().iter().map(|r| r.uri.clone()).collect();
        assert!(
            uris.contains(&"/api/v1/namespaces/shop/pods/web-1/log?&container=app&follow=true&tailLines=10&timestamps=true".to_owned()),
            "{uris:?}"
        );
    }

    #[test]
    fn splits_the_kubelet_time() {
        assert_eq!(
            split_time("2026-10-29T10:00:00.123Z hello world"),
            (Some("2026-10-29T10:00:00.123Z"), "hello world")
        );
        assert_eq!(
            split_time("2026-10-29T10:00:00.123Z"),
            (Some("2026-10-29T10:00:00.123Z"), "")
        );
        assert_eq!(split_time("hello 2026"), (None, "hello 2026"));
        let line = LogLine::parse(0, "2026-10-29T10:00:00Z x");
        assert_eq!(line.with_time(), "2026-10-29T10:00:00Z x");
    }

    #[test]
    fn reads_json_and_logfmt_fields() {
        let line = LogLine::parse(0, r#"{"level":"warn","msg":"slow","status":503}"#);
        assert_eq!(line.level, Some(Level::Warn));
        assert_eq!(line.fields.unwrap()["status"], json!(503));

        let line = LogLine::parse(0, r#"time=10:00 level=error msg="no route to host" code=7"#);
        assert_eq!(line.level, Some(Level::Error));
        let fields = line.fields.unwrap();
        assert_eq!(fields["msg"], json!("no route to host"));
        assert_eq!(fields["code"], json!("7"));

        // pino numbers
        assert_eq!(
            LogLine::parse(0, r#"{"level":50,"msg":"x"}"#).level,
            Some(Level::Error)
        );
        // One pair or prose is not logfmt.
        assert!(LogLine::parse(0, "retries=3").fields.is_none());
        assert!(LogLine::parse(0, "set a=1 and b=2").fields.is_none());
        assert!(LogLine::parse(0, "{not json}").fields.is_none());
    }

    #[test]
    fn recognizes_levels_in_plain_lines() {
        let level = |text: &str| LogLine::parse(0, text).level;
        assert_eq!(
            level("E1029 17:09:31.123456 1 reflector.go] failed"),
            Some(Level::Error)
        );
        assert_eq!(level("W1029 17:09:31.123456 1 x.go] slow"), Some(Level::Warn));
        assert_eq!(level("[INFO] started"), Some(Level::Info));
        assert_eq!(level("2026/10/29 WARN: disk"), Some(Level::Warn));
        assert_eq!(level("Error: connection refused"), Some(Level::Error));
        assert_eq!(
            level("Exception in thread \"main\" java.lang.IllegalStateException"),
            Some(Level::Error)
        );
        assert_eq!(level("java.lang.NullPointerException: x"), Some(Level::Error));
        assert_eq!(level("Traceback (most recent call last):"), Some(Level::Error));
        assert_eq!(
            level("panic: runtime error: index out of range"),
            Some(Level::Error)
        );
        // Lowercase info/debug words in prose are not levels; errors and warnings are.
        assert_eq!(level("more info at the docs"), None);
        assert_eq!(level("no errors found"), None);
        assert_eq!(level("GET /health 200"), None);
    }

    #[test]
    fn recognizes_continuation_lines() {
        assert!(is_continuation("    at com.shop.Main.run(Main.java:12)"));
        assert!(is_continuation("Caused by: java.io.IOException"));
        assert!(is_continuation("\tat x"));
        assert!(!is_continuation("starting"));
        assert!(!is_continuation(""));
    }
}
