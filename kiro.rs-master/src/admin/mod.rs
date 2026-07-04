//! Admin API module
//!
//! Provides credential management and monitoring features. HTTP API
//!
//! # feature
//! - query all credential statuses
//! - enable/disablecredential
//! - modify the credential priority
//! - reset the failure count
//! - query the credential balance
//!
//! # use
//! ```ignore
//! let admin_service = AdminService::new(token_manager.clone(), endpoint_names);
//! let admin_state = AdminState::new(admin_api_key, admin_service);
//! let admin_router = create_admin_router(admin_state);
//! ```

mod error;
mod handlers;
mod middleware;
pub mod proxy_pool;
mod router;
mod service;
pub mod types;
mod binary_update;
pub mod client_keys;
pub mod groups;
pub mod usage_stats;
pub mod trace_db;

pub use client_keys::ClientKeyManager;
pub use groups::GroupManager;
pub use middleware::AdminState;
pub use router::create_admin_router;
pub use service::AdminService;
pub use usage_stats::{UsageAggregator, UsageRecorder};
pub use trace_db::{SharedTraceStore, TraceStore};
