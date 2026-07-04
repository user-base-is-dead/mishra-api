//! forwordstypedefine
//!
//! define Kiro API The conversation related types, including messages, history, and so on.

use serde::{Deserialize, Serialize};

use super::tool::{Tool, ToolResult, ToolUseEntry};

/// conversation state
///
/// Kiro API The core structure in the request, containing the current message and history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationState {
    /// proxy continuation ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_continuation_id: Option<String>,
    /// proxy task type (usually "vibe")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_task_type: Option<String>,
    /// chat trigger type ("MANUAL" or "AUTO")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_trigger_type: Option<String>,
    /// current message
    pub current_message: CurrentMessage,
    /// session ID
    pub conversation_id: String,
    /// history messagelist
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Message>,
}

impl ConversationState {
    /// create a new conversation state
    pub fn new(conversation_id: impl Into<String>) -> Self {
        Self {
            agent_continuation_id: None,
            agent_task_type: None,
            chat_trigger_type: None,
            current_message: CurrentMessage::default(),
            conversation_id: conversation_id.into(),
            history: Vec::new(),
        }
    }

    /// setproxy continuation ID
    pub fn with_agent_continuation_id(mut self, id: impl Into<String>) -> Self {
        self.agent_continuation_id = Some(id.into());
        self
    }

    /// set the proxy task type
    pub fn with_agent_task_type(mut self, task_type: impl Into<String>) -> Self {
        self.agent_task_type = Some(task_type.into());
        self
    }

    /// set the chat trigger type
    pub fn with_chat_trigger_type(mut self, trigger_type: impl Into<String>) -> Self {
        self.chat_trigger_type = Some(trigger_type.into());
        self
    }

    /// setcurrent message
    pub fn with_current_message(mut self, message: CurrentMessage) -> Self {
        self.current_message = message;
        self
    }

    /// addhistory message
    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        self.history = history;
        self
    }
}

/// current messagecontainer
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentMessage {
    /// userinputmessage
    pub user_input_message: UserInputMessage,
}

impl CurrentMessage {
    /// create a new current message
    pub fn new(user_input_message: UserInputMessage) -> Self {
        Self { user_input_message }
    }
}

/// userinputmessage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputMessage {
    /// User input message context (always serialized,envState is CLI endpoint requiredfield)
    pub user_input_message_context: UserInputMessageContext,
    /// message content
    pub content: String,
    /// model ID
    pub model_id: String,
    /// image list
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<KiroImage>,
    /// message source (usually "AI_EDITOR")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl UserInputMessage {
    /// create a new user input message
    pub fn new(content: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            user_input_message_context: UserInputMessageContext::default(),
            content: content.into(),
            model_id: model_id.into(),
            images: Vec::new(),
            origin: Some("AI_EDITOR".to_string()),
        }
    }

    /// set the message context
    pub fn with_context(mut self, context: UserInputMessageContext) -> Self {
        self.user_input_message_context = context;
        self
    }

    /// add image
    pub fn with_images(mut self, images: Vec<KiroImage>) -> Self {
        self.images = images;
        self
    }

    /// set source
    pub fn with_origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
        self
    }
}

/// environmentstate(kiro-cli always send this field,CLI endpoint requirement)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvState {
    pub operating_system: String,
    pub current_working_directory: String,
}

impl Default for EnvState {
    fn default() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "/".to_string());
        Self {
            operating_system: "macos".to_string(),
            current_working_directory: cwd,
        }
    }
}

/// user input message context
///
/// Contains tool definitions and tool execution results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputMessageContext {
    /// environmentstate(kiro-cli always carries)
    pub env_state: EnvState,
    /// tool execution result list
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<ToolResult>,
    /// availabletoollist
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
}

impl Default for UserInputMessageContext {
    fn default() -> Self {
        Self {
            env_state: EnvState::default(),
            tool_results: Vec::new(),
            tools: Vec::new(),
        }
    }
}

fn is_empty_context(ctx: &UserInputMessageContext) -> bool {
    ctx.tools.is_empty() && ctx.tool_results.is_empty()
}

impl UserInputMessageContext {
    /// create a new message context
    pub fn new() -> Self {
        Self::default()
    }

