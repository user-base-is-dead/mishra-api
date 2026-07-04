//! Admin API HTTP handler

use std::collections::HashMap;

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use bytes::Bytes;
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use futures::StreamExt;
use std::sync::Arc;

use super::{
    client_keys::mask_client_key,
    middleware::AdminState,
    trace_db::TraceQuery,
    types::{
        AddCredentialRequest, AddProxyRequest, AssignProxyRequest, AssignRoundRobinRequest,
        BatchAddProxyRequest, BatchImportEvent, BatchImportRequest, BatchImportSummary,
        ClientKeyItem, ClientKeysResponse, CompleteSocialLoginRequest,
        CreateClientKeyRequest, CreateClientKeyResponse, GlobalProxyResponse,
        SetAccountThrottleConfigRequest, SetDisabledRequest, SetGlobalProxyRequest,
        SetLoadBalancingModeRequest, SetLogGovernanceConfigRequest, SetPriorityRequest,
        SetUpdateConfigRequest, StartIdcLoginRequest, StartSocialLoginRequest, SuccessResponse,
        UpdateAdminKeyRequest, UpdateClientKeyRequest, UpdateCredentialRequest,
        UpdateRefreshTokenRequest,
    },
    usage_stats::{Range, StatsGranularity, StatsQueryWindow},
};

// Path tupleextract:(credential_id, session_id)
type CredSessionPath = (u64, String);

/// GET /api/admin/credentials
/// get all credential statuses
pub async fn get_all_credentials(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_all_credentials();
    Json(response)
}

/// GET /api/admin/credentials/export
/// export the credential as compatible JSON(including refreshToken and other sensitive fields)
///
/// optional query parameter `ids`(comma separated) limits which credentials to export; omit to export all.
pub async fn export_credentials(
    State(state): State<AdminState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let id_filter: Option<std::collections::HashSet<u64>> = params
        .get("ids")
        .map(|raw| {
            raw.split(',')
                .filter_map(|s| {
                    let t = s.trim();
                    if t.is_empty() {
                        None
                    } else {
                        t.parse::<u64>().ok()
                    }
                })
                .collect::<std::collections::HashSet<u64>>()
        })
        .filter(|s| !s.is_empty());

    let response = state.service.export_credentials(id_filter.as_ref());
    Json(response)
}

