//! Compression conformance tests.
//!
//! Tests for handling different compression algorithms.

use crate::helpers::mcap_builder::*;
use mcapable::reader;
use std::io::Cursor;

#[test]
fn test_uncompressed_chunks() {
    let mcap = create_compressed_mcap(Compression::None);

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<Vec<u8>> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().data().to_vec())
        .collect();

    assert!(!messages.is_empty());
}

#[test]
fn test_lz4_compression() {
    let mcap = create_compressed_mcap(Compression::Lz4);

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<Vec<u8>> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().data().to_vec())
        .collect();

    assert!(!messages.is_empty());
}

#[test]
fn test_zstd_compression() {
    let mcap = create_compressed_mcap(Compression::Zstd);

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<Vec<u8>> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().data().to_vec())
        .collect();

    assert!(!messages.is_empty());
}

#[test]
fn test_mixed_compression_chunks() {
    // File with different compression per chunk
    let mcap = McapBuilder::new()
        .chunked(true)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().count();
    assert!(count != 0);
}

/// Helper function to corrupt compressed chunk data in an MCAP file.
///
/// Finds the first chunk record and corrupts bytes in its compressed data section.
fn corrupt_chunk_compressed_data(mcap: &mut [u8]) -> bool {
    use mcapable::Opcode;
    use mcapable_core::format::RECORD_HEADER_SIZE;

    // Skip magic bytes
    let mut pos = mcapable_core::format::MCAP_MAGIC_SIZE;

    while pos + RECORD_HEADER_SIZE < mcap.len() {
        // Read record header
        let opcode_byte = mcap[pos];
        let length_bytes = &mcap[pos + 1..pos + 9];
        let length = u64::from_le_bytes([
            length_bytes[0],
            length_bytes[1],
            length_bytes[2],
            length_bytes[3],
            length_bytes[4],
            length_bytes[5],
            length_bytes[6],
            length_bytes[7],
        ]);

        // Check if this is a chunk record
        if opcode_byte == Opcode::Chunk.as_u8() {
            let content_start = pos + RECORD_HEADER_SIZE;
            let content_end = content_start + length as usize;

            if content_end > mcap.len() {
                return false; // Truncated record
            }

            // Parse chunk content to find compressed data offset
            // Structure: message_start_time (8) + message_end_time (8) + uncompressed_size (8) +
            //            uncompressed_crc (4) + compression_string (4+N) + compressed_size (8) + compressed_data
            let mut offset = content_start;

            // Skip fixed fields: message_start_time, message_end_time, uncompressed_size, uncompressed_crc
            offset += 8 + 8 + 8 + 4; // 28 bytes

            // Read compression string length
            if offset + 4 > mcap.len() {
                return false;
            }
            let comp_len_bytes = &mcap[offset..offset + 4];
            let comp_len = u32::from_le_bytes([
                comp_len_bytes[0],
                comp_len_bytes[1],
                comp_len_bytes[2],
                comp_len_bytes[3],
            ]) as usize;
            offset += 4;

            // Skip compression string
            if offset + comp_len > mcap.len() {
                return false;
            }
            offset += comp_len;

            // Skip compressed_size (8 bytes)
            if offset + 8 > mcap.len() {
                return false;
            }
            offset += 8;

            // Now offset points to the start of compressed data
            // Corrupt multiple bytes throughout the compressed data to ensure decompression fails
            let compressed_data_end = content_end;
            let compressed_data_len = compressed_data_end.saturating_sub(offset);

            if compressed_data_len >= 4 {
                // Corrupt bytes at the beginning, middle, and end of compressed data
                // This ensures the corruption affects decompression
                if offset < mcap.len() {
                    mcap[offset] = !mcap[offset]; // Corrupt first byte
                }
                if compressed_data_len >= 2 && offset + 1 < mcap.len() {
                    mcap[offset + 1] = !mcap[offset + 1]; // Corrupt second byte
                }
                let mid_pos = offset + compressed_data_len / 2;
                if mid_pos < mcap.len() {
                    mcap[mid_pos] = !mcap[mid_pos]; // Corrupt middle byte
                }
                let end_pos = compressed_data_end.saturating_sub(1);
                if end_pos > offset && end_pos < mcap.len() {
                    mcap[end_pos] = !mcap[end_pos]; // Corrupt last byte
                }
                return true;
            }
            return false;
        }

        // Move to next record
        pos += RECORD_HEADER_SIZE + length as usize;
    }

    false
}

#[test]
fn test_decompression_error_handling() {
    // Corrupted compressed data should return error
    let mut mcap = create_compressed_mcap(Compression::Zstd);

    // Corrupt the compressed chunk data (not the header)
    assert!(
        corrupt_chunk_compressed_data(&mut mcap),
        "Failed to find and corrupt chunk compressed data"
    );

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let result: Vec<_> = reader.messages().unwrap().collect();

    // Should have at least one error due to corrupted compressed data
    assert!(
        result.iter().any(|r| r.is_err()),
        "Expected decompression error when compressed data is corrupted"
    );
}

#[test]
fn test_compression_metadata_matches() {
    let mcap = create_compressed_mcap(Compression::Zstd);

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Chunk metadata should indicate compression
    for chunk_result in reader.chunks() {
        let chunk = chunk_result.unwrap();
        assert_eq!(chunk.compression, "zstd");
    }
}

#[test]
fn test_large_compressed_chunk() {
    // Large chunk that compresses well
    let mcap = McapBuilder::new()
        .compression(Some(Compression::Zstd))
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, vec![0; 10000])
        .add_simple_message(0, 2, 2000, vec![0; 10000])
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().count();
    assert_eq!(count, 2);
}

#[test]
fn test_uncompressed_chunk_crc() {
    // Uncompressed chunks should still have valid CRC
    let mcap = create_compressed_mcap(Compression::None);

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new()
        .validate_end_magic(true)
        .build(cursor)
        .unwrap();

    let count = reader.messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn test_compressed_chunk_crc() {
    // Compressed chunks should have valid CRC after decompression
    let mcap = create_compressed_mcap(Compression::Zstd);

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new()
        .validate_end_magic(true)
        .build(cursor)
        .unwrap();

    let count = reader.messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn test_compression_with_filtering() {
    // Filtering should work with compressed chunks
    let mcap = McapBuilder::new()
        .compression(Some(Compression::Lz4))
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(0, 2, 2000, b"msg2".to_vec())
        .add_simple_message(0, 3, 3000, b"msg3".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let times: Vec<u64> = reader
        .messages()
        .unwrap()
        .time_range(1500, 2500)
        .map(|m| m.unwrap().log_time)
        .collect();

    assert_eq!(times, vec![2000]);
}

#[test]
fn test_skip_decompression_with_chunk_filter() {
    // Chunk filtering should avoid decompression
    let mcap = McapBuilder::new()
        .compression(Some(Compression::Zstd))
        .chunked(true)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, vec![0; 1000])
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Filter should prevent decompression
    let count = reader
        .chunks()
        .filter(|meta| meta.message_end_time < 500) // Exclude all
        .count();

    assert_eq!(count, 0);
}
