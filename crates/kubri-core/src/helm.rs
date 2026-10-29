//! Helm releases, read from the Secrets (or ConfigMaps) Helm stores them in. Changes go
//! through the `helm` command, so they behave exactly as Helm's own.

use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;

use base64::Engine;
use futures::StreamExt;
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use kube::api::ListParams;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::typed::scoped;
use crate::{Error, Result};

/// How long `helm rollback` and `helm uninstall` may take, hooks included: a little longer
/// than the `--timeout 5m` they get, so helm can say itself what it was waiting for.
const HELM_TIMEOUT: Duration = Duration::from_secs(5 * 60 + 30);
const FETCH_CONCURRENCY: usize = 8;

/// A release revision, as `helm list` and `helm history` show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmRelease {
    pub name: String,
    pub namespace: String,
    pub revision: u32,
    /// `deployed`, `failed`, `superseded`, `pending-upgrade`…
    pub status: String,
    pub chart: String,
    pub chart_version: String,
    pub app_version: String,
    pub updated: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmReleaseDetails {
    #[serde(flatten)]
    pub release: HelmRelease,
    pub notes: String,
    /// The values given at install or upgrade, like `helm get values`.
    pub values: Value,
    /// The chart's defaults merged with the given values, like `helm get values --all`.
    pub all_values: Value,
    /// The rendered manifests, like `helm get manifest`.
    pub manifest: String,
}

/// Where a revision is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stored {
    namespace: String,
    name: String,
    revision: u32,
    status: String,
    object: String,
    driver: Driver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Driver {
    Secret,
    ConfigMap,
}

/// The parts of Helm's release record Kubri shows.
#[derive(Debug, Deserialize)]
struct Record {
    name: String,
    namespace: String,
    version: u32,
    #[serde(default)]
    info: Info,
    #[serde(default)]
    chart: Chart,
    #[serde(default)]
    config: Option<Value>,
    #[serde(default)]
    manifest: String,
}

#[derive(Debug, Default, Deserialize)]
struct Info {
    #[serde(default)]
    last_deployed: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    notes: String,
}

#[derive(Debug, Default, Deserialize)]
struct Chart {
    #[serde(default)]
    metadata: ChartMetadata,
    #[serde(default)]
    values: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChartMetadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    app_version: String,
}

impl Record {
    fn summary(&self) -> HelmRelease {
        HelmRelease {
            name: self.name.clone(),
            namespace: self.namespace.clone(),
            revision: self.version,
            status: self.info.status.clone(),
            chart: self.chart.metadata.name.clone(),
            chart_version: self.chart.metadata.version.clone(),
            app_version: self.chart.metadata.app_version.clone(),
            updated: self.info.last_deployed.clone(),
            description: self.info.description.clone(),
        }
    }

    fn details(self) -> HelmReleaseDetails {
        let release = self.summary();
        let values = self.config.unwrap_or_else(|| Value::Object(Default::default()));
        let defaults = self
            .chart
            .values
            .unwrap_or_else(|| Value::Object(Default::default()));
        HelmReleaseDetails {
            release,
            notes: self.info.notes,
            all_values: coalesce(defaults, &values),
            values,
            manifest: self.manifest,
        }
    }
}

#[cfg(test)]
fn decode(encoded: &[u8]) -> Result<Record> {
    decode_as(encoded)
}

/// Helm's storage format: base64 of gzipped JSON (older releases are not gzipped).
fn decode_as<T: serde::de::DeserializeOwned>(encoded: &[u8]) -> Result<T> {
    let invalid = |e: String| Error::Unsupported(format!("cannot read the Helm release: {e}"));
    let text: Vec<u8> = encoded
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|e| invalid(e.to_string()))?;
    let json = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut out)
            .map_err(|e| invalid(e.to_string()))?;
        out
    } else {
        bytes
    };
    serde_json::from_slice(&json).map_err(|e| invalid(e.to_string()))
}

/// Merges chart defaults with given values the way Helm does: maps merge, anything else is
/// replaced, and `null` removes a default.
fn coalesce(defaults: Value, values: &Value) -> Value {
    match (defaults, values) {
        (Value::Object(mut base), Value::Object(over)) => {
            for (key, value) in over {
                if value.is_null() {
                    base.remove(key);
                    continue;
                }
                let merged = match base.remove(key) {
                    Some(existing) => coalesce(existing, value),
                    None => value.clone(),
                };
                base.insert(key.clone(), merged);
            }
            Value::Object(base)
        }
        (_, value) => value.clone(),
    }
}

