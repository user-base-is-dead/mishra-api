//! Anthropic API Handler function

use std::convert::Infallible;
use std::time::Instant;

use crate::admin::client_keys::SharedClientKeyManager;
use crate::admin::usage_stats::{SharedAggregator, SharedRecorder, UsageRecord};
use crate::admin::trace_db::{
    SharedTraceStore, TraceAttempt, TraceKeySource, TraceRecord, TraceSink, outcome,
};
use crate::kiro::model::events::Event;
use crate::kiro::model::requests::kiro::KiroRequest;
use crate::kiro::parser::decoder::EventStreamDecoder;
use crate::token;
use anyhow::Error;
use axum::{
    Json as JsonExtractor,
    body::Body,
    extract::{Extension, State},
    http::{StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;
use chrono::Utc;
use futures::{Stream, StreamExt, stream};
use serde_json::json;
use std::time::Duration;
use tokio::time::interval;
use uuid::Uuid;

use super::converter::{ConversionError, convert_request};
use super::middleware::{AppState, KeyContext};
use super::stream::{BufferedStreamContext, SseEvent, StreamContext};
use super::types::{
    CountTokensRequest, CountTokensResponse, ErrorResponse, MessagesRequest, Model, ModelsResponse,
    OutputConfig, Thinking,
};
use super::websearch;

/// The hook that records usage at request end.
///
/// in handler entry construction, call [`Self::record`] when take this request input/output token,
/// the hit upstream credential ID,statewrite:
/// - `usage_log.YYYY-MM-DD.jsonl`(persist history)
/// - In-memory aggregator (dashboard trend).
/// - client Key countcount(by Key cumulative)
#[derive(Clone)]
pub(crate) struct UsageRecordHook {
    pub recorder: Option<SharedRecorder>,
    pub aggregator: Option<SharedAggregator>,
    pub client_keys: Option<SharedClientKeyManager>,
    pub key_id: u64,
    pub model: String,
    pub started_at: Instant,
}

impl UsageRecordHook {
    pub fn from_state(state: &AppState, key_id: u64, model: String) -> Self {
        Self {
            recorder: state.usage_recorder.clone(),
            aggregator: state.usage_aggregator.clone(),
            client_keys: state.client_keys.clone(),
            key_id,
            model,
            started_at: Instant::now(),
        }
    }

    pub fn record(
        &self,
        credential_id: u64,
        input_tokens: i32,
        output_tokens: i32,
        cache_creation_tokens: i32,
        cache_read_tokens: i32,
        credits: f64,
        status: &str,
    ) {
        let rec = UsageRecord {
            ts: Utc::now().to_rfc3339(),
            key_id: self.key_id,
            credential_id,
            model: self.model.clone(),
            input_tokens: input_tokens.max(0) as u64,
            output_tokens: output_tokens.max(0) as u64,
            cache_creation_tokens: cache_creation_tokens.max(0) as u64,
            cache_read_tokens: cache_read_tokens.max(0) as u64,
            credits: if credits.is_finite() && credits > 0.0 {
                credits
            } else {
                0.0
            },
            duration_ms: self.started_at.elapsed().as_millis() as u64,
            status: status.to_string(),
        };
        if let Some(r) = &self.recorder {
            r.record(&rec);
        }
        if let Some(a) = &self.aggregator {
            a.ingest(&rec);
        }
        if status == "success" && self.key_id != 0 {
            if let Some(m) = &self.client_keys {
                m.record_usage(
                    self.key_id,
                    rec.input_tokens,
                    rec.output_tokens,
                    rec.cache_creation_tokens,
                    rec.cache_read_tokens,
                    rec.credits,
                );
            }
        }
    }
}

/// trace tracker for a single request
///
/// in handler entry construction, as [`TraceSink`] pass in provider;provider inretry loopin
/// each hopcall [`on_attempt`](TraceSink::on_attempt) accumulateoneentry [`TraceAttempt`].
/// called at the end of the request [`Self::finalize`] assemble [`TraceRecord`] and write SQLite.
///
/// `store` as None(not enabled Admin / trace) all methods are no ops, with zero overhead.
pub(crate) struct RequestTracer {
    store: Option<SharedTraceStore>,
    trace_id: String,
    ts: String,
    key_id: u64,
    key_source: TraceKeySource,
    model: String,
    is_stream: bool,
    started_at: Instant,
    /// firstitemupstream chunk Arrival moment (streaming mark only; takes the first).
    first_token_at: parking_lot::Mutex<Option<Instant>>,
    attempts: parking_lot::Mutex<Vec<TraceAttempt>>,
}

/// The usage snapshot of this request (placed into trace line, with usage_log same origin)
#[derive(Clone, Copy, Default)]
pub(crate) struct TraceUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub credits: f64,
}

impl TraceUsage {
    /// scenarios with no usage such as early error exit
    pub fn zero() -> Self {
        Self::default()
    }
}

struct RequestTraceOptions {
    key_ctx: KeyContext,
    model: String,
    is_stream: bool,
}

impl RequestTracer {
    fn new(state: &AppState, options: RequestTraceOptions) -> Self {
        Self {
            store: state.trace_store.clone(),
            trace_id: Uuid::new_v4().to_string(),
            ts: Utc::now().to_rfc3339(),
            key_id: options.key_ctx.key_id,
            key_source: options.key_ctx.key_source,
            model: options.model,
            is_stream: options.is_stream,
            started_at: Instant::now(),
            first_token_at: parking_lot::Mutex::new(None),
            attempts: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// markfirstitemupstream chunk arrives (idempotent, records only the first time).
    pub fn mark_first_token(&self) {
        let mut slot = self.first_token_at.lock();
        if slot.is_none() {
            *slot = Some(Instant::now());
        }
    }

    /// Assembles and persists one complete chain.store as None does nothing when.
    pub fn finalize(
        &self,
        final_status: &str,
        error_type: Option<&str>,
        error_message: Option<&str>,
        interrupted_after_bytes: Option<u64>,
        usage: TraceUsage,
    ) {
        let Some(store) = &self.store else { return };
        let attempts = std::mem::take(&mut *self.attempts.lock());
        // Final credential: the hit credential of the last hop (a success hop is the hit credential, a failed hop is the last attempted credential).
        let final_credential_id = attempts.last().map(|a| a.credential_id).unwrap_or(0);
        let first_token_ms = self
            .first_token_at
            .lock()
            .map(|t| t.duration_since(self.started_at).as_millis() as u64);
        let rec = TraceRecord {
            trace_id: self.trace_id.clone(),
            ts: self.ts.clone(),
            key_id: self.key_id,
            key_source: self.key_source,
            model: self.model.clone(),
            is_stream: self.is_stream,
            final_status: final_status.to_string(),
            final_credential_id,
            error_type: error_type.map(|s| s.to_string()),
            error_message: error_message.map(|s| s.to_string()),
            total_attempts: attempts.len() as u32,
            duration_ms: self.started_at.elapsed().as_millis() as u64,
            interrupted_after_bytes,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            credits: usage.credits,
            first_token_ms,
            attempts,
        };
        store.insert(&rec);
    }
}

impl TraceSink for RequestTracer {
    fn on_attempt(&self, attempt: TraceAttempt) {
        self.attempts.lock().push(attempt);
    }
}

/// take the last hop in the tracker outcome(used fortake provider the failure classification is elevated to record.error_type).
/// return 'static str(outcome constant),none attempt return when None.
fn last_attempt_outcome(tracer: &RequestTracer) -> Option<&'static str> {
    let last = tracer.attempts.lock().last()?.outcome.clone();
    Some(match last.as_str() {
        outcome::QUOTA_EXHAUSTED => outcome::QUOTA_EXHAUSTED,
        outcome::ACCOUNT_THROTTLED => outcome::ACCOUNT_THROTTLED,
        outcome::AUTH_FAILED => outcome::AUTH_FAILED,
        outcome::TRANSIENT => outcome::TRANSIENT,
        outcome::NETWORK_ERROR => outcome::NETWORK_ERROR,
        outcome::BAD_REQUEST => outcome::BAD_REQUEST,
        _ => outcome::UNKNOWN,
    })
}

/// Image-budget warning threshold (in raw base64 chars, not decoded bytes).
/// Emits a warning when the total base64 char count of all image content in one request exceeds this threshold.
/// The threshold does not reject the request (the upstream makes the final call); it only gives operators more precise diagnostics.
const IMAGE_BUDGET_WARN_BYTES: usize = 800 * 1024;

/// Budget statistics for the image content in one inbound request.
struct ImageBudget {
    count: usize,
    total_b64_bytes: usize,
    largest_b64_bytes: usize,
}

/// Counts the total number of images in the payload and their base64 byte size.
/// Looks only at inline base64 (image source.type == "base64"), skipping url-mode images (which do not
/// go directly into a Bedrock single message body). This is a lightweight O(N) scan that does not decode base64.
fn count_image_budget(payload: &super::types::MessagesRequest) -> ImageBudget {
    let mut count = 0usize;
    let mut total = 0usize;
    let mut largest = 0usize;
    for msg in &payload.messages {
        if let serde_json::Value::Array(arr) = &msg.content {
            for item in arr {
                if item.get("type").and_then(|v| v.as_str()) != Some("image") {
                    continue;
                }
                let Some(src) = item.get("source") else { continue };
                if src.get("type").and_then(|v| v.as_str()) != Some("base64") {
                    continue;
                }
                let n = src.get("data").and_then(|v| v.as_str()).map(|s| s.len()).unwrap_or(0);
                count += 1;
                total += n;
                if n > largest {
                    largest = n;
                }
            }
        }
    }
    ImageBudget {
        count,
        total_b64_bytes: total,
        largest_b64_bytes: largest,
    }
}

/// will KiroProvider errormappingas HTTP response
pub(super) fn map_provider_error(err: Error) -> Response {
    let err_str = err.to_string();

    // The context window is full (accumulated conversation history exceeds the model context window limit).
    if err_str.contains("CONTENT_LENGTH_EXCEEDS_THRESHOLD") {
        tracing::warn!(error = %err, "Upstream rejected the request: the context window is full (should not retry).");
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "invalid_request_error",
                "Context window is full. Reduce conversation history, system prompt, or tools.",
            )),
        )
            .into_response();
    }

    // A single input is too long (the request body itself exceeds the upstream limit).
    if err_str.contains("Input is too long") {
        tracing::warn!(error = %err, "Upstream rejected the request: input too long (should not retry).");
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "invalid_request_error",
                "Input is too long. Reduce the size of your messages.",
            )),
        )
            .into_response();
    }

    // Bedrock client-side validation errors (tool_use <-> tool_result mismatch, invalid message sequence, etc.)
    // The root cause is the client's own messages array, not an upstream failure, so it must not map to 5xx
    // otherwise it triggers an upstream cooldown that amplifies one client error into a 30+ burst of 503s.
    // Detection is centralized in the endpoint layer (single source of truth for the markers); the provider
    // already bails out without retry on these, and this mapping is the client-facing safety net.
    if crate::kiro::endpoint::default_is_client_validation_error(&err_str) {
        tracing::warn!(
            error = %err,
            "client messages array violates the protocol (Bedrock validation; mapped to 400 to avoid a false cooldown)"
        );
        // Return a stable, client-facing message and avoid echoing the raw upstream
        // error string (which can carry request IDs or internal validation details).
        // The full error is already logged above for diagnostics.
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "invalid_request_error",
                "Invalid message sequence: tool_use and tool_result blocks must be correctly paired and ordered.".to_string(),
            )),
        )
            .into_response();
    }

    tracing::error!("Kiro API callfailed: {}", err);
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse::new(
            "api_error",
            format!("upstream API callfailed: {}", err),
        )),
    )
        .into_response()
}

