//! Kiro OAuth credentialdata model
//!
//! support from Kiro IDE load from the credential file, use Social authmethod
//! Supports single credential and multi credential config formats.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::http_client::ProxyConfig;
use crate::model::config::Config;

pub const BUILDER_ID_PROFILE_ARN: &str =
    "arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX";
pub const SOCIAL_PROFILE_ARN: &str =
    "arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK";

/// Kiro OAuth credential
///
/// `Debug` the output is redacted:access_token / refresh_token / client_secret /
/// kiro_api_key / proxy_password Sensitive fields such as this only show the length and do not leak the plaintext.
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KiroCredentials {
    /// credential unique identifier (auto increment ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,

    /// access token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,

    /// refresh token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Profile ARN
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_arn: Option<String>,

    /// expiry time (RFC3339 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,

    /// authmethod (social / idc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,

    /// identityprovidevendor(BuilderId / Enterprise / Github / Google / IAM_SSO)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// OIDC Client ID (IdC auth needs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// OIDC Client Secret (IdC auth needs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// SSO Start URL(Enterprise / IAM Identity Center account dedicateduse)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_url: Option<String>,

    /// Credential priority (a smaller number means higher priority, default is 0)
    #[serde(default)]
    #[serde(skip_serializing_if = "is_zero")]
    pub priority: u32,

    /// credential level Region config(used for OIDC token refresh)
    /// fall back when not configured to config.json global region
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// credential level Auth Region(used for Token refresh)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_region: Option<String>,

    /// credential level API Region(used for API request)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_region: Option<String>,

    /// credential level Machine ID config (optional)
    /// fall back when not configured to config.json of machineId; when neither is configured by refreshToken derive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,

    /// useremail(from Anthropic API fetch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// subscriptionetc.level(KIRO PRO+ / KIRO FREE etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub subscription_title: Option<String>,

    /// credential levelproxy URL(optional)
    /// support http/https/socks5 protocol
    /// special value "direct" Indicates explicitly using no proxy (even if a proxy is globally configured).
    /// Falls back to the global proxy config when not configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,

    /// Credential level proxy auth username (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_username: Option<String>,

    /// Credential level proxy auth password (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_password: Option<String>,

    /// Whether the credential is disabled (default is false)
    #[serde(default)]
    pub disabled: bool,

    /// Kiro API Key(headless mode)
    /// format: ksk_xxxxxxxx
    /// after setting directly as Bearer Token use, no need refreshToken
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kiro_api_key: Option<String>,

    /// endpoint name (optional)
    ///
    /// decide which set this credential goes through Kiro API. fall back when not configured to `config.defaultEndpoint`(default "ide").
    /// The endpoint name must be among the endpoints registered at startup. registry exists in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// The groups the account belongs to (may belong to multiple).
    ///
    /// client Key after binding to a group, use that Key The requests it initiates are only scheduled to groups accounts containing this group name.
    /// An empty array means the account belongs to no group (only accounts not bound to a group Key / master apiKey canuse).
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,

    /// Account source channel (a plain note).
    ///
    /// mark the purchase source of this account/channel, convenient for operations tracking. Does not participate in scheduling, export, or filtering.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_channel: Option<String>,
}

/// Determines whether it is zero (used to skip serialization).
fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// Shows only the length, does not expose the plaintext. For example `Some(42 chars)` or `None`.
fn fmt_redacted(value: &Option<String>) -> String {
    match value {
        Some(s) if !s.is_empty() => format!("Some({} chars)", s.chars().count()),
        Some(_) => "Some(<empty>)".to_string(),
        None => "None".to_string(),
    }
}

