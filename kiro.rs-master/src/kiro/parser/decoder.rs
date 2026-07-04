//! AWS Event Stream streamingdecoder
//!
//! Uses a state machine to process streaming data, supporting resume and fault tolerance.
//!
//! ## statemachine design
//!
//! reference kiro-kt The project state machine design uses a four state model:
//!
//! ```text
//! ┌─────────────────┐
//! │      Ready      │  (initial state, ready to receive data)
//! └────────┬────────┘
//!          │ feed() provide data
//!          ↓
//! ┌─────────────────┐
//! │     Parsing     │  decode() try to parse
//! └────────┬────────┘
//!          │
//!     ┌────┴────────────┐
//!     ↓                 ↓
//!  [success]            [failed]
//!     │                 │
//!     ↓                 ├─> error_count++
//! ┌─────────┐           │
//! │  Ready  │           ├─> error_count < max_errors?
//! └─────────┘           │    YES → Recovering → Ready
//!                       │    NO  ↓
//!                  ┌────────────┐
//!                  │   Stopped  │ (terminal state)
//!                  └────────────┘
//! ```

use super::error::{ParseError, ParseResult};
use super::frame::{Frame, PRELUDE_SIZE, parse_frame};
use bytes::{Buf, BytesMut};

/// default maximum buffer size (16 MB)
pub const DEFAULT_MAX_BUFFER_SIZE: usize = 16 * 1024 * 1024;

/// default maximum consecutive error count
pub const DEFAULT_MAX_ERRORS: usize = 5;

/// default initial buffer capacity
pub const DEFAULT_BUFFER_CAPACITY: usize = 8192;

/// decoderstate
///
/// adopt a four state model, refer to kiro-kt design:
/// - Ready: Ready state, can receive data.
/// - Parsing: properduring parseframe
/// - Recovering: Recovering (tries to skip corrupt data).
/// - Stopped: Stopped (too many errors, terminal state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderState {
    /// ready, can receive data
    Ready,
    /// properduring parseframe
    Parsing,
    /// Recovering (skips corrupt data).
    Recovering,
    /// stopped (too many errors)
    Stopped,
}

/// streaming event decoder
///
/// used to parse from the byte stream AWS Event Stream message frame
///
/// # Example
///
/// ```rust,ignore
/// use kiro_rs::kiro::parser::EventStreamDecoder;
///
/// let mut decoder = EventStreamDecoder::new();
///
/// // providestreamdata
/// decoder.feed(chunk)?;
///
/// // decode all available frames
/// for result in decoder.decode_iter() {
///     match result {
///         Ok(frame) => println!("Got frame: {:?}", frame.event_type()),
///         Err(e) => eprintln!("Parse error: {}", e),
///     }
/// }
/// ```
pub struct EventStreamDecoder {
    /// insideinternal buffer
    buffer: BytesMut,
    /// current state
    state: DecoderState,
    /// the number of processed frames
    frames_decoded: usize,
    /// consecutiveerrorcountcount
    error_count: usize,
    /// maximum consecutive error count
    max_errors: usize,
    /// maximum buffer size
    max_buffer_size: usize,
    /// The number of bytes skipped (for debugging).
    bytes_skipped: usize,
}

