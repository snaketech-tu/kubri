//! Right-sizing: containers that request much more than they use, less than they use, or run
//! close to their limit, with a request or limit to set instead. Use comes from a week of
//! Prometheus history when there is a Prometheus, else from metrics-server's sample of now.

use std::collections::{BTreeMap, HashMap};

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::core::v1::PodTemplateSpec;
use kube::api::{Patch, PatchParams};
use kube::{Client, ResourceExt};
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};

use crate::integrations::Endpoint;
use crate::typed::{list, resource_ref, scoped};
use crate::{Error, Kubri, ObjectRef, Result, parse_quantity};

const MI: f64 = 1024.0 * 1024.0;
/// Headroom on top of what a container used, for a suggested request.
const REQUEST_HEADROOM: f64 = 1.2;
/// Headroom on top of the peak, for a suggested limit.
const LIMIT_HEADROOM: f64 = 1.5;
/// Use at or above this share of the limit is close to it.
const NEAR_LIMIT: f64 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SizeReason {
    /// Close to the limit: throttled CPU, or about to run out of memory.
    NearLimit,
    /// Uses more than it requests: the scheduler places it on too little.
    UnderRequested,
    /// Requests far more than it uses: capacity others cannot have.
    OverRequested,
    /// Requests nothing, so the scheduler does not count it.
    NoRequest,
}

/// One container's CPU or memory, and what to set instead.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeAdvice {
    pub workload: ObjectRef,
    pub kind: String,
    pub container: String,
    /// `cpu` or `memory`.
    pub resource: String,
    pub request: Option<String>,
    pub limit: Option<String>,
    /// The request in cores or bytes.
    pub requested: Option<f64>,
    /// CPU in cores (the 95th percentile with history), memory in bytes (the peak).
    pub used: f64,
    pub suggested_request: Option<String>,
    pub suggested_limit: Option<String>,
    pub reason: SizeReason,
    /// Cores or bytes a smaller request frees; negative when the request grows.
    pub freed: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RightSizing {
    /// Where the use comes from, like `Prometheus at monitoring/prometheus:9090, last 7 days`.
    pub source: String,
    /// Use over time, so 95th percentiles and peaks; else one sample of now.
    pub history: bool,
    pub advice: Vec<SizeAdvice>,
}

/// Use per namespace, pod and container: CPU cores and memory bytes.
type Used = HashMap<(String, String, String), (Option<f64>, Option<f64>)>;

/// Prometheus: the 95th percentile of CPU and the peak of memory over the last week.
async fn from_prometheus(client: &Client, endpoint: &Endpoint, namespace: Option<&str>) -> Result<Used> {
    let scope = namespace.map_or(String::new(), |ns| format!(",namespace=\"{ns}\""));
    let pods = format!("container!=\"\",container!=\"POD\",pod!=\"\"{scope}");
    let cpu = format!(
        "max by (namespace, pod, container) (quantile_over_time(0.95, rate(container_cpu_usage_seconds_total{{{pods}}}[5m])[7d:5m]))"
    );
    let memory = format!(
        "max by (namespace, pod, container) (max_over_time(container_memory_working_set_bytes{{{pods}}}[7d]))"
    );
    let (cpu, memory) = futures::join!(
        instant(client, endpoint, &cpu),
        instant(client, endpoint, &memory)
    );
    let mut used = Used::new();
    for (labels, value) in cpu? {
        used.entry(labels).or_default().0 = Some(value);
    }
    for (labels, value) in memory? {
        used.entry(labels).or_default().1 = Some(value);
    }
    Ok(used)
}

async fn instant(
    client: &Client,
    endpoint: &Endpoint,
    query: &str,
) -> Result<Vec<((String, String, String), f64)>> {
    let path = format!("/api/v1/query?query={}", crate::prometheus::encode(query));
    let answer = endpoint.get_json(client, &path, None).await?;
    crate::prometheus::check(&answer)?;
    Ok(parse_vector(&answer))
}

fn parse_vector(answer: &Value) -> Vec<((String, String, String), f64)> {
    answer
        .pointer("/data/result")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let label = |k: &str| {
                r.pointer(&format!("/metric/{k}"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            };
            let value = r.pointer("/value/1")?.as_str()?.parse::<f64>().ok()?;
            Some(((label("namespace")?, label("pod")?, label("container")?), value))
        })
        .filter(|(_, v)| v.is_finite())
        .collect()
}

