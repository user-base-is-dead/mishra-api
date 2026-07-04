//! proxy IP pool management
//!
//! Independent of credential management, stored as proxy_pool.json
//!
//! Besides create, read, update, and delete, it also provides active health checks: periodically (or on demand) it sends through each proxy a request to a
//! Lightweight public probe endpoint; records connectivity and latency. A proxy whose consecutive probe failures reach the threshold is automatically disabled.

use crate::http_client::{ProxyConfig, build_client};
use crate::model::config::TlsBackend;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// The health check probe endpoint: returns 204 No Content a lightweight public address, not dependent on upstream. Kiro.
const PROXY_HEALTH_CHECK_URL: &str = "https://www.gstatic.com/generate_204";
/// single probe timeout (seconds)
const PROXY_PROBE_TIMEOUT_SECS: u64 = 8;
/// Consecutive probe failure threshold: on reaching it, auto disables (like the credential MAX_FAILURES_PER_CREDENTIAL aligned)
const MAX_PROXY_PROBE_FAILURES: u32 = 3;

/// proxy health status
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyHealth {
    /// not yet probed
    #[default]
    Unknown,
    /// the most recent probe succeeded
    Healthy,
    /// the most recent probe failed
    Unhealthy,
}

/// persisted proxy entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyEntry {
    pub id: u64,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Health status (the health check result).
    #[serde(default)]
    pub health: ProxyHealth,
    /// The latency of the most recent successful probe (milliseconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    /// the most recent probe time (RFC3339)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    /// Consecutive probe failure count (cleared on success).
    #[serde(default)]
    pub consecutive_failures: u32,
    /// Whether it was auto disabled by the health check (distinct from a user manual disable).
    #[serde(default)]
    pub auto_disabled: bool,
}

fn default_true() -> bool {
    true
}

/// proxy assignment result
pub enum GetUrlResult {
    /// The proxy exists and is enabled; returns URL
    Ok(String),
    /// proxydoes not exist
    NotFound,
    /// the proxy exists but has been disabled
    Disabled,
}

/// A summary of one full health check.
#[derive(Debug, Clone, Default)]
pub struct CheckSummary {
    /// probesuccesscount
    pub healthy: usize,
    /// probefailedcount
    pub unhealthy: usize,
    /// The number newly auto disabled this round.
    pub auto_disabled: usize,
}

/// single proxy probe result
enum ProbeResult {
    Ok { latency_ms: u32 },
    Err { error: String },
}

pub struct ProxyPoolManager {
    entries: Mutex<Vec<ProxyEntry>>,
    // Only needs an atomic increment, no need to entries interlock; by convention used independently, no lock ordering issue.
    next_id: AtomicU64,
    path: Option<PathBuf>,
    /// TLS backend, build for probing HTTP client needed when
    tls_backend: TlsBackend,
}

/// validateproxy URL of scheme iswhethervalid
fn validate_proxy_url(url: &str) -> anyhow::Result<()> {
    let valid_schemes = ["http://", "https://", "socks5://", "socks4://"];
    if !valid_schemes.iter().any(|s| url.starts_with(s)) {
        anyhow::bail!(
            "proxy URL scheme noneeffect,support: http/https/socks4/socks5(received: {})",
            url
        );
    }
    // simplecheck host:port exists
    let after_scheme = valid_schemes
        .iter()
        .find(|s| url.starts_with(*s))
        .map(|s| &url[s.len()..])
        .unwrap_or(url);
    // after_scheme may be user:pass@host:port or host:port
    let host_part = after_scheme.rsplit('@').next().unwrap_or(after_scheme);
    if !host_part.contains(':') {
        anyhow::bail!("proxy URL missingendslogan: {}", url);
    }
    Ok(())
}

impl ProxyPoolManager {
    pub fn new(path: Option<PathBuf>, tls_backend: TlsBackend) -> Self {
        let entries = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Vec<ProxyEntry>>(&s).ok())
            .unwrap_or_default();

        let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;

