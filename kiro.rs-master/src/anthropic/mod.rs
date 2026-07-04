//! Anthropic API compatibleservicemodule
//!
//! provide with Anthropic Claude API compatible HTTP serviceendpoint.
//!
//! # supportofendpoint
//!
//! ## standardendpoint (/v1)
//! - `GET /v1/models` - get the available model list
//! - `POST /v1/messages` - create a message (conversation)
//! - `POST /v1/messages/count_tokens` - compute token count
//!
//! ## Claude Code compatibleendpoint (/cc/v1)
//! - `POST /cc/v1/messages` - Creates a message (a streaming response waits contextUsageEvent afterthen send message_start, ensure input_tokens accurate)
//! - `POST /cc/v1/messages/count_tokens` - compute token count(with /v1 same)
//!
//! # useexample
//! ```rust,ignore
//! use kiro_rs::anthropic;
//!
//! let app = anthropic::create_router("your-api-key");
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
//! axum::serve(listener, app).await?;
//! ```

mod converter;
mod handlers;
mod middleware;
pub mod cache_metering;
mod router;
pub mod stream;
pub mod types;
mod websearch;
mod websearch_loop;

// `create_router_with_provider` Is a public extension point (allows external custom provider constructroute),
// projectinsidedefaultgo `create_router_with_shared_key`, so it does not trigger this function itself.
#[allow(unused_imports)]
pub use router::create_router_with_provider;
pub use router::create_router;