/// metrics-server: what each container uses now.
async fn from_metrics_server(client: &Client, namespace: Option<&str>) -> Result<Used> {
    let path = match namespace {
        Some(ns) => format!("/apis/metrics.k8s.io/v1beta1/namespaces/{ns}/pods"),
        None => "/apis/metrics.k8s.io/v1beta1/pods".to_owned(),
    };
    let request = http::Request::get(path)
        .body(Vec::new())
        .map_err(|e| Error::Unsupported(e.to_string()))?;
    let list: Value = client.request(request).await.map_err(|_| {
        Error::Unsupported("neither Prometheus nor metrics-server is available to measure use".into())
    })?;
    Ok(parse_pod_metrics(&list))
}

fn parse_pod_metrics(list: &Value) -> Used {
    let mut used = Used::new();
    for pod in list.get("items").and_then(Value::as_array).into_iter().flatten() {
        let meta = |k: &str| {
            pod.pointer(&format!("/metadata/{k}"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        for c in pod
            .get("containers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = c
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let usage = |k: &str| {
                c.pointer(&format!("/usage/{k}"))
                    .and_then(Value::as_str)
                    .and_then(parse_quantity)
            };
            used.insert(
                (meta("namespace"), meta("name"), name),
                (usage("cpu"), usage("memory")),
            );
        }
    }
    used
}

/// Millicores, rounded up to 5m.
fn cpu_text(cores: f64) -> String {
    let milli = ((cores * 1000.0 / 5.0).ceil() * 5.0).max(5.0);
    format!("{milli:.0}m")
}

/// Mebibytes, rounded up to 16Mi.
fn memory_text(bytes: f64) -> String {
    let mi = ((bytes / MI / 16.0).ceil() * 16.0).max(16.0);
    format!("{mi:.0}Mi")
}

/// What one container's use says about one resource of it, if anything. `history` says the use
/// is a week's percentile or peak; a single sample needs a bigger gap to call a request too big.
pub(crate) fn advise(
    resource: &str,
    request: Option<&str>,
    limit: Option<&str>,
    used: f64,
    history: bool,
) -> Option<(SizeReason, Option<String>, Option<String>, f64)> {
    let (text, floor): (fn(f64) -> String, f64) = match resource {
        "cpu" => (cpu_text, 0.05),
        _ => (memory_text, 64.0 * MI),
    };
    let requested = request.and_then(parse_quantity);
    let limited = limit.and_then(parse_quantity);
    let wanted = used * REQUEST_HEADROOM;
    let suggested = parse_quantity(&text(wanted)).unwrap_or(wanted);
    if let Some(l) = limited
        && used >= l * NEAR_LIMIT
    {
        let new_limit = text(used * LIMIT_HEADROOM);
        // A request above the new limit would be refused.
        let request = requested
            .filter(|r| *r > used * LIMIT_HEADROOM)
            .map(|_| text(wanted));
        return Some((SizeReason::NearLimit, request, Some(new_limit), 0.0));
    }
    let Some(requested) = requested else {
        return (used > 0.0).then(|| (SizeReason::NoRequest, Some(text(wanted)), None, -suggested));
    };
    if used > requested {
        return Some((
            SizeReason::UnderRequested,
            Some(text(wanted)),
            None,
            requested - suggested,
        ));
    }
    let factor = if history { 2.0 } else { 4.0 };
    if requested >= suggested * factor && requested - suggested >= floor {
        return Some((
            SizeReason::OverRequested,
            Some(text(wanted)),
            None,
            requested - suggested,
        ));
    }
    None
}

/// The strategic merge patch that sets one container's requests and limits in a template.
pub(crate) fn resources_patch(
    container: &str,
    requests: &BTreeMap<String, String>,
    limits: &BTreeMap<String, String>,
) -> Result<Value> {
    for (what, amount) in requests.iter().chain(limits) {
        if !parse_quantity(amount).is_some_and(|q| q > 0.0) {
            return Err(Error::Unsupported(format!(
                "`{amount}` is not an amount of {what}"
            )));
        }
    }
    let mut resources = serde_json::Map::new();
    if !requests.is_empty() {
        resources.insert("requests".into(), json!(requests));
    }
    if !limits.is_empty() {
        resources.insert("limits".into(), json!(limits));
    }
    Ok(json!({ "spec": { "template": { "spec": { "containers": [
        { "name": container, "resources": resources }
    ] } } } }))
}

/// A workload and the pods it ran, by name.
struct Workload {
    target: ObjectRef,
    kind: String,
    template: Option<PodTemplateSpec>,
    pods: Regex,
}

async fn workloads(client: &Client, namespace: Option<&str>, context: &str) -> Result<Vec<Workload>> {
    let (deployments, stateful_sets, daemon_sets) = futures::join!(
        list(scoped::<Deployment>(client, namespace)),
        list(scoped::<StatefulSet>(client, namespace)),
        list(scoped::<DaemonSet>(client, namespace)),
    );
    let mut out = Vec::new();
    let mut add = |kind: &str, resource, meta: (String, String), template: Option<PodTemplateSpec>| {
        let (ns, name) = meta;
        let pattern = format!("^{}$", crate::prometheus::pod_pattern(kind, &name));
        if let Ok(pods) = Regex::new(&pattern) {
            out.push(Workload {
                target: ObjectRef {
                    context: Some(context.to_owned()),
                    resource,
                    namespace: Some(ns),
                    name,
                },
                kind: kind.to_owned(),
                template,
                pods,
            });
        }
    };
    for d in deployments? {
        let meta = (d.namespace().unwrap_or_default(), d.name_any());
        add(
            "Deployment",
            resource_ref::<Deployment>(),
            meta,
            d.spec.map(|s| s.template),
        );
    }
    for s in stateful_sets? {
        let meta = (s.namespace().unwrap_or_default(), s.name_any());
        add(
            "StatefulSet",
            resource_ref::<StatefulSet>(),
            meta,
            s.spec.map(|s| s.template),
        );
    }
    for d in daemon_sets? {
        let meta = (d.namespace().unwrap_or_default(), d.name_any());
        add(
            "DaemonSet",
            resource_ref::<DaemonSet>(),
            meta,
            d.spec.map(|s| s.template),
        );
    }
    Ok(out)
}

/// The advice for every container of `workloads`, from what their pods used.
fn advice(workloads: &[Workload], used: &Used, history: bool) -> Vec<SizeAdvice> {
    let mut out = Vec::new();
    for w in workloads {
        let ns = w.target.namespace.as_deref().unwrap_or_default();
        let containers = w
            .template
            .as_ref()
            .and_then(|t| t.spec.as_ref())
            .map(|s| s.containers.as_slice())
            .unwrap_or_default();
        for c in containers {
            // The most any of its pods used, now or before.
            let mut peak: (Option<f64>, Option<f64>) = (None, None);
            for ((pod_ns, pod, container), (cpu, memory)) in used {
                if pod_ns == ns && container == &c.name && w.pods.is_match(pod) {
                    peak.0 = max(peak.0, *cpu);
                    peak.1 = max(peak.1, *memory);
                }
            }
            let amounts =
                |of: Option<&BTreeMap<String, k8s_openapi::apimachinery::pkg::api::resource::Quantity>>,
                 what: &str| { of.and_then(|m| m.get(what)).map(|q| q.0.clone()) };
            let resources = c.resources.as_ref();
            for (what, used) in [("cpu", peak.0), ("memory", peak.1)] {
                let Some(used) = used else { continue };
                let request = amounts(resources.and_then(|r| r.requests.as_ref()), what);
                let limit = amounts(resources.and_then(|r| r.limits.as_ref()), what);
                if let Some((reason, suggested_request, suggested_limit, freed)) =
                    advise(what, request.as_deref(), limit.as_deref(), used, history)
                {
                    out.push(SizeAdvice {
                        workload: w.target.clone(),
                        kind: w.kind.clone(),
                        container: c.name.clone(),
                        resource: what.to_owned(),
                        requested: request.as_deref().and_then(parse_quantity),
                        request,
                        limit,
                        used,
                        suggested_request,
                        suggested_limit,
                        reason,
                        freed,
                    });
                }
            }
        }
    }
    // Risks first, then what frees the most.
    out.sort_by(|a, b| {
        a.reason
            .cmp(&b.reason)
            .then_with(|| a.resource.cmp(&b.resource))
            .then_with(|| b.freed.total_cmp(&a.freed))
    });
    out
}

fn max(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

impl Kubri {
    /// Right-sizing advice for the workloads of `namespace`, or of the cluster.
    pub async fn right_sizing(&self, namespace: Option<&str>) -> Result<RightSizing> {
        let session = self.session().await?;
        let workloads = workloads(&session.client, namespace, &session.info.context).await?;
        let prometheus = self
            .integrations(false)
            .await
            .ok()
            .and_then(|i| i.endpoint("prometheus").cloned());
        let (used, source, history) = match prometheus {
            Some(endpoint) => {
                let used = from_prometheus(&session.proxy_client, &endpoint, namespace).await?;
                (
                    used,
                    format!("Prometheus at {}, last 7 days", endpoint.label()),
                    true,
                )
            }
            None => (
                from_metrics_server(&session.client, namespace).await?,
                "metrics-server, now".to_owned(),
                false,
            ),
        };
        Ok(RightSizing {
            source,
            history,
            advice: advice(&workloads, &used, history),
        })
    }

    /// Sets one container's requests and limits in a workload's pod template.
    pub async fn set_resources(
        &self,
        target: &ObjectRef,
        container: &str,
        requests: &BTreeMap<String, String>,
        limits: &BTreeMap<String, String>,
    ) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        session.ensure_may("patch", target, "").await?;
        let patch = resources_patch(container, requests, limits)?;
        session
            .api(&target.resource, target.namespace.as_deref())?
            .patch(&target.name, &PatchParams::default(), &Patch::Strategic(patch))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advises() {
        // 1 core requested, 50m used at the 95th percentile.
        let (reason, request, limit, freed) = advise("cpu", Some("1"), None, 0.05, true).unwrap();
        assert_eq!(
            (reason, request.as_deref(), limit),
            (SizeReason::OverRequested, Some("60m"), None)
        );
        assert!((freed - 0.94).abs() < 1e-9);
        // The same from a single sample is only reported from four times over.
        assert!(advise("cpu", Some("200m"), None, 0.07, false).is_none());
        assert!(advise("cpu", Some("200m"), None, 0.07, true).is_some());
        // Close to a memory limit: raise it.
        let (reason, _, limit, _) = advise("memory", Some("64Mi"), Some("128Mi"), 120.0 * MI, true).unwrap();
        assert_eq!((reason, limit.as_deref()), (SizeReason::NearLimit, Some("192Mi")));
        // Using more than requested.
        let (reason, request, ..) = advise("memory", Some("64Mi"), None, 100.0 * MI, true).unwrap();
        assert_eq!(
            (reason, request.as_deref()),
            (SizeReason::UnderRequested, Some("128Mi"))
        );
        assert_eq!(
            advise("cpu", None, None, 0.2, true).unwrap().0,
            SizeReason::NoRequest
        );
        // Fine as it is.
        assert!(advise("memory", Some("128Mi"), Some("256Mi"), 90.0 * MI, true).is_none());
    }

    #[test]
    fn reads_use() {
        let vector = json!({ "status": "success", "data": { "resultType": "vector", "result": [
            { "metric": { "namespace": "shop", "pod": "web-1", "container": "nginx" }, "value": [1, "0.25"] },
            { "metric": { "namespace": "shop", "pod": "web-2" }, "value": [1, "1"] }
        ] } });
        assert_eq!(
            parse_vector(&vector),
            [(("shop".into(), "web-1".into(), "nginx".into()), 0.25)]
        );
        let metrics = json!({ "items": [{ "metadata": { "namespace": "shop", "name": "web-1" },
            "containers": [{ "name": "nginx", "usage": { "cpu": "250m", "memory": "64Mi" } }] }] });
        assert_eq!(
            parse_pod_metrics(&metrics)[&("shop".into(), "web-1".into(), "nginx".into())],
            (Some(0.25), Some(64.0 * MI))
        );
    }

    #[test]
    fn patches_one_container() {
        let requests = BTreeMap::from([("cpu".to_owned(), "60m".to_owned())]);
        assert_eq!(
            resources_patch("web", &requests, &BTreeMap::new()).unwrap(),
            json!({ "spec": { "template": { "spec": { "containers": [
                { "name": "web", "resources": { "requests": { "cpu": "60m" } } }
            ] } } } })
        );
        let bad = BTreeMap::from([("memory".to_owned(), "lots".to_owned())]);
        assert!(resources_patch("web", &BTreeMap::new(), &bad).is_err());
    }
}