fn helm_params() -> ListParams {
    ListParams::default().labels("owner=helm").match_any()
}

/// Every stored revision, from Secrets and ConfigMaps. ConfigMaps are only read when Secrets
/// hold none, as Helm uses one driver per cluster in practice.
async fn stored(client: &Client, namespace: Option<&str>, params: &ListParams) -> Result<Vec<Stored>> {
    let secrets = scoped::<Secret>(client, namespace).list_metadata(params).await?;
    let (driver, items) = if secrets.items.is_empty() {
        let maps = scoped::<ConfigMap>(client, namespace).list_metadata(params).await;
        (
            Driver::ConfigMap,
            maps.map(|l| l.items.into_iter().map(|m| m.metadata).collect())
                .unwrap_or_default(),
        )
    } else {
        (
            Driver::Secret,
            secrets.items.into_iter().map(|m| m.metadata).collect::<Vec<_>>(),
        )
    };
    Ok(items
        .into_iter()
        .filter_map(|meta| {
            let labels = meta.labels.unwrap_or_default();
            Some(Stored {
                namespace: meta.namespace?,
                name: labels.get("name")?.clone(),
                revision: labels.get("version")?.parse().ok()?,
                status: labels.get("status").cloned().unwrap_or_default(),
                object: meta.name?,
                driver,
            })
        })
        .collect())
}

async fn fetch(client: &Client, stored: &Stored) -> Result<Record> {
    fetch_as(client, stored).await
}

async fn fetch_as<T: serde::de::DeserializeOwned>(client: &Client, stored: &Stored) -> Result<T> {
    let missing = || Error::NotFound(format!("revision {} of release {}", stored.revision, stored.name));
    match stored.driver {
        Driver::Secret => {
            let secret = Api::<Secret>::namespaced(client.clone(), &stored.namespace)
                .get(&stored.object)
                .await?;
            let data = secret.data.unwrap_or_default();
            decode_as(&data.get("release").ok_or_else(missing)?.0)
        }
        Driver::ConfigMap => {
            let map = Api::<ConfigMap>::namespaced(client.clone(), &stored.namespace)
                .get(&stored.object)
                .await?;
            let data = map.data.unwrap_or_default();
            decode_as(data.get("release").ok_or_else(missing)?.as_bytes())
        }
    }
}

/// Reads the given revisions. One that cannot be read, because it was deleted since it was
/// listed or is damaged, is left out rather than failing the rest, unless none can be read.
async fn fetch_all(client: &Client, revisions: Vec<Stored>) -> Result<Vec<HelmRelease>> {
    let results: Vec<Result<HelmRelease>> = futures::stream::iter(revisions)
        .map(|s| async move {
            fetch(client, &s).await.map(|r| r.summary()).inspect_err(|err| {
                tracing::warn!(release = s.name, revision = s.revision, %err, "cannot read Helm release")
            })
        })
        .buffer_unordered(FETCH_CONCURRENCY)
        .collect()
        .await;
    let total = results.len();
    let mut first_error = None;
    let mut read = Vec::with_capacity(total);
    for result in results {
        match result {
            Ok(release) => read.push(release),
            Err(err) => {
                first_error.get_or_insert(err);
            }
        }
    }
    match first_error {
        Some(err) if read.is_empty() => Err(err),
        _ => Ok(read),
    }
}

/// The latest revision of every release, like `helm list --all`.
pub(crate) async fn releases(client: Client, namespace: Option<&str>) -> Result<Vec<HelmRelease>> {
    let mut latest: BTreeMap<(String, String), Stored> = BTreeMap::new();
    for s in stored(&client, namespace, &helm_params()).await? {
        let key = (s.namespace.clone(), s.name.clone());
        if latest.get(&key).is_none_or(|l| l.revision < s.revision) {
            latest.insert(key, s);
        }
    }
    let mut releases = fetch_all(&client, latest.into_values().collect()).await?;
    releases.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    Ok(releases)
}

