//! Anthropic API middleware

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};

use crate::admin::client_keys::SharedClientKeyManager;
use crate::admin::trace_db::{SharedTraceStore, TraceKeySource};
use crate::admin::usage_stats::{SharedAggregator, SharedRecorder};
use crate::common::auth;
use crate::kiro::provider::KiroProvider;

use super::cache_metering::SharedCacheMeter;
use super::types::ErrorResponse;

/// The hit auth context (injected into the request extensions, for handler recordusage)
#[derive(Clone, Debug)]
pub struct KeyContext {
    /// hitofclient Key id
    pub key_id: u64,
    /// this Key the bound account group;None means not bound, can use all accounts.
    pub group: Option<String>,
    /// hitofentry Key type.
    pub key_source: TraceKeySource,
}

/// shouldusesharestate
#[derive(Clone)]
pub struct AppState {
    /// Kiro Provider(optional, used for actual API call)
    /// insidepartuse MultiTokenManager, now supports thread safe multi credential management.
    pub kiro_provider: Option<Arc<KiroProvider>>,
    /// whether to enable the non streaming response thinking block extract
    pub extract_thinking: bool,
    /// client Key manager (optional, not enabled Admin is when None)
    pub client_keys: Option<SharedClientKeyManager>,
    /// usage log recorder
    pub usage_recorder: Option<SharedRecorder>,
    /// usageaggregator
    pub usage_aggregator: Option<SharedAggregator>,
    /// relay layer cache metering (based on cache_control the in memory cache of the breakpoint)
    pub cache_meter: Option<SharedCacheMeter>,
    /// request trace storage (SQLite,optional)
    pub trace_store: Option<SharedTraceStore>,
}

impl AppState {
    /// Creates a new application state (without client_keys the base construction, for embedding / testuse)
    #[allow(dead_code)]
    pub fn new(extract_thinking: bool) -> Self {
        Self {
            kiro_provider: None,
            extract_thinking,
            client_keys: None,
            usage_recorder: None,
            usage_aggregator: None,
            cache_meter: None,
            trace_store: None,
        }
    }

    /// set KiroProvider
    pub fn with_kiro_provider(mut self, provider: KiroProvider) -> Self {
        self.kiro_provider = Some(Arc::new(provider));
        self
    }

    /// inject the usage recording component
    pub fn with_usage(
        mut self,
        client_keys: Option<SharedClientKeyManager>,
        recorder: Option<SharedRecorder>,
        aggregator: Option<SharedAggregator>,
    ) -> Self {
        self.client_keys = client_keys;
        self.usage_recorder = recorder;
        self.usage_aggregator = aggregator;
        self
    }

    /// inject the cache meter
    pub fn with_cache_meter(mut self, cache: Option<SharedCacheMeter>) -> Self {
        self.cache_meter = cache;
        self
    }

    /// inject the trace storage
    pub fn with_trace_store(mut self, store: Option<SharedTraceStore>) -> Self {
        self.trace_store = store;
        self
    }
}

/// API Key authmiddleware
///
/// authentication order:master apiKey → client Key(`csk_*`). On a hit, injects into the request extensions.
/// [`KeyContext`], for handler used when recording usage.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let presented = match auth::extract_api_key(&request) {
        Some(k) => k,
        None => {
            let error = ErrorResponse::authentication_error();
            return (StatusCode::UNAUTHORIZED, Json(error)).into_response();
        }
    };

    // all Key unifygoclient Key managervalidate
    if let Some(mgr) = &state.client_keys {
        if let Some(id) = mgr.verify_and_touch(&presented) {
            let group = mgr.group_of(id);
            request.extensions_mut().insert(KeyContext {
                key_id: id,
                group,
                key_source: TraceKeySource::ClientKey,
            });
            return next.run(request).await;
        }
    }

    let error = ErrorResponse::authentication_error();
    (StatusCode::UNAUTHORIZED, Json(error)).into_response()
}

/// CORS middlewarelayer
///
/// **safenote**: the current config allows all origins (Any), this is to support public API service.
/// If stricter security control is needed, configure the specific allowed origins, methods, and headers according to actual requirements.
///
/// # confignote
/// - `allow_origin(Any)`: allow requests from any origin
/// - `allow_methods(Any)`: allowany HTTP method
/// - `allow_headers(Any)`: allow any request header
pub fn cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};

    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
