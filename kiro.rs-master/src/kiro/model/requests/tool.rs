//! tooltypedefine
//!
//! define Kiro API the tool related types in

use serde::{Deserialize, Serialize};

/// tool definition
///
/// Used to define the available tools in the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// tool spec
    pub tool_specification: ToolSpecification,
}

/// tool spec
///
/// Defines the tool name, description, and input schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpecification {
    /// tool name
    pub name: String,
    /// tool description
    pub description: String,
    /// input schema(JSON Schema)
    pub input_schema: InputSchema,
}

/// input schema
///
/// wrap JSON Schema define
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSchema {
    /// JSON Schema define
    pub json: serde_json::Value,
}

impl Default for InputSchema {
    fn default() -> Self {
        Self {
            json: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }
}

impl InputSchema {
    /// from JSON value create
    pub fn from_json(json: serde_json::Value) -> Self {
        Self { json }
    }
}

/// toolexecutelineresult
///
/// Used to return the tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    /// tool use ID(within request tool_use_id corresponds)
    pub tool_use_id: String,
    /// result content (array format)
    pub content: Vec<serde_json::Map<String, serde_json::Value>>,
    /// executelinestate("success" or "error")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// whether iserror
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_error: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl ToolResult {
    /// create a successful tool result
    pub fn success(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut map = serde_json::Map::new();
        map.insert(
            "text".to_string(),
            serde_json::Value::String(content.into()),
        );

        Self {
            tool_use_id: tool_use_id.into(),
            content: vec![map],
            status: Some("success".to_string()),
            is_error: false,
        }
    }

    /// create an error tool result
    pub fn error(tool_use_id: impl Into<String>, error_message: impl Into<String>) -> Self {
        let mut map = serde_json::Map::new();
        map.insert(
            "text".to_string(),
            serde_json::Value::String(error_message.into()),
        );

        Self {
            tool_use_id: tool_use_id.into(),
            content: vec![map],
            status: Some("error".to_string()),
            is_error: true,
        }
    }
}

/// tool useentryentry
///
/// Used to record tool calls in history messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseEntry {
    /// tool use ID
    pub tool_use_id: String,
    /// tool name
    pub name: String,
    /// toolinput parameter
    pub input: serde_json::Value,
}

impl ToolUseEntry {
    /// create a new tool use entry
    pub fn new(tool_use_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            name: name.into(),
            input: serde_json::json!({}),
        }
    }

    /// setinput parameter
    pub fn with_input(mut self, input: serde_json::Value) -> Self {
        self.input = input;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("tool-123", "Operation completed");

        assert!(!result.is_error);
        assert_eq!(result.status, Some("success".to_string()));
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("tool-456", "File not found");

        assert!(result.is_error);
        assert_eq!(result.status, Some("error".to_string()));
    }

    #[test]
    fn test_tool_result_serialize() {
        let result = ToolResult::success("tool-789", "Done");
        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains("\"toolUseId\":\"tool-789\""));
        assert!(json.contains("\"status\":\"success\""));
        // is_error = false shouldthisskipped
        assert!(!json.contains("isError"));
    }

    #[test]
    fn test_tool_use_entry() {
        let entry = ToolUseEntry::new("use-123", "read_file")
            .with_input(serde_json::json!({"path": "/test.txt"}));

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"toolUseId\":\"use-123\""));
        assert!(json.contains("\"name\":\"read_file\""));
        assert!(json.contains("\"path\":\"/test.txt\""));
    }

    #[test]
    fn test_input_schema_default() {
        let schema = InputSchema::default();
        assert_eq!(schema.json["type"], "object");
    }
}