async fn revisions(client: &Client, namespace: &str, name: &str) -> Result<Vec<Stored>> {
    validate_name(name)?;
    let params = ListParams::default()
        .labels(&format!("owner=helm,name={name}"))
        .match_any();
    let revisions = stored(client, Some(namespace), &params).await?;
    if revisions.is_empty() {
        return Err(Error::NotFound(format!("Helm release {name} in {namespace}")));
    }
    Ok(revisions)
}

/// All revisions of a release, newest first, like `helm history`.
pub(crate) async fn history(client: Client, namespace: &str, name: &str) -> Result<Vec<HelmRelease>> {
    let mut history = fetch_all(&client, revisions(&client, namespace, name).await?).await?;
    history.sort_by_key(|r| std::cmp::Reverse(r.revision));
    Ok(history)
}

/// One revision in full; the latest when `revision` is `None`.
pub(crate) async fn release(
    client: Client,
    namespace: &str,
    name: &str,
    revision: Option<u32>,
) -> Result<HelmReleaseDetails> {
    let revisions = revisions(&client, namespace, name).await?;
    let chosen = match revision {
        Some(r) => revisions.iter().find(|s| s.revision == r),
        None => revisions.iter().max_by_key(|s| s.revision),
    }
    .ok_or_else(|| {
        Error::NotFound(format!(
            "revision {} of release {name}",
            revision.unwrap_or_default()
        ))
    })?;
    Ok(fetch(&client, chosen).await?.details())
}

/// Release and namespace names go on the `helm` command line, so nothing that could be read
/// as a flag gets through.
pub(crate) fn validate_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 253
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && name.starts_with(|c: char| c.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(Error::Unsupported(format!("`{name}` is not a valid name")))
    }
}

/// The installed Helm version, if the `helm` command is available.
pub(crate) async fn cli_version() -> Option<String> {
    let output = tokio::process::Command::new("helm")
        .args(["version", "--short"])
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Runs `helm` against the connected context and returns what it printed.
async fn run(args: &[&str], context: &str) -> Result<String> {
    run_with(args, context, None).await
}

/// Like [`run`], writing `stdin` to helm, which reads values given as `--values=-` from it.
async fn run_with(args: &[&str], context: &str, stdin: Option<&str>) -> Result<String> {
    use tokio::io::AsyncWriteExt;
    let mut command = tokio::process::Command::new("helm");
    command
        .args(args)
        // One argument, so a context name starting with a dash cannot pass for a flag.
        .arg(format!("--kube-context={context}"))
        // The user's kubeconfig files and the contexts added in Kubri.
        .env("KUBECONFIG", crate::kubeconfig::env_value())
        .stdin(if stdin.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let not_started = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotFound => Error::Unsupported("the helm command is not installed".into()),
        _ => Error::Io(e),
    };
    let mut child = command.spawn().map_err(not_started)?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let text = text.to_owned();
        // Written beside the wait, so a large input cannot block on a full pipe.
        tokio::spawn(async move {
            let _ = pipe.write_all(text.as_bytes()).await;
        });
    }
    let output = tokio::time::timeout(HELM_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| Error::Unsupported("helm did not finish after its 5-minute timeout".into()))?
        .map_err(Error::Io)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(Error::Unsupported(
            stderr.trim().trim_start_matches("Error: ").to_owned(),
        ))
    }
}

/// A chart to install or upgrade to: a name in a repository given by URL, a chart from a
/// repository added to helm (`repo/chart`), an `oci://` reference or a local path.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmChartRef {
    pub chart: String,
    /// The repository URL, for a chart given by name.
    #[serde(default)]
    pub repo: Option<String>,
    /// The chart version; the latest when empty.
    #[serde(default)]
    pub version: Option<String>,
}

