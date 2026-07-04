//! Admin API business logic service

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Duration, Timelike, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::http_client::ProxyConfig;
use crate::kiro::auth::idc::{self, BUILDER_ID_START_URL};
use crate::kiro::auth::social;
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::token_manager::MultiTokenManager;
use crate::model::config::Config;

use super::error::AdminServiceError;
use super::proxy_pool::{GetUrlResult, ProxyPoolManager};
use super::types::{
    AccountThrottleConfigResponse, AddCredentialRequest, AddCredentialResponse,
    AssignProxyRequest, AssignRoundRobinResponse, AvailableModelItem, AvailableModelsResponse,
    BalanceResponse, BatchAddProxyRequest, BatchImportEvent,
    CheckRateLimitRequest, CredentialStatusItem, CredentialsStatusResponse, EnableOverageAllResult,
    GitHubRateLimitInfo, ImageUpdateResponse, ExportedAccount, ExportedCredentials,
    CredentialsExportResponse,
    LoadBalancingModeResponse, LogGovernanceConfigResponse, PollIdcLoginResponse,
    ProxyCheckAllResponse, ProxyCheckResponse, ProxyPoolEntry, ProxyPoolResponse,
    QuotaExceededResult, SetAccountThrottleConfigRequest, SetLoadBalancingModeRequest,
    SetLogGovernanceConfigRequest, SetUpdateConfigRequest, StartIdcLoginRequest,
    StartIdcLoginResponse, StartSocialLoginRequest, StartSocialLoginResponse, UpdateCheckInfo,
    UpdateConfigResponse, UpdateCredentialRequest, UpdateRefreshTokenRequest,
};

/// Balance cache expiry time (seconds),5 minutes
const BALANCE_CACHE_TTL_SECS: i64 = 300;

/// The cache time for the online check for update result (seconds),30 minutes.
/// The cache time for the online check for update result (seconds),30 minutes.
/// Docker Hub of tags the interface has for anonymous access IP dimensional throttle,30 minutes TTL sincecanletuser
/// sees the red dot reminder while avoiding repeated requests within a short time being throttled.
const UPDATE_CHECK_TTL_SECS: i64 = 1800;

/// The cached balance entry (including timestamp).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBalance {
    /// cachetime(Unix seconds)
    cached_at: f64,
    /// cached balance data
    data: BalanceResponse,
}

/// Single credential import result (for internal server use, mapped to SSE event)
pub(crate) enum ImportStatus {
    Verified,
    /// Direct import (no liveness check) succeeded.
    Imported,
    Duplicate,
    Failed,
}

pub(crate) struct ImportItemResult {
    pub status: ImportStatus,
    pub credential_id: Option<u64>,
    pub email: Option<String>,
    pub balance: Option<BalanceResponse>,
    pub error: Option<String>,
    pub rolled_back: bool,
}

impl ImportItemResult {
    /// convert to SSE event (carries the index within the array).
    pub fn into_event(self, index: usize) -> BatchImportEvent {
        let status = match self.status {
            ImportStatus::Verified => "verified",
            ImportStatus::Imported => "imported",
            ImportStatus::Duplicate => "duplicate",
            ImportStatus::Failed => "failed",
        }
        .to_string();
        BatchImportEvent {
            index: Some(index),
            status,
            credential_id: self.credential_id,
            email: self.email,
            usage: self.balance.as_ref().map(|b| {
                format!(
                    "{:.0}/{:.0}",
                    b.current_usage.round(),
                    b.usage_limit.round()
                )
            }),
            subscription: self.balance.and_then(|b| b.subscription_title),
            error: self.error,
            rolled_back: if self.rolled_back { Some(true) } else { None },
            summary: None,
        }
    }
}

/// cached"checkupdate"result
#[derive(Debug, Clone)]
struct CachedUpdateCheck {
    /// cachetime
    cached_at: DateTime<Utc>,
    /// the pulled update information
    info: UpdateCheckInfo,
}

#[derive(Debug, Clone)]
struct RuntimeUpdateConfig {
    previous_version: Option<String>,
    last_applied_at: Option<String>,
    github_token: Option<String>,
    auto_apply: bool,
    auto_apply_time: String,
}

impl RuntimeUpdateConfig {
    fn from_config(config: &Config) -> Self {
        Self {
            previous_version: config.update_previous_version.clone(),
            last_applied_at: config.update_last_applied_at.clone(),
            github_token: config.github_token.clone(),
            auto_apply: config.update_auto_apply,
            auto_apply_time: config.update_auto_apply_time.clone(),
        }
    }

    fn response(&self) -> UpdateConfigResponse {
        UpdateConfigResponse {
            previous_version: self.previous_version.clone(),
            last_applied_at: self.last_applied_at.clone(),
            github_token_set: self
                .github_token
                .as_deref()
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false),
            auto_apply: self.auto_apply,
            auto_apply_time: self.auto_apply_time.clone(),
        }
    }
}

/// Admin service
///
/// encapsulateall Admin API ofbusiness logic
pub struct AdminService {
    token_manager: Arc<MultiTokenManager>,
    balance_cache: Mutex<HashMap<u64, CachedBalance>>,
    cache_path: Option<PathBuf>,
    /// The set of registered endpoint names (used for add_credential validation)
    known_endpoints: HashSet<String>,
    /// proxy IP poolmanager
    proxy_pool: ProxyPoolManager,
    /// Online image update runtime config.
    update_config: Mutex<RuntimeUpdateConfig>,
    /// recentonce"checkupdate"result(carry TTL, used forreduce GitHub API call)
    update_check_cache: Mutex<Option<CachedUpdateCheck>>,
    /// enterlinein IdC device authorization session
    idc_sessions: Arc<Mutex<HashMap<String, IdcAuthSession>>>,
    /// enterlinein Social loginsession
    social_sessions: Arc<Mutex<HashMap<String, SocialAuthSession>>>,
    /// Request trace storage (for log governance: switch). + retention days changeable at runtime)
    trace_store: Option<crate::admin::trace_db::SharedTraceStore>,
    /// Usage log recorder (for log governance: retention days can be changed at runtime).
    usage_recorder: Option<crate::admin::usage_stats::SharedRecorder>,
}

/// Social login session state
struct SocialAuthSession {
    auth_endpoint: String,
    /// generated at initiation state, used for CSRF verify
    state: String,
    code_verifier: String,
    redirect_uri: String,
    expires_at: DateTime<Utc>,
    /// received OAuth the data at callback (code + login_option + path)
    callback_rx: tokio::sync::Mutex<tokio::sync::oneshot::Receiver<social::OAuthCallbackData>>,
    cred_template: KiroCredentials,
    proxy: Option<ProxyConfig>,
    /// Drop automatically closes the callback server and releases the port (local mode Some;remotemode None)
    _server_handle: Option<social::ServerHandle>,
    /// remote mode: public network GET the callback route via this Sender Delivers the callback data (local mode). None).
    /// takeoutafterthat is None, to ensure it is delivered only once.
    remote_callback_tx:
        Option<Mutex<Option<tokio::sync::oneshot::Sender<social::OAuthCallbackData>>>>,
    /// Updates this credential on re-login. Token(non None updates an existing credential rather than creating a new one)
    relogin_target_id: Option<u64>,
}

/// Remote public callback delivery result (for GET the callback route renders the hint page)
pub enum RemoteCallbackOutcome {
    /// Delivered successfully; waiting for polling to complete. token redeem
    Delivered,
    /// the session does not exist (state mismatch / non remote mode session)
    NotFound,
    /// sessionexpired
    Expired,
    /// The callback has already been processed (repeated click). / concurrency finishedinto)
    AlreadyCompleted,
}

/// IdC device authorization session state
struct IdcAuthSession {
    region: String,
    client_id: String,
    client_secret: String,
    device_code: String,
    expires_at: DateTime<Utc>,
    poll_interval: i64,
    /// The credential config written after a successful login.
    cred_template: KiroCredentials,
    /// used forinitiate token requestproxy
    proxy: Option<ProxyConfig>,
    /// Updates this credential on re-login. Token(non None updates an existing credential rather than creating a new one)
    relogin_target_id: Option<u64>,
}

/// Parses the auto update trigger time (`HH:MM`, local 24 hour format). allow `H:M` shorthand,
/// for example `3:0`; on parse failure returns the original string, convenient for error message hints.
fn parse_auto_apply_time(value: &str) -> Result<(u32, u32), AdminServiceError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AdminServiceError::InvalidCredential(
            "The auto update time cannot be empty.".to_string(),
        ));
    }
    let mut parts = trimmed.splitn(2, ':');
    let hour_str = parts.next().unwrap_or("");
    let minute_str = parts.next().unwrap_or("");
    let hour: u32 = hour_str.parse().map_err(|_| {
        AdminServiceError::InvalidCredential(format!(
            "The auto update time format is invalid:{}(should be HH:MM)",
            value
        ))
    })?;
    let minute: u32 = minute_str.parse().map_err(|_| {
        AdminServiceError::InvalidCredential(format!(
            "The auto update time format is invalid:{}(should be HH:MM)",
            value
        ))
    })?;
    if hour > 23 || minute > 59 {
        return Err(AdminServiceError::InvalidCredential(format!(
            "The auto update time is out of range:{}(HH 0-23,MM 0-59)",
            value
        )));
    }
    Ok((hour, minute))
}

/// take HH:MM normalizeinto `HH:MM`(zero padded two digits), convenient for storage and comparison.
fn normalize_auto_apply_time(value: &str) -> Result<String, AdminServiceError> {
    let (h, m) = parse_auto_apply_time(value)?;
    Ok(format!("{:02}:{:02}", h, m))
}

/// GitHub `repos/{owner}/{repo}/releases/tags/{tag}` return JSON inwe care about
/// field, used to attach this release info within the check for update result. changelog.
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    #[serde(default)]
    name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    tag_name: String,
}

/// comparetwo semver string. only by `MAJOR.MINOR.PATCH` three segment numeric comparison, ignore
/// Pre-release suffix; a segment that fails to parse is treated as 0 process (in the worst case"no update").
fn compare_semver(current: &str, latest: &str) -> std::cmp::Ordering {
    parse_semver_core(current).cmp(&parse_semver_core(latest))
}

/// parse semver Three numeric segments; a segment that fails to parse is treated 0; used for latest tag the stable sort.
fn parse_semver_core(value: &str) -> [u32; 3] {
    let core = value
        .trim_start_matches('v')
        .split(|c: char| c == '-' || c == '+')
        .next()
        .unwrap_or("");
    let mut out = [0u32; 3];
    for (i, part) in core.splitn(3, '.').enumerate() {
        if i >= 3 {
            break;
        }
        out[i] = part.parse::<u32>().unwrap_or(0);
    }
    out
}

/// The current build type. Online update goes through"download GitHub Releases binary + enterprocess exitby
/// docker restart policy take overrestart"ofplan.
const BUILD_TYPE: &str = "binary";

/// staging path: download to `<exe>.staged`, before the atomic replacement again mv to `<exe>`.
/// staging path: download to `<exe>.staged-<version>`, before the atomic replacement again mv to `<exe>`.
/// The file name carries the version number, convenient for apply reuse pull The already downloaded binary (skips re-download on a hit).
fn staged_binary_path(exe: &std::path::Path, version: &str) -> std::path::PathBuf {
    let mut s = exe.as_os_str().to_os_string();
    s.push(format!(".staged-{}", version.trim().trim_start_matches('v')));
    std::path::PathBuf::from(s)
}

