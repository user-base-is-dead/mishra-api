//! Kiro endpoint abstraction
//!
//! different Kiro endpoint (such as `ide` / `cli`) in URL, differences exist in the headers and request body,
//! but share the credential pool,Token refresh, retry logic and AWS event-stream responsedecode.
//!
//! [`KiroEndpoint`] Abstracts the difference points on the request side;`KiroProvider` holds a endpoint registry,
//! by credential `endpoint` field selects the corresponding implementation.

use reqwest::RequestBuilder;

use crate::kiro::model::credentials::KiroCredentials;
use crate::model::config::Config;

pub mod cli;
pub mod ide;

pub use cli::CliEndpoint;
pub use ide::IdeEndpoint;

/// Kiro endpoint
///
/// the same `KiroProvider` canholdhasmanyitem endpoint implementation, switches by the credential level field.
pub trait KiroEndpoint: Send + Sync {
    /// endpoint name (corresponds to credentials.endpoint / config.defaultEndpoint value)
    fn name(&self) -> &'static str;

    /// API request Content-Type(default application/json)
    fn content_type(&self) -> &'static str {
        "application/json"
    }

    /// API endpoint URL
    fn api_url(&self, ctx: &RequestContext<'_>) -> String;

    /// MCP endpoint URL
    fn mcp_url(&self, ctx: &RequestContext<'_>) -> String;

    /// decorate API specific to the requested endpoint header
    ///
    /// Provider alreadyalreadysetgood URL,content-type,Connection and body;
    /// implementresponsibleappend Authorization,host,user-agent and other endpoint related headers.
    fn decorate_api(&self, req: RequestBuilder, ctx: &RequestContext<'_>) -> RequestBuilder;

    /// decorate MCP specific to the requested endpoint header
    fn decorate_mcp(&self, req: RequestBuilder, ctx: &RequestContext<'_>) -> RequestBuilder;

    /// foralreadyserializeof API Applies endpoint specific processing to the request body (such as injecting profileArn)
    fn transform_api_body(&self, body: &str, ctx: &RequestContext<'_>) -> String;

    /// foralreadyserializeof MCP Applies endpoint specific processing to the request body (unchanged by default).
    fn transform_mcp_body(&self, body: &str, _ctx: &RequestContext<'_>) -> String {
        body.to_string()
    }

    /// determine whether the response body indicates"monthly quotauseexhaust"(disable the credential and transfer)
    fn is_monthly_request_limit(&self, body: &str) -> bool {
        default_is_monthly_request_limit(body)
    }

    /// determine whether the response body indicates"upstream bearer token invalid"(trigger a forced refresh)
    fn is_bearer_token_invalid(&self, body: &str) -> bool {
        default_is_bearer_token_invalid(body)
    }

    /// determine whether the response body indicates"account level temporary throttle"(429 + suspicious activity)
    ///
    /// with normal 429(high traffic) to distinguish: account level throttle only takes effect on the current credential,
    /// After failing over to another credential it recovers immediately; an ordinary 429 is an upstream global overload; switching is meaningless.
    fn is_account_throttled(&self, body: &str) -> bool {
        default_is_account_throttled(body)
    }

    /// determine whether the response body indicates"client request format error"(messages the array itself violates the protocol)
    ///
    /// this kinderror(tool_use↔tool_result unpaired, illegal message sequence, and so on) the root cause is the caller
    /// the request body rather than an upstream fault. Regardless of how upstream 4xx or still 5xx returns; retrying can never succeed;
    /// especiallywhenupstreamto 5xx On return, retrying it as a transient error would turn a bad request that can never succeed into
    /// placelargeintomanytimes 503(503 storm) and needlessly consuming the retry budget. Once identified, terminate immediately,
    /// do not retry, do not switch credentials.
    fn is_client_validation_error(&self, body: &str) -> bool {
        default_is_client_validation_error(body)
    }

    /// Determines whether the response body indicates an upstream gateway timeout.
    ///
    /// 524 usually from Cloudflare/edge layer; continuing to retry within the same client call would push the wait time
    /// amplified to the client own retry ceiling; letting the caller fail fast is better for reconnecting on the next request.
    fn is_gateway_timeout(&self, body: &str) -> bool {
        default_is_gateway_timeout(body)
    }
}

/// The context available when decorating a request.
///
/// Contains all runtime information determined for a single call. The reference form avoids needless clone.
pub struct RequestContext<'a> {
    /// current credential
    pub credentials: &'a KiroCredentials,
    /// valid access token(API Key under credential kiroApiKey)
    pub token: &'a str,
    /// the one corresponding to the current credential machineId
    pub machine_id: &'a str,
    /// global config
    pub config: &'a Config,
}

/// trigger"quota exhausted → disableand switch"of reason value set
///
/// - `MONTHLY_REQUEST_COUNT`: the monthly request quota is exhausted
/// - `OVERAGE_REQUEST_LIMIT_EXCEEDED`: overage (overage)quotaalso exhausted
///
/// Both semantics mean the credential cannot be used again in the current billing period; handling is the same:
/// Immediately disables the credential and fails over to the next available credential.
const QUOTA_EXHAUSTED_REASONS: &[&str] = &[
    "MONTHLY_REQUEST_COUNT",
    "OVERAGE_REQUEST_LIMIT_EXCEEDED",
];

/// default"requestquota exhausted"decision logic
///
/// samewhenidentifytop level `reason` fieldandnested `error.reason` field.
/// any known quota is exhausted reason hitthat isreturn true.
pub fn default_is_monthly_request_limit(body: &str) -> bool {
    // First does a fast string scan to avoid 99% the response body of a miss does JSON parse
    if QUOTA_EXHAUSTED_REASONS.iter().any(|r| body.contains(r)) {
        // further use JSON parse confirm reason field rather than an incidentally appearing substring.
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
            let top = value.get("reason").and_then(|v| v.as_str());
            let nested = value.pointer("/error/reason").and_then(|v| v.as_str());
            return [top, nested]
                .into_iter()
                .flatten()
                .any(|r| QUOTA_EXHAUSTED_REASONS.contains(&r));
        }
        // body is non JSON but contains the keyword (compatible with a simple text response).
        return true;
    }
    false
}

/// default bearer token invaliddecision logic
pub fn default_is_bearer_token_invalid(body: &str) -> bool {
    body.contains("The bearer token included in the request is invalid")
}

/// The default account level throttle judgment logic.
///
/// upstream Kiro/Q-Developer throttlewillreturn 429 + similar:
/// `Due to suspicious activity, we are imposing temporary limits on how
/// frequently your account (d-...) can send a request to Kiro while we investigate.`
///
/// with normal 429(high traffic / rate limit exceeded) the key difference is
/// mention "suspicious activity" andspecific account ID.
pub fn default_is_account_throttled(body: &str) -> bool {
    body.contains("suspicious activity")
        && body.contains("temporary limits")
}

/// The default upstream gateway timeout judgment logic.
pub fn default_is_gateway_timeout(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    body.contains("524")
        && (lower.contains("status code")
            || lower.contains("gateway timeout")
            || lower.contains("server-side issue"))
}

/// trigger"client request format error → terminate immediately, do not retry"exact reason value set
///
/// these are all the upstream messages The array itself failed protocol validation (root cause is the caller request body,
/// rather than an upstream fault). Only includes**exact reason value**, not included `ValidationException`
/// this kind of broad exception type——The latter is too broad; a bare substring match would treat a real upstream that happens to carry the word
/// misjudge a transient fault as"not retryable", instead killing a request that could recover on retry.
const CLIENT_VALIDATION_REASONS: &[&str] = &["TOOL_USE_RESULT_MISMATCH"];

/// the one triggering the same kind of determination message level characteristic phrase (used when there is no structured reason, text only message scenarios)
///
/// for example Bedrock of "Expected toolResult blocks ..." plain text error. The phrase must have
/// Specific enough not to conflict with normal response content.
const CLIENT_VALIDATION_MESSAGE_MARKERS: &[&str] = &["Expected toolResult blocks"];

/// default"client request format error"decision logic
///
/// and [`default_is_monthly_request_limit`] Same structure: first do a cheap substring quick scan, then after a hit use
/// JSON parse confirm `reason`(top levelandnested `error.reason`) field, avoiding treating one that incidentally appears in
/// misjudging a keyword in an ordinary field. When structured confirmation fails, falls back to message level specific phrase matching,
/// to cover non JSON the plain text error message.
pub fn default_is_client_validation_error(body: &str) -> bool {
    let reason_hit = CLIENT_VALIDATION_REASONS.iter().any(|r| body.contains(r));
    if reason_hit {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
            let top = value.get("reason").and_then(|v| v.as_str());
            let nested = value.pointer("/error/reason").and_then(|v| v.as_str());
            if [top, nested]
                .into_iter()
                .flatten()
                .any(|r| CLIENT_VALIDATION_REASONS.contains(&r))
            {
                return true;
            }
        } else {
            // non JSON but contains exact reason keyword (compatible with a simple text response).
            return true;
        }
    }
    // message level fallback: a plain text error message (no structured reason)
    CLIENT_VALIDATION_MESSAGE_MARKERS
        .iter()
        .any(|m| body.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_monthly_request_limit_detects_reason() {
        let body = r#"{"message":"You have reached the limit.","reason":"MONTHLY_REQUEST_COUNT"}"#;
        assert!(default_is_monthly_request_limit(body));
    }

    #[test]
    fn test_default_monthly_request_limit_nested_reason() {
        let body = r#"{"error":{"reason":"MONTHLY_REQUEST_COUNT"}}"#;
        assert!(default_is_monthly_request_limit(body));
    }

    #[test]
    fn test_default_monthly_request_limit_false() {
        let body = r#"{"message":"nope","reason":"DAILY_REQUEST_COUNT"}"#;
        assert!(!default_is_monthly_request_limit(body));
    }

    #[test]
    fn test_default_quota_exhausted_overage() {
        let body = r#"{"message":"You have reached the limit for overages.","reason":"OVERAGE_REQUEST_LIMIT_EXCEEDED"}"#;
        assert!(default_is_monthly_request_limit(body));
    }

    #[test]
    fn test_default_quota_exhausted_overage_nested() {
        let body = r#"{"error":{"reason":"OVERAGE_REQUEST_LIMIT_EXCEEDED"}}"#;
        assert!(default_is_monthly_request_limit(body));
    }

    #[test]
    fn test_default_quota_exhausted_substring_does_not_false_match() {
        // The keyword appears in an ordinary field rather than reason field: still hits (backward compatible with old behavior).
        // but reason When the field is another value it should strictly not hit.
        let body =
            r#"{"message":"some text MONTHLY_REQUEST_COUNT-like phrase","reason":"OTHER"}"#;
        assert!(!default_is_monthly_request_limit(body));
    }

    #[test]
    fn test_default_bearer_token_invalid() {
        assert!(default_is_bearer_token_invalid(
            "The bearer token included in the request is invalid"
        ));
        assert!(!default_is_bearer_token_invalid("unrelated error"));
    }

    #[test]
    fn test_default_is_account_throttled() {
        let body = r#"{"message":"Due to suspicious activity, we are imposing temporary limits on how frequently your account (d-9067c98495.84f894a8) can send a request to Kiro while we investigate.","reason":null}"#;
        assert!(default_is_account_throttled(body));
        // normal 429 should not be recognized as account throttle
        assert!(!default_is_account_throttled(
            "{\"message\":\"Too many requests\"}"
        ));
        // does not hit even when only half the keywords are present.
        assert!(!default_is_account_throttled("suspicious activity detected"));
    }

    #[test]
    fn test_default_is_gateway_timeout() {
        assert!(default_is_gateway_timeout(
            "API Error: 524 status code (no body). This is a server-side issue"
        ));
        assert!(default_is_gateway_timeout("524 Gateway Timeout"));
        assert!(!default_is_gateway_timeout(
            r#"{"message":"some unrelated field mentions 524 tokens"}"#
        ));
    }

    #[test]
    fn test_default_is_client_validation_error() {
        // top level reason hit (structured confirmation)
        assert!(default_is_client_validation_error(
            r#"{"reason":"TOOL_USE_RESULT_MISMATCH"}"#
        ));
        // nested error.reason hit
        assert!(default_is_client_validation_error(
            r#"{"error":{"reason":"TOOL_USE_RESULT_MISMATCH"}}"#
        ));
        // non JSON but contains exact reason keyword
        assert!(default_is_client_validation_error(
            "upstream error: TOOL_USE_RESULT_MISMATCH"
        ));
        // message level specific phrase (plain text, no structured reason)
        assert!(default_is_client_validation_error(
            "Expected toolResult blocks but found none"
        ));

        // An ordinary upstream error should not be misjudged (otherwise the proper retry would be skipped).
        assert!(!default_is_client_validation_error(
            r#"{"message":"Internal server error"}"#
        ));
        assert!(!default_is_client_validation_error("connection reset by peer"));
        // keyregression:reason The keyword incidentally appears in an ordinary field, but the real reason is another value —— notshould hit
        // (otherwise a real upstream fault that could recover on retry would be wrongly killed)
        assert!(!default_is_client_validation_error(
            r#"{"message":"trace mentions TOOL_USE_RESULT_MISMATCH internally","reason":"INTERNAL_SERVER_ERROR"}"#
        ));
        // broad ValidationException no longer hits separately (no exact reason / when there is no specific phrase)
        assert!(!default_is_client_validation_error(
            r#"{"__type":"ValidationException","message":"some other validation"}"#
        ));
    }
}
