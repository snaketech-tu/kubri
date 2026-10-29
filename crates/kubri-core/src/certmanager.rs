//! cert-manager Certificates: when they expire, whether they renew, and renewing one now, as
//! `cmctl renew` does.

use k8s_openapi::jiff::{SignedDuration, Timestamp};
use kube::api::{ApiResource, DynamicObject, ListParams, Patch, PatchParams};
use kube::{Api, Client};
use serde_json::{Value, json};

use crate::health::{Health, Status};
use crate::{Kubri, ObjectRef, ResourceRef, Result};

pub(crate) fn resource() -> ResourceRef {
    ResourceRef {
        group: "cert-manager.io".into(),
        version: "v1".into(),
        plural: "certificates".into(),
    }
}

fn api_resource() -> ApiResource {
    ApiResource {
        group: "cert-manager.io".into(),
        version: "v1".into(),
        api_version: "cert-manager.io/v1".into(),
        kind: "Certificate".into(),
        plural: "certificates".into(),
    }
}

fn api(client: &Client, namespace: Option<&str>) -> Api<DynamicObject> {
    match namespace {
        Some(ns) => Api::namespaced_with(client.clone(), ns, &api_resource()),
        None => Api::all_with(client.clone(), &api_resource()),
    }
}

/// The Certificates of `namespace` or the cluster; `None` when cert-manager is not installed.
pub(crate) async fn list(client: &Client, namespace: Option<&str>) -> Option<Result<Vec<Value>, String>> {
    match api(client, namespace)
        .list(&ListParams::default().match_any())
        .await
    {
        Ok(list) => Some(
            list.items
                .into_iter()
                .map(|c| serde_json::to_value(c).map_err(|e| e.to_string()))
                .collect(),
        ),
        Err(kube::Error::Api(e)) if e.code == 404 => None,
        Err(e) => Some(Err(crate::Error::from(e).to_string())),
    }
}

fn time(cert: &Value, pointer: &str) -> Option<Timestamp> {
    cert.pointer(pointer)?.as_str()?.parse().ok()
}

/// Days, or hours below two days, for messages.
fn span(d: SignedDuration) -> String {
    let hours = d.as_hours().abs();
    match hours {
        0..48 => format!("{hours} {}", if hours == 1 { "hour" } else { "hours" }),
        _ => format!("{} days", hours / 24),
    }
}

/// Whether a Certificate is ready and far from expiry. Expired or not ready is failed; close to
/// expiry, past the point cert-manager should have renewed it, is a warning, and a failure in
/// the last sixth of its lifetime or week.
pub(crate) fn status(cert: &Value, now: Timestamp) -> Status {
    let ready = cert
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|c| c.get("type").and_then(Value::as_str) == Some("Ready"));
    let field = |c: &Value, k: &str| c.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
    let not_after = time(cert, "/status/notAfter");
    if let Some(end) = not_after
        && end <= now
    {
        return Status::new(
            Health::Failed,
            format!("Expired {} ago", span(now.duration_since(end))),
        );
    }
    match ready {
        Some(c) if field(c, "status") == "False" => {
            let message = field(c, "message");
            let reason = field(c, "reason");
            return Status::new(
                Health::Failed,
                match (reason.is_empty(), message.is_empty()) {
                    (_, false) => format!("Not ready: {message}"),
                    (false, true) => format!("Not ready: {reason}"),
                    (true, true) => "Not ready".to_owned(),
                },
            );
        }
        None => return Status::new(Health::Warning, "Not issued yet"),
        _ => {}
    }
    let Some(end) = not_after else {
        return Status::new(Health::Ok, "Ready");
    };
    let left = end.duration_since(now);
    let lifetime = time(cert, "/status/notBefore").map(|start| end.duration_since(start));
    let part = |divisor: i32, cap: i64| {
        let cap = SignedDuration::from_hours(cap * 24);
        lifetime.map_or(cap, |l| (l / divisor).min(cap))
    };
    if left < part(6, 7) {
        Status::new(Health::Failed, format!("Expires in {}", span(left)))
    } else if left < part(3, 21) {
        Status::new(Health::Warning, format!("Expires in {}", span(left)))
    } else {
        Status::new(Health::Ok, format!("Ready, expires in {}", span(left)))
    }
}