impl std::fmt::Debug for KiroCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Individually redacts all that may contain the key./Token field; other metadata is printed normally.
        f.debug_struct("KiroCredentials")
            .field("id", &self.id)
            .field("access_token", &fmt_redacted(&self.access_token))
            .field("refresh_token", &fmt_redacted(&self.refresh_token))
            .field("profile_arn", &self.profile_arn)
            .field("expires_at", &self.expires_at)
            .field("auth_method", &self.auth_method)
            .field("provider", &self.provider)
            .field("client_id", &fmt_redacted(&self.client_id))
            .field("client_secret", &fmt_redacted(&self.client_secret))
            .field("start_url", &self.start_url)
            .field("priority", &self.priority)
            .field("region", &self.region)
            .field("auth_region", &self.auth_region)
            .field("api_region", &self.api_region)
            .field("machine_id", &fmt_redacted(&self.machine_id))
            .field("email", &self.email)
            .field("subscription_title", &self.subscription_title)
            .field("proxy_url", &self.proxy_url)
            .field("proxy_username", &self.proxy_username)
            .field("proxy_password", &fmt_redacted(&self.proxy_password))
            .field("disabled", &self.disabled)
            .field("kiro_api_key", &fmt_redacted(&self.kiro_api_key))
            .field("endpoint", &self.endpoint)
            .field("groups", &self.groups)
            .field("source_channel", &self.source_channel)
            .finish()
    }
}

fn canonicalize_auth_method_value(value: &str) -> &str {
    if value.eq_ignore_ascii_case("builder-id") || value.eq_ignore_ascii_case("iam") {
        "idc"
    } else if value.eq_ignore_ascii_case("api_key") || value.eq_ignore_ascii_case("apikey") {
        "api_key"
    } else {
        value
    }
}

/// Credential config (supports single object or array format).
///
/// Auto detects the config file format:
/// - Single object format (old format, backward compatible).
/// - Array format (new format, supports multiple credentials).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CredentialsConfig {
    /// single credential (old format)
    Single(KiroCredentials),
    /// multiple credential array (new format)
    Multiple(Vec<KiroCredentials>),
}

impl CredentialsConfig {
    /// load the credential configuration from the file
    ///
    /// - If the file does not exist, returns an empty array.
    /// - If the file content is empty, returns an empty array.
    /// - supports single object or array format
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();

        // Returns an empty array when the file does not exist.
        if !path.exists() {
            return Ok(CredentialsConfig::Multiple(vec![]));
        }

        let content = fs::read_to_string(path)?;

        // return an empty array when the file is empty
        if content.trim().is_empty() {
            return Ok(CredentialsConfig::Multiple(vec![]));
        }

        let config = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Converts to a credential list sorted by priority.
    pub fn into_sorted_credentials(self) -> Vec<KiroCredentials> {
        match self {
            CredentialsConfig::Single(mut cred) => {
                cred.canonicalize_auth_method();
                vec![cred]
            }
            CredentialsConfig::Multiple(mut creds) => {
                // Sorts by priority (a smaller number means higher priority).
                creds.sort_by_key(|c| c.priority);
                for cred in &mut creds {
                    cred.canonicalize_auth_method();
                }
                creds
            }
        }
    }

    /// Determines whether it is the multi credential format (array format).
    pub fn is_multiple(&self) -> bool {
        matches!(self, CredentialsConfig::Multiple(_))
    }
}

impl KiroCredentials {
    /// Special value: explicitly uses no proxy.
    pub const PROXY_DIRECT: &'static str = "direct";

