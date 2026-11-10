//! Whether Kubri may change the connected cluster: read-only modes, locked contexts and AI-agent rules.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

use kube::Api;
use serde::Serialize;

use crate::{Error, Kubri, PRODUCTION_NAMESPACE, Result};

/// How long unlocking a locked context lasts.
pub const UNLOCK_FOR: Duration = Duration::from_secs(15 * 60);

/// Whether Kubri may change the connected cluster, and why not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Guard {
    pub read_only: bool,
    /// Why changes are off, when they are.
    pub reason: Option<String>,
    /// The context is locked and nothing else keeps it read-only, so it can be unlocked.
    pub can_unlock: bool,
    /// Seconds until an unlocked context locks again.
    pub unlocked_for: Option<u64>,
    /// Read-only for a reason no switch in Kubri lifts: a snapshot, `--read-only`, k9s `readOnly`.
    pub fixed: bool,
}
impl Kubri {
    pub(crate) fn unlocks_map(&self) -> std::sync::MutexGuard<'_, HashMap<String, std::time::Instant>> {
        self.unlocks.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Seconds until `context` locks again, if it is unlocked now.
    pub(crate) fn unlocked_for(&self, context: &str) -> Option<u64> {
        let mut unlocks = self.unlocks_map();
        let until = *unlocks.get(context)?;
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            unlocks.remove(context);
            return None;
        }
        Some(left.as_secs().max(1))
    }

    /// Whether Kubri may change the connected cluster now, and why not.
    pub async fn guard(&self) -> Guard {
        let cluster = self.cluster().await;
        if cluster.as_ref().is_some_and(|c| c.snapshot.is_some()) {
            return Guard {
                read_only: true,
                reason: Some("this is a snapshot: it shows what was saved and cannot change anything".into()),
                fixed: true,
                ..Default::default()
            };
        }
        self.guard_for(cluster.map(|c| c.context).as_deref()).await
    }

    pub(crate) async fn guard_for(&self, context: Option<&str>) -> Guard {
        let fixed = if self.read_only.load(Ordering::Relaxed) {
            Some("it was started with --read-only or KUBRI_READ_ONLY")
        } else if self.k9s_files().await.read_only {
            Some("readOnly is set in the k9s configuration")
        } else {
            None
        };
        let settings = context.map(|c| self.settings().context(c)).unwrap_or_default();
        if let Some(reason) = fixed {
            return Guard {
                read_only: true,
                reason: Some(reason.into()),
                fixed: true,
                ..Default::default()
            };
        }
        if settings.read_only {
            return Guard {
                read_only: true,
                reason: Some("this context is set read-only in Kubri".into()),
                ..Default::default()
            };
        }
        if self.agent.load(Ordering::Relaxed) {
            let reason = if settings.production {
                Some("AI agents never change a production context")
            } else if !settings.agent_writes {
                Some("AI agents may not change this context; allow it in Kubri's settings")
            } else if settings.locked {
                Some("this context is locked; AI agents cannot unlock it")
            } else {
                None
            };
            return Guard {
                read_only: reason.is_some(),
                reason: reason.map(Into::into),
                ..Default::default()
            };
        }
        if !settings.locked {
            return Guard::default();
        }
        match context.and_then(|c| self.unlocked_for(c)) {
            Some(left) => Guard {
                can_unlock: true,
                unlocked_for: Some(left),
                ..Default::default()
            },
            None => Guard {
                read_only: true,
                reason: Some("this context is locked; unlock it to make changes".into()),
                can_unlock: true,
                unlocked_for: None,
                fixed: false,
            },
        }
    }

    /// Unlocks the connected context for [`UNLOCK_FOR`] if it is locked.
    pub async fn unlock(&self) -> Result<Guard> {
        let context = self.cluster().await.ok_or(Error::NotConnected)?.context;
        let guard = self.guard_for(Some(&context)).await;
        if !guard.can_unlock {
            return Err(Error::ReadOnly(
                guard
                    .reason
                    .unwrap_or_else(|| "this context is not locked".into()),
            ));
        }
        self.unlocks_map()
            .insert(context.clone(), std::time::Instant::now() + UNLOCK_FOR);
        tracing::info!(context, "unlocked for 15 minutes");
        Ok(self.guard_for(Some(&context)).await)
    }

    /// Locks the connected context again before its unlock runs out.
    pub async fn lock(&self) -> Result<Guard> {
        let context = self.cluster().await.ok_or(Error::NotConnected)?.context;
        self.unlocks_map().remove(&context);
        tracing::info!(context, "locked again");
        Ok(self.guard_for(Some(&context)).await)
    }

    /// Turns changes to clusters off for good, like k9s `--readonly`. The k9s `readOnly`
    /// setting can turn them off per context too; see [`K9sConfig::read_only`].
    pub fn set_read_only(&self) {
        self.read_only.store(true, Ordering::Relaxed);
    }

    /// Fails with [`Error::ReadOnly`] when changes are turned off. Port-forwards, logs and
    /// benchmarks only read, so they stay allowed, as in k9s.
    pub(crate) async fn ensure_writable(&self) -> Result<()> {
        match self.guard().await {
            Guard {
                read_only: true,
                reason,
                ..
            } => Err(Error::ReadOnly(reason.unwrap_or_default())),
            _ => Ok(()),
        }
    }

    /// As [`Self::ensure_writable`], for a change in `namespace`: AI agents also never change a
    /// namespace marked as production.
    pub(crate) async fn ensure_writable_in(&self, namespace: Option<&str>) -> Result<()> {
        self.ensure_writable().await?;
        if self.agent.load(Ordering::Relaxed) && self.production_namespace(namespace).await {
            return Err(Error::ReadOnly(PRODUCTION_NAMESPACE.into()));
        }
        Ok(())
    }

    /// Whether `namespace` of the connected context is marked as production, by its name or a
    /// pattern, or by its labels. Labels are read only where a rule needs them; a namespace
    /// whose labels cannot be read counts as production then.
    pub(crate) async fn production_namespace(&self, namespace: Option<&str>) -> bool {
        let Some(namespace) = namespace else { return false };
        let Ok(session) = self.session().await else {
            return false;
        };
        let settings = self.settings().context(&session.info.context);
        if settings.production_by_name(namespace).is_some() {
            return true;
        }
        if settings.production_labels.is_empty() {
            return false;
        }
        let api: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(session.client.clone());
        let labels = api
            .get(namespace)
            .await
            .ok()
            .map(|ns| ns.metadata.labels.unwrap_or_default());
        settings.production_by_labels(labels.as_ref()).is_some()
    }
}