impl Default for EventStreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl EventStreamDecoder {
    /// create a new decoder
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_BUFFER_CAPACITY)
    }

    /// Creates a decoder with the given buffer size.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(capacity),
            state: DecoderState::Ready,
            frames_decoded: 0,
            error_count: 0,
            max_errors: DEFAULT_MAX_ERRORS,
            max_buffer_size: DEFAULT_MAX_BUFFER_SIZE,
            bytes_skipped: 0,
        }
    }

    /// provide data to the decoder
    ///
    /// # Returns
    /// - `Ok(())` - data has been added to the buffer
    /// - `Err(BufferOverflow)` - bufferalreadyfull
    pub fn feed(&mut self, data: &[u8]) -> ParseResult<()> {
        // check the buffer size limit
        let new_size = self.buffer.len() + data.len();
        if new_size > self.max_buffer_size {
            return Err(ParseError::BufferOverflow {
                size: new_size,
                max: self.max_buffer_size,
            });
        }

        self.buffer.extend_from_slice(data);

        // from Recovering staterecoverto Ready
        if self.state == DecoderState::Recovering {
            self.state = DecoderState::Ready;
        }

        Ok(())
    }

    /// try to decode the next frame
    ///
    /// # Returns
    /// - `Ok(Some(frame))` - successfully decoded one frame
    /// - `Ok(None)` - Insufficient data; needs more data.
    /// - `Err(e)` - decode error
    pub fn decode(&mut self) -> ParseResult<Option<Frame>> {
        // If already stopped, returns an error directly.
        if self.state == DecoderState::Stopped {
            return Err(ParseError::TooManyErrors {
                count: self.error_count,
                last_error: "decoderalreadystop".to_string(),
            });
        }

        // the buffer is empty, keep Ready state
        if self.buffer.is_empty() {
            self.state = DecoderState::Ready;
            return Ok(None);
        }

        // transition to Parsing state
        self.state = DecoderState::Parsing;

        match parse_frame(&self.buffer) {
            Ok(Some((frame, consumed))) => {
                // parse success
                self.buffer.advance(consumed);
                self.state = DecoderState::Ready;
                self.frames_decoded += 1;
                self.error_count = 0; // reset the consecutive error count
                Ok(Some(frame))
            }
            Ok(None) => {
                // insufficient data, return to Ready state waiting for more data
                self.state = DecoderState::Ready;
                Ok(None)
            }
            Err(e) => {
                self.error_count += 1;
                let error_msg = e.to_string();

                // Checks whether the maximum error count is exceeded.
                if self.error_count >= self.max_errors {
                    self.state = DecoderState::Stopped;
                    tracing::error!(
                        "decoder stopped: consecutive {} errors, the last error: {}",
                        self.error_count,
                        error_msg
                    );
                    return Err(ParseError::TooManyErrors {
                        count: self.error_count,
                        last_error: error_msg,
                    });
                }

                // Uses different recovery strategies based on the error type.
                self.try_recover(&e);
                self.state = DecoderState::Recovering;
                Err(e)
            }
        }
    }

    /// create the decode iterator
    pub fn decode_iter(&mut self) -> DecodeIter<'_> {
        DecodeIter { decoder: self }
    }

    /// attempt fault-tolerant recovery
    ///
    /// Uses different recovery strategies based on the error type (refer to kiro-kt ofdesign):
    /// - Prelude stagesegmenterror(CRC failure, abnormal length): skips. 1 bytes, tries to find the next frame boundary.
    /// - Data stagesegmenterror(Message CRC failed,Header parse failure): skips the whole corrupt frame.
    fn try_recover(&mut self, error: &ParseError) {
        if self.buffer.is_empty() {
            return;
        }

        match error {
            // Prelude Stage error: the frame boundary may be misaligned; scan byte by byte to find the next valid boundary.
            ParseError::PreludeCrcMismatch { .. }
            | ParseError::MessageTooSmall { .. }
            | ParseError::MessageTooLarge { .. } => {
                let skipped_byte = self.buffer[0];
                self.buffer.advance(1);
                self.bytes_skipped += 1;
                tracing::warn!(
                    "Prelude error recovery: skip bytes 0x{:02x} (cumulative skipped {} bytes)",
                    skipped_byte,
                    self.bytes_skipped
                );
            }

            // Data Stage error: the frame boundary is correct but the data is corrupt; skip the whole frame.
            ParseError::MessageCrcMismatch { .. } | ParseError::HeaderParseFailed(_) => {
                // try to read total_length comeskipwholeframe
                if self.buffer.len() >= PRELUDE_SIZE {
                    let total_length = u32::from_be_bytes([
                        self.buffer[0],
                        self.buffer[1],
                        self.buffer[2],
                        self.buffer[3],
                    ]) as usize;

                    // ensure total_length reasonable and the buffer has enough data.
                    if total_length >= 16 && total_length <= self.buffer.len() {
                        tracing::warn!("Data error recovery: skipcorruptedframe ({} bytes)", total_length);
                        self.buffer.advance(total_length);
                        self.bytes_skipped += total_length;
                        return;
                    }
                }

                // Cannot determine the frame length; falls back to skipping byte by byte.
                let skipped_byte = self.buffer[0];
                self.buffer.advance(1);
                self.bytes_skipped += 1;
                tracing::warn!(
                    "Data error recovery (fallback): skip bytes 0x{:02x} (cumulative skipped {} bytes)",
                    skipped_byte,
                    self.bytes_skipped
                );
            }

            // other errors: skip byte by byte
            _ => {
                let skipped_byte = self.buffer[0];
                self.buffer.advance(1);
                self.bytes_skipped += 1;
                tracing::warn!(
                    "throughuseerror recovery: skip bytes 0x{:02x} (cumulative skipped {} bytes)",
                    skipped_byte,
                    self.bytes_skipped
                );
            }
        }
    }

}

/// decode iterator
pub struct DecodeIter<'a> {
    decoder: &'a mut EventStreamDecoder,
}

impl<'a> Iterator for DecodeIter<'a> {
    type Item = ParseResult<Frame>;

    fn next(&mut self) -> Option<Self::Item> {
        // if in Stopped or Recovering state, stop iterating
        match self.decoder.state {
            DecoderState::Stopped => return None,
            DecoderState::Recovering => return None,
            _ => {}
        }

        match self.decoder.decode() {
            Ok(Some(frame)) => Some(Ok(frame)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_feed() {
        let mut decoder = EventStreamDecoder::new();
        assert!(decoder.feed(&[1, 2, 3, 4]).is_ok());
    }

    #[test]
    fn test_decoder_insufficient_data() {
        let mut decoder = EventStreamDecoder::new();
        decoder.feed(&[0u8; 10]).unwrap();

        let result = decoder.decode();
        assert!(matches!(result, Ok(None)));
    }
}
