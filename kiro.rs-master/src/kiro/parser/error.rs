//! AWS Event Stream parse errordefine

use std::fmt;

/// parse errortype
#[derive(Debug)]
pub enum ParseError {
    /// Insufficient data; needs more bytes.
    Incomplete { needed: usize, available: usize },
    /// Prelude CRC validation failed
    PreludeCrcMismatch { expected: u32, actual: u32 },
    /// Message CRC validation failed
    MessageCrcMismatch { expected: u32, actual: u32 },
    /// invalid header value type
    InvalidHeaderType(u8),
    /// headerparse error
    HeaderParseFailed(String),
    /// messagelengthover limit
    MessageTooLarge { length: u32, max: u32 },
    /// messagelengthtoo small
    MessageTooSmall { length: u32, min: u32 },
    /// invalid message type
    InvalidMessageType(String),
    /// Payload deserializefailed
    PayloadDeserialize(serde_json::Error),
    /// IO error
    Io(std::io::Error),
    /// Too many consecutive errors; the decoder has stopped.
    TooManyErrors { count: usize, last_error: String },
    /// buffer overflow
    BufferOverflow { size: usize, max: usize },
}

impl std::error::Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incomplete { needed, available } => {
                write!(f, "insufficient data: need {} bytes, current {} bytes", needed, available)
            }
            Self::PreludeCrcMismatch { expected, actual } => {
                write!(
                    f,
                    "Prelude CRC validation failed: expected 0x{:08x}, actual 0x{:08x}",
                    expected, actual
                )
            }
            Self::MessageCrcMismatch { expected, actual } => {
                write!(
                    f,
                    "Message CRC validation failed: expected 0x{:08x}, actual 0x{:08x}",
                    expected, actual
                )
            }
            Self::InvalidHeaderType(t) => write!(f, "invalid header value type: {}", t),
            Self::HeaderParseFailed(msg) => write!(f, "header parsefailed: {}", msg),
            Self::MessageTooLarge { length, max } => {
                write!(f, "messagelengthover limit: {} bytes (maximum {})", length, max)
            }
            Self::MessageTooSmall { length, min } => {
                write!(f, "messagelengthtoo small: {} bytes (minimum {})", length, min)
            }
            Self::InvalidMessageType(t) => write!(f, "invalid message type: {}", t),
            Self::PayloadDeserialize(e) => write!(f, "Payload deserializefailed: {}", e),
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::TooManyErrors { count, last_error } => {
                write!(
                    f,
                    "consecutiveerrortoo many ({} times), the decoder has stopped: {}",
                    count, last_error
                )
            }
            Self::BufferOverflow { size, max } => {
                write!(f, "buffer overflow: {} bytes (maximum {})", size, max)
            }
        }
    }
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> Self {
        Self::PayloadDeserialize(e)
    }
}

/// parseresulttype
pub type ParseResult<T> = Result<T, ParseError>;