    /// get the default credential file path
    pub fn default_credentials_path() -> &'static str {
        "credentials.json"
    }

    /// fetchvalid Auth Region(used for Token refresh)
    /// priority:credential.auth_region > credential.region > config.auth_region > config.region
    pub fn effective_auth_region<'a>(&'a self, config: &'a Config) -> &'a str {
        self.auth_region
            .as_deref()
            .or(self.region.as_deref())
            .unwrap_or(config.effective_auth_region())
    }

    /// fetchvalid API Region(used for API request)
    /// priority:credential.api_region > config.api_region > config.region
    pub fn effective_api_region<'a>(&'a self, config: &'a Config) -> &'a str {
        self.api_region
            .as_deref()
            .unwrap_or(config.effective_api_region())
    }

    /// get the effective proxy configuration
    /// priority: credential proxy > global proxy > no proxy
    /// special value "direct" Indicates explicitly using no proxy (even if a proxy is globally configured).
    pub fn effective_proxy(&self, global_proxy: Option<&ProxyConfig>) -> Option<ProxyConfig> {
        match self.proxy_url.as_deref() {
            Some(url) if url.eq_ignore_ascii_case(Self::PROXY_DIRECT) => None,
            Some(url) => {
                let mut proxy = ProxyConfig::new(url);
                if let (Some(username), Some(password)) =
                    (&self.proxy_username, &self.proxy_password)
                {
                    proxy = proxy.with_auth(username, password);
                }
                Some(proxy)
            }
            None => global_proxy.cloned(),
        }
    }

    pub fn canonicalize_auth_method(&mut self) {
        let auth_method = match &self.auth_method {
            Some(m) => m,
            None => return,
        };

        let canonical = canonicalize_auth_method_value(auth_method);
        if canonical != auth_method {
            self.auth_method = Some(canonical.to_string());
        }
    }

    pub fn fill_default_profile_arn(&mut self) -> bool {
        if self.profile_arn.is_some() || self.is_api_key_credential() {
            return false;
        }

        self.profile_arn = Some(self.default_profile_arn().to_string());
        true
    }

    /// whether is Social login (Github / Google).
    fn is_social_login(&self) -> bool {
        self.auth_method
            .as_deref()
            .map(|m| m.eq_ignore_ascii_case("social"))
            .unwrap_or(false)
            || self
                .provider
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case("github") || p.eq_ignore_ascii_case("google"))
                .unwrap_or(false)
    }

    /// credential missingexplicit profileArn the default that should be used when ARN:
    /// Social loginuseshare Social ARN, rest (BuilderID etc.) use BuilderID placeholder.
    fn default_profile_arn(&self) -> &'static str {
        if self.is_social_login() {
            SOCIAL_PROFILE_ARN
        } else {
            BUILDER_ID_PROFILE_ARN
        }
    }

    /// check whether the credential supports Opus model
    ///
    /// Free accountnot supported Opus model,need PRO or a higher tier subscription
    pub fn supports_opus(&self) -> bool {
        match &self.subscription_title {
            Some(title) => {
                let title_upper = title.to_uppercase();
                // if contains FREE,thennot supported Opus
                !title_upper.contains("FREE")
            }
            // If subscription info has not been fetched yet, allow temporarily (it is fetched on first use).
            None => true,
        }
    }

    /// checkwhether is API Key credential
    ///
    /// API Key credentialdirectlyuse kiro_api_key as Bearer Token, no need refreshToken
    pub fn is_api_key_credential(&self) -> bool {
        self.kiro_api_key.is_some()
            || self
                .auth_method
                .as_deref()
                .map(|m| m.eq_ignore_ascii_case("api_key") || m.eq_ignore_ascii_case("apikey"))
                .unwrap_or(false)
    }

    /// Returns the real one that can be sent to upstream. profileArn(skip BuilderID placeholder).
    ///
    /// - real ARN(including Social share ARN)→ return as is;
    /// - [`BUILDER_ID_PROFILE_ARN`] placeholder → return `None`(non streaming/header type calls should not send
    ///   BuilderID placeholder; for streaming requests please use [`Self::streaming_profile_arn`]).
    pub fn effective_profile_arn(&self) -> Option<&str> {
        match self.profile_arn.as_deref() {
            Some(arn) if !is_placeholder_profile_arn(arn) => Some(arn),
            _ => None,
        }
    }

    /// return the streaming chat endpoint (`generateAssistantResponse` / `SendMessageStreaming`)
    /// should be sent profileArn.
    ///
    /// The new upstream mandatorily requires it for the streaming endpoint. `profileArn`,missing willreturn
    /// `400 {"message":"profileArn is required for this request."}`.Enterprise/IdC
    /// accountofreal ARN first by `resolve_profile_arn_for` backfill; pure BuilderID account has none
    /// canparseofreal profile, by official IDE send behavior BuilderID placeholder.
    ///
    /// - already has explicit profileArn(real ARN / Social ARN / BuilderID placeholder)→ return as is;
    /// - not yet filled → infer the default by the login method ARN(Social → Social ARN, rest → BuilderID);
    /// - API Key credential none profileArn concept → return `None`.
    pub fn streaming_profile_arn(&self) -> Option<String> {
        if self.is_api_key_credential() {
            return None;
        }
        Some(
            self.profile_arn
                .clone()
                .unwrap_or_else(|| self.default_profile_arn().to_string()),
        )
    }
}

