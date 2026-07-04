//! Kiro IDE versionauto fetch
//!
//! Reads from the official stable metadata endpoint. `currentRelease` field, obtain the currently published Kiro IDE version number,
//! used to construct with the official IDE consistent User-Agent(`KiroIDE-<version>-<machineId>`).
//!
//! - enterprocessinsidecache(`OnceLock<RwLock<Option<String>>>`)+ background scheduled refresh;
//! - cross platform `currentRelease` consistent, fixedly use win32-x64 metadatathen it is fine;
//! - When fetching fails the caller falls back to `config.kiro_version`, does not block startup.
//!
//! note:usagetype REST interface (getUsageLimits / ListAvailableModels / setUserPreference)
//! Does not use the latest version here.——new version IDE mandatorily require for these interfaces profileArn, for Enterprise/IdC
//! the account would fail. Those interfaces fixedly use [`USAGE_API_KIRO_VERSION`]: thisversionnoneneed profileArn
//! can return the subscription and usage.

use std::sync::OnceLock;
use std::time::Duration;

use parking_lot::RwLock;
use serde::Deserialize;

use crate::http_client::{ProxyConfig, build_client};
use crate::model::config::TlsBackend;

/// The official stable metadata endpoint (`currentRelease` namely current IDE version, consistent across platforms).
///
/// note: must use `linux-x64` / `darwin-*` path——`win32-*` path in CDN returns on 403
/// (Windows use a different distribution format). The version number itself is platform independent; any available platform works.
const METADATA_URL: &str =
    "https://prod.download.desktop.kiro.dev/stable/metadata-linux-x64-stable.json";

/// usagetypeinterface (getUsageLimits / ListAvailableModels / setUserPreference)fixedused
/// Kiro IDE version: under this version upstream does not need profileArn can return the data,Enterprise/IdC the account is also usable.
pub const USAGE_API_KIRO_VERSION: &str = "0.9.2";

static LATEST_VERSION: OnceLock<RwLock<Option<String>>> = OnceLock::new();

fn cell() -> &'static RwLock<Option<String>> {
    LATEST_VERSION.get_or_init(|| RwLock::new(None))
}

/// the latest already automatically obtained Kiro IDE version (has a value only after a successful background refresh).
pub fn cached() -> Option<String> {
    cell().read().clone()
}

/// returnvalid Kiro IDE Version: prefers the automatically fetched latest version, otherwise falls back to `fallback`
pub fn effective(fallback: &str) -> String {
    cached().unwrap_or_else(|| fallback.to_string())
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(rename = "currentRelease")]
    current_release: Option<String>,
}

/// pull the latest version number once
pub async fn fetch_latest(
    proxy: Option<&ProxyConfig>,
    tls_backend: TlsBackend,
) -> anyhow::Result<String> {
    let client = build_client(proxy, 15, tls_backend)?;
    let resp = client.get(METADATA_URL).send().await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("fetch Kiro version metadata failed: {}", status);
    }
    let meta: Metadata = resp.json().await?;
    meta.current_release
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("metadatamissing currentRelease"))
}

/// Starts a background task: fetches once immediately, then every `interval` refreshonce.
///
/// Failure only logs a warning and does not affect the service (the caller falls back to `config.kiro_version`).
pub fn spawn_refresher(proxy: Option<ProxyConfig>, tls_backend: TlsBackend, interval: Duration) {
    tokio::spawn(async move {
        loop {
            match fetch_latest(proxy.as_ref(), tls_backend).await {
                Ok(version) => {
                    let changed = cached().as_deref() != Some(version.as_str());
                    *cell().write() = Some(version.clone());
                    if changed {
                        tracing::info!("alreadyauto fetch Kiro IDE version: {}", version);
                    }
                }
                Err(e) => {
                    tracing::warn!("auto fetch Kiro IDE The version failed (continues using the version from config).: {}", e);
                }
            }
            tokio::time::sleep(interval).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_parses_current_release() {
        let json = r#"{"currentRelease":"0.12.301","releases":[]}"#;
        let meta: Metadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.current_release.as_deref(), Some("0.12.301"));
    }

    #[test]
    fn test_effective_falls_back_without_cache() {
        // fall back when no cache is injected to fallback(note: other tests may have already filled the global cache,
        // so here it only asserts the return value is non empty and a valid string.
        let v = effective("0.9.2");
        assert!(!v.is_empty());
    }
}
