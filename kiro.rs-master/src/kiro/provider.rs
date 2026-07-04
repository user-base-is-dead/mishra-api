//! Kiro API Provider
//!
//! core component, responsible for with Kiro API communicate
//! supports streaming and non streaming requests
//! Supports multi credential failover and retry.
//! supportbycredential level endpoint switch different Kiro API endpoint

use reqwest::Client;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use crate::admin::trace_db::{TraceAttempt, TraceSink, outcome, truncate_snippet};
use crate::http_client::{ProxyConfig, build_client};
use crate::kiro::endpoint::{KiroEndpoint, RequestContext};
use crate::kiro::machine_id;
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::token_manager::MultiTokenManager;
use crate::model::config::TlsBackend;
use parking_lot::Mutex;

/// The maximum retry count per credential.
const MAX_RETRIES_PER_CREDENTIAL: usize = 3;

/// Hard limit on total retries (avoids infinite retries).
///
/// note: upstream 429 mostly account level rate quota (SERVICE_REQUEST_RATE_EXCEEDED),peak period
/// When multiple accounts hit the ceiling at once, too many retries chain across accounts and amplify throttling. So the limit takes a smaller value,
/// together with 429 dedicated long backoff (see retry_delay_throttle), is time limited to return early rather than exhaust the quota.
const MAX_TOTAL_RETRIES: usize = 4;

/// HTTP Client Cache capacity limit (excludes the resident global proxy). client).
/// When the proxy pool has many entries, avoids each distinct proxy keeping a resident reqwest::Client causes unbounded memory growth.
const CLIENT_CACHE_CAP: usize = 64;

/// with a capacity limitof HTTP Client cache.
///
/// - key as effective proxy config (None = direct connect/globalfallback)
/// - protected key(the one corresponding to the global proxy effective configuration) is never evicted
/// - When capacity is exceeded, evicts the oldest non protected entry by insertion order.
struct ClientCache {
    map: HashMap<Option<ProxyConfig>, Client>,
    /// Insertion order (only records evictable non protected). key)
    order: std::collections::VecDeque<Option<ProxyConfig>>,
    /// protected, not participating in eviction key(global proxy)
    protected: Option<ProxyConfig>,
    cap: usize,
}

impl ClientCache {
    fn new(protected: Option<ProxyConfig>, initial: Client, cap: usize) -> Self {
        let mut map = HashMap::new();
        map.insert(protected.clone(), initial);
        Self {
            map,
            order: std::collections::VecDeque::new(),
            protected,
            cap,
        }
    }

    fn get(&self, key: &Option<ProxyConfig>) -> Option<Client> {
        self.map.get(key).cloned()
    }

