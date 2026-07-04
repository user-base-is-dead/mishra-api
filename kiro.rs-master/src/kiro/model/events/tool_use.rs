//! tool useevent
//!
//! handle toolUseEvent typeofevent

use serde::Deserialize;

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// tool useevent
///
/// Streaming data containing tool calls.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseEvent {
    /// tool name
    pub name: String,
    /// tool call ID
    pub tool_use_id: String,
    /// toolinputdata (JSON string, possibly partial streaming data.)
    #[serde(default)]
    pub input: String,
    /// whether it is the last block
    #[serde(default)]
    pub stop: bool,
}

impl EventPayload for ToolUseEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

impl std::fmt::Display for ToolUseEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stop {
            write!(
                f,
                "ToolUse[{}] (id={}, complete): {}",
                self.name, self.tool_use_id, self.input
            )
        } else {
            write!(
                f,
                "ToolUse[{}] (id={}, partial): {}",
                self.name, self.tool_use_id, self.input
            )
        }
    }
}