/// compute Anthropic usage basis input_tokens
fn resolve_usage_input_tokens(
    fallback_total_input_tokens: i32,
    context_total_input_tokens: Option<i32>,
) -> i32 {
    context_total_input_tokens.unwrap_or(fallback_total_input_tokens)
}

fn available_models() -> Vec<Model> {
    vec![
        Model {
            id: "claude-opus-4-8".to_string(),
            object: "model".to_string(),
            created: 1779897600, // May 28, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-8-thinking".to_string(),
            object: "model".to_string(),
            created: 1779897600, // May 28, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.8 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-8".to_string(),
            object: "model".to_string(),
            created: 1779897600, // May 28, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.8".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-8-thinking".to_string(),
            object: "model".to_string(),
            created: 1779897600, // May 28, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.8 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-7".to_string(),
            object: "model".to_string(),
            created: 1776276000, // Apr 16, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.7".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-7-thinking".to_string(),
            object: "model".to_string(),
            created: 1776276000, // Apr 16, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.7 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-6".to_string(),
            object: "model".to_string(),
            created: 1770163200, // Feb 4, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.6".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-6-thinking".to_string(),
            object: "model".to_string(),
            created: 1770163200, // Feb 4, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.6 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-6".to_string(),
            object: "model".to_string(),
            created: 1771286400, // Feb 17, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.6".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-6-thinking".to_string(),
            object: "model".to_string(),
            created: 1771286400, // Feb 17, 2026
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.6 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-5-20251101".to_string(),
            object: "model".to_string(),
            created: 1763942400, // Nov 24, 2025
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-5-20251101-thinking".to_string(),
            object: "model".to_string(),
            created: 1763942400, // Nov 24, 2025
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-5-20250929".to_string(),
            object: "model".to_string(),
            created: 1759104000, // Sep 29, 2025
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-5-20250929-thinking".to_string(),
            object: "model".to_string(),
            created: 1759104000, // Sep 29, 2025
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-haiku-4-5-20251001".to_string(),
            object: "model".to_string(),
            created: 1760486400, // Oct 15, 2025
            owned_by: "anthropic".to_string(),
            display_name: "Claude Haiku 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-haiku-4-5-20251001-thinking".to_string(),
            object: "model".to_string(),
            created: 1760486400, // Oct 15, 2025
            owned_by: "anthropic".to_string(),
            display_name: "Claude Haiku 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
    ]
}

/// GET /v1/models
///
/// return the available model list
pub async fn get_models() -> impl IntoResponse {
    tracing::info!("Received GET /v1/models request");

    let models = available_models();

    Json(ModelsResponse {
        object: "list".to_string(),
        data: models,
    })
}

/// POST /v1/messages
///
/// create a message (conversation)
pub async fn post_messages(
    State(state): State<AppState>,
    Extension(key_ctx): Extension<KeyContext>,
    JsonExtractor(mut payload): JsonExtractor<MessagesRequest>,
) -> Response {
    // Count the image budget on inbound to provide precise diagnostics for later context-window-full errors
    let img_stats = count_image_budget(&payload);
    tracing::info!(
        model = %payload.model,
        max_tokens = %payload.max_tokens,
        stream = %payload.stream,
        message_count = %payload.messages.len(),
        image_count = %img_stats.count,
        image_total_b64_kb = %(img_stats.total_b64_bytes / 1024),
        image_largest_b64_kb = %(img_stats.largest_b64_bytes / 1024),
        "Received POST /v1/messages request"
    );
    if img_stats.total_b64_bytes > IMAGE_BUDGET_WARN_BYTES {
        tracing::warn!(
            image_count = %img_stats.count,
            image_total_b64_kb = %(img_stats.total_b64_bytes / 1024),
            "incoming image payload is large; if upstream rejects with CONTENT_LENGTH_EXCEEDS_THRESHOLD, reduce image count or use lower-resolution screenshots"
        );
    }
    let hook = UsageRecordHook::from_state(&state, key_ctx.key_id, payload.model.clone());
    // check KiroProvider iswhetheravailable
    let provider = match &state.kiro_provider {
        Some(p) => p.clone(),
        None => {
            tracing::error!("KiroProvider not configured");
            hook.record(0, 0, 0, 0, 0, 0.0, "error");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(
                    "service_unavailable",
                    "Kiro API provider not configured",
                )),
            )
                .into_response();
        }
    };

    // detect whether the model name contains "thinking" suffix, if included then override thinking config
    override_thinking_from_model_name(&mut payload);

    // checkwhether is WebSearch request
    if websearch::has_web_search_tool(&payload) {
        tracing::info!("detected WebSearch tool,routeto WebSearch handle");

        // estimateinput tokens
        let input_tokens = token::count_all_tokens(
            payload.model.clone(),
            payload.system.clone(),
            payload.messages.clone(),
            payload.tools.clone(),
        ) as i32;

        let resp = websearch::handle_websearch_request(provider, &payload, input_tokens).await;
        // WebSearch path goes MCP endpoint,none credential_id context, uniformly record 0
        let status = if resp.status().is_success() { "success" } else { "error" };
        hook.record(0, input_tokens, 0, 0, 0, 0.0, status);
        return resp;
    }

    let payload_stream = payload.stream;
    // Mixed-tools (web_search + exec...) case: web_search coexists with other tools and falls onto the normal chat path,
    // where the upstream may return a tool_use with name=web_search. Take the internal agentic loop: search internally and feed the results back.
    if websearch::has_web_search_among_tools(&payload) {
        tracing::info!("detected mixed tools containing web_search, entering the web_search agentic loop");
        return super::websearch_loop::run_web_search_loop(provider, payload, hook, payload_stream, key_ctx.group.clone())
            .await;
    }

    // convertrequest
    let conversion_result = match convert_request(&payload) {
        Ok(result) => result,
        Err(e) => {
            let (error_type, message) = match &e {
                ConversionError::UnsupportedModel(model) => {
                    ("invalid_request_error", format!("modelnot supported: {}", model))
                }
                ConversionError::EmptyMessages => {
                    ("invalid_request_error", "messagelistis empty".to_string())
                }
            };
            tracing::warn!("requestconvertfailed: {}", e);
            hook.record(0, 0, 0, 0, 0, 0.0, "error");
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(error_type, message)),
            )
                .into_response();
        }
    };

    // Build the Kiro request. profile_arn is injected by the provider layer from the actual
    // credentials; additional_model_request_fields is already filtered by converter model support.
    let kiro_request = KiroRequest {
        conversation_state: conversion_result.conversation_state,
        profile_arn: None,
        additional_model_request_fields: conversion_result.additional_model_request_fields,
    };

    let request_body = match serde_json::to_string(&kiro_request) {
        Ok(body) => body,
        Err(e) => {
            tracing::error!("failed to serialize the request: {}", e);
            hook.record(0, 0, 0, 0, 0, 0.0, "error");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "internal_error",
                    format!("failed to serialize the request: {}", e),
                )),
            )
                .into_response();
        }
    };

    tracing::debug!("Kiro request body: {}", request_body);

    // estimateinput tokens
    let total_input_tokens = token::count_all_tokens(
        payload.model.clone(),
        payload.system.clone(),
        payload.messages.clone(),
        payload.tools.clone(),
    ) as i32;

    // check whether it is enabledthinking
    let thinking_enabled = payload
        .thinking
        .as_ref()
        .map(|t| t.is_enabled())
        .unwrap_or(false);

    let tool_name_map = conversion_result.tool_name_map;
    let known_tool_names = conversion_result.known_tool_names;

    // CacheMeter: based on cache_control breakpoint check / write the relay layer prompt cache.
    // return estimate the coverage amount of the measure; real input/cache mutually exclusive apportionment when obtaining total truthywhenenterline.
    let cache_usage = state
        .cache_meter
        .as_ref()
        .map(|cache| super::cache_metering::compute_cache_usage(cache, &payload, key_ctx.key_id))
        .unwrap_or_default();

    if payload.stream {
        // streamingresponse
        let tracer = std::sync::Arc::new(RequestTracer::new(
            &state,
            RequestTraceOptions {
                key_ctx: key_ctx.clone(),
                model: payload.model.clone(),
                is_stream: true,
            },
        ));
        handle_stream_request(
            provider,
            &request_body,
            &payload.model,
            total_input_tokens,
            thinking_enabled,
            tool_name_map,
            known_tool_names,
            hook,
            cache_usage,
            tracer,
            key_ctx.group.clone(),
        )
        .await
    } else {
        // Non streaming response: extracts only when enabled in config. thinking block
        let extract_thinking = state.extract_thinking && thinking_enabled;
        let tracer = std::sync::Arc::new(RequestTracer::new(
            &state,
            RequestTraceOptions {
                key_ctx: key_ctx.clone(),
                model: payload.model.clone(),
                is_stream: false,
            },
        ));
        handle_non_stream_request(
            provider,
            &request_body,
            &payload.model,
            total_input_tokens,
            extract_thinking,
            tool_name_map,
            known_tool_names,
            hook,
            cache_usage,
            tracer,
            key_ctx.group.clone(),
        )
        .await
    }
}

