//! Anthropic API typedefine

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// === errorresponse ===

/// API errorresponse
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

/// error detail
#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

impl ErrorResponse {
    /// create a new error response
    pub fn new(error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ErrorDetail {
                error_type: error_type.into(),
                message: message.into(),
            },
        }
    }

    /// create an authentication error response
    pub fn authentication_error() -> Self {
        Self::new("authentication_error", "Invalid API key")
    }
}

// === Models endpoint type ===

/// model info
#[derive(Debug, Serialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
    pub display_name: String,
    #[serde(rename = "type")]
    pub model_type: String,
    pub max_tokens: i32,
}

/// modellistresponse
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<Model>,
}

// === Messages endpoint type ===

/// maximumthinking budget tokens
const MAX_BUDGET_TOKENS: i32 = 24576;

/// Thinking config
#[derive(Debug, Deserialize, Clone)]
pub struct Thinking {
    #[serde(rename = "type")]
    pub thinking_type: String,
    #[serde(
        default = "default_budget_tokens",
        deserialize_with = "deserialize_budget_tokens"
    )]
    pub budget_tokens: i32,
}

impl Thinking {
    /// iswhetherenabledone thinking(enabled or adaptive)
    pub fn is_enabled(&self) -> bool {
        self.thinking_type == "enabled" || self.thinking_type == "adaptive"
    }
}

fn default_budget_tokens() -> i32 {
    20000
}
fn deserialize_budget_tokens<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = i32::deserialize(deserializer)?;
    Ok(value.min(MAX_BUDGET_TOKENS))
}

/// OutputConfig config
#[derive(Debug, Deserialize, Clone)]
pub struct OutputConfig {
    #[serde(default = "default_effort")]
    pub effort: String,
}

fn default_effort() -> String {
    "high".to_string()
}

/// Claude Code in request metadata
#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    /// user ID, format like: user_xxx_account__session_0b4445e1-f5be-49e1-87ce-62bbc28ad705
    pub user_id: Option<String>,
}

/// Messages request body
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MessagesRequest {
    pub model: String,
    pub max_tokens: i32,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default, deserialize_with = "deserialize_system")]
    pub system: Option<Vec<SystemMessage>>,
    pub tools: Option<Vec<Tool>>,
    pub tool_choice: Option<serde_json::Value>,
    pub thinking: Option<Thinking>,
    pub output_config: Option<OutputConfig>,
    /// Claude Code in request metadata, contains session info
    pub metadata: Option<Metadata>,
}

/// deserialize system field, supports string or array format.
fn deserialize_system<'de, D>(deserializer: D) -> Result<Option<Vec<SystemMessage>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // create a visitor to handle string or array
    struct SystemVisitor;

    impl<'de> serde::de::Visitor<'de> for SystemVisitor {
        type Value = Option<Vec<SystemMessage>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or an array of system messages")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(vec![SystemMessage {
                text: value.to_string(),
                cache_control: None,
            }]))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut messages = Vec::new();
            while let Some(msg) = seq.next_element()? {
                messages.push(msg);
            }
            Ok(if messages.is_empty() {
                None
            } else {
                Some(messages)
            })
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            serde::de::Deserialize::deserialize(deserializer)
        }
    }

    deserializer.deserialize_any(SystemVisitor)
}

/// message
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub role: String,
    /// can be string or ContentBlock array
    pub content: serde_json::Value,
}

/// system message
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemMessage {
    pub text: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// cache_control config
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub cache_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

/// tool definition
///
/// supports two formats:
/// 1. normaltool:{ name, description, input_schema }
/// 2. WebSearch tool:{ type: "web_search_20250305", name: "web_search", max_uses: 8 }
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tool {
    /// tooltype,if "web_search_20250305"(optional,only WebSearch tool)
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<String>,
    /// tool name
    #[serde(default)]
    pub name: String,
    /// Tool description (required for ordinary tools,WebSearch tooloptional)
    #[serde(default)]
    pub description: String,
    /// input parameter schema(required for an ordinary tool,WebSearch the tool has no such field)
    ///
    /// use `BTreeMap` rather than `HashMap`:key Iterates stably in lexicographic order to ensure serialization.
    /// the output is reproducible. this is for prompt cache critical——tool the signature participates in the cache prefix
    /// fingerprint,iftop level key order jitter will cause subsequent system/messages the breakpoint chain fails.
    #[serde(default)]
    pub input_schema: BTreeMap<String, serde_json::Value>,
    /// maximum usage count (only WebSearch tool)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i32>,
    /// cache control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// content block
#[derive(Debug, Deserialize, Serialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ImageSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// imagedatasource
#[derive(Debug, Deserialize, Serialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

// === Count Tokens endpoint type ===

/// Token count request
#[derive(Debug, Serialize, Deserialize)]
pub struct CountTokensRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_system"
    )]
    pub system: Option<Vec<SystemMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

/// Token count response
#[derive(Debug, Serialize, Deserialize)]
pub struct CountTokensResponse {
    pub input_tokens: i32,
}
