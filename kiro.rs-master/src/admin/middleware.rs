//! Admin API middleware

use std::sync::Arc;

use parking_lot::RwLock;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};

use super::client_keys::SharedClientKeyManager;
use super::groups::SharedGroupManager;
use super::service::AdminService;
use super::types::AdminErrorResponse;
use super::usage_stats::SharedAggregator;
use super::trace_db::SharedTraceStore;
use crate::common::auth;

/// Admin API sharestate
#[derive(Clone)]
pub struct AdminState {
    /// loginAPIKey (used for admin panel login, changeable at runtime).
    pub admin_api_key: Arc<RwLock<String>>,
    /// Admin service
    pub service: Arc<AdminService>,
    /// client Key manager(with anthropic routeshare)
    pub client_keys: SharedClientKeyManager,
    /// usage aggregator (with anthropic routeshare)
    pub usage_aggregator: SharedAggregator,
    /// Request trace storage (with anthropic routeshare)
    pub trace_store: SharedTraceStore,
    /// The account group registry (persisted to groups.json)
    pub groups: SharedGroupManager,
}

impl AdminState {
    pub fn new(
        admin_api_key: impl Into<String>,
        service: AdminService,
        client_keys: SharedClientKeyManager,
        usage_aggregator: SharedAggregator,
        trace_store: SharedTraceStore,
        groups: SharedGroupManager,
    ) -> Self {
        Self {
            admin_api_key: Arc::new(RwLock::new(admin_api_key.into())),
            service: Arc::new(service),
            client_keys,
            usage_aggregator,
            trace_store,
            groups,
        }
    }
}

/// Admin API authmiddleware — validateloginAPIkey (adminApiKey)
pub async fn admin_auth_middleware(
    State(state): State<AdminState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let api_key = auth::extract_api_key(&request);

    let current_key = state.admin_api_key.read().clone();
    match api_key {
        Some(key) if auth::constant_time_eq(&key, &current_key) => next.run(request).await,
        _ => {
            let error = AdminErrorResponse::authentication_error();
            (StatusCode::UNAUTHORIZED, Json(error)).into_response()
        }
    }
}
