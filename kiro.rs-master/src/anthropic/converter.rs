//! Anthropic → Kiro protocolconvertcomponent
//!
//! responsible for Anthropic API convert the request format to Kiro API requestformat

use std::collections::HashMap;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::kiro::model::requests::conversation::{
    AssistantMessage, ConversationState, CurrentMessage, HistoryAssistantMessage,
    HistoryUserMessage, KiroImage, Message, UserInputMessage, UserInputMessageContext, UserMessage,
};
use crate::kiro::model::requests::kiro::{AdditionalModelRequestFields, KiroOutputConfig};
use crate::kiro::model::requests::tool::{
    InputSchema, Tool, ToolResult, ToolSpecification, ToolUseEntry,
};

use super::types::{ContentBlock, ImageSource, MessagesRequest};

use crate::image_resize::{ResizeConfig, maybe_shrink_image};

/// normalize JSON Schema, fix MCP Common type issues in tool definitions.
/// normalize JSON Schema, fixes common type issues in tool definitions.
///
/// issueroot cause:Claude Code / MCP tool definitionuse JSON Schema Draft 2020-12 syntax (`$schema`,
/// `exclusiveMinimum` ascountcharacteretc.),kiro CLI endpoint only accept Draft 07 format,
/// a non compliant field will cause ValidationException "Improperly formed request.".
fn normalize_json_schema(schema: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(mut obj) = schema else {
        return serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": true
        });
    };

    // remove $schema(kiro API does not accept this field, and Draft 2020-12 declaration triggers validation failure)
    obj.remove("$schema");

    // type(must be a string)
    if !obj.get("type").and_then(|v| v.as_str()).is_some_and(|s| !s.is_empty()) {
        obj.insert("type".to_string(), serde_json::Value::String("object".to_string()));
    }

    // properties(must be object); recursively normalize each property sub schema
    match obj.remove("properties") {
        Some(serde_json::Value::Object(props)) => {
            let normalized: serde_json::Map<String, serde_json::Value> = props
                .into_iter()
                .map(|(k, v)| (k, normalize_property_schema(v)))
                .collect();
            obj.insert("properties".to_string(), serde_json::Value::Object(normalized));
        }
        _ => { obj.insert("properties".to_string(), serde_json::Value::Object(serde_json::Map::new())); }
    }

    // required(must be string array)
    let required = match obj.remove("required") {
        Some(serde_json::Value::Array(arr)) => serde_json::Value::Array(
            arr.into_iter()
                .filter_map(|v| v.as_str().map(|s| serde_json::Value::String(s.to_string())))
                .collect(),
        ),
        _ => serde_json::Value::Array(Vec::new()),
    };
    obj.insert("required".to_string(), required);

    // additionalProperties(allow bool or object,otherby true handle)
    match obj.get("additionalProperties") {
        Some(serde_json::Value::Bool(_)) | Some(serde_json::Value::Object(_)) => {}
        _ => { obj.insert("additionalProperties".to_string(), serde_json::Value::Bool(true)); }
    }

    serde_json::Value::Object(obj)
}

/// normalize property levelsub schema(nontop level inputSchema)
///
/// handle Draft 2020-12 specific field, to make it compatible Draft 07:
/// - remove `$schema`
/// - `exclusiveMinimum`/`exclusiveMaximum` ascountcharacterwhen(Draft 2019-09+)remove(Draft 07 only support bool)
/// - `maximum`/`minimum` exceeds i32 remove when out of range (partial AWS validator does not accept oversized integer constraints)
fn normalize_property_schema(schema: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(mut obj) = schema else {
        return schema;
    };

    obj.remove("$schema");

    // exclusiveMinimum/exclusiveMaximum:Draft 2019-09+ ascountcharacter,Draft 07 as bool; remove the numeric form
    if obj.get("exclusiveMinimum").and_then(|v| v.as_f64()).is_some() {
        obj.remove("exclusiveMinimum");
    }
    if obj.get("exclusiveMaximum").and_then(|v| v.as_f64()).is_some() {
        obj.remove("exclusiveMaximum");
    }

    // maximum/minimum exceeds i64::MAX or is JavaScript MAX_SAFE_INTEGER (9007199254740991) remove when
    for key in &["maximum", "minimum"] {
        if let Some(v) = obj.get(*key).and_then(|v| v.as_f64()) {
            if v > 2_147_483_647.0 || v < -2_147_483_648.0 {
                obj.remove(*key);
            }
        }
    }

    // recursionhandlenested properties
    if let Some(serde_json::Value::Object(props)) = obj.remove("properties") {
        let normalized: serde_json::Map<String, serde_json::Value> = props
            .into_iter()
            .map(|(k, v)| (k, normalize_property_schema(v)))
            .collect();
        obj.insert("properties".to_string(), serde_json::Value::Object(normalized));
    }

    // recursionhandle items(arrayelement schema)
    if let Some(items) = obj.remove("items") {
        obj.insert("items".to_string(), normalize_property_schema(items));
    }

    serde_json::Value::Object(obj)
}

/// append to Write tool description endofcontent
const WRITE_TOOL_DESCRIPTION_SUFFIX: &str = "- IMPORTANT: If the content to write exceeds 150 lines, you MUST only write the first 50 lines using this tool, then use `Edit` tool to append the remaining content in chunks of no more than 50 lines each. If needed, leave a unique placeholder to help append content. Do NOT attempt to write all content at once.";

/// append to Edit tool description endofcontent
const EDIT_TOOL_DESCRIPTION_SUFFIX: &str = "- IMPORTANT: If the `new_string` content exceeds 50 lines, you MUST split it into multiple Edit calls, each replacing no more than 50 lines at a time. If used to append content, leave a unique placeholder to help append content. On the final chunk, do NOT include the placeholder.";

/// The chunked write strategy appended to the system prompt.
const SYSTEM_CHUNKED_POLICY: &str = "\
When the Write or Edit tool has content size limits, always comply silently. \
Never suggest bypassing these limits via alternative tools. \
Never ask the user whether to switch approaches. \
Complete all chunked operations without commentary.";

/// modelmapping:will Anthropic model namemappingto Kiro model ID
/// strictly compare the version number
pub fn map_model(model: &str) -> Option<String> {
    let model_lower = model.to_lowercase();

    if model_lower.contains("sonnet") {
        if model_lower.contains("4-8") || model_lower.contains("4.8") {
            Some("claude-sonnet-4.8".to_string())
        } else if model_lower.contains("4-6") || model_lower.contains("4.6") {
            Some("claude-sonnet-4.6".to_string())
        } else if model_lower.contains("4-5") || model_lower.contains("4.5") {
            Some("claude-sonnet-4.5".to_string())
        } else {
            None
        }
    } else if model_lower.contains("opus") {
        if model_lower.contains("4-8") || model_lower.contains("4.8") {
            Some("claude-opus-4.8".to_string())
        } else if model_lower.contains("4-7") || model_lower.contains("4.7") {
            Some("claude-opus-4.7".to_string())
        } else if model_lower.contains("4-5") || model_lower.contains("4.5") {
            Some("claude-opus-4.5".to_string())
        } else if model_lower.contains("4-6") || model_lower.contains("4.6") {
            Some("claude-opus-4.6".to_string())
        } else {
            None
        }
    } else if model_lower.contains("haiku") {
        Some("claude-haiku-4.5".to_string())
    } else {
        None
    }
}

/// Returns the corresponding context window size based on the model name.
///
/// reuse `map_model` mapping logic, ensuring the window size judgment is consistent with the model mapping.
/// Kiro at 2026-03-24 will Opus 4.6 and Sonnet 4.6 upgrade to 1M context.
/// 4.7 / 4.8 same 1M
pub fn get_context_window_size(model: &str) -> i32 {
    match map_model(model) {
        Some(mapped)
            if mapped == "claude-sonnet-4.6"
                || mapped == "claude-sonnet-4.8"
                || mapped == "claude-opus-4.6"
                || mapped == "claude-opus-4.7"
                || mapped == "claude-opus-4.8" =>
        {
            1_000_000
        }
        _ => 200_000,
    }
}