        Self {
            entries: Mutex::new(entries),
            next_id: AtomicU64::new(next_id),
            path,
            tls_backend,
        }
    }

    pub fn list(&self) -> Vec<ProxyEntry> {
        self.entries.lock().clone()
    }

    pub fn add(&self, url: String, label: Option<String>) -> anyhow::Result<ProxyEntry> {
        let url = url.trim().to_string();
        if url.is_empty() {
            anyhow::bail!("proxy URL notcanis empty");
        }
        validate_proxy_url(&url)?;

        let mut entries = self.entries.lock();

        if entries.iter().any(|e| e.url == url) {
            anyhow::bail!("proxy URL already exists: {}", url);
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let entry = ProxyEntry {
            id,
            url,
            label,
            enabled: true,
            health: ProxyHealth::Unknown,
            latency_ms: None,
            last_checked_at: None,
            consecutive_failures: 0,
            auto_disabled: false,
        };
        entries.push(entry.clone());
        drop(entries);

        self.persist()?;
        Ok(entry)
    }

    /// Batch add: completes all inserts within a single lock, then persists once at the end.
    pub fn batch_add(&self, urls: Vec<String>) -> (Vec<ProxyEntry>, Vec<String>) {
        let mut added = vec![];
        let mut errors = vec![];

        let mut entries = self.entries.lock();
        for url in urls {
            let url = url.trim().to_string();
            if url.is_empty() || url.starts_with('#') {
                continue;
            }
            if let Err(e) = validate_proxy_url(&url) {
                errors.push(e.to_string());
                continue;
            }
            if entries.iter().any(|e| e.url == url) {
                errors.push(format!("proxy URL already exists: {}", url));
                continue;
            }
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let entry = ProxyEntry {
                id,
                url,
                label: None,
                enabled: true,
                health: ProxyHealth::Unknown,
                latency_ms: None,
                last_checked_at: None,
                consecutive_failures: 0,
                auto_disabled: false,
            };
            entries.push(entry.clone());
            added.push(entry);
        }
        drop(entries);

        if !added.is_empty() {
            if let Err(e) = self.persist() {
                tracing::warn!("Persistence failed after batch adding proxies.: {}", e);
            }
        }

        (added, errors)
    }

    pub fn delete(&self, id: u64) -> anyhow::Result<()> {
        let mut entries = self.entries.lock();
        let len_before = entries.len();
        entries.retain(|e| e.id != id);
        if entries.len() == len_before {
            anyhow::bail!("proxydoes not exist: {}", id);
        }
        drop(entries);
        self.persist()?;
        Ok(())
    }

    /// set the proxy enabled/disablestate
    ///
    /// On manual enable by the user, clears the health check auto disable flag and the consecutive failure count,
    /// Lets the proxy rejoin health checks and allocation.
    pub fn set_enabled(&self, id: u64, enabled: bool) -> anyhow::Result<()> {
        let mut entries = self.entries.lock();
        let entry = entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("proxydoes not exist: {}", id))?;
        entry.enabled = enabled;
        if enabled {
            entry.auto_disabled = false;
            entry.consecutive_failures = 0;
        }
        drop(entries);
        self.persist()?;
        Ok(())
    }

    /// fetchproxy URL, distinguish"does not exist"and"disabled"two cases
    pub fn get_url(&self, id: u64) -> GetUrlResult {
        match self.entries.lock().iter().find(|e| e.id == id) {
            None => GetUrlResult::NotFound,
            Some(e) if !e.enabled => GetUrlResult::Disabled,
            Some(e) => GetUrlResult::Ok(e.url.clone()),
        }
    }

    /// Gets all proxies available for allocation. URL: enabled and not Unhealthy
    pub fn assignable_urls(&self) -> Vec<String> {
        self.entries
            .lock()
            .iter()
            .filter(|e| e.enabled && e.health != ProxyHealth::Unhealthy)
            .map(|e| e.url.clone())
            .collect()
    }

    fn persist(&self) -> anyhow::Result<()> {
        let path = match &self.path {
            Some(p) => p,
            None => return Ok(()),
        };
        let entries = self.entries.lock();
        let json = serde_json::to_string_pretty(&*entries)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

// ============ healthcheck ============

impl ProxyPoolManager {
    /// probe a single proxy URL the connectivity and latency.
    ///
    /// request through this proxy `PROXY_HEALTH_CHECK_URL`,success(HTTP 2xx/3xx) is treated as connected,
    /// Returns the round trip latency; any network error or unexpected status code is treated as failure.
    async fn probe_one(&self, url: &str) -> ProbeResult {
        let proxy = ProxyConfig::new(url);
        let client = match build_client(Some(&proxy), PROXY_PROBE_TIMEOUT_SECS, self.tls_backend) {
            Ok(c) => c,
            Err(e) => {
                return ProbeResult::Err {
                    error: format!("buildprobe client failed: {}", e),
                };
            }
        };

        let started = Instant::now();
        match client.get(PROXY_HEALTH_CHECK_URL).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() || status.is_redirection() {
                    ProbeResult::Ok {
                        latency_ms: started.elapsed().as_millis().min(u32::MAX as u128) as u32,
                    }
                } else {
                    ProbeResult::Err {
                        error: format!("The probe endpoint returned an unexpected status.: {}", status),
                    }
                }
            }
            Err(e) => ProbeResult::Err {
                error: e.to_string(),
            },
        }
    }

    /// Writes a probe result back to the given entry and triggers auto disable as needed.
    ///
    /// return `(becomesnothealth, newly auto disabled this time)` for summary statistics.
    fn apply_probe_result(entry: &mut ProxyEntry, result: &ProbeResult) -> (bool, bool) {
        entry.last_checked_at = Some(chrono::Utc::now().to_rfc3339());
        match result {
            ProbeResult::Ok { latency_ms } => {
                entry.health = ProxyHealth::Healthy;
                entry.latency_ms = Some(*latency_ms);
                entry.consecutive_failures = 0;
                (false, false)
            }
            ProbeResult::Err { error } => {
                entry.health = ProxyHealth::Unhealthy;
                entry.latency_ms = None;
                entry.consecutive_failures += 1;
                tracing::warn!(
                    "proxy #{} probefailed({}/{}): {}",
                    entry.id,
                    entry.consecutive_failures,
                    MAX_PROXY_PROBE_FAILURES,
                    error
                );
                let mut newly_disabled = false;
                if entry.consecutive_failures >= MAX_PROXY_PROBE_FAILURES && entry.enabled {
                    entry.enabled = false;
                    entry.auto_disabled = true;
                    newly_disabled = true;
                    tracing::error!(
                        "proxy #{} consecutive probe failures {} times, has been auto disabled",
                        entry.id,
                        entry.consecutive_failures
                    );
                }
                (true, newly_disabled)
            }
        }
    }

    /// Full health check: concurrently probes all enabled proxies, writes back the results, and persists once.
    ///
    /// onlyprobecurrent enabled the entry; the user/Auto disabled entries are skipped (a manual re-enable zeroes the count).
    pub async fn check_all(&self) -> CheckSummary {
        // snapshot the ones to be probed (id, url), to avoid holding the lock for a long time
        let targets: Vec<(u64, String)> = self
            .entries
            .lock()
            .iter()
            .filter(|e| e.enabled)
            .map(|e| (e.id, e.url.clone()))
            .collect();

        if targets.is_empty() {
            return CheckSummary::default();
        }

        let probes = targets
            .iter()
            .map(|(id, url)| async move { (*id, self.probe_one(url).await) });
        let results = futures::future::join_all(probes).await;

        let mut summary = CheckSummary::default();
        {
            let mut entries = self.entries.lock();
            for (id, result) in &results {
                if let Some(entry) = entries.iter_mut().find(|e| e.id == *id) {
                    let (unhealthy, newly_disabled) = Self::apply_probe_result(entry, result);
                    if unhealthy {
                        summary.unhealthy += 1;
                    } else {
                        summary.healthy += 1;
                    }
                    if newly_disabled {
                        summary.auto_disabled += 1;
                    }
                }
            }
        }

        if let Err(e) = self.persist() {
            tracing::warn!("Persistence failed after the health check.: {}", e);
        }
        summary
    }

    /// Instant probe of a single proxy (for UIinvoked by the test button), writes back the result and persists.
    pub async fn check_one(&self, id: u64) -> anyhow::Result<ProxyEntry> {
        let url = self
            .entries
            .lock()
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.url.clone())
            .ok_or_else(|| anyhow::anyhow!("proxydoes not exist: {}", id))?;

        let result = self.probe_one(&url).await;

        let entry = {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("proxydoes not exist: {}", id))?;
            Self::apply_probe_result(entry, &result);
            entry.clone()
        };

        self.persist()?;
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(url: &str) -> ProxyEntry {
        ProxyEntry {
            id: 1,
            url: url.to_string(),
            label: None,
            enabled: true,
            health: ProxyHealth::Unknown,
            latency_ms: None,
            last_checked_at: None,
            consecutive_failures: 0,
            auto_disabled: false,
        }
    }

    #[test]
    fn old_json_without_new_fields_deserializes() {
        // old format JSON only id/url/label/enabled, the new field should be by serde default complete
        let json = r#"[{"id":1,"url":"socks5://127.0.0.1:1080","enabled":true}]"#;
        let entries: Vec<ProxyEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.health, ProxyHealth::Unknown);
        assert_eq!(e.latency_ms, None);
        assert_eq!(e.consecutive_failures, 0);
        assert!(!e.auto_disabled);
    }

    #[test]
    fn probe_failure_increments_and_auto_disables_at_threshold() {
        let mut entry = make_entry("socks5://127.0.0.1:1080");
        let err = ProbeResult::Err {
            error: "connection refused".to_string(),
        };
        // First two failures: the count accumulates, still enabled.
        for n in 1..MAX_PROXY_PROBE_FAILURES {
            let (unhealthy, disabled) = ProxyPoolManager::apply_probe_result(&mut entry, &err);
            assert!(unhealthy);
            assert!(!disabled);
            assert_eq!(entry.consecutive_failures, n);
            assert!(entry.enabled);
            assert!(!entry.auto_disabled);
        }
        // number N failures: auto disable
        let (_, disabled) = ProxyPoolManager::apply_probe_result(&mut entry, &err);
        assert!(disabled);
        assert_eq!(entry.consecutive_failures, MAX_PROXY_PROBE_FAILURES);
        assert!(!entry.enabled);
        assert!(entry.auto_disabled);
    }

    #[test]
    fn probe_success_clears_failures_and_marks_healthy() {
        let mut entry = make_entry("socks5://127.0.0.1:1080");
        entry.consecutive_failures = 2;
        entry.health = ProxyHealth::Unhealthy;
        let ok = ProbeResult::Ok { latency_ms: 123 };
        let (unhealthy, disabled) = ProxyPoolManager::apply_probe_result(&mut entry, &ok);
        assert!(!unhealthy);
        assert!(!disabled);
        assert_eq!(entry.consecutive_failures, 0);
        assert_eq!(entry.health, ProxyHealth::Healthy);
        assert_eq!(entry.latency_ms, Some(123));
    }

    #[test]
    fn set_enabled_true_clears_auto_disable_state() {
        let mgr = ProxyPoolManager::new(None, TlsBackend::Rustls);
        let entry = mgr.add("socks5://127.0.0.1:1080".to_string(), None).unwrap();
        // simulate the auto disable state
        {
            let mut entries = mgr.entries.lock();
            let e = entries.iter_mut().find(|e| e.id == entry.id).unwrap();
            e.enabled = false;
            e.auto_disabled = true;
            e.consecutive_failures = MAX_PROXY_PROBE_FAILURES;
        }
        mgr.set_enabled(entry.id, true).unwrap();
        let list = mgr.list();
        let e = list.iter().find(|e| e.id == entry.id).unwrap();
        assert!(e.enabled);
        assert!(!e.auto_disabled);
        assert_eq!(e.consecutive_failures, 0);
    }
}
