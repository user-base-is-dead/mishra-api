//! Admin API typedefine

use crate::admin::proxy_pool::ProxyHealth;
use serde::{Deserialize, Serialize};

// ============ credentialstate ============

/// all credential status response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialsStatusResponse {
    /// credentialtotalcount
    pub total: usize,
    /// Number of available credentials (not disabled).
    pub available: usize,
    /// currentactivecredential ID
    pub current_id: u64,
    /// list of each credential status
    pub credentials: Vec<CredentialStatusItem>,
}

/// status information of a single credential
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusItem {
    /// credentialunique ID
    pub id: u64,
    /// Priority (a smaller number means higher priority).
    pub priority: u32,
    /// iswhetherbydisable
    pub disabled: bool,
    /// consecutivefailedtimescount
    pub failure_count: u32,
    /// Cumulative failure count (all failure types, only increases, zeroed only by a manual reset).
    pub total_failure_count: u64,
    /// whether it is the currently active credential
    pub is_current: bool,
    /// Token expiry time(RFC3339 format)
    pub expires_at: Option<String>,
    /// authmethod
    pub auth_method: Option<String>,
    /// identityprovidevendor(BuilderId / Enterprise / Github / Google / IAM_SSO)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// whether has Profile ARN
    pub has_profile_arn: bool,
    /// refreshToken of SHA-256 hash(only OAuth credential, used for frontend deduplication)
    pub refresh_token_hash: Option<String>,
    /// kiroApiKey of SHA-256 hash(only API Key credential, used for frontend deduplication)
    pub api_key_hash: Option<String>,
    /// kiroApiKey the redacted display (only API Key credential, used for frontend display)
    pub masked_api_key: Option<String>,
    /// User email (for frontend display).
    pub email: Option<String>,
    /// API call successtimescount
    pub success_count: u64,
    /// mostafteronce API calltime(RFC3339 format)
    pub last_used_at: Option<String>,
    /// whether a credential level proxy is configured
    pub has_proxy: bool,
    /// proxy URL(used for frontend display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// Token consecutive refresh failure count
    pub refresh_failure_count: u32,
    /// disableoriginalbecause
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    /// Endpoint name (determines which set the credential uses Kiro API, has fallen back to the default endpoint)
    pub endpoint: String,
    /// The groups the account belongs to (may belong to multiple).
    #[serde(default)]
    pub groups: Vec<String>,
    /// Account source channel (a plain note).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_channel: Option<String>,
    /// Credential balance (the most recent result read from cache, may be None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<BalanceResponse>,
    /// update time of the balance cache (Unix seconds,only in balance hasvaluereturn when)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_updated_at: Option<f64>,
}

// ============ operationrequest ============

/// enable/disablecredentialrequest
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDisabledRequest {
    /// iswhetherdisable
    pub disabled: bool,
}

/// modify priority request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPriorityRequest {
    /// newpriorityvalue
    pub priority: u32,
}

