//! Token management module
//!
//! responsible Token expiry detection and refresh, supports Social and IdC authmethod
//! supportmultiple credentials (MultiTokenManager) manage

use anyhow::bail;
use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex as TokioMutex;

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration as StdDuration, Instant};

use crate::http_client::{ProxyConfig, build_client};
use crate::kiro::kiro_version::USAGE_API_KIRO_VERSION;
use crate::kiro::machine_id;
use crate::kiro::model::available_models::ListAvailableModelsResponse;
use crate::kiro::model::available_profiles::ListAvailableProfilesResponse;
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::model::token_refresh::{
    IdcRefreshRequest, IdcRefreshResponse, RefreshRequest, RefreshResponse,
};
use crate::kiro::model::usage_limits::UsageLimitsResponse;
use crate::model::config::Config;

/// check Token whether it expires within the specified time
pub(crate) fn is_token_expiring_within(
    credentials: &KiroCredentials,
    minutes: i64,
) -> Option<bool> {
    credentials
        .expires_at
        .as_ref()
        .and_then(|expires_at| DateTime::parse_from_rfc3339(expires_at).ok())
        .map(|expires| expires <= Utc::now() + Duration::minutes(minutes))
}

/// check Token whether it has expired (in advance 5 minutesdecision)
pub(crate) fn is_token_expired(credentials: &KiroCredentials) -> bool {
    is_token_expiring_within(credentials, 5).unwrap_or(true)
}

