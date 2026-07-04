use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TlsBackend {
    Rustls,
    NativeTls,
}

impl Default for TlsBackend {
    fn default() -> Self {
        Self::Rustls
    }
}

/// KNA app config
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    /// OAuth Public callback address (configured for remote deployment).
    ///
    /// leave empty:Social Login starts a temporary callback port on the server local machine (`http://127.0.0.1:{port}`),
    /// reachable only by the local browser.
    /// configafter(such as `https://example.com/api/admin/auth/callback`):OAuth `redirect_uri`
    /// Uses this address instead; after browser authorization it lands on `{callbackBaseUrl}/oauth/callback`,
    /// Received by this service public callback route. `code` and automatically complete login, adapt Docker / VPS / Render etc.remotedeploy.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_base_url: Option<String>,

    #[serde(default = "default_region")]
    pub region: String,

    /// Auth Region(used for Token refresh); falls back when not configured to region
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_region: Option<String>,

    /// API Region(used for API request); falls back when not configured to region
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_region: Option<String>,

    #[serde(default = "default_kiro_version")]
    pub kiro_version: String,

    #[serde(default)]
    pub machine_id: Option<String>,

    #[serde(default)]
    pub api_key: Option<String>,

    #[serde(default = "default_system_version")]
    pub system_version: String,

    #[serde(default = "default_node_version")]
    pub node_version: String,

    #[serde(default = "default_tls_backend")]
    pub tls_backend: TlsBackend,

    /// external count_tokens API address(optional)
    #[serde(default)]
    pub count_tokens_api_url: Option<String>,

    /// count_tokens API key (optional)
    #[serde(default)]
    pub count_tokens_api_key: Option<String>,

    /// count_tokens API authentication type (optional,"x-api-key" or "bearer", default "x-api-key")
    #[serde(default = "default_count_tokens_auth_type")]
    pub count_tokens_auth_type: String,

    /// HTTP proxy address (optional)
    /// supported format: http://host:port, https://host:port, socks5://host:port
    #[serde(default)]
    pub proxy_url: Option<String>,

    /// Proxy auth username (optional).
    #[serde(default)]
    pub proxy_username: Option<String>,

    /// proxy authentication password (optional)
    #[serde(default)]
    pub proxy_password: Option<String>,

    /// Admin API key (optional, enable Admin API feature)
    #[serde(default)]
    pub admin_api_key: Option<String>,

    /// The version that was running before the last successful update, used to show in the frontend as rollback to vX.Y.Zbutton.
    /// the actual rollback action via `<exe>.backup` the file completes, without accessing the network.
    #[serde(default)]
    pub update_previous_version: Option<String>,

    /// GitHub Personal Access Token(optional). after setting GitHub Releases the interface will carry
    /// `Authorization: Bearer <token>`, move the throttle from anonymous 60/h mention auth 5000/h.
    /// only need `public_repo` read permission suffices.
    #[serde(default)]
    pub github_token: Option<String>,

    /// The time the last online update successfully completed (RFC3339). Used by the frontend to show last updated at. ….
    #[serde(default)]
    pub update_last_applied_at: Option<String>,

    /// Whether to enable unattended auto update. When on, the service will each day `update_auto_apply_time`
    /// check at each moment GitHub Releases, on finding a new version, automatically downloads the binary and replaces and restarts.
    #[serde(default)]
    pub update_auto_apply: bool,

    /// The daily trigger time for auto update (local time zone,`HH:MM` 24 hourmechanism).
    /// default 03:00 Runs in the early morning to minimize impact on the online service.
    #[serde(default = "default_update_auto_apply_time")]
    pub update_auto_apply_time: String,

    /// load balancing mode ("priority" or "balanced")
    #[serde(default = "default_load_balancing_mode")]
    pub load_balancing_mode: String,

    /// account level 429 Whether, when throttle triggers, the current credential enters cooldown and fails over (default true).
    ///
    /// after close:429 + suspicious activity Still retries as an ordinary transient error, does not switch the credential.
    /// after enabling: recognized suspicious activity string, cools down the current credential. `account_throttle_cooldown_secs` seconds,
    /// Immediately switches to the next available credential.
    #[serde(default = "default_account_throttle_failover")]
    pub account_throttle_failover: bool,

    /// Account level throttle cooldown duration (seconds, default 1800 = 30 minutes).
    #[serde(default = "default_account_throttle_cooldown_secs")]
    pub account_throttle_cooldown_secs: u64,

    /// whether to enable the non streaming response thinking block extract(default true)
    ///
    /// When enabled, in a non streaming response the `<thinking>...</thinking>` the tag will be parsed as
    /// independent `{"type": "thinking", ...}` content block,consistent with the streaming response behavior.
    #[serde(default = "default_extract_thinking")]
    pub extract_thinking: bool,

    /// Default endpoint name (when the credential does not explicitly specify endpoint whenuse, default "ide")
    #[serde(default = "default_endpoint")]
    pub default_endpoint: String,

    /// Whether to enable request tracing (write traces.db). default true.
    ///
    /// after closing: no longer write trace record, notgo TraceSink, but `GET /api/admin/traces`
    /// Can still query historically stored records. Suitable for privacy sensitive or disk constrained scenarios.
    #[serde(default = "default_trace_enabled")]
    pub trace_enabled: bool,

    /// Retention days for request trace records (default 7). A background task cleans expired records daily.
    #[serde(default = "default_trace_retention_days")]
    pub trace_retention_days: u32,

    /// request usage log (usage_log.*.jsonl + aggregation bucket) retention days (default 31).
    #[serde(default = "default_usage_log_retention_days")]
    pub usage_log_retention_days: u32,

    /// endpoint specific configuration
    ///
    /// the key is the endpoint name (such as "ide" / "cli"), the value is a parameter object freely defined by the endpoint.
    /// Endpoints not present in this table use the implementation built in defaults.
    #[serde(default)]
    pub endpoints: HashMap<String, serde_json::Value>,

    /// The config file path (runtime metadata, not written into JSON)
    #[serde(skip)]
    config_path: Option<PathBuf>,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_region() -> String {
    "us-east-1".to_string()
}

