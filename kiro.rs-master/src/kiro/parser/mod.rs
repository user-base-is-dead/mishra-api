//! AWS Event Stream parser
//!
//! provide for AWS Event Stream the protocol parsing support,
//! used to handle generateAssistantResponse the streaming response of the endpoint

pub mod crc;
pub mod decoder;
pub mod error;
pub mod frame;
pub mod header;