/// check Token whether it is about to expire (10within minutes)
pub(crate) fn is_token_expiring_soon(credentials: &KiroCredentials) -> bool {
    is_token_expiring_within(credentials, 10).unwrap_or(false)
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// generate API Key masked display(before 4 + ... + after 4,lengthnotenoughornon ASCII fallback ***)
fn mask_api_key(key: &str) -> String {
    if key.is_ascii() && key.len() > 16 {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "***".to_string()
    }
}

/// verify refreshToken ofbasichasvalidity
pub(crate) fn validate_refresh_token(credentials: &KiroCredentials) -> anyhow::Result<()> {
    let refresh_token = credentials
        .refresh_token
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing refreshToken"))?;

    if refresh_token.is_empty() {
        bail!("refreshToken is empty");
    }

    if refresh_token.len() < 100 || refresh_token.ends_with("...") || refresh_token.contains("...")
    {
        bail!(
            "refreshToken has been truncated (length: {} characters).\n\
             this is usually Kiro IDE Deliberately truncated to prevent the credential from being used by third party tools.",
            refresh_token.len()
        );
    }

    Ok(())
}

/// Refresh Token permanently invaliderror
///
/// whenserviceend returns 400 + `invalid_grant` when, means refreshToken has been revoked or expired,
/// Should not retry; the corresponding credential must be disabled immediately.
#[derive(Debug)]
pub(crate) struct RefreshTokenInvalidError {
    pub message: String,
}

impl fmt::Display for RefreshTokenInvalidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RefreshTokenInvalidError {}

/// refresh Token
pub(crate) async fn refresh_token(
    credentials: &KiroCredentials,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroCredentials> {
    // API Key credentialnot supported Token refresh: low level contract level interception
    // othercallpoint(try_ensure_token / active path / add_credential) explicitly routed before the call API Key;
    // only force_refresh_token_for unassignedstream,here bail let the error propagate naturally as 400 BAD_REQUEST.
    if credentials.is_api_key_credential() {
        bail!("API Key the credential does not support refresh Token");
    }

    validate_refresh_token(credentials)?;

    // based on auth_method selectrefreshmethod
    // ifnot yetspecified auth_method,based onwhether has clientId/clientSecret auto determine
    let auth_method = credentials.auth_method.as_deref().unwrap_or_else(|| {
        if credentials.client_id.is_some() && credentials.client_secret.is_some() {
            "idc"
        } else {
            "social"
        }
    });

    if auth_method.eq_ignore_ascii_case("idc")
        || auth_method.eq_ignore_ascii_case("builder-id")
        || auth_method.eq_ignore_ascii_case("iam")
    {
        refresh_idc_token(credentials, config, proxy).await
    } else {
        refresh_social_token(credentials, config, proxy).await
    }
}

/// refresh Social Token
async fn refresh_social_token(
    credentials: &KiroCredentials,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroCredentials> {
    tracing::info!("refreshing Social Token...");

    let refresh_token = credentials.refresh_token.as_ref().unwrap();
    // priority:credential.auth_region > credential.region > config.auth_region > config.region
    let region = credentials.effective_auth_region(config);

    let refresh_url = format!("https://prod.{}.auth.desktop.kiro.dev/refreshToken", region);
    let refresh_domain = format!("prod.{}.auth.desktop.kiro.dev", region);
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    let kiro_version = crate::kiro::kiro_version::effective(&config.kiro_version);

    let client = build_client(proxy, 60, config.tls_backend)?;
    let body = RefreshRequest {
        refresh_token: refresh_token.to_string(),
    };

    let response = client
        .post(&refresh_url)
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            format!("KiroIDE-{}-{}", kiro_version, machine_id),
        )
        .header("Accept-Encoding", "gzip, compress, deflate, br")
        .header("host", &refresh_domain)
        .header("Connection", "close")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();

        // 400 + invalid_grant + Invalid refresh token provided → refreshToken permanently invalid
        if status.as_u16() == 400
            && body_text.contains("\"invalid_grant\"")
            && body_text.contains("Invalid refresh token provided")
        {
            return Err(RefreshTokenInvalidError {
                message: format!("Social refreshToken invalid (invalid_grant): {}", body_text),
            }
            .into());
        }

        let error_msg = match status.as_u16() {
            401 => "OAuth The credential has expired or is invalid; re-authentication is needed.",
            403 => "insufficient permission, cannot refresh Token",
            429 => "Requests too frequent; has been throttled.",
            500..=599 => "servicecomponenterror,AWS OAuth the service is temporarily unavailable",
            _ => "Token refreshfailed",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    let data: RefreshResponse = response.json().await?;

    let mut new_credentials = credentials.clone();
    new_credentials.access_token = Some(data.access_token);

    if let Some(new_refresh_token) = data.refresh_token {
        new_credentials.refresh_token = Some(new_refresh_token);
    }

    if let Some(profile_arn) = data.profile_arn {
        new_credentials.profile_arn = Some(profile_arn);
    }

    if let Some(expires_in) = data.expires_in {
        let expires_at = Utc::now() + Duration::seconds(expires_in);
        new_credentials.expires_at = Some(expires_at.to_rfc3339());
    }

    Ok(new_credentials)
}

/// refresh IdC Token (AWS SSO OIDC)
async fn refresh_idc_token(
    credentials: &KiroCredentials,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroCredentials> {
    tracing::info!("refreshing IdC Token...");

    let refresh_token = credentials.refresh_token.as_ref().unwrap();
    let client_id = credentials
        .client_id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("IdC refresh needs clientId"))?;
    let client_secret = credentials
        .client_secret
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("IdC refresh needs clientSecret"))?;

    // priority:credential.auth_region > credential.region > config.auth_region > config.region
    let region = credentials.effective_auth_region(config);
    let refresh_url = format!("https://oidc.{}.amazonaws.com/token", region);
    let os_name = &config.system_version;
    let node_version = &config.node_version;

    let x_amz_user_agent = "aws-sdk-js/3.980.0 KiroIDE";
    let user_agent = format!(
        "aws-sdk-js/3.980.0 ua/2.1 os/{} lang/js md/nodejs#{} api/sso-oidc#3.980.0 m/E KiroIDE",
        os_name, node_version
    );

    let client = build_client(proxy, 60, config.tls_backend)?;
    let body = IdcRefreshRequest {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        refresh_token: refresh_token.to_string(),
        grant_type: "refresh_token".to_string(),
    };

    let response = client
        .post(&refresh_url)
        .header("content-type", "application/json")
        .header("x-amz-user-agent", x_amz_user_agent)
        .header("user-agent", &user_agent)
        .header("host", format!("oidc.{}.amazonaws.com", region))
        .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
        .header("amz-sdk-request", "attempt=1; max=4")
        .header("Connection", "close")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();

        // 400 + invalid_grant + Invalid refresh token provided → refreshToken permanently invalid
        if status.as_u16() == 400
            && body_text.contains("\"invalid_grant\"")
            && body_text.contains("Invalid refresh token provided")
        {
            return Err(RefreshTokenInvalidError {
                message: format!("IdC refreshToken invalid (invalid_grant): {}", body_text),
            }
            .into());
        }

        let error_msg = match status.as_u16() {
            401 => "IdC The credential has expired or is invalid; re-authentication is needed.",
            403 => "insufficient permission, cannot refresh Token",
            429 => "Requests too frequent; has been throttled.",
            500..=599 => "servicecomponenterror,AWS OIDC the service is temporarily unavailable",
            _ => "IdC Token refreshfailed",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    let data: IdcRefreshResponse = response.json().await?;

    let mut new_credentials = credentials.clone();
    new_credentials.access_token = Some(data.access_token);

    if let Some(new_refresh_token) = data.refresh_token {
        new_credentials.refresh_token = Some(new_refresh_token);
    }

    if let Some(expires_in) = data.expires_in {
        let expires_at = Utc::now() + Duration::seconds(expires_in);
        new_credentials.expires_at = Some(expires_at.to_rfc3339());
    }

    // sync update profile_arn(if IdC responseincontains)
    if let Some(profile_arn) = data.profile_arn {
        new_credentials.profile_arn = Some(profile_arn);
    }

    Ok(new_credentials)
}

/// official Kiro usage / model REST interface (getUsageLimits / ListAvailableModels /
/// setUserPreference) only in `us-east-1` and `eu-central-1` two endpoints provide service.
///
/// based on credentialof SSO Region selects the primary endpoint and returns another endpoint as 403 fallbackcandidate:
/// - `eu-central-1` or any `eu-*` region → main endpoint `eu-central-1`
/// - other regions → main endpoint `us-east-1`
///
/// import this wayof Enterprise / IAM Identity Center (IdC) account even if SSO region is not
/// `us-east-1`, can still hit the correct endpoint, avoiding `403 {"message":"Invalid token"}`.
fn rest_api_region_candidates(sso_region: &str) -> [&'static str; 2] {
    let primary_eu = sso_region == "eu-central-1" || sso_region.starts_with("eu-");
    if primary_eu {
        ["eu-central-1", "us-east-1"]
    } else {
        ["us-east-1", "eu-central-1"]
    }
}

/// get the usage quota information
pub(crate) async fn get_usage_limits(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<UsageLimitsResponse> {
    tracing::debug!("obtaining usage quota information...");

    // getUsageLimits only in us-east-1 / eu-central-1 provideservice,
    // based on credential SSO the region selects the primary endpoint,403 falls back to another endpoint.
    let sso_region = credentials.effective_auth_region(config);
    let candidates = rest_api_region_candidates(sso_region);
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    // the usage type interface fixedly uses USAGE_API_KIRO_VERSION: new version IDE will forcerequire profileArn,
    // for Enterprise/IdC account failed; this version does not need profileArn.
    let kiro_version = USAGE_API_KIRO_VERSION;
    let os_name = &config.system_version;
    let node_version = &config.node_version;

    // profileArn query string: only send the real ARN, skip BuilderID placeholder
    let profile_arn_query = credentials
        .effective_profile_arn()
        .map(|arn| format!("&profileArn={}", urlencoding::encode(arn)))
        .unwrap_or_default();

    // build User-Agent headers
    let user_agent = format!(
        "aws-sdk-js/1.0.0 ua/2.1 os/{} lang/js md/nodejs#{} api/codewhispererruntime#1.0.0 m/N,E KiroIDE-{}-{}",
        os_name, node_version, kiro_version, machine_id
    );
    let amz_user_agent = format!("aws-sdk-js/1.0.0 KiroIDE-{}-{}", kiro_version, machine_id);

    let client = build_client(proxy, 60, config.tls_backend)?;

    let mut last_error: Option<String> = None;
    for (idx, region) in candidates.iter().enumerate() {
        let host = format!("q.{}.amazonaws.com", region);
        let url = format!(
            "https://{}/getUsageLimits?origin=AI_EDITOR&resourceType=AGENTIC_REQUEST&isEmailRequired=true{}",
            host, profile_arn_query
        );

        let mut request = client
            .get(&url)
            .header("x-amz-user-agent", &amz_user_agent)
            .header("user-agent", &user_agent)
            .header("host", &host)
            .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
            .header("amz-sdk-request", "attempt=1; max=1")
            .header("Authorization", format!("Bearer {}", token))
            .header("Connection", "close");

        if credentials.is_api_key_credential() {
            request = request.header("tokentype", "API_KEY");
        }

        let response = request.send().await?;

        let status = response.status();
        if status.is_success() {
            let data: UsageLimitsResponse = response.json().await?;
            return Ok(data);
        }

        let body_text = response.text().await.unwrap_or_default();

        // 403 and while backup endpoints remain, tries the next regional endpoint (Enterprise/IdC acrossregioncompatible)
        if status.as_u16() == 403 && idx + 1 < candidates.len() {
            tracing::debug!(
                "getUsageLimits in {} return 403, try the backup endpoint {}",
                region,
                candidates[idx + 1]
            );
            last_error = Some(format!("{} {}", status, body_text));
            continue;
        }

        let error_msg = match status.as_u16() {
            401 => "authfailed,Token invalid or expired",
            403 => "Insufficient permission; cannot obtain the usage quota.",
            429 => "Requests too frequent; has been throttled.",
            500..=599 => "servicecomponenterror,AWS the service is temporarily unavailable",
            _ => "failed to get the usage quota",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    // All candidate endpoints failed (in theory already within the loop return / bail, here is the fallback)
    bail!(
        "Insufficient permission; cannot obtain the usage quota.: {}",
        last_error.unwrap_or_else(|| "noneavailableendpoint".to_string())
    );
}

/// Gets the model list currently available for the credential.
///
/// upstreaminterface:`GET https://q.{api_region}.amazonaws.com/ListAvailableModels?origin=AI_EDITOR`
/// The return value differs by subscription tier (such as FREE account does not contain Opus).
/// the request headers and construction method are with [`get_usage_limits`] fully consistent.
pub(crate) async fn get_available_models(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<ListAvailableModelsResponse> {
    tracing::debug!("obtaining the available model list...");

    // ListAvailableModels only in us-east-1 / eu-central-1 provideservice,
    // based on credential SSO the region selects the primary endpoint,403 falls back to another endpoint.
    let sso_region = credentials.effective_auth_region(config);
    let candidates = rest_api_region_candidates(sso_region);
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    let kiro_version = USAGE_API_KIRO_VERSION;
    let os_name = &config.system_version;
    let node_version = &config.node_version;

    // profileArn query string: only send the real ARN, skip BuilderID placeholder
    let profile_arn_query = credentials
        .effective_profile_arn()
        .map(|arn| format!("&profileArn={}", urlencoding::encode(arn)))
        .unwrap_or_default();

    // build User-Agent headers(with get_usage_limits keepconsistent)
    let user_agent = format!(
        "aws-sdk-js/1.0.0 ua/2.1 os/{} lang/js md/nodejs#{} api/codewhispererruntime#1.0.0 m/N,E KiroIDE-{}-{}",
        os_name, node_version, kiro_version, machine_id
    );
    let amz_user_agent = format!("aws-sdk-js/1.0.0 KiroIDE-{}-{}", kiro_version, machine_id);

    let client = build_client(proxy, 60, config.tls_backend)?;

    let mut last_error: Option<String> = None;
    for (idx, region) in candidates.iter().enumerate() {
        let host = format!("q.{}.amazonaws.com", region);
        let url = format!(
            "https://{}/ListAvailableModels?origin=AI_EDITOR{}",
            host, profile_arn_query
        );

        let mut request = client
            .get(&url)
            .header("x-amz-user-agent", &amz_user_agent)
            .header("user-agent", &user_agent)
            .header("host", &host)
            .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
            .header("amz-sdk-request", "attempt=1; max=1")
            .header("Authorization", format!("Bearer {}", token))
            .header("Connection", "close");

        if credentials.is_api_key_credential() {
            request = request.header("tokentype", "API_KEY");
        }

        let response = request.send().await?;

        let status = response.status();
        if status.is_success() {
            let data: ListAvailableModelsResponse = response.json().await?;
            return Ok(data);
        }

        let body_text = response.text().await.unwrap_or_default();

        // 403 and while backup endpoints remain, tries the next regional endpoint (Enterprise/IdC acrossregioncompatible)
        if status.as_u16() == 403 && idx + 1 < candidates.len() {
            tracing::debug!(
                "ListAvailableModels in {} return 403, try the backup endpoint {}",
                region,
                candidates[idx + 1]
            );
            last_error = Some(format!("{} {}", status, body_text));
            continue;
        }

        let error_msg = match status.as_u16() {
            401 => "authfailed,Token invalid or expired",
            403 => "Insufficient permission; cannot obtain the available models.",
            429 => "Requests too frequent; has been throttled.",
            500..=599 => "servicecomponenterror,AWS the service is temporarily unavailable",
            _ => "failed to get the available models",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    // All candidate endpoints failed (in theory already within the loop return / bail, here is the fallback)
    bail!(
        "Insufficient permission; cannot obtain the available models.: {}",
        last_error.unwrap_or_else(|| "noneavailableendpoint".to_string())
    );
}

/// get the real one available for this credential profileArn list (`ListAvailableProfiles`).
///
/// Enterprise / IAM Identity Center (IdC) the account must use a real profileArn call the streaming endpoint;
/// this ARN neither is BuilderID placeholder, and not in OIDC Returned in the refresh response; can only be obtained through this interface.
///
/// upstreaminterface (AWS JSON 1.0,**andusagetypeof REST GET different**):
/// `POST https://q.{region}.amazonaws.com/`, request header
/// `x-amz-target: AmazonCodeWhispererService.ListAvailableProfiles`,
/// `Content-Type: application/x-amz-json-1.0`,Body `{"maxResults":N}`.
///
/// and [`get_usage_limits`] same only in `us-east-1` / `eu-central-1` provideservice,
/// based on credential SSO Region selects the primary endpoint; the primary endpoint did not return profile falls back to another endpoint.
pub(crate) async fn list_available_profiles(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<ListAvailableProfilesResponse> {
    tracing::debug!("properinget available profile list...");

    let sso_region = credentials.effective_auth_region(config);
    let candidates = rest_api_region_candidates(sso_region);
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    let kiro_version = USAGE_API_KIRO_VERSION;
    let os_name = &config.system_version;
    let node_version = &config.node_version;

    let user_agent = format!(
        "aws-sdk-js/1.0.0 ua/2.1 os/{} lang/js md/nodejs#{} api/codewhispererruntime#1.0.0 m/N,E KiroIDE-{}-{}",
        os_name, node_version, kiro_version, machine_id
    );
    let amz_user_agent = format!("aws-sdk-js/1.0.0 KiroIDE-{}-{}", kiro_version, machine_id);

    let client = build_client(proxy, 60, config.tls_backend)?;

    let mut last_error: Option<String> = None;
    let mut empty_seen = false;
    for region in candidates.iter() {
        let host = format!("q.{}.amazonaws.com", region);
        let url = format!("https://{}/", host);

        let mut request = client
            .post(&url)
            .header("content-type", "application/x-amz-json-1.0")
            .header(
                "x-amz-target",
                "AmazonCodeWhispererService.ListAvailableProfiles",
            )
            .header("x-amz-user-agent", &amz_user_agent)
            .header("user-agent", &user_agent)
            .header("host", &host)
            .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
            .header("amz-sdk-request", "attempt=1; max=1")
            .header("Authorization", format!("Bearer {}", token))
            .header("Connection", "close")
            .body(r#"{"maxResults":10}"#);

        if credentials.is_api_key_credential() {
            request = request.header("tokentype", "API_KEY");
        }

        let response = request.send().await?;
        let status = response.status();

        if status.is_success() {
            let data: ListAvailableProfilesResponse = response.json().await?;
            // region has none profile tries another regional endpoint (the account may be in eu-central-1)
            if data.first_arn().is_none() {
                empty_seen = true;
                continue;
            }
            return Ok(data);
        }

        let body_text = response.text().await.unwrap_or_default();
        last_error = Some(format!("{} {}", status, body_text));
        // 403 and similar errors, continues to try the next candidate endpoint.
    }

    // no endpoint returned profile: if at least one succeeded but is empty, treated as"account has none Enterprise profile"
    // (BuilderID and so on), returns an empty result so the caller falls back to the placeholder logic.
    if empty_seen {
        return Ok(ListAvailableProfilesResponse::default());
    }

    bail!(
        "get available profile failed: {}",
        last_error.unwrap_or_else(|| "noneavailableendpoint".to_string())
    );
}

/// set user preference (enable/closeoverage)
///
/// upstreaminterface:`POST https://q.{region}.amazonaws.com/setUserPreference`
/// Body: `{ "overageConfiguration": { "overageStatus": "ENABLED" | "DISABLED" }, "profileArn": "..." }`
pub(crate) async fn set_user_preference(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
    overage_status: &str, // "ENABLED" or "DISABLED"
) -> anyhow::Result<()> {
    tracing::debug!("setting user preference overageStatus={}", overage_status);

    // setUserPreference only in us-east-1 / eu-central-1 provideservice,
    // based on credential SSO the region selects the primary endpoint,403 falls back to another endpoint.
    let sso_region = credentials.effective_auth_region(config);
    let candidates = rest_api_region_candidates(sso_region);
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    let kiro_version = USAGE_API_KIRO_VERSION;
    let os_name = &config.system_version;
    let node_version = &config.node_version;

    let user_agent = format!(
        "aws-sdk-js/1.0.0 ua/2.1 os/{} lang/js md/nodejs#{} api/codewhispererruntime#1.0.0 m/N,E KiroIDE-{}-{}",
        os_name, node_version, kiro_version, machine_id
    );
    let amz_user_agent = format!("aws-sdk-js/1.0.0 KiroIDE-{}-{}", kiro_version, machine_id);

    let client = build_client(proxy, 60, config.tls_backend)?;

    // build body: onlysendreal profileArn, skip BuilderID placeholder
    let body = if let Some(profile_arn) = credentials.effective_profile_arn() {
        serde_json::json!({
            "overageConfiguration": { "overageStatus": overage_status },
            "profileArn": profile_arn,
        })
    } else {
        serde_json::json!({
            "overageConfiguration": { "overageStatus": overage_status },
        })
    };

    let mut last_error: Option<String> = None;
    for (idx, region) in candidates.iter().enumerate() {
        let host = format!("q.{}.amazonaws.com", region);
        let url = format!("https://{}/setUserPreference", host);

        let mut request = client
            .post(&url)
            .header("x-amz-user-agent", &amz_user_agent)
            .header("user-agent", &user_agent)
            .header("host", &host)
            .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
            .header("amz-sdk-request", "attempt=1; max=1")
            .header("Authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .header("Connection", "close")
            .json(&body);

        if credentials.is_api_key_credential() {
            request = request.header("tokentype", "API_KEY");
        }

        let response = request.send().await?;

        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        let body_text = response.text().await.unwrap_or_default();

        // 403 and while backup endpoints remain, tries the next regional endpoint (Enterprise/IdC acrossregioncompatible)
        if status.as_u16() == 403 && idx + 1 < candidates.len() {
            tracing::debug!(
                "setUserPreference in {} return 403, try the backup endpoint {}",
                region,
                candidates[idx + 1]
            );
            last_error = Some(format!("{} {}", status, body_text));
            continue;
        }

        let error_msg = match status.as_u16() {
            400 => "Request parameter error; the account may not support overage.",
            401 => "authfailed,Token invalid or expired",
            403 => "Insufficient permission; cannot set user preferences.",
            429 => "Requests too frequent; has been throttled.",
            500..=599 => "servicecomponenterror,AWS the service is temporarily unavailable",
            _ => "failed to set user preference",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    // All candidate endpoints failed (in theory already within the loop return / bail, here is the fallback)
    bail!(
        "Insufficient permission; cannot set user preferences.: {}",
        last_error.unwrap_or_else(|| "noneavailableendpoint".to_string())
    );
}

// ============================================================================
// multiple credentials Token manager
// ============================================================================

/// the status of a single credential entry
struct CredentialEntry {
    /// credentialunique ID
    id: u64,
    /// credential info
    credentials: KiroCredentials,
    /// API consecutive call failure count
    failure_count: u32,
    /// API Cumulative call failure count (includes all failure types: auth,/quota/throttle/transient/network).
    /// Only increases, never decreases; success does not clear it, only a manual reset of the failure count zeroes it. Used only for display and troubleshooting.
    total_failure_count: u64,
    /// Token consecutive refresh failure count
    refresh_failure_count: u32,
    /// iswhetherdisabled
    disabled: bool,
    /// Disable reason (used to distinguish a manual disable vs auto disable, to facilitate self healing)
    disabled_reason: Option<DisabledReason>,
    /// API call successtimescount
    success_count: u64,
    /// mostafteronce API calltime(RFC3339 format)
    last_used_at: Option<String>,
    /// Temporary cooldown expiry time (account level). 429 skips the credential briefly after throttle triggers)
    /// `Some(t)` and `t > now()` treated as unavailable when;`t <= now()` whenautomatic recovery.
    /// Not persisted; cleared after a process restart.
    throttled_until: Option<Instant>,
}

/// disableoriginalbecause
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisabledReason {
    /// Admin API manually disable
    Manual,
    /// Auto disables after consecutive failures reach the threshold.
    TooManyFailures,
    /// Token Auto disables after consecutive refresh failures reach the threshold.
    TooManyRefreshFailures,
    /// the quota is exhausted (such as MONTHLY_REQUEST_COUNT)
    QuotaExceeded,
    /// Refresh Token permanent invalidation (the server returns invalid_grant)
    InvalidRefreshToken,
    /// the credential configuration is invalid (such as authMethod=api_key but missing kiroApiKey)
    InvalidConfig,
}

/// statistics data persistence entry
#[derive(Serialize, Deserialize)]
struct StatsEntry {
    success_count: u64,
    #[serde(default)]
    total_failure_count: u64,
    last_used_at: Option<String>,
}

// ============================================================================
// Admin API public struct
// ============================================================================

/// credential entry snapshot (used for Admin API read)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialEntrySnapshot {
    /// credentialunique ID
    pub id: u64,
    /// priority
    pub priority: u32,
    /// iswhetherbydisable
    pub disabled: bool,
    /// consecutivefailedtimescount
    pub failure_count: u32,
    /// Cumulative failure count (all failure types, only increases, zeroed only by a manual reset).
    pub total_failure_count: u64,
    /// authmethod
    pub auth_method: Option<String>,
    /// identityprovidevendor(BuilderId / Enterprise / Github / Google / IAM_SSO)
    pub provider: Option<String>,
    /// whether has Profile ARN
    pub has_profile_arn: bool,
    /// Token expiry time
    pub expires_at: Option<String>,
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
    /// Temporary cooldown remaining seconds (account level). 429 throttle); in cooldown and `> 0` return only then
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttled_remaining_secs: Option<u64>,
    /// Endpoint name (returned when not explicitly configured None, by Admin the layer falls back to the default value)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// The groups the account belongs to (may belong to multiple).
    #[serde(default)]
    pub groups: Vec<String>,
    /// Account source channel (a plain note).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_channel: Option<String>,
}

/// credential manager state snapshot
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerSnapshot {
    /// credentialentryentrylist
    pub entries: Vec<CredentialEntrySnapshot>,
    /// currentactivecredential ID
    pub current_id: u64,
    /// totalcredential count
    pub total: usize,
    /// availablecredential count
    pub available: usize,
}

/// multiple credentials Token manager
///
/// Supports managing multiple credentials, implementing fixed priority. + failover strategy
/// failurestatisticsbased on API the call result, rather than Token refresh result
pub struct MultiTokenManager {
    config: Config,
    /// Global proxy (changeable at runtime).
    proxy: Mutex<Option<ProxyConfig>>,
    /// credentialentryentrylist
    entries: Mutex<Vec<CredentialEntry>>,
    /// currentactivitycredential ID
    current_id: Mutex<u64>,
    /// the next credential to be assigned ID. Monotonically increases in process, avoiding a new account reusing an old one after an account is deleted. ID,
    /// thereby inherit the old account by credential_id aggregated trace/usage history.
    next_id: AtomicU64,
    /// Token Refresh lock, ensuring only one refresh operation at a time.
    refresh_lock: TokioMutex<()>,
    /// Credential file path (used for write back).
    credentials_path: Option<PathBuf>,
    /// the credential file write lock.`persist_credentials` Uses full file overwrite; concurrent calls would step on each other,
    /// So this lock serializes all disk write operations (batch import and similar cases can trigger concurrently).
    persist_lock: Mutex<()>,
    /// Whether it is the multi credential format (write back only in array format; via add_credential dynamicupgradeas true)
    is_multiple_format: AtomicBool,
    /// Load balancing mode (changeable at runtime).
    load_balancing_mode: Mutex<String>,
    /// account level 429 Throttle failover switch (changeable at runtime).
    account_throttle_failover: AtomicBool,
    /// Account level throttle cooldown duration (seconds, changeable at runtime).
    account_throttle_cooldown_secs: AtomicU64,
    /// The time of the most recent statistics persistence (used for debounce)
    last_stats_save_at: Mutex<Option<Instant>>,
    /// Whether the statistics have unsaved updates.
    stats_dirty: AtomicBool,
}

/// eachitemcredentialmaximum API callfailedtimescount
const MAX_FAILURES_PER_CREDENTIAL: u32 = 3;
/// Statistics persistence debounce interval.
const STATS_SAVE_DEBOUNCE: StdDuration = StdDuration::from_secs(30);

/// API callcontext
///
/// The call context bound to a specific credential, ensuring token,credentials and id consistency
/// used to resolve during concurrent calls current_id race condition
#[derive(Clone)]
pub struct CallContext {
    /// credential ID(used for report_success/report_failure)
    pub id: u64,
    /// Credential info (used to build the request headers).
    pub credentials: KiroCredentials,
    /// access Token
    pub token: String,
}

/// Determines whether an account group set matches the request group (strict isolation).
///
/// - `group = None`:Key not bound to a group (including master apiKey), match all accounts.
/// - `group = Some(g)`: only match `cred_groups` contains `g` account.
fn group_matches(cred_groups: &[String], group: Option<&str>) -> bool {
    match group {
        None => true,
        Some(g) => cred_groups.iter().any(|cg| cg == g),
    }
}

fn credential_matches_request(
    credentials: &KiroCredentials,
    model: Option<&str>,
    group: Option<&str>,
) -> bool {
    let is_opus = model
        .map(|m| m.to_ascii_lowercase().contains("opus"))
        .unwrap_or(false);

    if is_opus && !credentials.supports_opus() {
        return false;
    }

    group_matches(&credentials.groups, group)
}

impl MultiTokenManager {
    /// createmultiple credentials Token manager
    ///
    /// # Arguments
    /// * `config` - app config
    /// * `credentials` - credential list
    /// * `proxy` - optional proxy configuration
    /// * `credentials_path` - Credential file path (used for write back).
    /// * `is_multiple_format` - Whether it is the multi credential format (write back only in array format).
    pub fn new(
        config: Config,
        credentials: Vec<KiroCredentials>,
        proxy: Option<ProxyConfig>,
        credentials_path: Option<PathBuf>,
        is_multiple_format: bool,
    ) -> anyhow::Result<Self> {
        // computecurrentmaximum ID, for those without ID ofcredentialallocate new ID
        let max_existing_id = credentials.iter().filter_map(|c| c.id).max().unwrap_or(0);
        let mut next_id = max_existing_id + 1;
        let mut has_new_ids = false;
        let mut has_new_machine_ids = false;
        let config_ref = &config;

        let entries: Vec<CredentialEntry> = credentials
            .into_iter()
            .map(|mut cred| {
                cred.canonicalize_auth_method();
                let id = cred.id.unwrap_or_else(|| {
                    let id = next_id;
                    next_id += 1;
                    cred.id = Some(id);
                    has_new_ids = true;
                    id
                });
                if cred.fill_default_profile_arn() {
                    has_new_ids = true;
                }
                if cred.machine_id.is_none() {
                    cred.machine_id =
                        Some(machine_id::generate_from_credentials(&cred, config_ref));
                    has_new_machine_ids = true;
                }
                CredentialEntry {
                    id,
                    credentials: cred.clone(),
                    failure_count: 0,
                    total_failure_count: 0,
                    refresh_failure_count: 0,
                    disabled: cred.disabled, // read from the configuration file disabled state
                    disabled_reason: if cred.disabled {
                        Some(DisabledReason::Manual)
                    } else {
                        None
                    },
                    success_count: 0,
                    last_used_at: None,
                    throttled_until: None,
                }
            })
            .collect();

        // validate API Key credential configuration integrity:authMethod=api_key whenmustprovide kiroApiKey
        let mut entries = entries;
        for entry in &mut entries {
            if entry.credentials.kiro_api_key.is_none()
                && entry
                    .credentials
                    .auth_method
                    .as_deref()
                    .map(|m| m.eq_ignore_ascii_case("api_key") || m.eq_ignore_ascii_case("apikey"))
                    .unwrap_or(false)
            {
                tracing::warn!(
                    "credential #{} configured authMethod=api_key but missing kiroApiKey field, has been automatically disabled",
                    entry.id
                );
                entry.disabled = true;
                entry.disabled_reason = Some(DisabledReason::InvalidConfig);
            }
        }

        // detect duplicate ID
        let mut seen_ids = std::collections::HashSet::new();
        let mut duplicate_ids = Vec::new();
        for entry in &entries {
            if !seen_ids.insert(entry.id) {
                duplicate_ids.push(entry.id);
            }
        }
        if !duplicate_ids.is_empty() {
            anyhow::bail!("detected a duplicate credential ID: {:?}", duplicate_ids);
        }

        // Selects the initial credential: highest priority (priority smallest) available credential; when none is available it is 0
        let initial_id = entries
            .iter()
            .filter(|e| !e.disabled)
            .min_by_key(|e| e.credentials.priority)
            .map(|e| e.id)
            .unwrap_or(0);

        let load_balancing_mode = config.load_balancing_mode.clone();
        let throttle_failover = config.account_throttle_failover;
        let throttle_cooldown_secs = config.account_throttle_cooldown_secs;
        let manager = Self {
            config,
            proxy: Mutex::new(proxy),
            entries: Mutex::new(entries),
            current_id: Mutex::new(initial_id),
            next_id: AtomicU64::new(next_id),
            refresh_lock: TokioMutex::new(()),
            credentials_path,
            persist_lock: Mutex::new(()),
            is_multiple_format: AtomicBool::new(is_multiple_format),
            load_balancing_mode: Mutex::new(load_balancing_mode),
            account_throttle_failover: AtomicBool::new(throttle_failover),
            account_throttle_cooldown_secs: AtomicU64::new(throttle_cooldown_secs),
            last_stats_save_at: Mutex::new(None),
            stats_dirty: AtomicBool::new(false),
        };

        // Single credential format auto migration: upgrades to array format, ensuring token rotation can write disk
        // Trigger condition: the original file is single object format. && credential exists && hasfilepath
        if !is_multiple_format
            && !manager.entries.lock().is_empty()
            && manager.credentials_path.is_some()
        {
            manager.is_multiple_format.store(true, Ordering::Relaxed);
            if let Err(e) = manager.persist_credentials() {
                tracing::warn!("Migrating the single credential format to array format failed.: {}", e);
            } else {
                tracing::info!(
                    "Migrated the credential file from single object format to array format,token rotation willcorrectpersist"
                );
            }
        }

        // if there is a newly assigned ID ornewgenerateof machineId, immediately persists to the config file.
        if has_new_ids || has_new_machine_ids {
            if let Err(e) = manager.persist_credentials() {
                tracing::warn!("complete credential ID/machineId afterpersistfailed: {}", e);
            } else {
                tracing::info!("alreadycomplete credential ID/machineId and write back to the configuration file");
            }
        }

        // Loads the persisted statistics (success_count, last_used_at)
        manager.load_stats();

        Ok(manager)
    }

    /// get a reference to the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Gets a clone of the global proxy config (safe to use across locks).
    pub fn proxy(&self) -> Option<ProxyConfig> {
        self.proxy.lock().clone()
    }

    /// Sets the global proxy config (runtime change, can pass None clear)
    pub fn set_global_proxy(&self, proxy: Option<ProxyConfig>) {
        *self.proxy.lock() = proxy;
    }

    /// fetchcredentialtotalcount
    pub fn total_count(&self) -> usize {
        self.entries.lock().len()
    }

    /// Gets the total credential count of the given group (group=None equals when total_count)
    ///
    /// used to compute by group failover Retry budget, avoiding a small group getting too many wasted retries based on the global account count.
    pub fn total_count_in_group(&self, group: Option<&str>) -> usize {
        self.entries
            .lock()
            .iter()
            .filter(|e| group_matches(&e.credentials.groups, group))
            .count()
    }

    /// get the number of available credentials
    pub fn available_count(&self) -> usize {
        let now = Instant::now();
        self.entries
            .lock()
            .iter()
            .filter(|e| !e.disabled && !e.throttled_until.map(|t| t > now).unwrap_or(false))
            .count()
    }

    /// Selects the next credential based on the load balancing mode.
    ///
    /// - priority Mode: selects the highest priority (priority the minimum) available credential
    /// - balanced Mode: balances the selection among available credentials.
    ///
    /// # parameter
    /// - `model`: Optional model name, used to filter credentials that support the model (such as opus the model requires a paid subscription)
    fn select_next_credential(&self, model: Option<&str>, group: Option<&str>) -> Option<(u64, KiroCredentials)> {
        let entries = self.entries.lock();
        let now = Instant::now();

        // filteravailablecredential
        let available: Vec<_> = entries
            .iter()
            .filter(|e| {
                if e.disabled {
                    return false;
                }
                // in temporary cooldown (account level 429 throttle):skip
                if e.throttled_until.map(|t| t > now).unwrap_or(false) {
                    return false;
                }
                // model/Group isolation: the requested model must be supported by the account, and the account must match the request group.
                if !credential_matches_request(&e.credentials, model, group) {
                    return false;
                }
                true
            })
            .collect();

        if available.is_empty() {
            return None;
        }

        let mode = self.load_balancing_mode.lock().clone();
        let mode = mode.as_str();

        match mode {
            "balanced" => {
                // Least-Used Strategy: selects the credential with the fewest successes.
                // On a tie, sorts by priority (a smaller number means higher priority).
                let entry = available
                    .iter()
                    .min_by_key(|e| (e.success_count, e.credentials.priority))?;

                Some((entry.id, entry.credentials.clone()))
            }
            _ => {
                // priority Mode (default): selects the highest priority one.
                let entry = available.iter().min_by_key(|e| e.credentials.priority)?;
                Some((entry.id, entry.credentials.clone()))
            }
        }
    }

    /// fetch API callcontext
    ///
    /// returnbound id,credentials and token ofcallcontext
    /// ensure whole API Uses consistent credential info throughout the call.
    ///
    /// if Token Expired or about to expire; auto refreshes.
    /// Token Refresh failures accumulate on the current credential; when the threshold is reached, it is disabled and switched.
    ///
    /// # parameter
    /// - `model`: Optional model name, used to filter credentials that support the model (such as opus the model requires a paid subscription)
    pub async fn acquire_context(&self, model: Option<&str>, group: Option<&str>) -> anyhow::Result<CallContext> {
        let total = self.total_count_in_group(group);
        let max_attempts = (total * MAX_FAILURES_PER_CREDENTIAL as usize).max(1);
        let mut attempt_count = 0;

        loop {
            if attempt_count >= max_attempts {
                anyhow::bail!(
                    "None of the credentials can obtain a valid one. Token(available: {}/{})",
                    self.available_count(),
                    total
                );
            }

            let (id, credentials) = {
                let is_balanced = self.load_balancing_mode.lock().as_str() == "balanced";

                // balanced Mode: each request rebalances the selection, not fixed. current_id
                // priority mode: use first current_id point toofcredential
                let current_hit = if is_balanced {
                    None
                } else {
                    let entries = self.entries.lock();
                    let current_id = *self.current_id.lock();
                    let now = Instant::now();
                    entries
                        .iter()
                        .find(|e| {
                            e.id == current_id
                                && !e.disabled
                                && !e.throttled_until.map(|t| t > now).unwrap_or(false)
                                && credential_matches_request(&e.credentials, model, group)
                        })
                        .map(|e| (e.id, e.credentials.clone()))
                };

                if let Some(hit) = current_hit {
                    hit
                } else {
                    // the current credential is unavailable or balanced Mode, selects based on the load balancing strategy.
                    let mut best = self.select_next_credential(model, group);

                    // no available credentials: if it is"auto disable causes total loss", performs a self healing similar to a restart.
                    if best.is_none() {
                        let mut entries = self.entries.lock();
                        if entries.iter().any(|e| {
                            e.disabled && e.disabled_reason == Some(DisabledReason::TooManyFailures)
                        }) {
                            tracing::warn!(
                                "All credentials have been automatically disabled; performs self healing: resets failure counts and re-enables (equivalent to a restart)."
                            );
                            for e in entries.iter_mut() {
                                if e.disabled_reason == Some(DisabledReason::TooManyFailures) {
                                    e.disabled = false;
                                    e.disabled_reason = None;
                                    e.failure_count = 0;
                                }
                            }
                            drop(entries);
                            best = self.select_next_credential(model, group);
                        }
                    }

                    if let Some((new_id, new_creds)) = best {
                        // update current_id
                        let mut current_id = self.current_id.lock();
                        *current_id = new_id;
                        (new_id, new_creds)
                    } else {
                        let entries = self.entries.lock();
                        // note:must be in bail! compute before available_count,
                        // because available_count() willtry to get entries lock,
                        // and at this point we already hold that lock, which would cause a deadlock.
                        let available = entries.iter().filter(|e| !e.disabled).count();
                        anyhow::bail!("all credentials are disabled ({}/{})", available, total);
                    }
                }
            };

            // try to get/refresh Token
            match self.try_ensure_token(id, &credentials).await {
                Ok(ctx) => {
                    return Ok(ctx);
                }
                Err(e) => {
                    let has_available = if e.downcast_ref::<RefreshTokenInvalidError>().is_some() {
                        // First tries to reload from the source file (applicable to IDE after exit token rotation the scenario causing invalidation)
                        if self.try_reload_credential_from_file(id) {
                            // find new Token, not counted in the failure count, retries directly.
                            continue;
                        }
                        tracing::warn!("credential #{} refreshToken permanently invalid: {}", id, e);
                        self.report_refresh_token_invalid(id)
                    } else {
                        tracing::warn!("credential #{} Token refreshfailed: {}", id, e);
                        self.report_refresh_failure(id)
                    };
                    attempt_count += 1;
                    if !has_available {
                        anyhow::bail!("all credentials are disabled (0/{})", total);
                    }
                }
            }
        }
    }

    /// Selects the highest priority non disabled credential as the current credential (internal method).
    ///
    /// Selects purely by priority without excluding the current credential; used to take effect immediately after a priority change.
    fn select_highest_priority(&self) {
        let entries = self.entries.lock();
        let mut current_id = self.current_id.lock();

        // Selects the highest priority non disabled credential (without excluding the current credential).
        if let Some(best) = entries
            .iter()
            .filter(|e| !e.disabled)
            .min_by_key(|e| e.credentials.priority)
        {
            if best.id != *current_id {
                tracing::info!(
                    "switch credential after priority change: #{} -> #{}(priority {})",
                    *current_id,
                    best.id,
                    best.credentials.priority
                );
                *current_id = best.id;
            }
        }
    }

    /// Tries to use the given credential to obtain a valid Token
    ///
    /// Uses the double checked locking pattern to ensure only one refresh operation at a time.
    ///
    /// # Arguments
    /// * `id` - credential ID, used to update the correct entry
    /// * `credentials` - credential info
    async fn try_ensure_token(
        &self,
        id: u64,
        credentials: &KiroCredentials,
    ) -> anyhow::Result<CallContext> {
        // API Key credentialdirectlyuse kiro_api_key as Bearer Token, no needrefresh
        if credentials.is_api_key_credential() {
            let token = credentials
                .kiro_api_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("API Key credential missing kiroApiKey"))?;
            return Ok(CallContext {
                id,
                credentials: credentials.clone(),
                token,
            });
        }

        // First check (lock free): quickly determines whether a refresh is needed.
        let needs_refresh = is_token_expired(credentials) || is_token_expiring_soon(credentials);

        let creds = if needs_refresh {
            // Acquires the refresh lock to ensure only one refresh operation at a time.
            let _guard = self.refresh_lock.lock().await;

            // Second check: after acquiring the lock, re-read the credential, because another request may have already finished the refresh.
            let current_creds = {
                let entries = self.entries.lock();
                entries
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.credentials.clone())
                    .ok_or_else(|| anyhow::anyhow!("credential #{} does not exist", id))?
            };

            if is_token_expired(&current_creds) || is_token_expiring_soon(&current_creds) {
                // indeedneedrefresh
                let global_proxy = self.proxy.lock().clone();
                let effective_proxy = current_creds.effective_proxy(global_proxy.as_ref());
                let new_creds =
                    refresh_token(&current_creds, &self.config, effective_proxy.as_ref()).await?;

                if is_token_expired(&new_creds) {
                    anyhow::bail!("refreshed Token still invalid or expired");
                }

                // update credential
                {
                    let mut entries = self.entries.lock();
                    if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                        entry.credentials = new_creds.clone();
                    }
                }

                // Writes credentials back to the file (multi credential format only); on failure only logs a warning.
                if let Err(e) = self.persist_credentials() {
                    tracing::warn!("Token Persistence failed after refresh (does not affect this request).: {}", e);
                }

                new_creds
            } else {
                // Another request has already finished the refresh; uses the new credential directly.
                tracing::debug!("Token Already refreshed by another request; skips the refresh.");
                current_creds
            }
        } else {
            credentials.clone()
        };

        let token = creds
            .access_token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("noneavailableof accessToken"))?;

        {
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                entry.refresh_failure_count = 0;
            }
        }

        Ok(CallContext {
            id,
            credentials: creds,
            token,
        })
    }

    /// Writes the credential list back to the source file.
    ///
    /// Writes back only when the following conditions are met:
    /// - The source file is the multi credential format (array).
    /// - credentials_path set
    ///
    /// # Returns
    /// - `Ok(true)` - successwritefile
    /// - `Ok(false)` - Skips the write (not the multi credential format or no path configured).
    /// - `Err(_)` - writefailed
    fn persist_credentials(&self) -> anyhow::Result<bool> {
        use anyhow::Context;

        // only the multi credential format writes back
        if !self.is_multiple_format.load(Ordering::Relaxed) {
            return Ok(false);
        }

        let path = match &self.credentials_path {
            Some(p) => p,
            None => return Ok(false),
        };

        // collectallcredential
        let credentials: Vec<KiroCredentials> = {
            let entries = self.entries.lock();
            entries
                .iter()
                .map(|e| {
                    let mut cred = e.credentials.clone();
                    cred.canonicalize_auth_method();
                    // sync disabled state to the credential object
                    cred.disabled = e.disabled;
                    cred
                })
                .collect()
        };

        // serialize to pretty JSON
        let json = serde_json::to_string_pretty(&credentials).context("failed to serialize the credential")?;

        // writefile(in Tokio runtime use within block_in_place avoid blocking worker)
        // hold persist_lock Serializes full file overwrites to avoid disk writes stepping on each other in concurrent cases like batch import.
        let _write_guard = self.persist_lock.lock();
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| std::fs::write(path, &json))
                .with_context(|| format!("failed to write back the credential file: {:?}", path))?;
        } else {
            std::fs::write(path, &json).with_context(|| format!("failed to write back the credential file: {:?}", path))?;
        }

        tracing::debug!("credential written back to the file: {:?}", path);
        Ok(true)
    }

    /// Tries to reload the given credential from the credential file. Token
    ///
    /// when refreshToken invalid (invalid_grant) checks whether the source file has been updated by another client.
    /// (for examplelocal IDE exitwhenrefreshdone Token, causing token rotation).
    /// if a different one exists in the file refreshToken, update the in memory credential and return true.
    ///
    /// # matching rule (by priority)
    /// 1. in the file and the in memory credential `id` identicalentry
    /// 2. in the file and the in memory credential `email` identicalentry
    /// 3. When both the file and memory have only one credential, matches directly.
    ///
    /// # update scope
    /// only update token related fieldsegment (refreshToken / accessToken / expiresAt),
    /// retainproxy,region,machineId etc.confignotchange.
    fn try_reload_credential_from_file(&self, id: u64) -> bool {
        use crate::kiro::model::credentials::CredentialsConfig;

        let path = match self.credentials_path.as_ref() {
            Some(p) => p.clone(),
            None => return false,
        };

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let file_config: CredentialsConfig = match serde_json::from_str(&content) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let file_creds = file_config.into_sorted_credentials();
        if file_creds.is_empty() {
            return false;
        }

        // First reads the current credential identity info (without holding the lock, to avoid deadlock).
        let (current_cred_id, current_email, current_refresh_token, entries_len) = {
            let entries = self.entries.lock();
            match entries.iter().find(|e| e.id == id) {
                Some(entry) => (
                    entry.credentials.id,
                    entry.credentials.email.clone(),
                    entry.credentials.refresh_token.clone(),
                    entries.len(),
                ),
                None => return false,
            }
        };

        // find the corresponding credential in the file
        let matched = file_creds
            .iter()
            .find(|fc| {
                if fc.id.is_some() && fc.id == current_cred_id {
                    return true;
                }
                if fc.email.is_some() && fc.email == current_email {
                    return true;
                }
                false
            })
            .or_else(|| {
                if file_creds.len() == 1 && entries_len == 1 {
                    file_creds.first()
                } else {
                    None
                }
            });

        let file_cred = match matched {
            Some(c) => c,
            None => return false,
        };

        // in file refreshToken Must exist and differ from the current one to be worth updating.
        if file_cred.refresh_token.is_none() || file_cred.refresh_token == current_refresh_token {
            return false;
        }

        let new_refresh_token = file_cred.refresh_token.clone();
        let new_access_token = file_cred.access_token.clone();
        let new_expires_at = file_cred.expires_at.clone();

        {
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                entry.credentials.refresh_token = new_refresh_token;
                entry.credentials.access_token = new_access_token;
                entry.credentials.expires_at = new_expires_at;
                entry.disabled = false;
                entry.disabled_reason = None;
                entry.refresh_failure_count = 0;
                entry.failure_count = 0;
            }
        }

        tracing::info!(
            "credential #{} detected new from the file refreshToken(suspected IDE token rotation), has auto recovered and will retry.",
            id
        );
        true
    }

    /// Gets the cache directory (the directory of the credential file).
    pub fn cache_dir(&self) -> Option<PathBuf> {
        self.credentials_path.as_ref().and_then(|p| {
            p.parent().map(|d| {
                // when a relative path is passed such as "credentials.json"(without a directory prefix) when parent is empty string,
                // directly join the extracted sub path will land in CWD, and read_dir("") Would error and cause the history log to be rebuilt as 0.
                // herenormalizeas ".", ensure join / read_dir behaviorcorrect.
                if d.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    d.to_path_buf()
                }
            })
        })
    }

    /// statistics data file path
    fn stats_path(&self) -> Option<PathBuf> {
        self.cache_dir().map(|d| d.join("kiro_stats.json"))
    }

    /// Loads statistics from disk and applies them to the current entries.
    fn load_stats(&self) {
        let path = match self.stats_path() {
            Some(p) => p,
            None => return,
        };

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return, // the file does not exist on first run
        };

        let stats: HashMap<String, StatsEntry> = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Parsing the statistics cache failed; will ignore it.: {}", e);
                return;
            }
        };

        let mut entries = self.entries.lock();
        for entry in entries.iter_mut() {
            if let Some(s) = stats.get(&entry.id.to_string()) {
                entry.success_count = s.success_count;
                entry.total_failure_count = s.total_failure_count;
                entry.last_used_at = s.last_used_at.clone();
            }
        }
        *self.last_stats_save_at.lock() = Some(Instant::now());
        self.stats_dirty.store(false, Ordering::Relaxed);
        tracing::info!("alreadyfromcacheload {} entrystatisticsdata", stats.len());
    }

    /// Persists the current statistics to disk.
    fn save_stats(&self) {
        let path = match self.stats_path() {
            Some(p) => p,
            None => return,
        };

        let stats: HashMap<String, StatsEntry> = {
            let entries = self.entries.lock();
            entries
                .iter()
                .map(|e| {
                    (
                        e.id.to_string(),
                        StatsEntry {
                            success_count: e.success_count,
                            total_failure_count: e.total_failure_count,
                            last_used_at: e.last_used_at.clone(),
                        },
                    )
                })
                .collect()
        };

        match serde_json::to_string_pretty(&stats) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::warn!("failed to save the statistics cache: {}", e);
                } else {
                    *self.last_stats_save_at.lock() = Some(Instant::now());
                    self.stats_dirty.store(false, Ordering::Relaxed);
                }
            }
            Err(e) => tracing::warn!("failed to serialize statistics data: {}", e),
        }
    }

    /// Marks the statistics as updated, and by debounce the policy decides whether to persist immediately
    fn save_stats_debounced(&self) {
        self.stats_dirty.store(true, Ordering::Relaxed);

        let should_flush = {
            let last = *self.last_stats_save_at.lock();
            match last {
                Some(last_saved_at) => last_saved_at.elapsed() >= STATS_SAVE_DEBOUNCE,
                None => true,
            }
        };

        if should_flush {
            self.save_stats();
        }
    }

    /// reportspecifiedcredential API call success
    ///
    /// reset the failure count of this credential
    ///
    /// # Arguments
    /// * `id` - credential ID(from CallContext)
    pub fn report_success(&self, id: u64) {
        {
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                entry.failure_count = 0;
                entry.refresh_failure_count = 0;
                entry.success_count += 1;
                entry.last_used_at = Some(Utc::now().to_rfc3339());
                // success = Throttle has been cleared; ends the cooldown early.
                entry.throttled_until = None;
                tracing::debug!(
                    "credential #{} API call succeeded (cumulative {} times)",
                    id,
                    entry.success_count
                );
            }
        }
        self.save_stats_debounced();
    }

    /// reportspecifiedcredential API callfailed
    ///
    /// Increments the failure count; when the threshold is reached, disables the credential and switches to the highest priority available credential.
    /// Returns whether there is still an available credential to retry.
    ///
    /// # Arguments
    /// * `id` - credential ID(from CallContext)
    pub fn report_failure(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();
            let mut current_id = self.current_id.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.failure_count += 1;
            entry.total_failure_count += 1;
            entry.last_used_at = Some(Utc::now().to_rfc3339());
            let failure_count = entry.failure_count;

            tracing::warn!(
                "credential #{} API callfailed({}/{})",
                id,
                failure_count,
                MAX_FAILURES_PER_CREDENTIAL
            );

            if failure_count >= MAX_FAILURES_PER_CREDENTIAL {
                entry.disabled = true;
                entry.disabled_reason = Some(DisabledReason::TooManyFailures);
                tracing::error!("credential #{} alreadyconsecutivefailed {} times,alreadybydisable", id, failure_count);

                // Switches to the highest priority available credential.
                if let Some(next) = entries
                    .iter()
                    .filter(|e| !e.disabled)
                    .min_by_key(|e| e.credentials.priority)
                {
                    *current_id = next.id;
                    tracing::info!(
                        "alreadyswitchtocredential #{}(priority {})",
                        next.id,
                        next.credentials.priority
                    );
                } else {
                    tracing::error!("all credentials are disabled!");
                }
            }

            entries.iter().any(|e| !e.disabled)
        };
        self.save_stats_debounced();
        result
    }

    /// Reports that the given credential quota is exhausted.
    ///
    /// used to handle 402 Payment Required and reason as `MONTHLY_REQUEST_COUNT` scenario:
    /// - Immediately disables the credential (without waiting for the consecutive failure threshold).
    /// - Switches to the next available credential and continues retrying.
    /// - return whether there are still available credentials
    pub fn report_quota_exhausted(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();
            let mut current_id = self.current_id.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.disabled = true;
            entry.disabled_reason = Some(DisabledReason::QuotaExceeded);
            entry.last_used_at = Some(Utc::now().to_rfc3339());
            // Set to the threshold, so the admin panel clearly shows the credential is unavailable.
            entry.failure_count = MAX_FAILURES_PER_CREDENTIAL;
            entry.total_failure_count += 1;

            tracing::error!(
                "credential #{} quotaalreadyuseexhaust(MONTHLY_REQUEST_COUNT or OVERAGE_REQUEST_LIMIT_EXCEEDED),alreadybydisable",
                id
            );

            // Switches to the highest priority available credential.
            if let Some(next) = entries
                .iter()
                .filter(|e| !e.disabled)
                .min_by_key(|e| e.credentials.priority)
            {
                *current_id = next.id;
                tracing::info!(
                    "alreadyswitchtocredential #{}(priority {})",
                    next.id,
                    next.credentials.priority
                );
                true
            } else {
                tracing::error!("all credentials are disabled!");
                false
            }
        };
        self.save_stats_debounced();
        result
    }

    /// report the specified credential refresh Token failed.
    ///
    /// After consecutive refresh failures reach the threshold, disables the credential and switches; within the threshold it keeps the current credential without switching,
    /// and API 401/403 keeps consistent with the cumulative failure strategy.
    pub fn report_refresh_failure(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();
            let mut current_id = self.current_id.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.last_used_at = Some(Utc::now().to_rfc3339());
            entry.refresh_failure_count += 1;
            let refresh_failure_count = entry.refresh_failure_count;

            tracing::warn!(
                "credential #{} Token refreshfailed({}/{})",
                id,
                refresh_failure_count,
                MAX_FAILURES_PER_CREDENTIAL
            );

            if refresh_failure_count < MAX_FAILURES_PER_CREDENTIAL {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.disabled = true;
            entry.disabled_reason = Some(DisabledReason::TooManyRefreshFailures);

            tracing::error!(
                "credential #{} Token consecutive refresh failures {} times,alreadybydisable",
                id,
                refresh_failure_count
            );

            if let Some(next) = entries
                .iter()
                .filter(|e| !e.disabled)
                .min_by_key(|e| e.credentials.priority)
            {
                *current_id = next.id;
                tracing::info!(
                    "alreadyswitchtocredential #{}(priority {})",
                    next.id,
                    next.credentials.priority
                );
                true
            } else {
                tracing::error!("all credentials are disabled!");
                false
            }
        };
        self.save_stats_debounced();
        result
    }

    /// report the specified credential refreshToken permanently invalid(invalid_grant).
    ///
    /// Immediately disables the credential, without accumulating and without retrying.
    /// Returns whether there is still an available credential.
    pub fn report_refresh_token_invalid(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();
            let mut current_id = self.current_id.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.last_used_at = Some(Utc::now().to_rfc3339());
            entry.disabled = true;
            entry.disabled_reason = Some(DisabledReason::InvalidRefreshToken);

            tracing::error!(
                "credential #{} refreshToken invalid (invalid_grant),alreadyimmediatelydisable",
                id
            );

            if let Some(next) = entries
                .iter()
                .filter(|e| !e.disabled)
                .min_by_key(|e| e.credentials.priority)
            {
                *current_id = next.id;
                tracing::info!(
                    "alreadyswitchtocredential #{}(priority {})",
                    next.id,
                    next.credentials.priority
                );
                true
            } else {
                tracing::error!("all credentials are disabled!");
                false
            }
        };
        self.save_stats_debounced();
        result
    }

    /// Switches to the highest priority available credential.
    ///
    /// return whether the switch succeeded
    pub fn switch_to_next(&self) -> bool {
        let entries = self.entries.lock();
        let mut current_id = self.current_id.lock();

        // Selects the highest priority non disabled credential (excluding the current credential).
        if let Some(next) = entries
            .iter()
            .filter(|e| !e.disabled && e.id != *current_id)
            .min_by_key(|e| e.credentials.priority)
        {
            *current_id = next.id;
            tracing::info!(
                "alreadyswitchtocredential #{}(priority {})",
                next.id,
                next.credentials.priority
            );
            true
        } else {
            // No other available credential; checks whether the current credential is usable.
            entries.iter().any(|e| e.id == *current_id && !e.disabled)
        }
    }

    // ========================================================================
    // Admin API method
    // ========================================================================

    /// Clones all credentials (including sensitive fields:refreshToken,accessToken,clientSecret etc.)
    ///
    /// only for Admin API Export case; the caller must ensure redaction and access control by itself.
    /// The return value is cloned in call order, not sorted.
    pub fn clone_all_credentials(&self) -> Vec<KiroCredentials> {
        let entries = self.entries.lock();
        entries
            .iter()
            .map(|e| {
                let mut cred = e.credentials.clone();
                cred.canonicalize_auth_method();
                cred.disabled = e.disabled;
                cred.id = Some(e.id);
                cred
            })
            .collect()
    }

    /// Gets a snapshot of the manager state (for Admin API)
    pub fn snapshot(&self) -> ManagerSnapshot {
        let entries = self.entries.lock();
        let current_id = *self.current_id.lock();
        let now = Instant::now();
        let available = entries
            .iter()
            .filter(|e| !e.disabled && !e.throttled_until.map(|t| t > now).unwrap_or(false))
            .count();

        ManagerSnapshot {
            entries: entries
                .iter()
                .map(|e| CredentialEntrySnapshot {
                    id: e.id,
                    priority: e.credentials.priority,
                    disabled: e.disabled,
                    failure_count: e.failure_count,
                    total_failure_count: e.total_failure_count,
                    auth_method: if e.credentials.is_api_key_credential() {
                        Some("api_key".to_string())
                    } else {
                        e.credentials.auth_method.as_deref().map(|m| {
                            if m.eq_ignore_ascii_case("builder-id") || m.eq_ignore_ascii_case("iam")
                            {
                                "idc".to_string()
                            } else {
                                m.to_string()
                            }
                        })
                    },
                    provider: if e.credentials.is_api_key_credential() {
                        None
                    } else {
                        e.credentials.provider.clone()
                    },
                    has_profile_arn: e.credentials.profile_arn.is_some(),
                    expires_at: if e.credentials.is_api_key_credential() {
                        None // API Key The credential does not keep an expiry locally (the server policy is unknown).
                    } else {
                        e.credentials.expires_at.clone()
                    },
                    refresh_token_hash: if e.credentials.is_api_key_credential() {
                        None
                    } else {
                        e.credentials.refresh_token.as_deref().map(sha256_hex)
                    },
                    api_key_hash: if e.credentials.is_api_key_credential() {
                        e.credentials.kiro_api_key.as_deref().map(sha256_hex)
                    } else {
                        None
                    },
                    masked_api_key: if e.credentials.is_api_key_credential() {
                        e.credentials.kiro_api_key.as_deref().map(mask_api_key)
                    } else {
                        None
                    },
                    email: e.credentials.email.clone(),
                    success_count: e.success_count,
                    last_used_at: e.last_used_at.clone(),
                    has_proxy: e.credentials.proxy_url.is_some(),
                    proxy_url: e.credentials.proxy_url.clone(),
                    refresh_failure_count: e.refresh_failure_count,
                    disabled_reason: e.disabled_reason.map(|r| {
                        match r {
                            DisabledReason::Manual => "Manual",
                            DisabledReason::TooManyFailures => "TooManyFailures",
                            DisabledReason::TooManyRefreshFailures => "TooManyRefreshFailures",
                            DisabledReason::QuotaExceeded => "QuotaExceeded",
                            DisabledReason::InvalidRefreshToken => "InvalidRefreshToken",
                            DisabledReason::InvalidConfig => "InvalidConfig",
                        }
                        .to_string()
                    }),
                    throttled_remaining_secs: e
                        .throttled_until
                        .and_then(|t| t.checked_duration_since(now))
                        .map(|d| d.as_secs())
                        .filter(|s| *s > 0),
                    endpoint: e.credentials.endpoint.clone(),
                    groups: e.credentials.groups.clone(),
                    source_channel: e.credentials.source_channel.clone(),
                })
                .collect(),
            current_id,
            total: entries.len(),
            available,
        }
    }

    /// set the credential disabled state (Admin API)
    pub fn set_disabled(&self, id: u64, disabled: bool) -> anyhow::Result<()> {
        {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?;
            entry.disabled = disabled;
            if !disabled {
                // reset the failure count when enabling
                entry.failure_count = 0;
                entry.refresh_failure_count = 0;
                entry.disabled_reason = None;
                entry.throttled_until = None;
            } else {
                entry.disabled_reason = Some(DisabledReason::Manual);
            }
        }
        // persistchange
        self.persist_credentials()?;
        Ok(())
    }

    /// Marks the credential entering a temporary cooldown period (account level). 429 throttletrigger)
    ///
    /// and `report_failure` Different: not counted toward permanent disable, auto recovers on expiry, can be used for"`suspicious activity` 429"
    /// this kind of short term account level throttle——the current credential cools down first N minutes, fails over to another credential.
    ///
    /// Returns the number of remaining available credentials (excluding those in cooldown).
    pub fn report_account_throttled(&self, id: u64, cooldown: StdDuration) -> usize {
        let now = Instant::now();
        {
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                let until = now + cooldown;
                // Takes the later expiry time (extends cooldown when triggered multiple times).
                entry.throttled_until = Some(match entry.throttled_until {
                    Some(prev) if prev > until => prev,
                    _ => until,
                });
                // Counts toward cumulative failures (account throttle does not touch the consecutive failure_count, avoiding a wrong disable after the cooldown ends)
                entry.total_failure_count += 1;
                tracing::warn!(
                    "credential #{} trigger account level throttle, cool down {} second",
                    id,
                    cooldown.as_secs()
                );
            }

            let throttled_now = Instant::now();
            entries
                .iter()
                .filter(|e| {
                    !e.disabled
                        && !e.throttled_until.map(|t| t > throttled_now).unwrap_or(false)
                })
                .count()
        }
    }

    /// Manually clears the temporary cooldown of the given credential (Admin API)
    ///
    /// Clears immediately even if the cooldown has not expired, letting the credential rejoin scheduling.
    pub fn clear_throttle(&self, id: u64) -> anyhow::Result<()> {
        let mut entries = self.entries.lock();
        let entry = entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?;
        entry.throttled_until = None;
        tracing::info!("credential #{} the throttle cooldown has been manually lifted", id);
        Ok(())
    }

    /// to"quotaalreadyuseexhaust"disable the credential for a reason (Admin one click overage feature)
    ///
    /// Different from a manual disable; the reason is recorded as `QuotaExceeded`, to facilitate self healing logic recognition.
    pub fn disable_quota_exceeded(&self, id: u64) -> anyhow::Result<()> {
        {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?;
            entry.disabled = true;
            entry.disabled_reason = Some(DisabledReason::QuotaExceeded);
        }
        self.persist_credentials()?;
        Ok(())
    }

    /// set the credential priority (Admin API)
    ///
    /// After changing priority, immediately reselects the current credential by the new priority.
    /// Even if persistence fails, the in-memory priority and current credential selection still take effect.
    pub fn set_priority(&self, id: u64, priority: u32) -> anyhow::Result<()> {
        {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?;
            entry.credentials.priority = priority;
        }
        // Immediately reselects the current credential by the new priority (regardless of whether persistence succeeds).
        self.select_highest_priority();
        // persistchange
        self.persist_credentials()?;
        Ok(())
    }

    /// Resets the credential failure count and re-enables (Admin API)
    pub fn reset_and_enable(&self, id: u64) -> anyhow::Result<()> {
        {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?;
            if entry.disabled_reason == Some(DisabledReason::InvalidConfig) {
                anyhow::bail!("credential #{} Disabled due to invalid config; please fix the config and restart the service.", id);
            }
            entry.failure_count = 0;
            entry.total_failure_count = 0;
            entry.refresh_failure_count = 0;
            entry.disabled = false;
            entry.disabled_reason = None;
            entry.throttled_until = None;
        }
        // persistchange
        self.persist_credentials()?;
        Ok(())
    }

    pub fn reset_success_count(&self, id: Option<u64>) -> anyhow::Result<u32> {
        let mut count = 0u32;
        {
            let mut entries = self.entries.lock();
            match id {
                Some(target_id) => {
                    let entry = entries
                        .iter_mut()
                        .find(|e| e.id == target_id)
                        .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", target_id))?;
                    entry.success_count = 0;
                    count = 1;
                }
                None => {
                    for entry in entries.iter_mut() {
                        entry.success_count = 0;
                        count += 1;
                    }
                }
            }
        }
        self.save_stats();
        Ok(count)
    }

    /// parseand backfill Enterprise / IdC accountofreal profileArn.
    ///
    /// streaming endpoint(`generateAssistantResponse`)forcerequire profileArn: without → 400
    /// `profileArn is required`.Enterprise / IdC if account has BuilderID placeholderwill because of
    /// token identity mismatch triggers 403, real profileArn only through `ListAvailableProfiles` fetch.
    ///
    /// behavior:
    /// - API Key credential / already has a real (non placeholder)profileArn → Returns directly, does not initiate a network request;
    /// - otherwisecallupstream `ListAvailableProfiles`,hit real ARN write back the credential and persist;
    /// - upstream none profile(such as pure BuilderID account)→ return `None`, the caller falls back to the placeholder.
    ///
    /// Returns the one that should be used for this request. profileArn(`Some` means real ARN).
    pub async fn resolve_profile_arn_for(
        &self,
        id: u64,
        token: &str,
    ) -> anyhow::Result<Option<String>> {
        use crate::kiro::model::credentials::is_placeholder_profile_arn;

        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
        };

        // API Key credential has none profileArn concept
        if credentials.is_api_key_credential() {
            return Ok(None);
        }

        // already has real ARN(including Social share ARN)→ use directly, no query needed
        if let Some(arn) = credentials.profile_arn.as_deref() {
            if !is_placeholder_profile_arn(arn) {
                return Ok(Some(arn.to_string()));
            }
        }

        let global_proxy = self.proxy.lock().clone();
        let effective_proxy = credentials.effective_proxy(global_proxy.as_ref());
        let profiles =
            list_available_profiles(&credentials, &self.config, token, effective_proxy.as_ref())
                .await?;

        let Some(arn) = profiles.first_arn().map(|s| s.to_string()) else {
            // none Enterprise profile(such as pure BuilderID account): keeps the placeholder fallback logic.
            return Ok(None);
        };

        // write back real ARN and persist
        {
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                entry.credentials.profile_arn = Some(arn.clone());
            }
        }
        if let Err(e) = self.persist_credentials() {
            tracing::warn!("profileArn Persistence failed after backfill (does not affect this request).: {}", e);
        }
        tracing::info!("credential #{} resolved and backfilled with the real profileArn: {}", id, arn);

        Ok(Some(arn))
    }

    /// Gets the usage quota of the given credential (Admin API)
    pub async fn get_usage_limits_for(&self, id: u64) -> anyhow::Result<UsageLimitsResponse> {
        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
        };

        // API Key credentialdirectlyuse kiro_api_key, no needrefresh
        let token = if credentials.is_api_key_credential() {
            credentials
                .kiro_api_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("API Key credential missing kiroApiKey"))?
        } else {
            // check whether a refresh is needed token
            let needs_refresh =
                is_token_expired(&credentials) || is_token_expiring_soon(&credentials);

            if needs_refresh {
                let _guard = self.refresh_lock.lock().await;
                let current_creds = {
                    let entries = self.entries.lock();
                    entries
                        .iter()
                        .find(|e| e.id == id)
                        .map(|e| e.credentials.clone())
                        .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
                };

                if is_token_expired(&current_creds) || is_token_expiring_soon(&current_creds) {
                    let global_proxy = self.proxy.lock().clone();
                    let effective_proxy = current_creds.effective_proxy(global_proxy.as_ref());
                    let new_creds =
                        refresh_token(&current_creds, &self.config, effective_proxy.as_ref())
                            .await?;
                    {
                        let mut entries = self.entries.lock();
                        if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                            entry.credentials = new_creds.clone();
                        }
                    }
                    // Persistence failure only logs a warning and does not affect this request.
                    if let Err(e) = self.persist_credentials() {
                        tracing::warn!("Token Persistence failed after refresh (does not affect this request).: {}", e);
                    }
                    new_creds
                        .access_token
                        .ok_or_else(|| anyhow::anyhow!("none after refresh access_token"))?
                } else {
                    current_creds
                        .access_token
                        .ok_or_else(|| anyhow::anyhow!("credential none access_token"))?
                }
            } else {
                credentials
                    .access_token
                    .ok_or_else(|| anyhow::anyhow!("credential none access_token"))?
            }
        };

        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
        };

        let global_proxy = self.proxy.lock().clone();
        let effective_proxy = credentials.effective_proxy(global_proxy.as_ref());
        let usage_limits =
            get_usage_limits(&credentials, &self.config, &token, effective_proxy.as_ref()).await?;

        // Updates the subscription tier on the credential (persists only when it changes).
        if let Some(subscription_title) = usage_limits.subscription_title() {
            let changed = {
                let mut entries = self.entries.lock();
                if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                    let old_title = entry.credentials.subscription_title.clone();
                    if old_title.as_deref() != Some(subscription_title) {
                        entry.credentials.subscription_title = Some(subscription_title.to_string());
                        tracing::info!(
                            "credential #{} the subscription tier has been updated: {:?} -> {}",
                            id,
                            old_title,
                            subscription_title
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if changed {
                if let Err(e) = self.persist_credentials() {
                    tracing::warn!("Persistence failed after updating the subscription tier (does not affect this request).: {}", e);
                }
            }
        }

        // Backfills email: writes only when the credential has no email and upstream returned one.
        if let Some(email) = usage_limits.email() {
            let changed = {
                let mut entries = self.entries.lock();
                if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                    let is_empty = entry
                        .credentials
                        .email
                        .as_deref()
                        .map(|s| s.is_empty())
                        .unwrap_or(true);
                    if is_empty {
                        entry.credentials.email = Some(email.to_string());
                        tracing::info!("credential #{} emailalreadybackfill: {}", id, email);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if changed {
                if let Err(e) = self.persist_credentials() {
                    tracing::warn!("Persistence failed after email backfill (does not affect this request).: {}", e);
                }
            }
        }

        Ok(usage_limits)
    }

    /// Prepares a valid one for read only upstream queries. token with the latest credential snapshot
    ///
    /// reuse [`Self::get_usage_limits_for`] of token prepareflow:API Key credentialdirectlyuse
    /// kiroApiKey;OAuth credentialbyneedin `refresh_lock` Refreshes and persists within it. The returned credential is
    /// The latest snapshot re-read after refresh; the caller builds the request from it.
    async fn prepare_request_token(
        &self,
        id: u64,
    ) -> anyhow::Result<(String, KiroCredentials)> {
        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
        };

        // API Key credentialdirectlyuse kiro_api_key, no needrefresh
        let token = if credentials.is_api_key_credential() {
            credentials
                .kiro_api_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("API Key credential missing kiroApiKey"))?
        } else if is_token_expired(&credentials) || is_token_expiring_soon(&credentials) {
            let _guard = self.refresh_lock.lock().await;
            let current_creds = {
                let entries = self.entries.lock();
                entries
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.credentials.clone())
                    .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
            };

            if is_token_expired(&current_creds) || is_token_expiring_soon(&current_creds) {
                let global_proxy = self.proxy.lock().clone();
                let effective_proxy = current_creds.effective_proxy(global_proxy.as_ref());
                let new_creds =
                    refresh_token(&current_creds, &self.config, effective_proxy.as_ref()).await?;
                {
                    let mut entries = self.entries.lock();
                    if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                        entry.credentials = new_creds.clone();
                    }
                }
                // Persistence failure only logs a warning and does not affect this request.
                if let Err(e) = self.persist_credentials() {
                    tracing::warn!("Token Persistence failed after refresh (does not affect this request).: {}", e);
                }
                new_creds
                    .access_token
                    .ok_or_else(|| anyhow::anyhow!("none after refresh access_token"))?
            } else {
                current_creds
                    .access_token
                    .ok_or_else(|| anyhow::anyhow!("credential none access_token"))?
            }
        } else {
            credentials
                .access_token
                .clone()
                .ok_or_else(|| anyhow::anyhow!("credential none access_token"))?
        };

        // Re-reads the latest credential (the refresh may have rewritten access_token outside ofoffield)
        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
        };

        Ok((token, credentials))
    }

    /// Gets the currently available model list for the given credential (Admin API)
    ///
    /// query the upstream in real time on demand `ListAvailableModels`,notdocache.
    pub async fn get_available_models_for(
        &self,
        id: u64,
    ) -> anyhow::Result<ListAvailableModelsResponse> {
        let (token, credentials) = self.prepare_request_token(id).await?;
        let global_proxy = self.proxy.lock().clone();
        let effective_proxy = credentials.effective_proxy(global_proxy.as_ref());
        get_available_models(&credentials, &self.config, &token, effective_proxy.as_ref()).await
    }

    /// set user preference (enable/closeoverage)— Admin API
    ///
    /// and `get_usage_limits_for` similar token Preparation flow, finally calls upstream.
    /// `setUserPreference` interfacewritenewof `overageStatus`.
    pub async fn set_user_preference_for(
        &self,
        id: u64,
        overage_status: &str,
    ) -> anyhow::Result<()> {
        // only accept "ENABLED" / "DISABLED",othervalueearly fail
        if overage_status != "ENABLED" && overage_status != "DISABLED" {
            anyhow::bail!("overageStatus must be ENABLED or DISABLED");
        }

        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
        };

        // API Key credential:directlywhen Bearer use
        let token = if credentials.is_api_key_credential() {
            credentials
                .kiro_api_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("API Key credential missing kiroApiKey"))?
        } else {
            // reuse and get_usage_limits_for Exactly the same expiry check and refresh logic.
            let needs_refresh =
                is_token_expired(&credentials) || is_token_expiring_soon(&credentials);

            if needs_refresh {
                let _guard = self.refresh_lock.lock().await;
                let current_creds = {
                    let entries = self.entries.lock();
                    entries
                        .iter()
                        .find(|e| e.id == id)
                        .map(|e| e.credentials.clone())
                        .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
                };

                if is_token_expired(&current_creds) || is_token_expiring_soon(&current_creds) {
                    let global_proxy = self.proxy.lock().clone();
                    let effective_proxy = current_creds.effective_proxy(global_proxy.as_ref());
                    let new_creds =
                        refresh_token(&current_creds, &self.config, effective_proxy.as_ref())
                            .await?;
                    {
                        let mut entries = self.entries.lock();
                        if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                            entry.credentials = new_creds.clone();
                        }
                    }
                    if let Err(e) = self.persist_credentials() {
                        tracing::warn!("Token Persistence failed after refresh (does not affect this request).: {}", e);
                    }
                    new_creds
                        .access_token
                        .ok_or_else(|| anyhow::anyhow!("none after refresh access_token"))?
                } else {
                    current_creds
                        .access_token
                        .ok_or_else(|| anyhow::anyhow!("credential none access_token"))?
                }
            } else {
                credentials
                    .access_token
                    .ok_or_else(|| anyhow::anyhow!("credential none access_token"))?
            }
        };

        // Re-reads the latest credential snapshot (refresh cancanalreadymodify access_token outside ofoffield)
        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
        };

        let global_proxy = self.proxy.lock().clone();
        let effective_proxy = credentials.effective_proxy(global_proxy.as_ref());
        set_user_preference(
            &credentials,
            &self.config,
            &token,
            effective_proxy.as_ref(),
            overage_status,
        )
        .await
    }

    /// addnewcredential (Admin API)
    ///
    /// # flow
    /// 1. validate the basic credential fields (API Key: kiroApiKey not empty; OAuth: refreshToken not empty)
    /// 2. based on kiroApiKey or refreshToken of SHA-256 hashdetect duplicate
    /// 3. OAuth: try to refresh Token validate credential validity; API Key: skip
    /// 4. allocate new ID(currentmaximum ID + 1)
    /// 5. add to entries list
    /// 6. persist to the configuration file
    ///
    /// # return
    /// - `Ok(u64)` - new credential ID
    /// - `Err(_)` - validation failed or add failed
    pub async fn add_credential(&self, new_cred: KiroCredentials) -> anyhow::Result<u64> {
        // 1. basic validation
        if new_cred.is_api_key_credential() {
            let api_key = new_cred
                .kiro_api_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("API Key credential missing kiroApiKey"))?;
            if api_key.is_empty() {
                anyhow::bail!("kiroApiKey is empty");
            }
        } else {
            validate_refresh_token(&new_cred)?;
        }

        // 2. detect duplicates based on hash
        if new_cred.is_api_key_credential() {
            let new_api_key = new_cred
                .kiro_api_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing kiroApiKey"))?;
            let new_api_key_hash = sha256_hex(new_api_key);
            let duplicate_exists = {
                let entries = self.entries.lock();
                entries.iter().any(|entry| {
                    entry
                        .credentials
                        .kiro_api_key
                        .as_deref()
                        .map(sha256_hex)
                        .as_deref()
                        == Some(new_api_key_hash.as_str())
                })
            };
            if duplicate_exists {
                anyhow::bail!("credentialalready exists(kiroApiKey duplicate)");
            }
        } else {
            let new_refresh_token = new_cred
                .refresh_token
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing refreshToken"))?;
            let new_refresh_token_hash = sha256_hex(new_refresh_token);
            let duplicate_exists = {
                let entries = self.entries.lock();
                entries.iter().any(|entry| {
                    entry
                        .credentials
                        .refresh_token
                        .as_deref()
                        .map(sha256_hex)
                        .as_deref()
                        == Some(new_refresh_token_hash.as_str())
                })
            };
            if duplicate_exists {
                anyhow::bail!("credentialalready exists(refreshToken duplicate)");
            }
        }

        // 3. validate credential validity (API Key no network refresh needed)
        let mut validated_cred = if new_cred.is_api_key_credential() {
            new_cred.clone()
        } else {
            let global_proxy = self.proxy.lock().clone();
            let effective_proxy = new_cred.effective_proxy(global_proxy.as_ref());
            refresh_token(&new_cred, &self.config, effective_proxy.as_ref()).await?
        };

        // Captures the dedup fingerprint of the original input. A refresh may rotate refreshToken, and below step 5 will
        // new_cred field move path; so the fingerprint must be taken here (while the fields are still complete),
        // For the authoritative dedup recheck in the insertion critical section.
        let dedup_is_api_key = new_cred.is_api_key_credential();
        let dedup_hash: Option<String> = if dedup_is_api_key {
            new_cred
                .kiro_api_key
                .as_deref()
                .filter(|k| !k.is_empty())
                .map(sha256_hex)
        } else {
            new_cred.refresh_token.as_deref().map(sha256_hex)
        };

        // 4. allocate new ID. Must use a monotonic counter, not based on the current entries maximumvaluerecompute;
        // Otherwise, after deleting the last account and adding again would reuse the old ID, causing trace/usage/kiro_stats
        // such by credential_id The aggregated history is inherited by the new account.
        let new_id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // 5. set ID and preserves the metadata input by the user.
        validated_cred.id = Some(new_id);
        validated_cred.priority = new_cred.priority;
        validated_cred.auth_method = new_cred.auth_method.map(|m| {
            if m.eq_ignore_ascii_case("builder-id") || m.eq_ignore_ascii_case("iam") {
                "idc".to_string()
            } else {
                m
            }
        });
        if new_cred.profile_arn.is_some() {
            validated_cred.profile_arn = new_cred.profile_arn;
        }
        validated_cred.provider = new_cred.provider;
        validated_cred.fill_default_profile_arn();
        validated_cred.client_id = new_cred.client_id;
        validated_cred.client_secret = new_cred.client_secret;
        validated_cred.region = new_cred.region;
        validated_cred.auth_region = new_cred.auth_region;
        validated_cred.api_region = new_cred.api_region;
        validated_cred.machine_id = new_cred.machine_id;
        validated_cred.email = new_cred.email;
        validated_cred.proxy_url = new_cred.proxy_url;
        validated_cred.proxy_username = new_cred.proxy_username;
        validated_cred.proxy_password = new_cred.proxy_password;
        validated_cred.kiro_api_key = new_cred.kiro_api_key;

        {
            let mut entries = self.entries.lock();
            // concurrency safe:token The refresh (network) completes outside the lock; meanwhile there may be other concurrent
            // add_credential viathe step 2 pre dedup and already inserted the same credential. So while holding the lock
            // At the insertion point, does an authoritative dedup again using the original input fingerprint, closing TOCTOU(such ashitthen bail,
            // next_id Even if already incremented it only skips a number, with no side effect.
            if let Some(hash) = &dedup_hash {
                let dup = entries.iter().any(|e| {
                    let entry_hash = if dedup_is_api_key {
                        e.credentials.kiro_api_key.as_deref().map(sha256_hex)
                    } else {
                        e.credentials.refresh_token.as_deref().map(sha256_hex)
                    };
                    entry_hash.as_deref() == Some(hash.as_str())
                });
                if dup {
                    let msg = if dedup_is_api_key {
                        "credentialalready exists(kiroApiKey duplicate)"
                    } else {
                        "credentialalready exists(refreshToken duplicate)"
                    };
                    anyhow::bail!(msg);
                }
            }
            entries.push(CredentialEntry {
                id: new_id,
                credentials: validated_cred,
                failure_count: 0,
                total_failure_count: 0,
                refresh_failure_count: 0,
                disabled: false,
                disabled_reason: None,
                success_count: 0,
                last_used_at: None,
                throttled_until: None,
            });
        }

        // 6. Upgrades to the multi credential format (ensures subsequent token rotation can write to disk) and persist
        self.is_multiple_format.store(true, Ordering::Relaxed);
        self.persist_credentials()?;

        tracing::info!("successaddcredential #{}", new_id);
        Ok(new_id)
    }

    /// Updates the editable fields of the credential (Admin API)
    ///
    /// support update email,proxy_url,proxy_username,proxy_password.
    /// pass `None` means do not modify this field, pass `Some("")` means clear this field.
    pub fn update_credential(
        &self,
        id: u64,
        email: Option<Option<String>>,
        proxy_url: Option<Option<String>>,
        proxy_username: Option<Option<String>>,
        proxy_password: Option<Option<String>>,
        groups: Option<Vec<String>>,
        source_channel: Option<Option<String>>,
    ) -> anyhow::Result<()> {
        {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?;

            if let Some(v) = email {
                entry.credentials.email = v.filter(|s| !s.is_empty());
            }
            if let Some(v) = proxy_url {
                entry.credentials.proxy_url = v.filter(|s| !s.is_empty());
            }
            if let Some(v) = proxy_username {
                entry.credentials.proxy_username = v.filter(|s| !s.is_empty());
            }
            if let Some(v) = proxy_password {
                entry.credentials.proxy_password = v.filter(|s| !s.is_empty());
            }
            if let Some(g) = groups {
                entry.credentials.groups =
                    g.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
            if let Some(v) = source_channel {
                entry.credentials.source_channel =
                    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            }
        }
        self.persist_credentials()?;
        Ok(())
    }

    /// Lists the group names currently referenced by all credentials (deduplicated and sorted).
    /// used at startup to migrate to GroupManager the registry, and the frontend reference count display.
    pub fn list_credential_groups(&self) -> Vec<String> {
        let entries = self.entries.lock();
        let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
        for e in entries.iter() {
            for g in &e.credentials.groups {
                if !g.is_empty() {
                    set.insert(g.clone());
                }
            }
        }
        let mut list: Vec<String> = set.into_iter().collect();
        list.sort();
        list
    }

    /// Counts how many credentials reference the given group (for the group management page). / prompt before deletion).
    pub fn count_credentials_with_group(&self, group: &str) -> usize {
        let entries = self.entries.lock();
        entries
            .iter()
            .filter(|e| e.credentials.groups.iter().any(|g| g == group))
            .count()
    }

    /// takeallcredential `groups` fieldinetc.at `old` ofelement changeas `new`(for group rename cascade).
    /// alreadyalready explicitly carried `new` credentials are not added again. Returns the number of affected credentials.
    pub fn rename_credential_group(&self, old: &str, new: &str) -> anyhow::Result<usize> {
        let mut affected = 0usize;
        {
            let mut entries = self.entries.lock();
            for entry in entries.iter_mut() {
                let groups = &mut entry.credentials.groups;
                let mut hit = false;
                let mut already_has_new = false;
                for g in groups.iter() {
                    if g == old {
                        hit = true;
                    }
                    if g == new {
                        already_has_new = true;
                    }
                }
                if hit {
                    if already_has_new {
                        // old and new totalstore:onlyremove old, avoidduplicate
                        groups.retain(|g| g != old);
                    } else {
                        for g in groups.iter_mut() {
                            if g == old {
                                *g = new.to_string();
                            }
                        }
                    }
                    affected += 1;
                }
            }
        }
        if affected > 0 {
            self.persist_credentials()?;
        }
        Ok(affected)
    }

    /// take `name` this group from all credentials `groups` Removes from the field (for force delete group cascade).
    /// return the number of affected credentials.
    pub fn remove_credential_group(&self, name: &str) -> anyhow::Result<usize> {
        let mut affected = 0usize;
        {
            let mut entries = self.entries.lock();
            for entry in entries.iter_mut() {
                let before = entry.credentials.groups.len();
                entry.credentials.groups.retain(|g| g != name);
                if entry.credentials.groups.len() != before {
                    affected += 1;
                }
            }
        }
        if affected > 0 {
            self.persist_credentials()?;
        }
        Ok(affected)
    }

    /// deletecredential (Admin API)
    ///
    /// # precondition
    /// - the credential must be disabled (disabled = true)
    ///
    /// # behavior
    /// 1. verifycredentialexists
    /// 2. validate the credential is disabled
    /// 3. from entries remove
    /// 4. If the deleted one is the current credential, switch to the highest priority available credential.
    /// 5. If there are no credentials after deletion, will current_id reset to 0
    /// 6. persisttofile
    ///
    /// # return
    /// - `Ok(())` - delete success
    /// - `Err(_)` - The credential does not exist or persistence failed.
    pub fn delete_credential(&self, id: u64) -> anyhow::Result<()> {
        let was_current = {
            let mut entries = self.entries.lock();

            // find credential
            let _entry = entries
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?;

            // record whether it is the current credential
            let current_id = *self.current_id.lock();
            let was_current = current_id == id;

            // deletecredential
            entries.retain(|e| e.id != id);

            was_current
        };

        // If the deleted one is the current credential, switch to the highest priority available credential.
        if was_current {
            self.select_highest_priority();
        }

        // If there are no credentials after deletion, will current_id reset to 0(consistent with the initialization behavior)
        {
            let entries = self.entries.lock();
            if entries.is_empty() {
                let mut current_id = self.current_id.lock();
                *current_id = 0;
                tracing::info!("all credentials deleted,current_id reset to 0");
            }
        }

        // persistchange
        self.persist_credentials()?;

        // Immediately writes statistics back, clearing residual entries of deleted credentials.
        self.save_stats();

        tracing::info!("deletedcredential #{}", id);
        Ok(())
    }

    /// update the specified credential refreshToken(Admin API)
    ///
    /// # precondition
    /// - the credential must be disabled (disabled = true), preventing accidental overwrite of the one in use. Token
    ///
    /// # behavior
    /// 1. validate the credential exists and is disabled
    /// 2. verify new refreshToken format
    /// 3. update refreshToken
    /// 4. reset refresh_failure_count(keep disabled state, letting the user enable it manually)
    /// 5. persisttofile
    pub fn update_refresh_token(
        &self,
        id: u64,
        new_refresh_token: String,
        new_access_token: Option<String>,
        new_expires_at: Option<String>,
    ) -> anyhow::Result<()> {
        {
            let mut entries = self.entries.lock();

            // Locates by index, avoiding two linear scans and subsequent unwrap
            let idx = entries
                .iter()
                .position(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?;

            if !entries[idx].disabled {
                anyhow::bail!(
                    "Can only update for a disabled credential. refreshToken(please disable the credential first #{})",
                    id
                );
            }

            // verify new refreshToken format
            let tmp_creds = KiroCredentials {
                refresh_token: Some(new_refresh_token.clone()),
                ..entries[idx].credentials.clone()
            };
            validate_refresh_token(&tmp_creds)?;

            // Checks whether it duplicates an existing other credential.
            let new_hash = sha256_hex(&new_refresh_token);
            let duplicate = entries.iter().enumerate().any(|(i, e)| {
                i != idx
                    && e.credentials
                        .refresh_token
                        .as_ref()
                        .map(|t| sha256_hex(t) == new_hash)
                        .unwrap_or(false)
            });
            if duplicate {
                anyhow::bail!("refreshToken duplicate with another credential");
            }

            let entry = &mut entries[idx];
            entry.credentials.refresh_token = Some(new_refresh_token);
            // if the caller provided accessToken(fromimport/export), then keeps it directly, without immediately calling the auth server.
            // Otherwise clears it; the system auto refreshes on next use.
            entry.credentials.access_token = new_access_token;
            entry.credentials.expires_at = new_expires_at;
            entry.refresh_failure_count = 0;
        }
        self.persist_credentials()?;
        tracing::info!("credential #{} refreshToken updated", id);
        Ok(())
    }

    /// force refresh the specified credential Token(Admin API)
    ///
    /// unconditionally call the upstream API re fetch access token, do not check whether it is expired.
    /// applicable for troubleshooting,Token Cases such as abnormal but not expired, or actively updating credential state.
    pub async fn force_refresh_token_for(&self, id: u64) -> anyhow::Result<()> {
        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("credentialdoes not exist: {}", id))?
        };

        // Acquires the refresh lock to prevent concurrent refresh.
        let _guard = self.refresh_lock.lock().await;

        // noneentryitemcall refresh_token
        let global_proxy = self.proxy.lock().clone();
        let effective_proxy = credentials.effective_proxy(global_proxy.as_ref());
        let new_creds = refresh_token(&credentials, &self.config, effective_proxy.as_ref()).await?;

        // update entries incorrespondcredential
        {
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                entry.credentials = new_creds;
                entry.refresh_failure_count = 0;
            }
        }

        // persist
        if let Err(e) = self.persist_credentials() {
            tracing::warn!("forcerefresh Token afterpersistfailed: {}", e);
        }

        tracing::info!("credential #{} Token alreadyforcerefresh", id);
        Ok(())
    }

    /// get the load balancing mode (Admin API)
    pub fn get_load_balancing_mode(&self) -> String {
        self.load_balancing_mode.lock().clone()
    }

    fn persist_load_balancing_mode(&self, mode: &str) -> anyhow::Result<()> {
        use anyhow::Context;

        let config_path = match self.config.config_path() {
            Some(path) => path.to_path_buf(),
            None => {
                tracing::warn!("The config file path is unknown; load balancing mode takes effect only in the current process.: {}", mode);
                return Ok(());
            }
        };

        let mut config = Config::load(&config_path)
            .with_context(|| format!("failed to reload the configuration: {}", config_path.display()))?;
        config.load_balancing_mode = mode.to_string();
        config
            .save()
            .with_context(|| format!("Persisting the load balancing mode failed.: {}", config_path.display()))?;

        Ok(())
    }

    /// set the load balancing mode (Admin API)
    pub fn set_load_balancing_mode(&self, mode: String) -> anyhow::Result<()> {
        // verifymodevalue
        if mode != "priority" && mode != "balanced" {
            anyhow::bail!("invalid load balancing mode: {}", mode);
        }

        let previous_mode = self.get_load_balancing_mode();
        if previous_mode == mode {
            return Ok(());
        }

        *self.load_balancing_mode.lock() = mode.clone();

        if let Err(err) = self.persist_load_balancing_mode(&mode) {
            *self.load_balancing_mode.lock() = previous_mode;
            return Err(err);
        }

        tracing::info!("the load balancing mode has been set to: {}", mode);
        Ok(())
    }

    /// Gets the account level throttle failover config (Admin API)
    pub fn get_account_throttle_failover(&self) -> bool {
        self.account_throttle_failover.load(Ordering::Relaxed)
    }

    /// Gets the account level throttle cooldown duration in seconds (Admin API)
    pub fn get_account_throttle_cooldown_secs(&self) -> u64 {
        self.account_throttle_cooldown_secs.load(Ordering::Relaxed)
    }

    /// Sets the account level throttle failover config (Admin API)
    ///
    /// any oneparameterpass `None` means do not modify this field.
    pub fn set_account_throttle_config(
        &self,
        failover: Option<bool>,
        cooldown_secs: Option<u64>,
    ) -> anyhow::Result<()> {
        if let Some(secs) = cooldown_secs {
            // limit to a reasonable range:1 seconds to 24 hour
            if !(1..=86_400).contains(&secs) {
                anyhow::bail!("the cooldown duration must be within 1..=86400 within seconds: {}", secs);
            }
        }

        let prev_failover = self.get_account_throttle_failover();
        let prev_cooldown = self.get_account_throttle_cooldown_secs();
        let new_failover = failover.unwrap_or(prev_failover);
        let new_cooldown = cooldown_secs.unwrap_or(prev_cooldown);

        if new_failover == prev_failover && new_cooldown == prev_cooldown {
            return Ok(());
        }

        self.account_throttle_failover
            .store(new_failover, Ordering::Relaxed);
        self.account_throttle_cooldown_secs
            .store(new_cooldown, Ordering::Relaxed);

        if let Err(err) = self.persist_account_throttle_config(new_failover, new_cooldown) {
            // rollbackmemoryvalue
            self.account_throttle_failover
                .store(prev_failover, Ordering::Relaxed);
            self.account_throttle_cooldown_secs
                .store(prev_cooldown, Ordering::Relaxed);
            return Err(err);
        }

        tracing::info!(
            "account level throttle configuration has been updated: failover={}, cooldown_secs={}",
            new_failover,
            new_cooldown
        );
        Ok(())
    }

    fn persist_account_throttle_config(&self, failover: bool, cooldown_secs: u64) -> anyhow::Result<()> {
        use anyhow::Context;

        let config_path = match self.config.config_path() {
            Some(path) => path.to_path_buf(),
            None => {
                tracing::warn!("The config file path is unknown; account level throttle config takes effect only in the current process.");
                return Ok(());
            }
        };

        let mut config = Config::load(&config_path)
            .with_context(|| format!("failed to reload the configuration: {}", config_path.display()))?;
        config.account_throttle_failover = failover;
        config.account_throttle_cooldown_secs = cooldown_secs;
        config
            .save()
            .with_context(|| format!("Persisting the account level throttle config failed.: {}", config_path.display()))?;

        Ok(())
    }
}