/// Whether this request should use `additionalModelRequestFields.output_config`.
///
/// The field is currently only known to be accepted by the Opus 4.6 adaptive-thinking path.
/// Sending it to other models causes upstream 400 responses such as
/// `additionalModelRequestFields is not supported for this model`.
fn should_emit_output_config(req: &MessagesRequest, model_id: &str) -> bool {
    model_id == "claude-opus-4.6"
        && req
            .thinking
            .as_ref()
            .is_some_and(|t| t.thinking_type == "adaptive")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffortTier {
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl EffortTier {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" | "x-high" | "x_high" => Some(Self::XHigh),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }
}

fn normalize_effort_for_model(model_id: &str, raw_effort: &str) -> Option<String> {
    let trimmed = raw_effort.trim();
    if trimmed.is_empty() {
        return None;
    }

    let requested = match EffortTier::parse(trimmed) {
        Some(tier) => tier,
        None => {
            tracing::debug!(
                model_id = %model_id,
                effort = %trimmed,
                fallback_effort = EffortTier::High.as_str(),
                "falling back unsupported output_config.effort"
            );
            return Some(EffortTier::High.as_str().to_string());
        }
    };

    // `xhigh` is a newer effort tier. Known older effort-capable models reject
    // it with `Invalid additionalModelRequestFields`, so map to the nearest
    // lower tier instead of failing the request. Unknown/future models keep
    // recognized values intact to avoid maintaining a brittle full allow-list.
    let normalized = if requested == EffortTier::XHigh && !model_supports_xhigh_effort(model_id) {
        EffortTier::High
    } else {
        requested
    };
    if normalized != requested || normalized.as_str() != trimmed {
        tracing::debug!(
            model_id = %model_id,
            effort = %trimmed,
            normalized_effort = normalized.as_str(),
            "normalized output_config.effort for model"
        );
    }

    Some(normalized.as_str().to_string())
}

fn model_supports_xhigh_effort(model_id: &str) -> bool {
    let model = model_id.to_ascii_lowercase();

    // Anthropic documents xhigh for Opus 4.7/4.8, Fable 5, and Mythos 5.
    if model.contains("opus-4.7")
        || model.contains("opus-4.8")
        || model.contains("fable-5")
        || model.contains("mythos-5")
        || model.contains("claude-5")
    {
        return true;
    }

    // Known Kiro/Claude model ids that predate xhigh. Keep this as a compact
    // deny-list, not a full capability matrix.
    !matches!(
        model.as_str(),
        "claude-opus-4.6"
            | "claude-sonnet-4.6"
            | "claude-opus-4.5"
            | "claude-sonnet-4.5"
            | "claude-haiku-4.5"
    )
}

fn build_additional_model_request_fields(
    req: &MessagesRequest,
    model_id: &str,
) -> Option<AdditionalModelRequestFields> {
    let output_config = if should_emit_output_config(req, model_id) {
        req.output_config.as_ref().and_then(|oc| {
            normalize_effort_for_model(model_id, &oc.effort)
                .map(|effort| KiroOutputConfig { effort })
        })
    } else {
        if let Some(oc) = &req.output_config
            && !oc.effort.trim().is_empty()
        {
            tracing::debug!(
                model_id = %model_id,
                "skipping unsupported additionalModelRequestFields.output_config for model"
            );
        }
        None
    };

    output_config.map(|output_config| AdditionalModelRequestFields {
        output_config: Some(output_config),
    })
}

/// convertresult
#[derive(Debug)]
pub struct ConversionResult {
    /// convertafterof Kiro request
    pub conversation_state: ConversationState,
    /// tool name mapping (short name → original name), non empty only when an overlong tool name exists.
    pub tool_name_map: HashMap<String, String>,
    /// All tool names declared by this request (original client name).used for `<invoke>` disaster fallback for text fault tolerance:
    /// Only when the synthesized tool name is in this set is the literal allowed to be `<invoke>` retrieveintostructtransform tool_use;
    /// Otherwise emits it as plain text, avoiding executing a tool call shown in the body as a real command.
    pub known_tool_names: std::collections::HashSet<String>,
    /// Additional model request fields (including `output_config.effort`), translated from the
    /// `output_config` field of the client's Anthropic request. Not sent when empty.
    pub additional_model_request_fields: Option<AdditionalModelRequestFields>,
}

/// converterror
#[derive(Debug)]
pub enum ConversionError {
    UnsupportedModel(String),
    EmptyMessages,
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversionError::UnsupportedModel(model) => write!(f, "modelnot supported: {}", model),
            ConversionError::EmptyMessages => write!(f, "messagelistis empty"),
        }
    }
}

impl std::error::Error for ConversionError {}

/// from metadata.user_id extract from session UUID
///
/// supporttwo kindsformat:
/// 1. stringformat: user_xxx_account__session_0b4445e1-f5be-49e1-87ce-62bbc28ad705
/// 2. JSON format: {"device_id":"...","account_uuid":"...","session_id":"UUID"}
///
/// extract session UUID as conversationId
fn extract_session_id(user_id: &str) -> Option<String> {
    // try first JSON parse
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(user_id) {
        if let Some(session_id) = json.get("session_id").and_then(|v| v.as_str()) {
            if is_valid_uuid(session_id) {
                return Some(session_id.to_string());
            }
        }
    }

    // fall back to the string format: find "session_" afterfaceofcontent
    if let Some(pos) = user_id.find("session_") {
        let session_part = &user_id[pos + 8..]; // "session_" length is 8
        if session_part.len() >= 36 {
            let uuid_str = &session_part[..36];
            if is_valid_uuid(uuid_str) {
                return Some(uuid_str.to_string());
            }
        }
    }
    None
}

/// simpleverify UUID format (36 character, contains 4 itemconnectcharacters)
fn is_valid_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4
}

/// Collects all tool names used in history messages.
fn collect_history_tool_names(history: &[Message]) -> Vec<String> {
    let mut tool_names = Vec::new();

    for msg in history {
        if let Message::Assistant(assistant_msg) = msg {
            if let Some(ref tool_uses) = assistant_msg.assistant_response_message.tool_uses {
                for tool_use in tool_uses {
                    if !tool_names.contains(&tool_use.name) {
                        tool_names.push(tool_use.name.clone());
                    }
                }
            }
        }
    }

    tool_names
}

/// used in history but not in tools Creates placeholder definitions for tools in the list.
/// Kiro API Requirement: tools referenced in history messages must be in currentMessage.tools hasdefine
fn create_placeholder_tool(name: &str) -> Tool {
    Tool {
        tool_specification: ToolSpecification {
            name: name.to_string(),
            description: "Tool used in conversation history".to_string(),
            input_schema: InputSchema::from_json(serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": true
            })),
        },
    }
}

