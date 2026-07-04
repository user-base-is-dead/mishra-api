//! reasoning contentevent
//!
//! handle reasoningContentEvent typeofevent.

use serde::Deserialize;

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// Kiro native thinking / reasoning event.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningContentEvent {
    /// plaintext thinking content fragment.
    #[serde(default)]
    pub text: Option<String>,
    /// thinkingblocksignature,Anthropic The client returns it as is on the next round.
    #[serde(default)]
    pub signature: Option<String>,
    /// The encrypted thinking content returned by upstream.
    #[serde(default)]
    pub redacted_content: Option<String>,
}

impl EventPayload for ReasoningContentEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_signature_payload() {
        let v: ReasoningContentEvent =
            serde_json::from_str(r#"{"text":"abc","signature":"sig"}"#).unwrap();
        assert_eq!(v.text.as_deref(), Some("abc"));
        assert_eq!(v.signature.as_deref(), Some("sig"));
    }

    #[test]
    fn parse_redacted_payload() {
        let v: ReasoningContentEvent =
            serde_json::from_str(r#"{"redactedContent":"encrypted"}"#).unwrap();
        assert_eq!(v.redacted_content.as_deref(), Some("encrypted"));
    }
}