impl Drop for MultiTokenManager {
    fn drop(&mut self) {
        if self.stats_dirty.load(Ordering::Relaxed) {
            self.save_stats();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_is_token_expired_with_expired_token() {
        let mut credentials = KiroCredentials::default();
        credentials.expires_at = Some("2020-01-01T00:00:00Z".to_string());
        assert!(is_token_expired(&credentials));
    }

    #[test]
    fn test_is_token_expired_with_valid_token() {
        let mut credentials = KiroCredentials::default();
        let future = Utc::now() + Duration::hours(1);
        credentials.expires_at = Some(future.to_rfc3339());
        assert!(!is_token_expired(&credentials));
    }

    #[test]
    fn test_is_token_expired_within_5_minutes() {
        let mut credentials = KiroCredentials::default();
        let expires = Utc::now() + Duration::minutes(3);
        credentials.expires_at = Some(expires.to_rfc3339());
        assert!(is_token_expired(&credentials));
    }

    #[test]
    fn test_is_token_expired_no_expires_at() {
        let credentials = KiroCredentials::default();
        assert!(is_token_expired(&credentials));
    }

    #[test]
    fn test_is_token_expiring_soon_within_10_minutes() {
        let mut credentials = KiroCredentials::default();
        let expires = Utc::now() + Duration::minutes(8);
        credentials.expires_at = Some(expires.to_rfc3339());
        assert!(is_token_expiring_soon(&credentials));
    }

    #[test]
    fn test_is_token_expiring_soon_beyond_10_minutes() {
        let mut credentials = KiroCredentials::default();
        let expires = Utc::now() + Duration::minutes(15);
        credentials.expires_at = Some(expires.to_rfc3339());
        assert!(!is_token_expiring_soon(&credentials));
    }

    #[test]
    fn test_validate_refresh_token_missing() {
        let credentials = KiroCredentials::default();
        let result = validate_refresh_token(&credentials);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_refresh_token_valid() {
        let mut credentials = KiroCredentials::default();
        credentials.refresh_token = Some("a".repeat(150));
        let result = validate_refresh_token(&credentials);
        assert!(result.is_ok());
    }

    #[test]
    fn test_sha256_hex() {
        let result = sha256_hex("test");
        assert_eq!(
            result,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[tokio::test]
    async fn test_refresh_token_rejects_api_key_credential() {
        let config = Config::default();
        let mut credentials = KiroCredentials::default();
        credentials.kiro_api_key = Some("ksk_test_key_123".to_string());
        credentials.auth_method = Some("api_key".to_string());

        let result = refresh_token(&credentials, &config, None).await;

        assert!(result.is_err(), "API Key credential should be refresh_token reject");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("API Key the credential does not support refresh"),
            "expect the error message to contain 'API Key the credential does not support refresh', actual: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_add_credential_reject_duplicate_refresh_token() {
        let config = Config::default();

        let mut existing = KiroCredentials::default();
        existing.refresh_token = Some("a".repeat(150));

        let manager = MultiTokenManager::new(config, vec![existing], None, None, false).unwrap();

        let mut duplicate = KiroCredentials::default();
        duplicate.refresh_token = Some("a".repeat(150));

        let result = manager.add_credential(duplicate).await;
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("credentialalready exists"));
    }

    #[tokio::test]
    async fn test_add_credential_api_key_success() {
        let config = Config::default();
        let manager = MultiTokenManager::new(config, vec![], None, None, false).unwrap();

        let mut api_key_cred = KiroCredentials::default();
        api_key_cred.kiro_api_key = Some("ksk_test_key_123".to_string());
        api_key_cred.auth_method = Some("api_key".to_string());

        let result = manager.add_credential(api_key_cred).await;
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(id > 0);
        assert_eq!(manager.total_count(), 1);
        assert_eq!(manager.available_count(), 1);
    }

    #[tokio::test]
    async fn test_add_credential_reject_duplicate_api_key() {
        let config = Config::default();

        let mut existing = KiroCredentials::default();
        existing.kiro_api_key = Some("ksk_existing_key".to_string());
        existing.auth_method = Some("api_key".to_string());

        let manager = MultiTokenManager::new(config, vec![existing], None, None, false).unwrap();

        let mut duplicate = KiroCredentials::default();
        duplicate.kiro_api_key = Some("ksk_existing_key".to_string());
        duplicate.auth_method = Some("api_key".to_string());

        let result = manager.add_credential(duplicate).await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("kiroApiKey duplicate")
        );
    }

    #[tokio::test]
    async fn test_add_credential_api_key_empty_rejected() {
        let config = Config::default();
        let manager = MultiTokenManager::new(config, vec![], None, None, false).unwrap();

        let mut cred = KiroCredentials::default();
        cred.kiro_api_key = Some(String::new());
        cred.auth_method = Some("api_key".to_string());

        let result = manager.add_credential(cred).await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("kiroApiKey is empty")
        );
    }

    #[tokio::test]
    async fn test_add_credential_api_key_missing_key_rejected() {
        let config = Config::default();
        let manager = MultiTokenManager::new(config, vec![], None, None, false).unwrap();

        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("api_key".to_string());
        // kiro_api_key is None

        let result = manager.add_credential(cred).await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("missing kiroApiKey")
        );
    }

    #[tokio::test]
    async fn test_add_credential_api_key_and_oauth_coexist() {
        let config = Config::default();

        let mut oauth_cred = KiroCredentials::default();
        oauth_cred.refresh_token = Some("a".repeat(150));

        let manager = MultiTokenManager::new(config, vec![oauth_cred], None, None, false).unwrap();

        let mut api_key_cred = KiroCredentials::default();
        api_key_cred.kiro_api_key = Some("ksk_new_key".to_string());
        api_key_cred.auth_method = Some("api_key".to_string());

        let result = manager.add_credential(api_key_cred).await;
        assert!(result.is_ok());
        assert_eq!(manager.total_count(), 2);
        assert_eq!(manager.available_count(), 2);
    }

    // MultiTokenManager test

    #[test]
    fn test_multi_token_manager_new() {
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.priority = 0;
        let mut cred2 = KiroCredentials::default();
        cred2.priority = 1;

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();
        assert_eq!(manager.total_count(), 2);
        assert_eq!(manager.available_count(), 2);
    }

    #[test]
    fn test_multi_token_manager_empty_credentials() {
        let config = Config::default();
        let result = MultiTokenManager::new(config, vec![], None, None, false);
        // support 0 credentials at startup (can be added via the admin panel).
        assert!(result.is_ok());
        let manager = result.unwrap();
        assert_eq!(manager.total_count(), 0);
        assert_eq!(manager.available_count(), 0);
    }

    #[test]
    fn test_multi_token_manager_duplicate_ids() {
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.id = Some(1);
        let mut cred2 = KiroCredentials::default();
        cred2.id = Some(1); // duplicate ID

        let result = MultiTokenManager::new(config, vec![cred1, cred2], None, None, false);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("duplicate credential ID"),
            "the error message should contain 'duplicate credential ID', actual: {}",
            err_msg
        );
    }

