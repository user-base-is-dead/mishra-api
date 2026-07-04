//! assistant responseevent
//!
//! handle assistantResponseEvent typeofevent

use serde::{Deserialize, Serialize};

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// assistant responseevent
///
/// contains AI the assistant streaming response content
///
/// # design note
///
/// This struct keeps only the actually used ones. `content` charactersegment,other API returnedfield
/// via `#[serde(flatten)]` captured `extra` in, ensuring deserialization does not fail.
///
/// # example
///
/// ```rust
/// use kiro_rs::kiro::model::events::AssistantResponseEvent;
///
/// let json = r#"{"content":"Hello, world!"}"#;
/// let event: AssistantResponseEvent = serde_json::from_str(json).unwrap();
/// assert_eq!(event.content, "Hello, world!");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantResponseEvent {
    /// response contentslicesegment
    #[serde(default)]
    pub content: String,

    /// Captures other unused fields to ensure deserialization compatibility.
    #[serde(flatten)]
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    extra: serde_json::Value,
}

impl EventPayload for AssistantResponseEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

impl Default for AssistantResponseEvent {
    fn default() -> Self {
        Self {
            content: String::new(),
            extra: serde_json::Value::Null,
        }
    }
}

impl std::fmt::Display for AssistantResponseEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_simple() {
        let json = r#"{"content":"Hello, world!"}"#;
        let event: AssistantResponseEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.content, "Hello, world!");
    }

    #[test]
    fn test_deserialize_with_extra_fields() {
        // Ensures deserialization does not fail when extra fields are present.
        let json = r#"{
            "content": "Done",
            "conversationId": "conv-123",
            "messageId": "msg-456",
            "messageStatus": "COMPLETED",
            "followupPrompt": {
                "content": "Would you like me to explain further?",
                "userIntent": "EXPLAIN_CODE_SELECTION"
            }
        }"#;
        let event: AssistantResponseEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.content, "Done");
    }

    #[test]
    fn test_serialize_minimal() {
        let event = AssistantResponseEvent::default();
        let event = AssistantResponseEvent {
            content: "Test".to_string(),
            ..event
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"content\":\"Test\""));
        // extra the field should not be serialized
        assert!(!json.contains("extra"));
    }

    #[test]
    fn test_display() {
        let event = AssistantResponseEvent {
            content: "test".to_string(),
            ..Default::default()
        };
        assert_eq!(format!("{}", event), "test");
    }
}