/// addcredentialrequest
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCredentialRequest {
    /// refresh token(OAuth credentialrequired,API Key credentialnotneed)
    pub refresh_token: Option<String>,

    /// access token (optional, import/exportwhenretained)
    #[serde(default)]
    pub access_token: Option<String>,

    /// Profile ARN(optional; when missing, some upstream interfaces reject the request)
    #[serde(default)]
    pub profile_arn: Option<String>,

    /// Token expiry time (optional,RFC3339 format)
    #[serde(default)]
    pub expires_at: Option<String>,

    /// authentication method (optional, default social)
    #[serde(default = "default_auth_method")]
    pub auth_method: String,

    /// identityprovidevendor
    #[serde(default)]
    pub provider: Option<String>,

    /// OIDC Client ID(IdC auth needs)
    pub client_id: Option<String>,

    /// OIDC Client Secret(IdC auth needs)
    pub client_secret: Option<String>,

    /// SSO Start URL(Enterprise / IAM Identity Center account dedicateduse)
    #[serde(default)]
    pub start_url: Option<String>,

    /// priority (optional, default 0)
    #[serde(default)]
    pub priority: u32,

    /// credential level Region config(used for OIDC token refresh)
    /// fall back when not configured to config.json global region
    pub region: Option<String>,

    /// credential level Auth Region(used for Token refresh)
    pub auth_region: Option<String>,

    /// credential level API Region(used for API request)
    pub api_region: Option<String>,

    /// credential level Machine ID(optional,64 bitstring)
    /// fall back when not configured to config.json of machineId
    pub machine_id: Option<String>,

    /// User email (optional, for frontend display).
    pub email: Option<String>,

    /// credential levelproxy URL(optional, special value "direct" means do not use a proxy)
    pub proxy_url: Option<String>,

    /// Credential level proxy auth username (optional).
    pub proxy_username: Option<String>,

    /// Credential level proxy auth password (optional).
    pub proxy_password: Option<String>,

    /// Kiro API Key(API Key credential required, format: ksk_xxxxxxxx)
    /// after setting directly as Bearer Token use, no need refreshToken
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kiro_api_key: Option<String>,

    /// Endpoint name (optional, used when not configured config.defaultEndpoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// The groups the account belongs to (may belong to multiple, optional).
    #[serde(default)]
    pub groups: Vec<String>,
    /// Account source channel (a plain note, optional).
    #[serde(default)]
    pub source_channel: Option<String>,
}

fn default_auth_method() -> String {
    "social".to_string()
}

/// update refreshToken request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRefreshTokenRequest {
    /// newofrefresh token
    pub refresh_token: String,
    /// optional: update at the same time accessToken(avoids needing a refresh immediately after a forced clear)
    #[serde(default)]
    pub access_token: Option<String>,
    /// optional: update at the same time expiresAt(with accessToken matching)
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Update credential request (editable fields only,None meansdo not modify,Some("") meansclear)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCredentialRequest {
    /// User email (for frontend display).
    pub email: Option<String>,
    /// credential levelproxy URL(an empty string means clear)
    pub proxy_url: Option<String>,
    /// credential level proxy authentication username
    pub proxy_username: Option<String>,
    /// credential level proxy authentication password
    pub proxy_password: Option<String>,
    /// the group the account belongs to (None meansdo not modify,Some means a full replacement)
    #[serde(default)]
    pub groups: Option<Vec<String>>,
    /// account source channel (None means no change, an empty string means clear)
    #[serde(default)]
    pub source_channel: Option<String>,
}

/// add credential success response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCredentialResponse {
    pub success: bool,
    pub message: String,
    /// newaddofcredential ID
    pub credential_id: u64,
    /// User email (if fetched successfully).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

// ============ batch import(SSE) ============

/// Batch import request. The server by `concurrency`(default 8,clamptaketo [1,16])hasboundary concurrently
/// process one by one, results via SSE streamperentrypush.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportRequest {
    /// Credential pending import (reuses the rich type of single add).
    pub credentials: Vec<AddCredentialRequest>,
    /// concurrency level,default 8,serviceend clamp to [1, 16]
    #[serde(default)]
    pub concurrency: Option<u8>,
    /// iswhethervalidate.`true`(default):add then fetches the balance for validation, rolls back on failure;
    /// `false`: only add persist to db ("directlyimport"), does not fetch the balance and does not roll back.
    #[serde(default = "default_batch_verify")]
    pub verify: bool,
}

fn default_batch_verify() -> bool {
    true
}

/// batch import SSE event. Sends one when each credential completes. `index` events; sends one after all complete.
/// `status == "summary"` the summary event (at this point `index` as None).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportEvent {
    /// corresponds to the request array index;summary event is None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// "verified" | "duplicate" | "failed" | "summary"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// "current/limit" usagestring,verified fill when
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// failed and when rolled back (deleted) it is true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rolled_back: Option<bool>,
    /// only summary eventfill
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<BatchImportSummary>,
}

