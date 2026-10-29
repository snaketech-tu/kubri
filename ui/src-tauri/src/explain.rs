//! Explain in the desktop app. The API key lives in the system keychain on macOS and Windows,
//! and elsewhere in a file in Kubri's config folder that only the user can read.

use std::sync::Arc;

use kubri_core::{ExplainAnswer, ExplainPrompt, ExplainSettings, ExplainSubject, Kubri, Settings};
use serde::Serialize;
use tauri::State;

type CommandResult<T> = Result<T, String>;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod key {
    fn entry() -> keyring::Result<keyring::Entry> {
        keyring::Entry::new("Kubri", "explain-api-key")
    }

    pub fn read() -> Option<String> {
        match entry().and_then(|e| e.get_password()) {
            Ok(key) => Some(key),
            Err(keyring::Error::NoEntry) => kubri_core::explain::read_key_file(),
            Err(err) => {
                tracing::warn!("cannot read the Explain key from the keychain: {err}");
                kubri_core::explain::read_key_file()
            }
        }
    }

    pub fn write(key: Option<&str>) -> Result<(), String> {
        let entry = entry().map_err(|e| e.to_string())?;
        match key.map(str::trim).filter(|k| !k.is_empty()) {
            Some(key) => entry.set_password(key).map_err(|e| e.to_string())?,
            None => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(err) => return Err(err.to_string()),
            },
        }
        // A key from a file kept before stays no longer.
        kubri_core::explain::write_key_file(None).map_err(|e| e.to_string())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod key {
    pub fn read() -> Option<String> {
        kubri_core::explain::read_key_file()
    }

    pub fn write(key: Option<&str>) -> Result<(), String> {
        kubri_core::explain::write_key_file(key).map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainKey {
    /// Never the key itself: only whether one is stored.
    has_key: bool,
}

#[tauri::command]
pub fn set_explain_settings(
    kubri: State<'_, Arc<Kubri>>,
    settings: ExplainSettings,
) -> CommandResult<Settings> {
    kubri.set_explain_settings(settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn explain_key() -> CommandResult<ExplainKey> {
    tauri::async_runtime::spawn_blocking(|| ExplainKey {
        has_key: key::read().is_some(),
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_explain_key(key: Option<String>) -> CommandResult<ExplainKey> {
    tauri::async_runtime::spawn_blocking(move || {
        key::write(key.as_deref())?;
        Ok(ExplainKey {
            has_key: key::read().is_some(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn explain_prompt(
    kubri: State<'_, Arc<Kubri>>,
    subject: ExplainSubject,
) -> CommandResult<ExplainPrompt> {
    let mut prompt = kubri.explain_prompt(&subject).await.map_err(|e| e.to_string())?;
    prompt.has_key = explain_key().await?.has_key;
    Ok(prompt)
}

#[tauri::command]
pub async fn explain(kubri: State<'_, Arc<Kubri>>, text: String) -> CommandResult<ExplainAnswer> {
    let key = tauri::async_runtime::spawn_blocking(key::read)
        .await
        .map_err(|e| e.to_string())?;
    kubri
        .explain(&text, key.as_deref())
        .await
        .map_err(|e| e.to_string())
}
