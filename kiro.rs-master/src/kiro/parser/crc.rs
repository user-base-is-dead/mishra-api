//! CRC32 validation implementation
//!
//! AWS Event Stream use CRC32 (ISO-HDLC/ethernet/ZIP standard)

use crc::{CRC_32_ISO_HDLC, Crc};

/// CRC32 computecomponentinstance (ISO-HDLC standard,polynomial 0xEDB88320)
const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

/// compute CRC32 checksum (ISO-HDLC standard)
///
/// # Arguments
/// * `data` - the data to compute the checksum
///
/// # Returns
/// CRC32 checksum value
pub fn crc32(data: &[u8]) -> u32 {
    CRC32.checksum(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_empty() {
        // empty data CRC32 should be 0
        assert_eq!(crc32(&[]), 0);
    }

    #[test]
    fn test_crc32_known_value() {
        // "123456789" of CRC32 (ISO-HDLC) value is 0xCBF43926
        let data = b"123456789";
        assert_eq!(crc32(data), 0xCBF43926);
    }
}