/// determine given profileArn whether is BuilderID placeholder (not a real usable one profile).
pub fn is_placeholder_profile_arn(arn: &str) -> bool {
    arn == BUILDER_ID_PROFILE_ARN
}

#[cfg(test)]
impl KiroCredentials {
    fn from_json(json_string: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_string)
    }

    fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::Config;

    #[test]
    fn test_from_json() {
        let json = r#"{
            "accessToken": "test_token",
            "refreshToken": "test_refresh",
            "profileArn": "arn:aws:test",
            "expiresAt": "2024-01-01T00:00:00Z",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.access_token, Some("test_token".to_string()));
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.profile_arn, Some("arn:aws:test".to_string()));
        assert_eq!(creds.expires_at, Some("2024-01-01T00:00:00Z".to_string()));
        assert_eq!(creds.auth_method, Some("social".to_string()));
    }

    #[test]
    fn test_from_json_with_unknown_keys() {
        let json = r#"{
            "accessToken": "test_token",
            "unknownField": "should be ignored"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.access_token, Some("test_token".to_string()));
    }

    #[test]
    fn test_to_json() {
        let creds = KiroCredentials {
            id: None,
            access_token: Some("token".to_string()),
            refresh_token: None,
            profile_arn: None,
            expires_at: None,
            auth_method: Some("social".to_string()),
            provider: None,
            client_id: None,
            client_secret: None,
            start_url: None,
            priority: 0,
            region: None,
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            kiro_api_key: None,
            endpoint: None,
            groups: vec![],
            source_channel: None,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("accessToken"));
        assert!(json.contains("authMethod"));
        assert!(!json.contains("refreshToken"));
        // priority as 0 whennotserialize
        assert!(!json.contains("priority"));
    }

    #[test]
    fn test_default_credentials_path() {
        assert_eq!(
            KiroCredentials::default_credentials_path(),
            "credentials.json"
        );
    }

    #[test]
    fn test_is_placeholder_profile_arn() {
        assert!(is_placeholder_profile_arn(BUILDER_ID_PROFILE_ARN));
        assert!(!is_placeholder_profile_arn(SOCIAL_PROFILE_ARN));
        assert!(!is_placeholder_profile_arn(
            "arn:aws:codewhisperer:us-east-1:123456789012:profile/REAL123"
        ));
    }

    #[test]
    fn test_effective_profile_arn_skips_placeholder() {
        // BuilderID placeholder → None(not sent to the upstream)
        let mut cred = KiroCredentials::default();
        cred.profile_arn = Some(BUILDER_ID_PROFILE_ARN.to_string());
        assert_eq!(cred.effective_profile_arn(), None);

        // Social share ARN → return as is
        cred.profile_arn = Some(SOCIAL_PROFILE_ARN.to_string());
        assert_eq!(cred.effective_profile_arn(), Some(SOCIAL_PROFILE_ARN));

        // real Enterprise ARN → return as is
        let real = "arn:aws:codewhisperer:us-east-1:123456789012:profile/REAL123";
        cred.profile_arn = Some(real.to_string());
        assert_eq!(cred.effective_profile_arn(), Some(real));

        // none ARN → None
        cred.profile_arn = None;
        assert_eq!(cred.effective_profile_arn(), None);
    }

    #[test]
    fn test_streaming_profile_arn_includes_placeholder() {
        // streaming endpoint: explicitly BuilderID The placeholder is sent as is; a missing one is by upstream 400 reject
        let mut cred = KiroCredentials::default();
        cred.profile_arn = Some(BUILDER_ID_PROFILE_ARN.to_string());
        assert_eq!(
            cred.streaming_profile_arn().as_deref(),
            Some(BUILDER_ID_PROFILE_ARN)
        );

        // real ARN send as is
        let real = "arn:aws:codewhisperer:us-east-1:123456789012:profile/REAL123";
        cred.profile_arn = Some(real.to_string());
        assert_eq!(cred.streaming_profile_arn().as_deref(), Some(real));

        // not filled + non social(BuilderID account)→ fallback BuilderID placeholder
        let mut builder = KiroCredentials::default();
        builder.profile_arn = None;
        builder.refresh_token = Some("r".to_string());
        assert_eq!(
            builder.streaming_profile_arn().as_deref(),
            Some(BUILDER_ID_PROFILE_ARN)
        );

        // not filled + social → fallback Social share ARN(not a placeholder, sent as is)
        let mut social = KiroCredentials::default();
        social.profile_arn = None;
        social.auth_method = Some("social".to_string());
        assert_eq!(
            social.streaming_profile_arn().as_deref(),
            Some(SOCIAL_PROFILE_ARN)
        );

        // API Key credential none profileArn concept → None
        let mut api = KiroCredentials::default();
        api.kiro_api_key = Some("ksk_xxx".to_string());
        assert_eq!(api.streaming_profile_arn(), None);
    }

    #[test]
    fn test_priority_default() {
        let json = r#"{"refreshToken": "test"}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.priority, 0);
    }

    #[test]
    fn test_priority_explicit() {
        let json = r#"{"refreshToken": "test", "priority": 5}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.priority, 5);
    }

    #[test]
    fn test_credentials_config_single() {
        let json = r#"{"refreshToken": "test", "expiresAt": "2025-12-31T00:00:00Z"}"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, CredentialsConfig::Single(_)));
    }

    #[test]
    fn test_credentials_config_multiple() {
        let json = r#"[
            {"refreshToken": "test1", "priority": 1},
            {"refreshToken": "test2", "priority": 0}
        ]"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, CredentialsConfig::Multiple(_)));
        assert_eq!(config.into_sorted_credentials().len(), 2);
    }

    #[test]
    fn test_credentials_config_priority_sorting() {
        let json = r#"[
            {"refreshToken": "t1", "priority": 2},
            {"refreshToken": "t2", "priority": 0},
            {"refreshToken": "t3", "priority": 1}
        ]"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        // validate sorting by priority
        assert_eq!(list[0].refresh_token, Some("t2".to_string())); // priority 0
        assert_eq!(list[1].refresh_token, Some("t3".to_string())); // priority 1
        assert_eq!(list[2].refresh_token, Some("t1".to_string())); // priority 2
    }

    // ============ Region field test ============

    #[test]
    fn test_region_field_parsing() {
        // testparsecontains region field JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "region": "us-east-1"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.region, Some("us-east-1".to_string()));
    }

    #[test]
    fn test_region_field_missing_backward_compat() {
        // test backward compatibility: does not include region fieldold format JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.region, None);
    }

    #[test]
    fn test_region_field_serialization() {
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            provider: None,
            client_id: None,
            client_secret: None,
            start_url: None,
            priority: 0,
            region: Some("eu-west-1".to_string()),
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            kiro_api_key: None,
            endpoint: None,
            groups: vec![],
            source_channel: None,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("region"));
        assert!(json.contains("eu-west-1"));
    }

    #[test]
    fn test_region_field_none_not_serialized() {
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            provider: None,
            client_id: None,
            client_secret: None,
            start_url: None,
            priority: 0,
            region: None,
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            kiro_api_key: None,
            endpoint: None,
            groups: vec![],
            source_channel: None,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("region"));
    }

    // ============ MachineId field test ============

    #[test]
    fn test_machine_id_field_parsing() {
        let machine_id = "a".repeat(64);
        let json = format!(
            r#"{{
                "refreshToken": "test_refresh",
                "machineId": "{machine_id}"
            }}"#
        );

        let creds = KiroCredentials::from_json(&json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.machine_id, Some(machine_id));
    }

    #[test]
    fn test_machine_id_field_serialization() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.machine_id = Some("b".repeat(64));

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("machineId"));
    }

    #[test]
    fn test_machine_id_field_none_not_serialized() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.machine_id = None;

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("machineId"));
    }

    #[test]
    fn test_multiple_credentials_with_different_regions() {
        // Tests that in the multi credential case different credentials use their own region
        let json = r#"[
            {"refreshToken": "t1", "region": "us-east-1"},
            {"refreshToken": "t2", "region": "eu-west-1"},
            {"refreshToken": "t3"}
        ]"#;

        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        assert_eq!(list[0].region, Some("us-east-1".to_string()));
        assert_eq!(list[1].region, Some("eu-west-1".to_string()));
        assert_eq!(list[2].region, None);
    }

    #[test]
    fn test_region_field_with_all_fields() {
        // Tests the complete one containing all fields. JSON
        let json = r#"{
            "id": 1,
            "accessToken": "access",
            "refreshToken": "refresh",
            "profileArn": "arn:aws:test",
            "expiresAt": "2025-12-31T00:00:00Z",
            "authMethod": "idc",
            "clientId": "client123",
            "clientSecret": "secret456",
            "priority": 5,
            "region": "ap-northeast-1"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.id, Some(1));
        assert_eq!(creds.access_token, Some("access".to_string()));
        assert_eq!(creds.refresh_token, Some("refresh".to_string()));
        assert_eq!(creds.profile_arn, Some("arn:aws:test".to_string()));
        assert_eq!(creds.expires_at, Some("2025-12-31T00:00:00Z".to_string()));
        assert_eq!(creds.auth_method, Some("idc".to_string()));
        assert_eq!(creds.client_id, Some("client123".to_string()));
        assert_eq!(creds.client_secret, Some("secret456".to_string()));
        assert_eq!(creds.priority, 5);
        assert_eq!(creds.region, Some("ap-northeast-1".to_string()));
    }

    #[test]
    fn test_region_roundtrip() {
        // Tests the round trip consistency of serialization and deserialization.
        let original = KiroCredentials {
            id: Some(42),
            access_token: Some("token".to_string()),
            refresh_token: Some("refresh".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: Some("social".to_string()),
            provider: None,
            client_id: None,
            client_secret: None,
            start_url: None,
            priority: 3,
            region: Some("us-west-2".to_string()),
            auth_region: None,
            api_region: None,
            machine_id: Some("c".repeat(64)),
            email: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            kiro_api_key: None,
            endpoint: None,
            groups: vec![],
            source_channel: None,
        };

        let json = original.to_pretty_json().unwrap();
        let parsed = KiroCredentials::from_json(&json).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.access_token, original.access_token);
        assert_eq!(parsed.refresh_token, original.refresh_token);
        assert_eq!(parsed.priority, original.priority);
        assert_eq!(parsed.region, original.region);
        assert_eq!(parsed.machine_id, original.machine_id);
    }

    // ============ auth_region / api_region field test ============

    #[test]
    fn test_auth_region_field_parsing() {
        let json = r#"{
            "refreshToken": "test_refresh",
            "authRegion": "eu-central-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.auth_region, Some("eu-central-1".to_string()));
        assert_eq!(creds.api_region, None);
    }

    #[test]
    fn test_api_region_field_parsing() {
        let json = r#"{
            "refreshToken": "test_refresh",
            "apiRegion": "ap-southeast-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.api_region, Some("ap-southeast-1".to_string()));
        assert_eq!(creds.auth_region, None);
    }

    #[test]
    fn test_auth_api_region_serialization() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.auth_region = Some("eu-west-1".to_string());
        creds.api_region = Some("us-west-2".to_string());

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("authRegion"));
        assert!(json.contains("eu-west-1"));
        assert!(json.contains("apiRegion"));
        assert!(json.contains("us-west-2"));
    }

    #[test]
    fn test_auth_api_region_none_not_serialized() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.auth_region = None;
        creds.api_region = None;

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("authRegion"));
        assert!(!json.contains("apiRegion"));
    }

    #[test]
    fn test_auth_api_region_roundtrip() {
        let mut original = KiroCredentials::default();
        original.refresh_token = Some("refresh".to_string());
        original.region = Some("us-east-1".to_string());
        original.auth_region = Some("eu-west-1".to_string());
        original.api_region = Some("ap-northeast-1".to_string());

        let json = original.to_pretty_json().unwrap();
        let parsed = KiroCredentials::from_json(&json).unwrap();

        assert_eq!(parsed.region, original.region);
        assert_eq!(parsed.auth_region, original.auth_region);
        assert_eq!(parsed.api_region, original.api_region);
    }

    #[test]
    fn test_backward_compat_no_auth_api_region() {
        // old format JSON does not contain authRegion/apiRegion,shouldnormalparse
        let json = r#"{
            "refreshToken": "test_refresh",
            "region": "us-east-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.region, Some("us-east-1".to_string()));
        assert_eq!(creds.auth_region, None);
        assert_eq!(creds.api_region, None);
    }

    // ============ effective_auth_region / effective_api_region prioritytest ============

    #[test]
    fn test_effective_auth_region_credential_auth_region_highest() {
        // credential.auth_region > credential.region > config.auth_region > config.region
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());
        creds.auth_region = Some("cred-auth-region".to_string());

        assert_eq!(creds.effective_auth_region(&config), "cred-auth-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_credential_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());
        // auth_region unset

        assert_eq!(creds.effective_auth_region(&config), "cred-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_config_auth_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let creds = KiroCredentials::default();
        // auth_region and region all unset

        assert_eq!(creds.effective_auth_region(&config), "config-auth-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_config_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        // config.auth_region unset

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_auth_region(&config), "config-region");
    }

    #[test]
    fn test_effective_api_region_credential_api_region_highest() {
        // credential.api_region > config.api_region > config.region
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.api_region = Some("config-api-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.api_region = Some("cred-api-region".to_string());

        assert_eq!(creds.effective_api_region(&config), "cred-api-region");
    }

    #[test]
    fn test_effective_api_region_fallback_to_config_api_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.api_region = Some("config-api-region".to_string());

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_api_region(&config), "config-api-region");
    }

    #[test]
    fn test_effective_api_region_fallback_to_config_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_api_region(&config), "config-region");
    }

    #[test]
    fn test_effective_api_region_ignores_credential_region() {
        // credential.region do not participate api_region fallback chain
        let mut config = Config::default();
        config.region = "config-region".to_string();

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());

        assert_eq!(creds.effective_api_region(&config), "config-region");
    }

    #[test]
    fn test_auth_and_api_region_independent() {
        // auth_region and api_region do not affect each other
        let mut config = Config::default();
        config.region = "default".to_string();

        let mut creds = KiroCredentials::default();
        creds.auth_region = Some("auth-only".to_string());
        creds.api_region = Some("api-only".to_string());

        assert_eq!(creds.effective_auth_region(&config), "auth-only");
        assert_eq!(creds.effective_api_region(&config), "api-only");
    }

    // ============ credential level proxy priority test ============

    #[test]
    fn test_effective_proxy_credential_overrides_global() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("socks5://cred:1080".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, Some(ProxyConfig::new("socks5://cred:1080")));
    }

    #[test]
    fn test_effective_proxy_credential_with_auth() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("http://proxy:3128".to_string());
        creds.proxy_username = Some("user".to_string());
        creds.proxy_password = Some("pass".to_string());

        let result = creds.effective_proxy(Some(&global));
        let expected = ProxyConfig::new("http://proxy:3128").with_auth("user", "pass");
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_effective_proxy_direct_bypasses_global() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("direct".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, None);
    }

    #[test]
    fn test_effective_proxy_direct_case_insensitive() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("DIRECT".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, None);
    }

    #[test]
    fn test_effective_proxy_fallback_to_global() {
        let global = ProxyConfig::new("http://global:8080");
        let creds = KiroCredentials::default();

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, Some(ProxyConfig::new("http://global:8080")));
    }

    #[test]
    fn test_effective_proxy_none_when_no_proxy() {
        let creds = KiroCredentials::default();
        let result = creds.effective_proxy(None);
        assert_eq!(result, None);
    }
}
