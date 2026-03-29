//! Low-level record iteration utilities.
//!
//! This module provides building blocks for iterating through MCAP records,
//! both at the file level and within decompressed chunk data.
//!
//! These are intentionally simple, stateless functions that can be thoroughly
//! tested in isolation.

use crate::error::Result as CoreResult;
#[cfg(test)]
use crate::format::MESSAGE_HEADER_SIZE;
use crate::format::RECORD_HEADER_SIZE;
use crate::parser::record_header;
use crate::types::Opcode;

/// Result of reading a record header from a buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordHeader {
    /// The opcode identifying the record type
    pub opcode: u8,
    /// The length of the record content (not including the 9-byte header)
    pub length: u64,
}

impl RecordHeader {
    /// Convert the raw opcode byte into a typed [`Opcode`], if valid.
    pub fn opcode_typed(&self) -> Option<Opcode> {
        Opcode::from_repr(self.opcode)
    }
}

/// Decode a record header buffer into a typed opcode + content length.
pub fn decode_record_header(header_bytes: &[u8]) -> CoreResult<(Opcode, u64)> {
    let (_, (opcode_raw, length)) = record_header(header_bytes)?;
    let opcode = Opcode::from_repr(opcode_raw).ok_or(crate::Error::InvalidOpcode(opcode_raw))?;
    Ok((opcode, length))
}

/// Decode a record header buffer, returning `Ok(None)` for invalid opcodes.
pub fn try_decode_record_header(header_bytes: &[u8]) -> CoreResult<Option<(Opcode, u64)>> {
    let (_, (opcode_raw, length)) = record_header(header_bytes)?;
    let Some(opcode) = Opcode::from_repr(opcode_raw) else {
        return Ok(None);
    };
    Ok(Some((opcode, length)))
}

/// Information about a record's position in a buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLocation {
    /// Position of the record header (opcode byte)
    pub header_start: usize,
    /// Position where the record content starts (after header)
    pub content_start: usize,
    /// Position where the next record starts (or end of buffer)
    pub next_record_start: usize,
    /// The record header
    pub header: RecordHeader,
}

/// Error type for record iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    /// Buffer too small to contain a record header
    BufferTooSmall { needed: usize, available: usize },
    /// Record content extends beyond buffer
    RecordTruncated {
        content_start: usize,
        content_length: u64,
        buffer_length: usize,
    },
    /// Invalid record header
    InvalidHeader,
}

/// Read a record header from a buffer at the given position.
///
/// Returns the header and the position where the content starts.
pub fn read_header_at(data: &[u8], position: usize) -> Result<RecordHeader, RecordError> {
    // Check if we have enough bytes for the header
    if position + RECORD_HEADER_SIZE > data.len() {
        return Err(RecordError::BufferTooSmall {
            needed: RECORD_HEADER_SIZE,
            available: data.len().saturating_sub(position),
        });
    }

    let header_bytes = &data[position..position + RECORD_HEADER_SIZE];
    let (_, (opcode, length)) =
        record_header(header_bytes).map_err(|_| RecordError::InvalidHeader)?;

    Ok(RecordHeader { opcode, length })
}

/// Get the full location information for a record at the given position.
///
/// This validates that the entire record content is available in the buffer.
pub fn get_record_location(data: &[u8], position: usize) -> Result<RecordLocation, RecordError> {
    let header = read_header_at(data, position)?;

    let content_start = position + RECORD_HEADER_SIZE;
    let next_record_start = content_start + header.length as usize;

    // Verify the content is fully contained in the buffer
    if next_record_start > data.len() {
        return Err(RecordError::RecordTruncated {
            content_start,
            content_length: header.length,
            buffer_length: data.len(),
        });
    }

    Ok(RecordLocation {
        header_start: position,
        content_start,
        next_record_start,
        header,
    })
}

/// Iterator over records in a byte buffer.
///
/// This is a simple, stateless iterator that yields record locations.
pub struct RecordIterator<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> RecordIterator<'a> {
    /// Create a new iterator starting at the given position.
    pub fn new(data: &'a [u8], start_position: usize) -> Self {
        Self {
            data,
            position: start_position,
        }
    }

    /// Get the current position in the buffer.
    #[cfg(test)]
    pub fn position(&self) -> usize {
        self.position
    }

    /// Check if there's more data to read.
    #[cfg(test)]
    pub fn has_more(&self) -> bool {
        self.position < self.data.len()
    }
}

impl<'a> Iterator for RecordIterator<'a> {
    type Item = Result<RecordLocation, RecordError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.data.len() {
            return None;
        }

        // Try to get the next record
        match get_record_location(self.data, self.position) {
            Ok(location) => {
                self.position = location.next_record_start;
                Some(Ok(location))
            }
            Err(e) => {
                // Stop iteration on error
                self.position = self.data.len();
                Some(Err(e))
            }
        }
    }
}