fn default_kiro_version() -> String {
    "2.3.0".to_string()
}

fn default_system_version() -> String {
    "macos".to_string()
}

fn default_node_version() -> String {
    "22.22.0".to_string()
}

fn default_count_tokens_auth_type() -> String {
    "x-api-key".to_string()
}

fn default_tls_backend() -> TlsBackend {
    TlsBackend::Rustls
}

fn default_load_balancing_mode() -> String {
    "priority".to_string()
}

fn default_account_throttle_failover() -> bool {
    true
}

fn default_account_throttle_cooldown_secs() -> u64 {
    30 * 60
}

fn default_update_auto_apply_time() -> String {
    "03:00".to_string()
}

fn default_extract_thinking() -> bool {
    true
}

fn default_endpoint() -> String {
    crate::kiro::endpoint::ide::IDE_ENDPOINT_NAME.to_string()
}

fn default_trace_enabled() -> bool {
    true
}

fn default_trace_retention_days() -> u32 {
    7
}

fn default_usage_log_retention_days() -> u32 {
    31
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            callback_base_url: None,
            region: default_region(),
            auth_region: None,
            api_region: None,
            kiro_version: default_kiro_version(),
            machine_id: None,
            api_key: None,
            system_version: default_system_version(),
            node_version: default_node_version(),
            tls_backend: default_tls_backend(),
            count_tokens_api_url: None,
            count_tokens_api_key: None,
            count_tokens_auth_type: default_count_tokens_auth_type(),
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            admin_api_key: None,
            update_previous_version: None,
            github_token: None,
            update_last_applied_at: None,
            update_auto_apply: false,
            update_auto_apply_time: default_update_auto_apply_time(),
            load_balancing_mode: default_load_balancing_mode(),
            account_throttle_failover: default_account_throttle_failover(),
            account_throttle_cooldown_secs: default_account_throttle_cooldown_secs(),
            extract_thinking: default_extract_thinking(),
            default_endpoint: default_endpoint(),
            trace_enabled: default_trace_enabled(),
            trace_retention_days: default_trace_retention_days(),
            usage_log_retention_days: default_usage_log_retention_days(),
            endpoints: HashMap::new(),
            config_path: None,
        }
    }
}

impl Config {
    /// get the default configuration file path
    pub fn default_config_path() -> &'static str {
        "config.json"
    }

    /// fetchvalid Auth Region(used for Token refresh)
    /// prefer use auth_region, fall back when not configured to region
    pub fn effective_auth_region(&self) -> &str {
        self.auth_region.as_deref().unwrap_or(&self.region)
    }

    /// fetchvalid API Region(used for API request)
    /// prefer use api_region, fall back when not configured to region
    pub fn effective_api_region(&self) -> &str {
        self.api_region.as_deref().unwrap_or(&self.region)
    }

    /// load the configuration from the file
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            // The config file does not exist; returns the default config.
            let mut config = Self::default();
            config.config_path = Some(path.to_path_buf());
            return Ok(config);
        }

        let content = fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&content)?;
        config.config_path = Some(path.to_path_buf());

        // The user manually cleared a string field (such as `"updateAutoApplyTime": ""`) when,serde defaultvaluenotwill
        // intervene;heretake"looks likeempty"key fields fall back to defaults, avoiding later business logic using
        // An empty string causes hard to diagnose errors.
        if config.update_auto_apply_time.trim().is_empty() {
            config.update_auto_apply_time = default_update_auto_apply_time();
        }

        Ok(config)
    }

    /// Gets the config file path (if any).
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    /// Writes the current config back to the original config file.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self
            .config_path
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("The config file path is unknown; cannot save the config."))?;

        let content = serde_json::to_string_pretty(self).context("failed to serialize the configuration")?;
        fs::write(path, content)
            .with_context(|| format!("failed to write the configuration file: {}", path.display()))?;
        Ok(())
    }
}
