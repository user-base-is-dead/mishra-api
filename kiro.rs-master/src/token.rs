//! Token compute module
//!
//! provide text token the count computation feature.
//!
//! # compute rule
//! - non western characters: each counts 4.5 itemcharactersinglebit
//! - western characters: each counts 1 itemcharactersinglebit
//! - 4 itemcharactersinglebit = 1 token(round to nearest)

use crate::anthropic::types::{
    CountTokensRequest, CountTokensResponse, Message, SystemMessage, Tool,
};
use crate::http_client::{ProxyConfig, build_client};
use crate::model::config::TlsBackend;
use std::sync::OnceLock;

/// Count Tokens API config
#[derive(Clone, Default)]
pub struct CountTokensConfig {
    /// external count_tokens API address
    pub api_url: Option<String>,
    /// count_tokens API key
    pub api_key: Option<String>,
    /// count_tokens API authtype("x-api-key" or "bearer")
    pub auth_type: String,
    /// proxy config
    pub proxy: Option<ProxyConfig>,

    pub tls_backend: TlsBackend,
}

/// global configstore
static COUNT_TOKENS_CONFIG: OnceLock<CountTokensConfig> = OnceLock::new();

/// initialize count_tokens config
///
/// Should be called once at application startup.
pub fn init_config(config: CountTokensConfig) {
    let _ = COUNT_TOKENS_CONFIG.set(config);
}

/// get config
fn get_config() -> Option<&'static CountTokensConfig> {
    COUNT_TOKENS_CONFIG.get()
}

/// Determines whether the character is non Western.
///
/// western characters include:
/// - ASCII character (U+0000..U+007F)
/// - Latin letter extension (U+0080..U+024F)
/// - Latin extended additional (U+1E00..U+1EFF)
///
/// return true Indicates the character is non Western (such as Chinese, Japanese, Korean, Arabic, and so on).
fn is_non_western_char(c: char) -> bool {
    !matches!(c,
        // basic ASCII
        '\u{0000}'..='\u{007F}' |
        // Latin letter extension-A (Latin Extended-A)
        '\u{0080}'..='\u{00FF}' |
        // Latin letter extension-B (Latin Extended-B)
        '\u{0100}'..='\u{024F}' |
        // Latin extended additional (Latin Extended Additional)
        '\u{1E00}'..='\u{1EFF}' |
        // Latin letter extension-C/D/E
        '\u{2C60}'..='\u{2C7F}' |
        '\u{A720}'..='\u{A7FF}' |
        '\u{AB30}'..='\u{AB6F}'
    )
}

/// computetextof token count
///
/// # compute rule
/// - non western characters: each counts 4.5 itemcharactersinglebit
/// - western characters: each counts 1 itemcharactersinglebit
/// - 4 itemcharactersinglebit = 1 token(round to nearest)
/// ```
pub fn count_tokens(text: &str) -> u64 {
    // println!("text: {}", text);

    let char_units: f64 = text
        .chars()
        .map(|c| if is_non_western_char(c) { 4.0 } else { 1.0 })
        .sum();

    let tokens = char_units / 4.0;

    let acc_token = if tokens < 100.0 {
        tokens * 1.5
    } else if tokens < 200.0 {
        tokens * 1.3
    } else if tokens < 300.0 {
        tokens * 1.25
    } else if tokens < 800.0 {
        tokens * 1.2
    } else {
        tokens * 1.0
    } as u64;

    // println!("tokens: {}, acc_tokens: {}", tokens, acc_token);
    acc_token
}

/// estimate the request input tokens
///
/// prioritycall remote API, falls back to local computation on failure.
pub(crate) fn count_all_tokens(
    model: String,
    system: Option<Vec<SystemMessage>>,
    messages: Vec<Message>,
    tools: Option<Vec<Tool>>,
) -> u64 {
    // check whether remote is configured API
    if let Some(config) = get_config() {
        if let Some(api_url) = &config.api_url {
            // attemptcall remote API
            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(call_remote_count_tokens(
                    api_url, config, model, &system, &messages, &tools,
                ))
            });

            match result {
                Ok(tokens) => {
                    tracing::debug!("remote count_tokens API return: {}", tokens);
                    return tokens;
                }
                Err(e) => {
                    tracing::warn!("remote count_tokens API The call failed; falls back to local computation.: {}", e);
                }
            }
        }
    }

    // local compute
    count_all_tokens_local(system, messages, tools)
}

/// call remote count_tokens API
async fn call_remote_count_tokens(
    api_url: &str,
    config: &CountTokensConfig,
    model: String,
    system: &Option<Vec<SystemMessage>>,
    messages: &Vec<Message>,
    tools: &Option<Vec<Tool>>,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let client = build_client(config.proxy.as_ref(), 300, config.tls_backend)?;

    // build requestbody
    let request = CountTokensRequest {
        model: model, // model namenameused for token compute
        messages: messages.clone(),
        system: system.clone(),
        tools: tools.clone(),
    };

    // build request
    let mut req_builder = client.post(api_url);

    // setauthhead
    if let Some(api_key) = &config.api_key {
        if config.auth_type == "bearer" {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        } else {
            req_builder = req_builder.header("x-api-key", api_key);
        }
    }

    // send request
    let response = req_builder
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("API returnerrorstate: {}", response.status()).into());
    }

    let result: CountTokensResponse = response.json().await?;
    Ok(result.input_tokens as u64)
}

/// locally compute the request input tokens
fn count_all_tokens_local(
    system: Option<Vec<SystemMessage>>,
    messages: Vec<Message>,
    tools: Option<Vec<Tool>>,
) -> u64 {
    let mut total = 0;

    // system message
    if let Some(ref system) = system {
        for msg in system {
            total += count_tokens(&msg.text);
        }
    }

    // user message
    for msg in &messages {
        if let serde_json::Value::String(s) = &msg.content {
            total += count_tokens(s);
        } else if let serde_json::Value::Array(arr) = &msg.content {
            for item in arr {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    total += count_tokens(text);
                }
            }
        }
    }

    // tool definition
    if let Some(ref tools) = tools {
        for tool in tools {
            total += count_tokens(&tool.name);
            total += count_tokens(&tool.description);
            let input_schema_json = serde_json::to_string(&tool.input_schema).unwrap_or_default();
            total += count_tokens(&input_schema_json);
        }
    }

    total.max(1)
}

/// estimateoutput tokens
pub(crate) fn estimate_output_tokens(content: &[serde_json::Value]) -> i32 {
    let mut total = 0;

    for block in content {
        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
            total += count_tokens(text) as i32;
        }
        if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
            total += count_tokens(thinking) as i32;
        }
        if block.get("type").and_then(|v| v.as_str()) == Some("redacted_thinking") {
            total += 8;
        }
        if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
            // tool calloverhead
            if let Some(input) = block.get("input") {
                let input_str = serde_json::to_string(input).unwrap_or_default();
                total += count_tokens(&input_str) as i32;
            }
        }
    }

    total.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn estimate_output_tokens_counts_thinking_blocks() {
        let with_thinking = estimate_output_tokens(&[json!({
            "type": "thinking",
            "thinking": "needcount inoutput token"
        })]);
        let text_only = estimate_output_tokens(&[json!({
            "type": "text",
            "text": ""
        })]);

        assert!(with_thinking > text_only);
    }

    #[test]
    fn estimate_output_tokens_counts_redacted_thinking() {
        let tokens = estimate_output_tokens(&[json!({
            "type": "redacted_thinking",
            "data": "encrypted"
        })]);

        assert!(tokens >= 8);
    }
}