/// handlestreaming request
async fn handle_stream_request(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    thinking_enabled: bool,
    tool_name_map: std::collections::HashMap<String, String>,
    known_tool_names: std::collections::HashSet<String>,
    hook: UsageRecordHook,
    cache_usage: super::cache_metering::CacheUsage,
    tracer: std::sync::Arc<RequestTracer>,
    group: Option<String>,
) -> Response {
    // call Kiro API(supports multi credential failover)
    let call_result = match provider.call_api_stream(request_body, Some(tracer.as_ref()), group.as_deref()).await {
        Ok(resp) => resp,
        Err(e) => {
            hook.record(0, input_tokens, 0, 0, 0, 0.0, "error");
            // The whole retry chain failed and no content began to return:error_type take the last hop classification
            tracer.finalize("error", last_attempt_outcome(&tracer), Some(&e.to_string()), None, TraceUsage::zero());
            return map_provider_error(e);
        }
    };
    let response = call_result.response;
    let credential_id = call_result.credential_id;

    // create a stream processing context
    let mut ctx = StreamContext::new_with_thinking(model, input_tokens, thinking_enabled, tool_name_map, known_tool_names);
    ctx.cache_usage = cache_usage;

    // generateinitialevent
    let initial_events = ctx.generate_initial_events();

    // create SSE stream
    let stream = create_sse_stream(response, ctx, initial_events, hook, credential_id, tracer);

    // return SSE response
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// Ping eventinterval(25seconds)
const PING_INTERVAL_SECS: u64 = 25;

/// create ping event SSE string
fn create_ping_sse() -> Bytes {
    Bytes::from("event: ping\ndata: {\"type\": \"ping\"}\n\n")
}

/// create SSE event stream
fn create_sse_stream(
    response: reqwest::Response,
    ctx: StreamContext,
    initial_events: Vec<SseEvent>,
    hook: UsageRecordHook,
    credential_id: u64,
    tracer: std::sync::Arc<RequestTracer>,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    // send the initial event first
    let initial_stream = stream::iter(
        initial_events
            .into_iter()
            .map(|e| Ok(Bytes::from(e.to_sse_string()))),
    );

    // soafterhandle Kiro the response stream, at the same time each25send per second ping keep alive
    let body_stream = response.bytes_stream();

    let processing_stream = stream::unfold(
        (body_stream, ctx, EventStreamDecoder::new(), false, interval(Duration::from_secs(PING_INTERVAL_SECS)), hook, credential_id, tracer, 0u64),
        |(mut body_stream, mut ctx, mut decoder, finished, mut ping_interval, hook, credential_id, tracer, mut sent_bytes)| async move {
            if finished {
                return None;
            }

            // use select! wait for both data and ping timer
            tokio::select! {
                // handledatastream
                chunk_result = body_stream.next() => {
                    match chunk_result {
                        Some(Ok(chunk)) => {
                            tracer.mark_first_token();
                            sent_bytes += chunk.len() as u64;
                            // decodeevent
                            if let Err(e) = decoder.feed(&chunk) {
                                tracing::warn!("buffer overflow: {}", e);
                            }

                            let mut events = Vec::new();
                            for result in decoder.decode_iter() {
                                match result {
                                    Ok(frame) => {
                                        if let Ok(event) = Event::from_frame(frame) {
                                            let sse_events = ctx.process_kiro_event(&event);
                                            events.extend(sse_events);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("decodeeventfailed: {}", e);
                                    }
                                }
                            }

                            // convert to SSE byte stream
                            let bytes: Vec<Result<Bytes, Infallible>> = events
                                .into_iter()
                                .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                .collect();

                            Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval, hook, credential_id, tracer, sent_bytes)))
                        }
                        Some(Err(e)) => {
                            tracing::error!("failed to read the response stream: {}", e);
                            // Sends the final event and ends (recorded as error)
                            let final_events = ctx.generate_final_events();
                            record_stream_usage(&hook, &ctx, credential_id, "error");
                            // Upstream stream broke after content began returning: marked as interrupted, with the number of bytes already sent
                            tracer.finalize(
                                "interrupted",
                                Some(outcome::STREAM_INTERRUPTED),
                                Some(&e.to_string()),
                                Some(sent_bytes),
                                stream_trace_usage(&ctx),
                            );
                            let bytes: Vec<Result<Bytes, Infallible>> = final_events
                                .into_iter()
                                .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                .collect();
                            Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, hook, credential_id, tracer, sent_bytes)))
                        }
                        None => {
                            // the stream ends, send the final event
                            let final_events = ctx.generate_final_events();
                            record_stream_usage(&hook, &ctx, credential_id, "success");
                            tracer.finalize("success", None, None, None, stream_trace_usage(&ctx));
                            let bytes: Vec<Result<Bytes, Infallible>> = final_events
                                .into_iter()
                                .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                .collect();
                            Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, hook, credential_id, tracer, sent_bytes)))
                        }
                    }
                }
                // send ping keep alive
                _ = ping_interval.tick() => {
                    tracing::trace!("send ping keep aliveevent");
                    let bytes: Vec<Result<Bytes, Infallible>> = vec![Ok(create_ping_sse())];
                    Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval, hook, credential_id, tracer, sent_bytes)))
                }
            }
        },
    )
    .flatten();

    initial_stream.chain(processing_stream)
}