impl HelmChartRef {
    /// The arguments for this chart, after checking nothing in them can pass for a flag.
    fn args(&self) -> Result<Vec<String>> {
        let bad = |what: &str, value: &str| Error::Unsupported(format!("`{value}` is not a valid {what}"));
        let plain = |value: &str| {
            !value.is_empty()
                && !value.starts_with('-')
                && !value.chars().any(|c| c.is_whitespace() || c.is_control())
        };
        if !plain(&self.chart) {
            return Err(bad("chart", &self.chart));
        }
        let mut args = vec![self.chart.clone()];
        if let Some(repo) = self.repo.as_deref().filter(|r| !r.is_empty()) {
            if !(repo.starts_with("https://") || repo.starts_with("http://")) || !plain(repo) {
                return Err(bad("repository URL", repo));
            }
            if self.chart.contains("://") {
                return Err(Error::Unsupported(
                    "a chart given by URL needs no repository".into(),
                ));
            }
            args.push(format!("--repo={repo}"));
        }
        if let Some(version) = self.version.as_deref().filter(|v| !v.is_empty()) {
            if !plain(version) {
                return Err(bad("version", version));
            }
            args.push(format!("--version={version}"));
        }
        Ok(args)
    }
}

/// An install or upgrade, previewed with a dry run first.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmChange {
    pub namespace: String,
    pub name: String,
    /// The chart. An upgrade without one keeps the chart and version the release runs.
    #[serde(default)]
    pub chart: Option<HelmChartRef>,
    /// The values as YAML. They replace the values the release was given before, like
    /// `helm upgrade --reset-values --values`.
    #[serde(default)]
    pub values: String,
    /// Installs a new release instead of upgrading one.
    #[serde(default)]
    pub install: bool,
    /// Creates the namespace for an install when it is missing.
    #[serde(default)]
    pub create_namespace: bool,
    /// Renders against the cluster without changing it, like `helm upgrade --dry-run=server`.
    #[serde(default)]
    pub dry_run: bool,
}

/// What an install or upgrade made, or would make.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmOutcome {
    #[serde(flatten)]
    pub release: HelmRelease,
    pub notes: String,
    /// The rendered manifests, for a diff against the release's current ones.
    pub manifest: String,
    /// How many hooks the chart runs around the change; their manifests are not in `manifest`.
    pub hooks: usize,
}

#[derive(Debug, Deserialize)]
struct Rendered {
    #[serde(flatten)]
    record: Record,
    #[serde(default)]
    hooks: Vec<Value>,
}

/// The chart as Helm stores it in a release, enough to write it back out as a directory.
#[derive(Debug, Deserialize)]
struct StoredChart {
    chart: RawChart,
}

#[derive(Debug, Default, Deserialize)]
struct RawChart {
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    lock: Option<Value>,
    #[serde(default)]
    templates: Vec<ChartFile>,
    #[serde(default)]
    values: Option<Value>,
    /// values.schema.json, base64-encoded.
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    files: Vec<ChartFile>,
}

#[derive(Debug, Deserialize)]
struct ChartFile {
    name: String,
    /// The contents, base64-encoded.
    #[serde(default)]
    data: String,
}

/// A directory removed when dropped.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("kubri-chart-{}-{nanos}", std::process::id()));
        // Fails when the path exists, so nothing else can have put a link there.
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl RawChart {
    /// Subcharts, which Helm does not store with the release.
    fn has_dependencies(&self) -> bool {
        self.metadata
            .get("dependencies")
            .and_then(Value::as_array)
            .is_some_and(|d| !d.is_empty())
    }

    /// Writes the chart out as a directory `helm upgrade` can read. Chart.yaml and the values
    /// are written as JSON, which YAML reads as it is.
    fn write(&self, dir: &std::path::Path) -> Result<()> {
        let invalid = |e: String| Error::Unsupported(format!("cannot read the stored chart: {e}"));
        let b64 = |data: &str| {
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| invalid(e.to_string()))
        };
        let put = |name: &str, bytes: &[u8]| -> Result<()> {
            let safe = !name.is_empty()
                && !name.contains('\\')
                && name
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..");
            if !safe {
                return Err(invalid(format!("the file name `{name}` is not allowed")));
            }
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, bytes)?;
            Ok(())
        };
        put("Chart.yaml", self.metadata.to_string().as_bytes())?;
        if let Some(lock) = &self.lock {
            put("Chart.lock", lock.to_string().as_bytes())?;
        }
        let values = self
            .values
            .clone()
            .unwrap_or_else(|| Value::Object(Default::default()));
        put("values.yaml", values.to_string().as_bytes())?;
        if let Some(schema) = self.schema.as_deref().filter(|s| !s.is_empty()) {
            put("values.schema.json", &b64(schema)?)?;
        }
        for file in self.templates.iter().chain(&self.files) {
            put(&file.name, &b64(&file.data)?)?;
        }
        Ok(())
    }
}