/// Get the content slice for a record.
#[cfg(test)]
pub fn get_record_content<'a>(data: &'a [u8], location: &RecordLocation) -> &'a [u8] {
    &data[location.content_start..location.next_record_start]
}

/// Iterator specifically designed for iterating over records in decompressed chunk data.
///
/// This is optimized for the common chunk iteration pattern where we want to
/// iterate through messages in a decompressed chunk buffer. Unlike `RecordIterator`,
/// this yields `(Opcode, &[u8])` pairs for valid records and silently skips invalid opcodes.
///
/// # Example
/// ```
/// # use mcapable_core::records::{ChunkRecordIterator, RECORD_HEADER_SIZE};
/// # let chunk_data = vec![0u8; 100];  // Dummy chunk data
/// for result in ChunkRecordIterator::new(&chunk_data) {
///     match result {
///         Ok((opcode, data)) => {
///             // Process record with opcode and data
///         }
///         Err(e) => {
///             // Handle error (truncated record, etc.)
///             break;
///         }
///     }
/// }
/// ```
pub struct ChunkRecordIterator<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> ChunkRecordIterator<'a> {
    /// Create a new chunk record iterator.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// Get the current position in the buffer.
    #[cfg(test)]
    pub fn position(&self) -> usize {
        self.position
    }
}