/// Batch import summary (the final event).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportSummary {
    pub total: usize,
    /// Direct import (no liveness check) success count.
    pub imported: usize,
    pub verified: usize,
    pub duplicate: usize,
    pub failed: usize,
    pub rolled_back: usize,
}

// ============ balancequery ============

/// balancequeryresponse
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceResponse {
    /// credential ID
    pub id: u64,
    /// subscriptiontype
    pub subscription_title: Option<String>,
    /// currentusage
    pub current_usage: f64,
    /// usequota limit
    pub usage_limit: f64,
    /// remainingquota
    pub remaining: f64,
    /// usepercentage
    pub usage_percentage: f64,
    /// next reset time (Unix timestamp)
    pub next_reset_at: Option<f64>,
    /// Whether the user currently enabled overage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_enabled: Option<bool>,
    /// whether the account can enable overage (FREE etc.subscription usuallyas false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_capable: Option<bool>,
    /// upstream `overageCapability` raw string (used for troubleshooting"unknown"state)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_capability_raw: Option<String>,
}

// ============ availablemodelquery ============

/// Response of the model list currently available for a credential.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModelsResponse {
    /// credential ID
    pub id: u64,
    /// The model currently available for the credential (by subscription tier).
    pub models: Vec<AvailableModelItem>,
}

/// single availablemodel
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModelItem {
    /// model ID
    pub model_id: String,
    /// modeldisplay name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// modeldescription
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// maximuminput Token count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<i64>,
}

// ============ one clickoverage ============

/// one click overage disable result
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaExceededResult {
    /// the disabled credential ID list
    pub disabled_ids: Vec<u64>,
    /// skipofcredential ID list (such as disable failure, cache miss, and so on).
    pub skipped_ids: Vec<u64>,
}

/// Sets the overage switch of a single credential.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOverageRequest {
    /// true openoverage;false close
    pub enabled: bool,
}

/// one click enable overage result
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableOverageAllResult {
    /// the successfully enabled credential ID list
    pub enabled_ids: Vec<u64>,
    /// skip (cannot enable / enabled / cachemissing)
    pub skipped_ids: Vec<u64>,
    /// the credential whose call failed ID list
    pub failed_ids: Vec<u64>,
    /// failedoriginalbecause(with failed_ids one by onecorresponds)
    pub failure_messages: Vec<String>,
}

// ============ load balancingconfig ============

/// load balancing mode response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadBalancingModeResponse {
    /// currentmode ("priority" or "balanced")
    pub mode: String,
}

/// set load balancing mode request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLoadBalancingModeRequest {
    /// mode ("priority" or "balanced")
    pub mode: String,
}

/// Account level throttle failover config response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountThrottleConfigResponse {
    /// whether to enable account level 429 failover
    pub failover: bool,
    /// cooldown duration (seconds)
    pub cooldown_secs: u64,
}

/// Updates the account level throttle failover config.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAccountThrottleConfigRequest {
    /// Whether to enable failover; omitted means no change.
    #[serde(default)]
    pub failover: Option<bool>,
    /// Cooldown duration (seconds); omitted means no change,1..=86400
    #[serde(default)]
    pub cooldown_secs: Option<u64>,
}

/// log governance configuration response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogGovernanceConfigResponse {
    /// Whether to enable request trace writing.
    pub trace_enabled: bool,
    /// trace recordretaindaycount
    pub trace_retention_days: u32,
    /// usage log retention days
    pub usage_log_retention_days: u32,
}

/// Updates the log governance config (an omitted field means no change).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLogGovernanceConfigRequest {
    #[serde(default)]
    pub trace_enabled: Option<bool>,
    /// trace retaindaycount,1..=365
    #[serde(default)]
    pub trace_retention_days: Option<u32>,
    /// usage log retention days,1..=365
    #[serde(default)]
    pub usage_log_retention_days: Option<u32>,
}

// ============ proxy pool ============

