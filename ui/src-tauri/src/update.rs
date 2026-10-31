//! Updates the desktop app in place with the signed packages the Release workflow publishes.
//!
//! Whether a newer release is out comes from kubri-core, as for the notice in the browser. Only
//! when it is newer, and this copy of Kubri can replace itself, the updater asks the release's
//! `latest.json` for the package matching this install (AppImage, .deb, .rpm, .msi, setup.exe or
//! the macOS app) and its signature.

use std::sync::{Arc, Mutex};

use kubri_core::LatestRelease;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::CommandResult;

/// The update found by [`update_check`], for [`update_install`].
#[derive(Default)]
pub struct Pending(Mutex<Option<Arc<Update>>>);

/// The latest release, and whether Kubri can install it itself.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    #[serde(flatten)]
    release: LatestRelease,
    /// Kubri downloads, checks and installs the release, then starts again.
    installable: bool,
    /// Why it cannot, when the release is newer: the download page is the way then.
    manual: Option<String>,
}

/// Progress of [`update_install`], for the UI.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UpdateProgress {
    /// Bytes so far, and of how many when the server says.
    Download { done: u64, total: Option<u64> },
    /// Downloaded and the signature checked; the package is being installed.
    Install,
}

/// The public key updates are signed with, set by the Release workflow. Builds without it, like
/// local ones, cannot check a download, so they only point at the download page.
fn pubkey<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let key = app
        .config()
        .plugins
        .0
        .get("updater")?
        .get("pubkey")?
        .as_str()?
        .trim();
    (!key.is_empty()).then(|| key.to_owned())
}

/// Why this install cannot replace itself, if it cannot.
fn not_installable<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    if cfg!(debug_assertions) {
        return Some("This is a development build.".into());
    }
    if pubkey(app).is_none() {
        return Some("This build cannot check the signature of updates.".into());
    }
    #[cfg(target_os = "linux")]
    if let Some(why) = linux_package_manager() {
        return Some(why);
    }
    None
}

/// Kubri installed in a way the updater must not touch: by Flatpak, by a package made from the
/// .deb (like the AUR's), or from a build without a package format.
#[cfg(target_os = "linux")]
fn linux_package_manager() -> Option<String> {
    use tauri::utils::config::BundleType;
    use tauri::utils::platform::bundle_type;

    const ELSEWHERE: &str = "Kubri was installed with a package manager; update it there.";
    if std::env::var_os("FLATPAK_ID").is_some() {
        return Some("Kubri runs as a Flatpak; Flatpak updates it.".into());
    }
    match bundle_type() {
        Some(BundleType::AppImage) => None,
        // The updater installs the new package with dpkg or rpm, which must own this one.
        Some(BundleType::Deb) => {
            (!std::path::Path::new("/var/lib/dpkg/info/kubri.list").exists()).then(|| ELSEWHERE.into())
        }
        Some(BundleType::Rpm) => {
            let owned = std::process::Command::new("rpm")
                .args(["-q", "kubri"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            (!owned).then(|| ELSEWHERE.into())
        }
        _ => Some("Kubri was not installed from one of its packages.".into()),
    }
}

/// The latest release, and whether this Kubri can install it. Asks GitHub at most every six
/// hours, like [`kubri_core::latest_release`], and the release's `latest.json` only when there is
/// something newer to install.
#[tauri::command]
pub async fn update_check(app: AppHandle, pending: State<'_, Pending>) -> CommandResult<UpdateCheck> {
    let release = kubri_core::latest_release().await.map_err(|e| e.to_string())?;
    if !release.newer {
        return Ok(UpdateCheck {
            release,
            installable: false,
            manual: None,
        });
    }
    let found = match not_installable(&app) {
        Some(why) => Err(why),
        None => find(&app).await,
    };
    let (update, manual) = match found {
        Ok(update) => (Some(Arc::new(update)), None),
        Err(why) => {
            tracing::info!("Kubri {} is out, to install by hand: {why}", release.version);
            (None, Some(why))
        }
    };
    let installable = update.is_some();
    *pending.0.lock().unwrap() = update;
    Ok(UpdateCheck {
        release,
        installable,
        manual,
    })
}

/// The update package for this install, from the latest release's `latest.json`. The updater's
/// HTTP client uses the TLS provider [`kubri_core::latest_release`] has installed.
async fn find(app: &AppHandle) -> Result<Update, String> {
    let fail = |e: tauri_plugin_updater::Error| {
        tracing::warn!("cannot look up the update package: {e}");
        "The release has no update package for this install.".to_owned()
    };
    app.updater()
        .map_err(fail)?
        .check()
        .await
        .map_err(fail)?
        .ok_or_else(|| "The release has no update package for this install yet.".to_owned())
}

/// Downloads the update [`update_check`] found, checks its signature, installs it and starts the
/// new version. On Windows the installer closes Kubri and starts it again itself.
#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    pending: State<'_, Pending>,
    on_progress: Channel<UpdateProgress>,
) -> CommandResult<()> {
    let update = pending
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or("There is no update to install; check again.")?;
    let mut done = 0u64;
    let mut last = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                done += chunk as u64;
                // Every 256 KiB is plenty for a progress bar.
                if done - last >= 256 * 1024 || total == Some(done) {
                    last = done;
                    let _ = on_progress.send(UpdateProgress::Download { done, total });
                }
            },
            || {
                let _ = on_progress.send(UpdateProgress::Install);
            },
        )
        .await
        .map_err(|e| format!("Could not download the update: {e}"))?;
    // Installing waits for the password prompt of pkexec or macOS, so off the async threads.
    let version = update.version.clone();
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Could not install the update: {e}"))?;
    tracing::info!("updated to Kubri {version}, starting it");
    app.restart()
}
