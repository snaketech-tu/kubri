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

/// How long `helm rollback` and `helm uninstall` may take, hooks included.
const HELM_TIMEOUT: Duration = Duration::from_secs(5 * 60);
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

/// Helm's storage format: base64 of gzipped JSON (older releases are not gzipped).
fn decode(encoded: &[u8]) -> Result<Record> {
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
    let missing = || Error::NotFound(format!("revision {} of release {}", stored.revision, stored.name));
    match stored.driver {
        Driver::Secret => {
            let secret = Api::<Secret>::namespaced(client.clone(), &stored.namespace)
                .get(&stored.object)
                .await?;
            let data = secret.data.unwrap_or_default();
            decode(&data.get("release").ok_or_else(missing)?.0)
        }
        Driver::ConfigMap => {
            let map = Api::<ConfigMap>::namespaced(client.clone(), &stored.namespace)
                .get(&stored.object)
                .await?;
            let data = map.data.unwrap_or_default();
            decode(data.get("release").ok_or_else(missing)?.as_bytes())
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
fn validate_name(name: &str) -> Result<()> {
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
    let mut command = tokio::process::Command::new("helm");
    command
        .args(args)
        // One argument, so a context name starting with a dash cannot pass for a flag.
        .arg(format!("--kube-context={context}"))
        .kill_on_drop(true);
    let output = tokio::time::timeout(HELM_TIMEOUT, command.output())
        .await
        .map_err(|_| Error::Unsupported("helm did not finish within 5 minutes".into()))?
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::Unsupported("the helm command is not installed".into()),
            _ => Error::Io(e),
        })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(Error::Unsupported(
            stderr.trim().trim_start_matches("Error: ").to_owned(),
        ))
    }
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