/// proxy poolentryentry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolEntry {
    /// unique ID(auto increment)
    pub id: u64,
    /// proxy URL(such as socks5://user:pass@host:port)
    pub url: String,
    /// note label (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// iswhetherenable
    pub enabled: bool,
    /// number of credentials using this proxy
    pub credential_count: u32,
    /// healthstate
    pub health: ProxyHealth,
    /// The latency of the most recent successful probe (milliseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    /// the most recent probe time (RFC3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    /// consecutive probe failure count
    pub consecutive_failures: u32,
    /// Whether it was auto disabled by the health check.
    pub auto_disabled: bool,
}

/// proxy pool list response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolResponse {
    pub total: usize,
    pub proxies: Vec<ProxyPoolEntry>,
}

/// single proxy health check response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyCheckResponse {
    pub id: u64,
    pub health: ProxyHealth,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    pub enabled: bool,
    pub auto_disabled: bool,
}

/// full health check response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyCheckAllResponse {
    pub healthy: usize,
    pub unhealthy: usize,
    pub auto_disabled: usize,
}

/// load balancing batch assignment request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignRoundRobinRequest {
    /// targetcredential ID list; empty or omitted means allocate to all credentials.
    #[serde(default)]
    pub credential_ids: Option<Vec<u64>>,
}

/// load balancing batch assignment response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignRoundRobinResponse {
    /// number of successfully assigned credentials
    pub assigned: usize,
    /// number of available proxies participating in load balancing
    pub proxy_count: usize,
}

/// addproxyrequest
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddProxyRequest {
    pub url: String,
    #[serde(default)]
    pub label: Option<String>,
}

/// batch import proxy request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchAddProxyRequest {
    /// proxy URL list (one per line)
    pub urls: Vec<String>,
}

/// assign proxy to credential request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignProxyRequest {
    /// the proxy in the proxy pool ID;null meansclearproxy
    #[serde(default)]
    pub proxy_id: Option<u64>,
}

// ============ globalproxy config ============

/// global proxy configuration response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalProxyResponse {
    /// currentglobal proxy URL(null meansnot configured)
    pub proxy_url: Option<String>,
}

/// set global proxy request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalProxyRequest {
    /// proxy URL,null means clear the global proxy
    pub proxy_url: Option<String>,
}

// ============ inlineupdateconfig ============

/// online update configuration response
///
/// inlineupdatego"download GitHub Releases binary + enterprocess exitby docker restart policy take over"
/// approach, exposing only version related metadata.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigResponse {
    /// The version running before the last successful update (with `v` prefix); when present the frontend can show the rollback button.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_version: Option<String>,
    /// The time the last online update successfully completed (RFC3339); used by the frontend to show last updated at. ….
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_applied_at: Option<String>,
    /// iswhetherconfigured GitHub Token(returns only a boolean, not the plaintext, to avoid frontend leakage).
    pub github_token_set: bool,
    /// Whether to enable unattended auto update.
    pub auto_apply: bool,
    /// Auto update trigger time (local time zone,HH:MM 24 hourmechanism)
    pub auto_apply_time: String,
}

/// update the online update configuration
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetUpdateConfigRequest {
    /// GitHub Personal Access Token; an empty string means clear, omit to keep the original value.
    pub github_token: Option<String>,
    /// Whether to enable unattended auto update; omit to keep the original value.
    pub auto_apply: Option<bool>,
    /// auto update trigger time (HH:MM); if not passed keep the original value
    pub auto_apply_time: Option<String>,
}

/// online update operation result
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageUpdateResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub applied: bool,
    pub need_restart: bool,
}

/// GitHub API limitstreamstate(including token verifyresult)
///
/// call `GET https://api.github.com/rate_limit`: this endpoint itself does not consume throttle quota,
/// used to display for the frontend the current token whether haseffect / remainingtimescount / resettime.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRateLimitInfo {
    /// provided token whether haseffect(no token is when false but the anonymous limit can still be queried)
    pub valid: bool,
    /// whether carries token call (false = anonymousquery)
    pub authenticated: bool,
    /// throttle upper limit (anonymous 60, auth 5000)
    pub limit: u64,
    /// remainingavailabletimescount
    pub remaining: u64,
    /// alreadyusetimescount
    pub used: u64,
    /// throttle window reset time (Unix seconds)
    pub reset: u64,
    /// token the corresponding username (only token returned when valid and belonging to an individual)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    /// the prompt message on failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// test GitHub Token the request body for validity; an empty or missing field is treated as"usealreadysaveof token"
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRateLimitRequest {
    /// pendingtestof token; use when default or empty `config.github_token`, if still default then anonymous query
    #[serde(default)]
    pub github_token: Option<String>,
}

