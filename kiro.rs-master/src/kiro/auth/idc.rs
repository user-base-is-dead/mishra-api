//! AWS SSO OIDC device authorization login flow
//!
//! implement a three step flow:
//! 1. register OIDC client (register_client)
//! 2. Initiates device authorization to obtain the user verification code (start_device_authorization)
//! 3. Polls the token endpoint, waiting for the user to complete authorization (poll_token)

use anyhow::Context;

use crate::http_client::{ProxyConfig, build_client};
use crate::kiro::model::token_refresh::{
    CreateTokenRequest, CreateTokenResponse, OidcErrorResponse, RegisterClientRequest,
    RegisterClientResponse, StartDeviceAuthorizationRequest, StartDeviceAuthorizationResponse,
};
use crate::model::config::Config;

/// device authorization polling result
#[derive(Debug)]
pub enum PollResult {
    /// The user has not completed authorization yet; keeps waiting.
    Pending,
    /// authorization succeeded, return token
    Success(CreateTokenResponse),
    /// The device code has expired; it must be started again.
    Expired,
    /// other error
    Error(anyhow::Error),
}

/// AWS Builder ID / IAM Identity Center default Start URL
pub const BUILDER_ID_START_URL: &str = "https://view.awsapps.com/start";

/// Kiro IDE used OIDC scope
const KIRO_SCOPES: &[&str] = &[
    "codewhisperer:completions",
    "codewhisperer:analysis",
    "codewhisperer:conversations",
    "codewhisperer:transformations",
    "codewhisperer:taskassist",
];

fn oidc_endpoint(region: &str) -> String {
    format!("https://oidc.{}.amazonaws.com", region)
}

/// register OIDC client
///
/// Called before each device authorization to obtain clientId and clientSecret.
/// The registration result has an expiry (usually a few days), but here it re-registers each time to keep things simple.
/// `start_url` as issuerUrl togethercommit:Builder ID as default Start URL,
/// enterprise IAM Identity Center asorganizes itselfof Start URL.
pub async fn register_client(
    region: &str,
    start_url: &str,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<RegisterClientResponse> {
    let url = format!("{}/client/register", oidc_endpoint(region));
    let client = build_client(proxy, 30, config.tls_backend)?;

    let body = RegisterClientRequest {
        client_name: "kiro-rs".to_string(),
        client_type: "public".to_string(),
        scopes: KIRO_SCOPES.iter().map(|s| s.to_string()).collect(),
        grant_types: vec![
            "urn:ietf:params:oauth:grant-type:device_code".to_string(),
            "refresh_token".to_string(),
        ],
        issuer_url: start_url.to_string(),
    };

    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .header("host", format!("oidc.{}.amazonaws.com", region))
        .json(&body)
        .send()
        .await
        .context("register OIDC client request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("register OIDC clientfailed {}: {}", status, body_text);
    }

    resp.json::<RegisterClientResponse>()
        .await
        .context("failed to parse the registration response")
}

/// Initiates device authorization, returning the verification code for the user to access and URL
pub async fn start_device_authorization(
    region: &str,
    start_url: &str,
    client_id: &str,
    client_secret: &str,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<StartDeviceAuthorizationResponse> {
    let url = format!("{}/device_authorization", oidc_endpoint(region));
    let client = build_client(proxy, 30, config.tls_backend)?;

    let body = StartDeviceAuthorizationRequest {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        start_url: start_url.to_string(),
    };

    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .header("host", format!("oidc.{}.amazonaws.com", region))
        .json(&body)
        .send()
        .await
        .context("failed to initiate the device authorization request")?;

    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("failed to initiate device authorization {}: {}", status, body_text);
    }

    resp.json::<StartDeviceAuthorizationResponse>()
        .await
        .context("failed to parse the device authorization response")
}

/// poll the token endpoint once
///
/// return `PollResult`, the caller decides whether to keep polling.
pub async fn poll_token(
    region: &str,
    client_id: &str,
    client_secret: &str,
    device_code: &str,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> PollResult {
    let url = format!("{}/token", oidc_endpoint(region));
    let client = match build_client(proxy, 30, config.tls_backend) {
        Ok(c) => c,
        Err(e) => return PollResult::Error(e),
    };

    let body = CreateTokenRequest {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
        device_code: device_code.to_string(),
    };

    let resp = match client
        .post(&url)
        .header("content-type", "application/json")
        .header("host", format!("oidc.{}.amazonaws.com", region))
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return PollResult::Error(e.into()),
    };

    let status = resp.status();

    if status.is_success() {
        return match resp.json::<CreateTokenResponse>().await {
            Ok(token) => PollResult::Success(token),
            Err(e) => PollResult::Error(e.into()),
        };
    }

    let body_text = match resp.text().await {
        Ok(t) => t,
        Err(e) => return PollResult::Error(e.into()),
    };

    // parse standard OIDC error code
    if let Ok(err_resp) = serde_json::from_str::<OidcErrorResponse>(&body_text) {
        match err_resp.error.as_str() {
            "authorization_pending" => return PollResult::Pending,
            "slow_down" => return PollResult::Pending,
            "expired_token" => return PollResult::Expired,
            "access_denied" => return PollResult::Error(anyhow::anyhow!("the user rejected the authorization request")),
            _ => {}
        }
    }

    PollResult::Error(anyhow::anyhow!("polltokenfailed {}: {}", status, body_text))
}