    /// Inserts a new entry, evicting the oldest non protected entry when necessary.
    fn insert(&mut self, key: Option<ProxyConfig>, client: Client) {
        if key == self.protected || self.map.contains_key(&key) {
            self.map.insert(key, client);
            return;
        }
        while self.order.len() >= self.cap {
            if let Some(evict) = self.order.pop_front() {
                self.map.remove(&evict);
            } else {
                break;
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, client);
    }
}

/// API The call result, along with the upstream credential actually hit this time. ID(used for usage statistics)
pub struct KiroCallResult {
    pub response: reqwest::Response,
    pub credential_id: u64,
}

/// Kiro API Provider
///
/// core component, responsible for with Kiro API communicate
/// Supports the multi credential failover and retry mechanism.
/// by credential `endpoint` field selection [`KiroEndpoint`] implement
pub struct KiroProvider {
    token_manager: Arc<MultiTokenManager>,
    /// Global proxy config (used as fallback when a credential has no custom proxy).
    global_proxy: Option<ProxyConfig>,
    /// Client cache:key = effective proxy config, value = reqwest::Client
    /// Credentials with different proxy configs use different Client, credentials sharing the same proxy reuse it. Client.
    /// Evicts with a capacity limit (the global proxy client resident), avoiding unbounded memory growth as the number of proxies grows.
    client_cache: Mutex<ClientCache>,
    /// TLS backend config
    tls_backend: TlsBackend,
    /// endpoint implementation registry (key: endpoint name)
    endpoints: HashMap<String, Arc<dyn KiroEndpoint>>,
    /// Default endpoint name (when the credential does not specify endpoint used when)
    default_endpoint: String,
    /// already tried profileArn parseofcredential ID(enterprocessinside).
    ///
    /// avoidforopen bracketnone Enterprise profilethe account of (such as pure BuilderID) is called repeatedly on every request.
    /// `ListAvailableProfiles`.hit real ARN ofaccountwill ARN persist into the credential, afterwards
    /// via `streaming_profile_arn()` Hits directly and no longer enters the parse path.
    profile_resolution_attempted: Mutex<HashSet<u64>>,
}

impl KiroProvider {
    /// Creates one with the proxy config and endpoint registry. KiroProvider instance
    ///
    /// # Arguments
    /// * `token_manager` - multiple credentials Token manager
    /// * `proxy` - globalproxy config
    /// * `endpoints` - endpoint name → the implemented registry (contains at least `default_endpoint` correspondentryentry)
    /// * `default_endpoint` - the credential does not explicitly specify endpoint whenusedname
    pub fn with_proxy(
        token_manager: Arc<MultiTokenManager>,
        proxy: Option<ProxyConfig>,
        endpoints: HashMap<String, Arc<dyn KiroEndpoint>>,
        default_endpoint: String,
    ) -> Self {
        assert!(
            endpoints.contains_key(&default_endpoint),
            "default endpoint {} not in endpoints in registry",
            default_endpoint
        );
        let tls_backend = token_manager.config().tls_backend;
        // Warm up: builds the one corresponding to the global proxy. Client(as a protected resident entry)
        let initial_client = build_client(proxy.as_ref(), 720, tls_backend)
            .expect("create HTTP clientfailed");
        let client_cache = ClientCache::new(proxy.clone(), initial_client, CLIENT_CACHE_CAP);

        Self {
            token_manager,
            global_proxy: proxy,
            client_cache: Mutex::new(client_cache),
            tls_backend,
            endpoints,
            default_endpoint,
            profile_resolution_attempted: Mutex::new(HashSet::new()),
        }
    }

    /// Gets (or creates and caches) the corresponding one based on the credential proxy config. reqwest::Client
    fn client_for(&self, credentials: &KiroCredentials) -> anyhow::Result<Client> {
        let effective = credentials.effective_proxy(self.global_proxy.as_ref());
        let mut cache = self.client_cache.lock();
        if let Some(client) = cache.get(&effective) {
            return Ok(client);
        }
        let client = build_client(effective.as_ref(), 720, self.tls_backend)?;
        cache.insert(effective, client.clone());
        Ok(client)
    }

    /// based oncredentialselect endpoint implement
    fn endpoint_for(
        &self,
        credentials: &KiroCredentials,
    ) -> anyhow::Result<Arc<dyn KiroEndpoint>> {
        let name = credentials
            .endpoint
            .as_deref()
            .unwrap_or(&self.default_endpoint);
        self.endpoints
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknownendpoint: {}", name))
    }