/// "checkupdate"interfacereturnresult
///
/// when has_update=true the frontend can show a red dot reminder on the toolbar icon.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckInfo {
    /// current running version (taken from Cargo.toml)
    pub current_version: String,
    /// GitHub Release The latest version number on it (with the prefix removed v); an empty string when the query fails.
    pub latest_version: String,
    /// whether a new version exists
    pub has_update: bool,
    /// build type; currently fixed as "binary",beforeenddisplayuse
    pub build_type: String,
    /// Release title(such ashas)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_name: Option<String>,
    /// Release note
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    /// Release page URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
    /// Release publishtime(RFC 3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// checktime(RFC 3339)
    pub checked_at: String,
    /// iswhetherfromcache
    pub cached: bool,
    /// Warning info when the query fails (still carries the cached old result).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

// ============ loginAPIkeymodify ============

/// modifyloginAPIkey (used for admin panel login adminApiKey) request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAdminKeyRequest {
    /// newofloginAPIkey
    pub new_key: String,
}

// ============ client API Key dispatch ============

/// client Key list item (redacted display)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientKeyItem {
    pub id: u64,
    /// redactafterof Key display(such as csk_abcd...mnop)
    pub masked_key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub disabled: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    pub total_calls: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// whether it is a system key (config.json apiKey imported, cannot be deleted / notcanrotate)
    #[serde(default)]
    pub is_system: bool,
}

/// client Key listresponse
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientKeysResponse {
    pub total: usize,
    pub keys: Vec<ClientKeyItem>,
}

/// createclient Key request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateClientKeyRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

/// createclient Key response(plaintext Key returned only once here)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateClientKeyResponse {
    pub id: u64,
    pub key: String,
    pub name: String,
    pub created_at: String,
}

/// updateclient Key metadata
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateClientKeyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

// ============ IdC device authorization login ============

/// initiate IdC device authorizationrequest
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartIdcLoginRequest {
    pub region: String,
    #[serde(default)]
    pub start_url: Option<String>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
}

/// initiate IdC device authorizationresponse
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartIdcLoginResponse {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri_complete: Option<String>,
    pub expires_at: String,
    pub poll_interval: i64,
}

/// poll IdC loginstateresponse
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum PollIdcLoginResponse {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "success")]
    Success { credential_id: u64 },
    #[serde(rename = "expired")]
    Expired,
}

// ============ Social login (Portal PKCE OAuth) ============

/// initiate Social loginrequest
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSocialLoginRequest {
    /// priority(default 0)
    #[serde(default)]
    pub priority: u32,
    /// user email (optional)
    #[serde(default)]
    pub email: Option<String>,
    /// proxy URL(optional)
    #[serde(default)]
    pub proxy_url: Option<String>,
    /// Kiro auth endpoint(leave empty to use the default)
    #[serde(default)]
    pub auth_endpoint: Option<String>,
    /// OAuth Public callback address (remote mode). Usually derived automatically by the frontend from the current access address:
    /// `${location.origin}/api/admin/auth/callback`. if `config.callbackBaseUrl` If configured, it takes precedence (overrides).
    #[serde(default)]
    pub callback_base_url: Option<String>,
}

/// initiate Social loginresponse
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSocialLoginResponse {
    /// session ID
    pub session_id: String,
    /// the one opened in the browser portal URL
    pub portal_url: String,
    /// session expiry time (RFC3339)
    pub expires_at: String,
    /// Whether it is in remote callback mode (configured). callbackBaseUrl).
    /// true when OAuth The callback points to a public route; the frontend can auto poll to completion;false goes through the local port.
    pub remote: bool,
}