/// Cleans all except the target version. staged file, avoiding interference from a previously downloaded old version.
fn cleanup_other_staged(exe: &std::path::Path, keep_version: &str) {
    let dir = match exe.parent() {
        Some(d) => d,
        None => return,
    };
    let exe_name = match exe.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return,
    };
    let keep = format!(
        "{}.staged-{}",
        exe_name,
        keep_version.trim().trim_start_matches('v')
    );
    let prefix = format!("{}.staged-", exe_name);
    let entries = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if name.starts_with(&prefix) && name != keep {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Maps a single credential into a nested `Account` struct
///
/// API Key credential none refreshToken, the export format has no corresponding field, skipped.
/// Empty string fields are filtered to keep the export JSON tidy.
fn credential_to_export_account(cred: KiroCredentials) -> Option<ExportedAccount> {
    let refresh_token = cred
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)?;

    fn non_empty(value: Option<String>) -> Option<String> {
        value
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    // authMethod normalize:"idc" → "IdC", restby social handle
    let auth_method = non_empty(cred.auth_method.clone()).map(|m| {
        if m.eq_ignore_ascii_case("idc")
            || m.eq_ignore_ascii_case("builder-id")
            || m.eq_ignore_ascii_case("iam")
        {
            "IdC".to_string()
        } else {
            "social".to_string()
        }
    });
    let is_idc = auth_method.as_deref() == Some("IdC");

    let provider = non_empty(cred.provider.clone());
    // idp and provider Synonymous; when missing, falls back to a valid identity provider based on the auth method.
    let idp = provider
        .clone()
        .unwrap_or_else(|| if is_idc { "BuilderId" } else { "Google" }.to_string());

    let status = if cred.disabled {
        "unknown".to_string()
    } else {
        "active".to_string()
    };

    // expiresAt → Millisecond timestamp (on parse failure or when missing it is 0)
    let expires_at_ms = cred
        .expires_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);

    // Subscription: minimal usable structure (type + raw title)
    let subscription = serde_json::json!({
        "type": subscription_type_from_title(cred.subscription_title.as_deref()),
        "title": cred.subscription_title,
    });
    let now_ms = Utc::now().timestamp_millis();
    let usage = serde_json::json!({
        "current": 0,
        "limit": 0,
        "percentUsed": 0,
        "lastUpdated": now_ms,
    });

    // onlyexportreal profileArn, skip BuilderID placeholder
    let profile_arn = cred.effective_profile_arn().map(str::to_string);

    let credentials = ExportedCredentials {
        access_token: non_empty(cred.access_token).unwrap_or_default(),
        csrf_token: String::new(),
        refresh_token: Some(refresh_token),
        client_id: non_empty(cred.client_id),
        client_secret: non_empty(cred.client_secret),
        region: non_empty(cred.region.clone())
            .or_else(|| non_empty(cred.auth_region.clone()))
            .or_else(|| non_empty(cred.api_region.clone())),
        start_url: non_empty(cred.start_url.clone()),
        expires_at: expires_at_ms,
        auth_method,
        provider: provider.clone(),
    };

    Some(ExportedAccount {
        id: uuid::Uuid::new_v4().to_string(),
        email: non_empty(cred.email).unwrap_or_default(),
        nickname: None,
        idp,
        user_id: None,
        profile_arn,
        machine_id: non_empty(cred.machine_id),
        credentials,
        subscription,
        usage,
        tags: Vec::new(),
        status,
        created_at: now_ms,
        last_used_at: now_ms,
    })
}

/// inferred from the subscription title `SubscriptionType`(coarse grained, the importer self corrects after a refresh)
fn subscription_type_from_title(title: Option<&str>) -> &'static str {
    let Some(title) = title else { return "Free" };
    let u = title.to_uppercase();
    if u.contains("FREE") {
        "Free"
    } else if u.contains("PRO+") || u.contains("PRO PLUS") || u.contains("PRO_PLUS") {
        "Pro_Plus"
    } else if u.contains("POWER") || u.contains("ENTERPRISE") || u.contains("TEAM") {
        "Enterprise"
    } else if u.contains("PRO") {
        "Pro"
    } else {
        "Free"
    }
}

/// GitHub Release repositoryname(owner/repo).
/// The version number needed for online update,changelog, binary assets are all taken from here.
const GITHUB_RELEASES_REPO: &str = "ZyphrZero/kiro.rs";