    /// before initiating the request, ensure Enterprise / IdC accountofreal profileArn alreadyparseand write `ctx`.
    ///
    /// the streaming endpoint mandatorily requires profileArn;Enterprise / IdC accountmust firsttake BuilderID
    /// resolve the placeholder into the real ARN, pure BuilderID the account falls back to a placeholder.
    /// only forOAuth credential + profileArn an account that is missing or a placeholder triggers one upstream
    /// `ListAvailableProfiles` query (in process deduplication):
    /// - hit real ARN → write back `ctx.credentials.profile_arn` and by token_manager persist;
    ///   afterthiscredentialof `streaming_profile_arn()` Hits directly and no longer enters this path.
    /// - none Enterprise profile(pure BuilderID etc.)→ Keeps the placeholder fallback logic and marks as tried,
    ///   Avoids repeated queries on every request.
    async fn ensure_profile_arn(&self, ctx: &mut crate::kiro::token_manager::CallContext) {
        use crate::kiro::model::credentials::is_placeholder_profile_arn;

        if ctx.credentials.is_api_key_credential() {
            return;
        }
        let needs = match ctx.credentials.profile_arn.as_deref() {
            None => true,
            Some(arn) => is_placeholder_profile_arn(arn),
        };
        if !needs {
            return;
        }
        // In-process deduplication: marks as tried only after obtaining a definite upstream result, avoiding a single network jitter
        // Permanently keeps the account stuck on the placeholder (no retry until restart).
        if self.profile_resolution_attempted.lock().contains(&ctx.id) {
            return;
        }
        match self
            .token_manager
            .resolve_profile_arn_for(ctx.id, &ctx.token)
            .await
        {
            Ok(Some(arn)) => {
                ctx.credentials.profile_arn = Some(arn);
                self.profile_resolution_attempted.lock().insert(ctx.id);
            }
            Ok(None) => {
                // the upstream confirms the account has no Enterprise profile(pure BuilderID etc): mark as attempted,
                // Subsequent requests fall back to the placeholder logic and no longer query repeatedly.
                self.profile_resolution_attempted.lock().insert(ctx.id);
            }
            Err(e) => {
                // network/Transient error: does not mark, retries on the next request; this time by the original profileArn continue
                tracing::warn!("credential #{} parse real profileArn failed(byoriginal profileArn continue): {}", ctx.id, e);
            }
        }
    }

    /// sendnon streaming API request
    ///
    /// Supports multi credential failover (see [`Self::call_api_with_retry`]).
    /// `sink` Optional, used to report the trace per hop.
    pub async fn call_api(
        &self,
        request_body: &str,
        sink: Option<&dyn TraceSink>,
        group: Option<&str>,
    ) -> anyhow::Result<KiroCallResult> {
        self.call_api_with_retry(request_body, false, sink, group).await
    }

    /// send streaming API request
    pub async fn call_api_stream(
        &self,
        request_body: &str,
        sink: Option<&dyn TraceSink>,
        group: Option<&str>,
    ) -> anyhow::Result<KiroCallResult> {
        self.call_api_with_retry(request_body, true, sink, group).await
    }

    /// send MCP API request (WebSearch etc.tool call)
    pub async fn call_mcp(&self, request_body: &str) -> anyhow::Result<reqwest::Response> {
        self.call_mcp_with_retry(request_body).await
    }

