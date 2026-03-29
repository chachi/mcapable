//! Compression and CRC utilities for MCAP.
//!
//! This module provides decompression support for MCAP's supported
//! compression algorithms (LZ4, Zstd) and CRC32 validation.

use crate::error::{Error, Result};
use crate::support::ToString;
use crate::support::str::FromStr;

#[cfg(feature = "std")]
mod std;

#[cfg(feature = "std")]
pub use std::decompress;

#[cfg(feature = "std")]
pub(crate) fn compress_chunk_records(
    compression: &Compression,
    message_prefixes: &[u8],
    payloads: &[bytes::Bytes],
) -> Result<Vec<u8>> {
    std::compress_chunk_records(compression, message_prefixes, payloads)
}

/// Compression algorithm used for chunk data.
#[derive(Debug, Clone, PartialEq, Eq, strum::Display, strum::EnumString)]
pub enum Compression {
    /// LZ4 compression.
    #[cfg(feature = "lz4")]
    #[strum(serialize = "lz4")]
    Lz4,
    /// Zstandard compression.
    #[cfg(feature = "zstd")]
    #[strum(serialize = "zstd")]
    Zstd,
}

/// Parse compression algorithm from string.
///
/// MCAP uses empty string or "none" for no compression.
///
/// # Examples
///
/// ```
/// use mcapable_core::compression::{Compression, parse_compression};
///
/// let none = parse_compression("").unwrap();
/// assert_eq!(none, None);
///
/// let lz4 = parse_compression("lz4").unwrap();
/// assert_eq!(lz4, Some(Compression::Lz4));
///
/// let zstd = parse_compression("zstd").unwrap();
/// assert_eq!(zstd, Some(Compression::Zstd));
/// ```
pub fn parse_compression(s: &str) -> Result<Option<Compression>> {
    if s.is_empty() || s == "none" {
        return Ok(None);
    }
    Compression::from_str(s)
        .map(Some)
        .map_err(|_| Error::InvalidCompression(s.to_string()))
}

/// Calculate CRC32 checksum of data.
///
/// Uses the CRC32 algorithm as specified by MCAP.
///
/// # Examples
///
/// ```
/// use mcapable_core::compression::calculate_crc;
///
/// let data = b"hello world";
/// let crc = calculate_crc(data);
/// assert_eq!(crc, 0x0d4a1185);
/// ```
pub fn calculate_crc(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

/// Verify that CRC32 checksum matches expected value.
///
/// # Arguments
///
/// * `data` - The data to checksum
/// * `expected` - The expected CRC32 value
///
/// # Returns
///
/// `Ok(())` if checksums match, or `Error::CrcMismatch` if they don't.
///
/// # Examples
///
/// ```
/// use mcapable_core::compression::{calculate_crc, verify_crc};
///
/// let data = b"hello world";
/// let crc = calculate_crc(data);
///
/// // Should succeed
/// assert!(verify_crc(data, crc).is_ok());
///
/// // Should fail
/// assert!(verify_crc(data, 0xdeadbeef).is_err());
/// ```
pub fn verify_crc(data: &[u8], expected: u32) -> Result<()> {
    let actual = calculate_crc(data);
    if actual == expected {
        Ok(())
    } else {
        Err(Error::CrcMismatch { expected, actual })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_from_string_none() {
        assert_eq!(parse_compression("").unwrap(), None);
        assert_eq!(parse_compression("none").unwrap(), None);
    }

    #[test]
    fn test_compression_from_string_invalid() {
        assert!(parse_compression("invalid").is_err());
        assert!(parse_compression("gzip").is_err());
    }

    #[test]
    fn test_calculate_crc() {
        // Known CRC32 values (using crc32fast algorithm)
        assert_eq!(calculate_crc(b""), 0);
        assert_eq!(calculate_crc(b"hello world"), 0x0d4a1185);
        assert_eq!(
            calculate_crc(b"The quick brown fox jumps over the lazy dog"),
            0x414fa339
        );
    }

    #[test]
    fn test_verify_crc_success() {
        let data = b"test data";
        let crc = calculate_crc(data);
        assert!(verify_crc(data, crc).is_ok());
    }

    #[test]
    fn test_verify_crc_failure() {
        let data = b"test data";
        let result = verify_crc(data, 0xdeadbeef);
        assert!(result.is_err());

        match result.unwrap_err() {
            Error::CrcMismatch { expected, actual } => {
                assert_eq!(expected, 0xdeadbeef);
                assert_eq!(actual, calculate_crc(data));
            }
            _ => panic!("Expected CrcMismatch error"),
        }
    }

    #[test]
    fn test_crc_different_data() {
        let data1 = b"hello";
        let data2 = b"world";
        let crc1 = calculate_crc(data1);
        let crc2 = calculate_crc(data2);

        // Different data should produce different CRCs
        assert_ne!(crc1, crc2);
    }

    #[test]
    fn test_crc_empty_data() {
        let empty: &[u8] = &[];
        let crc = calculate_crc(empty);
        assert_eq!(crc, 0);
    }
}

#[cfg(all(test, feature = "lz4"))]
mod lz4_tests {
    use super::*;

    #[test]
    fn test_compression_from_string_lz4() {
        assert_eq!(parse_compression("lz4").unwrap(), Some(Compression::Lz4));
    }

    #[test]
    fn test_compression_to_string_lz4() {
        assert_eq!(Compression::Lz4.to_string(), "lz4");
    }
}

#[cfg(all(test, feature = "zstd"))]
mod zstd_tests {
    use super::*;

    #[test]
    fn test_compression_from_string_zstd() {
        assert_eq!(parse_compression("zstd").unwrap(), Some(Compression::Zstd));
    }

    #[test]
    fn test_compression_to_string_zstd() {
        assert_eq!(Compression::Zstd.to_string(), "zstd");
    }
}