/// manually finishedinto Social Login request (remote access case: copy the callback from the browser address bar). URL)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteSocialLoginRequest {
    /// OAuth authorization code (from the callback URL of code parameterextract)
    pub code: String,
    /// OAuth state(fromcallback URL of state parameter extraction, used for CSRF validation)
    pub state: String,
    /// login option (from the callback URL of login_option parameter extraction, may be empty)
    #[serde(default)]
    pub login_option: String,
    /// callback URL path(such as /oauth/callback)
    #[serde(default = "default_oauth_path")]
    pub path: String,
}

fn default_oauth_path() -> String {
    "/oauth/callback".to_string()
}

// ============ throughuseresponse ============

// ============ account export ============

/// The auth credential of a single account in the account export file (nested). `credentials` object)
///
/// `expiresAt` as a millisecond timestamp,`authMethod` take `"IdC"` / `"social"`,
/// `accessToken` / `csrfToken` A required field (outputs an empty string when there is no value).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedCredentials {
    pub access_token: String,
    pub csrf_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_url: Option<String>,
    pub expires_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// A single account in the account export file (nested). `Account` struct)
///
/// The account fields are at the top level, credentials collected into a nested `credentials` object, convenient for third party account management tools to import directly.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedAccount {
    pub id: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub idp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_arn: Option<String>,
    pub credentials: ExportedCredentials,
    /// Subscription info (minimal usable structure:type + title)
    pub subscription: serde_json::Value,
    /// Usage information (minimal usable structure: zeroed).
    pub usage: serde_json::Value,
    pub tags: Vec<String>,
    pub status: String,
    pub created_at: i64,
    pub last_used_at: i64,
}

/// account export response (including top level `groups` / `tags` array, convenient for a third party importer to consume directly.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialsExportResponse {
    /// export format version number
    pub version: String,
    /// Export time (millisecond timestamp).
    pub exported_at: i64,
    /// account list (nested Account format)
    pub accounts: Vec<ExportedAccount>,
    /// Groups (export does not include groups, fixed empty array).
    pub groups: Vec<serde_json::Value>,
    /// Tags (export does not include tags, fixed empty array).
    pub tags: Vec<serde_json::Value>,
}

/// operationsuccess response
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

impl SuccessResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }
}

/// errorresponse
#[derive(Debug, Serialize)]
pub struct AdminErrorResponse {
    pub error: AdminError,
}

#[derive(Debug, Serialize)]
pub struct AdminError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

impl AdminErrorResponse {
    pub fn new(error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: AdminError {
                error_type: error_type.into(),
                message: message.into(),
            },
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message)
    }

    pub fn authentication_error() -> Self {
        Self::new("authentication_error", "Invalid or missing admin API key")
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    pub fn api_error(message: impl Into<String>) -> Self {
        Self::new("api_error", message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new("internal_error", message)
    }
}

// ============ Account group (independent entity).============

/// single group (list item)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupItem {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: String,
    /// Reference count: how many credentials carry this group (shown by the frontend). / deletebeforeremind)
    pub credential_count: usize,
    /// Reference count: how many client keys Key bind thisitemgroup
    pub client_key_count: usize,
}

/// grouplistresponse
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupsResponse {
    pub total: usize,
    pub groups: Vec<GroupItem>,
}

/// creategrouprequest
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// update group request (rename / change note; both are optional)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupRequest {
    /// New name; omit or match the original name means no rename.
    #[serde(default)]
    pub new_name: Option<String>,
    /// New note; pass an empty string to clear the note; omit the field to keep it.
    #[serde(default)]
    pub description: Option<String>,
}

/// Optional query parameters for deleting a group.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteGroupQuery {
    /// Force delete: deletes even if references remain; also cascade cleans credentials. / Key reference
    #[serde(default)]
    pub force: bool,
}
