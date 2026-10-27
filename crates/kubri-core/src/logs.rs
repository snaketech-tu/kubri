//! Live container logs, merged across the containers of a pod or the pods of a workload.

use std::pin::Pin;

use futures::io::AsyncBufRead;
use futures::stream::{self, BoxStream};
use futures::{AsyncBufReadExt, Stream, StreamExt};
use k8s_openapi::api::core::v1::{ContainerStatus, Pod};
use kube::api::LogParams;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
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
    /// Prefix every line with the time the kubelet received it.
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
            timestamps: self.timestamps,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    /// Index into the list of [`LogEvent::Sources`].
    pub source: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// Streams the logs of `sources` until every stream ends or the returned stream is dropped.
pub(crate) fn stream_logs(
    client: Client,
    sources: Vec<LogSource>,
    options: LogOptions,
) -> impl Stream<Item = LogEvent> + Send + 'static {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        tokio::select! {
            _ = run(client, sources, options, &tx) => {}
            _ = tx.closed() => {}
        }
    });
    stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|event| (event, rx)) },
    )
}

async fn run(client: Client, sources: Vec<LogSource>, options: LogOptions, tx: &mpsc::Sender<LogEvent>) {
    let opened = sources.iter().enumerate().map(|(index, source)| {
        let api: Api<Pod> = Api::namespaced(client.clone(), &source.namespace);
        let params = options.params(&source.container);
        let pod = source.pod.clone();
        async move {
            match api.log_stream(&pod, &params).await {
                Ok(reader) => lines(Box::pin(reader)).map(move |line| (index, line)).boxed(),
                Err(err) => {
                    stream::once(async move { (index, Err(crate::Error::from(err).to_string())) }).boxed()
                }
            }
        }
    });
    let streams: Vec<BoxStream<'static, (usize, Result<String, String>)>> =
        futures::future::join_all(opened).await;
    if tx.send(LogEvent::Sources { sources }).await.is_err() {
        return;
    }

    let mut batches = stream::select_all(streams).ready_chunks(MAX_BATCH);
    while let Some(batch) = batches.next().await {
        let mut lines = Vec::with_capacity(batch.len());
        for (source, line) in batch {
            match line {
                Ok(text) => lines.push(LogLine { source, text }),
                Err(message) => {
                    if !lines.is_empty() {
                        let lines = std::mem::take(&mut lines);
                        if tx.send(LogEvent::Lines { lines }).await.is_err() {
                            return;
                        }
                    }
                    let error = LogEvent::Error {
                        source: Some(source),
                        message,
                    };
                    if tx.send(error).await.is_err() {
                        return;
                    }
                }
            }
        }
        if !lines.is_empty() && tx.send(LogEvent::Lines { lines }).await.is_err() {
            return;
        }
    }
    let _ = tx.send(LogEvent::End).await;
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
                (200, "first\nsecond\n".to_owned())
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
            tail_lines: Some(10),
            ..Default::default()
        };
        let events: Vec<LogEvent> = stream_logs(client, vec![source("web-1"), source("web-2")], options)
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
                    text: "first".into()
                },
                LogLine {
                    source: 0,
                    text: "second".into()
                }
            ]
        );
        assert!(events.contains(&LogEvent::Error {
            source: Some(1),
            message: "container \"app\" is waiting to start".into()
        }));

        let uris: Vec<_> = requests.lock().unwrap().iter().map(|r| r.uri.clone()).collect();
        assert!(
            uris.contains(&"/api/v1/namespaces/shop/pods/web-1/log?&container=app&tailLines=10".to_owned())
        );
    }
}