/// will Anthropic requestconvert to Kiro request
pub fn convert_request(req: &MessagesRequest) -> Result<ConversionResult, ConversionError> {
    // 1. mappingmodel
    let model_id = map_model(&req.model)
        .ok_or_else(|| ConversionError::UnsupportedModel(req.model.clone()))?;

    // 2. checkmessagelist
    if req.messages.is_empty() {
        return Err(ConversionError::EmptyMessages);
    }

    // 2.5. preprocess prefill:ifendis assistant, silently discards and truncates to the last one. user
    // Claude 4.x deprecated assistant prefill,Kiro API alsonot supported
    let messages: &[_] = if req.messages.last().is_some_and(|m| m.role != "user") {
        tracing::info!("detectedend assistant message (prefill),silently discard");
        let last_user_idx = req
            .messages
            .iter()
            .rposition(|m| m.role == "user")
            .ok_or(ConversionError::EmptyMessages)?;
        &req.messages[..=last_user_idx]
    } else {
        &req.messages
    };

    // 3. generatesession ID and proxy ID
    // prefer from metadata.user_id extract from session UUID as conversationId
    let conversation_id = req
        .metadata
        .as_ref()
        .and_then(|m| m.user_id.as_ref())
        .and_then(|user_id| extract_session_id(user_id))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let agent_continuation_id = Uuid::new_v4().to_string();

    // 4. determinetrigger type
    let chat_trigger_type = determine_chat_trigger_type(req);

    // 5. process the last message as current_message(after prefill preprocess, the tail must be user)
    let last_message = messages.last().unwrap();
    let (text_content, images, tool_results) = process_message_content(&last_message.content)?;

    // 6. Converts tool definitions (overlong names are automatically shortened and the mapping recorded).
    let mut tool_name_map = HashMap::new();
    let mut tools = convert_tools(&req.tools, &mut tool_name_map);

    // Collects all tool names declared by this request (original client name),provide `<invoke>` fault tolerant tool table validation.
    let mut known_tool_names: std::collections::HashSet<String> = req
        .tools
        .as_ref()
        .map(|ts| ts.iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default();
    // suggest3 fix: oversized tool name (>63) will be shorten into a short name sent to upstream; what the model returns is also the short name.
    // tool_name_map of key exactly these short names, added together, avoiding the legitimate case of an overlong name tool invoke bymissed retrieval.
    for short in tool_name_map.keys() {
        known_tool_names.insert(short.clone());
    }

    // 7. Builds history messages (must be built first so the tools used in history can be collected).
    let mut history = build_history(req, messages, &model_id, &mut tool_name_map)?;

    // 8. verifyandfilter tool_use/tool_result pair
    // removeorphan tool_result(nonecorrespondof tool_use)
    // at the same time return the orphaned tool_use_id set, used for subsequent cleanup
    let (validated_tool_results, orphaned_tool_use_ids) =
        validate_tool_pairing(&history, &tool_results);

    // 9. remove the orphaned ones from history tool_use(Kiro API require tool_use must havecorrespondof tool_result)
    remove_orphaned_tool_uses(&mut history, &orphaned_tool_use_ids);

    // 10. Collects tool names used in history and generates placeholder definitions for missing tools.
    // Kiro API Requirement: tools referenced in history messages must be in tools listhasdefine
    // note:Kiro Tool name matching ignores case, so the comparison here must also ignore case.
    let history_tool_names = collect_history_tool_names(&history);
    let existing_tool_names: std::collections::HashSet<_> = tools
        .iter()
        .map(|t| t.tool_specification.name.to_lowercase())
        .collect();

    for tool_name in history_tool_names {
        if !existing_tool_names.contains(&tool_name.to_lowercase()) {
            tools.push(create_placeholder_tool(&tool_name));
        }
    }

    // 11. build UserInputMessageContext
    let mut context = UserInputMessageContext::new();
    if !tools.is_empty() {
        context = context.with_tools(tools);
    }
    if !validated_tool_results.is_empty() {
        context = context.with_tool_results(validated_tool_results);
    }

    // 12. buildcurrent message
    // Keeps text content; even if there are tool results, does not drop user text.
    let content = text_content;

    let mut user_input = UserInputMessage::new(content, &model_id)
        .with_context(context)
        .with_origin("AI_EDITOR");

    if !images.is_empty() {
        user_input = user_input.with_images(images);
    }

    let current_message = CurrentMessage::new(user_input);

    // 13. build ConversationState
    let conversation_state = ConversationState::new(conversation_id)
        .with_agent_continuation_id(agent_continuation_id)
        .with_agent_task_type("vibe")
        .with_chat_trigger_type(chat_trigger_type)
        .with_current_message(current_message)
        .with_history(history);

    if !tool_name_map.is_empty() {
        tracing::info!(
            "tool namemapping: {} oversized names have been shortened",
            tool_name_map.len()
        );
    }

    // 14. Extract effort into AdditionalModelRequestFields only for models that accept it.
    //
    // The system-prompt thinking prefix remains available for every thinking mode. The real
    // wire field is narrower: newer/non-adaptive models reject it with
    // `additionalModelRequestFields is not supported for this model`, so keep the field opt-in
    // by upstream model capability rather than by the mere presence of client output_config.
    let additional_model_request_fields = build_additional_model_request_fields(req, &model_id);

    Ok(ConversionResult {
        conversation_state,
        tool_name_map,
        known_tool_names,
        additional_model_request_fields,
    })
}

/// determine the chat trigger type
/// "AUTO" the mode may cause 400 Bad Request error
fn determine_chat_trigger_type(_req: &MessagesRequest) -> String {
    "MANUAL".to_string()
}

/// Processes message content, extracting text, images, and tool results.
fn process_message_content(
    content: &serde_json::Value,
) -> Result<(String, Vec<KiroImage>, Vec<ToolResult>), ConversionError> {
    process_message_content_dedup(content, None)
}

/// Same as `process_message_content`, but when `dedup` is `Some` it deduplicates images by SHA256:
/// the same image (identical base64) recurring across history is kept only on first sight and later replaced with placeholder text,
/// avoiding the same screenshot being re-sent as base64 over multiple turns and burning tokens.
fn process_message_content_dedup(
    content: &serde_json::Value,
    mut dedup: Option<&mut std::collections::HashSet<String>>,
) -> Result<(String, Vec<KiroImage>, Vec<ToolResult>), ConversionError> {
    let mut text_parts = Vec::new();
    let mut images = Vec::new();
    let mut tool_results = Vec::new();

    match content {
        serde_json::Value::String(s) => {
            text_parts.push(s.clone());
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Ok(block) = serde_json::from_value::<ContentBlock>(item.clone()) {
                    match block.block_type.as_str() {
                        "text" => {
                            if let Some(text) = block.text {
                                text_parts.push(text);
                            }
                        }
                        "image" => {
                            if let Some(source) = block.source
                                && let Some(placeholder) =
                                    extract_kiro_image(&source, &mut dedup, &mut images)
                            {
                                text_parts.push(placeholder);
                            }
                        }
                        "tool_result" => {
                            if let Some(tool_use_id) = block.tool_use_id {
                                let result_content =
                                    extract_tool_result_content(&block.content, &mut dedup, &mut images);
                                let is_error = block.is_error.unwrap_or(false);

                                let mut result = if is_error {
                                    ToolResult::error(&tool_use_id, result_content)
                                } else {
                                    ToolResult::success(&tool_use_id, result_content)
                                };
                                result.status =
                                    Some(if is_error { "error" } else { "success" }.to_string());

                                tool_results.push(result);
                            }
                        }
                        "tool_use" => {
                            // tool_use in assistant processed in the message, ignored here
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }

    Ok((text_parts.join("\n"), images, tool_results))
}

/// from media_type fetchimageformat
fn get_image_format(media_type: &str) -> Option<String> {
    match media_type {
        "image/jpeg" => Some("jpeg".to_string()),
        "image/png" => Some("png".to_string()),
        "image/gif" => Some("gif".to_string()),
        "image/webp" => Some("webp".to_string()),
        _ => None,
    }
}

/// Converts an image block's source into a `KiroImage` and pushes it onto the top-level `images`.
///
/// Reuses the same conversion chain as top-level images (format validation + SHA256 dedup + resize + `from_base64`),
/// so an image inside a tool_result is lifted into the top-level images field the same way.
/// Returns `Some(placeholder)` when history dedup hit and the image was omitted; `None` when it was lifted or the format is unsupported.
fn extract_kiro_image(
    source: &ImageSource,
    dedup: &mut Option<&mut std::collections::HashSet<String>>,
    images: &mut Vec<KiroImage>,
) -> Option<String> {
    let format = get_image_format(&source.media_type)?;
    // History dedup: an already-seen image omits its base64 and returns placeholder text
    if let Some(seen) = dedup.as_deref_mut() {
        let mut hasher = Sha256::new();
        hasher.update(source.data.as_bytes());
        let digest = format!("{:x}", hasher.finalize());
        if !seen.insert(digest) {
            return Some("[image omitted: identical to an earlier screenshot]".to_string());
        }
    }
    let cfg = ResizeConfig::from_env();
    let processed = maybe_shrink_image(cfg, &format, &source.data);
    images.push(KiroImage::from_base64(processed.format, processed.data_base64));
    None
}

/// extract the tool result content
///
/// Text elements remain as tool_result placeholder text; blocks with `type=="image"` are extracted into a `KiroImage`
/// and lifted to the top-level `images` (Amazon Q's `ToolResult` has no image field, so images can only go through the top-level channel).
/// If a tool_result has only images and no text, the placeholder text "[image attached]" is used.
fn extract_tool_result_content(
    content: &Option<serde_json::Value>,
    dedup: &mut Option<&mut std::collections::HashSet<String>>,
    images: &mut Vec<KiroImage>,
) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => {
            let mut parts = Vec::new();
            let mut had_image = false;
            for item in arr {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    parts.push(text.to_string());
                } else if item.get("type").and_then(|v| v.as_str()) == Some("image")
                    && let Ok(block) = serde_json::from_value::<ContentBlock>(item.clone())
                    && let Some(source) = block.source
                {
                    had_image = true;
                    if let Some(placeholder) = extract_kiro_image(&source, dedup, images) {
                        parts.push(placeholder);
                    }
                }
            }
            if parts.is_empty() && had_image {
                "[image attached]".to_string()
            } else {
                parts.join("\n")
            }
        }
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

/// verifyandfilter tool_use/tool_result pair
///
/// collectall tool_use_id, verify tool_result iswhether it matches
/// silently skip the orphaned tool_use and tool_result, output a warning log
///
/// # Arguments
/// * `history` - history messagereference
/// * `tool_results` - current messagein tool_result list
///
/// # Returns
/// tuple:(after validation and filtering tool_result list, orphan tool_use_id set)
fn validate_tool_pairing(
    history: &[Message],
    tool_results: &[ToolResult],
) -> (Vec<ToolResult>, std::collections::HashSet<String>) {
    use std::collections::HashSet;

    // 1. collect all in history tool_use_id
    let mut all_tool_use_ids: HashSet<String> = HashSet::new();
    // 2. collect those already in history tool_result of tool_use_id
    let mut history_tool_result_ids: HashSet<String> = HashSet::new();

    for msg in history {
        match msg {
            Message::Assistant(assistant_msg) => {
                if let Some(ref tool_uses) = assistant_msg.assistant_response_message.tool_uses {
                    for tool_use in tool_uses {
                        all_tool_use_ids.insert(tool_use.tool_use_id.clone());
                    }
                }
            }
            Message::User(user_msg) => {
                // collect history user messagein tool_results
                for result in &user_msg
                    .user_input_message
                    .user_input_message_context
                    .tool_results
                {
                    history_tool_result_ids.insert(result.tool_use_id.clone());
                }
            }
        }
    }

    // 3. compute the truly unpaired ones tool_use_ids(excludes those already paired in history)
    let mut unpaired_tool_use_ids: HashSet<String> = all_tool_use_ids
        .difference(&history_tool_result_ids)
        .cloned()
        .collect();

    // 4. filter and validate the current message tool_results
    let mut filtered_results = Vec::new();

    for result in tool_results {
        if unpaired_tool_use_ids.contains(&result.tool_use_id) {
            // pairsuccess
            filtered_results.push(result.clone());
            unpaired_tool_use_ids.remove(&result.tool_use_id);
        } else if all_tool_use_ids.contains(&result.tool_use_id) {
            // tool_use It exists but was already paired in history; this is a duplicate. tool_result
            tracing::warn!(
                "skipduplicate tool_result: this tool_use already paired in history,tool_use_id={}",
                result.tool_use_id
            );
        } else {
            // orphan tool_result - findnottocorrespondof tool_use
            tracing::warn!(
                "skiporphan tool_result: cannot find the corresponding tool_use,tool_use_id={}",
                result.tool_use_id
            );
        }
    }

    // 5. detect the truly orphaned tool_use(has tool_use but is present in neither history nor the current message. tool_result)
    for orphaned_id in &unpaired_tool_use_ids {
        tracing::warn!(
            "detectedorphan tool_use: cannot find the corresponding tool_result, will be removed from history,tool_use_id={}",
            orphaned_id
        );
    }

    (filtered_results, unpaired_tool_use_ids)
}

/// Removes orphaned ones from history messages. tool_use
///
/// Kiro API requireeachitem tool_use must havecorrespondof tool_result, otherwisereturn 400 Bad Request.
/// this function iterates over history assistant messages, remove those without a corresponding tool_result of tool_use.
///
/// # Arguments
/// * `history` - mutable history message list
/// * `orphaned_ids` - the orphaned ones that need removal tool_use_id set
fn remove_orphaned_tool_uses(
    history: &mut [Message],
    orphaned_ids: &std::collections::HashSet<String>,
) {
    if orphaned_ids.is_empty() {
        return;
    }

    for msg in history.iter_mut() {
        if let Message::Assistant(assistant_msg) = msg {
            if let Some(ref mut tool_uses) = assistant_msg.assistant_response_message.tool_uses {
                let original_len = tool_uses.len();
                tool_uses.retain(|tu| !orphaned_ids.contains(&tu.tool_use_id));

                // If empty after removal, sets to None
                if tool_uses.is_empty() {
                    assistant_msg.assistant_response_message.tool_uses = None;
                } else if tool_uses.len() != original_len {
                    tracing::debug!(
                        "from assistant messageinremovedone {} itemorphan tool_use",
                        original_len - tool_uses.len()
                    );
                }
            }
        }
    }
}

/// Kiro API tool name maximum length limit
const TOOL_NAME_MAX_LEN: usize = 63;

/// Generates a deterministic short name: truncated prefix. + "_" + 8 bit SHA256 hex
fn shorten_tool_name(name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    let hash_hex = format!("{:x}", hasher.finalize());
    let hash_suffix = &hash_hex[..8];
    // 54 prefix + 1 underscore + 8 hash = 63
    let prefix_max = TOOL_NAME_MAX_LEN - 1 - 8;
    let prefix = match name.char_indices().nth(prefix_max) {
        Some((idx, _)) => &name[..idx],
        None => name,
    };
    format!("{}_{}", prefix, hash_suffix)
}

/// If the name is overlong, shortens it and records the mapping (short → original)
fn map_tool_name(name: &str, tool_name_map: &mut HashMap<String, String>) -> String {
    if name.len() <= TOOL_NAME_MAX_LEN {
        return name.to_string();
    }
    let short = shorten_tool_name(name);
    tool_name_map.insert(short.clone(), name.to_string());
    short
}

/// converttool definition
fn convert_tools(tools: &Option<Vec<super::types::Tool>>, tool_name_map: &mut HashMap<String, String>) -> Vec<Tool> {
    let Some(tools) = tools else {
        return Vec::new();
    };

    tools
        .iter()
        .map(|t| {
            let mut description = t.description.clone();

            // for Write/Edit The tool appends a custom description suffix.
            let suffix = match t.name.as_str() {
                "Write" => WRITE_TOOL_DESCRIPTION_SUFFIX,
                "Edit" => EDIT_TOOL_DESCRIPTION_SUFFIX,
                _ => "",
            };
            if !suffix.is_empty() {
                description.push('\n');
                description.push_str(suffix);
            }

            // kiro API Does not accept an empty description; fills a placeholder.
            let description = if description.trim().is_empty() {
                t.name.clone()
            } else {
                description
            };

            // limit the description length to 10000 characters (safe truncation UTF-8,singletimesiterate)
            let description = match description.char_indices().nth(10000) {
                Some((idx, _)) => description[..idx].to_string(),
                None => description,
            };

            Tool {
                tool_specification: ToolSpecification {
                    name: map_tool_name(&t.name, tool_name_map),
                    description,
                    input_schema: InputSchema::from_json(normalize_json_schema(serde_json::json!(t.input_schema))),
                },
            }
        })
        .collect()
}

/// generatethinkingtagprefix
fn generate_thinking_prefix(req: &MessagesRequest, model_id: &str) -> Option<String> {
    if let Some(t) = &req.thinking {
        if t.thinking_type == "enabled" {
            return Some(format!(
                "<thinking_mode>enabled</thinking_mode><max_thinking_length>{}</max_thinking_length>",
                t.budget_tokens
            ));
        } else if t.thinking_type == "adaptive" {
            let effort = req
                .output_config
                .as_ref()
                .and_then(|c| normalize_effort_for_model(model_id, &c.effort))
                .unwrap_or_else(|| "high".to_string());
            return Some(format!(
                "<thinking_mode>adaptive</thinking_mode><thinking_effort>{}</thinking_effort>",
                effort
            ));
        }
    }
    None
}

/// check whether the content already containsthinkingtag
fn has_thinking_tags(content: &str) -> bool {
    content.contains("<thinking_mode>") || content.contains("<max_thinking_length>")
}

/// buildhistory message
///
/// # Arguments
/// * `req` - the original request, used to read `system`,`thinking` etc.configfield
/// * `messages` - after prefill Preprocessed message slices; the end is always user message.
///   note: this slice and `req.messages` cancandifferent(prefill will truncate the trailing assistant message),
///   The caller should always use this parameter rather than `req.messages`.
/// * `model_id` - alreadymappingof Kiro model ID
fn build_history(req: &MessagesRequest, messages: &[super::types::Message], model_id: &str, tool_name_map: &mut HashMap<String, String>) -> Result<Vec<Message>, ConversionError> {
    let mut history = Vec::new();

    // generatethinkingprefix (if needed)
    let thinking_prefix = generate_thinking_prefix(req, model_id);

    // 1. handlesystem message
    if let Some(ref system) = req.system {
        let system_content: String = system
            .iter()
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        if !system_content.is_empty() {
            // Appends the chunked write strategy to the system message.
            let system_content = format!("{}\n{}", system_content, SYSTEM_CHUNKED_POLICY);

            // injectthinkingthe tag to the front of the system message (if needed and not present).
            let final_content = if let Some(ref prefix) = thinking_prefix {
                if !has_thinking_tags(&system_content) {
                    format!("{}\n{}", prefix, system_content)
                } else {
                    system_content
                }
            } else {
                system_content
            };

            // system messageas user + assistant pair
            let user_msg = HistoryUserMessage::new(final_content, model_id);
            history.push(Message::User(user_msg));

            let assistant_msg = HistoryAssistantMessage::new("I will follow these instructions.");
            history.push(Message::Assistant(assistant_msg));
        }
    } else if let Some(ref prefix) = thinking_prefix {
        // no system message but hasthinkingconfig, inserts a new system message.
        let user_msg = HistoryUserMessage::new(prefix.clone(), model_id);
        history.push(Message::User(user_msg));

        let assistant_msg = HistoryAssistantMessage::new("I will follow these instructions.");
        history.push(Message::Assistant(assistant_msg));
    }

    // 2. process the regular message history
    // the last message as currentMessage,notadd to history
    // after prefill preprocessafter,messages the end must beis user, so simply truncating the last one suffices.
    let history_end_index = messages.len().saturating_sub(1);

    // collect and pair messages
    let mut user_buffer: Vec<&super::types::Message> = Vec::new();
    let mut assistant_buffer: Vec<&super::types::Message> = Vec::new();
    // SHA256 dedup set for images spanning the whole history; a repeated image is kept only on first sight
    let mut image_dedup: std::collections::HashSet<String> = std::collections::HashSet::new();

    for i in 0..history_end_index {
        let msg = &messages[i];

        if msg.role == "user" {
            // firsthandleaccumulateof assistant message
            if !assistant_buffer.is_empty() {
                let merged = merge_assistant_messages(&assistant_buffer, tool_name_map)?;
                history.push(Message::Assistant(merged));
                assistant_buffer.clear();
            }
            user_buffer.push(msg);
        } else if msg.role == "assistant" {
            // firsthandleaccumulateof user message
            if !user_buffer.is_empty() {
                let merged_user = merge_user_messages(&user_buffer, model_id, &mut image_dedup)?;
                history.push(Message::User(merged_user));
                user_buffer.clear();
            }
            // accumulate assistant message (supports multiple consecutive)
            assistant_buffer.push(msg);
        }
    }

    // process the tail accumulated assistant message
    if !assistant_buffer.is_empty() {
        let merged = merge_assistant_messages(&assistant_buffer, tool_name_map)?;
        history.push(Message::Assistant(merged));
    }

    // process the trailing orphaned user message
    if !user_buffer.is_empty() {
        let merged_user = merge_user_messages(&user_buffer, model_id, &mut image_dedup)?;
        history.push(Message::User(merged_user));

        // automaticpaira "OK" of assistant response
        let auto_assistant = HistoryAssistantMessage::new("OK");
        history.push(Message::Assistant(auto_assistant));
    }

    Ok(history)
}

/// merge multipleitem user message
fn merge_user_messages(
    messages: &[&super::types::Message],
    model_id: &str,
    dedup: &mut std::collections::HashSet<String>,
) -> Result<HistoryUserMessage, ConversionError> {
    let mut content_parts = Vec::new();
    let mut all_images = Vec::new();
    let mut all_tool_results = Vec::new();

    for msg in messages {
        let (text, images, tool_results) =
            process_message_content_dedup(&msg.content, Some(dedup))?;
        if !text.is_empty() {
            content_parts.push(text);
        }
        all_images.extend(images);
        all_tool_results.extend(tool_results);
    }

    let content = content_parts.join("\n");
    // Keeps text content; even if there are tool results, does not drop user text.
    let mut user_msg = UserMessage::new(&content, model_id);

    if !all_images.is_empty() {
        user_msg = user_msg.with_images(all_images);
    }

    if !all_tool_results.is_empty() {
        let mut ctx = UserInputMessageContext::new();
        ctx = ctx.with_tool_results(all_tool_results);
        user_msg = user_msg.with_context(ctx);
    }

    Ok(HistoryUserMessage {
        user_input_message: user_msg,
    })
}

/// convert assistant message
fn convert_assistant_message(
    msg: &super::types::Message,
    tool_name_map: &mut HashMap<String, String>,
) -> Result<HistoryAssistantMessage, ConversionError> {
    let mut thinking_content = String::new();
    let mut text_content = String::new();
    let mut tool_uses = Vec::new();

    match &msg.content {
        serde_json::Value::String(s) => {
            text_content = s.clone();
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Ok(block) = serde_json::from_value::<ContentBlock>(item.clone()) {
                    match block.block_type.as_str() {
                        "thinking" => {
                            if let Some(thinking) = block.thinking {
                                thinking_content.push_str(&thinking);
                            }
                        }
                        "text" => {
                            if let Some(text) = block.text {
                                text_content.push_str(&text);
                            }
                        }
                        "tool_use" => {
                            if let (Some(id), Some(name)) = (block.id, block.name) {
                                let input = block.input.unwrap_or(serde_json::json!({}));
                                let mapped_name = map_tool_name(&name, tool_name_map);
                                tool_uses.push(ToolUseEntry::new(id, mapped_name).with_input(input));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }

    // combine thinking and text content
    // format: <thinking>thinkingcontent</thinking>\n\ntextcontent
    // note: Kiro API require content the field cannot be empty, when there is only tool_use needed whenplaceholder
    let final_content = if !thinking_content.is_empty() {
        if !text_content.is_empty() {
            format!(
                "<thinking>{}</thinking>\n\n{}",
                thinking_content, text_content
            )
        } else {
            format!("<thinking>{}</thinking>", thinking_content)
        }
    } else if text_content.is_empty() && !tool_uses.is_empty() {
        " ".to_string()
    } else {
        text_content
    };

    let mut assistant = AssistantMessage::new(final_content);
    if !tool_uses.is_empty() {
        assistant = assistant.with_tool_uses(tool_uses);
    }

    Ok(HistoryAssistantMessage {
        assistant_response_message: assistant,
    })
}

/// merge multiple consecutive assistant messageasoneentry
/// Used to handle the consecutive ones produced when the network is unstable. assistant message (Issue #79)
fn merge_assistant_messages(
    messages: &[&super::types::Message],
    tool_name_map: &mut HashMap<String, String>,
) -> Result<HistoryAssistantMessage, ConversionError> {
    assert!(!messages.is_empty());
    if messages.len() == 1 {
        return convert_assistant_message(messages[0], tool_name_map);
    }

    let mut all_tool_uses: Vec<ToolUseEntry> = Vec::new();
    let mut content_parts: Vec<String> = Vec::new();

    for msg in messages {
        let converted = convert_assistant_message(msg, tool_name_map)?;
        let am = converted.assistant_response_message;
        if !am.content.trim().is_empty() {
            content_parts.push(am.content);
        }
        if let Some(tus) = am.tool_uses {
            all_tool_uses.extend(tus);
        }
    }

    let content = if content_parts.is_empty() && !all_tool_uses.is_empty() {
        " ".to_string()
    } else {
        content_parts.join("\n\n")
    };

    let mut assistant = AssistantMessage::new(content);
    if !all_tool_uses.is_empty() {
        assistant = assistant.with_tool_uses(all_tool_uses);
    }
    Ok(HistoryAssistantMessage {
        assistant_response_message: assistant,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_model_sonnet() {
        assert!(
            map_model("claude-sonnet-4-5-20250929")
                .unwrap()
                .contains("sonnet")
        );
        assert!(
            map_model("claude-sonnet-4-6")
                .unwrap()
                .contains("sonnet")
        );
    }

    #[test]
    fn test_map_model_opus() {
        assert!(
            map_model("claude-opus-4-5-20251101")
                .unwrap()
                .contains("opus")
        );
    }

    #[test]
    fn test_map_model_opus_4_7() {
        assert_eq!(
            map_model("claude-opus-4-7"),
            Some("claude-opus-4.7".to_string())
        );
        assert_eq!(
            map_model("claude-opus-4.7-thinking"),
            Some("claude-opus-4.7".to_string())
        );
        assert_eq!(get_context_window_size("claude-opus-4-7"), 1_000_000);
    }

    #[test]
    fn test_map_model_opus_4_8() {
        assert_eq!(
            map_model("claude-opus-4-8"),
            Some("claude-opus-4.8".to_string())
        );
        assert_eq!(
            map_model("claude-opus-4.8-thinking"),
            Some("claude-opus-4.8".to_string())
        );
        assert_eq!(get_context_window_size("claude-opus-4-8"), 1_000_000);
    }

    #[test]
    fn test_map_model_sonnet_4_8() {
        assert_eq!(
            map_model("claude-sonnet-4-8"),
            Some("claude-sonnet-4.8".to_string())
        );
        assert_eq!(
            map_model("claude-sonnet-4.8-thinking"),
            Some("claude-sonnet-4.8".to_string())
        );
        assert_eq!(get_context_window_size("claude-sonnet-4-8"), 1_000_000);
    }

    #[test]
    fn test_map_model_haiku() {
        assert!(
            map_model("claude-haiku-4-20250514")
                .unwrap()
                .contains("haiku")
        );
    }

    #[test]
    fn test_map_model_unsupported() {
        assert!(map_model("gpt-4").is_none());
    }

    #[test]
    fn test_map_model_thinking_suffix_sonnet() {
        // thinking afteraffixshould notimpact sonnet modelmapping
        let result = map_model("claude-sonnet-4-5-20250929-thinking");
        assert_eq!(result, Some("claude-sonnet-4.5".to_string()));
    }

    #[test]
    fn test_map_model_thinking_suffix_opus_4_5() {
        // thinking afteraffixshould notimpact opus 4.5 modelmapping
        let result = map_model("claude-opus-4-5-20251101-thinking");
        assert_eq!(result, Some("claude-opus-4.5".to_string()));
    }

    #[test]
    fn test_map_model_thinking_suffix_opus_4_6() {
        // thinking afteraffixshould notimpact opus 4.6 modelmapping
        let result = map_model("claude-opus-4-6-thinking");
        assert_eq!(result, Some("claude-opus-4.6".to_string()));
    }

    #[test]
    fn test_map_model_thinking_suffix_haiku() {
        // thinking afteraffixshould notimpact haiku modelmapping
        let result = map_model("claude-haiku-4-5-20251001-thinking");
        assert_eq!(result, Some("claude-haiku-4.5".to_string()));
    }

    fn minimal_request_with_output_config(model: &str) -> MessagesRequest {
        minimal_request_with_effort(model, "high")
    }

    fn minimal_request_with_effort(model: &str, effort: &str) -> MessagesRequest {
        use super::super::types::{Message as AnthropicMessage, OutputConfig};

        MessagesRequest {
            model: model.to_string(),
            max_tokens: 1024,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("test"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: Some(OutputConfig {
                effort: effort.to_string(),
            }),
            metadata: None,
        }
    }

    fn minimal_adaptive_thinking_request_with_output_config(model: &str) -> MessagesRequest {
        use super::super::types::Thinking;

        let mut req = minimal_request_with_output_config(model);
        req.thinking = Some(Thinking {
            thinking_type: "adaptive".to_string(),
            budget_tokens: 20000,
        });
        req
    }

    fn minimal_adaptive_thinking_request_with_effort(model: &str, effort: &str) -> MessagesRequest {
        use super::super::types::Thinking;

        let mut req = minimal_request_with_effort(model, effort);
        req.thinking = Some(Thinking {
            thinking_type: "adaptive".to_string(),
            budget_tokens: 20000,
        });
        req
    }

    fn minimal_thinking_request(model: &str, thinking_type: &str) -> MessagesRequest {
        use super::super::types::Thinking;

        let mut req = minimal_request_with_output_config(model);
        req.output_config = None;
        req.thinking = Some(Thinking {
            thinking_type: thinking_type.to_string(),
            budget_tokens: 20000,
        });
        req
    }

    #[test]
    fn test_output_config_does_not_emit_unsupported_additional_fields() {
        let req = minimal_request_with_output_config("claude-sonnet-4-8-thinking");
        let result = convert_request(&req).unwrap();

        assert!(
            result.additional_model_request_fields.is_none(),
            "sonnet 4.8 rejects additionalModelRequestFields even when the client sends output_config"
        );
    }

    #[test]
    fn test_output_config_does_not_emit_for_non_adaptive_opus_4_6() {
        let req = minimal_request_with_output_config("claude-opus-4-6");
        let result = convert_request(&req).unwrap();

        assert!(
            result.additional_model_request_fields.is_none(),
            "opus 4.6 only uses additionalModelRequestFields for adaptive thinking"
        );
    }

    #[test]
    fn test_thinking_does_not_emit_additional_fields_for_sonnet_4_5() {
        let req = minimal_thinking_request("claude-sonnet-4-5-20250929-thinking", "enabled");
        let result = convert_request(&req).unwrap();

        assert!(
            result.additional_model_request_fields.is_none(),
            "sonnet 4.5 rejects additionalModelRequestFields even when thinking is enabled"
        );
    }

    #[test]
    fn test_enabled_thinking_does_not_emit_output_config_for_opus_4_6() {
        let mut req = minimal_request_with_output_config("claude-opus-4-6-thinking");
        req.thinking = minimal_thinking_request("claude-opus-4-6-thinking", "enabled").thinking;
        let result = convert_request(&req).unwrap();

        assert!(
            result.additional_model_request_fields.is_none(),
            "opus 4.6 output_config is only accepted on adaptive thinking requests"
        );
    }

    #[test]
    fn test_output_config_emits_additional_fields_for_opus_4_6() {
        let req = minimal_adaptive_thinking_request_with_output_config("claude-opus-4-6-thinking");
        let result = convert_request(&req).unwrap();

        let fields = result
            .additional_model_request_fields
            .expect("opus 4.6 adaptive thinking should keep the real effort field");
        assert_eq!(
            fields.output_config.unwrap().effort,
            "high",
            "effort should be passed through for the supported model"
        );
    }

    #[test]
    fn test_output_config_downgrades_xhigh_for_opus_4_6() {
        let req =
            minimal_adaptive_thinking_request_with_effort("claude-opus-4-6-thinking", "xhigh");
        let result = convert_request(&req).unwrap();

        let fields = result
            .additional_model_request_fields
            .expect("opus 4.6 adaptive thinking should keep output_config");
        assert_eq!(
            fields.output_config.unwrap().effort,
            "high",
            "opus 4.6 upstream only accepts low/medium/high/max, so xhigh should downgrade"
        );
    }

    #[test]
    fn test_output_config_downgrades_xhigh_for_known_older_models() {
        for model in [
            "claude-opus-4.6",
            "claude-sonnet-4.6",
            "claude-opus-4.5",
            "claude-sonnet-4.5",
            "claude-haiku-4.5",
        ] {
            assert_eq!(
                normalize_effort_for_model(model, "xhigh").as_deref(),
                Some("high"),
                "{model} should not emit xhigh"
            );
        }
    }

    #[test]
    fn test_output_config_preserves_xhigh_for_models_without_known_restriction() {
        assert_eq!(
            normalize_effort_for_model("claude-opus-4.7", "xhigh").as_deref(),
            Some("xhigh"),
            "opus 4.7 supports xhigh"
        );
        assert_eq!(
            normalize_effort_for_model("claude-opus-4.8", "xhigh").as_deref(),
            Some("xhigh"),
            "opus 4.8 supports xhigh"
        );
        assert_eq!(
            normalize_effort_for_model("claude-5", "xhigh").as_deref(),
            Some("xhigh"),
            "claude 5 supports xhigh"
        );
        assert_eq!(
            normalize_effort_for_model("claude-sonnet-5.1", "xhigh").as_deref(),
            Some("xhigh"),
            "future models should not require explicit allow-listing for recognized effort values"
        );
        assert_eq!(
            normalize_effort_for_model("claude-unknown-9", "xhigh").as_deref(),
            Some("xhigh"),
            "unknown future models should keep recognized effort values"
        );
    }

    #[test]
    fn test_output_config_normalizes_effort_case_and_spacing() {
        let req =
            minimal_adaptive_thinking_request_with_effort("claude-opus-4-6-thinking", "  MAX  ");
        let result = convert_request(&req).unwrap();

        let fields = result
            .additional_model_request_fields
            .expect("opus 4.6 adaptive thinking should keep output_config");
        assert_eq!(
            fields.output_config.unwrap().effort,
            "max",
            "effort should be normalized before being sent to upstream"
        );
    }

    #[test]
    fn test_output_config_unknown_effort_falls_back_to_high() {
        let req =
            minimal_adaptive_thinking_request_with_effort("claude-opus-4-6-thinking", "extreme");
        let result = convert_request(&req).unwrap();

        let fields = result
            .additional_model_request_fields
            .expect("opus 4.6 adaptive thinking should keep output_config");
        assert_eq!(
            fields.output_config.unwrap().effort,
            "high",
            "unknown effort values should fall back instead of causing upstream validation errors"
        );
    }

    #[test]
    fn test_determine_chat_trigger_type() {
        // nonetoolreturn when MANUAL
        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };
        assert_eq!(determine_chat_trigger_type(&req), "MANUAL");
    }

    #[test]
    fn test_collect_history_tool_names() {
        use crate::kiro::model::requests::tool::ToolUseEntry;

        // Creates a history message that contains tool usage.
        let mut assistant_msg = AssistantMessage::new("I'll read the file.");
        assistant_msg = assistant_msg.with_tool_uses(vec![
            ToolUseEntry::new("tool-1", "read")
                .with_input(serde_json::json!({"path": "/test.txt"})),
            ToolUseEntry::new("tool-2", "write")
                .with_input(serde_json::json!({"path": "/out.txt"})),
        ]);

        let history = vec![
            Message::User(HistoryUserMessage::new(
                "Read the file",
                "claude-sonnet-4.5",
            )),
            Message::Assistant(HistoryAssistantMessage {
                assistant_response_message: assistant_msg,
            }),
        ];

        let tool_names = collect_history_tool_names(&history);
        assert_eq!(tool_names.len(), 2);
        assert!(tool_names.contains(&"read".to_string()));
        assert!(tool_names.contains(&"write".to_string()));
    }

    #[test]
    fn test_create_placeholder_tool() {
        let tool = create_placeholder_tool("my_custom_tool");

        assert_eq!(tool.tool_specification.name, "my_custom_tool");
        assert!(!tool.tool_specification.description.is_empty());

        // verify JSON serialization is correct
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"name\":\"my_custom_tool\""));
    }

    #[test]
    fn test_shorten_tool_name_deterministic() {
        let long_name = "mcp__some_very_long_server_name__some_very_long_tool_name_that_exceeds_limit";
        assert!(long_name.len() > TOOL_NAME_MAX_LEN);

        let short1 = shorten_tool_name(long_name);
        let short2 = shorten_tool_name(long_name);
        assert_eq!(short1, short2, "The same input should produce the same short name.");
        assert!(short1.len() <= TOOL_NAME_MAX_LEN, "short namelengthshould <= 63, actual {}", short1.len());
    }

    #[test]
    fn test_shorten_tool_name_uniqueness() {
        let name_a = "mcp__server_alpha__tool_name_that_is_very_long_and_exceeds_the_limit_a";
        let name_b = "mcp__server_alpha__tool_name_that_is_very_long_and_exceeds_the_limit_b";
        let short_a = shorten_tool_name(name_a);
        let short_b = shorten_tool_name(name_b);
        assert_ne!(short_a, short_b, "Different inputs should produce different short names.");
    }

    #[test]
    fn test_map_tool_name_short_passthrough() {
        let mut map = HashMap::new();
        let result = map_tool_name("short_name", &mut map);
        assert_eq!(result, "short_name");
        assert!(map.is_empty(), "a short name should not produce a mapping");
    }

    #[test]
    fn test_map_tool_name_long_creates_mapping() {
        let mut map = HashMap::new();
        let long_name = "mcp__plugin_very_long_server_name__extremely_long_tool_name_exceeds_63";
        let result = map_tool_name(long_name, &mut map);
        assert!(result.len() <= TOOL_NAME_MAX_LEN);
        assert_eq!(map.get(&result), Some(&long_name.to_string()));
    }

    #[test]
    fn test_tool_name_mapping_in_convert_request() {
        use super::super::types::{Message as AnthropicMessage, Tool as AnthropicTool};

        let long_tool_name = "mcp__plugin_very_long_server_name__extremely_long_tool_name_exceeds_63";
        assert!(long_tool_name.len() > TOOL_NAME_MAX_LEN);

        let mut schema = std::collections::BTreeMap::new();
        schema.insert("type".to_string(), serde_json::json!("object"));
        schema.insert("properties".to_string(), serde_json::json!({}));

        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!("test"),
                },
            ],
            system: None,
            stream: false,
            tools: Some(vec![AnthropicTool {
                name: long_tool_name.to_string(),
                description: "A test tool".to_string(),
                input_schema: schema,
                tool_type: None,
                max_uses: None,
                cache_control: None,
            }]),
            thinking: None,
            tool_choice: None,
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();

        // should havemapping
        assert_eq!(result.tool_name_map.len(), 1);

        // The value in the mapping should be the original name.
        let (short, original) = result.tool_name_map.iter().next().unwrap();
        assert_eq!(original, long_tool_name);
        assert!(short.len() <= TOOL_NAME_MAX_LEN);

        // Kiro The tool name in the request should be the short name.
        let tools = &result.conversation_state.current_message.user_input_message
            .user_input_message_context.tools;
        assert_eq!(tools[0].tool_specification.name, *short);
    }

    #[test]
    fn test_tool_name_mapping_in_history() {
        use super::super::types::{Message as AnthropicMessage, Tool as AnthropicTool};

        let long_tool_name = "mcp__plugin_very_long_server_name__extremely_long_tool_name_exceeds_63";

        let mut schema = std::collections::BTreeMap::new();
        schema.insert("type".to_string(), serde_json::json!("object"));
        schema.insert("properties".to_string(), serde_json::json!({}));

        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!("use the tool"),
                },
                AnthropicMessage {
                    role: "assistant".to_string(),
                    content: serde_json::json!([
                        {"type": "text", "text": "calling tool"},
                        {"type": "tool_use", "id": "toolu_01", "name": long_tool_name, "input": {}}
                    ]),
                },
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!([
                        {"type": "tool_result", "tool_use_id": "toolu_01", "content": "done"}
                    ]),
                },
            ],
            system: None,
            stream: false,
            tools: Some(vec![AnthropicTool {
                name: long_tool_name.to_string(),
                description: "A test tool".to_string(),
                input_schema: schema,
                tool_type: None,
                max_uses: None,
                cache_control: None,
            }]),
            thinking: None,
            tool_choice: None,
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();
        let short_name = result.tool_name_map.iter().next().unwrap().0.clone();

        // in history assistant message tool_use name alsoshouldthisbymapping
        let history = &result.conversation_state.history;
        let mut found = false;
        for msg in history {
            if let Message::Assistant(a) = msg {
                if let Some(ref tool_uses) = a.assistant_response_message.tool_uses {
                    for tu in tool_uses {
                        if tu.tool_use_id == "toolu_01" {
                            assert_eq!(tu.name, short_name, "in historyof tool_use name should beshort name");
                            found = true;
                        }
                    }
                }
            }
        }
        assert!(found, "should be found in history tool_use");
    }

    #[test]
    fn test_history_tools_added_to_tools_list() {
        use super::super::types::Message as AnthropicMessage;

        // Creates a request with tool usage in history, but tools listis empty
        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!("Read the file"),
                },
                AnthropicMessage {
                    role: "assistant".to_string(),
                    content: serde_json::json!([
                        {"type": "text", "text": "I'll read the file."},
                        {"type": "tool_use", "id": "tool-1", "name": "read", "input": {"path": "/test.txt"}}
                    ]),
                },
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!([
                        {"type": "tool_result", "tool_use_id": "tool-1", "content": "file content"}
                    ]),
                },
            ],
            stream: false,
            system: None,
            tools: None, // no tool definition provided
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();

        // verify tools The list includes placeholder definitions for tools used in history.
        let tools = &result
            .conversation_state
            .current_message
            .user_input_message
            .user_input_message_context
            .tools;

        assert!(!tools.is_empty(), "tools listshould notis empty");
        assert!(
            tools.iter().any(|t| t.tool_specification.name == "read"),
            "tools listshould contain 'read' placeholder definition of the tool"
        );
    }

    #[test]
    fn test_extract_session_id_valid() {
        // testvalid user_id format
        let user_id = "user_0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd_account__session_8bb5523b-ec7c-4540-a9ca-beb6d79f1552";
        let session_id = extract_session_id(user_id);
        assert_eq!(
            session_id,
            Some("8bb5523b-ec7c-4540-a9ca-beb6d79f1552".to_string())
        );
    }

    #[test]
    fn test_extract_session_id_json_format() {
        // test JSON formatted user_id
        let user_id = r#"{"device_id":"0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd","account_uuid":"","session_id":"8bb5523b-ec7c-4540-a9ca-beb6d79f1552"}"#;
        let session_id = extract_session_id(user_id);
        assert_eq!(
            session_id,
            Some("8bb5523b-ec7c-4540-a9ca-beb6d79f1552".to_string())
        );
    }

    #[test]
    fn test_extract_session_id_json_invalid_session() {
        // test JSON format but session_id is nothaseffect UUID
        let user_id = r#"{"device_id":"abc","session_id":"not-a-uuid"}"#;
        let session_id = extract_session_id(user_id);
        assert_eq!(session_id, None);
    }

    #[test]
    fn test_extract_session_id_no_session() {
        // testnone session of user_id
        let user_id = "user_0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd";
        let session_id = extract_session_id(user_id);
        assert_eq!(session_id, None);
    }

    #[test]
    fn test_extract_session_id_invalid_uuid() {
        // testnoneeffectof UUID format
        let user_id = "user_xxx_session_invalid-uuid";
        let session_id = extract_session_id(user_id);
        assert_eq!(session_id, None);
    }

    #[test]
    fn test_convert_request_with_session_metadata() {
        use super::super::types::{Message as AnthropicMessage, Metadata};

        // testcarryhas metadata the request should use session UUID as conversationId
        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: Some(Metadata {
                user_id: Some(
                    "user_0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd_account__session_a0662283-7fd3-4399-a7eb-52b9a717ae88".to_string(),
                ),
            }),
        };

        let result = convert_request(&req).unwrap();
        assert_eq!(
            result.conversation_state.conversation_id,
            "a0662283-7fd3-4399-a7eb-52b9a717ae88"
        );
    }

    #[test]
    fn test_convert_request_without_metadata() {
        use super::super::types::Message as AnthropicMessage;

        // testnone metadata the request should generate a new UUID
        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();
        // validate that what is generated is valid UUID format
        assert_eq!(result.conversation_state.conversation_id.len(), 36);
        assert_eq!(
            result
                .conversation_state
                .conversation_id
                .chars()
                .filter(|c| *c == '-')
                .count(),
            4
        );
    }

    #[test]
    fn test_validate_tool_pairing_orphaned_result() {
        // testorphan tool_result filtered
        // in historynone tool_use, but tool_results has tool_result
        let history = vec![
            Message::User(HistoryUserMessage::new("Hello", "claude-sonnet-4.5")),
            Message::Assistant(HistoryAssistantMessage::new("Hi there!")),
        ];

        let tool_results = vec![ToolResult::success("orphan-123", "some result")];

        let (filtered, _) = validate_tool_pairing(&history, &tool_results);

        // orphan tool_result shouldthisfilteredoff
        assert!(filtered.is_empty(), "orphan tool_result shouldthisfiltered");
    }

    #[test]
    fn test_validate_tool_pairing_orphaned_use() {
        use crate::kiro::model::requests::tool::ToolUseEntry;

        // testorphan tool_use(has tool_use butnonecorrespondof tool_result)
        let mut assistant_msg = AssistantMessage::new("I'll read the file.");
        assistant_msg = assistant_msg.with_tool_uses(vec![
            ToolUseEntry::new("tool-orphan", "read")
                .with_input(serde_json::json!({"path": "/test.txt"})),
        ]);

        let history = vec![
            Message::User(HistoryUserMessage::new(
                "Read the file",
                "claude-sonnet-4.5",
            )),
            Message::Assistant(HistoryAssistantMessage {
                assistant_response_message: assistant_msg,
            }),
        ];

        // none tool_result
        let tool_results: Vec<ToolResult> = vec![];

        let (filtered, orphaned) = validate_tool_pairing(&history, &tool_results);

        // The result should be empty (because there is no tool_result)
        // at the same time should return the orphaned tool_use_id
        assert!(filtered.is_empty());
        assert!(orphaned.contains("tool-orphan"));
    }

    #[test]
    fn test_validate_tool_pairing_valid() {
        use crate::kiro::model::requests::tool::ToolUseEntry;

        // test the normal pairing case
        let mut assistant_msg = AssistantMessage::new("I'll read the file.");
        assistant_msg = assistant_msg.with_tool_uses(vec![
            ToolUseEntry::new("tool-1", "read")
                .with_input(serde_json::json!({"path": "/test.txt"})),
        ]);

        let history = vec![
            Message::User(HistoryUserMessage::new(
                "Read the file",
                "claude-sonnet-4.5",
            )),
            Message::Assistant(HistoryAssistantMessage {
                assistant_response_message: assistant_msg,
            }),
        ];

        let tool_results = vec![ToolResult::success("tool-1", "file content")];

        let (filtered, orphaned) = validate_tool_pairing(&history, &tool_results);

        // Paired successfully, should be kept, no orphans.
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].tool_use_id, "tool-1");
        assert!(orphaned.is_empty());
    }

    #[test]
    fn test_validate_tool_pairing_mixed() {
        use crate::kiro::model::requests::tool::ToolUseEntry;

        // Tests a mixed case: some pair successfully, some are orphaned.
        let mut assistant_msg = AssistantMessage::new("I'll use two tools.");
        assistant_msg = assistant_msg.with_tool_uses(vec![
            ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({})),
            ToolUseEntry::new("tool-2", "write").with_input(serde_json::json!({})),
        ]);

        let history = vec![
            Message::User(HistoryUserMessage::new("Do something", "claude-sonnet-4.5")),
            Message::Assistant(HistoryAssistantMessage {
                assistant_response_message: assistant_msg,
            }),
        ];

        // tool_results: tool-1 pair,tool-3 orphan
        let tool_results = vec![
            ToolResult::success("tool-1", "result 1"),
            ToolResult::success("tool-3", "orphan result"), // orphan
        ];

        let (filtered, orphaned) = validate_tool_pairing(&history, &tool_results);

        // only tool-1 shouldthisretain
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].tool_use_id, "tool-1");
        // tool-2 isorphan tool_use(no result),tool-3 isorphan tool_result
        assert!(orphaned.contains("tool-2"));
    }

    #[test]
    fn test_validate_tool_pairing_history_already_paired() {
        use crate::kiro::model::requests::tool::ToolUseEntry;

        // test the already paired ones in history tool_use should not be reported as orphaned
        // Case: in a multi round conversation, the previous tool_use already has a corresponding one in history tool_result
        let mut assistant_msg1 = AssistantMessage::new("I'll read the file.");
        assistant_msg1 = assistant_msg1.with_tool_uses(vec![
            ToolUseEntry::new("tool-1", "read")
                .with_input(serde_json::json!({"path": "/test.txt"})),
        ]);

        // buildin historyof user message, contains tool_result
        let mut user_msg_with_result = UserMessage::new("", "claude-sonnet-4.5");
        let mut ctx = UserInputMessageContext::new();
        ctx = ctx.with_tool_results(vec![ToolResult::success("tool-1", "file content")]);
        user_msg_with_result = user_msg_with_result.with_context(ctx);

        let history = vec![
            // the first round: user request
            Message::User(HistoryUserMessage::new(
                "Read the file",
                "claude-sonnet-4.5",
            )),
            // numberoneround:assistant usetool
            Message::Assistant(HistoryAssistantMessage {
                assistant_response_message: assistant_msg1,
            }),
            // Round two: the user returns the tool result (already paired in history).
            Message::User(HistoryUserMessage {
                user_input_message: user_msg_with_result,
            }),
            // numbertworound:assistant response
            Message::Assistant(HistoryAssistantMessage::new("The file contains...")),
        ];

        // current messagenone tool_results(the user is just continuing the conversation)
        let tool_results: Vec<ToolResult> = vec![];

        let (filtered, orphaned) = validate_tool_pairing(&history, &tool_results);

        // The result should be empty, and there should be no orphans. tool_use
        // because tool-1 already paired in history
        assert!(filtered.is_empty());
        assert!(orphaned.is_empty());
    }

    #[test]
    fn test_validate_tool_pairing_duplicate_result() {
        use crate::kiro::model::requests::tool::ToolUseEntry;

        // testduplicate tool_result(already paired in history, but the current message sent the same one again) tool_result)
        let mut assistant_msg = AssistantMessage::new("I'll read the file.");
        assistant_msg = assistant_msg.with_tool_uses(vec![
            ToolUseEntry::new("tool-1", "read")
                .with_input(serde_json::json!({"path": "/test.txt"})),
        ]);

        // in historyalreadyhas tool_result
        let mut user_msg_with_result = UserMessage::new("", "claude-sonnet-4.5");
        let mut ctx = UserInputMessageContext::new();
        ctx = ctx.with_tool_results(vec![ToolResult::success("tool-1", "file content")]);
        user_msg_with_result = user_msg_with_result.with_context(ctx);

        let history = vec![
            Message::User(HistoryUserMessage::new(
                "Read the file",
                "claude-sonnet-4.5",
            )),
            Message::Assistant(HistoryAssistantMessage {
                assistant_response_message: assistant_msg,
            }),
            Message::User(HistoryUserMessage {
                user_input_message: user_msg_with_result,
            }),
            Message::Assistant(HistoryAssistantMessage::new("Done")),
        ];

        // The current message sent the same one again. tool_result(duplicate)
        let tool_results = vec![ToolResult::success("tool-1", "file content again")];

        let (filtered, _) = validate_tool_pairing(&history, &tool_results);

        // duplicate tool_result shouldthisfilteredoff
        assert!(filtered.is_empty(), "duplicate tool_result shouldthisfiltered");
    }

    #[test]
    fn test_convert_assistant_message_tool_use_only() {
        use super::super::types::Message as AnthropicMessage;

        // testonlycontains tool_use of assistant message (none text block)
        // Kiro API require content fieldnotcanis empty
        let msg = AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::json!([
                {"type": "tool_use", "id": "toolu_01ABC", "name": "read_file", "input": {"path": "/test.txt"}}
            ]),
        };

        let result = convert_assistant_message(&msg, &mut HashMap::new()).expect("shouldthissuccessconvert");

        // verify content not empty (use a placeholder)
        assert!(
            !result.assistant_response_message.content.is_empty(),
            "content should notis empty"
        );
        assert_eq!(
            result.assistant_response_message.content, " ",
            "only tool_use whenshoulduse ' ' placeholder"
        );

        // verify tool_uses byproperensurekeep
        let tool_uses = result
            .assistant_response_message
            .tool_uses
            .expect("should have tool_uses");
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].tool_use_id, "toolu_01ABC");
        assert_eq!(tool_uses[0].name, "read_file");
    }

    #[test]
    fn test_convert_assistant_message_with_text_and_tool_use() {
        use super::super::types::Message as AnthropicMessage;

        // testsamewhencontains text and tool_use of assistant message
        let msg = AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::json!([
                {"type": "text", "text": "Let me read that file for you."},
                {"type": "tool_use", "id": "toolu_02XYZ", "name": "read_file", "input": {"path": "/data.json"}}
            ]),
        };

        let result = convert_assistant_message(&msg, &mut HashMap::new()).expect("shouldthissuccessconvert");

        // verify content Uses the original text (not the placeholder).
        assert_eq!(
            result.assistant_response_message.content,
            "Let me read that file for you."
        );

        // verify tool_uses byproperensurekeep
        let tool_uses = result
            .assistant_response_message
            .tool_uses
            .expect("should have tool_uses");
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].tool_use_id, "toolu_02XYZ");
    }

    #[test]
    fn test_remove_orphaned_tool_uses() {
        use crate::kiro::model::requests::tool::ToolUseEntry;

        // Tests removing orphaned ones from history. tool_use
        let mut assistant_msg = AssistantMessage::new("I'll use multiple tools.");
        assistant_msg = assistant_msg.with_tool_uses(vec![
            ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({})),
            ToolUseEntry::new("tool-2", "write").with_input(serde_json::json!({})),
            ToolUseEntry::new("tool-3", "delete").with_input(serde_json::json!({})),
        ]);

        let mut history = vec![
            Message::User(HistoryUserMessage::new("Do something", "claude-sonnet-4.5")),
            Message::Assistant(HistoryAssistantMessage {
                assistant_response_message: assistant_msg,
            }),
        ];

        // remove tool-1 and tool-3
        let mut orphaned = std::collections::HashSet::new();
        orphaned.insert("tool-1".to_string());
        orphaned.insert("tool-3".to_string());

        remove_orphaned_tool_uses(&mut history, &orphaned);

        // verifyonlyremaining tool-2
        if let Message::Assistant(ref assistant_msg) = history[1] {
            let tool_uses = assistant_msg
                .assistant_response_message
                .tool_uses
                .as_ref()
                .expect("shouldthisstillhas tool_uses");
            assert_eq!(tool_uses.len(), 1);
            assert_eq!(tool_uses[0].tool_use_id, "tool-2");
        } else {
            panic!("should be Assistant message");
        }
    }

    #[test]
    fn test_remove_orphaned_tool_uses_all_removed() {
        use crate::kiro::model::requests::tool::ToolUseEntry;

        // testremoveall tool_use after,tool_uses becomes None
        let mut assistant_msg = AssistantMessage::new("I'll use a tool.");
        assistant_msg = assistant_msg.with_tool_uses(vec![
            ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({})),
        ]);

        let mut history = vec![
            Message::User(HistoryUserMessage::new("Do something", "claude-sonnet-4.5")),
            Message::Assistant(HistoryAssistantMessage {
                assistant_response_message: assistant_msg,
            }),
        ];

        let mut orphaned = std::collections::HashSet::new();
        orphaned.insert("tool-1".to_string());

        remove_orphaned_tool_uses(&mut history, &orphaned);

        // verify tool_uses becomes None
        if let Message::Assistant(ref assistant_msg) = history[1] {
            assert!(
                assistant_msg.assistant_response_message.tool_uses.is_none(),
                "removeall tool_use should be after None"
            );
        } else {
            panic!("should be Assistant message");
        }
    }

    #[test]
    fn test_merge_consecutive_assistant_messages() {
        // testconsecutive assistant the messages are correctly merged (Issue #79)
        use super::super::types::Message as AnthropicMessage;

        let msg1 = AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::json!([
                {"type": "thinking", "thinking": "Let me think about this..."},
                {"type": "text", "text": " "}
            ]),
        };

        let msg2 = AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::json!([
                {"type": "thinking", "thinking": "I should read the file."},
                {"type": "text", "text": "Let me read that file."},
                {"type": "tool_use", "id": "toolu_01ABC", "name": "read_file", "input": {"path": "/test.txt"}}
            ]),
        };

        let messages: Vec<&AnthropicMessage> = vec![&msg1, &msg2];
        let result = merge_assistant_messages(&messages, &mut HashMap::new()).expect("mergeshouldsuccess");

        let content = &result.assistant_response_message.content;
        assert!(content.contains("<thinking>"), "should contain thinking tag");
        assert!(content.contains("Let me read that file"), "should contain the second message text content");

        let tool_uses = result.assistant_response_message.tool_uses.expect("should have tool_uses");
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].tool_use_id, "toolu_01ABC");
    }

    #[test]
    fn test_consecutive_assistant_with_tool_use_result_pairing() {
        // test Issue #79 ofcomplete scenario
        use super::super::types::Message as AnthropicMessage;

        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!("Read the config file"),
                },
                AnthropicMessage {
                    role: "assistant".to_string(),
                    content: serde_json::json!([
                        {"type": "thinking", "thinking": "I need to read the file..."},
                        {"type": "text", "text": " "}
                    ]),
                },
                AnthropicMessage {
                    role: "assistant".to_string(),
                    content: serde_json::json!([
                        {"type": "thinking", "thinking": "Let me read the config."},
                        {"type": "text", "text": "I'll read the config file for you."},
                        {"type": "tool_use", "id": "toolu_01XYZ", "name": "read_file", "input": {"path": "/config.json"}}
                    ]),
                },
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!([
                        {"type": "tool_result", "tool_use_id": "toolu_01XYZ", "content": "{\"key\": \"value\"}"}
                    ]),
                },
            ],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req);
        assert!(result.is_ok(), "consecutive assistant the message scenario should not error: {:?}", result.err());

        let state = result.unwrap().conversation_state;
        let mut found_tool_use = false;
        for msg in &state.history {
            if let Message::Assistant(assistant_msg) = msg {
                if let Some(ref tool_uses) = assistant_msg.assistant_response_message.tool_uses {
                    if tool_uses.iter().any(|t| t.tool_use_id == "toolu_01XYZ") {
                        found_tool_use = true;
                        break;
                    }
                }
            }
        }
        assert!(found_tool_use, "mergeafterof assistant messageshould contain tool_use");
    }

    // base64 of a 1x1 PNG (valid PNG header, so resize just passes it through)
    const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M8AAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

    #[test]
    fn test_tool_result_image_lifts_to_top_level() {
        use super::super::types::Message as AnthropicMessage;

        // user question -> assistant tool_use -> user tool_result (with image + text)
        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!("take a screenshot"),
                },
                AnthropicMessage {
                    role: "assistant".to_string(),
                    content: serde_json::json!([
                        {"type": "tool_use", "id": "tool-1", "name": "screenshot", "input": {}}
                    ]),
                },
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!([
                        {"type": "tool_result", "tool_use_id": "tool-1", "content": [
                            {"type": "text", "text": "here is the screen"},
                            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": TINY_PNG_B64}}
                        ]}
                    ]),
                },
            ],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();
        let msg = &result.conversation_state.current_message.user_input_message;

        // image is lifted to the top-level images
        assert_eq!(msg.images.len(), 1, "image in tool_result should be lifted to top-level images");
        assert_eq!(msg.images[0].format, "png");
        assert_eq!(msg.images[0].source.bytes, TINY_PNG_B64);

        // tool_result itself keeps only the text placeholder (image stripped out)
        let tr = &msg.user_input_message_context.tool_results;
        assert_eq!(tr.len(), 1);
        assert_eq!(
            tr[0].content[0].get("text").and_then(|v| v.as_str()),
            Some("here is the screen"),
            "tool_result content should keep the text and contain no base64"
        );
    }

    #[test]
    fn test_tool_result_text_only_unchanged() {
        use super::super::types::Message as AnthropicMessage;

        // text-only tool_result: regression unchanged, should produce no top-level image
        let req = MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!("read the file"),
                },
                AnthropicMessage {
                    role: "assistant".to_string(),
                    content: serde_json::json!([
                        {"type": "tool_use", "id": "tool-1", "name": "read", "input": {"path": "/a.txt"}}
                    ]),
                },
                AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::json!([
                        {"type": "tool_result", "tool_use_id": "tool-1", "content": "file content"}
                    ]),
                },
            ],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();
        let msg = &result.conversation_state.current_message.user_input_message;

        assert!(msg.images.is_empty(), "text-only tool_result should produce no top-level image");
        let tr = &msg.user_input_message_context.tool_results;
        assert_eq!(tr.len(), 1);
        assert_eq!(
            tr[0].content[0].get("text").and_then(|v| v.as_str()),
            Some("file content"),
            "text-only tool_result content should be preserved as-is"
        );
    }
}