impl AdminService {
    pub fn new(
        token_manager: Arc<MultiTokenManager>,
        known_endpoints: impl IntoIterator<Item = String>,
    ) -> Self {
        let cache_path = token_manager
            .cache_dir()
            .map(|d| d.join("kiro_balance_cache.json"));

        let proxy_pool_path = token_manager.cache_dir().map(|d| d.join("proxy_pool.json"));
        let token_manager_tls_backend = token_manager.config().tls_backend;

        let balance_cache = Self::load_balance_cache_from(&cache_path);
        let update_config = RuntimeUpdateConfig::from_config(token_manager.config());

        let svc = Self {
            token_manager,
            balance_cache: Mutex::new(balance_cache),
            cache_path,
            known_endpoints: known_endpoints.into_iter().collect(),
            proxy_pool: ProxyPoolManager::new(proxy_pool_path, token_manager_tls_backend),
            update_config: Mutex::new(update_config),
            update_check_cache: Mutex::new(None),
            idc_sessions: Arc::new(Mutex::new(HashMap::new())),
            social_sessions: Arc::new(Mutex::new(HashMap::new())),
            trace_store: None,
            usage_recorder: None,
        };

        // background task: every 5 Cleans expired login sessions every minute to prevent memory leaks.
        {
            let idc = Arc::clone(&svc.idc_sessions);
            let social = Arc::clone(&svc.social_sessions);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    let now = Utc::now();
                    idc.lock().retain(|_, s| now < s.expires_at);
                    social.lock().retain(|_, s| now < s.expires_at);
                }
            });
        }

        svc
    }

    /// expose TokenManager give handlers(group management needs count / rename / remove credential groups field)
    pub fn token_manager(&self) -> &Arc<MultiTokenManager> {
        &self.token_manager
    }

    /// inject the log governance handle (trace store + usage recorder), used to change the retention period at runtime./switch.
    pub fn with_log_governance(
        mut self,
        trace_store: Option<crate::admin::trace_db::SharedTraceStore>,
        usage_recorder: Option<crate::admin::usage_stats::SharedRecorder>,
    ) -> Self {
        self.trace_store = trace_store;
        self.usage_recorder = usage_recorder;
        self
    }

    /// get all credential statuses
    pub fn get_all_credentials(&self) -> CredentialsStatusResponse {
        let snapshot = self.token_manager.snapshot();
        let default_endpoint = self.token_manager.config().default_endpoint.clone();

        // A one shot snapshot of the balance cache, avoiding N times lock
        let balance_snapshot: HashMap<u64, CachedBalance> = {
            let cache = self.balance_cache.lock();
            cache.clone()
        };
        let now_ts = Utc::now().timestamp() as f64;

        let mut credentials: Vec<CredentialStatusItem> = snapshot
            .entries
            .into_iter()
            .map(|entry| {
                let (balance, balance_updated_at) = balance_snapshot
                    .get(&entry.id)
                    .filter(|c| (now_ts - c.cached_at) < BALANCE_CACHE_TTL_SECS as f64)
                    .map(|c| (Some(c.data.clone()), Some(c.cached_at)))
                    .unwrap_or((None, None));

                CredentialStatusItem {
                    id: entry.id,
                    priority: entry.priority,
                    disabled: entry.disabled,
                    failure_count: entry.failure_count,
                    total_failure_count: entry.total_failure_count,
                    is_current: entry.id == snapshot.current_id,
                    expires_at: entry.expires_at,
                    auth_method: entry.auth_method,
                    provider: entry.provider,
                    has_profile_arn: entry.has_profile_arn,
                    refresh_token_hash: entry.refresh_token_hash,
                    api_key_hash: entry.api_key_hash,
                    masked_api_key: entry.masked_api_key,
                    email: entry.email,
                    success_count: entry.success_count,
                    last_used_at: entry.last_used_at.clone(),
                    has_proxy: entry.has_proxy,
                    proxy_url: entry.proxy_url,
                    refresh_failure_count: entry.refresh_failure_count,
                    disabled_reason: entry.disabled_reason,
                    endpoint: entry.endpoint.unwrap_or_else(|| default_endpoint.clone()),
                    groups: entry.groups,
                    source_channel: entry.source_channel,
                    balance,
                    balance_updated_at,
                }
            })
            .collect();

        // Sorts by priority (a smaller number means higher priority).
        credentials.sort_by_key(|c| c.priority);

        CredentialsStatusResponse {
            total: snapshot.total,
            available: snapshot.available,
            current_id: snapshot.current_id,
            credentials,
        }
    }

    /// export the credential as compatible JSON(nested `Account` format)
    ///
    /// the returned struct contains refreshToken,accessToken,clientSecret and other sensitive fields,
    /// The caller must ensure transport and storage security by itself; by priority ascending sort, with UI listconsistent.
    /// `id_filter` as None export all credentials when; as Some only export those in the set when ID.
    pub fn export_credentials(
        &self,
        id_filter: Option<&HashSet<u64>>,
    ) -> CredentialsExportResponse {
        let mut credentials = self.token_manager.clone_all_credentials();
        if let Some(filter) = id_filter {
            credentials.retain(|c| c.id.map(|id| filter.contains(&id)).unwrap_or(false));
        }
        credentials.sort_by_key(|c| c.priority);

        let accounts = credentials
            .into_iter()
            .filter_map(credential_to_export_account)
            .collect();

        CredentialsExportResponse {
            version: "1.8.3".to_string(),
            exported_at: Utc::now().timestamp_millis(),
            accounts,
            groups: Vec::new(),
            tags: Vec::new(),
        }
    }

    /// one click disable all"over quota"ofcredential (remaining ≤ 0 or usage_percentage ≥ 100)
    ///
    /// datasourceis `balance_cache`, so the frontend had better trigger one first before calling."queryinfo"
    /// or waits for the background scheduler to complete the first refresh. Returns (disablecount, skipcount, overage but not disabled list).
    pub fn disable_quota_exceeded(&self) -> QuotaExceededResult {
        let snapshot = self.token_manager.snapshot();
        let current_id = snapshot.current_id;

        let cache_snapshot: HashMap<u64, CachedBalance> = {
            let cache = self.balance_cache.lock();
            cache.clone()
        };
        let now_ts = Utc::now().timestamp() as f64;

        let mut disabled_ids: Vec<u64> = Vec::new();
        let mut skipped_ids: Vec<u64> = Vec::new();
        let mut switched_current = false;

        for entry in snapshot.entries.iter() {
            if entry.disabled {
                continue;
            }
            let cached = match cache_snapshot.get(&entry.id) {
                Some(c) if (now_ts - c.cached_at) < BALANCE_CACHE_TTL_SECS as f64 => c,
                _ => continue,
            };
            let exceeded = cached.data.remaining <= 0.0 || cached.data.usage_percentage >= 100.0;
            if !exceeded {
                continue;
            }
            match self.token_manager.disable_quota_exceeded(entry.id) {
                Ok(()) => {
                    disabled_ids.push(entry.id);
                    if entry.id == current_id {
                        switched_current = true;
                    }
                }
                Err(e) => {
                    tracing::warn!("one click overage: disable credentials #{} failed: {}", entry.id, e);
                    skipped_ids.push(entry.id);
                }
            }
        }

        if switched_current {
            let _ = self.token_manager.switch_to_next();
        }

        QuotaExceededResult {
            disabled_ids,
            skipped_ids,
        }
    }

    /// set the credential disabled state
    pub fn set_disabled(&self, id: u64, disabled: bool) -> Result<(), AdminServiceError> {
        // first get the current credential ID, used to decide whether a switch is needed.
        let snapshot = self.token_manager.snapshot();
        let current_id = snapshot.current_id;

        self.token_manager
            .set_disabled(id, disabled)
            .map_err(|e| self.classify_error(e, id))?;

        // Only when the disabled one is the current credential does it try to switch to the next.
        if disabled && id == current_id {
            let _ = self.token_manager.switch_to_next();
        }
        Ok(())
    }

    /// set the credential priority
    pub fn set_priority(&self, id: u64, priority: u32) -> Result<(), AdminServiceError> {
        self.token_manager
            .set_priority(id, priority)
            .map_err(|e| self.classify_error(e, id))
    }

    /// Resets the failure count and re-enables.
    pub fn reset_and_enable(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .reset_and_enable(id)
            .map_err(|e| self.classify_error(e, id))
    }

    pub fn clear_throttle(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .clear_throttle(id)
            .map_err(|e| self.classify_error(e, id))
    }

    pub fn reset_success_count(&self, id: Option<u64>) -> Result<u32, AdminServiceError> {
        self.token_manager
            .reset_success_count(id)
            .map_err(|e| self.classify_error(e, id.unwrap_or(0)))
    }

    /// Gets the credential balance (with cache).
    pub async fn get_balance(&self, id: u64) -> Result<BalanceResponse, AdminServiceError> {
        // query firstcache
        {
            let cache = self.balance_cache.lock();
            if let Some(cached) = cache.get(&id) {
                let now = Utc::now().timestamp() as f64;
                if (now - cached.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    tracing::debug!("credential #{} balance hit the cache", id);
                    return Ok(cached.data.clone());
                }
            }
        }

        // Cache miss or expired; fetches from upstream.
        let balance = self.fetch_balance(id).await?;

        // updatecache
        {
            let mut cache = self.balance_cache.lock();
            cache.insert(
                id,
                CachedBalance {
                    cached_at: Utc::now().timestamp() as f64,
                    data: balance.clone(),
                },
            );
        }
        self.save_balance_cache();

        Ok(balance)
    }

    /// Fetches the balance from upstream (no cache).
    async fn fetch_balance(&self, id: u64) -> Result<BalanceResponse, AdminServiceError> {
        let usage = self
            .token_manager
            .get_usage_limits_for(id)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;

        let current_usage = usage.current_usage();
        let usage_limit = usage.usage_limit();
        // allow remaining Shown as a negative value: after enabling overage, actual usage may exceed the limit,
        // directly keep the difference for convenience in UI reflected in"alreadyhow much is owed".
        let remaining = usage_limit - current_usage;
        // usage_percentage Similarly keeps the real value; when over the limit > 100%.
        let usage_percentage = if usage_limit > 0.0 {
            current_usage / usage_limit * 100.0
        } else {
            0.0
        };

        Ok(BalanceResponse {
            id,
            subscription_title: usage.subscription_title().map(|s| s.to_string()),
            current_usage,
            usage_limit,
            remaining,
            usage_percentage,
            next_reset_at: usage.next_date_reset,
            overage_enabled: usage.overage_enabled(),
            overage_capable: usage.overage_capable(),
            overage_capability_raw: usage
                .subscription_info
                .as_ref()
                .and_then(|s| s.overage_capability.clone()),
        })
    }

    /// Gets the currently available model list for the given credential (queries upstream in real time on demand, no cache).
    pub async fn get_available_models(
        &self,
        id: u64,
    ) -> Result<AvailableModelsResponse, AdminServiceError> {
        let resp = self
            .token_manager
            .get_available_models_for(id)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;

        let models = resp
            .models
            .into_iter()
            .map(|m| AvailableModelItem {
                model_id: m.model_id,
                model_name: m.model_name,
                description: m.description,
                max_input_tokens: m.token_limits.and_then(|t| t.max_input_tokens),
            })
            .collect();

        Ok(AvailableModelsResponse { id, models })
    }

    /// Batch refreshes the balance of all non disabled credentials (for background scheduling).
    ///
    /// Runs serially to avoid momentary high concurrency toward upstream; each successful query updates the in-memory cache.
    /// and disk cache. A failed entry does not clear the old cache; the caller may retry on the next poll.
    pub async fn refresh_all_balances(&self) -> (usize, usize) {
        let snapshot = self.token_manager.snapshot();
        let mut success = 0_usize;
        let mut failure = 0_usize;

        for entry in snapshot.entries.into_iter() {
            if entry.disabled {
                continue;
            }
            match self.fetch_balance(entry.id).await {
                Ok(balance) => {
                    {
                        let mut cache = self.balance_cache.lock();
                        cache.insert(
                            entry.id,
                            CachedBalance {
                                cached_at: Utc::now().timestamp() as f64,
                                data: balance,
                            },
                        );
                    }
                    success += 1;
                }
                Err(e) => {
                    tracing::warn!("background refresh the credential #{} balancefailed: {}", entry.id, e);
                    failure += 1;
                }
            }
            // throttle, to avoid upstream throttling
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }

        if success > 0 {
            self.save_balance_cache();
        }
        (success, failure)
    }

    /// Starts the balance background refresh scheduler.
    ///
    /// - Performs one refresh right after startup.
    /// - then sort by `interval` periodic loop refresh
    /// - callsideholdhas `Arc<Self>` suffices, the task in the background tokio runtime run on
    pub fn start_balance_refresher(self: &Arc<Self>, interval: std::time::Duration) {
        let svc = Arc::clone(self);
        tokio::spawn(async move {
            // Waits a moment after startup to let upstream/Token Manager ready
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            loop {
                let started = std::time::Instant::now();
                let (ok, err) = svc.refresh_all_balances().await;
                tracing::info!(
                    "Balance background refresh completed: success {}, failed {}, elapsed {:.1}s",
                    ok,
                    err,
                    started.elapsed().as_secs_f32()
                );
                tokio::time::sleep(interval).await;
            }
        });
    }

    /// Starts the proxy pool background health check scheduler.
    ///
    /// - Waits a moment after startup before the first probe.
    /// - then sort by `interval` Periodic loop, concurrently probes all enabled proxies.
    /// - A proxy whose consecutive probe failures reach the threshold is by `check_all` internal auto disable
    pub fn start_proxy_health_checker(self: &Arc<Self>, interval: std::time::Duration) {
        let svc = Arc::clone(self);
        tokio::spawn(async move {
            // Waits a moment after startup to let the network/proxyready
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            loop {
                let started = std::time::Instant::now();
                let summary = svc.proxy_pool.check_all().await;
                tracing::info!(
                    "Proxy pool health check completed: healthy {}, exception {}, auto disabled this round {}, elapsed {:.1}s",
                    summary.healthy,
                    summary.unhealthy,
                    summary.auto_disabled,
                    started.elapsed().as_secs_f32()
                );
                tokio::time::sleep(interval).await;
            }
        });
    }

    /// Starts the unattended auto update scheduler.
    ///
    /// The task always runs, waking once per minute:
    /// - `update_auto_apply` when closed just record"not yet due", does not make any remote call.
    /// - When enabled, compares the current local time with `update_auto_apply_time`, hit the target minute
    ///   thentriggeronce `apply_image_update`. The same target version is auto applied only once.
    pub fn start_auto_update_scheduler(self: &Arc<Self>) {
        let svc = Arc::clone(self);
        tokio::spawn(async move {
            // give Docker socket / compose Leaves some preparation time for metadata probing.
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            // Within the same minute avoids duplicate triggers; records the most recently applied one."date + version"
            let mut last_run_marker: Option<String> = None;
            let mut last_applied_version: Option<String> = None;

            loop {
                let runtime = svc.update_config.lock().clone();
                if runtime.auto_apply {
                    let target = parse_auto_apply_time(&runtime.auto_apply_time).ok();
                    if let Some((target_hour, target_minute)) = target {
                        let now = chrono::Local::now();
                        let date_minute_marker = format!(
                            "{}-{:02}:{:02}",
                            now.format("%Y-%m-%d"),
                            now.hour(),
                            now.minute()
                        );

                        let hit = now.hour() == target_hour && now.minute() == target_minute;
                        let already_ran_this_minute = last_run_marker.as_deref()
                            == Some(date_minute_marker.as_str());

                        if hit && !already_ran_this_minute {
                            last_run_marker = Some(date_minute_marker);
                            let info = svc.check_update(true).await;
                            if info.has_update
                                && !info.latest_version.is_empty()
                                && last_applied_version.as_deref()
                                    != Some(info.latest_version.as_str())
                            {
                                tracing::info!(
                                    "Auto update: the scheduled time is reached. {}, found a new version {}(current {}), begin applying",
                                    runtime.auto_apply_time,
                                    info.latest_version,
                                    info.current_version
                                );
                            match svc.apply_image_update().await {
                                    Ok(res) => {
                                        tracing::info!("auto update complete:{}", res.message);
                                        last_applied_version = Some(info.latest_version);
                                    }
                                    Err(e) => {
                                        tracing::warn!("auto update failed:{}", e);
                                    }
                                }
                            } else {
                                tracing::info!(
                                    "Auto update: the scheduled time is reached. {}, but the current version is already the latest ({})",
                                    runtime.auto_apply_time,
                                    info.current_version
                                );
                            }
                        }
                    } else {
                        tracing::warn!(
                            "The auto update time config is invalid:{}, skip this round of check",
                            runtime.auto_apply_time
                        );
                    }
                }

                // 30 Second granularity is enough to reliably hit the target minute without missing it under system clock drift.
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        });
    }

    /// addnew credential
    pub async fn add_credential(
        &self,
        req: AddCredentialRequest,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        // Fetches the balance by default (keeps single add / The existing behavior of the login path: the subscription tier is visible right after adding.
        self.add_credential_inner(req, true).await
    }

    /// The core implementation for adding a credential.
    ///
    /// - `fetch_balance = true`: actively fetches the balance after adding (including subscription tier). / email) and write into the cache,
    ///   both"visible once added",alsoas API Key the validity check (that is"validate").
    /// - `fetch_balance = false`: skips the balance fetch, only persists ("directlyimport"path),
    ///   Subscription info is fetched on demand at the first request.
    async fn add_credential_inner(
        &self,
        req: AddCredentialRequest,
        fetch_balance: bool,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        // Validates the endpoint name: unspecified is valid by default, specified must already be registered.
        if let Some(ref name) = req.endpoint {
            if !self.known_endpoints.contains(name) {
                let mut known: Vec<&str> =
                    self.known_endpoints.iter().map(|s| s.as_str()).collect();
                known.sort();
                return Err(AdminServiceError::InvalidCredential(format!(
                    "unknownendpoint \"{}\", registered endpoint: {:?}",
                    name, known
                )));
            }
        }

        // build the credential object
        let email = req.email.clone();
        let new_cred = KiroCredentials {
            id: None,
            access_token: req.access_token,
            refresh_token: req.refresh_token,
            profile_arn: req.profile_arn,
            expires_at: req.expires_at,
            auth_method: Some(req.auth_method),
            provider: req.provider,
            client_id: req.client_id,
            client_secret: req.client_secret,
            start_url: req.start_url,
            priority: req.priority,
            region: req.region,
            auth_region: req.auth_region,
            api_region: req.api_region,
            machine_id: req.machine_id,
            email: req.email,
            subscription_title: None, // Will auto update on the first fetch of the usage quota.
            proxy_url: req.proxy_url,
            proxy_username: req.proxy_username,
            proxy_password: req.proxy_password,
            disabled: false, // A newly added credential is enabled by default.
            kiro_api_key: req.kiro_api_key,
            endpoint: req.endpoint,
            groups: req.groups,
            source_channel: req.source_channel,
        };

        // call token_manager addcredential
        let credential_id = self
            .token_manager
            .add_credential(new_cred)
            .await
            .map_err(|e| self.classify_add_error(e))?;

        // Actively fetches the balance (including subscription tier). / email) and writes to the cache, visible right after adding,
        // while avoiding on the first request Free account bypass Opus modelfilter.
        // only the activation check path needs;"directlyimport"The path is skipped to save this upstream round trip.
        if fetch_balance {
            if let Err(e) = self.get_balance(credential_id).await {
                tracing::warn!("Refreshing the balance failed after adding the credential (does not affect the credential addition).: {}", e);
            }
        }

        Ok(AddCredentialResponse {
            success: true,
            message: format!("credential added successfully,ID: {}", credential_id),
            credential_id,
            email,
        })
    }

    /// Single item handling for batch import.
    ///
    /// - `verify = true`(activation check path):add(internal refresh + cache balance)→ explicitly get the balance for activation check
    ///   → Rolls back the delete on failure. Mirrors the old frontend flow"add → getCredentialBalance → failedrollback".
    /// - `verify = false`(direct import path): only add Persists, does not fetch the balance and does not roll back.
    ///
    /// All done on the server, convenient for `buffer_unordered` bounded concurrency under.
    pub async fn import_one_credential(
        &self,
        req: AddCredentialRequest,
        verify: bool,
    ) -> ImportItemResult {
        // 1. add: dedup / unknownendpoint / token A refresh failure surfaces here; since it was not inserted, no rollback is needed.
        //    verify=false skips the internal balance fetch.
        let resp = match self.add_credential_inner(req, verify).await {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                let is_duplicate =
                    msg.contains("credentialalready exists") || msg.contains("duplicate");
                return ImportItemResult {
                    status: if is_duplicate {
                        ImportStatus::Duplicate
                    } else {
                        ImportStatus::Failed
                    },
                    credential_id: None,
                    email: None,
                    balance: None,
                    error: Some(msg),
                    rolled_back: false,
                };
            }
        };

        // 2. directlyimport:add Completes on success; does not do balance liveness check and does not roll back.
        if !verify {
            return ImportItemResult {
                status: ImportStatus::Imported,
                credential_id: Some(resp.credential_id),
                email: resp.email.clone(),
                balance: None,
                error: None,
                rolled_back: false,
            };
        }

        // 3. Liveness path: explicitly fetches the balance for liveness (OAuth normal path hit add insidecache;
        //    API Key none token refresh; fetching the balance is the real liveness check, and on failure it rolls back).
        match self.get_balance(resp.credential_id).await {
            Ok(balance) => ImportItemResult {
                status: ImportStatus::Verified,
                credential_id: Some(resp.credential_id),
                email: resp.email.clone(),
                balance: Some(balance),
                error: None,
                rolled_back: false,
            },
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!(
                    "batch import credentials #{} activation check failed, roll back and delete: {}",
                    resp.credential_id,
                    msg
                );
                // rollback: delete directly (delete_credential will clean balance cache and trace).
                // not first disable——delete is a whole entry removal, no enabled guard, atomic enough.
                let rolled_back = self.delete_credential(resp.credential_id).is_ok();
                ImportItemResult {
                    status: ImportStatus::Failed,
                    credential_id: Some(resp.credential_id),
                    email: resp.email,
                    balance: None,
                    error: Some(msg),
                    rolled_back,
                }
            }
        }
    }

    /// Updates the editable fields of the credential (email,proxy etc.)
    pub fn update_credential(
        &self,
        id: u64,
        req: UpdateCredentialRequest,
    ) -> Result<(), AdminServiceError> {
        self.token_manager
            .update_credential(
                id,
                req.email.map(|v| if v.is_empty() { None } else { Some(v) }),
                req.proxy_url
                    .map(|v| if v.is_empty() { None } else { Some(v) }),
                req.proxy_username
                    .map(|v| if v.is_empty() { None } else { Some(v) }),
                req.proxy_password
                    .map(|v| if v.is_empty() { None } else { Some(v) }),
                req.groups,
                req.source_channel
                    .map(|v| if v.is_empty() { None } else { Some(v) }),
            )
            .map_err(|e| self.classify_error(e, id))
    }

    /// deletecredential
    pub fn delete_credential(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .delete_credential(id)
            .map_err(|e| self.classify_delete_error(e, id))?;

        // Cleans the balance cache of deleted credentials.
        {
            let mut cache = self.balance_cache.lock();
            cache.remove(&id);
        }
        self.save_balance_cache();

        if let Some(trace_store) = &self.trace_store {
            trace_store.delete_for_credential(id);
        }

        Ok(())
    }

    /// Loads the latest config from disk, applies the update, then writes back to disk.
    ///
    /// Reads the latest file each time before writing, avoiding fields overwriting each other across calls.
    fn update_config_file(&self, updater: impl FnOnce(&mut Config)) {
        let base = self.token_manager.config();
        let Some(path) = base.config_path() else {
            return;
        };
        match Config::load(path) {
            Ok(mut fresh) => {
                updater(&mut fresh);
                if let Err(e) = fresh.save() {
                    tracing::warn!("failed to save the configuration file: {}", e);
                }
            }
            Err(e) => tracing::warn!("Reading the config file failed (skips persistence).: {}", e),
        }
    }

    /// get the global proxy URL
    pub fn get_global_proxy(&self) -> Option<String> {
        self.token_manager.proxy().map(|p| p.url.clone())
    }

    /// set the global proxy URL(None means clear) and persists to the config file.
    pub fn set_global_proxy(&self, url: Option<String>) -> Result<(), AdminServiceError> {
        if let Some(ref u) = url {
            let valid_prefix = u.starts_with("http://")
                || u.starts_with("https://")
                || u.starts_with("socks5://")
                || u.starts_with("socks4://");
            if !valid_prefix {
                return Err(AdminServiceError::InvalidCredential(
                    "proxy URL the format is invalid, needs to http://,https://,socks5:// or socks4:// start"
                        .to_string(),
                ));
            }
        }

        let proxy = url.as_deref().map(ProxyConfig::new);
        self.token_manager.set_global_proxy(proxy);

        // load the latest from disk config then writes, avoiding overwriting concurrent changes to other fields.
        let url_for_save = url;
        self.update_config_file(move |c| c.proxy_url = url_for_save);
        Ok(())
    }

    /// persist the new loginAPIkey (adminApiKey) to the config file (the in-memory key by handler the layer is responsible for updating)
    pub fn persist_admin_key(&self, new_key: &str) {
        let key = new_key.to_string();
        self.update_config_file(move |c| c.admin_api_key = Some(key));
    }

    /// persistnewof apiKey(sync after system key rotation config.json, ensuring no duplicate import on the next startup)
    pub fn persist_api_key(&self, new_key: &str) {
        let key = new_key.to_string();
        self.update_config_file(move |c| c.api_key = Some(key));
    }

    /// get the online update configuration (GitHub Token only return whether it is configured)
    pub fn get_update_config(&self) -> UpdateConfigResponse {
        self.update_config.lock().response()
    }

    /// update the online update configuration.
    pub fn set_update_config(
        &self,
        req: SetUpdateConfigRequest,
    ) -> Result<UpdateConfigResponse, AdminServiceError> {
        // Validates the time format before writing to runtime, and normalizes it to zero padded two digit HH:MM
        let normalized_time = match req.auto_apply_time.as_deref() {
            Some(value) => Some(normalize_auto_apply_time(value)?),
            None => None,
        };

        // GitHub Token: an empty string means clear,None means keep the original value
        let token_update: Option<Option<String>> = req.github_token.as_ref().map(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        {
            let mut runtime = self.update_config.lock();
            if let Some(auto_apply) = req.auto_apply {
                runtime.auto_apply = auto_apply;
            }
            if let Some(time) = &normalized_time {
                runtime.auto_apply_time = time.clone();
            }
            if let Some(token) = &token_update {
                runtime.github_token = token.clone();
            }
        }

        self.update_config_file(move |c| {
            if let Some(auto_apply) = req.auto_apply {
                c.update_auto_apply = auto_apply;
            }
            if let Some(time) = normalized_time {
                c.update_auto_apply_time = time;
            }
            if let Some(token) = token_update {
                c.github_token = token;
            }
        });

        Ok(self.get_update_config())
    }

    /// Downloads the new binary and verifies it via checksum (corresponds to the frontend pull image button).
    /// Does not replace the current executable, so the user can confirm a successful download before applying.
    /// save the download artifact to `<exe>.staged-<version>`, next time apply reuse when hitting the same version.
    pub async fn pull_update_image(&self) -> Result<ImageUpdateResponse, AdminServiceError> {
        let (proxy, token) = {
            let runtime = self.update_config.lock();
            (
                self.token_manager.proxy().map(|p| p.url.clone()),
                runtime.github_token.clone(),
            )
        };
        let exe = super::binary_update::current_executable()?;

        let version = self.resolve_target_version(false).await?;
        let staged = staged_binary_path(&exe, &version);

        // When the same version was already downloaded, reuses it directly to avoid duplicate network requests.
        let reused = staged.exists();
        if !reused {
            super::binary_update::download_release_binary(
                &version,
                proxy.as_deref(),
                token.as_deref(),
                &staged,
            )
            .await?;
        }
        // clean up other versions of the old staged file, to avoid occupying disk
        cleanup_other_staged(&exe, &version);

        Ok(ImageUpdateResponse {
            success: true,
            message: if reused {
                format!("v{} Downloaded and verified; the update and restart can be run directly.", version)
            } else {
                format!(
                    "downloaded and validated v{} binary; the update and restart can be run directly.",
                    version
                )
            },
            output: Some(format!(
                "{}: v{}\nstaged: {}",
                if reused { "reused" } else { "downloaded" },
                version,
                staged.display()
            )),
            applied: false,
            need_restart: false,
        })
    }

    /// Downloads the new binary and replaces the current executable, then lets the process exit so that
    /// `restart: unless-stopped` takes over the restart (corresponds to the frontend update and restart button).
    /// if pull has already downloaded the target version to `<exe>.staged-<version>`, skip the duplicate download.
    pub async fn apply_image_update(&self) -> Result<ImageUpdateResponse, AdminServiceError> {
        let (proxy, token) = {
            let runtime = self.update_config.lock();
            (
                self.token_manager.proxy().map(|p| p.url.clone()),
                runtime.github_token.clone(),
            )
        };
        let exe = super::binary_update::current_executable()?;

        let version = self.resolve_target_version(true).await?;
        let staged = staged_binary_path(&exe, &version);

        let reused = staged.exists();
        if !reused {
            super::binary_update::download_release_binary(
                &version,
                proxy.as_deref(),
                token.as_deref(),
                &staged,
            )
            .await?;
        }
        cleanup_other_staged(&exe, &version);

        // Records the current version as the previous version, for the frontend to show the rollback button.
        let previous_version = env!("CARGO_PKG_VERSION").to_string();
        super::binary_update::install_binary(&exe, &staged)?;

        let prev_label = format!("v{}", previous_version);
        let applied_at = chrono::Utc::now().to_rfc3339();
        {
            let mut runtime = self.update_config.lock();
            runtime.previous_version = Some(prev_label.clone());
            runtime.last_applied_at = Some(applied_at.clone());
        }
        let prev_to_persist = prev_label.clone();
        let applied_at_to_persist = applied_at.clone();
        self.update_config_file(move |c| {
            c.update_previous_version = Some(prev_to_persist);
            c.update_last_applied_at = Some(applied_at_to_persist);
        });

        super::binary_update::schedule_self_exit(std::time::Duration::from_secs(2));

        Ok(ImageUpdateResponse {
            success: true,
            message: format!(
                "alreadyreplace with v{},enterprocesswillin 2 exits after some seconds, taken over by the container restart policy.",
                version
            ),
            output: Some(format!(
                "previous: v{}\n{}: v{}",
                previous_version,
                if reused { "reused-staged" } else { "installed" },
                version
            )),
            applied: true,
            need_restart: true,
        })
    }

    /// roll back the executable file to `<exe>.backup`, then restart the process.
    pub async fn rollback_image_update(&self) -> Result<ImageUpdateResponse, AdminServiceError> {
        let previous_label = self
            .update_config
            .lock()
            .previous_version
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                AdminServiceError::InvalidCredential(
                    "No rollback version has been recorded yet; please perform an online update first.".to_string(),
                )
            })?
            .to_string();

        let exe = super::binary_update::current_executable()?;
        super::binary_update::restore_backup(&exe)?;
        // clear all after rollback staged: the user has expressed a position"the last update was wrong", the residue will only mislead
        cleanup_other_staged(&exe, "");

        // Rollback is treated as undoing the last update: clears previous_version and last_applied_at
        {
            let mut runtime = self.update_config.lock();
            runtime.previous_version = None;
            runtime.last_applied_at = None;
        }
        self.update_config_file(|c| {
            c.update_previous_version = None;
            c.update_last_applied_at = None;
        });

        super::binary_update::schedule_self_exit(std::time::Duration::from_secs(2));

        Ok(ImageUpdateResponse {
            success: true,
            message: format!(
                "alreadyfallbackto {},enterprocesswillin 2 exits after some seconds, taken over by the container restart policy.",
                previous_label
            ),
            output: Some(format!("rolled back to: {}", previous_label)),
            applied: true,
            need_restart: true,
        })
    }

    /// return GitHub Releases The latest available version number on it (no `v` prefix).
    /// failedreturn when `InternalError`, the caller should return it directly to the frontend.
    /// return GitHub Releases The latest available version number on it (no `v` prefix).
    /// failedreturn when `InternalError`, the caller should return it directly to the frontend.
    ///
    /// `require_update` as true when the current version is already the latest (no update available),
    /// Returns an error directly instead of returning the same version number.——avoid apply flow downloads and replaces the same version.
    async fn resolve_target_version(
        &self,
        require_update: bool,
    ) -> Result<String, AdminServiceError> {
        let info = self.check_update(true).await;
        if let Some(warn) = info.warning {
            return Err(AdminServiceError::InternalError(warn));
        }
        if info.latest_version.is_empty() {
            return Err(AdminServiceError::InternalError(
                "Cannot parse the latest version number (GitHub Releases returnempty)".to_string(),
            ));
        }
        if require_update && !info.has_update {
            return Err(AdminServiceError::InvalidCredential(format!(
                "already the latest version v{}, no needupdate",
                info.current_version
            )));
        }
        Ok(info.latest_version)
    }

    /// check GitHub Releases whether a new version exists on.
    ///
    /// `force=false` whenpriorityreturn 30 cached result within minutes;`force=true` whenforcequery
    /// remote. When the query fails but an old cache exists, returns the old cache along with warning.
    pub async fn check_update(&self, force: bool) -> UpdateCheckInfo {
        if !force {
            if let Some(cached) = self.update_check_cache.lock().clone() {
                let age = Utc::now()
                    .signed_duration_since(cached.cached_at)
                    .num_seconds();
                if age < UPDATE_CHECK_TTL_SECS {
                    let mut info = cached.info.clone();
                    info.cached = true;
                    return info;
                }
            }
        }

        match self.fetch_latest_release().await {
            Ok(info) => {
                self.update_check_cache.lock().replace(CachedUpdateCheck {
                    cached_at: Utc::now(),
                    info: info.clone(),
                });
                info
            }
            Err(err) => {
                let warning = format!("failed to check for updates:{}", err);
                if let Some(cached) = self.update_check_cache.lock().clone() {
                    let mut info = cached.info.clone();
                    info.cached = true;
                    info.warning = Some(warning);
                    return info;
                }
                UpdateCheckInfo {
                    current_version: env!("CARGO_PKG_VERSION").to_string(),
                    latest_version: String::new(),
                    has_update: false,
                    build_type: BUILD_TYPE.to_string(),
                    release_name: None,
                    release_notes: None,
                    release_url: None,
                    published_at: None,
                    checked_at: Utc::now().to_rfc3339(),
                    cached: false,
                    warning: Some(warning),
                }
            }
        }
    }

    async fn fetch_latest_release(&self) -> Result<UpdateCheckInfo, AdminServiceError> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            GITHUB_RELEASES_REPO
        );
        let token = self.update_config.lock().github_token.clone();
        let mut req = reqwest::Client::new()
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "kiro-rs-update-checker")
            .timeout(std::time::Duration::from_secs(15));
        if let Some(t) = token.as_deref() {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", trimmed));
            }
        }
        let resp = req.send().await.map_err(|e| {
            AdminServiceError::InternalError(format!("request GitHub API failed: {}", e))
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AdminServiceError::InternalError(format!(
                "GitHub API return {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        let release: GitHubRelease = resp.json().await.map_err(|e| {
            AdminServiceError::InternalError(format!("parse GitHub release failed: {}", e))
        })?;

        let current = env!("CARGO_PKG_VERSION").to_string();
        let latest_version = release.tag_name.trim().trim_start_matches('v').to_string();
        let has_update =
            !latest_version.is_empty() && compare_semver(&current, &latest_version).is_lt();

        Ok(UpdateCheckInfo {
            current_version: current,
            latest_version,
            has_update,
            build_type: BUILD_TYPE.to_string(),
            release_name: Some(release.name).filter(|v| !v.is_empty()),
            release_notes: Some(release.body).filter(|v| !v.is_empty()),
            release_url: Some(release.html_url).filter(|v| !v.is_empty()),
            published_at: Some(release.published_at).filter(|v| !v.is_empty()),
            checked_at: Utc::now().to_rfc3339(),
            cached: false,
            warning: None,
        })
    }

    /// query GitHub API the current throttle quota.
    ///
    /// `req.github_token` use it when not empty token verify(used for"try it before saving"),
    /// Otherwise uses the one saved in config. `config.github_token`, if still missing then anonymous query.
    /// `/rate_limit` The endpoint itself consumes no quota.
    pub async fn check_rate_limit(
        &self,
        req: CheckRateLimitRequest,
    ) -> GitHubRateLimitInfo {
        // prefer useinput parameter token; an empty string is treated as"attempt anonymous"; default falls back to the saved token
        let token = req
            .github_token
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .or_else(|| {
                self.update_config
                    .lock()
                    .github_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            });
        let authenticated = token.is_some();

        let proxy = self.token_manager.proxy().map(|p| p.url.clone());
        let client = match super::binary_update::build_http_client(proxy.as_deref()) {
            Ok(c) => c,
            Err(e) => {
                return GitHubRateLimitInfo {
                    valid: false,
                    authenticated,
                    limit: 0,
                    remaining: 0,
                    used: 0,
                    reset: 0,
                    login: None,
                    warning: Some(format!("construct HTTP clientfailed: {}", e)),
                };
            }
        };

        let mut req_builder = client
            .get("https://api.github.com/rate_limit")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "kiro-rs-update-checker")
            .timeout(std::time::Duration::from_secs(10));
        if let Some(t) = token.as_deref() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", t));
        }

        let resp = match req_builder.send().await {
            Ok(r) => r,
            Err(e) => {
                return GitHubRateLimitInfo {
                    valid: false,
                    authenticated,
                    limit: 0,
                    remaining: 0,
                    used: 0,
                    reset: 0,
                    login: None,
                    warning: Some(format!("request GitHub API failed: {}", e)),
                };
            }
        };

        let status = resp.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return GitHubRateLimitInfo {
                valid: false,
                authenticated,
                limit: 0,
                remaining: 0,
                used: 0,
                reset: 0,
                login: None,
                warning: Some("GitHub Token invalid or expired".to_string()),
            };
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return GitHubRateLimitInfo {
                valid: false,
                authenticated,
                limit: 0,
                remaining: 0,
                used: 0,
                reset: 0,
                login: None,
                warning: Some(format!(
                    "GitHub API return {}: {}",
                    status,
                    body.chars().take(200).collect::<String>()
                )),
            };
        }

        let payload: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                return GitHubRateLimitInfo {
                    valid: false,
                    authenticated,
                    limit: 0,
                    remaining: 0,
                    used: 0,
                    reset: 0,
                    login: None,
                    warning: Some(format!("parse GitHub responsefailed: {}", e)),
                };
            }
        };

        // /rate_limit returnstruct:{ resources: { core: { limit, remaining, used, reset } }, rate: {...} }
        // where `core` is REST API The overall quota, closest to the actual consumption of an online update.
        let core = payload
            .get("resources")
            .and_then(|r| r.get("core"))
            .or_else(|| payload.get("rate"));
        let limit = core.and_then(|c| c.get("limit")).and_then(|v| v.as_u64()).unwrap_or(0);
        let remaining = core
            .and_then(|c| c.get("remaining"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let used = core.and_then(|c| c.get("used")).and_then(|v| v.as_u64()).unwrap_or(0);
        let reset = core.and_then(|c| c.get("reset")).and_then(|v| v.as_u64()).unwrap_or(0);

        // samewhentry to acquire token The corresponding username; failure does not affect the main result.
        let login = if authenticated {
            self.fetch_github_login(&client, token.as_deref()).await
        } else {
            None
        };

        GitHubRateLimitInfo {
            valid: true,
            authenticated,
            limit,
            remaining,
            used,
            reset,
            login,
            warning: None,
        }
    }

    async fn fetch_github_login(
        &self,
        client: &reqwest::Client,
        token: Option<&str>,
    ) -> Option<String> {
        let mut req = client
            .get("https://api.github.com/user")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "kiro-rs-update-checker")
            .timeout(std::time::Duration::from_secs(10));
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {}", t));
        }
        let resp = req.send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let payload: serde_json::Value = resp.json().await.ok()?;
        payload
            .get("login")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// get the load balancing mode
    pub fn get_load_balancing_mode(&self) -> LoadBalancingModeResponse {
        LoadBalancingModeResponse {
            mode: self.token_manager.get_load_balancing_mode(),
        }
    }

    /// set the load balancing mode
    pub fn set_load_balancing_mode(
        &self,
        req: SetLoadBalancingModeRequest,
    ) -> Result<LoadBalancingModeResponse, AdminServiceError> {
        // verifymodevalue
        if req.mode != "priority" && req.mode != "balanced" {
            return Err(AdminServiceError::InvalidCredential(
                "mode must be 'priority' or 'balanced'".to_string(),
            ));
        }

        self.token_manager
            .set_load_balancing_mode(req.mode.clone())
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        Ok(LoadBalancingModeResponse { mode: req.mode })
    }

    /// Gets the account level throttle failover config.
    pub fn get_account_throttle_config(&self) -> AccountThrottleConfigResponse {
        AccountThrottleConfigResponse {
            failover: self.token_manager.get_account_throttle_failover(),
            cooldown_secs: self.token_manager.get_account_throttle_cooldown_secs(),
        }
    }

    /// Updates the account level throttle failover config.
    pub fn set_account_throttle_config(
        &self,
        req: SetAccountThrottleConfigRequest,
    ) -> Result<AccountThrottleConfigResponse, AdminServiceError> {
        if req.failover.is_none() && req.cooldown_secs.is_none() {
            return Err(AdminServiceError::InvalidCredential(
                "at leastprovide failover or cooldownSecs afield".to_string(),
            ));
        }

        self.token_manager
            .set_account_throttle_config(req.failover, req.cooldown_secs)
            .map_err(|e| AdminServiceError::InvalidCredential(e.to_string()))?;

        Ok(self.get_account_throttle_config())
    }

    /// read the log governance configuration (trace switch / trace retaindaycount / usage retaindaycount)
    pub fn get_log_governance_config(&self) -> LogGovernanceConfigResponse {
        let cfg = self.token_manager.config();
        LogGovernanceConfigResponse {
            trace_enabled: self
                .trace_store
                .as_ref()
                .map(|s| s.is_enabled())
                .unwrap_or(cfg.trace_enabled),
            trace_retention_days: self
                .trace_store
                .as_ref()
                .map(|s| s.retention_days() as u32)
                .unwrap_or(cfg.trace_retention_days),
            usage_log_retention_days: self
                .usage_recorder
                .as_ref()
                .map(|r| r.retention_days() as u32)
                .unwrap_or(cfg.usage_log_retention_days),
        }
    }

    /// Updates the log governance config: changes the runtime atomic value. + persistto config.json.
    /// An omitted field means no change.
    pub fn set_log_governance_config(
        &self,
        req: SetLogGovernanceConfigRequest,
    ) -> Result<LogGovernanceConfigResponse, AdminServiceError> {
        if req.trace_enabled.is_none()
            && req.trace_retention_days.is_none()
            && req.usage_log_retention_days.is_none()
        {
            return Err(AdminServiceError::InvalidCredential(
                "at leastprovide traceEnabled / traceRetentionDays / usageLogRetentionDays afield"
                    .to_string(),
            ));
        }
        // validation range: retention days 1..=365
        for (name, v) in [
            ("traceRetentionDays", req.trace_retention_days),
            ("usageLogRetentionDays", req.usage_log_retention_days),
        ] {
            if let Some(d) = v {
                if !(1..=365).contains(&d) {
                    return Err(AdminServiceError::InvalidCredential(format!(
                        "{} must be in 1..=365 inside: {}",
                        name, d
                    )));
                }
            }
        }

        // first change the runtime atomic value
        if let Some(enabled) = req.trace_enabled {
            if let Some(s) = &self.trace_store {
                s.set_enabled(enabled);
            }
        }
        if let Some(days) = req.trace_retention_days {
            if let Some(s) = &self.trace_store {
                s.set_retention_days(days);
            }
        }
        if let Some(days) = req.usage_log_retention_days {
            if let Some(r) = &self.usage_recorder {
                r.set_retention_days(days as i64);
            }
        }

        // persistto config.json
        if let Err(e) = self.persist_log_governance_config(&req) {
            tracing::warn!("Persisting the log governance config failed (already effective at runtime).: {}", e);
        }

        Ok(self.get_log_governance_config())
    }

    fn persist_log_governance_config(
        &self,
        req: &SetLogGovernanceConfigRequest,
    ) -> anyhow::Result<()> {
        use anyhow::Context;
        let config_path = match self.token_manager.config().config_path() {
            Some(p) => p.to_path_buf(),
            None => {
                tracing::warn!("The config file path is unknown; log governance config takes effect only in the current process.");
                return Ok(());
            }
        };
        let mut config = crate::model::config::Config::load(&config_path)
            .with_context(|| format!("failed to reload the configuration: {}", config_path.display()))?;
        if let Some(v) = req.trace_enabled {
            config.trace_enabled = v;
        }
        if let Some(v) = req.trace_retention_days {
            config.trace_retention_days = v;
        }
        if let Some(v) = req.usage_log_retention_days {
            config.usage_log_retention_days = v;
        }
        config
            .save()
            .with_context(|| format!("Persisting the log governance config failed.: {}", config_path.display()))?;
        Ok(())
    }

    /// update the specified credential refreshToken(disabled credentials only)
    pub fn update_refresh_token(
        &self,
        id: u64,
        req: UpdateRefreshTokenRequest,
    ) -> Result<(), AdminServiceError> {
        self.token_manager
            .update_refresh_token(id, req.refresh_token, req.access_token, req.expires_at)
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("does not exist") {
                    AdminServiceError::NotFound { id }
                } else if msg.contains("can only be disabled")
                    || msg.contains("refreshToken duplicate")
                    || msg.contains("alreadybytruncate")
                    || msg.contains("refreshToken is empty")
                    || msg.contains("missing refreshToken")
                {
                    AdminServiceError::InvalidCredential(msg)
                } else {
                    AdminServiceError::InternalError(msg)
                }
            })
    }

    /// one click enable all"Overage can be enabled and is currently not enabled."credentialofoverage
    /// datasourceis balance_cache(5 minutes valid); if the cache is missing or capable If the state is unknown, optimistically tries,
    /// by upstream setUserPreference The interface itself decides whether it succeeds (an unsupported subscription returns 4xx failed).
    pub async fn enable_overage_for_all_capable(&self) -> EnableOverageAllResult {
        let snapshot = self.token_manager.snapshot();
        let cache_snapshot: HashMap<u64, CachedBalance> = {
            let cache = self.balance_cache.lock();
            cache.clone()
        };
        let now_ts = Utc::now().timestamp() as f64;

        // select those that need operation ID list
        let mut targets: Vec<u64> = Vec::new();
        let mut skipped: Vec<u64> = Vec::new();
        for entry in snapshot.entries.iter() {
            if entry.disabled {
                skipped.push(entry.id);
                continue;
            }
            let cached = cache_snapshot.get(&entry.id).filter(|c| {
                (now_ts - c.cached_at) < BALANCE_CACHE_TTL_SECS as f64
            });

            match cached {
                // Cache hit: explicitly cannot be enabled, skipped.
                Some(c) if c.data.overage_capable == Some(false) => {
                    skipped.push(entry.id);
                    continue;
                }
                // Cache hit: explicitly already enabled, skipped.
                Some(c) if c.data.overage_enabled == Some(true) => {
                    skipped.push(entry.id);
                    continue;
                }
                // others (cache miss / stateunknown / clearly can enable but not enabled)— optimistic attempt
                _ => targets.push(entry.id),
            }
        }

        let mut enabled_ids: Vec<u64> = Vec::new();
        let mut failed_ids: Vec<u64> = Vec::new();
        let mut failure_messages: Vec<String> = Vec::new();

        for id in targets {
            match self.token_manager.set_user_preference_for(id, "ENABLED").await {
                Ok(()) => {
                    enabled_ids.push(id);
                    // invalidate the local cache
                    let mut cache = self.balance_cache.lock();
                    cache.remove(&id);
                }
                Err(e) => {
                    tracing::warn!("one click enable overage: credentials #{} failed: {}", id, e);
                    failed_ids.push(id);
                    failure_messages.push(e.to_string());
                }
            }
            // throttle
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        if !enabled_ids.is_empty() {
            self.save_balance_cache();
        }

        EnableOverageAllResult {
            enabled_ids,
            skipped_ids: skipped,
            failed_ids,
            failure_messages,
        }
    }

    /// force refresh the specified credential Token
    pub async fn force_refresh_token(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .force_refresh_token_for(id)
            .await
            .map_err(|e| self.classify_balance_error(e, id))
    }

    /// setcredentialof"overage"switch (ENABLED / DISABLED)
    /// On success it actively invalidates the local balance cache so the next list refresh shows the latest. overage state
    pub async fn set_overage(&self, id: u64, enabled: bool) -> Result<(), AdminServiceError> {
        let status = if enabled { "ENABLED" } else { "DISABLED" };
        self.token_manager
            .set_user_preference_for(id, status)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;

        // let the local cache overage The state is invalidated (re-pulled on the next refresh).
        {
            let mut cache = self.balance_cache.lock();
            cache.remove(&id);
        }
        self.save_balance_cache();

        // Asynchronously triggers a new balance query (does not block the response).
        let svc_handle = self.token_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = svc_handle.get_usage_limits_for(id).await {
                tracing::warn!("Warming the balance failed after the overage state changed. #{}: {}", id, e);
            }
        });

        Ok(())
    }

    // ============ balance cache persistence ============

    fn load_balance_cache_from(cache_path: &Option<PathBuf>) -> HashMap<u64, CachedBalance> {
        let path = match cache_path {
            Some(p) => p,
            None => return HashMap::new(),
        };

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        // use a string in the file key for compat JSON format
        let map: HashMap<String, CachedBalance> = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Parsing the balance cache failed; will ignore it.: {}", e);
                return HashMap::new();
            }
        };

        let now = Utc::now().timestamp() as f64;
        map.into_iter()
            .filter_map(|(k, v)| {
                let id = k.parse::<u64>().ok()?;
                // discardexceeds TTL entry
                if (now - v.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    Some((id, v))
                } else {
                    None
                }
            })
            .collect()
    }

    fn save_balance_cache(&self) {
        let path = match &self.cache_path {
            Some(p) => p,
            None => return,
        };

        // Completes serialization and writing while holding the lock, preventing concurrent corruption.
        let cache = self.balance_cache.lock();
        let map: HashMap<String, &CachedBalance> =
            cache.iter().map(|(k, v)| (k.to_string(), v)).collect();

        match serde_json::to_string_pretty(&map) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    tracing::warn!("failed to save the balance cache: {}", e);
                }
            }
            Err(e) => tracing::warn!("failed to serialize the balance cache: {}", e),
        }
    }

    // ============ proxypool management ============

    /// Gets the proxy pool list (including credential reference counts).
    pub fn get_proxy_pool(&self) -> ProxyPoolResponse {
        let proxies = self.proxy_pool.list();
        let credentials = {
            let snapshot = self.token_manager.snapshot();
            snapshot.entries
        };

        let pool: Vec<ProxyPoolEntry> = proxies
            .into_iter()
            .map(|p| {
                let count = credentials
                    .iter()
                    .filter(|c| c.proxy_url.as_deref().map(|u| u == p.url).unwrap_or(false))
                    .count() as u32;
                ProxyPoolEntry {
                    id: p.id,
                    url: p.url,
                    label: p.label,
                    enabled: p.enabled,
                    credential_count: count,
                    health: p.health,
                    latency_ms: p.latency_ms,
                    last_checked_at: p.last_checked_at,
                    consecutive_failures: p.consecutive_failures,
                    auto_disabled: p.auto_disabled,
                }
            })
            .collect();

        ProxyPoolResponse {
            total: pool.len(),
            proxies: pool,
        }
    }

    /// add a proxy to the pool
    pub fn add_proxy(
        &self,
        url: String,
        label: Option<String>,
    ) -> Result<ProxyPoolEntry, AdminServiceError> {
        let entry = self
            .proxy_pool
            .add(url, label)
            .map_err(|e| AdminServiceError::InvalidCredential(e.to_string()))?;
        Ok(ProxyPoolEntry {
            id: entry.id,
            url: entry.url,
            label: entry.label,
            enabled: entry.enabled,
            credential_count: 0,
            health: entry.health,
            latency_ms: entry.latency_ms,
            last_checked_at: entry.last_checked_at,
            consecutive_failures: entry.consecutive_failures,
            auto_disabled: entry.auto_disabled,
        })
    }

    /// batch add proxies
    pub fn batch_add_proxies(
        &self,
        req: BatchAddProxyRequest,
    ) -> (Vec<ProxyPoolEntry>, Vec<String>) {
        let (added, errors) = self.proxy_pool.batch_add(req.urls);
        let result = added
            .into_iter()
            .map(|e| ProxyPoolEntry {
                id: e.id,
                url: e.url,
                label: e.label,
                enabled: e.enabled,
                credential_count: 0,
                health: e.health,
                latency_ms: e.latency_ms,
                last_checked_at: e.last_checked_at,
                consecutive_failures: e.consecutive_failures,
                auto_disabled: e.auto_disabled,
            })
            .collect();
        (result, errors)
    }

    /// delete a proxy from the proxy pool
    pub fn delete_proxy(&self, id: u64) -> Result<(), AdminServiceError> {
        self.proxy_pool.delete(id).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("does not exist") {
                AdminServiceError::NotFound { id }
            } else {
                AdminServiceError::InternalError(msg)
            }
        })
    }

    /// set the proxy enabled/disablestate
    pub fn set_proxy_enabled(&self, id: u64, enabled: bool) -> Result<(), AdminServiceError> {
        self.proxy_pool
            .set_enabled(id, enabled)
            .map_err(|_| AdminServiceError::NotFound { id })
    }

    /// Allocates a proxy from the pool to the given credential.
    pub fn assign_proxy_to_credential(
        &self,
        credential_id: u64,
        req: AssignProxyRequest,
    ) -> Result<(), AdminServiceError> {
        let proxy_url = match req.proxy_id {
            Some(proxy_id) => {
                let url = match self.proxy_pool.get_url(proxy_id) {
                    GetUrlResult::Ok(url) => url,
                    GetUrlResult::NotFound => {
                        return Err(AdminServiceError::NotFound { id: proxy_id });
                    }
                    GetUrlResult::Disabled => {
                        return Err(AdminServiceError::InvalidCredential(format!(
                            "proxy #{} It is disabled; please enable it before allocating.",
                            proxy_id
                        )));
                    }
                };
                Some(url)
            }
            None => None, // clearproxy
        };

        self.token_manager
            .update_credential(
                credential_id,
                None,            // email do not modify
                Some(proxy_url), // setorclear proxy_url(Some(None) = clear,Some(Some(url)) = set)
                None,            // proxy_username do not modify
                None,            // proxy_password do not modify
                None,            // groups do not modify
                None,            // source_channel do not modify
            )
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("does not exist") {
                    AdminServiceError::NotFound { id: credential_id }
                } else {
                    AdminServiceError::InternalError(msg)
                }
            })
    }

    /// Instantly probes a single proxy connectivity (for UIcalled by the test button)
    pub async fn check_proxy(&self, id: u64) -> Result<ProxyCheckResponse, AdminServiceError> {
        let entry = self
            .proxy_pool
            .check_one(id)
            .await
            .map_err(|_| AdminServiceError::NotFound { id })?;
        Ok(ProxyCheckResponse {
            id: entry.id,
            health: entry.health,
            latency_ms: entry.latency_ms,
            last_checked_at: entry.last_checked_at,
            enabled: entry.enabled,
            auto_disabled: entry.auto_disabled,
        })
    }

    /// Triggers the health check of all proxies.
    pub async fn check_all_proxies(&self) -> ProxyCheckAllResponse {
        let summary = self.proxy_pool.check_all().await;
        ProxyCheckAllResponse {
            healthy: summary.healthy,
            unhealthy: summary.unhealthy,
            auto_disabled: summary.auto_disabled,
        }
    }

    /// Takes the available proxies (enabled and not Unhealthy) batch allocates to credentials in round robin.
    ///
    /// - `credential_ids` as None assign to all credentials when
    /// - Returns an error when no proxy is available.
    pub fn assign_proxies_round_robin(
        &self,
        credential_ids: Option<Vec<u64>>,
    ) -> Result<AssignRoundRobinResponse, AdminServiceError> {
        let urls = self.proxy_pool.assignable_urls();
        if urls.is_empty() {
            return Err(AdminServiceError::InvalidCredential(
                "No available proxy (must be enabled and not failed the health check).".to_string(),
            ));
        }

        let target_ids: Vec<u64> = match credential_ids {
            Some(ids) if !ids.is_empty() => ids,
            _ => self
                .token_manager
                .snapshot()
                .entries
                .iter()
                .map(|c| c.id)
                .collect(),
        };

        let mut assigned = 0;
        for (i, cred_id) in target_ids.iter().enumerate() {
            let url = urls[i % urls.len()].clone();
            if self
                .token_manager
                .update_credential(*cred_id, None, Some(Some(url)), None, None, None, None)
                .is_ok()
            {
                assigned += 1;
            }
        }

        Ok(AssignRoundRobinResponse {
            assigned,
            proxy_count: urls.len(),
        })
    }

    // ============ errorclassify ============

    /// classify simple operation errors (set_disabled, set_priority, reset_and_enable)
    fn classify_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("does not exist") {
            AdminServiceError::NotFound { id }
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// Classifies balance query errors (may involve upstream). API call)
    fn classify_balance_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();

        // 1. credentialdoes not exist
        if msg.contains("does not exist") {
            return AdminServiceError::NotFound { id };
        }

        // 2. API Key The credential does not support refresh: a client request error, mapped to 400
        if msg.contains("API Key the credential does not support refresh") {
            return AdminServiceError::InvalidCredential(msg);
        }

        // 3. Upstream explicitly indicates the credential is missing or carries a wrong Profile ARN, belongs to an incomplete imported credential./invalid.
        if msg.contains("Invalid profileArn") {
            return AdminServiceError::InvalidCredential(
                "the credential is missing or contains invalid profileArn, cannot query the balance; please log in again to obtain it. profileArn, or import containing profileArn ofcompletecredential"
                    .to_string(),
            );
        }

        // 3. upstream service error characteristics:HTTP response error or network error
        let is_upstream_error = msg.contains("failed to get the usage quota") ||
            // HTTP response error (from refresh_*_token the error message)
            msg.contains("the credential has expired or is invalid") ||
            msg.contains("permissionnotenough") ||
            msg.contains("alreadybylimitstream") ||
            msg.contains("servicecomponenterror") ||
            msg.contains("Token refreshfailed") ||
            msg.contains("temporarilywhennotavailable") ||
            // network error(reqwest errorformat)
            msg.contains("error sending request") ||
            msg.contains("error trying to connect") ||
            msg.contains("connection") ||
            msg.contains("timeout") ||
            msg.contains("timed out") ||
            msg.contains("proxy") ||
            msg.contains("SOCKS") ||
            msg.contains("dns") ||
            msg.contains("DNS");

        if is_upstream_error {
            AdminServiceError::UpstreamError(msg)
        } else {
            // 4. Defaults to classifying as an internal error (local validation failure, config error, and so on).
            // including:missing refreshToken,refreshToken has been truncated, cannot generate machineId etc.
            AdminServiceError::InternalError(msg)
        }
    }

    /// classify add credential errors
    fn classify_add_error(&self, e: anyhow::Error) -> AdminServiceError {
        let msg = e.to_string();

        // credential validation failed (refreshToken invalid, format error, etc)
        let is_invalid_credential = msg.contains("missing refreshToken")
            || msg.contains("refreshToken is empty")
            || msg.contains("refreshToken alreadybytruncate")
            || msg.contains("credentialalready exists")
            || msg.contains("refreshToken duplicate")
            || msg.contains("kiroApiKey duplicate")
            || msg.contains("missing kiroApiKey")
            || msg.contains("kiroApiKey is empty")
            || msg.contains("the credential has expired or is invalid")
            || msg.contains("permissionnotenough")
            || msg.contains("alreadybylimitstream");

        if is_invalid_credential {
            AdminServiceError::InvalidCredential(msg)
        } else if msg.contains("error trying to connect")
            || msg.contains("connection")
            || msg.contains("timeout")
        {
            AdminServiceError::UpstreamError(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    // ── Social login (Portal PKCE OAuth)────────────────────────────────────────

    /// initiate Social login, return portal URL for the user to open in a browser
    ///
    /// callbackmodeby `config.callbackBaseUrl` decide:
    /// - configured → remotemode:redirect_uri Uses the public address, by this service `/auth/callback` GET the route receives the callback
    /// - not configured → local mode: start a temporary TCP Callback server (browser and server must be on the same machine).
    pub async fn start_social_login(
        &self,
        req: StartSocialLoginRequest,
    ) -> Result<StartSocialLoginResponse, AdminServiceError> {
        let global_proxy = self.token_manager.proxy();
        let proxy = req
            .proxy_url
            .as_deref()
            .map(ProxyConfig::new)
            .or(global_proxy);

        let auth_endpoint = req
            .auth_endpoint
            .unwrap_or_else(|| social::KIRO_AUTH_ENDPOINT.to_string());

        let (code_verifier, code_challenge) = social::generate_pkce();
        let state = uuid::Uuid::new_v4().to_string();

        // callback mode: configured callbackBaseUrl → Remote mode (the public callback route auto receives);
        // Otherwise local mode (starts a temporary TCP port, reachable only by the local browser).
        let remote_base = self.resolve_callback_base(req.callback_base_url.as_deref());
        let (redirect_uri, server_handle, remote_callback_tx, rx) = match remote_base.clone() {
            Some(base) => {
                let (tx, rx) = tokio::sync::oneshot::channel::<social::OAuthCallbackData>();
                // remote mode: stage Sender, bypublic network GET The callback route delivers the callback data.
                (
                    base,
                    None,
                    Some(Mutex::new(Some(tx))),
                    rx,
                )
            }
            None => {
                let (tx, rx) = tokio::sync::oneshot::channel::<social::OAuthCallbackData>();
                let (port, server_handle) = social::start_callback_server(tx)
                    .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;
                (
                    format!("http://127.0.0.1:{}", port),
                    Some(server_handle),
                    None,
                    rx,
                )
            }
        };
        let portal_url = social::build_portal_url(&state, &code_challenge, &redirect_uri);

        let expires_at = Utc::now() + Duration::minutes(10);
        let session_id = uuid::Uuid::new_v4().to_string();

        let cred_template = KiroCredentials {
            auth_method: Some("social".to_string()),
            priority: req.priority,
            email: req.email,
            proxy_url: req.proxy_url,
            ..Default::default()
        };

        let session = SocialAuthSession {
            auth_endpoint,
            state,
            code_verifier,
            redirect_uri,
            expires_at,
            callback_rx: tokio::sync::Mutex::new(rx),
            cred_template,
            proxy,
            _server_handle: server_handle,
            remote_callback_tx,
            relogin_target_id: None,
        };

        self.social_sessions
            .lock()
            .insert(session_id.clone(), session);

        Ok(StartSocialLoginResponse {
            session_id,
            portal_url,
            expires_at: expires_at.to_rfc3339(),
            remote: remote_base.is_some(),
        })
    }

    /// pollonce Social loginstate
    pub async fn poll_social_login(
        &self,
        session_id: &str,
    ) -> Result<PollIdcLoginResponse, AdminServiceError> {
        use tokio::sync::oneshot::error::TryRecvError;

        // Completes within a single lock: the expiry check + Non blocking callback reception, eliminating TOCTOU
        enum PollOutcome {
            Expired,
            Closed,
            Pending,
            Received(social::OAuthCallbackData),
        }

        let outcome = {
            let sessions = self.social_sessions.lock();
            let Some(session) = sessions.get(session_id) else {
                return Err(AdminServiceError::NotFound { id: 0 });
            };

            if Utc::now() >= session.expires_at {
                PollOutcome::Expired
            } else {
                match session.callback_rx.try_lock() {
                    Ok(mut rx) => match rx.try_recv() {
                        Ok(data) => PollOutcome::Received(data),
                        Err(TryRecvError::Empty) => PollOutcome::Pending,
                        Err(TryRecvError::Closed) => PollOutcome::Closed,
                    },
                    Err(_) => PollOutcome::Pending,
                }
            }
        };

        match outcome {
            PollOutcome::Pending => return Ok(PollIdcLoginResponse::Pending),
            PollOutcome::Expired => {
                self.social_sessions.lock().remove(session_id);
                return Ok(PollIdcLoginResponse::Expired);
            }
            PollOutcome::Closed => {
                self.social_sessions.lock().remove(session_id);
                return Err(AdminServiceError::InternalError(
                    "Social The login callback server has closed; please start login again.".to_string(),
                ));
            }
            PollOutcome::Received(callback) => {
                self.do_complete_social_login(session_id, callback).await
            }
        }
    }

    /// insidepart:doneinto Social login token Redemption and credential creation (shared by polling callback and manual completion).
    ///
    /// must confirm before calling session exists and is not expired. Does internally state CSRF validate.
    async fn do_complete_social_login(
        &self,
        session_id: &str,
        callback: social::OAuthCallbackData,
    ) -> Result<PollIdcLoginResponse, AdminServiceError> {
        // do first CSRF validate (do not remove session, keep when validation fails session cancontinuepoll)
        {
            let sessions = self.social_sessions.lock();
            let s = sessions
                .get(session_id)
                .ok_or(AdminServiceError::NotFound { id: 0 })?;
            if callback.state != s.state {
                tracing::warn!(
                    "Social login state mismatch(expected {}, received {}),alreadyreject",
                    s.state,
                    callback.state
                );
                return Err(AdminServiceError::InternalError(
                    "OAuth state does not match; please start login again.".to_string(),
                ));
            }
        }

        // remove session(including code_verifier etc.sensitivedata)
        let session = self
            .social_sessions
            .lock()
            .remove(session_id)
            .ok_or(AdminServiceError::NotFound { id: 0 })?;

        let config = self.token_manager.config();

        // buildcompleteof redirect_uri(with IDE consistent behavior)
        let full_redirect_uri = if callback.login_option.is_empty() {
            format!("{}{}", session.redirect_uri, callback.path)
        } else {
            format!(
                "{}{}?login_option={}",
                session.redirect_uri,
                callback.path,
                urlencoding::encode(&callback.login_option),
            )
        };

        let token = social::exchange_code_for_token(
            &session.auth_endpoint,
            &callback.code,
            &session.code_verifier,
            &full_redirect_uri,
            config,
            session.proxy.as_ref(),
        )
        .await
        .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        // Re-login mode: updates an existing credential rather than creating a new one.
        if let Some(target_id) = session.relogin_target_id {
            let refresh_token = token.refresh_token.ok_or_else(|| {
                AdminServiceError::InternalError(
                    "Social loginnot yetreturn refreshToken, cannot update the credential".to_string(),
                )
            })?;
            self.do_relogin_update(target_id, refresh_token)
                .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;
            tracing::info!("Social re login succeeded, the credential #{} Token updated", target_id);
            return Ok(PollIdcLoginResponse::Success {
                credential_id: target_id,
            });
        }

        let mut new_cred = session.cred_template;
        new_cred.access_token = Some(token.access_token);
        new_cred.refresh_token = token.refresh_token;
        new_cred.expires_at = token.expires_at.or_else(|| {
            token
                .expires_in
                .map(|secs| (Utc::now() + Duration::seconds(secs)).to_rfc3339())
        });
        if let Some(arn) = token.profile_arn {
            new_cred.profile_arn = Some(arn);
        }

        let credential_id = self
            .token_manager
            .add_credential(new_cred)
            .await
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        // Actively refreshes the balance (including subscription tier). / email) and writes to the cache, visible right after login.
        if let Err(e) = self.get_balance(credential_id).await {
            tracing::warn!("Social Refreshing the balance failed after login (does not affect login).: {}", e);
        }

        tracing::info!("Social Login succeeded; the credential has been added. #{}", credential_id);
        Ok(PollIdcLoginResponse::Success { credential_id })
    }

    /// manually finishedinto Social Login: the callback pasted from the browser address bar during remote access. URL extract parameters from, complete directly token redeem
    pub async fn complete_social_login(
        &self,
        session_id: &str,
        code: String,
        state: String,
        login_option: String,
        path: String,
    ) -> Result<PollIdcLoginResponse, AdminServiceError> {
        // expiredcheck
        {
            let sessions = self.social_sessions.lock();
            let s = sessions
                .get(session_id)
                .ok_or(AdminServiceError::NotFound { id: 0 })?;
            if Utc::now() >= s.expires_at {
                return Ok(PollIdcLoginResponse::Expired);
            }
        }

        let callback = social::OAuthCallbackData {
            code,
            login_option,
            path,
            state,
        };
        self.do_complete_social_login(session_id, callback).await
    }

    /// parseremotecallback base,priority:`config.callbackBaseUrl`(explicit override / escape hatch)> requestbuilt in base > None(local mode).
    ///
    /// return None means falling back to local mode (neither provided / when the provided value is illegal record warn).
    fn resolve_callback_base(&self, req_base: Option<&str>) -> Option<String> {
        // prefer use config explicitly configured; otherwise uses the request value the frontend derives from the current access address.
        let raw = self
            .token_manager
            .config()
            .callback_base_url
            .as_deref()
            .map(str::to_string)
            .or_else(|| req_base.map(str::to_string))?;
        let trimmed = raw.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            return None;
        }
        if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
            tracing::warn!(
                "callbackBaseUrl invalid(mustto http:// or https:// start), falls back to local callback mode.: {}",
                raw
            );
            return None;
        }
        Some(trimmed.to_string())
    }

    /// public network GET callback route call: by OAuth state Locates the session and delivers the callback data.
    ///
    /// hitandnot expired → deliverentersession oneshot channel(by poll_social_login unifydoneinto token redeem);
    /// does not exist / expired / nonremotesession → Returns the corresponding result; the caller renders the hint page.
    pub fn deliver_remote_social_callback(
        &self,
        state: &str,
        data: social::OAuthCallbackData,
    ) -> RemoteCallbackOutcome {
        let sessions = self.social_sessions.lock();
        // find state matchofsession(state random per session, provide CSRF protect)
        let session_id = sessions
            .iter()
            .find_map(|(id, s)| (s.state == state).then_some(id.clone()));

        let Some(session_id) = session_id else {
            return RemoteCallbackOutcome::NotFound;
        };
        let session = sessions.get(&session_id).expect("the session just found must exist");
        if Utc::now() >= session.expires_at {
            return RemoteCallbackOutcome::Expired;
        }
        let tx_slot = match session.remote_callback_tx.as_ref() {
            Some(slot) => slot,
            None => return RemoteCallbackOutcome::NotFound, // Local mode session: should not be delivered by the public route.
        };
        // release the outer lock before dispatching (send Does not block, but avoids sending while holding the lock.
        let tx = tx_slot.lock().take();
        drop(sessions);
        match tx {
            Some(tx) => {
                if tx.send(data).is_ok() {
                    RemoteCallbackOutcome::Delivered
                } else {
                    // The receiver has disappeared (the session was concurrently completed)./removed)→ viewasalreadyhandle
                    RemoteCallbackOutcome::AlreadyCompleted
                }
            }
            None => RemoteCallbackOutcome::AlreadyCompleted,
        }
    }

    /// classify delete credential errors
    fn classify_delete_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("does not exist") {
            AdminServiceError::NotFound { id }
        } else if msg.contains("can only delete disabled credentials") || msg.contains("please firstdisablecredential")
        {
            AdminServiceError::InvalidCredential(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    // ── IdC device authorization login ──────────────────────────────────────────────────────

    /// initiate IdC Device authorization, returns the verification code and URL
    pub async fn start_idc_login(
        &self,
        req: StartIdcLoginRequest,
    ) -> Result<StartIdcLoginResponse, AdminServiceError> {
        let config = self.token_manager.config();
        let global_proxy = self.token_manager.proxy();

        // Proxy: prefers the request level, otherwise falls back to global.
        let proxy = req
            .proxy_url
            .as_deref()
            .map(ProxyConfig::new)
            .or(global_proxy);

        let start_url = req.start_url.as_deref().unwrap_or(BUILDER_ID_START_URL);

        // 1. register OIDC client
        let reg = idc::register_client(&req.region, start_url, config, proxy.as_ref())
            .await
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        // 2. initiatedevice authorization
        let device = idc::start_device_authorization(
            &req.region,
            start_url,
            &reg.client_id,
            &reg.client_secret,
            config,
            proxy.as_ref(),
        )
        .await
        .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        let expires_at = Utc::now() + Duration::seconds(device.expires_in);
        let session_id = Uuid::new_v4().to_string();

        // identity provider: default Start URL as AWS Builder ID,fromdefine Start URL is enterprise IAM Identity Center
        let provider = if start_url == BUILDER_ID_START_URL {
            "BuilderId"
        } else {
            "Enterprise"
        };

        // Builds the credential template written after a successful login.
        let cred_template = KiroCredentials {
            auth_method: Some("idc".to_string()),
            provider: Some(provider.to_string()),
            client_id: Some(reg.client_id.clone()),
            client_secret: Some(reg.client_secret.clone()),
            start_url: Some(start_url.to_string()),
            region: Some(req.region.clone()),
            priority: req.priority,
            email: req.email,
            proxy_url: req.proxy_url,
            ..Default::default()
        };

        let session = IdcAuthSession {
            region: req.region,
            client_id: reg.client_id,
            client_secret: reg.client_secret,
            device_code: device.device_code,
            expires_at,
            poll_interval: device.interval.max(5),
            cred_template,
            proxy,
            relogin_target_id: None,
        };

        let poll_interval = session.poll_interval;
        self.idc_sessions.lock().insert(session_id.clone(), session);

        Ok(StartIdcLoginResponse {
            session_id,
            user_code: device.user_code,
            verification_uri: device.verification_uri,
            verification_uri_complete: device.verification_uri_complete,
            expires_at: expires_at.to_rfc3339(),
            poll_interval,
        })
    }

    /// pollonce IdC loginstate
    pub async fn poll_idc_login(
        &self,
        session_id: &str,
    ) -> Result<PollIdcLoginResponse, AdminServiceError> {
        let (
            region,
            client_id,
            client_secret,
            device_code,
            _expires_at,
            proxy,
            cred_template,
            relogin_target_id,
        ) = {
            let sessions = self.idc_sessions.lock();
            let s = sessions
                .get(session_id)
                .ok_or_else(|| AdminServiceError::NotFound { id: 0 })?;

            if Utc::now() >= s.expires_at {
                return Ok(PollIdcLoginResponse::Expired);
            }

            (
                s.region.clone(),
                s.client_id.clone(),
                s.client_secret.clone(),
                s.device_code.clone(),
                s.expires_at,
                s.proxy.clone(),
                s.cred_template.clone(),
                s.relogin_target_id,
            )
        };

        let config = self.token_manager.config();

        match idc::poll_token(
            &region,
            &client_id,
            &client_secret,
            &device_code,
            config,
            proxy.as_ref(),
        )
        .await
        {
            idc::PollResult::Pending => Ok(PollIdcLoginResponse::Pending),
            idc::PollResult::Expired => {
                self.idc_sessions.lock().remove(session_id);
                Ok(PollIdcLoginResponse::Expired)
            }
            idc::PollResult::Error(e) => Err(AdminServiceError::InternalError(e.to_string())),
            idc::PollResult::Success(token) => {
                self.idc_sessions.lock().remove(session_id);

                // Re-login mode: updates an existing credential rather than creating a new one.
                if let Some(target_id) = relogin_target_id {
                    if let Some(refresh_token) = token.refresh_token {
                        self.do_relogin_update(target_id, refresh_token)
                            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;
                    }
                    tracing::info!("IdC re login succeeded, the credential #{} Token updated", target_id);
                    return Ok(PollIdcLoginResponse::Success {
                        credential_id: target_id,
                    });
                }

                // writecredential
                let mut new_cred = cred_template;
                new_cred.access_token = Some(token.access_token);
                new_cred.refresh_token = token.refresh_token;
                if let Some(secs) = token.expires_in {
                    new_cred.expires_at = Some((Utc::now() + Duration::seconds(secs)).to_rfc3339());
                }

                let credential_id = self
                    .token_manager
                    .add_credential(new_cred)
                    .await
                    .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

                // Actively refreshes the balance (including subscription tier). / email) and writes to the cache, visible right after login.
                if let Err(e) = self.get_balance(credential_id).await {
                    tracing::warn!("IdC Refreshing the balance failed after login (does not affect login).: {}", e);
                }

                tracing::info!("IdC Device authorization login succeeded; the credential has been added. #{}", credential_id);
                Ok(PollIdcLoginResponse::Success { credential_id })
            }
        }
    }

    /// Internal: after re-login completes, updates the existing credential Token(disable→update→reset→enabled)
    fn do_relogin_update(&self, target_id: u64, refresh_token: String) -> anyhow::Result<()> {
        // firstdisable(update_refresh_token requires the credential to be in the disabled state)
        self.token_manager.set_disabled(target_id, true)?;
        // update refreshToken(samewhenclearempty accessToken and expiresAt, the system auto refreshes on next use)
        self.token_manager
            .update_refresh_token(target_id, refresh_token, None, None)?;
        // Resets the failure count and re-enables.
        self.token_manager.reset_and_enable(target_id)?;
        Ok(())
    }

    /// initiate Social Re-login (updates the existing credential Token rather than creating a new credential)
    pub async fn start_social_relogin(
        &self,
        target_id: u64,
        req: StartSocialLoginRequest,
    ) -> Result<StartSocialLoginResponse, AdminServiceError> {
        // validate the target credential exists
        {
            let snapshot = self.token_manager.snapshot();
            if !snapshot.entries.iter().any(|e| e.id == target_id) {
                return Err(AdminServiceError::NotFound { id: target_id });
            }
        }

        let global_proxy = self.token_manager.proxy();
        let proxy = req
            .proxy_url
            .as_deref()
            .map(ProxyConfig::new)
            .or(global_proxy);

        let auth_endpoint = req
            .auth_endpoint
            .unwrap_or_else(|| social::KIRO_AUTH_ENDPOINT.to_string());

        let (code_verifier, code_challenge) = social::generate_pkce();
        let state = uuid::Uuid::new_v4().to_string();

        // callbackmodesame start_social_login: remote mode uses the public callback route, local mode uses a temporary port.
        let remote_base = self.resolve_callback_base(req.callback_base_url.as_deref());
        let (redirect_uri, server_handle, remote_callback_tx, rx) = match remote_base.clone() {
            Some(base) => {
                let (tx, rx) = tokio::sync::oneshot::channel::<social::OAuthCallbackData>();
                (base, None, Some(Mutex::new(Some(tx))), rx)
            }
            None => {
                let (tx, rx) = tokio::sync::oneshot::channel::<social::OAuthCallbackData>();
                let (port, server_handle) = social::start_callback_server(tx)
                    .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;
                (
                    format!("http://127.0.0.1:{}", port),
                    Some(server_handle),
                    None,
                    rx,
                )
            }
        };
        let portal_url = social::build_portal_url(&state, &code_challenge, &redirect_uri);

        let expires_at = Utc::now() + Duration::minutes(10);
        let session_id = uuid::Uuid::new_v4().to_string();

        let session = SocialAuthSession {
            auth_endpoint,
            state,
            code_verifier,
            redirect_uri,
            expires_at,
            callback_rx: tokio::sync::Mutex::new(rx),
            cred_template: KiroCredentials::default(),
            proxy,
            _server_handle: server_handle,
            remote_callback_tx,
            relogin_target_id: Some(target_id),
        };

        self.social_sessions
            .lock()
            .insert(session_id.clone(), session);

        Ok(StartSocialLoginResponse {
            session_id,
            portal_url,
            expires_at: expires_at.to_rfc3339(),
            remote: remote_base.is_some(),
        })
    }

    /// initiate IdC Re-login (updates the existing credential Token rather than creating a new credential)
    pub async fn start_idc_relogin(
        &self,
        target_id: u64,
        req: StartIdcLoginRequest,
    ) -> Result<StartIdcLoginResponse, AdminServiceError> {
        // validate the target credential exists
        {
            let snapshot = self.token_manager.snapshot();
            if !snapshot.entries.iter().any(|e| e.id == target_id) {
                return Err(AdminServiceError::NotFound { id: target_id });
            }
        }

        let config = self.token_manager.config();
        let global_proxy = self.token_manager.proxy();

        let proxy = req
            .proxy_url
            .as_deref()
            .map(ProxyConfig::new)
            .or(global_proxy);

        let start_url = req.start_url.as_deref().unwrap_or(BUILDER_ID_START_URL);

        let reg = idc::register_client(&req.region, start_url, config, proxy.as_ref())
            .await
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        let device = idc::start_device_authorization(
            &req.region,
            start_url,
            &reg.client_id,
            &reg.client_secret,
            config,
            proxy.as_ref(),
        )
        .await
        .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        let expires_at = Utc::now() + Duration::seconds(device.expires_in);
        let session_id = Uuid::new_v4().to_string();

        let session = IdcAuthSession {
            region: req.region,
            client_id: reg.client_id,
            client_secret: reg.client_secret,
            device_code: device.device_code,
            expires_at,
            poll_interval: device.interval.max(5),
            cred_template: KiroCredentials::default(),
            proxy,
            relogin_target_id: Some(target_id),
        };

        let poll_interval = session.poll_interval;
        self.idc_sessions.lock().insert(session_id.clone(), session);

        Ok(StartIdcLoginResponse {
            session_id,
            user_code: device.user_code,
            verification_uri: device.verification_uri,
            verification_uri_complete: device.verification_uri_complete,
            expires_at: expires_at.to_rfc3339(),
            poll_interval,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_compares_correctly() {
        use std::cmp::Ordering;
        assert_eq!(compare_semver("0.3.0", "0.3.1"), Ordering::Less);
        assert_eq!(compare_semver("v0.3.1", "0.3.1"), Ordering::Equal);
        assert_eq!(compare_semver("1.0.0", "0.99.99"), Ordering::Greater);
        assert_eq!(compare_semver("0.3.1-rc.1", "0.3.1"), Ordering::Equal);
    }

    #[test]
    fn export_uses_nested_account_format() {
        let mut cred = KiroCredentials::default();
        cred.refresh_token = Some("rt-123".to_string());
        cred.client_id = Some("cid".to_string());
        cred.client_secret = Some("csec".to_string());
        cred.auth_method = Some("idc".to_string());
        cred.provider = Some("Enterprise".to_string());
        cred.region = Some("us-east-1".to_string());
        cred.email = Some("e@example.com".to_string());
        cred.expires_at = Some("2026-06-06T00:00:00Z".to_string());
        // placeholder profileArn should be stripped during export
        cred.profile_arn = Some(
            crate::kiro::model::credentials::BUILDER_ID_PROFILE_ARN.to_string(),
        );

        let acc = credential_to_export_account(cred).expect("shouldgenerateaccount");

        // nested credentials struct
        assert_eq!(acc.credentials.refresh_token.as_deref(), Some("rt-123"));
        assert_eq!(acc.credentials.client_id.as_deref(), Some("cid"));
        // authMethod normalizeas "IdC"
        assert_eq!(acc.credentials.auth_method.as_deref(), Some("IdC"));
        // expiresAt parse into a millisecond timestamp
        assert!(acc.credentials.expires_at > 0);
        // idp take provider
        assert_eq!(acc.idp, "Enterprise");
        // placeholder profileArn skipped
        assert_eq!(acc.profile_arn, None);
        // required csrfToken outputemptystring
        assert_eq!(acc.credentials.csrf_token, "");
    }

    #[test]
    fn export_skips_api_key_credentials() {
        let mut cred = KiroCredentials::default();
        cred.kiro_api_key = Some("ksk_abc".to_string());
        cred.auth_method = Some("api_key".to_string());
        // none refreshToken → skip
        assert!(credential_to_export_account(cred).is_none());
    }

    #[test]
    fn subscription_type_mapping() {
        assert_eq!(subscription_type_from_title(Some("KIRO FREE")), "Free");
        assert_eq!(subscription_type_from_title(Some("KIRO PRO+")), "Pro_Plus");
        assert_eq!(subscription_type_from_title(Some("KIRO PRO")), "Pro");
        assert_eq!(subscription_type_from_title(Some("KIRO POWER")), "Enterprise");
        assert_eq!(subscription_type_from_title(None), "Free");
    }
}
