//! common authentication utility function

use axum::{
    body::Body,
    http::{Request, header},
};
use subtle::ConstantTimeEq;

/// fromrequestextract from API Key
///
/// supports two authentication methods:
/// - `x-api-key` header
/// - `Authorization: Bearer <token>` header
pub fn extract_api_key(request: &Request<Body>) -> Option<String> {
    // prioritycheck x-api-key
    if let Some(key) = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
    {
        return Some(key.to_string());
    }

    // then check Authorization: Bearer
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Constant time string comparison to prevent timing attacks.
///
/// Regardless of the string content, the time required for the comparison is constant,
/// This prevents an attacker from guessing the login by measuring response time.APIkey.
///
/// use a security audited `subtle` crate implement
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}
