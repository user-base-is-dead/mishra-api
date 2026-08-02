mod admin;
mod admin_ui;
mod anthropic;
mod common;
mod http_client;
mod image_resize;
mod kiro;
mod model;
pub mod token;

use std::collections::HashMap;
use std::sync::Arc;

use clap::Parser;
use kiro::endpoint::{CliEndpoint, IdeEndpoint, KiroEndpoint};
use kiro::model::credentials::{CredentialsConfig, KiroCredentials};
use kiro::provider::KiroProvider;
use kiro::token_manager::MultiTokenManager;
use model::arg::Args;
use model::config::Config;

#[tokio::main]
async fn main() {
    // parse the command line arguments
    let args = Args::parse();

    // initializelog
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // parse config/credential path
    let config_path = args
        .config
        .unwrap_or_else(|| Config::default_config_path().to_string());
    let credentials_path = args
        .credentials
        .unwrap_or_else(|| KiroCredentials::default_credentials_path().to_string());

    // Auto initializes when the file does not exist (Docker first deployment friendly)
    ensure_config_files(&config_path, &credentials_path);

    // load config
    let config = Config::load(&config_path).unwrap_or_else(|e| {
        tracing::error!("load configfailed: {}", e);
        std::process::exit(1);
    });

    // Loads credentials (supports single object or array format).
    let credentials_config = CredentialsConfig::load(&credentials_path).unwrap_or_else(|e| {
        tracing::error!("load credentialfailed: {}", e);
        std::process::exit(1);
    });

    // Determines whether it is the multi credential format (for write back after refresh).
    let is_multiple_format = credentials_config.is_multiple();

    // Converts to a credential list sorted by priority.
    let mut credentials_list = credentials_config.into_sorted_credentials();

    // check KIRO_API_KEY environment variable, auto create API Key credential
    if let Ok(kiro_api_key) = std::env::var("KIRO_API_KEY") {
        if kiro_api_key.is_empty() {
            tracing::warn!("KIRO_API_KEY The environment variable is set but empty; treated as not configured.");
        } else {
            tracing::info!("detected KIRO_API_KEY environment variable, add API Key credential (highest priority)");
            let api_key_cred = KiroCredentials {
                kiro_api_key: Some(kiro_api_key),
                auth_method: Some("api_key".to_string()),
                priority: 0,
                ..Default::default()
            };
            credentials_list.insert(0, api_key_cred);
        }
    }

    tracing::info!("loaded {} itemcredentialconfig", credentials_list.len());

    // Shows only safe metadata to avoid leaking in logs. token / client_secret
    let first_credentials = credentials_list.first().cloned().unwrap_or_default();
    tracing::debug!(
        id = ?first_credentials.id,
        email = ?first_credentials.email,
        auth_method = ?first_credentials.auth_method,
        priority = first_credentials.priority,
        endpoint = ?first_credentials.endpoint,
        "alreadyselect primarycredential"
    );

    // apiKey only used at first startup bootstrap numberoneentryclient Key;
    // subsequent /v1 authentication all goes through the client Key system.adminApiKey is still the admin panel login key.
    let bootstrap_key = config.api_key.clone().filter(|k| !k.trim().is_empty());

    // buildproxy config
    let proxy_config = config.proxy_url.as_ref().map(|url| {
        let mut proxy = http_client::ProxyConfig::new(url);
        if let (Some(username), Some(password)) = (&config.proxy_username, &config.proxy_password) {
            proxy = proxy.with_auth(username, password);
        }
        proxy
    });

    if proxy_config.is_some() {
        tracing::info!("configured HTTP proxy: {}", config.proxy_url.as_ref().unwrap());
    }

    // start Kiro IDE Version auto fetch: pulls from the official metadata endpoint. currentRelease,
    // used forstreaming endpoint User-Agent(replaces the hardcoded version); falls back on failure. config.kiroVersion.
    kiro::kiro_version::spawn_refresher(
        proxy_config.clone(),
        config.tls_backend,
        std::time::Duration::from_secs(12 * 3600),
    );

    // build the endpoint registry
    let mut endpoints: HashMap<String, Arc<dyn KiroEndpoint>> = HashMap::new();
    {
        let ide = IdeEndpoint::new();
        endpoints.insert(ide.name().to_string(), Arc::new(ide));
        let cli = CliEndpoint::new();
        endpoints.insert(cli.name().to_string(), Arc::new(cli));
    }

    // validate the default endpoint exists
    if !endpoints.contains_key(&config.default_endpoint) {
        tracing::error!("default endpoint \"{}\" not registered", config.default_endpoint);
        std::process::exit(1);
    }

    // Validates that the endpoints declared by all credentials are registered.
    for cred in &credentials_list {
        let name = cred.endpoint.as_deref().unwrap_or(&config.default_endpoint);
        if !endpoints.contains_key(name) {
            tracing::error!(
                "credential id={:?} an unknown endpoint was specified \"{}\"(registered: {:?})",
                cred.id,
                name,
                endpoints.keys().collect::<Vec<_>>()
            );
            std::process::exit(1);
        }
    }

    let endpoint_names: Vec<String> = endpoints.keys().cloned().collect();

    // create MultiTokenManager and KiroProvider
    let token_manager = MultiTokenManager::new(
        config.clone(),
        credentials_list,
        proxy_config.clone(),
        Some(credentials_path.into()),
        is_multiple_format,
    )
    .unwrap_or_else(|e| {
        tracing::error!("create Token managerfailed: {}", e);
        std::process::exit(1);
    });
    let token_manager = Arc::new(token_manager);
    let kiro_provider = KiroProvider::with_proxy(
        token_manager.clone(),
        proxy_config.clone(),
        endpoints,
        config.default_endpoint.clone(),
    );

    // initialize count_tokens config
    token::init_config(token::CountTokensConfig {
        api_url: config.count_tokens_api_url.clone(),
        api_key: config.count_tokens_api_key.clone(),
        auth_type: config.count_tokens_auth_type.clone(),
        proxy: proxy_config,
        tls_backend: config.tls_backend,
    });

    // client Key manager + usagerecordcomponent + aggregator (in the same directory as the credential file).
    let cache_dir = token_manager
        .cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let client_keys_path = admin::client_keys::default_path_in(&cache_dir);
    let client_key_manager = std::sync::Arc::new(
        admin::ClientKeyManager::load(&client_keys_path).unwrap_or_else(|e| {
            tracing::warn!("loadclient Key failed ({}): {}", client_keys_path.display(), e);
            admin::ClientKeyManager::new()
        }),
    );
    let usage_recorder = std::sync::Arc::new(admin::UsageRecorder::with_retention(
        cache_dir.clone(),
        config.usage_log_retention_days as i64,
    ));
    let usage_aggregator = std::sync::Arc::new(admin::UsageAggregator::new());
    usage_aggregator.rebuild_from_logs(&cache_dir);

    // The account group registry (persisted to groups.json).
    // On startup, if the file does not exist, creates it for the first time and puts existing credentials / client Key of groups reverse migrate the field into it,
    // Ensures that after existing users upgrade, all used groups are automatically registered and do not disappear because of this change.
    let groups_path = admin::groups::default_path_in(&cache_dir);
    let group_manager = std::sync::Arc::new(
        admin::GroupManager::load(&groups_path).unwrap_or_else(|e| {
            tracing::warn!("failed to load the group registry ({}): {}", groups_path.display(), e);
            admin::GroupManager::new()
        }),
    );
    {
        let mut all_used: Vec<String> = token_manager.list_credential_groups();
        all_used.extend(client_key_manager.used_group_names());
        let added = group_manager.bootstrap_from_existing(all_used);
        if added > 0 {
            tracing::info!("group registry: auto migrate {} itemalreadyusegroup", added);
        }
    }

    // request trace storage (SQLite,traces.db). failure is not fatal:trace unavailable but the service is normal.
    let trace_store: Option<admin::SharedTraceStore> = match admin::TraceStore::open(
        cache_dir.join("traces.db"),
        config.trace_enabled,
        config.trace_retention_days,
    ) {
        Ok(s) => Some(std::sync::Arc::new(s)),
        Err(e) => {
            tracing::warn!("open traces.db failed; request tracing is unavailable.: {}", e);
            None
        }
    };

    // periodically clean up expired after startup usage_log and trace record
    {
        let recorder = usage_recorder.clone();
        let trace_store = trace_store.clone();
        tokio::spawn(async move {
            let day = std::time::Duration::from_secs(24 * 3600);
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            loop {
                recorder.cleanup_old_logs();
                if let Some(ts) = &trace_store {
                    ts.cleanup();
                }
                tokio::time::sleep(day).await;
            }
        });
    }

    // each startup idempotently ensures config.apiKey correspondofsystem Key exists (cannot be deleted / cannot be rotated).
    // When an old deployment upgrades, it takes the existing apiKey complete into system Key, ensuring the root key is always usable for /v1 traffic.
    if let Some(initial_key) = bootstrap_key.as_ref() {
        client_key_manager.ensure_system_key(
            "Default Key".to_string(),
            Some("Auto-imported from config.json apiKey (system key)".to_string()),
            initial_key.clone(),
        );
    }

    // CacheMeter: simulate Anthropic cache,metering cache_read/creation token the in process component.
    // persistto cache_dir/cache_metering.json, auto loads non expired entries at startup.
    let cache_meter = std::sync::Arc::new(anthropic::cache_metering::CacheMeter::new(Some(
        cache_dir.join("cache_metering.json"),
    )));
    cache_meter.clone().spawn_background();

    let anthropic_app = anthropic::create_router(
        Some(kiro_provider),
        config.extract_thinking,
        Some(client_key_manager.clone()),
        Some(usage_recorder.clone()),
        Some(usage_aggregator.clone()),
        Some(cache_meter.clone()),
        trace_store.clone(),
    );

    // build Admin API route (configured with non empty adminApiKey enabled when)
    // Safety check: an empty string is treated as not configured, preventing an empty key bypass auth
    let app = if let Some(admin_key) = &config.admin_api_key {
        if admin_key.trim().is_empty() {
            tracing::warn!("admin_api_key configis empty,Admin API not enabled");
            anthropic_app
        } else {
            // Admin the query needs a definite store;traces.db Uses an in-memory fallback when opening fails (effective only in this process).
            let admin_trace_store = trace_store.clone().unwrap_or_else(|| {
                std::sync::Arc::new(
                    admin::TraceStore::open_in_memory()
                        .expect("memory trace store initializefailed"),
                )
            });
            let admin_service =
                admin::AdminService::new(token_manager.clone(), endpoint_names.clone())
                    .with_log_governance(
                        Some(admin_trace_store.clone()),
                        Some(usage_recorder.clone()),
                    );
            let admin_state = admin::AdminState::new(
                admin_key,
                admin_service,
                client_key_manager.clone(),
                usage_aggregator.clone(),
                admin_trace_store,
                group_manager.clone(),
            );

            // Starts the balance background refresh scheduler (every 5 once per minute, with the cache TTL aligned)
            admin_state
                .service
                .start_balance_refresher(std::time::Duration::from_secs(300));

            // Starts the proxy pool health check scheduler (every 5 minutesonce)
            admin_state
                .service
                .start_proxy_health_checker(std::time::Duration::from_secs(300));

            // Starts the auto update scheduler: checks the local time once a minute, and when reached update_auto_apply_time
            // and enabled update_auto_apply performs one update; otherwise silently waits.
            admin_state.service.start_auto_update_scheduler();

            let admin_app = admin::create_admin_router(admin_state);

            // create Admin UI route
            let admin_ui_app = admin_ui::create_admin_ui_router();

            tracing::info!("Admin API enabled");
            tracing::info!("Admin UI enabled: /admin");
            anthropic_app
                .nest("/api/admin", admin_app)
                .nest("/admin", admin_ui_app)
        }
    } else {
        anthropic_app
    };

    // startservicecomponent
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("start Anthropic API endpoint: {}", addr);
    tracing::info!("available API:");
    tracing::info!("  GET  /v1/models");
    tracing::info!("  POST /v1/messages");
    tracing::info!("  POST /v1/messages/count_tokens");
    tracing::info!("Admin API:");
    tracing::info!("  GET  /api/admin/credentials");
    tracing::info!("  POST /api/admin/credentials/:index/disabled");
    tracing::info!("  POST /api/admin/credentials/:index/priority");
    tracing::info!("  POST /api/admin/credentials/:index/reset");
    tracing::info!("  GET  /api/admin/credentials/:index/balance");
    tracing::info!("Admin UI:");
    tracing::info!("  GET  /admin");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Initializes the config when the file does not exist./credential file
///
/// - `config.json`:writewith randomness `apiKey`(auto imported as the first client on first startup) Key)/ `adminApiKey`(admin panel login key)
///   the minimal default configuration;`host` set to `0.0.0.0` to adapt to container scenarios, the port/The default endpoint and other fields use the code defaults.
/// - `credentials.json`:writeemptyarray `[]`, to facilitate subsequently through Admin UI addcredential.
///
/// Any step failing only prints a warning and does not interrupt startup; later `Config::load` / `CredentialsConfig::load`
/// Still handled by the existing logic (exits on failure).
fn ensure_config_files(config_path: &str, credentials_path: &str) {
    let config_p = std::path::Path::new(config_path);
    if !config_p.exists() {
        if let Some(parent) = config_p.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("failed to create the configuration directory {}: {}", parent.display(), e);
                }
            }
        }
        let api_key = format!("sk-kiro-rs-{}", random_token(24));
        let admin_api_key = "admin".to_string();
        let default = serde_json::json!({
            "host": "0.0.0.0",
            "port": 8990,
            "apiKey": api_key,
            "adminApiKey": admin_api_key,
            "region": "us-east-1",
            "tlsBackend": "rustls",
            "defaultEndpoint": "ide"
        });
        match serde_json::to_string_pretty(&default)
            .map_err(anyhow::Error::from)
            .and_then(|s| std::fs::write(config_p, s).map_err(anyhow::Error::from))
        {
            Ok(_) => {
                tracing::info!("the default configuration has been generated: {}", config_p.display());
                tracing::info!("  apiKey      = {}(on first startup it is auto imported as the first client Key)", api_key);
                tracing::info!("  adminApiKey = {}(admin panel login key)", admin_api_key);
                tracing::info!("Please keep the above key safe; it can be changed in the config file.");
            }
            Err(e) => tracing::warn!("failed to write the default configuration {}: {}", config_p.display(), e),
        }
    }

    let cred_p = std::path::Path::new(credentials_path);
    if !cred_p.exists() {
        if let Some(parent) = cred_p.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("failed to create the credential directory {}: {}", parent.display(), e);
                }
            }
        }
        if let Err(e) = std::fs::write(cred_p, "[]\n") {
            tracing::warn!("failed to write the empty credential file {}: {}", cred_p.display(), e);
        } else {
            tracing::info!("an empty credential file has been generated: {}(can via Admin UI addcredential)", cred_p.display());
        }
    }
}

/// generate a segment of length `len` alphanumeric random string, used for the default API Key
fn random_token(len: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    (0..len)
        .map(|_| {
            let idx = fastrand::usize(..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
