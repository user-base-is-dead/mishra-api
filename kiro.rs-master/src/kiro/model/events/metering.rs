//! billing event
//!
//! Kiro upstream meteringEvent payload like `{"unit":"credit","unitPlural":"credits","usage":<f64>}`,
//! `usage` is what this request consumed credit count. The relay layer accumulates each time window accordingly. credit total.
//!
//! upstream **do not dispatch** token / cache field (measured and confirmed), so here**only**parse `usage`,
//! Does not do any field name candidate compatibility; a parse failure is handled directly by ParseError throw up.

use serde::Deserialize;

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// billing event payload
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeteringEvent {
    /// what this request consumed credit count (consistent with the billing unit, floating point).
    #[serde(default)]
    pub usage: f64,
}

impl EventPayload for MeteringEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_real_payload_shape() {
        // from real packet capture: only contains unit / unitPlural / usage
        let v: MeteringEvent = serde_json::from_str(
            r#"{"unit":"credit","unitPlural":"credits","usage":0.0169543708291874}"#,
        )
        .unwrap();
        assert!((v.usage - 0.0169543708291874).abs() < 1e-12);
    }

    #[test]
    fn missing_usage_is_zero() {
        let v: MeteringEvent =
            serde_json::from_str(r#"{"unit":"credit"}"#).unwrap();
        assert_eq!(v.usage, 0.0);
    }
}