/// Installs or upgrades a release with `helm install` or `helm upgrade`, or with `dry_run`
/// shows what that would render.
pub(crate) async fn change(client: Client, context: &str, change: &HelmChange) -> Result<HelmOutcome> {
    validate_name(&change.namespace)?;
    validate_name(&change.name)?;
    let mut _stored_chart = None;
    let chart_args = match &change.chart {
        Some(chart) => chart.args()?,
        None if change.install => return Err(Error::Unsupported("an install needs a chart".into())),
        None => {
            // The chart the release runs, as Helm stored it with the latest revision.
            let revisions = revisions(&client, &change.namespace, &change.name).await?;
            let latest = revisions
                .iter()
                .max_by_key(|s| s.revision)
                .ok_or_else(|| Error::NotFound(format!("Helm release {}", change.name)))?;
            let chart = fetch_as::<StoredChart>(&client, latest).await?.chart;
            let dir = TempDir::new()?;
            chart.write(&dir.0)?;
            let path = dir.0.to_string_lossy().into_owned();
            if chart.has_dependencies() {
                run(&["dependency", "build", &path], context).await.map_err(|e| {
                    Error::Unsupported(format!(
                        "the chart's subcharts are not stored with the release and could not be fetched again ({e}); give the chart's repository instead"
                    ))
                })?;
            }
            _stored_chart = Some(dir);
            vec![path]
        }
    };
    let mut args: Vec<String> = vec![
        if change.install { "install" } else { "upgrade" }.into(),
        change.name.clone(),
    ];
    args.extend(chart_args);
    args.extend([
        format!("--namespace={}", change.namespace),
        "--values=-".into(),
        "--output=json".into(),
        "--timeout=5m".into(),
    ]);
    if change.install {
        if change.create_namespace {
            args.push("--create-namespace".into());
        }
    } else {
        args.push("--reset-values".into());
    }
    if change.dry_run {
        args.push("--dry-run=server".into());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_with(&args, context, Some(&change.values)).await?;
    // Helm may print warnings before the release.
    let json = output.find('{').map(|at| &output[at..]).unwrap_or(&output);
    let rendered: Rendered = serde_json::from_str(json)
        .map_err(|e| Error::Unsupported(format!("cannot read what helm printed: {e}")))?;
    let release = rendered.record.summary();
    Ok(HelmOutcome {
        release,
        notes: rendered.record.info.notes,
        manifest: rendered.record.manifest,
        hooks: rendered.hooks.len(),
    })
}

/// A chart's default values, like `helm show values`.
pub(crate) async fn show_values(context: &str, chart: &HelmChartRef) -> Result<String> {
    let mut args = vec!["show".to_owned(), "values".to_owned()];
    args.extend(chart.args()?);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&args, context).await
}

/// A chart found on Artifact Hub.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubChart {
    pub name: String,
    pub version: String,
    pub app_version: String,
    pub description: String,
    /// The chart's page on Artifact Hub.
    pub url: String,
    pub repo_name: String,
    pub repo_url: String,
}

#[derive(Debug, Deserialize)]
struct HubEntry {
    #[serde(default)]
    url: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    repository: HubRepository,
}

#[derive(Debug, Default, Deserialize)]
struct HubRepository {
    #[serde(default)]
    url: String,
    #[serde(default)]
    name: String,
}

fn hub_charts(json: &str) -> Result<Vec<HubChart>> {
    let entries: Vec<HubEntry> = serde_json::from_str(json)
        .map_err(|e| Error::Unsupported(format!("cannot read the search results: {e}")))?;
    Ok(entries
        .into_iter()
        .map(|e| HubChart {
            // The page is https://artifacthub.io/packages/helm/<repository>/<chart>.
            name: e
                .url
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned(),
            version: e.version,
            app_version: e.app_version,
            description: e.description,
            url: e.url,
            repo_name: e.repository.name,
            repo_url: e.repository.url,
        })
        .filter(|c| !c.name.is_empty())
        .collect())
}