/// from StreamContext extract the final usage and write into hook
fn record_stream_usage(
    hook: &UsageRecordHook,
    ctx: &StreamContext,
    credential_id: u64,
    status: &str,
) {
    // mutually exclusive allocationafterof (input, cache_creation, cache_read), with trace the reporting measure is consistent.
    let (input, cache_creation, cache_read) = ctx.resolved_usage();
    hook.record(
        credential_id,
        input,
        ctx.output_tokens,
        cache_creation,
        cache_read,
        ctx.credits,
        status,
    );
}

/// from StreamContext extract the usage, convert into trace lineusage(with record_stream_usage same origin)
fn stream_trace_usage(ctx: &StreamContext) -> TraceUsage {
    let (input, cache_creation, cache_read) = ctx.resolved_usage();
    TraceUsage {
        input_tokens: input.max(0) as u64,
        output_tokens: ctx.output_tokens.max(0) as u64,
        cache_creation_tokens: cache_creation.max(0) as u64,
        cache_read_tokens: cache_read.max(0) as u64,
        credits: if ctx.credits.is_finite() && ctx.credits > 0.0 { ctx.credits } else { 0.0 },
    }
}

use super::converter::get_context_window_size;

/// handle non streaming requests
async fn handle_non_stream_request(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    thinking_enabled: bool,
    tool_name_map: std::collections::HashMap<String, String>,
    // The non streaming path handles structured directly. Event::ToolUse,notafter <invoke> text sniffing,
    // So no tool table validation is needed here; the parameter is kept to align with the caller signature.
    _known_tool_names: std::collections::HashSet<String>,
    hook: UsageRecordHook,
    cache_usage: super::cache_metering::CacheUsage,
    tracer: std::sync::Arc<RequestTracer>,
    group: Option<String>,
) -> Response {
    // call Kiro API(supports multi credential failover)
    let call_result = match provider.call_api(request_body, Some(tracer.as_ref()), group.as_deref()).await {
        Ok(resp) => resp,
        Err(e) => {
            hook.record(0, input_tokens, 0, 0, 0, 0.0, "error");
            tracer.finalize("error", last_attempt_outcome(&tracer), Some(&e.to_string()), None, TraceUsage::zero());
            return map_provider_error(e);
        }
    };
    let response = call_result.response;
    let credential_id = call_result.credential_id;

    // readresponse body
    let body_bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("failed to read the response body: {}", e);
            hook.record(credential_id, input_tokens, 0, 0, 0, 0.0, "error");
            tracer.finalize(
                "interrupted",
                Some(outcome::STREAM_INTERRUPTED),
                Some(&e.to_string()),
                None,
                TraceUsage::zero(),
            );
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::new(
                    "api_error",
                    format!("readresponsefailed: {}", e),
                )),
            )
                .into_response();
        }
    };

    // parse eventstream
    let mut decoder = EventStreamDecoder::new();
    if let Err(e) = decoder.feed(&body_bytes) {
        tracing::warn!("buffer overflow: {}", e);
    }

    let mut text_content = String::new();
    let mut native_thinking = String::new();
    let mut native_thinking_signature: Option<String> = None;
    let mut native_redacted_thinking: Vec<String> = Vec::new();
    let mut tool_uses: Vec<serde_json::Value> = Vec::new();
    let mut has_tool_use = false;
    let mut stop_reason = "end_turn".to_string();
    // from contextUsageEvent the computed actual input tokens
    let mut context_input_tokens: Option<i32> = None;
    // meteringEvent reported credit Billing amount (truly delivered by upstream);
    // input/cache_* the mutually exclusive apportionment when obtaining total truthyafterby cache_usage done.
    let mut credits: f64 = 0.0;

    // collect the increments of the tool call JSON
    let mut tool_json_buffers: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for result in decoder.decode_iter() {
        match result {
            Ok(frame) => {
                if let Ok(event) = Event::from_frame(frame) {
                    match event {
                        Event::AssistantResponse(resp) => {
                            text_content.push_str(&resp.content);
                        }
                        Event::ReasoningContent(reasoning) => {
                            if let Some(text) = reasoning.text
                                && !text.is_empty()
                            {
                                native_thinking.push_str(&text);
                            }
                            if let Some(signature) = reasoning.signature
                                && !signature.is_empty()
                            {
                                native_thinking_signature = Some(signature);
                            }
                            if let Some(redacted) = reasoning.redacted_content
                                && !redacted.is_empty()
                            {
                                native_redacted_thinking.push(redacted);
                            }
                        }
                        Event::ToolUse(tool_use) => {
                            has_tool_use = true;

                            // accumulatetool JSON input
                            let buffer = tool_json_buffers
                                .entry(tool_use.tool_use_id.clone())
                                .or_insert_with(String::new);
                            buffer.push_str(&tool_use.input);

                            // If it is a complete tool call, adds it to the list.
                            if tool_use.stop {
                                let input: serde_json::Value = if buffer.is_empty() {
                                    serde_json::json!({})
                                } else {
                                    serde_json::from_str(buffer).unwrap_or_else(|e| {
                                        tracing::warn!(
                                            "toolinput JSON parsefailed: {}, tool_use_id: {}",
                                            e,
                                            tool_use.tool_use_id
                                        );
                                        serde_json::json!({})
                                    })
                                };

                                let original_name = tool_name_map
                                    .get(&tool_use.name)
                                    .cloned()
                                    .unwrap_or_else(|| tool_use.name.clone());

                                tool_uses.push(json!({
                                    "type": "tool_use",
                                    "id": tool_use.tool_use_id,
                                    "name": original_name,
                                    "input": input
                                }));
                            }
                        }
                        Event::ContextUsage(context_usage) => {
                            // Computes the actual one from the context usage percentage. input_tokens
                            let window_size = get_context_window_size(model);
                            let actual_input_tokens =
                                (context_usage.context_usage_percentage * (window_size as f64)
                                    / 100.0) as i32;
                            context_input_tokens = Some(actual_input_tokens);
                            // the context usage reaches 100% when,set stop_reason as model_context_window_exceeded
                            if context_usage.context_usage_percentage >= 100.0 {
                                stop_reason = "model_context_window_exceeded".to_string();
                            }
                            tracing::debug!(
                                "received contextUsageEvent: {}%, compute input_tokens: {}",
                                context_usage.context_usage_percentage,
                                actual_input_tokens
                            );
                        }
                        Event::Metering(metering) => {
                            // upstreamonly dispatch credit;token / cache fielddoes not exist
                            credits += metering.usage;
                            tracing::debug!("metering credits +{:.6}", metering.usage);
                        }
                        Event::Exception { exception_type, .. } => {
                            if exception_type == "ContentLengthExceededException" {
                                stop_reason = "max_tokens".to_string();
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                tracing::warn!("decodeeventfailed: {}", e);
            }
        }
    }

    // determine stop_reason
    if has_tool_use && stop_reason == "end_turn" {
        stop_reason = "tool_use".to_string();
    }

    // buildresponse content
    let mut content = build_non_stream_content(
        thinking_enabled,
        text_content,
        native_thinking,
        native_thinking_signature,
        native_redacted_thinking,
    );
    content.extend(tool_uses);

    // estimateoutput tokens(upstreamdo not dispatch token, all go through estimation)
    let output_tokens = token::estimate_output_tokens(&content);

    // input tokens:contextUsage The real value takes precedence; otherwise uses the client estimate.
    let total_input_tokens = resolve_usage_input_tokens(input_tokens, context_input_tokens);
    // mutually exclusive allocation:input + cache_creation + cache_read == total
    let (final_input_tokens, cache_creation_tokens, cache_read_tokens) =
        cache_usage.split_against_total(total_input_tokens);

    // build Anthropic response
    let response_body = json!({
        "id": format!("msg_{}", Uuid::new_v4().to_string().replace('-', "")),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": model,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": final_input_tokens,
            "output_tokens": output_tokens,
            "cache_creation_input_tokens": cache_creation_tokens,
            "cache_read_input_tokens": cache_read_tokens
        }
    });

    hook.record(
        credential_id,
        final_input_tokens,
        output_tokens,
        cache_creation_tokens,
        cache_read_tokens,
        credits,
        "success",
    );
    tracer.finalize(
        "success",
        None,
        None,
        None,
        TraceUsage {
            input_tokens: final_input_tokens.max(0) as u64,
            output_tokens: output_tokens.max(0) as u64,
            cache_creation_tokens: cache_creation_tokens.max(0) as u64,
            cache_read_tokens: cache_read_tokens.max(0) as u64,
            credits: if credits.is_finite() && credits > 0.0 { credits } else { 0.0 },
        },
    );
    (StatusCode::OK, Json(response_body)).into_response()
}

fn build_non_stream_content(
    thinking_enabled: bool,
    text_content: String,
    native_thinking: String,
    native_thinking_signature: Option<String>,
    native_redacted_thinking: Vec<String>,
) -> Vec<serde_json::Value> {
    let mut content = Vec::new();
    let has_native_thinking = !native_thinking.is_empty();

    if thinking_enabled {
        if has_native_thinking {
            content.push(json!({
                "type": "thinking",
                "thinking": native_thinking.clone(),
                "signature": native_thinking_signature
                    .unwrap_or_else(|| super::stream::THINKING_SIGNATURE_PLACEHOLDER.to_string()),
            }));
        } else {
            // extract from the complete text thinking block,compatibleold <thinking> textpath.
            let (thinking, remaining_text) =
                super::stream::extract_thinking_from_complete_text(&text_content);

            if let Some(thinking_text) = thinking {
                content.push(json!({
                    "type": "thinking",
                    "thinking": thinking_text,
                    "signature": super::stream::THINKING_SIGNATURE_PLACEHOLDER,
                }));
            }

            if !remaining_text.is_empty() {
                content.push(json!({
                    "type": "text",
                    "text": remaining_text
                }));
            }
        }

        for redacted in native_redacted_thinking {
            content.push(json!({
                "type": "redacted_thinking",
                "data": redacted
            }));
        }

        if has_native_thinking && !text_content.is_empty() {
            content.push(json!({
                "type": "text",
                "text": text_content
            }));
        }
    } else if !text_content.is_empty() {
        content.push(json!({
            "type": "text",
            "text": text_content
        }));
    } else if has_native_thinking {
        content.push(json!({
            "type": "text",
            "text": native_thinking
        }));
    }
    content
}

/// detect whether the model name contains "thinking" suffix, if included then override thinking config
///
/// - Opus 4.6:overwriteas adaptive type
/// - other models: override to enabled type
/// - budget_tokens fixed to 20000
fn override_thinking_from_model_name(payload: &mut MessagesRequest) {
    let model_lower = payload.model.to_lowercase();
    if !model_lower.contains("thinking") {
        return;
    }

    let is_opus_4_6 = model_lower.contains("opus")
        && (model_lower.contains("4-6") || model_lower.contains("4.6"));

    let thinking_type = if is_opus_4_6 { "adaptive" } else { "enabled" };

    tracing::info!(
        model = %payload.model,
        thinking_type = thinking_type,
        "model namecontains thinking afteraffix,overwrite thinking config"
    );

    payload.thinking = Some(Thinking {
        thinking_type: thinking_type.to_string(),
        budget_tokens: 20000,
    });

    if is_opus_4_6 {
        payload.output_config = Some(OutputConfig {
            effort: "high".to_string(),
        });
    }
}

/// POST /v1/messages/count_tokens
///
/// computemessage token count
pub async fn count_tokens(
    Extension(_key_ctx): Extension<KeyContext>,
    JsonExtractor(payload): JsonExtractor<CountTokensRequest>,
) -> impl IntoResponse {
    tracing::info!(
        model = %payload.model,
        message_count = %payload.messages.len(),
        "Received POST /v1/messages/count_tokens request"
    );

    let total_tokens = token::count_all_tokens(
        payload.model,
        payload.system,
        payload.messages,
        payload.tools,
    ) as i32;

    Json(CountTokensResponse {
        input_tokens: total_tokens.max(1) as i32,
    })
}

/// POST /cc/v1/messages
///
/// Claude Code compatibleendpoint, with /v1/messages ofdifferenceinat:
/// - the streaming response will wait kiro end returns contextUsageEvent afterthen send message_start
/// - message_start in input_tokens is from contextUsageEvent computeofaccuratevalue
pub async fn post_messages_cc(
    State(state): State<AppState>,
    Extension(key_ctx): Extension<KeyContext>,
    JsonExtractor(mut payload): JsonExtractor<MessagesRequest>,
) -> Response {
    tracing::info!(
        model = %payload.model,
        max_tokens = %payload.max_tokens,
        stream = %payload.stream,
        message_count = %payload.messages.len(),
        "Received POST /cc/v1/messages request"
    );
    let hook = UsageRecordHook::from_state(&state, key_ctx.key_id, payload.model.clone());

    // check KiroProvider iswhetheravailable
    let provider = match &state.kiro_provider {
        Some(p) => p.clone(),
        None => {
            tracing::error!("KiroProvider not configured");
            hook.record(0, 0, 0, 0, 0, 0.0, "error");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(
                    "service_unavailable",
                    "Kiro API provider not configured",
                )),
            )
                .into_response();
        }
    };

    // detect whether the model name contains "thinking" suffix, if included then override thinking config
    override_thinking_from_model_name(&mut payload);

    // checkwhether is WebSearch request
    if websearch::has_web_search_tool(&payload) {
        tracing::info!("detected WebSearch tool,routeto WebSearch handle");

        // estimateinput tokens
        let input_tokens = token::count_all_tokens(
            payload.model.clone(),
            payload.system.clone(),
            payload.messages.clone(),
            payload.tools.clone(),
        ) as i32;

        let resp = websearch::handle_websearch_request(provider, &payload, input_tokens).await;
        let status = if resp.status().is_success() { "success" } else { "error" };
        hook.record(0, input_tokens, 0, 0, 0, 0.0, status);
        return resp;
    }

    let payload_stream = payload.stream;
    // Mixed-tools (web_search + exec...) case: web_search coexists with other tools and falls onto the normal chat path,
    // where the upstream may return a tool_use with name=web_search. Take the internal agentic loop: search internally and feed the results back.
    if websearch::has_web_search_among_tools(&payload) {
        tracing::info!("detected mixed tools containing web_search, entering the web_search agentic loop");
        return super::websearch_loop::run_web_search_loop(provider, payload, hook, payload_stream, key_ctx.group.clone())
            .await;
    }

    // convertrequest
    let conversion_result = match convert_request(&payload) {
        Ok(result) => result,
        Err(e) => {
            let (error_type, message) = match &e {
                ConversionError::UnsupportedModel(model) => {
                    ("invalid_request_error", format!("modelnot supported: {}", model))
                }
                ConversionError::EmptyMessages => {
                    ("invalid_request_error", "messagelistis empty".to_string())
                }
            };
            tracing::warn!("requestconvertfailed: {}", e);
            hook.record(0, 0, 0, 0, 0, 0.0, "error");
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(error_type, message)),
            )
                .into_response();
        }
    };

    // Build the Kiro request. profile_arn is injected by the provider layer from the actual
    // credentials; additional_model_request_fields is already filtered by converter model support.
    let kiro_request = KiroRequest {
        conversation_state: conversion_result.conversation_state,
        profile_arn: None,
        additional_model_request_fields: conversion_result.additional_model_request_fields,
    };

    let request_body = match serde_json::to_string(&kiro_request) {
        Ok(body) => body,
        Err(e) => {
            tracing::error!("failed to serialize the request: {}", e);
            hook.record(0, 0, 0, 0, 0, 0.0, "error");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "internal_error",
                    format!("failed to serialize the request: {}", e),
                )),
            )
                .into_response();
        }
    };

    tracing::debug!("Kiro request body: {}", request_body);

    // compute total input tokens
    let total_input_tokens = token::count_all_tokens(
        payload.model.clone(),
        payload.system.clone(),
        payload.messages.clone(),
        payload.tools.clone(),
    ) as i32;

    // check whether it is enabledthinking
    let thinking_enabled = payload
        .thinking
        .as_ref()
        .map(|t| t.is_enabled())
        .unwrap_or(false);

    let tool_name_map = conversion_result.tool_name_map;
    let known_tool_names = conversion_result.known_tool_names;

    // CacheMeter: based on cache_control breakpoint check / write the relay layer prompt cache (estimate basis).
    let cache_usage = state
        .cache_meter
        .as_ref()
        .map(|cache| super::cache_metering::compute_cache_usage(cache, &payload, key_ctx.key_id))
        .unwrap_or_default();

    if payload.stream {
        // streaming response (buffered mode)
        let tracer = std::sync::Arc::new(RequestTracer::new(
            &state,
            RequestTraceOptions {
                key_ctx: key_ctx.clone(),
                model: payload.model.clone(),
                is_stream: true,
            },
        ));
        handle_stream_request_buffered(
            provider,
            &request_body,
            &payload.model,
            thinking_enabled,
            tool_name_map,
            known_tool_names,
            hook,
            total_input_tokens,
            cache_usage,
            tracer,
            key_ctx.group.clone(),
        )
        .await
    } else {
        // Non streaming response: extracts only when enabled in config. thinking block
        let extract_thinking = state.extract_thinking && thinking_enabled;
        let tracer = std::sync::Arc::new(RequestTracer::new(
            &state,
            RequestTraceOptions {
                key_ctx: key_ctx.clone(),
                model: payload.model.clone(),
                is_stream: false,
            },
        ));
        handle_non_stream_request(
            provider,
            &request_body,
            &payload.model,
            total_input_tokens,
            extract_thinking,
            tool_name_map,
            known_tool_names,
            hook,
            cache_usage,
            tracer,
            key_ctx.group.clone(),
        )
        .await
    }
}

