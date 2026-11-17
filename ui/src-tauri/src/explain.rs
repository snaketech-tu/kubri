//! Explain in the desktop app. Each provider's API key lives in the system keychain on macOS and
//! Windows, and elsewhere in a file in Kubri's config folder that only the user can read.

use std::sync::Arc;

use kubri_core::{ExplainAnswer, ExplainPrompt, ExplainSubject, Kubri, Provider};
use serde::Serialize;
use tauri::State;

type CommandResult<T> = Result<T, String>;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod key {
    use kubri_core::Provider;

    /// Each provider's key has its own entry, so a key only goes to the provider it was saved
    /// for.
    fn entry(provider: Provider) -> Result<keyring::Entry, String> {
        let name = provider
            .key_name()
            .ok_or_else(|| "this provider takes no API key".to_owned())?;
        keyring::Entry::new("Kubri", &format!("explain-api-key-{name}")).map_err(|e| e.to_string())
    }

    pub fn read(provider: Provider) -> Option<String> {
        match entry(provider).ok()?.get_password() {
            Ok(key) => Some(key),
            Err(keyring::Error::NoEntry) => kubri_core::explain::read_key_file(provider),
            Err(err) => {
                tracing::warn!("cannot read the Explain key from the keychain: {err}");
                kubri_core::explain::read_key_file(provider)
            }
        }
    }

    pub fn write(provider: Provider, key: Option<&str>) -> Result<(), String> {
        let entry = entry(provider)?;
        match key.map(str::trim).filter(|k| !k.is_empty()) {
            Some(key) => entry.set_password(key).map_err(|e| e.to_string())?,
            None => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(err) => return Err(err.to_string()),
            },
        }
        // A key from a file kept before stays no longer.
        kubri_core::explain::write_key_file(provider, None).map_err(|e| e.to_string())
    }

    /// Gives the one key of older versions to `owner`, the provider in the settings, which
    /// is the one it was saved for.
    pub fn adopt_old(owner: Provider) {
        kubri_core::explain::adopt_old_key_file(owner);
        if owner.key_name().is_none() {
            return;
        }
        let Ok(old) = keyring::Entry::new("Kubri", "explain-api-key") else {
            return;
        };
        let Ok(key) = old.get_password() else {
            return;
        };
        if read(owner).is_none() && write(owner, Some(&key)).is_err() {
            return;
        }
        let _ = old.delete_credential();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod key {
    use kubri_core::Provider;

    pub fn read(provider: Provider) -> Option<String> {
        kubri_core::explain::read_key_file(provider)
    }

    pub fn write(provider: Provider, key: Option<&str>) -> Result<(), String> {
        kubri_core::explain::write_key_file(provider, key).map_err(|e| e.to_string())
    }

    pub fn adopt_old(owner: Provider) {
        kubri_core::explain::adopt_old_key_file(owner);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainKey {
    /// Never the key itself: only whether one is stored.
    has_key: bool,
}

/// The key of `provider`, after the key of an older Kubri found its provider.
async fn read_key(kubri: &Kubri, provider: Provider) -> CommandResult<Option<String>> {
    let owner = kubri.settings().explain.provider;
    tauri::async_runtime::spawn_blocking(move || {
        key::adopt_old(owner);
        key::read(provider)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn explain_key(kubri: State<'_, Arc<Kubri>>, provider: Provider) -> CommandResult<ExplainKey> {
    Ok(ExplainKey {
        has_key: read_key(&kubri, provider).await?.is_some(),
    })
}

#[tauri::command]
pub async fn set_explain_key(
    kubri: State<'_, Arc<Kubri>>,
    provider: Provider,
    key: Option<String>,
) -> CommandResult<ExplainKey> {
    // Adopted first, so an old key cannot later overwrite the one saved now.
    read_key(&kubri, provider).await?;
    tauri::async_runtime::spawn_blocking(move || key::write(provider, key.as_deref()))
        .await
        .map_err(|e| e.to_string())??;
    Ok(ExplainKey {
        has_key: read_key(&kubri, provider).await?.is_some(),
    })
}

#[tauri::command]
pub async fn explain_prompt(
    kubri: State<'_, Arc<Kubri>>,
    subject: ExplainSubject,
) -> CommandResult<ExplainPrompt> {
    let mut prompt = kubri.explain_prompt(&subject).await.map_err(|e| e.to_string())?;
    prompt.has_key = read_key(&kubri, prompt.provider).await?.is_some();
    Ok(prompt)
}

#[tauri::command]
pub async fn explain(kubri: State<'_, Arc<Kubri>>, text: String) -> CommandResult<ExplainAnswer> {
    let key = read_key(&kubri, kubri.settings().explain.provider).await?;
    kubri
        .explain(&text, key.as_deref())
        .await
        .map_err(|e| e.to_string())
}
