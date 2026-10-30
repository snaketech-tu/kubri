//! Whether a newer Kubri is out, from the latest release on GitHub.

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const LATEST: &str = "https://api.github.com/repos/snaketech-tu/kubri/releases/latest";
/// GitHub is asked at most this often; the answer in between is the last one.
const KEEP: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestRelease {
    /// Like `1.0.0`.
    pub version: String,
    /// The release page with its notes and downloads.
    pub url: String,
    /// The version running.
    pub current: String,
    /// The release is newer than the version running.
    pub newer: bool,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

static CLIENT: LazyLock<std::result::Result<reqwest::Client, String>> = LazyLock::new(|| {
    // kube brings ring; reqwest uses whatever provider the process has.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        // GitHub refuses requests without one.
        .user_agent(concat!("kubri/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
});

static LAST: Mutex<Option<(Instant, LatestRelease)>> = Mutex::new(None);

/// The latest release of Kubri, and whether it is newer than this one. Only this asks GitHub,
/// and only when the app asks, which it does once a start unless turned off in the settings.
pub async fn latest_release() -> Result<LatestRelease> {
    if let Some((at, release)) = LAST.lock().expect("release lock").clone()
        && at.elapsed() < KEEP
    {
        return Ok(release);
    }
    let fail =
        |e: reqwest::Error| Error::Unsupported(format!("Could not ask GitHub for the latest release: {e}"));
    let client = CLIENT.as_ref().map_err(|e| Error::Unsupported(e.clone()))?;
    let found: GitHubRelease = client
        .get(LATEST)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(fail)?
        .json()
        .await
        .map_err(fail)?;
    let release = compare(&found.tag_name, &found.html_url, env!("CARGO_PKG_VERSION"));
    *LAST.lock().expect("release lock") = Some((Instant::now(), release.clone()));
    Ok(release)
}

fn compare(tag: &str, url: &str, current: &str) -> LatestRelease {
    let version = tag.trim_start_matches('v').to_owned();
    let newer =
        matches!((parse(&version), parse(current)), (Some(latest), Some(running)) if latest > running);
    LatestRelease {
        version,
        url: url.to_owned(),
        current: current.to_owned(),
        newer,
    }
}

/// `1.2.3` as numbers; anything after the patch number, like `-rc.1`, is ignored.
fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?;
    let digits = patch.find(|c: char| !c.is_ascii_digit()).unwrap_or(patch.len());
    Some((major, minor, patch[..digits].parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_by_number() {
        let url = "https://github.com/snaketech-tu/kubri/releases/tag/v1.0.0";
        let release = compare("v1.0.0", url, "0.9.0");
        assert_eq!(release.version, "1.0.0");
        assert_eq!(release.current, "0.9.0");
        assert!(release.newer);
        assert!(compare("v0.10.0", url, "0.9.2").newer);
        assert!(!compare("v0.9.0", url, "0.9.0").newer);
        assert!(!compare("v0.8.5", url, "0.9.0").newer);
        assert!(compare("v1.0.1-rc.1", url, "1.0.0").newer);
        assert!(!compare("nightly", url, "1.0.0").newer);
    }
}