    /// Internal method: with retry logic. MCP API call
    async fn call_mcp_with_retry(&self, request_body: &str) -> anyhow::Result<reqwest::Response> {
        let total_credentials = self.token_manager.total_count();
        let max_retries = (total_credentials * MAX_RETRIES_PER_CREDENTIAL).min(MAX_TOTAL_RETRIES);
        let mut last_error: Option<anyhow::Error> = None;
        let mut force_refreshed: HashSet<u64> = HashSet::new();

        for attempt in 0..max_retries {
            // MCP call (WebSearch and similar tools) do not involve model selection nor participate in group isolation.
            let ctx = match self.token_manager.acquire_context(None, None).await {
                Ok(c) => c,
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };

            let config = self.token_manager.config();
            let machine_id = machine_id::generate_from_credentials(&ctx.credentials, config);

            let endpoint = match self.endpoint_for(&ctx.credentials) {
                Ok(e) => e,
                Err(e) => {
                    last_error = Some(e);
                    // endpoint Parse failure: counted as a failure, switches to the next credential.
                    self.token_manager.report_failure(ctx.id);
                    continue;
                }
            };

            let rctx = RequestContext {
                credentials: &ctx.credentials,
                token: &ctx.token,
                machine_id: &machine_id,
                config,
            };

            let url = endpoint.mcp_url(&rctx);
            let body = endpoint.transform_mcp_body(request_body, &rctx);

            let base = self
                .client_for(&ctx.credentials)?
                .post(&url)
                .body(body)
                .header("content-type", endpoint.content_type())
                .header("Connection", "close");
            let request = endpoint.decorate_mcp(base, &rctx);

            let response = match request.send().await {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::warn!(
                        "MCP request send failed (attempt {}/{}): {}",
                        attempt + 1,
                        max_retries,
                        e
                    );
                    last_error = Some(e.into());
                    if attempt + 1 < max_retries {
                        sleep(Self::retry_delay(attempt)).await;
                    }
                    continue;
                }
            };

            let status = response.status();

            // success response
            if status.is_success() {
                self.token_manager.report_success(ctx.id);
                return Ok(response);
            }

            // failed response
            let body = response.text().await.unwrap_or_default();

            // 402 quota exhausted
            if status.as_u16() == 402 && endpoint.is_monthly_request_limit(&body) {
                let has_available = self.token_manager.report_quota_exhausted(ctx.id);
                if !has_available {
                    anyhow::bail!("MCP Request failed (all credentials exhausted).: {} {}", status, body);
                }
                last_error = Some(anyhow::anyhow!("MCP requestfailed: {} {}", status, body));
                continue;
            }

            // 400 Bad Request
            if status.as_u16() == 400 {
                anyhow::bail!("MCP requestfailed: {} {}", status, body);
            }

            // 401/403 credential issue
            if matches!(status.as_u16(), 401 | 403) {
                // token invalidated by the upstream: try first force-refresh, each credential gets only one chance
                if endpoint.is_bearer_token_invalid(&body) && !force_refreshed.contains(&ctx.id) {
                    force_refreshed.insert(ctx.id);
                    tracing::info!("credential #{} token Suspected to be invalidated by upstream; tries a forced refresh.", ctx.id);
                    if self.token_manager.force_refresh_token_for(ctx.id).await.is_ok() {
                        tracing::info!("credential #{} token Forced refresh succeeded; retries the request.", ctx.id);
                        continue;
                    }
                    tracing::warn!("credential #{} token Forced refresh failed; counted as a failure.", ctx.id);
                }

                let has_available = self.token_manager.report_failure(ctx.id);
                if !has_available {
                    anyhow::bail!("MCP Request failed (all credentials exhausted).: {} {}", status, body);
                }
                last_error = Some(anyhow::anyhow!("MCP requestfailed: {} {}", status, body));
                continue;
            }

            // transient error
            if matches!(status.as_u16(), 408 | 429) || status.is_server_error() {
                tracing::warn!(
                    "MCP Request failed (upstream transient error, try {}/{}): {} {}",
                    attempt + 1,
                    max_retries,
                    status,
                    body
                );
                last_error = Some(anyhow::anyhow!("MCP requestfailed: {} {}", status, body));
                if attempt + 1 < max_retries {
                    // 429 throttle uses a longer backoff;408/5xx still use the general fast backoff
                    let delay = if status.as_u16() == 429 {
                        Self::retry_delay_throttle(attempt)
                    } else {
                        Self::retry_delay(attempt)
                    };
                    sleep(delay).await;
                }
                continue;
            }

            // other 4xx
            if status.is_client_error() {
                anyhow::bail!("MCP requestfailed: {} {}", status, body);
            }

            // fallback
            last_error = Some(anyhow::anyhow!("MCP requestfailed: {} {}", status, body));
            if attempt + 1 < max_retries {
                sleep(Self::retry_delay(attempt)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("MCP Request failed: the maximum retry count has been reached ({}times)", max_retries)
        }))
    }

