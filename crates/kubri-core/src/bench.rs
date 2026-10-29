//! HTTP benchmarks through a running port-forward, like the `hey` runs of k9s.
//!
//! Only the local end of a port-forward can be targeted, so a benchmark always hits a pod
//! the user forwarded to, never an arbitrary host.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::{HeaderName, HeaderValue, Method, Request, Uri};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::{Error, ObjectRef, PortForwardInfo, Result};

const MAX_CONCURRENCY: u32 = 200;
const MAX_REQUESTS: u32 = 1_000_000;
const MAX_DURATION_SECS: u32 = 600;
const MAX_TIMEOUT_SECS: u32 = 120;
const DEFAULT_TIMEOUT_SECS: u32 = 20;
const MAX_BODY: usize = 1024 * 1024;
/// Finished runs kept for the list; older ones are dropped.
const KEPT: usize = 20;
const PERCENTILES: [u8; 7] = [10, 25, 50, 75, 90, 95, 99];
const BUCKETS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchRequest {
    /// The port-forward to send the requests through.
    pub forward_id: u32,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default = "default_path")]
    pub path: String,
    /// Extra headers; `Host` overrides the default `localhost:<port>`.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    pub concurrency: u32,
    /// Stop after this many requests…
    #[serde(default)]
    pub requests: Option<u32>,
    /// …or after this many seconds. Exactly one of the two is set.
    #[serde(default)]
    pub duration_secs: Option<u32>,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

fn default_method() -> String {
    "GET".to_owned()
}

