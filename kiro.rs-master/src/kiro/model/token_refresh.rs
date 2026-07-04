use serde::{Deserialize, Serialize};

/// refresh Token request body (Social auth)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// refresh Token response body (Social auth)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub profile_arn: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

/// IdC Token refreshrequest body (AWS SSO OIDC)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdcRefreshRequest {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub grant_type: String,
}

/// IdC Token refreshresponse body (AWS SSO OIDC)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdcRefreshResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub profile_arn: Option<String>,
}

// ============ AWS SSO OIDC device authorizationflow ============

/// register OIDC clientrequest body
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterClientRequest {
    pub client_name: String,
    pub client_type: String,
    pub scopes: Vec<String>,
    pub grant_types: Vec<String>,
    pub issuer_url: String,
}

/// register OIDC clientresponse body
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterClientResponse {
    pub client_id: String,
    pub client_secret: String,
    // Upstream field, used only for complete deserialization of the record; the current flow does not depend on its specific value.
    #[allow(dead_code)]
    pub client_id_issued_at: Option<i64>,
    #[allow(dead_code)]
    pub client_secret_expires_at: Option<i64>,
}

/// initiate device authorization request body
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDeviceAuthorizationRequest {
    pub client_id: String,
    pub client_secret: String,
    pub start_url: String,
}

/// initiate device authorization response body
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDeviceAuthorizationResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: i64,
    pub interval: i64,
}

/// poll Token request body (device authorization)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenRequest {
    pub client_id: String,
    pub client_secret: String,
    pub grant_type: String,
    pub device_code: String,
}

/// poll Token response body
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

/// AWS SSO OIDC errorresponse
#[derive(Debug, Deserialize)]
pub struct OidcErrorResponse {
    pub error: String,
    // Detailed description for logging, kept during deserialization for troubleshooting.
    #[allow(dead_code)]
    #[serde(default)]
    pub error_description: Option<String>,
}

// ============ Social (Portal) login flow ============

/// Social token exchangerequest body(PKCE)
#[derive(Debug, Serialize)]
pub struct SocialCreateTokenRequest {
    pub code: String,
    pub code_verifier: String,
    pub redirect_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_code: Option<String>,
}

/// Social token response body
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialCreateTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub profile_arn: Option<String>,
}