    /// Internal method: with retry logic. API call
    ///
    /// retry strategy:
    /// - each credential retries at most MAX_RETRIES_PER_CREDENTIAL times
    /// - total retriestimescount = min(credential count × retry count per credential, MAX_TOTAL_RETRIES)
    /// - hard limit 9 times, to avoid infinite retries
    async fn call_api_with_retry(
        &self,
        request_body: &str,
        is_stream: bool,
        sink: Option<&dyn TraceSink>,
        group: Option<&str>,
    ) -> anyhow::Result<KiroCallResult> {
        // The retry budget is computed from the account count of the group the current request belongs to, avoiding a small group getting too many wasted retries based on the global account count.
        let total_credentials = self.token_manager.total_count_in_group(group).max(1);
        let max_retries = (total_credentials * MAX_RETRIES_PER_CREDENTIAL).min(MAX_TOTAL_RETRIES);
        let mut last_error: Option<anyhow::Error> = None;
        let mut force_refreshed: HashSet<u64> = HashSet::new();
        let api_type = if is_stream { "streaming" } else { "non streaming" };

        // Tries to extract model info from the request body.
        let model = Self::extract_model_from_request(request_body);

        for attempt in 0..max_retries {
            let attempt_start = Instant::now();
            // get the call context (binding index,credentials,token)
            let mut ctx = match self.token_manager.acquire_context(model.as_deref(), group).await {
                Ok(c) => c,
                Err(e) => {
                    Self::emit_attempt(
                        sink, attempt, 0, "", None, outcome::UNKNOWN,
                        Some(&e.to_string()), attempt_start,
                    );
                    last_error = Some(e);
                    continue;
                }
            };

            // ensure Enterprise / IdC accountofreal profileArn Parsed (mandatorily required by the streaming endpoint).
            self.ensure_profile_arn(&mut ctx).await;

            let config = self.token_manager.config();
            let machine_id = machine_id::generate_from_credentials(&ctx.credentials, config);

            let endpoint = match self.endpoint_for(&ctx.credentials) {
                Ok(e) => e,
                Err(e) => {
                    Self::emit_attempt(
                        sink, attempt, ctx.id, "", None, outcome::UNKNOWN,
                        Some(&e.to_string()), attempt_start,
                    );
                    last_error = Some(e);
                    self.token_manager.report_failure(ctx.id);
                    continue;
                }
            };
            let endpoint_name = endpoint.name();

            let rctx = RequestContext {
                credentials: &ctx.credentials,
                token: &ctx.token,
                machine_id: &machine_id,
                config,
            };

            let url = endpoint.api_url(&rctx);
            let body = endpoint.transform_api_body(request_body, &rctx);

            tracing::debug!("use endpoint [{}] POST {}", endpoint.name(), url);
            tracing::debug!("the actually sent request body: {}", body);

            let base = self
                .client_for(&ctx.credentials)?
                .post(&url)
                .body(body)
                .header("content-type", endpoint.content_type())
                .header("Connection", "close");
            let request = endpoint.decorate_api(base, &rctx);

            // Prints the request headers actually sent (RUST_LOG=debug outputs it, convenient for troubleshooting)
            let request = request.build().map_err(|e| anyhow::anyhow!("build requestfailed: {}", e))?;
            if tracing::enabled!(tracing::Level::DEBUG) {
                for (k, v) in request.headers() {
                    tracing::debug!("  header {}: {}", k, v.to_str().unwrap_or("<binary>"));
                }
            }
            let response = match self.client_for(&ctx.credentials)?.execute(request).await {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::warn!(
                        "API request send failed (attempt {}/{}): {}",
                        attempt + 1,
                        max_retries,
                        e
                    );
                    Self::emit_attempt(
                        sink, attempt, ctx.id, endpoint_name, None,
                        outcome::NETWORK_ERROR, Some(&e.to_string()), attempt_start,
                    );
                    // a network error is usually the upstream/A transient link issue should not cause"disablecredential"or"switch credential"
                    // (otherwise a period of network jitter would wrongly disable all credentials, requiring a restart to recover)
                    last_error = Some(e.into());
                    if attempt + 1 < max_retries {
                        sleep(Self::retry_delay(attempt)).await;
                    }
                    continue;
                }
            };

            let status = response.status();

            // success response
            if status.is_success() {
                Self::emit_attempt(
                    sink, attempt, ctx.id, endpoint_name, Some(status.as_u16()),
                    outcome::SUCCESS, None, attempt_start,
                );
                self.token_manager.report_success(ctx.id);
                return Ok(KiroCallResult {
                    response,
                    credential_id: ctx.id,
                });
            }

            // failure response: read body for logging/error info
            let body = response.text().await.unwrap_or_default();

            // 402 Payment Required and quota exhausted: disables the credential and fails over.
            if status.as_u16() == 402 && endpoint.is_monthly_request_limit(&body) {
                tracing::warn!(
                    "API Request failed (quota exhausted, disable the credential and switch, try {}/{}): {} {}",
                    attempt + 1,
                    max_retries,
                    status,
                    body
                );
                Self::emit_attempt(
                    sink, attempt, ctx.id, endpoint_name, Some(status.as_u16()),
                    outcome::QUOTA_EXHAUSTED, Some(&body), attempt_start,
                );

                let has_available = self.token_manager.report_quota_exhausted(ctx.id);
                if !has_available {
                    anyhow::bail!(
                        "{} API Request failed (all credentials exhausted).: {} {}",
                        api_type,
                        status,
                        body
                    );
                }

                last_error = Some(anyhow::anyhow!(
                    "{} API requestfailed: {} {}",
                    api_type,
                    status,
                    body
                ));
                continue;
            }

            // 400 Bad Request - request problem, retry/switching credentials is meaningless
            if status.as_u16() == 400 {
                Self::emit_attempt(
                    sink, attempt, ctx.id, endpoint_name, Some(400),
                    outcome::BAD_REQUEST, Some(&body), attempt_start,
                );
                anyhow::bail!("{} API requestfailed: {} {}", api_type, status, body);
            }

            // 401/403 - moremay becredential/Permission issue: counted as a failure and allows failover.
            if matches!(status.as_u16(), 401 | 403) {
                tracing::warn!(
                    "API Request failed (possibly a credential error, try {}/{}): {} {}",
                    attempt + 1,
                    max_retries,
                    status,
                    body
                );
                Self::emit_attempt(
                    sink, attempt, ctx.id, endpoint_name, Some(status.as_u16()),
                    outcome::AUTH_FAILED, Some(&body), attempt_start,
                );

                // token invalidated by the upstream: try first force-refresh, each credential gets only one chance
                if endpoint.is_bearer_token_invalid(&body) && !force_refreshed.contains(&ctx.id) {
                    force_refreshed.insert(ctx.id);
                    tracing::info!("credential #{} token Suspected to be invalidated by upstream; tries a forced refresh.", ctx.id);
                    if self.token_manager.force_refresh_token_for(ctx.id).await.is_ok() {
                        tracing::info!("credential #{} token Forced refresh succeeded; retries the request.", ctx.id);
                        continue;
                    }
                    tracing::warn!("credential #{} token Forced refresh failed; counted as a failure.", ctx.id);
                }

                let has_available = self.token_manager.report_failure(ctx.id);
                if !has_available {
                    anyhow::bail!(
                        "{} API Request failed (all credentials exhausted).: {} {}",
                        api_type,
                        status,
                        body
                    );
                }

                last_error = Some(anyhow::anyhow!(
                    "{} API requestfailed: {} {}",
                    api_type,
                    status,
                    body
                ));
                continue;
            }

            // 429 + suspicious activity = account level temporary throttle
            // Only the current credential is targeted; failover to other credentials recovers immediately (controlled by a config switch).
            if status.as_u16() == 429
                && self.token_manager.get_account_throttle_failover()
                && endpoint.is_account_throttled(&body)
            {
                let cooldown_secs = self
                    .token_manager
                    .get_account_throttle_cooldown_secs()
                    .max(1);
                let cooldown = std::time::Duration::from_secs(cooldown_secs);
                tracing::warn!(
                    "API Request failed (account level throttle, credential #{} cooldown {}s and switch,attempt {}/{}): {}",
                    ctx.id,
                    cooldown_secs,
                    attempt + 1,
                    max_retries,
                    body
                );

                let remaining = self.token_manager.report_account_throttled(ctx.id, cooldown);
                Self::emit_attempt(
                    sink, attempt, ctx.id, endpoint_name, Some(429),
                    outcome::ACCOUNT_THROTTLED, Some(&body), attempt_start,
                );
                last_error = Some(anyhow::anyhow!(
                    "{} API Request failed (account level throttle, credential #{} cooled down {} minutes): {} {}",
                    api_type,
                    ctx.id,
                    cooldown_secs / 60,
                    status,
                    body
                ));

                if remaining == 0 {
                    anyhow::bail!(
                        "{} API Request failed: all credentials are in account throttle cooldown or disabled state.\
                         upstreamforcredential #{} ofaccounttriggerdone \"suspicious activity\" temporarywhenthrottle,\
                         suggest:(1) add moredifferent AWS accountofcredential;\
                         (2) Lower the cooldown duration or manually clear the cooldown in the admin panel to retry;\
                         (3) commit AWS Support appeal to unban the account. The original response: {} {}",
                        api_type,
                        ctx.id,
                        status,
                        body
                    );
                }
                continue;
            }

            // client request format error (messages the array violates the protocol): the root cause is the caller, retrying is meaningless.
            // upstream often 5xx return, must be below"transient errorretry"Intercepts before the branch; otherwise it would be treated as
            // upstreamfailure retry max_retries times, amplifying a bad request into many. 503(503 storm).
            // Terminates directly: no retry, no credential switch, not counted as a credential failure.
            if endpoint.is_client_validation_error(&body) {
                tracing::warn!(
                    "API Request failed (client request format error, no retry).: {} {}",
                    status,
                    body
                );
                Self::emit_attempt(
                    sink, attempt, ctx.id, endpoint_name, Some(status.as_u16()),
                    outcome::BAD_REQUEST, Some(&body), attempt_start,
                );
                anyhow::bail!("{} API requestfailed: {} {}", api_type, status, body);
            }

            // 524 / gateway timeout: upstream edge layer timeout; continuing to retry within this request usually only
            // amplify the client wait time and Claude end Retrying rounds; returns fast so the client next call
            // heavynewestablish connection.
            if status.as_u16() == 524 || endpoint.is_gateway_timeout(&body) {
                tracing::warn!(
                    "API Request failed (upstream gateway timeout, no retry).: {} {}",
                    status,
                    body
                );
                Self::emit_attempt(
                    sink,
                    attempt,
                    ctx.id,
                    endpoint_name,
                    Some(status.as_u16()),
                    outcome::TRANSIENT,
                    Some(&body),
                    attempt_start,
                );
                anyhow::bail!("{} API requestfailed: {} {}", api_type, status, body);
            }

            // 429/408/5xx - Transient upstream error: retries but does not disable or switch the credential.
            // (avoid 429 high traffic / 502 high load and similar transient errors locking all credentials)
            if matches!(status.as_u16(), 408 | 429) || status.is_server_error() {
                tracing::warn!(
                    "API Request failed (upstream transient error, try {}/{}): {} {}",
                    attempt + 1,
                    max_retries,
                    status,
                    body
                );
                Self::emit_attempt(
                    sink, attempt, ctx.id, endpoint_name, Some(status.as_u16()),
                    outcome::TRANSIENT, Some(&body), attempt_start,
                );
                last_error = Some(anyhow::anyhow!(
                    "{} API requestfailed: {} {}",
                    api_type,
                    status,
                    body
                ));
                if attempt + 1 < max_retries {
                    // 429 Throttling uses a longer backoff to give account quota time to recover;408/5xx still use the general fast backoff
                    let delay = if status.as_u16() == 429 {
                        Self::retry_delay_throttle(attempt)
                    } else {
                        Self::retry_delay(attempt)
                    };
                    sleep(delay).await;
                }
                continue;
            }

            // other 4xx - usuallyasrequest/Config issue: returns directly, not counted as a credential failure.
            if status.is_client_error() {
                Self::emit_attempt(
                    sink, attempt, ctx.id, endpoint_name, Some(status.as_u16()),
                    outcome::BAD_REQUEST, Some(&body), attempt_start,
                );
                anyhow::bail!("{} API requestfailed: {} {}", api_type, status, body);
            }

            // Fallback: treat as a retryable transient error (does not switch credentials).
            tracing::warn!(
                "API Request failed (unknown error, try {}/{}): {} {}",
                attempt + 1,
                max_retries,
                status,
                body
            );
            Self::emit_attempt(
                sink, attempt, ctx.id, endpoint_name, Some(status.as_u16()),
                outcome::UNKNOWN, Some(&body), attempt_start,
            );
            last_error = Some(anyhow::anyhow!(
                "{} API requestfailed: {} {}",
                api_type,
                status,
                body
            ));
            if attempt + 1 < max_retries {
                sleep(Self::retry_delay(attempt)).await;
            }
        }