    #[test]
    fn test_multi_token_manager_api_key_missing_kiro_api_key_auto_disabled() {
        let config = Config::default();

        // auth_method=api_key but missing kiro_api_key → should beautomaticdisable
        let mut bad_cred = KiroCredentials::default();
        bad_cred.auth_method = Some("api_key".to_string());
        // kiro_api_key keep None

        let mut good_cred = KiroCredentials::default();
        good_cred.refresh_token = Some("valid_token".to_string());

        let manager =
            MultiTokenManager::new(config, vec![bad_cred, good_cred], None, None, false).unwrap();
        assert_eq!(manager.total_count(), 2);
        assert_eq!(manager.available_count(), 1); // bad_cred bydisable,onlyremain 1 available
    }

    #[test]
    fn test_multi_token_manager_api_key_with_kiro_api_key_not_disabled() {
        let config = Config::default();

        // auth_method=api_key and has kiro_api_key → should notbydisable
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("api_key".to_string());
        cred.kiro_api_key = Some("ksk_test123".to_string());

        let manager = MultiTokenManager::new(config, vec![cred], None, None, false).unwrap();
        assert_eq!(manager.total_count(), 1);
        assert_eq!(manager.available_count(), 1);
    }

    #[test]
    fn test_multi_token_manager_report_failure() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // the credential is automatically assigned ID(from 1 start)
        // The first two failures do not disable (using ID 1)
        assert!(manager.report_failure(1));
        assert!(manager.report_failure(1));
        assert_eq!(manager.available_count(), 2);

