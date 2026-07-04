//! eventbasisdefine
//!
//! define the event type enum,trait and the unified event structure

use crate::kiro::parser::error::{ParseError, ParseResult};
use crate::kiro::parser::frame::Frame;

/// eventtypeenumerate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    /// assistant responseevent
    AssistantResponse,
    /// tool useevent
    ToolUse,
    /// billing event
    Metering,
    /// context usage rate event
    ContextUsage,
    /// reasoning contentevent
    ReasoningContent,
    /// unknown eventtype
    Unknown,
}

impl EventType {
    /// parse from the event type string
    pub fn from_str(s: &str) -> Self {
        match s {
            "assistantResponseEvent" => Self::AssistantResponse,
            "toolUseEvent" => Self::ToolUse,
            "meteringEvent" => Self::Metering,
            "contextUsageEvent" => Self::ContextUsage,
            "reasoningContentEvent" => Self::ReasoningContent,
            _ => Self::Unknown,
        }
    }

    /// convert to the event type string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AssistantResponse => "assistantResponseEvent",
            Self::ToolUse => "toolUseEvent",
            Self::Metering => "meteringEvent",
            Self::ContextUsage => "contextUsageEvent",
            Self::ReasoningContent => "reasoningContentEvent",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// event payload trait
///
/// Every concrete event type must implement this. trait
pub trait EventPayload: Sized {
    /// parse the event payload from the frame
    fn from_frame(frame: &Frame) -> ParseResult<Self>;
}

/// unifyeventenumerate
///
/// Encapsulates all possible event types.
#[derive(Debug, Clone)]
pub enum Event {
    /// assistant response
    AssistantResponse(super::AssistantResponseEvent),
    /// tool use
    ToolUse(super::ToolUseEvent),
    /// billing
    Metering(super::MeteringEvent),
    /// contextuserate
    ContextUsage(super::ContextUsageEvent),
    /// reasoning content
    ReasoningContent(super::ReasoningContentEvent),
    /// unknown event (keep the raw frame data)
    Unknown {},
    /// serviceenderror
    Error {
        /// error code
        error_code: String,
        /// error message
        error_message: String,
    },
    /// serviceendexception
    Exception {
        /// exception type
        exception_type: String,
        /// exception message
        message: String,
    },
}

impl Event {
    /// fromframeparse event
    pub fn from_frame(frame: Frame) -> ParseResult<Self> {
        let message_type = frame.message_type().unwrap_or("event");

        match message_type {
            "event" => Self::parse_event(frame),
            "error" => Self::parse_error(frame),
            "exception" => Self::parse_exception(frame),
            other => Err(ParseError::InvalidMessageType(other.to_string())),
        }
    }

    /// parse the event type message
    fn parse_event(frame: Frame) -> ParseResult<Self> {
        let event_type_str = frame.event_type().unwrap_or("unknown");
        let event_type = EventType::from_str(event_type_str);

        match event_type {
            EventType::AssistantResponse => {
                let payload = super::AssistantResponseEvent::from_frame(&frame)?;
                Ok(Self::AssistantResponse(payload))
            }
            EventType::ToolUse => {
                let payload = super::ToolUseEvent::from_frame(&frame)?;
                Ok(Self::ToolUse(payload))
            }
            EventType::Metering => {
                let payload = super::MeteringEvent::from_frame(&frame)?;
                Ok(Self::Metering(payload))
            }
            EventType::ContextUsage => {
                let payload = super::ContextUsageEvent::from_frame(&frame)?;
                Ok(Self::ContextUsage(payload))
            }
            EventType::ReasoningContent => {
                let payload = super::ReasoningContentEvent::from_frame(&frame)?;
                Ok(Self::ReasoningContent(payload))
            }
            EventType::Unknown => Ok(Self::Unknown {}),
        }
    }

    /// parse the error type message
    fn parse_error(frame: Frame) -> ParseResult<Self> {
        let error_code = frame
            .headers
            .error_code()
            .unwrap_or("UnknownError")
            .to_string();
        let error_message = frame.payload_as_str();

        Ok(Self::Error {
            error_code,
            error_message,
        })
    }

    /// parse the exception type message
    fn parse_exception(frame: Frame) -> ParseResult<Self> {
        let exception_type = frame
            .headers
            .exception_type()
            .unwrap_or("UnknownException")
            .to_string();
        let message = frame.payload_as_str();

        Ok(Self::Exception {
            exception_type,
            message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_from_str() {
        assert_eq!(
            EventType::from_str("assistantResponseEvent"),
            EventType::AssistantResponse
        );
        assert_eq!(EventType::from_str("toolUseEvent"), EventType::ToolUse);
        assert_eq!(EventType::from_str("meteringEvent"), EventType::Metering);
        assert_eq!(
            EventType::from_str("contextUsageEvent"),
            EventType::ContextUsage
        );
        assert_eq!(
            EventType::from_str("reasoningContentEvent"),
            EventType::ReasoningContent
        );
        assert_eq!(EventType::from_str("unknown_type"), EventType::Unknown);
    }

    #[test]
    fn test_event_type_as_str() {
        assert_eq!(
            EventType::AssistantResponse.as_str(),
            "assistantResponseEvent"
        );
        assert_eq!(EventType::ToolUse.as_str(), "toolUseEvent");
    }
}