/// POST /api/admin/credentials/:id/disabled
/// set the credential disabled state
pub async fn set_credential_disabled(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetDisabledRequest>,
) -> impl IntoResponse {
    match state.service.set_disabled(id, payload.disabled) {
        Ok(_) => {
            let action = if payload.disabled { "disable" } else { "enable" };
            Json(SuccessResponse::new(format!("credential #{} already{}", id, action))).into_response()
        }
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/priority
/// set the credential priority
pub async fn set_credential_priority(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetPriorityRequest>,
) -> impl IntoResponse {
    match state.service.set_priority(id, payload.priority) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "credential #{} the priority has been set to {}",
            id, payload.priority
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/reset
/// Resets the failure count and re-enables.
pub async fn reset_failure_count(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.reset_and_enable(id) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "credential #{} The failure count has been reset and re-enabled.",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/clear-throttle
/// Manually clears the account level throttle cooldown of a credential.
pub async fn clear_throttle(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.clear_throttle(id) {
        Ok(_) => Json(SuccessResponse::new(format!("credential #{} the throttle cooldown has been lifted", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/credentials/:id/balance
/// get the balance of the specified credential
pub async fn get_credential_balance(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.get_balance(id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/credentials/:id/models
/// Gets the currently available model list for the given credential (queries upstream in real time on demand).
pub async fn get_credential_models(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.get_available_models(id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/disable-quota-exceeded
/// one click disable all"over quota"credential (remaining ≤ 0 or usage_percentage ≥ 100)
pub async fn disable_quota_exceeded(State(state): State<AdminState>) -> impl IntoResponse {
    let result = state.service.disable_quota_exceeded();
    Json(result).into_response()
}

/// POST /api/admin/credentials/:id/overage
/// Enables or disables the overage capability of the given credential.
pub async fn set_credential_overage(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<super::types::SetOverageRequest>,
) -> impl IntoResponse {
    match state.service.set_overage(id, payload.enabled).await {
        Ok(_) => Json(SuccessResponse::new(format!(
            "credential #{} already{}overage",
            id,
            if payload.enabled { "open" } else { "close" }
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/overage/enable-all
/// one click enable all"Overage can be enabled and is currently not enabled."credential overage (based on balance_cache decision)
pub async fn enable_overage_all(State(state): State<AdminState>) -> impl IntoResponse {
    let result = state.service.enable_overage_for_all_capable().await;
    Json(result).into_response()
}

/// POST /api/admin/credentials
/// addnew credential
pub async fn add_credential(
    State(state): State<AdminState>,
    Json(payload): Json<AddCredentialRequest>,
) -> impl IntoResponse {
    match state.service.add_credential(payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/batch-import
///
/// Batch imports credentials. The server by `concurrency`(default 8,clamptaketo [1,16]) processes one by one with bounded concurrency,
/// resultvia SSE the stream pushes one by one (`index` corresponds to the request array index, out of order); after one final summary event the stream closes.
///
/// `verify = true`(default):add then fetches the balance for liveness, rolls back on failure;`verify = false`: only add persist to db.
/// client disconnected (frontend abort / closes the connection), the event write back fails. → Immediately stops processing the remaining credentials.
/// (at most those already in processing concurrency entries end naturally), thereby supporting"stop the import".
pub async fn batch_import_credentials(
    State(state): State<AdminState>,
    Json(req): Json<BatchImportRequest>,
) -> Response {
    let concurrency = req.concurrency.unwrap_or(8).clamp(1, 16) as usize;
    let total = req.credentials.len();
    let verify = req.verify;

    let (tx, rx) = futures::channel::mpsc::unbounded::<BatchImportEvent>();
    let service = state.service.clone();

    // single orchestrator task:buffer_unordered Provides bounded concurrency and writes results back one by one. SSE stream.
    tokio::spawn(async move {
        let mut work = futures::stream::iter(req.credentials.into_iter().enumerate())
            .map(|(index, cred_req)| {
                let service = Arc::clone(&service);
                async move {
                    let result = service.import_one_credential(cred_req, verify).await;
                    (index, result)
                }
            })
            .buffer_unordered(concurrency);

        let mut imported = 0_usize;
        let mut verified = 0_usize;
        let mut duplicate = 0_usize;
        let mut failed = 0_usize;
        let mut rolled_back = 0_usize;
        let mut cancelled = false;

        while let Some((index, result)) = work.next().await {
            let event = result.into_event(index);
            match event.status.as_str() {
                "imported" => imported += 1,
                "verified" => verified += 1,
                "duplicate" => duplicate += 1,
                "failed" => {
                    failed += 1;
                    if event.rolled_back == Some(true) {
                        rolled_back += 1;
                    }
                }
                _ => {}
            }
            // client disconnected (abort / closeconnection)→ the receiver along with the response body is drop,send failed:
            // stop processing the remaining credentials.break will discard buffer_unordered inside in-flight of future.
            if tx.unbounded_send(event).is_err() {
                let processed = imported + verified + duplicate + failed;
                tracing::info!(
                    "Batch import was aborted by the client; stops the remaining credentials (already completed {}/{})",
                    processed,
                    total
                );
                cancelled = true;
                break;
            }
        }

        // Sends the summary only on normal completion; does not send if the client aborts (the stream was closed by the peer).
        if !cancelled {
            let summary = BatchImportEvent {
                index: None,
                status: "summary".to_string(),
                credential_id: None,
                email: None,
                usage: None,
                subscription: None,
                error: None,
                rolled_back: None,
                summary: Some(BatchImportSummary {
                    total,
                    imported,
                    verified,
                    duplicate,
                    failed,
                    rolled_back,
                }),
            };
            let _ = tx.unbounded_send(summary);
        }
        // tx here drop,SSE streamaccordinglyclose
    });

    let body = rx.map(|event| {
        let json = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
        Ok::<_, std::io::Error>(Bytes::from(format!("data: {}\n\n", json)))
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(body))
        .unwrap()
}

/// DELETE /api/admin/credentials/:id
/// deletecredential
pub async fn delete_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.delete_credential(id) {
        Ok(_) => Json(SuccessResponse::new(format!("credential #{} deleted", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// PUT /api/admin/credentials/:id
/// Updates the editable fields of the credential (email,proxy etc.)
pub async fn update_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateCredentialRequest>,
) -> impl IntoResponse {
    match state.service.update_credential(id, payload) {
        Ok(_) => Json(SuccessResponse::new(format!("credential #{} updated", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// PUT /api/admin/credentials/:id/refresh-token
/// update the disabled credential refreshToken
pub async fn update_refresh_token(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateRefreshTokenRequest>,
) -> impl IntoResponse {
    match state.service.update_refresh_token(id, payload) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "credential #{} refreshToken Updated (currently still disabled, please enable manually).",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/refresh
/// force refresh the credential Token
pub async fn force_refresh_token(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.force_refresh_token(id).await {
        Ok(_) => Json(SuccessResponse::new(format!(
            "credential #{} Token alreadyforcerefresh",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/reset-stats
/// reset all credentials success_count
pub async fn reset_all_success_count(State(state): State<AdminState>) -> impl IntoResponse {
    match state.service.reset_success_count(None) {
        Ok(count) => Json(SuccessResponse::new(format!(
            "reset {} itemcredentialof success_count",
            count
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/reset-stats
/// reset the specified credential success_count
pub async fn reset_success_count(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.reset_success_count(Some(id)) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "credential #{} success_count reset",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/proxy-pool
/// get the proxy pool list
pub async fn get_proxy_pool(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_proxy_pool();
    Json(response)
}

/// POST /api/admin/proxy-pool
/// add a proxy to the pool
pub async fn add_proxy(
    State(state): State<AdminState>,
    Json(payload): Json<AddProxyRequest>,
) -> impl IntoResponse {
    match state.service.add_proxy(payload.url, payload.label) {
        Ok(entry) => Json(entry).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/proxy-pool/batch
/// batch add proxies
pub async fn batch_add_proxies(
    State(state): State<AdminState>,
    Json(payload): Json<BatchAddProxyRequest>,
) -> impl IntoResponse {
    let (added, errors) = state.service.batch_add_proxies(payload);
    Json(serde_json::json!({
        "added": added.len(),
        "errors": errors.len(),
        "proxies": added,
        "errorMessages": errors
    }))
}

/// DELETE /api/admin/proxy-pool/:id
/// deleteproxy
pub async fn delete_proxy(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.delete_proxy(id) {
        Ok(_) => Json(SuccessResponse::new(format!("proxy #{} deleted", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/proxy-pool/:id/enabled
/// set the proxy enabled/disable
pub async fn set_proxy_enabled(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let enabled = payload
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    match state.service.set_proxy_enabled(id, enabled) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "proxy #{} already{}",
            id,
            if enabled { "enable" } else { "disable" }
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/proxy
/// Allocates a proxy from the pool to a credential.
pub async fn assign_proxy_to_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<AssignProxyRequest>,
) -> impl IntoResponse {
    match state.service.assign_proxy_to_credential(id, payload) {
        Ok(_) => Json(SuccessResponse::new(format!("credential #{} proxyupdated", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/proxy-pool/:id/check
/// Instantly probes a single proxy connectivity.
pub async fn check_proxy(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.check_proxy(id).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/proxy-pool/check-all
/// Triggers the health check of all proxies.
pub async fn check_all_proxies(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.check_all_proxies().await)
}

/// POST /api/admin/proxy-pool/assign-round-robin
/// Round robin batch allocates available proxies to credentials.
pub async fn assign_proxies_round_robin(
    State(state): State<AdminState>,
    Json(payload): Json<AssignRoundRobinRequest>,
) -> impl IntoResponse {
    match state
        .service
        .assign_proxies_round_robin(payload.credential_ids)
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/config/load-balancing
/// get the load balancing mode
pub async fn get_load_balancing_mode(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_load_balancing_mode();
    Json(response)
}

/// PUT /api/admin/config/load-balancing
/// set the load balancing mode
pub async fn set_load_balancing_mode(
    State(state): State<AdminState>,
    Json(payload): Json<SetLoadBalancingModeRequest>,
) -> impl IntoResponse {
    match state.service.set_load_balancing_mode(payload) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/config/account-throttle
/// Gets the account level throttle failover config.
pub async fn get_account_throttle_config(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.get_account_throttle_config())
}

/// PUT /api/admin/config/account-throttle
/// Updates the account level throttle failover config.
pub async fn set_account_throttle_config(
    State(state): State<AdminState>,
    Json(payload): Json<SetAccountThrottleConfigRequest>,
) -> impl IntoResponse {
    match state.service.set_account_throttle_config(payload) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/config/log-governance
/// get the log governance configuration (trace switch / trace retain / usage retained)
pub async fn get_log_governance_config(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.get_log_governance_config())
}

/// PUT /api/admin/config/log-governance
/// Updates the log governance config (effective at runtime). + persist config.json)
pub async fn set_log_governance_config(
    State(state): State<AdminState>,
    Json(payload): Json<SetLogGovernanceConfigRequest>,
) -> impl IntoResponse {
    match state.service.set_log_governance_config(payload) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/idc/start
/// initiate IdC device authorization login
pub async fn start_idc_login(
    State(state): State<AdminState>,
    Json(payload): Json<StartIdcLoginRequest>,
) -> impl IntoResponse {
    match state.service.start_idc_login(payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/idc/poll/:session_id
/// poll IdC login state (by the frontend as poll_interval call)
pub async fn poll_idc_login(
    State(state): State<AdminState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.service.poll_idc_login(&session_id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/social/start
/// initiate Social login, return portal URL
pub async fn start_social_login(
    State(state): State<AdminState>,
    Json(payload): Json<StartSocialLoginRequest>,
) -> impl IntoResponse {
    match state.service.start_social_login(payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/social/poll/:session_id
/// poll Social loginstate
pub async fn poll_social_login(
    State(state): State<AdminState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.service.poll_social_login(&session_id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/social/complete/:session_id
///
/// Manually completed in the remote access case. Social login:
/// The user copies from the browser address bar. OAuth callback URL,beforeendextract code/state/login_option call this interface after.
pub async fn complete_social_login(
    State(state): State<AdminState>,
    Path(session_id): Path<String>,
    Json(payload): Json<CompleteSocialLoginRequest>,
) -> impl IntoResponse {
    match state
        .service
        .complete_social_login(
            &session_id,
            payload.code,
            payload.state,
            payload.login_option,
            payload.path,
        )
        .await
    {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/auth/callback/{*tail}
///
/// under remote deployment mode OAuth Public callback entry (no auth, reached by browser top level navigation).
/// Kiro portal in redirect_uri endappend `/oauth/callback` or `/signin/callback`,
/// so the full path looks like `/api/admin/auth/callback/oauth/callback?code=...&state=...`.
///
/// safe:dependency OAuth `state`(random per session UUID) locate the session, provide CSRF protection, at the same trust level as the local callback server.
/// This route only delivers the callback data into the session. channel,real token redemption by `poll_social_login` unifydone.
pub async fn social_oauth_callback(
    State(state): State<AdminState>,
    Path(tail): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Html<String> {
    use crate::kiro::auth::social::OAuthCallbackData;
    use super::service::RemoteCallbackOutcome;

    // OAuth Error callback (such as the user denying authorization).
    if params.contains_key("error") {
        let msg = params
            .get("error_description")
            .or_else(|| params.get("error"))
            .cloned()
            .unwrap_or_else(|| "unknownerror".to_string());
        return Html(render_callback_page(false, &format!("authorizationfailed:{}", msg)));
    }

    let Some(code) = params.get("code").cloned() else {
        return Html(render_callback_page(false, "callbackmissing code parameter"));
    };
    let oauth_state = params.get("state").cloned().unwrap_or_default();
    let login_option = params.get("login_option").cloned().unwrap_or_default();
    // portal the appended path (oauth/callback or signin/callback), used to restore token redeemuseof redirect_uri
    let path = {
        let trimmed = tail.trim_start_matches('/');
        if trimmed.is_empty() {
            "/oauth/callback".to_string()
        } else {
            format!("/{}", trimmed)
        }
    };

    let data = OAuthCallbackData {
        code,
        login_option,
        path,
        state: oauth_state.clone(),
    };

    match state.service.deliver_remote_social_callback(&oauth_state, data) {
        RemoteCallbackOutcome::Delivered => {
            Html(render_callback_page(true, "The login callback was received; please go back. Kiro Admin the tab views the result"))
        }
        RemoteCallbackOutcome::AlreadyCompleted => {
            Html(render_callback_page(true, "This login callback has already been processed; please go back. Kiro Admin tab"))
        }
        RemoteCallbackOutcome::Expired => {
            Html(render_callback_page(false, "The login session has expired; please return to the admin panel and start login again."))
        }
        RemoteCallbackOutcome::NotFound => Html(render_callback_page(
            false,
            "The matching login session was not found (the callback address may not be configured or the session expired); please return to the admin panel and start again.",
        )),
    }
}

/// render OAuth callback prompt page (success / two styles for failure)
fn render_callback_page(success: bool, message: &str) -> String {
    let (title, icon, color) = if success {
        ("logincallback", "✓", "#34c759")
    } else {
        ("loginfailed", "✗", "#ff3b30")
    };
    format!(
        "<html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>{title}</title></head>\
         <body style='font-family:-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif;text-align:center;padding:60px 20px;background:#f5f5f7;margin:0'>\
         <div style='max-width:420px;margin:0 auto;background:#fff;border-radius:16px;padding:40px 24px;box-shadow:0 1px 3px rgba(0,0,0,.08)'>\
         <div style='font-size:48px;line-height:1;color:{color};margin-bottom:16px'>{icon}</div>\
         <h2 style='margin:0 0 12px;font-size:20px;color:#1d1d1f'>{title}</h2>\
         <p style='margin:0;color:#6e6e73;font-size:15px;line-height:1.5'>{message}</p>\
         <p style='margin:20px 0 0;color:#aeaeb2;font-size:13px'>this tab can be closed.</p>\
         </div></body></html>"
    )
}

/// GET /api/admin/config/global-proxy
/// Gets the current global proxy config.
pub async fn get_global_proxy(State(state): State<AdminState>) -> impl IntoResponse {
    Json(GlobalProxyResponse {
        proxy_url: state.service.get_global_proxy(),
    })
}

/// PUT /api/admin/config/global-proxy
/// Sets or clears the global proxy config.
pub async fn set_global_proxy(
    State(state): State<AdminState>,
    Json(payload): Json<SetGlobalProxyRequest>,
) -> impl IntoResponse {
    match state.service.set_global_proxy(payload.proxy_url) {
        Ok(_) => Json(SuccessResponse::new("the global proxy has been updated")).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/config/update
/// Gets the online update config (does not echo GitHub Token plaintext)
pub async fn get_update_config(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.get_update_config())
}

/// PUT /api/admin/config/update
/// set the online update configuration
pub async fn set_update_config(
    State(state): State<AdminState>,
    Json(payload): Json<SetUpdateConfigRequest>,
) -> impl IntoResponse {
    match state.service.set_update_config(payload) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/system/update/pull
/// Downloads and verifies the new binary (does not replace the current process).
pub async fn pull_update_image(State(state): State<AdminState>) -> impl IntoResponse {
    match state.service.pull_update_image().await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/system/update/apply
/// Downloads the new binary and replaces it. exe, the process exit is taken over by the container restart policy.
pub async fn apply_image_update(State(state): State<AdminState>) -> impl IntoResponse {
    match state.service.apply_image_update().await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/system/update/rollback
/// use `<exe>.backup` Restores the executable and exits the process.
pub async fn rollback_image_update(State(state): State<AdminState>) -> impl IntoResponse {
    match state.service.rollback_image_update().await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/system/update/check?force=true
/// query GitHub Releases whether there is a new version (with 30 minutescache)
pub async fn check_update(
    State(state): State<AdminState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let force = matches!(params.get("force").map(String::as_str), Some("true" | "1"));
    let info = state.service.check_update(force).await;
    Json(info).into_response()
}

/// POST /api/admin/system/update/rate-limit
/// query GitHub API The current throttle quota (may carry token used for"validate before saving")
pub async fn check_rate_limit(
    State(state): State<AdminState>,
    payload: Option<Json<super::types::CheckRateLimitRequest>>,
) -> impl IntoResponse {
    let req = payload.map(|Json(p)| p).unwrap_or_default();
    let info = state.service.check_rate_limit(req).await;
    Json(info).into_response()
}

/// POST /api/admin/credentials/:id/relogin/social/start
/// initiate Social Re-login (updates the existing credential Token rather than creating a new credential)
pub async fn start_social_relogin(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<StartSocialLoginRequest>,
) -> impl IntoResponse {
    match state.service.start_social_relogin(id, payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/relogin/social/poll/:session_id
/// poll Social re login state
pub async fn poll_social_relogin(
    State(state): State<AdminState>,
    Path((_, session_id)): Path<CredSessionPath>,
) -> impl IntoResponse {
    match state.service.poll_social_login(&session_id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/relogin/social/complete/:session_id
/// manually complete under remote mode Social heavynew login
pub async fn complete_social_relogin(
    State(state): State<AdminState>,
    Path((_, session_id)): Path<CredSessionPath>,
    Json(payload): Json<CompleteSocialLoginRequest>,
) -> impl IntoResponse {
    match state
        .service
        .complete_social_login(
            &session_id,
            payload.code,
            payload.state,
            payload.login_option,
            payload.path,
        )
        .await
    {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/relogin/idc/start
/// initiate IdC Re-login (updates the existing credential Token rather than creating a new credential)
pub async fn start_idc_relogin(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<StartIdcLoginRequest>,
) -> impl IntoResponse {
    match state.service.start_idc_relogin(id, payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/relogin/idc/poll/:session_id
/// poll IdC re login state
pub async fn poll_idc_relogin(
    State(state): State<AdminState>,
    Path((_, session_id)): Path<CredSessionPath>,
) -> impl IntoResponse {
    match state.service.poll_idc_login(&session_id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// PUT /api/admin/config/admin-key
/// modifyloginAPIkey (adminApiKey) and persists to the config file.
/// this key Used for admin panel login; takes effect immediately after change.
pub async fn update_admin_key(
    State(state): State<AdminState>,
    Json(payload): Json<UpdateAdminKeyRequest>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    let new_key = payload.new_key.trim().to_string();
    if new_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(super::types::AdminErrorResponse::invalid_request(
                "new loginAPIthe key cannot be empty",
            )),
        )
            .into_response();
    }

    // update the in memory loginAPIkey
    *state.admin_api_key.write() = new_key.clone();

    // via service persistto config.json(load the latest from disk before writing, avoiding overwriting other fields)
    state.service.persist_admin_key(&new_key);

    Json(SuccessResponse::new("loginAPIkeyupdated")).into_response()
}

// ============ client API Key dispatch ============

fn key_to_item(k: &super::client_keys::ClientKey) -> ClientKeyItem {
    ClientKeyItem {
        id: k.id,
        masked_key: mask_client_key(&k.key),
        name: k.name.clone(),
        description: k.description.clone(),
        disabled: k.disabled,
        created_at: k.created_at.clone(),
        last_used_at: k.last_used_at.clone(),
        total_calls: k.total_calls,
        total_input_tokens: k.total_input_tokens,
        total_output_tokens: k.total_output_tokens,
        total_cache_creation_tokens: k.total_cache_creation_tokens,
        total_cache_read_tokens: k.total_cache_read_tokens,
        group: k.group.clone(),
        is_system: k.is_system,
    }
}

/// GET /api/admin/client-keys
pub async fn list_client_keys(State(state): State<AdminState>) -> impl IntoResponse {
    let keys = state.client_keys.list();
    let items: Vec<ClientKeyItem> = keys.iter().map(key_to_item).collect();
    Json(ClientKeysResponse {
        total: items.len(),
        keys: items,
    })
}

/// POST /api/admin/client-keys
pub async fn create_client_key(
    State(state): State<AdminState>,
    Json(payload): Json<CreateClientKeyRequest>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    let name = payload.name.trim();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(super::types::AdminErrorResponse::invalid_request(
                "name notcanis empty",
            )),
        )
            .into_response();
    }
    let entry = state.client_keys.create(
        name.to_string(),
        payload
            .description
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty()),
        payload
            .group
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty()),
    );
    Json(CreateClientKeyResponse {
        id: entry.id,
        key: entry.key,
        name: entry.name,
        created_at: entry.created_at,
    })
    .into_response()
}

/// DELETE /api/admin/client-keys/:id
pub async fn delete_client_key(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    if state.client_keys.is_system(id) {
        return (
            StatusCode::CONFLICT,
            Json(super::types::AdminErrorResponse::invalid_request(
                "systemkey (config.json apiKey)notdeletable",
            )),
        )
            .into_response();
    }
    if state.client_keys.delete(id) {
        Json(SuccessResponse::new(format!("Key #{} deleted", id))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(super::types::AdminErrorResponse::not_found(format!(
                "Key #{} does not exist",
                id
            ))),
        )
            .into_response()
    }
}

/// PUT /api/admin/client-keys/:id
pub async fn update_client_key(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateClientKeyRequest>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    let description = payload
        .description
        .map(|d| if d.is_empty() { None } else { Some(d) });
    let group = payload
        .group
        .map(|g| {
            let t = g.trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        });
    if state.client_keys.update_meta(id, payload.name, description, group) {
        Json(SuccessResponse::new(format!("Key #{} updated", id))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(super::types::AdminErrorResponse::not_found(format!(
                "Key #{} does not exist",
                id
            ))),
        )
            .into_response()
    }
}

/// POST /api/admin/client-keys/:id/disabled
pub async fn set_client_key_disabled(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetDisabledRequest>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    if state.client_keys.set_disabled(id, payload.disabled) {
        let action = if payload.disabled { "disable" } else { "enable" };
        Json(SuccessResponse::new(format!("Key #{} already{}", id, action))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(super::types::AdminErrorResponse::not_found(format!(
                "Key #{} does not exist",
                id
            ))),
        )
            .into_response()
    }
}

/// POST /api/admin/client-keys/:id/reset-stats
pub async fn reset_client_key_stats(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    if state.client_keys.reset_stats(id) {
        Json(SuccessResponse::new(format!("Key #{} statisticsreset", id))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(super::types::AdminErrorResponse::not_found(format!(
                "Key #{} does not exist",
                id
            ))),
        )
            .into_response()
    }
}

/// POST /api/admin/client-keys/:id/rotate
///
/// rotate Key value: the old plaintext immediately becomes invalid, a new plaintext is generated and returned (visible only this once).
/// retain id/name/description/group/statistics/disabled unchanged; no need to rebind the group.
pub async fn rotate_client_key(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    use axum::http::StatusCode;
    match state.client_keys.rotate(id) {
        Some(entry) => {
            // After the system key rotates, the plaintext changed and must be written back in sync. config.json apiKey,
            // otherwise the next startup ensure_system_key due to old apiKey Not in the list, causing a duplicate import.
            if entry.is_system {
                state.service.persist_api_key(&entry.key);
            }
            Json(CreateClientKeyResponse {
                id: entry.id,
                key: entry.key,
                name: entry.name,
                created_at: entry.created_at,
            })
            .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(super::types::AdminErrorResponse::not_found(format!(
                "Key #{} does not exist",
                id
            ))),
        )
            .into_response(),
    }
}

// ============ usagestatistics ============

fn parse_range(params: &std::collections::HashMap<String, String>) -> Result<Range, String> {
    let Some(range) = params.get("range") else {
        return Err("range must be 24h,7d or 30d".to_string());
    };
    Range::parse(range.as_str()).ok_or_else(|| "range must be 24h,7d or 30d".to_string())
}

fn parse_key_id(params: &HashMap<String, String>) -> Result<Option<u64>, String> {
    match params.get("keyId") {
        Some(s) => s
            .parse::<u64>()
            .map(Some)
            .map_err(|_| "keyId must becountcharacter".to_string()),
        None => Ok(None),
    }
}

/// Parses the optional group filter parameter. An empty string is treated as not provided.
fn parse_group_filter(params: &HashMap<String, String>) -> Option<String> {
    params
        .get("group")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// take group the name into all credentials under that group. id the allow list, give UsageAggregator use.
/// return None Indicates no group specified (no filter); returns Some(empty set) is alsovalidvalue——means there are no credentials under that group,
/// all query will naturally return an empty result.
fn group_to_cred_ids(
    state: &AdminState,
    group: Option<&str>,
) -> Option<std::collections::HashSet<u64>> {
    let g = group?;
    let snapshot = state.service.get_all_credentials();
    Some(
        snapshot
            .credentials
            .iter()
            .filter(|c| c.groups.iter().any(|cg| cg == g))
            .map(|c| c.id)
            .collect(),
    )
}

fn parse_granularity(params: &HashMap<String, String>) -> Result<StatsGranularity, String> {
    match params.get("granularity") {
        Some(s) => {
            StatsGranularity::parse(s).ok_or_else(|| "granularity must be hour or day".to_string())
        }
        None => Err("granularity must be hour or day".to_string()),
    }
}

fn parse_stats_window(params: &HashMap<String, String>) -> Result<StatsQueryWindow, String> {
    let granularity = parse_granularity(params)?;
    match (params.get("startDate"), params.get("endDate")) {
        (Some(start), Some(end)) => custom_stats_window(start, end, granularity),
        (None, None) => Ok(StatsQueryWindow::preset(parse_range(params)?, granularity)),
        _ => Err("startDate and endDate must be provided together".to_string()),
    }
}

fn custom_stats_window(
    start: &str,
    end: &str,
    granularity: StatsGranularity,
) -> Result<StatsQueryWindow, String> {
    let start_date = parse_stats_date(start, "startDate")?;
    let end_date = parse_stats_date(end, "endDate")?;
    if end_date < start_date {
        return Err("endDate notcanearlyat startDate".to_string());
    }
    let start_ts = local_midnight_ts(start_date)?;
    let end_ts = local_midnight_ts(end_date + Duration::days(1))?;
    Ok(StatsQueryWindow {
        start_ts,
        end_ts,
        granularity,
    })
}

fn parse_stats_date(value: &str, name: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("{} mustuse YYYY-MM-DD format", name))
}

fn local_midnight_ts(date: NaiveDate) -> Result<i64, String> {
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
        .map(|d| d.timestamp())
        .ok_or_else(|| format!("date {} cannot convert to local time", date))
}

fn stats_query_parts(
    params: &HashMap<String, String>,
) -> Result<(StatsQueryWindow, Option<u64>), String> {
    Ok((parse_stats_window(params)?, parse_key_id(params)?))
}

fn stats_bad_request(message: String) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(super::types::AdminErrorResponse::invalid_request(message)),
    )
        .into_response()
}

/// GET /api/admin/stats/overview
pub async fn stats_overview(State(state): State<AdminState>) -> impl IntoResponse {
    let overview = state.usage_aggregator.overview();
    // additional: currently active Key / credential count
    let active_keys = state.client_keys.active_count() as u64;
    let snapshot = state.service.get_all_credentials();
    let active_credentials = snapshot.credentials.iter().filter(|c| !c.disabled).count() as u64;
    let response = serde_json::json!({
        "todayCalls": overview.today_calls,
        "todayInputTokens": overview.today_input_tokens,
        "todayOutputTokens": overview.today_output_tokens,
        "todayErrors": overview.today_errors,
        "todayCredits": overview.today_credits,
        "weekCalls": overview.week_calls,
        "weekInputTokens": overview.week_input_tokens,
        "weekOutputTokens": overview.week_output_tokens,
        "weekCredits": overview.week_credits,
        "activeClientKeys": active_keys,
        "activeCredentials": active_credentials,
    });
    Json(response)
}

/// GET /api/admin/stats/timeseries?range=24h|7d|30d&granularity=hour|day&group=...
pub async fn stats_timeseries(
    State(state): State<AdminState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    let (window, key_id) = match stats_query_parts(&params) {
        Ok(parts) => parts,
        Err(message) => return stats_bad_request(message),
    };
    let group = parse_group_filter(&params);
    let cred_ids = group_to_cred_ids(&state, group.as_deref());
    let points = state.usage_aggregator.query_timeseries(window, key_id, cred_ids.as_ref());
    Json(points).into_response()
}

/// GET /api/admin/stats/by-model?range=24h|7d|30d
pub async fn stats_by_model(
    State(state): State<AdminState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    let (window, key_id) = match stats_query_parts(&params) {
        Ok(parts) => parts,
        Err(message) => return stats_bad_request(message),
    };
    let data = state.usage_aggregator.query_by_model(window, key_id);
    Json(data).into_response()
}

/// GET /api/admin/stats/by-credential?range=24h|7d|30d
pub async fn stats_by_credential(
    State(state): State<AdminState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    let (window, key_id) = match stats_query_parts(&params) {
        Ok(parts) => parts,
        Err(message) => return stats_bad_request(message),
    };
    let group = parse_group_filter(&params);
    // Pulls a credential snapshot (both to attach to the response email,alsousecomeby group build cred_ids whitelist,
    // avoid querying twice separately)
    let snapshot = state.service.get_all_credentials();
    let email_map: std::collections::HashMap<u64, Option<String>> = snapshot
        .credentials
        .iter()
        .map(|c| (c.id, c.email.clone()))
        .collect();
    let cred_ids: Option<std::collections::HashSet<u64>> = group.as_deref().map(|g| {
        snapshot
            .credentials
            .iter()
            .filter(|c| c.groups.iter().any(|cg| cg == g))
            .map(|c| c.id)
            .collect()
    });
    let data = state.usage_aggregator.query_by_credential(window, key_id, cred_ids.as_ref());
    let enriched: Vec<serde_json::Value> = data
        .into_iter()
        .map(|d| {
            let email = email_map.get(&d.credential_id).cloned().flatten();
            serde_json::json!({
                "credentialId": d.credential_id,
                "email": email,
                "calls": d.calls,
                "inputTokens": d.input_tokens,
                "outputTokens": d.output_tokens,
                "errors": d.errors,
            })
        })
        .collect();
    Json(enriched).into_response()
}

/// GET /api/admin/traces
/// Queries the request trace records (including per hop detail).
/// query parameter:status / errorType / credentialId / keyId / group / model / onlyFailed / limit / offset
/// returns:{ records: [...], total: N }
pub async fn list_traces(
    State(state): State<AdminState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // parse the group filter: take group name conversionascredential id Allowlist (executed before the query, avoiding pagination misalignment).
    let group = params.get("group").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let credential_ids: Option<Vec<u64>> = group.as_ref().map(|g| {
        state
            .service
            .get_all_credentials()
            .credentials
            .iter()
            .filter(|c| c.groups.iter().any(|cg| cg == g))
            .map(|c| c.id)
            .collect()
    });

    let query = TraceQuery {
        status: params.get("status").filter(|s| !s.is_empty()).cloned(),
        error_type: params.get("errorType").filter(|s| !s.is_empty()).cloned(),
        credential_id: params
            .get("credentialId")
            .and_then(|s| s.parse::<u64>().ok()),
        key_id: params.get("keyId").and_then(|s| s.parse::<u64>().ok()),
        failed_attempt_credential_id: params
            .get("failedAttemptCredentialId")
            .and_then(|s| s.parse::<u64>().ok()),
        model: params.get("model").filter(|s| !s.is_empty()).cloned(),
        only_failed: params
            .get("onlyFailed")
            .map(|s| s == "true" || s == "1")
            .unwrap_or(false),
        credential_ids,
        limit: params
            .get("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(crate::admin::trace_db::DEFAULT_QUERY_LIMIT)
            .min(1000),
        offset: params
            .get("offset")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0),
    };
    let (records, total) = state.trace_store.query_paged(&query);

    // append credential email convenient for frontend display (with stats_by_credential consistent)
    let snapshot = state.service.get_all_credentials();
    let email_map: HashMap<u64, Option<String>> = snapshot
        .credentials
        .iter()
        .map(|c| (c.id, c.email.clone()))
        .collect();
    let client_key_name_map: HashMap<u64, String> = state
        .client_keys
        .list()
        .into_iter()
        .map(|k| (k.id, k.name))
        .collect();
    // entry Key Name resolution: matches a client Key the name table takes the name, otherwise falls back. #id
    // (master apiKey taken offline, history key_id=0 the record will be shown as #0)
    let key_label = |key_id: u64| -> String {
        client_key_name_map
            .get(&key_id)
            .cloned()
            .unwrap_or_else(|| format!("#{}", key_id))
    };

    let enriched: Vec<serde_json::Value> = records
        .into_iter()
        .map(|r| {
            let final_email = email_map.get(&r.final_credential_id).cloned().flatten();
            let key_name = key_label(r.key_id);
            // attempts ineach hop also attaches email
            let attempts: Vec<serde_json::Value> = r
                .attempts
                .iter()
                .map(|a| {
                    let email = email_map.get(&a.credential_id).cloned().flatten();
                    serde_json::json!({
                        "attempt": a.attempt,
                        "credentialId": a.credential_id,
                        "email": email,
                        "endpoint": a.endpoint,
                        "httpStatus": a.http_status,
                        "outcome": a.outcome,
                        "errorSnippet": a.error_snippet,
                        "durationMs": a.duration_ms,
                    })
                })
                .collect();
            serde_json::json!({
                "traceId": r.trace_id,
                "ts": r.ts,
                "keyId": r.key_id,
                "keySource": r.key_source,
                "keyName": key_name,
                "model": r.model,
                "isStream": r.is_stream,
                "finalStatus": r.final_status,
                "finalCredentialId": r.final_credential_id,
                "finalEmail": final_email,
                "errorType": r.error_type,
                "errorMessage": r.error_message,
                "totalAttempts": r.total_attempts,
                "durationMs": r.duration_ms,
                "interruptedAfterBytes": r.interrupted_after_bytes,
                "inputTokens": r.input_tokens,
                "outputTokens": r.output_tokens,
                "cacheCreationTokens": r.cache_creation_tokens,
                "cacheReadTokens": r.cache_read_tokens,
                "totalTokens": r.input_tokens + r.output_tokens + r.cache_creation_tokens + r.cache_read_tokens,
                "credits": r.credits,
                "firstTokenMs": r.first_token_ms,
                "attempts": attempts,
            })
        })
        .collect();
    Json(serde_json::json!({ "records": enriched, "total": total }))
}

/// GET /api/admin/traces/failure-stats
/// Aggregates failure counts by credential (auth, / accountthrottle / the other three types), used for color coded card display.
/// return { "<credentialId>": { auth, throttle, other }, ... }
pub async fn trace_failure_stats(State(state): State<AdminState>) -> impl IntoResponse {
    let stats = state.trace_store.failure_stats();
    let map: std::collections::HashMap<String, serde_json::Value> = stats
        .into_iter()
        .map(|(id, s)| {
            (
                id.to_string(),
                serde_json::json!({
                    "auth": s.auth,
                    "throttle": s.throttle,
                    "other": s.other,
                }),
            )
        })
        .collect();
    Json(map)
}

// ============ Account group (independent entity).============

fn group_to_item(
    g: &super::groups::Group,
    state: &AdminState,
) -> super::types::GroupItem {
    super::types::GroupItem {
        name: g.name.clone(),
        description: g.description.clone(),
        created_at: g.created_at.clone(),
        credential_count: state
            .service
            .token_manager()
            .count_credentials_with_group(&g.name),
        client_key_count: state.client_keys.count_with_group(&g.name),
    }
}

/// GET /api/admin/groups
pub async fn list_groups(State(state): State<AdminState>) -> impl IntoResponse {
    let groups = state.groups.list();
    let items: Vec<super::types::GroupItem> =
        groups.iter().map(|g| group_to_item(g, &state)).collect();
    Json(super::types::GroupsResponse {
        total: items.len(),
        groups: items,
    })
}

/// POST /api/admin/groups
pub async fn create_group(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::CreateGroupRequest>,
) -> impl IntoResponse {
    match state
        .groups
        .create(payload.name, payload.description)
    {
        Ok(g) => Json(group_to_item(&g, &state)).into_response(),
        Err(e) => {
            let msg = e.to_string();
            // "already exists" → 409; other validation failures → 400
            let (code, resp) = if msg.contains("already exists") {
                (
                    StatusCode::CONFLICT,
                    super::types::AdminErrorResponse::invalid_request(msg),
                )
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    super::types::AdminErrorResponse::invalid_request(msg),
                )
            };
            (code, Json(resp)).into_response()
        }
    }
}

/// PATCH /api/admin/groups/:name
///
/// rename / Changes the note. On rename, cascade updates all credentials that reference the group. / client Key.
pub async fn update_group(
    State(state): State<AdminState>,
    Path(name): Path<String>,
    Json(payload): Json<super::types::UpdateGroupRequest>,
) -> impl IntoResponse {
    if !state.groups.exists(&name) {
        return (
            StatusCode::NOT_FOUND,
            Json(super::types::AdminErrorResponse::not_found(format!(
                "group {} does not exist",
                name
            ))),
        )
            .into_response();
    }

    // 1. Rename (validate the target name first, then cascade).
    let mut current_name = name.clone();
    if let Some(new_name) = payload.new_name.as_deref() {
        let trimmed = new_name.trim();
        if !trimmed.is_empty() && trimmed != name {
            // GroupManager insidedouniqueproperty / length / empty check
            match state.groups.rename(&name, trimmed) {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    let code = if msg.contains("already exists") {
                        StatusCode::CONFLICT
                    } else {
                        StatusCode::BAD_REQUEST
                    };
                    return (
                        code,
                        Json(super::types::AdminErrorResponse::invalid_request(msg)),
                    )
                        .into_response();
                }
            }
            // Cascade: on failure, tries to roll back the group rename (avoiding registry and credential / Key notconsistent)
            let cred_res = state
                .service
                .token_manager()
                .rename_credential_group(&name, trimmed);
            if let Err(e) = cred_res {
                let _ = state.groups.rename(trimmed, &name);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(super::types::AdminErrorResponse::internal_error(format!(
                        "cascade update credentials failed: {}",
                        e
                    ))),
                )
                    .into_response();
            }
            state.client_keys.rename_group(&name, trimmed);
            current_name = trimmed.to_string();
        }
    }

    // 2. edit note
    if let Some(desc) = payload.description {
        let desc_opt = if desc.trim().is_empty() {
            None
        } else {
            Some(desc)
        };
        if let Err(e) = state.groups.update_description(&current_name, desc_opt) {
            return (
                StatusCode::BAD_REQUEST,
                Json(super::types::AdminErrorResponse::invalid_request(e.to_string())),
            )
                .into_response();
        }
    }

    let group = match state.groups.get(&current_name) {
        Some(g) => g,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::types::AdminErrorResponse::internal_error(
                    "The group disappeared during the update; abnormal state.",
                )),
            )
                .into_response();
        }
    };
    Json(group_to_item(&group, &state)).into_response()
}

/// DELETE /api/admin/groups/:name?force=true
///
/// By default refuses to delete a group still referenced; with `force=true` cascade cleans all references and deletes.
pub async fn delete_group(
    State(state): State<AdminState>,
    Path(name): Path<String>,
    Query(query): Query<super::types::DeleteGroupQuery>,
) -> impl IntoResponse {
    if !state.groups.exists(&name) {
        return (
            StatusCode::NOT_FOUND,
            Json(super::types::AdminErrorResponse::not_found(format!(
                "group {} does not exist",
                name
            ))),
        )
            .into_response();
    }

    let cred_count = state
        .service
        .token_manager()
        .count_credentials_with_group(&name);
    let key_count = state.client_keys.count_with_group(&name);

    if (cred_count > 0 || key_count > 0) && !query.force {
        return (
            StatusCode::CONFLICT,
            Json(super::types::AdminErrorResponse::invalid_request(format!(
                "the group is still referenced (credential {} / client Key {}), pass ?force=true cascade cleanup",
                cred_count, key_count
            ))),
        )
            .into_response();
    }

    if query.force {
        if let Err(e) = state
            .service
            .token_manager()
            .remove_credential_group(&name)
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::types::AdminErrorResponse::internal_error(format!(
                    "cascade cleanup credentials failed: {}",
                    e
                ))),
            )
                .into_response();
        }
        state.client_keys.clear_group(&name);
    }

    state.groups.delete(&name);
    Json(super::types::SuccessResponse::new(format!(
        "group {} deleted",
        name
    )))
    .into_response()
}