        // all retries failed
        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!(
                "{} API Request failed: the maximum retry count has been reached ({}times)",
                api_type,
                max_retries
            )
        }))
    }

    /// toward trace sink report one hop result (sink as None whennoneoverhead)
    #[allow(clippy::too_many_arguments)]
    fn emit_attempt(
        sink: Option<&dyn TraceSink>,
        attempt: usize,
        credential_id: u64,
        endpoint: &str,
        http_status: Option<u16>,
        outcome: &str,
        error_body: Option<&str>,
        started: Instant,
    ) {
        let Some(sink) = sink else { return };
        sink.on_attempt(TraceAttempt {
            attempt: attempt as u32,
            credential_id,
            endpoint: endpoint.to_string(),
            http_status,
            outcome: outcome.to_string(),
            error_snippet: error_body.and_then(truncate_snippet),
            duration_ms: started.elapsed().as_millis() as u64,
        });
    }

    /// Extracts model info from the request body.
    ///
    /// try to parse JSON request body,extract conversationState.currentMessage.userInputMessage.modelId
    fn extract_model_from_request(request_body: &str) -> Option<String> {
        use serde_json::Value;

        let json: Value = serde_json::from_str(request_body).ok()?;

        json.get("conversationState")?
            .get("currentMessage")?
            .get("userInputMessage")?
            .get("modelId")?
            .as_str()
            .map(|s| s.to_string())
    }

    fn retry_delay(attempt: usize) -> Duration {
        // exponential backoff + A small jitter, avoiding amplifying faults when upstream jitters.
        const BASE_MS: u64 = 200;
        const MAX_MS: u64 = 2_000;
        let exp = BASE_MS.saturating_mul(2u64.saturating_pow(attempt.min(6) as u32));
        let backoff = exp.min(MAX_MS);
        let jitter_max = (backoff / 4).max(1);
        let jitter = fastrand::u64(0..=jitter_max);
        Duration::from_millis(backoff.saturating_add(jitter))
    }

    /// 429 Throttle specific backoff: longer than the general backoff.
    ///
    /// upstream 429(SERVICE_REQUEST_RATE_EXCEEDED) is account level rate quota exhaustion, needing a longer
    /// recovers over time; use a general ≤2s Fast backoff only makes the request repeatedly hit the wall and keep hitting the ceiling before quota recovers.
    /// here base 1s, cap 8s, leaving a recovery window for the account quota.
    fn retry_delay_throttle(attempt: usize) -> Duration {
        const BASE_MS: u64 = 1_000;
        const MAX_MS: u64 = 8_000;
        let exp = BASE_MS.saturating_mul(2u64.saturating_pow(attempt.min(6) as u32));
        let backoff = exp.min(MAX_MS);
        let jitter_max = (backoff / 4).max(1);
        let jitter = fastrand::u64(0..=jitter_max);
        Duration::from_millis(backoff.saturating_add(jitter))
    }
}