    /// settoollist
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = tools;
        self
    }

    /// settoolresult
    pub fn with_tool_results(mut self, results: Vec<ToolResult>) -> Self {
        self.tool_results = results;
        self
    }
}

/// Kiro image
///
/// API the image format used in
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroImage {
    /// imageformat ("jpeg", "png", "gif", "webp")
    pub format: String,
    /// imagedatasource
    pub source: KiroImageSource,
}

impl KiroImage {
    /// from base64 datacreateimage
    pub fn from_base64(format: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            format: format.into(),
            source: KiroImageSource { bytes: data.into() },
        }
    }
}

/// Kiro imagedatasource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiroImageSource {
    /// base64 encoded image data
    pub bytes: String,
}

/// history message
///
/// Can be a user message or an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    /// user message
    User(HistoryUserMessage),
    /// assistant message
    Assistant(HistoryAssistantMessage),
}

/// historyuser message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryUserMessage {
    /// userinputmessage
    pub user_input_message: UserMessage,
}

impl HistoryUserMessage {
    /// create a new history user message
    pub fn new(content: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            user_input_message: UserMessage::new(content, model_id),
        }
    }
}

/// User message (used in history).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessage {
    /// message content
    pub content: String,
    /// model ID
    pub model_id: String,
    /// message source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// image list
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<KiroImage>,
    /// User input message context (skipped when history messages have no tools).
    #[serde(default, skip_serializing_if = "is_empty_context")]
    pub user_input_message_context: UserInputMessageContext,
}

impl UserMessage {
    /// create a new user message
    pub fn new(content: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            model_id: model_id.into(),
            origin: Some("AI_EDITOR".to_string()),
            images: Vec::new(),
            user_input_message_context: UserInputMessageContext::default(),
        }
    }

    /// set image
    pub fn with_images(mut self, images: Vec<KiroImage>) -> Self {
        self.images = images;
        self
    }

    /// setcontext
    pub fn with_context(mut self, context: UserInputMessageContext) -> Self {
        self.user_input_message_context = context;
        self
    }
}

/// historyassistant message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryAssistantMessage {
    /// assistant responsemessage
    pub assistant_response_message: AssistantMessage,
}

impl HistoryAssistantMessage {
    /// create a new history assistant message
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            assistant_response_message: AssistantMessage::new(content),
        }
    }
}

/// Assistant message (used in history).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    /// response content
    pub content: String,
    /// tool uselist
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_uses: Option<Vec<ToolUseEntry>>,
}

impl AssistantMessage {
    /// create a new assistant message
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tool_uses: None,
        }
    }

    /// settool use
    pub fn with_tool_uses(mut self, tool_uses: Vec<ToolUseEntry>) -> Self {
        self.tool_uses = Some(tool_uses);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_state_new() {
        let state = ConversationState::new("conv-123")
            .with_agent_task_type("vibe")
            .with_chat_trigger_type("MANUAL");

        assert_eq!(state.conversation_id, "conv-123");
        assert_eq!(state.agent_task_type, Some("vibe".to_string()));
        assert_eq!(state.chat_trigger_type, Some("MANUAL".to_string()));
    }

    #[test]
    fn test_user_input_message() {
        let msg = UserInputMessage::new("Hello", "claude-3-5-sonnet").with_origin("AI_EDITOR");

        assert_eq!(msg.content, "Hello");
        assert_eq!(msg.model_id, "claude-3-5-sonnet");
        assert_eq!(msg.origin, Some("AI_EDITOR".to_string()));
    }

    #[test]
    fn test_history_serialize() {
        let history = vec![
            Message::User(HistoryUserMessage::new("Hello", "claude-3-5-sonnet")),
            Message::Assistant(HistoryAssistantMessage::new("Hi! How can I help you?")),
        ];

        let json = serde_json::to_string(&history).unwrap();
        assert!(json.contains("userInputMessage"));
        assert!(json.contains("assistantResponseMessage"));
    }

    #[test]
    fn test_conversation_state_serialize() {
        let state = ConversationState::new("conv-123")
            .with_agent_task_type("vibe")
            .with_current_message(CurrentMessage::new(UserInputMessage::new(
                "Hello",
                "claude-3-5-sonnet",
            )));

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"conversationId\":\"conv-123\""));
        assert!(json.contains("\"agentTaskType\":\"vibe\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }
}