fn default_path() -> String {
    "/".to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BenchState {
    Running,
    Done,
    /// Stopped by the user; the results cover what finished until then.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchInfo {
    pub id: u32,
    pub forward_id: u32,
    pub target: ObjectRef,
    pub pod: String,
    pub url: String,
    pub method: String,
    pub concurrency: u32,
    pub requests: Option<u32>,
    pub duration_secs: Option<u32>,
    /// What was asked for, so the run can be repeated.
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
    pub timeout_secs: u32,
    pub started: String,
    pub state: BenchState,
    /// Requests finished so far, with a response or an error.
    pub completed: u32,
    /// Seconds since the start, or the whole run once finished.
    pub elapsed: f64,
    /// Set once the run has finished.
    pub results: Option<BenchResults>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchResults {
    pub total: u32,
    /// Seconds.
    pub duration: f64,
    pub requests_per_second: f64,
    /// Response times in seconds, over requests that got a response.
    pub fastest: Option<f64>,
    pub slowest: Option<f64>,
    pub average: Option<f64>,
    pub percentiles: Vec<Percentile>,
    pub histogram: Vec<Bucket>,
    pub status_codes: Vec<StatusCount>,
    pub errors: Vec<ErrorCount>,
    /// Bytes of response bodies.
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Percentile {
    pub percent: u8,
    pub seconds: f64,
}

/// Responses that took from `from` up to `to` seconds.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub from: f64,
    pub to: f64,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCount {
    pub code: u16,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorCount {
    pub message: String,
    pub count: u32,
}

struct Shared {
    info: Mutex<BenchInfo>,
    started: Instant,
    completed: AtomicU32,
    stop: watch::Sender<bool>,
}

struct Run {
    id: u32,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

impl Run {
    /// Ends the run for good. The workers are tasks of their own, so aborting the run's
    /// task alone would leave them sending requests.
    fn end(self) {
        self.shared.stop.send_replace(true);
        self.task.abort();
    }
}

#[derive(Default)]
pub(crate) struct Benchmarks {
    next_id: u32,
    runs: Vec<Run>,
}

impl Benchmarks {
    pub(crate) fn list(&self) -> Vec<BenchInfo> {
        self.runs.iter().rev().map(|r| snapshot(&r.shared)).collect()
    }

    /// Stops a running benchmark; its results cover what finished until then.
    pub(crate) fn stop(&self, id: u32) -> Result<()> {
        let run = self.find(id)?;
        run.shared.stop.send_replace(true);
        Ok(())
    }

    /// Forgets a benchmark, stopping it first if it still runs.
    pub(crate) fn remove(&mut self, id: u32) -> Result<()> {
        let i = self
            .runs
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| not_found(id))?;
        self.runs.remove(i).end();
        Ok(())
    }

    pub(crate) fn stop_all(&mut self) {
        for run in self.runs.drain(..) {
            run.end();
        }
    }

    fn find(&self, id: u32) -> Result<&Run> {
        self.runs.iter().find(|r| r.id == id).ok_or_else(|| not_found(id))
    }
}

fn not_found(id: u32) -> Error {
    Error::NotFound(format!("benchmark {id}"))
}

fn snapshot(shared: &Shared) -> BenchInfo {
    let mut info = shared.info.lock().unwrap().clone();
    if info.state == BenchState::Running {
        info.completed = shared.completed.load(Ordering::Relaxed);
        info.elapsed = shared.started.elapsed().as_secs_f64();
    }
    info
}

/// A checked benchmark, ready to run.
struct Plan {
    method: Method,
    uri: Uri,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: Bytes,
    concurrency: u32,
    requests: Option<u32>,
    duration: Option<Duration>,
    timeout: Duration,
}

fn plan(request: &BenchRequest, forward: &PortForwardInfo) -> Result<Plan> {
    let invalid = |message: String| Error::Unsupported(message);
    let method = match request.method.to_ascii_uppercase().as_str() {
        m @ ("GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS") => {
            Method::from_bytes(m.as_bytes()).unwrap()
        }
        other => return Err(invalid(format!("unsupported method {other}"))),
    };
    if !request.path.starts_with('/') {
        return Err(invalid("the path must start with /".into()));
    }
    let host = connect_host(&forward.address);
    let uri: Uri = format!("http://{host}:{}{}", forward.local_port, request.path)
        .parse()
        .map_err(|_| invalid(format!("invalid path {}", request.path)))?;
    let mut headers = Vec::new();
    for (name, value) in &request.headers {
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| invalid(format!("invalid header name {name}")))?;
        let value = HeaderValue::from_str(value.trim())
            .map_err(|_| invalid(format!("invalid value for header {name}")))?;
        headers.push((name, value));
    }
    if !headers.iter().any(|(n, _)| n == http::header::HOST) {
        let value = HeaderValue::from_str(&format!("localhost:{}", forward.local_port)).unwrap();
        headers.push((http::header::HOST, value));
    }
    let body = request.body.clone().unwrap_or_default();
    if body.len() > MAX_BODY {
        return Err(invalid("the body may be at most 1 MiB".into()));
    }
    if !(1..=MAX_CONCURRENCY).contains(&request.concurrency) {
        return Err(invalid(format!(
            "concurrency must be between 1 and {MAX_CONCURRENCY}"
        )));
    }
    match (request.requests, request.duration_secs) {
        (Some(n), None) if (1..=MAX_REQUESTS).contains(&n) => {}
        (None, Some(s)) if (1..=MAX_DURATION_SECS).contains(&s) => {}
        (Some(_), None) => return Err(invalid(format!("requests must be between 1 and {MAX_REQUESTS}"))),
        (None, Some(_)) => {
            return Err(invalid(format!(
                "the duration must be between 1 and {MAX_DURATION_SECS} seconds"
            )));
        }
        _ => return Err(invalid("set either a number of requests or a duration".into())),
    }
    let timeout = request.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    if !(1..=MAX_TIMEOUT_SECS).contains(&timeout) {
        return Err(invalid(format!(
            "the timeout must be between 1 and {MAX_TIMEOUT_SECS} seconds"
        )));
    }
    Ok(Plan {
        method,
        uri,
        headers,
        body: Bytes::from(body),
        concurrency: request.concurrency,
        requests: request.requests,
        duration: request.duration_secs.map(|s| Duration::from_secs(u64::from(s))),
        timeout: Duration::from_secs(u64::from(timeout)),
    })
}

/// Where to connect to reach a forward listening on `address`.
fn connect_host(address: &str) -> String {
    match address {
        "0.0.0.0" | "" => "127.0.0.1".to_owned(),
        "::" => "[::1]".to_owned(),
        a if a.contains(':') => format!("[{a}]"),
        a => a.to_owned(),
    }
}

/// Checks the request and starts the benchmark in the background.
pub(crate) fn start(
    benchmarks: &Mutex<Benchmarks>,
    request: &BenchRequest,
    forward: &PortForwardInfo,
) -> Result<BenchInfo> {
    let plan = plan(request, forward)?;
    let mut guard = benchmarks.lock().unwrap();
    guard.next_id += 1;
    let info = BenchInfo {
        id: guard.next_id,
        forward_id: forward.id,
        target: forward.target.clone(),
        pod: forward.pod.clone(),
        url: plan.uri.to_string(),
        method: plan.method.to_string(),
        concurrency: plan.concurrency,
        requests: plan.requests,
        duration_secs: request.duration_secs,
        path: request.path.clone(),
        headers: request.headers.clone(),
        body: request.body.clone(),
        timeout_secs: plan.timeout.as_secs() as u32,
        started: k8s_openapi::jiff::Timestamp::now().to_string(),
        state: BenchState::Running,
        completed: 0,
        elapsed: 0.0,
        results: None,
    };
    let shared = Arc::new(Shared {
        info: Mutex::new(info.clone()),
        started: Instant::now(),
        completed: AtomicU32::new(0),
        stop: watch::channel(false).0,
    });
    let task = tokio::spawn(run(Arc::clone(&shared), plan));
    guard.runs.push(Run {
        id: info.id,
        shared,
        task,
    });
    // Drop the oldest finished runs beyond the limit.
    while guard.runs.len() > KEPT {
        let Some(i) = guard
            .runs
            .iter()
            .position(|r| r.shared.info.lock().unwrap().state != BenchState::Running)
        else {
            break;
        };
        guard.runs.remove(i);
    }
    tracing::info!(id = info.id, url = %info.url, concurrency = info.concurrency, "benchmark started");
    Ok(info)
}

/// What one worker saw.
#[derive(Default)]
struct Tally {
    latencies: Vec<f64>,
    codes: HashMap<u16, u32>,
    errors: HashMap<String, u32>,
    bytes: u64,
}

impl Tally {
    fn merge(&mut self, other: Tally) {
        self.latencies.extend(other.latencies);
        for (code, n) in other.codes {
            *self.codes.entry(code).or_default() += n;
        }
        for (message, n) in other.errors {
            *self.errors.entry(message).or_default() += n;
        }
        self.bytes += other.bytes;
    }
}

async fn run(shared: Arc<Shared>, plan: Plan) {
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_max_idle_per_host(plan.concurrency as usize)
        .build_http();
    let plan = Arc::new(plan);
    let issued = Arc::new(AtomicU32::new(0));
    let deadline = plan.duration.map(|d| tokio::time::Instant::now() + d);
    let workers: Vec<_> = (0..plan.concurrency)
        .map(|_| {
            let (client, plan, issued, shared) = (
                client.clone(),
                Arc::clone(&plan),
                Arc::clone(&issued),
                Arc::clone(&shared),
            );
            tokio::spawn(async move { worker(client, &plan, &issued, deadline, &shared).await })
        })
        .collect();
    let mut tally = Tally::default();
    for worker in workers {
        if let Ok(t) = worker.await {
            tally.merge(t);
        }
    }
    let duration = shared.started.elapsed().as_secs_f64();
    let stopped = *shared.stop.borrow();
    let mut info = shared.info.lock().unwrap();
    info.completed = shared.completed.load(Ordering::Relaxed);
    info.elapsed = duration;
    info.state = if stopped {
        BenchState::Stopped
    } else {
        BenchState::Done
    };
    info.results = Some(results(tally, duration));
    tracing::info!(id = info.id, completed = info.completed, "benchmark finished");
}

async fn worker(
    client: Client<HttpConnector, Full<Bytes>>,
    plan: &Plan,
    issued: &AtomicU32,
    deadline: Option<tokio::time::Instant>,
    shared: &Shared,
) -> Tally {
    let mut tally = Tally::default();
    let mut stop = shared.stop.subscribe();
    loop {
        if *stop.borrow() || deadline.is_some_and(|d| tokio::time::Instant::now() >= d) {
            break;
        }
        if let Some(n) = plan.requests
            && issued.fetch_add(1, Ordering::Relaxed) >= n
        {
            break;
        }
        let mut request = Request::builder()
            .method(plan.method.clone())
            .uri(plan.uri.clone());
        for (name, value) in &plan.headers {
            request = request.header(name, value);
        }
        let request = request
            .body(Full::new(plan.body.clone()))
            .expect("checked when planned");
        let began = Instant::now();
        let exchange = async {
            let response = client.request(request).await.map_err(describe)?;
            let code = response.status().as_u16();
            // Counted as it arrives, so a large response is not held in memory.
            let mut body = response.into_body();
            let mut bytes = 0;
            while let Some(frame) = body.frame().await {
                if let Some(data) = frame.map_err(|e| e.to_string())?.data_ref() {
                    bytes += data.len();
                }
            }
            Ok::<_, String>((code, bytes))
        };
        let end = async {
            match deadline {
                Some(d) => tokio::time::sleep_until(d).await,
                None => std::future::pending().await,
            }
        };
        // Requests cut off by the deadline or a stop are not counted.
        let outcome = tokio::select! {
            outcome = tokio::time::timeout(plan.timeout, exchange) => outcome,
            _ = stop.wait_for(|s| *s) => break,
            () = end => break,
        };
        match outcome {
            Ok(Ok((code, bytes))) => {
                tally.latencies.push(began.elapsed().as_secs_f64());
                *tally.codes.entry(code).or_default() += 1;
                tally.bytes += bytes as u64;
            }
            Ok(Err(message)) => *tally.errors.entry(message).or_default() += 1,
            Err(_) => {
                *tally
                    .errors
                    .entry(format!("timed out after {}s", plan.timeout.as_secs()))
                    .or_default() += 1
            }
        }
        shared.completed.fetch_add(1, Ordering::Relaxed);
    }
    tally
}

/// The innermost cause, which says what went wrong (refused, reset) instead of "client error".
fn describe(error: hyper_util::client::legacy::Error) -> String {
    let mut source: &dyn std::error::Error = &error;
    while let Some(next) = source.source() {
        source = next;
    }
    source.to_string()
}

fn results(mut tally: Tally, duration: f64) -> BenchResults {
    let latencies = &mut tally.latencies;
    latencies.sort_by(f64::total_cmp);
    let n = latencies.len();
    let total = (n as u32) + tally.errors.values().sum::<u32>();
    let percentiles = if n == 0 {
        Vec::new()
    } else {
        PERCENTILES
            .iter()
            .map(|&p| {
                let rank = (f64::from(p) / 100.0 * n as f64).ceil() as usize;
                Percentile {
                    percent: p,
                    seconds: latencies[rank.clamp(1, n) - 1],
                }
            })
            .collect()
    };
    let mut status_codes: Vec<StatusCount> = tally
        .codes
        .into_iter()
        .map(|(code, count)| StatusCount { code, count })
        .collect();
    status_codes.sort_by_key(|s| s.code);
    let mut errors: Vec<ErrorCount> = tally
        .errors
        .into_iter()
        .map(|(message, count)| ErrorCount { message, count })
        .collect();
    errors.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.message.cmp(&b.message)));
    BenchResults {
        total,
        duration,
        requests_per_second: if duration > 0.0 {
            f64::from(total) / duration
        } else {
            0.0
        },
        fastest: latencies.first().copied(),
        slowest: latencies.last().copied(),
        average: (n > 0).then(|| latencies.iter().sum::<f64>() / n as f64),
        percentiles,
        histogram: histogram(latencies),
        status_codes,
        errors,
        bytes: tally.bytes,
    }
}

/// Equal-width buckets from the fastest to the slowest response, like `hey`.
fn histogram(sorted: &[f64]) -> Vec<Bucket> {
    let (Some(&fastest), Some(&slowest)) = (sorted.first(), sorted.last()) else {
        return Vec::new();
    };
    if slowest <= fastest {
        return vec![Bucket {
            from: fastest,
            to: slowest,
            count: sorted.len() as u32,
        }];
    }
    let width = (slowest - fastest) / BUCKETS as f64;
    let mut buckets: Vec<Bucket> = (0..BUCKETS)
        .map(|i| Bucket {
            from: fastest + width * i as f64,
            to: fastest + width * (i + 1) as f64,
            count: 0,
        })
        .collect();
    for &latency in sorted {
        let i = (((latency - fastest) / width) as usize).min(BUCKETS - 1);
        buckets[i].count += 1;
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn forward(port: u16) -> PortForwardInfo {
        PortForwardInfo {
            id: 7,
            context: "test".into(),
            target: ObjectRef {
                context: None,
                resource: crate::ResourceRef {
                    group: String::new(),
                    version: "v1".into(),
                    plural: "pods".into(),
                },
                namespace: Some("shop".into()),
                name: "web".into(),
            },
            namespace: "shop".into(),
            pod: "web".into(),
            remote_port: 80,
            pod_port: 80,
            address: "127.0.0.1".into(),
            local_port: port,
            started: String::new(),
            connections: 0,
            error: None,
        }
    }

    fn request(extra: serde_json::Value) -> BenchRequest {
        let mut value = serde_json::json!({"forwardId": 7, "concurrency": 2, "requests": 10});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn checks_requests_before_running() {
        let f = forward(8080);
        let ok = plan(
            &request(serde_json::json!({"path": "/healthz?x=1", "headers": {"Host": "shop.acme.io"}})),
            &f,
        )
        .unwrap();
        assert_eq!(ok.uri.to_string(), "http://127.0.0.1:8080/healthz?x=1");
        assert_eq!(ok.headers.len(), 1, "the given Host replaces the default");
        for bad in [
            serde_json::json!({"method": "TRACE"}),
            serde_json::json!({"path": "healthz"}),
            serde_json::json!({"path": "/a b"}),
            serde_json::json!({"concurrency": 0}),
            serde_json::json!({"concurrency": 201}),
            serde_json::json!({"requests": null}),
            serde_json::json!({"durationSecs": 10}),
            serde_json::json!({"headers": {"bad header": "x"}}),
            serde_json::json!({"timeoutSecs": 0}),
        ] {
            assert!(plan(&request(bad.clone()), &f).is_err(), "{bad}");
        }
        assert_eq!(connect_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(connect_host("::"), "[::1]");
    }

    #[test]
    fn summarizes_latencies() {
        let mut tally = Tally {
            latencies: (1..=100).map(|i| f64::from(i) / 1000.0).collect(),
            ..Default::default()
        };
        tally.codes.insert(200, 98);
        tally.codes.insert(503, 2);
        tally.errors.insert("connection refused".into(), 4);
        let r = results(tally, 2.0);
        assert_eq!(r.total, 104);
        assert_eq!(r.requests_per_second, 52.0);
        assert_eq!(r.fastest, Some(0.001));
        assert_eq!(r.slowest, Some(0.1));
        let p = |percent| {
            r.percentiles
                .iter()
                .find(|x| x.percent == percent)
                .unwrap()
                .seconds
        };
        assert_eq!(p(50), 0.05);
        assert_eq!(p(99), 0.099);
        assert_eq!(r.histogram.len(), BUCKETS);
        assert_eq!(r.histogram.iter().map(|b| b.count).sum::<u32>(), 100);
        assert_eq!(
            r.status_codes,
            vec![
                StatusCount { code: 200, count: 98 },
                StatusCount { code: 503, count: 2 }
            ]
        );
        assert_eq!(r.errors[0].count, 4);
        assert!(results(Tally::default(), 1.0).histogram.is_empty());
    }

    /// A tiny keep-alive HTTP server that answers every request with "ok".
    async fn server() -> (u16, Arc<AtomicU32>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&hits);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let counter = Arc::clone(&counter);
                tokio::spawn(async move {
                    let mut buffer = vec![0; 4096];
                    let mut pending = Vec::new();
                    loop {
                        let Ok(n) = socket.read(&mut buffer).await else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        pending.extend_from_slice(&buffer[..n]);
                        while let Some(end) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
                            pending.drain(..end + 4);
                            counter.fetch_add(1, Ordering::Relaxed);
                            let reply = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok";
                            if socket.write_all(reply).await.is_err() {
                                return;
                            }
                        }
                    }
                });
            }
        });
        (port, hits)
    }

    #[tokio::test]
    async fn runs_the_requested_number_of_requests() {
        let (port, hits) = server().await;
        let benchmarks = Mutex::new(Benchmarks::default());
        let info = start(
            &benchmarks,
            &request(serde_json::json!({"requests": 50, "concurrency": 4})),
            &forward(port),
        )
        .unwrap();
        assert_eq!(info.state, BenchState::Running);
        let finished = loop {
            let list = benchmarks.lock().unwrap().list();
            if list[0].state != BenchState::Running {
                break list[0].clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert_eq!(finished.state, BenchState::Done);
        assert_eq!(finished.completed, 50);
        let results = finished.results.unwrap();
        assert_eq!(results.status_codes, vec![StatusCount { code: 200, count: 50 }]);
        assert_eq!(results.bytes, 100);
        assert_eq!(hits.load(Ordering::Relaxed), 50);
    }

    #[tokio::test]
    async fn removing_a_running_benchmark_stops_its_requests() {
        let (port, hits) = server().await;
        let benchmarks = Mutex::new(Benchmarks::default());
        let info = start(
            &benchmarks,
            &request(serde_json::json!({"requests": null, "durationSecs": 60})),
            &forward(port),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        benchmarks.lock().unwrap().remove(info.id).unwrap();
        // Let requests that were already on the wire land.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let after = hits.load(Ordering::Relaxed);
        assert!(after > 0);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(hits.load(Ordering::Relaxed), after, "workers kept sending");
    }

    #[tokio::test]
    async fn stops_on_request_and_counts_refused_connections() {
        // Nothing listens on a port that was just freed.
        let port = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let benchmarks = Mutex::new(Benchmarks::default());
        let info = start(
            &benchmarks,
            &request(serde_json::json!({"requests": null, "durationSecs": 60})),
            &forward(port),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        benchmarks.lock().unwrap().stop(info.id).unwrap();
        let finished = loop {
            let list = benchmarks.lock().unwrap().list();
            if list[0].state != BenchState::Running {
                break list[0].clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert_eq!(finished.state, BenchState::Stopped);
        let results = finished.results.unwrap();
        assert!(results.status_codes.is_empty());
        assert!(
            results.errors[0].message.to_lowercase().contains("refused"),
            "{:?}",
            results.errors
        );
        benchmarks.lock().unwrap().remove(info.id).unwrap();
        assert!(benchmarks.lock().unwrap().list().is_empty());
    }
}
