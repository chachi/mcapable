//! Error types for MCAP operations.

use crate::support;
use crate::support::String;
use crate::types::Opcode;
use thiserror::Error;

/// Result type alias using our Error type.
pub type Result<T> = support::Result<T, Error>;

/// Structured parse error for MCAP records.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseError {
    /// Parsing failed for the given record opcode.
    Opcode(Opcode),
    /// Parsing failed for some other reason.
    Other(String),
}

impl support::fmt::Display for ParseError {
    fn fmt(&self, f: &mut support::fmt::Formatter<'_>) -> support::fmt::Result {
        match self {
            ParseError::Opcode(opcode) => write!(f, "{opcode:?}"),
            ParseError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl support::error::Error for ParseError {}

/// Errors that can occur when reading MCAP files.
#[derive(Error, Debug)]
pub enum Error {
    /// IO error occurred during reading.
    #[error("IO error: {0}")]
    #[cfg(feature = "std")]
    Io(#[from] std::io::Error),

    /// Invalid MCAP magic bytes.
    #[error("Invalid MCAP magic bytes")]
    InvalidMagic,

    /// Invalid or corrupted record.
    #[error("Invalid record: {0}")]
    InvalidRecord(String),

    /// Unsupported MCAP version.
    #[error("Unsupported MCAP version: {0}")]
    UnsupportedVersion(String),

    /// Invalid compression format.
    #[error("Invalid compression: {0}")]
    InvalidCompression(String),

    /// Schema not found for the given ID.
    #[error("Schema not found: {0}")]
    SchemaNotFound(u16),

    /// Channel not found for the given ID.
    #[error("Channel not found: {0}")]
    ChannelNotFound(u16),

    /// Attempted to seek to an invalid time.
    #[error("Invalid seek time: {0}")]
    InvalidSeekTime(String),

    /// Invalid or missing summary section.
    #[error("Invalid summary: {0}")]
    InvalidSummary(String),

    /// Failed to parse message data.
    #[error("Parse error while parsing {0} record")]
    ParseError(ParseError),

    /// Unexpected end of input during parsing.
    #[error("Unexpected end of input at offset {0}")]
    UnexpectedEof(u64),

    /// Invalid opcode encountered in MCAP file.
    #[error("Invalid opcode: {0:#x}")]
    InvalidOpcode(u8),

    /// CRC32 checksum mismatch.
    #[error("CRC mismatch: expected {expected:#x}, got {actual:#x}")]
    CrcMismatch {
        /// Expected CRC32 value.
        expected: u32,
        /// Actual CRC32 value computed.
        actual: u32,
    },

    /// Decompression failed.
    #[error("Decompression failed: {0}")]
    DecompressionError(String),
}

// Convert nom parsing errors into a generic InvalidRecord error when opcode context is unavailable.
impl<'a> From<nom::Err<nom::error::Error<&'a [u8]>>> for Error {
    fn from(err: nom::Err<nom::error::Error<&'a [u8]>>) -> Self {
        Error::ParseError(ParseError::Other(support::format!("{:?}", err)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Opcode;
    use nom::error::ErrorKind;

    #[test]
    fn test_error_display() {
        let err = Error::InvalidMagic;
        assert_eq!(err.to_string(), "Invalid MCAP magic bytes");

        let err = Error::UnexpectedEof(42);
        assert_eq!(err.to_string(), "Unexpected end of input at offset 42");

        let err = Error::InvalidOpcode(0xFF);
        assert_eq!(err.to_string(), "Invalid opcode: 0xff");

        let err = Error::CrcMismatch {
            expected: 0x12345678,
            actual: 0x87654321,
        };
        assert_eq!(
            err.to_string(),
            "CRC mismatch: expected 0x12345678, got 0x87654321"
        );

        let err = Error::DecompressionError("test error".to_string());
        assert_eq!(err.to_string(), "Decompression failed: test error");
    }

    #[test]
    fn test_nom_error_conversion() {
        let data = b"test";
        let nom_err = nom::Err::Error(nom::error::Error::new(data.as_slice(), ErrorKind::Tag));
        let err: Error = nom_err.into();
        assert!(matches!(err, Error::ParseError(ParseError::Other(_))));
    }

    #[test]
    fn test_error_variants() {
        // Test all error variants can be constructed
        let _ = Error::InvalidMagic;
        let _ = Error::InvalidRecord("test".to_string());
        let _ = Error::UnsupportedVersion("1.0".to_string());
        let _ = Error::InvalidCompression("unknown".to_string());
        let _ = Error::SchemaNotFound(1);
        let _ = Error::ChannelNotFound(2);
        let _ = Error::InvalidSeekTime("invalid".to_string());
        let _ = Error::InvalidSummary("corrupt".to_string());
        let _ = Error::ParseError(ParseError::Opcode(Opcode::Header));
        let _ = Error::UnexpectedEof(100);
        let _ = Error::InvalidOpcode(0x99);
        let _ = Error::CrcMismatch {
            expected: 1,
            actual: 2,
        };
        let _ = Error::DecompressionError("failed".to_string());
    }
}

#[cfg(all(test, feature = "std"))]
mod std_tests {
    use super::*;

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }
}