impl<'a> Iterator for ChunkRecordIterator<'a> {
    type Item = Result<(crate::types::Opcode, &'a [u8]), RecordError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if we have enough space for a header
        if self.position + RECORD_HEADER_SIZE > self.data.len() {
            return None;
        }

        // Parse header
        let header_bytes = &self.data[self.position..self.position + RECORD_HEADER_SIZE];
        let (opcode_raw, length) = match crate::parser::record_header(header_bytes) {
            Ok((_, (opcode, len))) => (opcode, len),
            Err(_) => {
                // Invalid header - stop iteration
                self.position = self.data.len();
                return None;
            }
        };

        // Convert opcode
        let opcode = match crate::types::Opcode::from_repr(opcode_raw) {
            Some(op) => op,
            None => {
                // Invalid opcode - skip this record
                self.position += RECORD_HEADER_SIZE + length as usize;
                return self.next(); // Recursively get next valid record
            }
        };

        // Move past header
        self.position += RECORD_HEADER_SIZE;

        // Check if we have enough data for the record content
        let length_usize = length as usize;
        if self.position + length_usize > self.data.len() {
            // Truncated record
            return Some(Err(RecordError::RecordTruncated {
                content_start: self.position,
                content_length: length,
                buffer_length: self.data.len(),
            }));
        }

        // Extract data slice
        let data_slice = &self.data[self.position..self.position + length_usize];
        self.position += length_usize;

        Some(Ok((opcode, data_slice)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_header_at_valid() {
        // Opcode 0x05 (Message), length 23
        let data = [
            0x05, // opcode
            0x17, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, // length = 23
                  // ... content would follow
        ];

        let header = read_header_at(&data, 0).unwrap();
        assert_eq!(header.opcode, 0x05);
        assert_eq!(header.length, 23);
    }

    #[test]
    fn test_read_header_at_buffer_too_small() {
        let data = [0x05, 0x17, 0x00]; // Only 3 bytes

        let result = read_header_at(&data, 0);
        assert!(matches!(result, Err(RecordError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_read_header_at_offset() {
        // Some prefix data, then a record
        let data = [
            0xFF, 0xFF, 0xFF, // 3 bytes prefix
            0x04, // opcode = Channel
            0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // length = 16
        ];

        let header = read_header_at(&data, 3).unwrap();
        assert_eq!(header.opcode, 0x04);
        assert_eq!(header.length, 16);
    }

    #[test]
    fn test_get_record_location_valid() {
        // Complete record: opcode + length + content
        let data = [
            0x05, // opcode
            0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // length = 3
            0xAA, 0xBB, 0xCC, // 3 bytes content
        ];

        let location = get_record_location(&data, 0).unwrap();
        assert_eq!(location.header_start, 0);
        assert_eq!(location.content_start, 9);
        assert_eq!(location.next_record_start, 12);
        assert_eq!(location.header.opcode, 0x05);
        assert_eq!(location.header.length, 3);
    }

    #[test]
    fn test_get_record_location_truncated() {
        // Record header says length=100, but we only have 3 bytes of content
        let data = [
            0x05, // opcode
            0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // length = 100
            0xAA, 0xBB, 0xCC, // only 3 bytes
        ];

        let result = get_record_location(&data, 0);
        assert!(matches!(result, Err(RecordError::RecordTruncated { .. })));
    }

    #[test]
    fn test_record_iterator_multiple_records() {
        // Two complete records
        let data = [
            // Record 1: opcode=0x04, length=2
            0x04, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xAA, 0xBB,
            // Record 2: opcode=0x05, length=3
            0x05, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xCC, 0xDD, 0xEE,
        ];

        let mut iter = RecordIterator::new(&data, 0);

        // First record
        let loc1 = iter.next().unwrap().unwrap();
        assert_eq!(loc1.header.opcode, 0x04);
        assert_eq!(loc1.header.length, 2);
        assert_eq!(loc1.header_start, 0);
        assert_eq!(loc1.content_start, 9);
        assert_eq!(loc1.next_record_start, 11);

        // Second record
        let loc2 = iter.next().unwrap().unwrap();
        assert_eq!(loc2.header.opcode, 0x05);
        assert_eq!(loc2.header.length, 3);
        assert_eq!(loc2.header_start, 11);
        assert_eq!(loc2.content_start, 20);
        assert_eq!(loc2.next_record_start, 23);

        // No more records
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_record_iterator_position_tracking() {
        let data = [
            0x04, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xAA, 0xBB,
        ];

        let mut iter = RecordIterator::new(&data, 0);
        assert_eq!(iter.position(), 0);
        assert!(iter.has_more());

        iter.next();
        assert_eq!(iter.position(), 11);
        assert!(!iter.has_more());
    }

    #[test]
    fn test_get_record_content() {
        let data = [
            0x05, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xAA, 0xBB, 0xCC,
        ];

        let location = get_record_location(&data, 0).unwrap();
        let content = get_record_content(&data, &location);
        assert_eq!(content, &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn test_constants() {
        assert_eq!(RECORD_HEADER_SIZE, 9);
        assert_eq!(MESSAGE_HEADER_SIZE, 22);
    }

    #[test]
    fn test_chunk_record_iterator_empty() {
        let data: &[u8] = &[];
        let mut iter = super::ChunkRecordIterator::new(data);
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_chunk_record_iterator_single_record() {
        // Single message record (opcode 0x05, length 3)
        let data = [
            0x05, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // header
            0xAA, 0xBB, 0xCC, // data
        ];

        let mut iter = super::ChunkRecordIterator::new(&data);

        let result = iter.next().unwrap().unwrap();
        assert_eq!(result.0.as_u8(), 0x05);
        assert_eq!(result.1, &[0xAA, 0xBB, 0xCC]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_chunk_record_iterator_multiple_records() {
        // Two records
        let data = [
            // Record 1: opcode 0x04 (channel), length 2
            0x04, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xAA, 0xBB,
            // Record 2: opcode 0x05 (message), length 3
            0x05, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xCC, 0xDD, 0xEE,
        ];

        let mut iter = super::ChunkRecordIterator::new(&data);

        // First record
        let result1 = iter.next().unwrap().unwrap();
        assert_eq!(result1.0.as_u8(), 0x04);
        assert_eq!(result1.1, &[0xAA, 0xBB]);

        // Second record
        let result2 = iter.next().unwrap().unwrap();
        assert_eq!(result2.0.as_u8(), 0x05);
        assert_eq!(result2.1, &[0xCC, 0xDD, 0xEE]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_chunk_record_iterator_truncated_header() {
        // Incomplete header (only 5 bytes instead of 9)
        let data = [0x05, 0x03, 0x00, 0x00, 0x00];
        let mut iter = super::ChunkRecordIterator::new(&data);
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_chunk_record_iterator_truncated_content() {
        // Header says length 10, but only 3 bytes of content
        let data = [
            0x05, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // length = 10
            0xAA, 0xBB, 0xCC, // only 3 bytes
        ];

        let mut iter = super::ChunkRecordIterator::new(&data);
        let result = iter.next().unwrap();
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(super::RecordError::RecordTruncated { .. })
        ));
    }

    #[test]
    fn test_chunk_record_iterator_invalid_opcode_skipped() {
        // First record has invalid opcode (0xFF), second is valid
        let data = [
            // Record 1: invalid opcode 0xFF, length 2
            0xFF, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xAA, 0xBB,
            // Record 2: valid opcode 0x05, length 3
            0x05, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xCC, 0xDD, 0xEE,
        ];

        let mut iter = super::ChunkRecordIterator::new(&data);

        // Should skip invalid opcode and return the valid one
        let result = iter.next().unwrap().unwrap();
        assert_eq!(result.0.as_u8(), 0x05);
        assert_eq!(result.1, &[0xCC, 0xDD, 0xEE]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_chunk_record_iterator_position_tracking() {
        let data = [
            0x05, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xAA, 0xBB, 0xCC,
        ];

        let mut iter = super::ChunkRecordIterator::new(&data);
        assert_eq!(iter.position(), 0);

        iter.next();
        assert_eq!(iter.position(), 12); // 9 (header) + 3 (data)
    }

    #[test]
    fn decode_record_header_rejects_invalid_opcode() {
        let header_bytes = [
            0xFF, // invalid opcode
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // length = 1
        ];

        let err = decode_record_header(&header_bytes).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidOpcode(0xFF)));
        assert_eq!(try_decode_record_header(&header_bytes).unwrap(), None);
    }

    // `decode_record_header_at_offset` was removed; offsets are tracked by stream iterators.
}