        // The third failure disables the first credential.
        assert!(manager.report_failure(1));
        assert_eq!(manager.available_count(), 1);

        // Continues to fail the second credential (using ID 2)
        assert!(manager.report_failure(2));
        assert!(manager.report_failure(2));
        assert!(!manager.report_failure(2)); // all credentials are disabled
        assert_eq!(manager.available_count(), 0);
    }

    #[test]
    fn test_multi_token_manager_report_success() {
        let config = Config::default();
        let cred = KiroCredentials::default();

        let manager = MultiTokenManager::new(config, vec![cred], None, None, false).unwrap();

        // failed twice (use ID 1)
        manager.report_failure(1);
        manager.report_failure(1);

        // reset the count after success (use ID 1)
        manager.report_success(1);

        // two more failures will not disable
        manager.report_failure(1);
        manager.report_failure(1);
        assert_eq!(manager.available_count(), 1);
    }

    #[test]
    fn test_multi_token_manager_switch_to_next() {
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.refresh_token = Some("token1".to_string());
        let mut cred2 = KiroCredentials::default();
        cred2.refresh_token = Some("token2".to_string());

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        let initial_id = manager.snapshot().current_id;

        // switchtonext
        assert!(manager.switch_to_next());
        assert_ne!(manager.snapshot().current_id, initial_id);
    }

    #[test]
    fn test_set_load_balancing_mode_persists_to_config_file() {
        let config_path =
            std::env::temp_dir().join(format!("kiro-load-balancing-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&config_path, r#"{"loadBalancingMode":"priority"}"#).unwrap();

        let config = Config::load(&config_path).unwrap();
        let manager =
            MultiTokenManager::new(config, vec![KiroCredentials::default()], None, None, false)
                .unwrap();

        manager
            .set_load_balancing_mode("balanced".to_string())
            .unwrap();

        let persisted = Config::load(&config_path).unwrap();
        assert_eq!(persisted.load_balancing_mode, "balanced");
        assert_eq!(manager.get_load_balancing_mode(), "balanced");

        std::fs::remove_file(&config_path).unwrap();
    }

    #[tokio::test]
    async fn test_multi_token_manager_acquire_context_auto_recovers_all_disabled() {
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.access_token = Some("t1".to_string());
        cred1.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());
        let mut cred2 = KiroCredentials::default();
        cred2.access_token = Some("t2".to_string());
        cred2.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // the credential is automatically assigned ID(from 1 start)
        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_failure(1);
        }
        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_failure(2);
        }

        assert_eq!(manager.available_count(), 0);

        // Should trigger self healing: reset the failure count and re-enable, avoiding the need to restart the process.
        let ctx = manager.acquire_context(None, None).await.unwrap();
        assert!(ctx.token == "t1" || ctx.token == "t2");
        assert_eq!(manager.available_count(), 2);
    }

    #[tokio::test]
    async fn test_multi_token_manager_acquire_context_balanced_retries_until_bad_credential_disabled()
     {
        let mut config = Config::default();
        config.load_balancing_mode = "balanced".to_string();

        let mut bad_cred = KiroCredentials::default();
        bad_cred.priority = 0;
        bad_cred.refresh_token = Some("bad".to_string());

        let mut good_cred = KiroCredentials::default();
        good_cred.priority = 1;
        good_cred.access_token = Some("good-token".to_string());
        good_cred.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());

        let manager =
            MultiTokenManager::new(config, vec![bad_cred, good_cred], None, None, false).unwrap();

        let ctx = manager.acquire_context(None, None).await.unwrap();
        assert_eq!(ctx.id, 2);
        assert_eq!(ctx.token, "good-token");
    }

    #[test]
    fn test_multi_token_manager_report_refresh_failure() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        assert_eq!(manager.available_count(), 2);
        for _ in 0..(MAX_FAILURES_PER_CREDENTIAL - 1) {
            assert!(manager.report_refresh_failure(1));
        }
        assert_eq!(manager.available_count(), 2);

        assert!(manager.report_refresh_failure(1));
        assert_eq!(manager.available_count(), 1);

        let snapshot = manager.snapshot();
        let first = snapshot.entries.iter().find(|e| e.id == 1).unwrap();
        assert!(first.disabled);
        assert_eq!(first.refresh_failure_count, MAX_FAILURES_PER_CREDENTIAL);
        assert_eq!(snapshot.current_id, 2);
    }

    #[tokio::test]
    async fn test_multi_token_manager_refresh_failure_disabled_is_not_auto_recovered() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_refresh_failure(1);
            manager.report_refresh_failure(2);
        }
        assert_eq!(manager.available_count(), 0);

        let err = manager
            .acquire_context(None, None)
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.contains("all credentials are disabled"),
            "The error should indicate all credentials are disabled; actually: {}",
            err
        );
    }

    #[test]
    fn test_multi_token_manager_report_quota_exhausted() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // the credential is automatically assigned ID(from 1 start)
        assert_eq!(manager.available_count(), 2);
        assert!(manager.report_quota_exhausted(1));
        assert_eq!(manager.available_count(), 1);

        // After disabling the second one, no credential is available.
        assert!(!manager.report_quota_exhausted(2));
        assert_eq!(manager.available_count(), 0);
    }

    #[tokio::test]
    async fn test_multi_token_manager_quota_disabled_is_not_auto_recovered() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        manager.report_quota_exhausted(1);
        manager.report_quota_exhausted(2);
        assert_eq!(manager.available_count(), 0);

        let err = manager
            .acquire_context(None, None)
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.contains("all credentials are disabled"),
            "The error should indicate all credentials are disabled; actually: {}",
            err
        );
        assert_eq!(manager.available_count(), 0);
    }

    // ============ credential level Region prioritytest ============

    #[test]
    fn test_credential_region_priority_uses_credential_auth_region() {
        // credentialconfigured auth_region when, should use the credential auth_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("eu-west-1".to_string());

        let region = credentials.effective_auth_region(&config);
        assert_eq!(region, "eu-west-1");
    }

    #[test]
    fn test_credential_region_priority_fallback_to_credential_region() {
        // credentialnot configured auth_region but configured region when, should fall back to the credential.region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.region = Some("eu-central-1".to_string());

        let region = credentials.effective_auth_region(&config);
        assert_eq!(region, "eu-central-1");
    }

    #[test]
    fn test_credential_region_priority_fallback_to_config() {
        // credentialnot configured auth_region and region when,shouldfallbackto config
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let credentials = KiroCredentials::default();
        assert!(credentials.auth_region.is_none());
        assert!(credentials.region.is_none());

        let region = credentials.effective_auth_region(&config);
        assert_eq!(region, "us-west-2");
    }

    #[test]
    fn test_multiple_credentials_use_respective_regions() {
        // In the multi credential case, different credentials use their own auth_region
        let mut config = Config::default();
        config.region = "ap-northeast-1".to_string();

        let mut cred1 = KiroCredentials::default();
        cred1.auth_region = Some("us-east-1".to_string());

        let mut cred2 = KiroCredentials::default();
        cred2.region = Some("eu-west-1".to_string());

        let cred3 = KiroCredentials::default(); // none region, use config

        assert_eq!(cred1.effective_auth_region(&config), "us-east-1");
        assert_eq!(cred2.effective_auth_region(&config), "eu-west-1");
        assert_eq!(cred3.effective_auth_region(&config), "ap-northeast-1");
    }

    #[test]
    fn test_idc_oidc_endpoint_uses_credential_auth_region() {
        // verify IdC OIDC endpoint URL use credential auth_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("eu-central-1".to_string());

        let region = credentials.effective_auth_region(&config);
        let refresh_url = format!("https://oidc.{}.amazonaws.com/token", region);

        assert_eq!(refresh_url, "https://oidc.eu-central-1.amazonaws.com/token");
    }

    #[test]
    fn test_social_refresh_endpoint_uses_credential_auth_region() {
        // verify Social refresh endpoint URL use credential auth_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("ap-southeast-1".to_string());

        let region = credentials.effective_auth_region(&config);
        let refresh_url = format!("https://prod.{}.auth.desktop.kiro.dev/refreshToken", region);

        assert_eq!(
            refresh_url,
            "https://prod.ap-southeast-1.auth.desktop.kiro.dev/refreshToken"
        );
    }

    #[test]
    fn test_api_call_uses_effective_api_region() {
        // verify API call use effective_api_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.region = Some("eu-west-1".to_string());

        // credential.region do not participate api_region fallback chain
        let api_region = credentials.effective_api_region(&config);
        let api_host = format!("q.{}.amazonaws.com", api_region);

        assert_eq!(api_host, "q.us-west-2.amazonaws.com");
    }

    #[test]
    fn test_api_call_uses_credential_api_region() {
        // credentialconfigured api_region when,API the call should use the credential api_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.api_region = Some("eu-central-1".to_string());

        let api_region = credentials.effective_api_region(&config);
        let api_host = format!("q.{}.amazonaws.com", api_region);

        assert_eq!(api_host, "q.eu-central-1.amazonaws.com");
    }

    #[test]
    fn test_rest_api_region_candidates_us_default() {
        // non EU region → main endpoint us-east-1, fallback eu-central-1
        assert_eq!(
            rest_api_region_candidates("us-east-1"),
            ["us-east-1", "eu-central-1"]
        );
        assert_eq!(
            rest_api_region_candidates("us-east-2"),
            ["us-east-1", "eu-central-1"]
        );
        assert_eq!(
            rest_api_region_candidates("ap-southeast-1"),
            ["us-east-1", "eu-central-1"]
        );
    }

    #[test]
    fn test_rest_api_region_candidates_eu() {
        // EU region → main endpoint eu-central-1, fallback us-east-1
        assert_eq!(
            rest_api_region_candidates("eu-central-1"),
            ["eu-central-1", "us-east-1"]
        );
        assert_eq!(
            rest_api_region_candidates("eu-west-1"),
            ["eu-central-1", "us-east-1"]
        );
        assert_eq!(
            rest_api_region_candidates("eu-north-1"),
            ["eu-central-1", "us-east-1"]
        );
    }

    #[test]
    fn test_rest_api_region_candidates_uses_credential_auth_region() {
        // Enterprise/IdC only carries at account import SSO region field (no api_region),
        // effective_auth_region falls back to credential.region, and thereby select the correct endpoint.
        let config = Config::default(); // default region = us-east-1

        let mut eu_cred = KiroCredentials::default();
        eu_cred.region = Some("eu-west-1".to_string());
        let sso_region = eu_cred.effective_auth_region(&config);
        assert_eq!(
            rest_api_region_candidates(sso_region),
            ["eu-central-1", "us-east-1"]
        );

        // not configuredany region ofcredentialfallbackto config default us-east-1
        let plain_cred = KiroCredentials::default();
        let sso_region = plain_cred.effective_auth_region(&config);
        assert_eq!(
            rest_api_region_candidates(sso_region),
            ["us-east-1", "eu-central-1"]
        );
    }

    #[test]
    fn test_credential_region_empty_string_treated_as_set() {
        // empty string auth_region Treated as set (although not recommended, the behavior should be consistent).
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("".to_string());

        let region = credentials.effective_auth_region(&config);
        // An empty string is treated as set and does not fall back to config
        assert_eq!(region, "");
    }

    #[test]
    fn test_auth_and_api_region_independent() {
        // auth_region and api_region do not affect each other
        let mut config = Config::default();
        config.region = "default".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("auth-only".to_string());
        credentials.api_region = Some("api-only".to_string());

        assert_eq!(credentials.effective_auth_region(&config), "auth-only");
        assert_eq!(credentials.effective_api_region(&config), "api-only");
    }

    // ── is_multiple_format auto upgrade ──────────────────────────────────────────

    fn tmp_creds_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("kiro_test_{}.json", name));
        p
    }

    /// singlecredentialformat (is_multiple_format=false) auto migrates to array format at startup,
    /// after migration persist_credentials canwrite to disk correctly,token rotation notagainlost.
    #[test]
    fn test_single_format_auto_migrates_to_multiple_on_startup() {
        let path = tmp_creds_path("single_migrate");
        let mut cred = KiroCredentials::default();
        cred.kiro_api_key = Some("ksk_test_migrate_key".to_string());
        cred.auth_method = Some("api_key".to_string());
        let single_json = serde_json::to_string(&cred).unwrap();
        std::fs::write(&path, &single_json).unwrap();

        let manager = MultiTokenManager::new(
            Config::default(),
            vec![cred],
            None,
            Some(path.clone()),
            false,
        )
        .unwrap();

        assert!(
            manager.is_multiple_format.load(Ordering::Relaxed),
            "The single credential format should auto upgrade at startup to true"
        );

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.trim_start().starts_with('['),
            "After migration the file should be array format; actually: {}",
            &content[..content.len().min(50)]
        );

        let _ = std::fs::remove_file(&path);
    }

    /// An empty credential list does not trigger migration.
    #[test]
    fn test_empty_credentials_no_migration() {
        let path = tmp_creds_path("empty_no_migrate");
        std::fs::write(&path, "{}").unwrap();

        let manager =
            MultiTokenManager::new(Config::default(), vec![], None, Some(path.clone()), false)
                .unwrap();

        assert!(
            !manager.is_multiple_format.load(Ordering::Relaxed),
            "No credentials should not trigger a format upgrade."
        );

        let _ = std::fs::remove_file(&path);
    }

    /// add_credential after is_multiple_format must upgradeas true, the file is written in array format
    #[tokio::test(flavor = "multi_thread")]
    async fn test_add_credential_upgrades_multiple_format() {
        let path = tmp_creds_path("add_cred_upgrade");
        std::fs::write(&path, "[]").unwrap();

        let manager =
            MultiTokenManager::new(Config::default(), vec![], None, Some(path.clone()), false)
                .unwrap();

        assert!(!manager.is_multiple_format.load(Ordering::Relaxed));

        let mut cred = KiroCredentials::default();
        cred.kiro_api_key = Some("ksk_test_upgrade_key".to_string());
        cred.auth_method = Some("api_key".to_string());

        manager.add_credential(cred).await.unwrap();

        assert!(
            manager.is_multiple_format.load(Ordering::Relaxed),
            "add_credential after is_multiple_format should upgrade to true"
        );

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.trim_start().starts_with('['),
            "add_credential afterwards the file should be in array format"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_add_credential_does_not_reuse_deleted_id() {
        let path = tmp_creds_path("add_cred_no_reuse_deleted_id");
        let mut cred1 = KiroCredentials::default();
        cred1.id = Some(1);
        cred1.kiro_api_key = Some("ksk_existing_1".to_string());
        cred1.auth_method = Some("api_key".to_string());

        let mut cred2 = KiroCredentials::default();
        cred2.id = Some(2);
        cred2.kiro_api_key = Some("ksk_existing_2".to_string());
        cred2.auth_method = Some("api_key".to_string());

        let manager = MultiTokenManager::new(
            Config::default(),
            vec![cred1, cred2],
            None,
            Some(path.clone()),
            true,
        )
        .unwrap();

        manager.delete_credential(2).unwrap();

        let mut new_cred = KiroCredentials::default();
        new_cred.kiro_api_key = Some("ksk_new_3".to_string());
        new_cred.auth_method = Some("api_key".to_string());

        let new_id = manager.add_credential(new_cred).await.unwrap();
        assert_eq!(
            new_id, 3,
            "new credential IDs must not reuse deleted IDs, otherwise historical failure logs attach to the new account"
        );

        let _ = std::fs::remove_file(&path);
    }

    // ── concurrent deduplication(TOCTOU regression guard) ───────────────────────────────────────────

    /// concurrently add multiple identical API Key credential, must insert only one.
    ///
    /// `add_credential` the deduplication precheck (step 2) and insertion (step 5) are not in the same critical section,
    /// token The refresh (network) completes outside the lock.8 concurrent tasks can easily have several pass the pre-check at once,
    /// without at this point"authoritative recheck at the insertion point"implementation would insert all duplicate credentials. This test is the regression guard for that.
    /// select API Key The credential is set so as to skip the network refresh, making the race reproducible purely locally.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_concurrent_add_same_api_key_inserts_once() {
        let path = tmp_creds_path("concurrent_dedup");
        let manager = Arc::new(
            MultiTokenManager::new(Config::default(), vec![], None, Some(path.clone()), true).unwrap(),
        );

        const N: usize = 8;
        let mut handles = Vec::with_capacity(N);
        for _ in 0..N {
            let m = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                let mut c = KiroCredentials::default();
                c.kiro_api_key = Some("ksk_duplicate".to_string());
                c.auth_method = Some("api_key".to_string());
                m.add_credential(c).await
            }));
        }

        let mut ok_count = 0_usize;
        for h in handles {
            if h.await.unwrap().is_ok() {
                ok_count += 1;
            }
        }
        assert_eq!(ok_count, 1, "Concurrently adding the same credential should succeed only once; actual successes {ok_count} times");

        let snapshot = manager.snapshot();
        assert_eq!(
            snapshot.entries.len(),
            1,
            "Should insert only one identical credential; actually {} entry",
            snapshot.entries.len()
        );

        let _ = std::fs::remove_file(&path);
    }

    // ── try_reload_credential_from_file ─────────────────────────────────────

    /// filehasnew refreshToken when,reload return true and update the in memory credential
    #[test]
    fn test_reload_from_file_succeeds_when_token_rotated() {
        let path = tmp_creds_path("reload_rotated");

        // initial token
        let mut cred = KiroCredentials::default();
        cred.id = Some(1);
        cred.refresh_token = Some("original_token_aaaa".repeat(10));
        let initial_json = serde_json::to_vec_pretty(&[&cred]).unwrap();
        std::fs::write(&path, &initial_json).unwrap();

        let manager = MultiTokenManager::new(
            Config::default(),
            vec![cred],
            None,
            Some(path.clone()),
            true,
        )
        .unwrap();

        // simulate IDE rotation:filewritenew token
        let mut updated_cred = KiroCredentials::default();
        updated_cred.id = Some(1);
        updated_cred.refresh_token = Some("rotated_token_bbbb".repeat(10));
        updated_cred.access_token = Some("new_access".to_string());
        let updated_json = serde_json::to_vec_pretty(&[&updated_cred]).unwrap();
        std::fs::write(&path, &updated_json).unwrap();

        let reloaded = manager.try_reload_credential_from_file(1);
        assert!(reloaded, "filehasnew token,reload should return true");

        let snapshot = manager.snapshot();
        let entry = snapshot.entries.iter().find(|e| e.id == 1).unwrap();
        assert!(!entry.disabled, "reload afterwards the credential should be re enabled");
        assert_eq!(entry.failure_count, 0);

        let _ = std::fs::remove_file(&path);
    }

    /// file token when identical to memory,reload return false(no update available)
    #[test]
    fn test_reload_from_file_returns_false_when_token_unchanged() {
        let path = tmp_creds_path("reload_unchanged");

        let mut cred = KiroCredentials::default();
        cred.id = Some(1);
        cred.refresh_token = Some("same_token".repeat(15));
        let json = serde_json::to_vec_pretty(&[&cred]).unwrap();
        std::fs::write(&path, &json).unwrap();

        let manager = MultiTokenManager::new(
            Config::default(),
            vec![cred],
            None,
            Some(path.clone()),
            true,
        )
        .unwrap();

        let reloaded = manager.try_reload_credential_from_file(1);
        assert!(!reloaded, "token unchanged,reload should return false");

        let _ = std::fs::remove_file(&path);
    }

    /// not configured credentials_path when,reload return false
    #[test]
    fn test_reload_from_file_returns_false_without_path() {
        let mut cred = KiroCredentials::default();
        cred.id = Some(1);
        cred.refresh_token = Some("some_token".repeat(15));

        let manager = MultiTokenManager::new(
            Config::default(),
            vec![cred],
            None,
            None, // nonefilepath
            false,
        )
        .unwrap();

        let reloaded = manager.try_reload_credential_from_file(1);
        assert!(!reloaded, "none credentials_path should return when false");
    }

    /// singlecredentialfilenone ID field, matches via the single credential rule.
    #[test]
    fn test_reload_from_file_single_credential_no_id() {
        let path = tmp_creds_path("reload_single_no_id");

        // initial: none ID field
        let mut cred = KiroCredentials::default();
        cred.refresh_token = Some("original_no_id".repeat(10));
        let initial_json = serde_json::to_vec_pretty(&[&cred]).unwrap();
        std::fs::write(&path, &initial_json).unwrap();

        let manager = MultiTokenManager::new(
            Config::default(),
            vec![cred],
            None,
            Some(path.clone()),
            true,
        )
        .unwrap();

        // fileupdateasnew token(no ID)
        let mut updated = KiroCredentials::default();
        updated.refresh_token = Some("rotated_no_id".repeat(10));
        let updated_json = serde_json::to_vec_pretty(&[&updated]).unwrap();
        std::fs::write(&path, &updated_json).unwrap();

        // get actual ID(manager automatic allocation)
        let actual_id = manager.snapshot().entries[0].id;
        let reloaded = manager.try_reload_credential_from_file(actual_id);
        assert!(reloaded, "single credential none ID should still be able to match and when reload");

        let _ = std::fs::remove_file(&path);
    }

    // ===== account group isolation regression test =====

    /// construct acarry token, an available credential belonging to the given group.
    fn grouped_cred(token: &str, groups: &[&str]) -> KiroCredentials {
        let mut c = KiroCredentials::default();
        c.access_token = Some(token.to_string());
        c.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());
        c.groups = groups.iter().map(|s| s.to_string()).collect();
        c
    }

    #[test]
    fn test_group_matches_helper() {
        // not boundgroup(None)match any account
        assert!(group_matches(&[], None));
        assert!(group_matches(&["g1".to_string()], None));
        // when binding a group only match groups containsthisnameofaccount
        assert!(group_matches(&["g1".to_string(), "g2".to_string()], Some("g1")));
        assert!(!group_matches(&["g2".to_string()], Some("g1")));
        assert!(!group_matches(&[], Some("g1")));
    }

    #[test]
    fn test_select_next_credential_filters_by_group() {
        // A∈g1, B∈g2, C∈no group
        let manager = MultiTokenManager::new(
            Config::default(),
            vec![
                grouped_cred("a", &["g1"]),
                grouped_cred("b", &["g2"]),
                grouped_cred("c", &[]),
            ],
            None,
            None,
            false,
        )
        .unwrap();

        // g1 can only select A(id=1)
        let g1 = manager.select_next_credential(None, Some("g1"));
        assert_eq!(g1.map(|(id, _)| id), Some(1));
        // g2 can only select B(id=2)
        let g2 = manager.select_next_credential(None, Some("g2"));
        assert_eq!(g2.map(|(id, _)| id), Some(2));
        // does not existofgroup → noneavailableaccount
        assert!(manager.select_next_credential(None, Some("nope")).is_none());
        // not boundgroup(None) → optionaltoaccount
        assert!(manager.select_next_credential(None, None).is_some());
    }

    #[tokio::test]
    async fn test_acquire_context_priority_current_respects_model_support() {
        let mut free_cred = grouped_cred("free", &[]);
        free_cred.subscription_title = Some("KIRO FREE".to_string());

        let mut pro_cred = grouped_cred("pro", &[]);
        pro_cred.subscription_title = Some("KIRO PRO".to_string());
        pro_cred.priority = 10;

        let manager =
            MultiTokenManager::new(Config::default(), vec![free_cred, pro_cred], None, None, false)
                .unwrap();

        // Warm current_id with the highest-priority Free account.
        let current = manager.acquire_context(None, None).await.unwrap();
        assert_eq!(current.id, 1);

        let opus = manager
            .acquire_context(Some("claude-opus-4.6"), None)
            .await
            .unwrap();
        assert_eq!(
            opus.id, 2,
            "priority current_id must not bypass Opus subscription filtering"
        );
    }

    #[test]
    fn test_total_count_in_group() {
        let manager = MultiTokenManager::new(
            Config::default(),
            vec![
                grouped_cred("a", &["g1"]),
                grouped_cred("b", &["g1", "g2"]),
                grouped_cred("c", &[]),
            ],
            None,
            None,
            false,
        )
        .unwrap();

        assert_eq!(manager.total_count_in_group(Some("g1")), 2); // A,B
        assert_eq!(manager.total_count_in_group(Some("g2")), 1); // B
        assert_eq!(manager.total_count_in_group(None), 3); // all
        assert_eq!(manager.total_count_in_group(Some("none")), 0);
    }

    #[test]
    fn test_balanced_mode_independent_per_group() {
        let mut config = Config::default();
        config.load_balancing_mode = "balanced".to_string();
        // g1: A(id1),B(id2);g2: C(id3)
        let manager = MultiTokenManager::new(
            config,
            vec![
                grouped_cred("a", &["g1"]),
                grouped_cred("b", &["g1"]),
                grouped_cred("c", &["g2"]),
            ],
            None,
            None,
            false,
        )
        .unwrap();

        // let A(id1) successifcleantimes → balanced should redirect to success_count smaller B(id2)
        manager.report_success(1);
        manager.report_success(1);
        let pick = manager.select_next_credential(None, Some("g1"));
        assert_eq!(pick.map(|(id, _)| id), Some(2), "balanced should be in g1 select within success_count smallest B");
        // g2 not affected by g1 count effect, will still only select C(id3)
        let pick_g2 = manager.select_next_credential(None, Some("g2"));
        assert_eq!(pick_g2.map(|(id, _)| id), Some(3));
    }

    #[tokio::test]
    async fn test_acquire_context_strict_isolation_fails_when_group_empty() {
        // g1 onlyaaccount A(id1),disableafterbind g1 the request should fail directly and not fall back to g2/no group
        let manager = MultiTokenManager::new(
            Config::default(),
            vec![
                grouped_cred("a", &["g1"]),
                grouped_cred("b", &["g2"]),
                grouped_cred("c", &[]),
            ],
            None,
            None,
            false,
        )
        .unwrap();

        // under normal conditions g1 can get context
        assert!(manager.acquire_context(None, Some("g1")).await.is_ok());

        // manually disable g1 insideuniqueaccount A(id1)
        manager.set_disabled(1, true).unwrap();

        // strict isolation:g1 noneavailableaccount → Err, andnotwill selectto B/C
        let res = manager.acquire_context(None, Some("g1")).await;
        assert!(res.is_err(), "g1 After all accounts within it are disabled, it should fail and not fall back to other groups.");

        // but g2 still available
        assert!(manager.acquire_context(None, Some("g2")).await.is_ok());
    }
}