/// Handles the streaming request (buffered version).
///
/// and `handle_stream_request` Different, this function buffers all events until the stream ends,
/// soafterusefrom contextUsageEvent computeofcorrect input_tokens generate message_start event.
async fn handle_stream_request_buffered(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    thinking_enabled: bool,
    tool_name_map: std::collections::HashMap<String, String>,
    known_tool_names: std::collections::HashSet<String>,
    hook: UsageRecordHook,
    fallback_input_tokens: i32,
    cache_usage: super::cache_metering::CacheUsage,
    tracer: std::sync::Arc<RequestTracer>,
    group: Option<String>,
) -> Response {
    // call Kiro API(supports multi credential failover)
    let call_result = match provider.call_api_stream(request_body, Some(tracer.as_ref()), group.as_deref()).await {
        Ok(resp) => resp,
        Err(e) => {
            hook.record(0, fallback_input_tokens, 0, 0, 0, 0.0, "error");
            tracer.finalize("error", last_attempt_outcome(&tracer), Some(&e.to_string()), None, TraceUsage::zero());
            return map_provider_error(e);
        }
    };
    let response = call_result.response;
    let credential_id = call_result.credential_id;

    // create a buffered stream processing context
    let mut ctx = BufferedStreamContext::new(
        model,
        fallback_input_tokens,
        thinking_enabled,
        tool_name_map,
        known_tool_names,
    );
    ctx.set_cache_usage(cache_usage);

    // createbuffer SSE stream
    let stream = create_buffered_sse_stream(response, ctx, hook, credential_id, tracer);

    // return SSE response
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// createbuffer SSE event stream
///
/// workflow:
/// 1. Waits for the upstream stream to finish, sending only ping keep alivesignal
/// 2. use StreamContext the event handling logic processes all Kiro event, the result is cached
/// 3. after the stream ends, with the correct input_tokens correct message_start event
/// 4. send all events at once
fn create_buffered_sse_stream(
    response: reqwest::Response,
    ctx: BufferedStreamContext,
    hook: UsageRecordHook,
    credential_id: u64,
    tracer: std::sync::Arc<RequestTracer>,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    let body_stream = response.bytes_stream();

    stream::unfold(
        (
            body_stream,
            ctx,
            EventStreamDecoder::new(),
            false,
            interval(Duration::from_secs(PING_INTERVAL_SECS)),
            hook,
            credential_id,
            tracer,
            0u64,
        ),
        |(mut body_stream, mut ctx, mut decoder, finished, mut ping_interval, hook, credential_id, tracer, mut sent_bytes)| async move {
            if finished {
                return None;
            }

            loop {
                tokio::select! {
                    // use biased mode, check first ping timer
                    // avoid inupstream chunk when dense ping by"starve"
                    biased;

                    // prioritycheck ping Keep alive (the only data sent while waiting).
                    _ = ping_interval.tick() => {
                        tracing::trace!("send ping keep alive event (buffered mode)");
                        let bytes: Vec<Result<Bytes, Infallible>> = vec![Ok(create_ping_sse())];
                        return Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval, hook, credential_id, tracer, sent_bytes)));
                    }

                    // then process the data stream
                    chunk_result = body_stream.next() => {
                        match chunk_result {
                            Some(Ok(chunk)) => {
                                tracer.mark_first_token();
                                sent_bytes += chunk.len() as u64;
                                // decodeevent
                                if let Err(e) = decoder.feed(&chunk) {
                                    tracing::warn!("buffer overflow: {}", e);
                                }

                                for result in decoder.decode_iter() {
                                    match result {
                                        Ok(frame) => {
                                            if let Ok(event) = Event::from_frame(frame) {
                                                // buffer the event (reuse StreamContext ofhandlelogic)
                                                ctx.process_and_buffer(&event);
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!("decodeeventfailed: {}", e);
                                        }
                                    }
                                }
                                // continue reading the next chunk, do not send any data
                            }
                            Some(Err(e)) => {
                                tracing::error!("failed to read the response stream: {}", e);
                                // An error occurred; completes processing and returns all events.
                                let all_events = ctx.finish_and_get_all_events();
                                let (i, o, cc, cr, credits) = ctx.final_usage();
                                hook.record(credential_id, i, o, cc, cr, credits, "error");
                                // buffered mode chunk Read failed: the upstream stream broke midway.
                                tracer.finalize(
                                    "interrupted",
                                    Some(outcome::STREAM_INTERRUPTED),
                                    Some(&e.to_string()),
                                    Some(sent_bytes),
                                    TraceUsage {
                                        input_tokens: i.max(0) as u64,
                                        output_tokens: o.max(0) as u64,
                                        cache_creation_tokens: cc.max(0) as u64,
                                        cache_read_tokens: cr.max(0) as u64,
                                        credits: if credits.is_finite() && credits > 0.0 { credits } else { 0.0 },
                                    },
                                );
                                let bytes: Vec<Result<Bytes, Infallible>> = all_events
                                    .into_iter()
                                    .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                    .collect();
                                return Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, hook, credential_id, tracer, sent_bytes)));
                            }
                            None => {
                                // The stream ends; completes processing and returns all events (already corrected input_tokens)
                                let all_events = ctx.finish_and_get_all_events();
                                let (i, o, cc, cr, credits) = ctx.final_usage();
                                hook.record(credential_id, i, o, cc, cr, credits, "success");
                                tracer.finalize(
                                    "success",
                                    None,
                                    None,
                                    None,
                                    TraceUsage {
                                        input_tokens: i.max(0) as u64,
                                        output_tokens: o.max(0) as u64,
                                        cache_creation_tokens: cc.max(0) as u64,
                                        cache_read_tokens: cr.max(0) as u64,
                                        credits: if credits.is_finite() && credits > 0.0 { credits } else { 0.0 },
                                    },
                                );
                                let bytes: Vec<Result<Bytes, Infallible>> = all_events
                                    .into_iter()
                                    .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                    .collect();
                                return Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, hook, credential_id, tracer, sent_bytes)));
                            }
                        }
                    }
                }
            }
        },
    )
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bedrock_client_validation_errors_map_to_400() {
        // A client validation error must be mapped to 400(rather than 5xx), otherwisewill be provider treat asupstream
        // A transient error triggers cooldown, amplified into 503 storm. the recognition logic is centralized in endpoint layer.
        for needle in [
            // exact reason(provider the error string embeds the upstream body)
            "non streaming API requestfailed: 500 {\"reason\":\"TOOL_USE_RESULT_MISMATCH\"}",
            // message level specific phrase (plain text message).
            "Expected toolResult blocks but found none",
        ] {
            let resp = map_provider_error(anyhow::anyhow!(needle.to_string()));
            assert_eq!(
                resp.status(),
                StatusCode::BAD_REQUEST,
                "error string `{needle}` shouldmappingas 400"
            );
        }
    }

    #[test]
    fn generic_upstream_error_still_maps_to_502() {
        // Regression: an ordinary upstream error should not be harmed by the new branch and should still be 502 BAD_GATEWAY.
        let resp = map_provider_error(anyhow::anyhow!("connection reset by peer"));
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        // regression:broad ValidationException No longer treated as a client validation error and misjudged as 400,
        // still handle as an upstream error 502(avoids wrongly killing a retryable fault).
        let resp = map_provider_error(anyhow::anyhow!(
            "ValidationException: transient backend issue".to_string()
        ));
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn non_stream_native_thinking_precedes_redacted_and_text() {
        let content = build_non_stream_content(
            true,
            "final answer".to_string(),
            "native thinking".to_string(),
            Some("real-signature".to_string()),
            vec!["encrypted-thinking".to_string()],
        );

        assert_eq!(content.len(), 3);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "native thinking");
        assert_eq!(content[0]["signature"], "real-signature");
        assert_eq!(content[1]["type"], "redacted_thinking");
        assert_eq!(content[1]["data"], "encrypted-thinking");
        assert_eq!(content[2]["type"], "text");
        assert_eq!(content[2]["text"], "final answer");
    }

    #[test]
    fn non_stream_legacy_thinking_extraction_still_works_without_native_reasoning() {
        let content = build_non_stream_content(
            true,
            "<thinking>legacy thinking</thinking>\n\nfinal answer".to_string(),
            String::new(),
            None,
            Vec::new(),
        );

        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "legacy thinking");
        assert_eq!(
            content[0]["signature"],
            crate::anthropic::stream::THINKING_SIGNATURE_PLACEHOLDER
        );
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], "final answer");
    }

    #[test]
    fn non_stream_native_thinking_downgrades_to_text_when_thinking_disabled() {
        let content = build_non_stream_content(
            false,
            String::new(),
            "native thinking fallback".to_string(),
            Some("ignored-signature".to_string()),
            vec!["ignored-redacted".to_string()],
        );

        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "native thinking fallback");
    }

    #[test]
    fn available_models_include_opus_4_7_variants() {
        let models = available_models();
        let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();

        assert!(ids.contains(&"claude-opus-4-7"));
        assert!(ids.contains(&"claude-opus-4-7-thinking"));
    }

    #[test]
    fn count_image_budget_handles_empty() {
        let req: super::super::types::MessagesRequest = serde_json::from_str(r#"{
            "model": "claude-opus-4-7",
            "max_tokens": 100,
            "messages": []
        }"#).unwrap();
        let stats = count_image_budget(&req);
        assert_eq!(stats.count, 0);
        assert_eq!(stats.total_b64_bytes, 0);
        assert_eq!(stats.largest_b64_bytes, 0);
    }

    #[test]
    fn count_image_budget_counts_inline_base64() {
        let req: super::super::types::MessagesRequest = serde_json::from_str(r#"{
            "model": "claude-opus-4-7",
            "max_tokens": 100,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "hi"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA1111"}},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "BBBBBBBBBB"}},
                    {"type": "image", "source": {"type": "url", "url": "https://example.com/x.png"}}
                ]
            }]
        }"#).unwrap();
        let stats = count_image_budget(&req);
        assert_eq!(stats.count, 2);
        assert_eq!(stats.total_b64_bytes, 18);
        assert_eq!(stats.largest_b64_bytes, 10);
    }

    #[test]
    fn count_image_budget_skips_url_only_images() {
        let req: super::super::types::MessagesRequest = serde_json::from_str(r#"{
            "model": "claude-opus-4-7",
            "max_tokens": 100,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image", "source": {"type": "url", "url": "https://example.com/x.png"}}
                ]
            }]
        }"#).unwrap();
        let stats = count_image_budget(&req);
        assert_eq!(stats.count, 0);
    }

    #[test]
    fn available_models_include_4_8_variants() {
        let models = available_models();
        let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();

        assert!(ids.contains(&"claude-opus-4-8"));
        assert!(ids.contains(&"claude-opus-4-8-thinking"));
        assert!(ids.contains(&"claude-sonnet-4-8"));
        assert!(ids.contains(&"claude-sonnet-4-8-thinking"));
    }
}