/// Searches Artifact Hub for charts, like `helm search hub`.
pub(crate) async fn search_hub(context: &str, query: &str) -> Result<Vec<HubChart>> {
    let query = query.trim();
    if query.is_empty() || query.starts_with('-') {
        return Err(Error::Unsupported(format!("`{query}` is not a search")));
    }
    let output = run(
        &[
            "search",
            "hub",
            query,
            "--list-repo-url",
            "--max-col-width=0",
            "--output=json",
        ],
        context,
    )
    .await?;
    hub_charts(&output)
}

pub(crate) async fn rollback(context: &str, namespace: &str, name: &str, revision: u32) -> Result<String> {
    validate_name(namespace)?;
    validate_name(name)?;
    let revision = revision.to_string();
    run(
        &[
            "rollback",
            name,
            &revision,
            "--namespace",
            namespace,
            "--timeout",
            "5m",
        ],
        context,
    )
    .await
}

pub(crate) async fn uninstall(context: &str, namespace: &str, name: &str) -> Result<String> {
    validate_name(namespace)?;
    validate_name(name)?;
    run(
        &["uninstall", name, "--namespace", namespace, "--timeout", "5m"],
        context,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn encode(record: &Value) -> String {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(record.to_string().as_bytes()).unwrap();
        base64::engine::general_purpose::STANDARD.encode(gz.finish().unwrap())
    }

    fn record(revision: u32, status: &str) -> Value {
        json!({
            "name": "shop", "namespace": "apps", "version": revision,
            "info": { "status": status, "description": "Upgrade complete", "notes": "Visit http://shop",
                      "last_deployed": "2026-10-28T07:00:00Z" },
            "chart": { "metadata": { "name": "shop", "version": "1.2.0", "appVersion": "3.4" },
                       "values": { "replicas": 1, "image": { "tag": "3.4", "pullPolicy": "IfNotPresent" }, "debug": true } },
            "config": { "image": { "tag": "3.5" }, "debug": null },
            "manifest": "---\nkind: Deployment\n"
        })
    }

    #[test]
    fn decodes_stored_releases() {
        let details = decode(encode(&record(3, "deployed")).as_bytes())
            .unwrap()
            .details();
        assert_eq!(details.release.revision, 3);
        assert_eq!(details.release.chart, "shop");
        assert_eq!(details.release.app_version, "3.4");
        assert_eq!(details.notes, "Visit http://shop");
        assert_eq!(
            details.values,
            json!({ "image": { "tag": "3.5" }, "debug": null })
        );
        assert_eq!(
            details.all_values,
            json!({ "replicas": 1, "image": { "tag": "3.5", "pullPolicy": "IfNotPresent" } })
        );
        // Releases from old Helm versions are not compressed.
        let plain = base64::engine::general_purpose::STANDARD.encode(record(1, "superseded").to_string());
        assert_eq!(decode(plain.as_bytes()).unwrap().info.status, "superseded");
        assert!(decode(b"not base64!").is_err());
    }

    #[test]
    fn chart_references_cannot_pass_for_flags() {
        let chart = |chart: &str, repo: Option<&str>, version: Option<&str>| HelmChartRef {
            chart: chart.into(),
            repo: repo.map(Into::into),
            version: version.map(Into::into),
        };
        assert_eq!(
            chart("nginx", Some("https://charts.example.com"), Some("1.2.0"))
                .args()
                .unwrap(),
            ["nginx", "--repo=https://charts.example.com", "--version=1.2.0"]
        );
        assert_eq!(
            chart("oci://registry.example.com/charts/nginx", None, Some(""))
                .args()
                .unwrap(),
            ["oci://registry.example.com/charts/nginx"]
        );
        assert!(chart("--set=a", None, None).args().is_err());
        assert!(chart("nginx", Some("file:///etc"), None).args().is_err());
        assert!(chart("nginx", None, Some("-1")).args().is_err());
        assert!(chart("nginx x", None, None).args().is_err());
        assert!(
            chart("oci://r/nginx", Some("https://charts.example.com"), None)
                .args()
                .is_err()
        );
    }

    #[test]
    fn writes_a_stored_chart_back_out() {
        let b64 = |s: &str| base64::engine::general_purpose::STANDARD.encode(s);
        let stored: StoredChart = serde_json::from_value(json!({ "chart": {
            "metadata": { "apiVersion": "v2", "name": "shop", "version": "1.2.0" },
            "values": { "replicas": 1 },
            "schema": b64("{}"),
            "templates": [{ "name": "templates/deploy.yaml", "data": b64("kind: Deployment") }],
            "files": [{ "name": "README.md", "data": b64("# shop") }]
        }}))
        .unwrap();
        assert!(!stored.chart.has_dependencies());
        let dir = TempDir::new().unwrap();
        stored.chart.write(&dir.0).unwrap();
        let read = |name: &str| std::fs::read_to_string(dir.0.join(name)).unwrap();
        assert_eq!(read("templates/deploy.yaml"), "kind: Deployment");
        assert_eq!(read("README.md"), "# shop");
        assert_eq!(read("values.schema.json"), "{}");
        let chart: Value = serde_json::from_str(&read("Chart.yaml")).unwrap();
        assert_eq!(chart["name"], "shop");
        let path = dir.0.clone();
        drop(dir);
        assert!(!path.exists());

        // A record that names a file outside the chart is refused.
        let escaping: StoredChart = serde_json::from_value(json!({ "chart": {
            "metadata": { "name": "x", "dependencies": [{ "name": "common" }] },
            "files": [{ "name": "../../escape", "data": b64("x") }]
        }}))
        .unwrap();
        assert!(escaping.chart.has_dependencies());
        let dir = TempDir::new().unwrap();
        assert!(escaping.chart.write(&dir.0).is_err());
    }

    #[test]
    fn reads_artifact_hub_results() {
        let charts = hub_charts(
            r#"[{"url":"https://artifacthub.io/packages/helm/bitnami/nginx","version":"25.2.13",
                 "app_version":"1.31.8","description":"NGINX Open Source",
                 "repository":{"url":"https://charts.bitnami.com/bitnami","name":"bitnami"}}]"#,
        )
        .unwrap();
        assert_eq!(charts.len(), 1);
        assert_eq!(charts[0].name, "nginx");
        assert_eq!(charts[0].repo_url, "https://charts.bitnami.com/bitnami");
        assert_eq!(charts[0].app_version, "1.31.8");
        assert!(hub_charts("[]").unwrap().is_empty());
    }

    #[test]
    fn rejects_names_that_look_like_flags() {
        assert!(validate_name("shop-db.v2").is_ok());
        assert!(validate_name("--kubeconfig=/etc/x").is_err());
        assert!(validate_name("-n").is_err());
        assert!(validate_name("Shop").is_err());
        assert!(validate_name("").is_err());
    }

    #[tokio::test]
    async fn lists_the_latest_revision_of_each_release() {
        let meta = |name: &str, revision: u32, status: &str| {
            json!({ "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata", "metadata": {
                "name": format!("sh.helm.release.v1.{name}.v{revision}"), "namespace": "apps",
                "labels": { "owner": "helm", "name": name, "version": revision.to_string(), "status": status }
            }})
        };
        let (client, requests) = crate::testing::client(move |req| {
            let path = req.uri.split('?').next().unwrap_or_default();
            if path == "/api/v1/secrets" {
                let list = json!({ "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadataList",
                                   "metadata": {}, "items": [meta("shop", 1, "superseded"), meta("shop", 2, "deployed"),
                                             meta("gone", 1, "uninstalling")] });
                return (200, list.to_string());
            }
            if path == "/api/v1/namespaces/apps/secrets/sh.helm.release.v1.shop.v2" {
                // Secret data is base64 on the wire, on top of Helm's own base64.
                let data = base64::engine::general_purpose::STANDARD.encode(encode(&record(2, "deployed")));
                let secret = json!({ "apiVersion": "v1", "kind": "Secret",
                                     "metadata": { "name": "sh.helm.release.v1.shop.v2", "namespace": "apps" },
                                     "type": "helm.sh/release.v1", "data": { "release": data } });
                return (200, secret.to_string());
            }
            // Uninstalled since it was listed: left out instead of failing the list.
            (404, crate::testing::status(404, "not found"))
        });
        let releases = releases(client, None).await.unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(
            (releases[0].revision, releases[0].status.as_str()),
            (2, "deployed")
        );
        let uris: Vec<String> = requests.lock().unwrap().iter().map(|r| r.uri.clone()).collect();
        assert!(uris[0].contains("labelSelector=owner%3Dhelm"), "{uris:?}");
        // Only the latest revision of each release is read in full.
        assert_eq!(uris.len(), 3);
    }
}