/// The status patch that asks cert-manager to issue again: an `Issuing` condition, the way
/// `cmctl renew` adds it.
pub(crate) fn renew_patch(cert: &Value, now: &str) -> Value {
    let mut conditions: Vec<Value> = cert
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.get("type").and_then(Value::as_str) != Some("Issuing"))
        .collect();
    conditions.push(json!({
        "type": "Issuing",
        "status": "True",
        "reason": "ManuallyTriggered",
        "message": "Certificate re-issuance manually triggered",
        "lastTransitionTime": now,
        "observedGeneration": cert.pointer("/metadata/generation").cloned().unwrap_or(Value::Null),
    }));
    json!({ "status": { "conditions": conditions } })
}

impl Kubri {
    /// Asks cert-manager to issue the Certificate `target` again now.
    pub async fn renew_certificate(&self, target: &ObjectRef) -> Result<()> {
        self.ensure_writable().await?;
        let session = self.object_session(target).await?;
        session.ensure_may("patch", target, "status").await?;
        let api = session.api(&target.resource, target.namespace.as_deref())?;
        let cert = serde_json::to_value(api.get(&target.name).await?)?;
        let issuing = cert
            .pointer("/status/conditions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|c| {
                c.get("type").and_then(Value::as_str) == Some("Issuing")
                    && c.get("status").and_then(Value::as_str) == Some("True")
            });
        if issuing {
            return Err(crate::Error::Unsupported(format!(
                "cert-manager is already issuing {}",
                target.name
            )));
        }
        let now = Timestamp::from_second(Timestamp::now().as_second())
            .unwrap_or_else(|_| Timestamp::now())
            .to_string();
        api.patch_status(
            &target.name,
            &PatchParams::default(),
            &Patch::Merge(renew_patch(&cert, &now)),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cert(ready: &str, not_before: &str, not_after: &str) -> Value {
        json!({ "status": {
            "notBefore": not_before, "notAfter": not_after,
            "conditions": [{ "type": "Ready", "status": ready, "reason": "Ready", "message": "" }]
        } })
    }

    #[test]
    fn judges_expiry() {
        let now: Timestamp = "2026-10-29T12:00:00Z".parse().unwrap();
        let check = |c: &Value| {
            let s = status(c, now);
            (s.health, s.detail)
        };
        // 90 days, 60 left: fine.
        assert_eq!(
            check(&cert("True", "2026-10-01T12:00:00Z", "2026-12-28T12:00:00Z")),
            (Health::Ok, "Ready, expires in 60 days".into())
        );
        // 90 days, 20 left: cert-manager should have renewed at 30.
        assert_eq!(
            check(&cert("True", "2026-08-20T12:00:00Z", "2026-11-18T12:00:00Z")).0,
            Health::Warning
        );
        // 90 days, 5 left.
        assert_eq!(
            check(&cert("True", "2026-08-05T12:00:00Z", "2026-11-03T12:00:00Z")),
            (Health::Failed, "Expires in 5 days".into())
        );
        // 24 hours, 20 left: short-lived by design.
        assert_eq!(
            check(&cert("True", "2026-10-29T08:00:00Z", "2026-10-30T08:00:00Z")).0,
            Health::Ok
        );
        assert_eq!(
            check(&cert("True", "2026-09-01T12:00:00Z", "2026-10-29T10:00:00Z")),
            (Health::Failed, "Expired 2 hours ago".into())
        );
        let failing = json!({ "status": { "conditions": [
            { "type": "Ready", "status": "False", "reason": "DoesNotExist", "message": "Issuer letsencrypt not found" }
        ] } });
        assert_eq!(
            check(&failing),
            (Health::Failed, "Not ready: Issuer letsencrypt not found".into())
        );
        assert_eq!(check(&json!({})).0, Health::Warning);
    }

    #[test]
    fn renewing_adds_issuing() {
        let c = json!({ "metadata": { "generation": 2 }, "status": { "conditions": [
            { "type": "Ready", "status": "True" },
            { "type": "Issuing", "status": "False" }
        ] } });
        let patch = renew_patch(&c, "2026-10-29T12:00:00Z");
        let conditions = patch["status"]["conditions"].as_array().unwrap();
        assert_eq!(conditions.len(), 2);
        assert_eq!(conditions[1]["type"], "Issuing");
        assert_eq!(conditions[1]["status"], "True");
        assert_eq!(conditions[1]["observedGeneration"], 2);
    }
}
