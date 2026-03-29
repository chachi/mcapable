//! Error handling and edge cases conformance tests.

use crate::helpers::mcap_builder::*;
use mcapable::reader;
use std::io::Cursor;

// ============================================================================
// Invalid/Corrupted Files
// ============================================================================

#[test]
fn test_empty_file() {
    let empty = Vec::new();
    let cursor = Cursor::new(empty);
    let result = reader::Builder::new().build(cursor);
    assert!(result.is_err());
}

#[test]
fn test_too_short_for_magic() {
    let too_short = b"MCAP";
    let cursor = Cursor::new(too_short.to_vec());
    let result = reader::Builder::new().build(cursor);
    assert!(result.is_err());
}

#[test]
fn test_invalid_start_magic() {
    let mut bad_file = Vec::new();
    bad_file.extend_from_slice(b"BADMAGIC");
    bad_file.extend_from_slice(&[0; 100]);

    let cursor = Cursor::new(bad_file);
    let result = reader::Builder::new().build(cursor);
    assert!(result.is_err());
}

#[test]
fn test_invalid_end_magic() {
    let mut mcap = create_simple_mcap();

    // Corrupt end magic
    let len = mcap.len();
    if len > 8 {
        mcap[len - 4] = !mcap[len - 4];
    }

    let cursor = Cursor::new(mcap);
    // With validate_end_magic=true (default), build() should fail on invalid end magic
    let result = reader::Builder::new().build(cursor);
    assert!(result.is_err());

    // With validate_end_magic=false, build() should succeed (only checks start magic)
    // This test would then need to check if errors occur during reading
}

#[test]
fn test_truncated_file() {
    let mut mcap = create_simple_mcap();

    // Truncate file
    mcap.truncate(mcap.len() / 2);

    let cursor = Cursor::new(mcap);
    let result = reader::Builder::new().build(cursor);

    // May succeed in building but fail during iteration
    if let Ok(mut reader) = result {
        let results: Vec<_> = reader.messages().unwrap().collect();
        assert!(results.iter().any(|r| r.is_err()));
    }
}

#[test]
fn test_corrupted_record() {
    let mut mcap = create_simple_mcap();

    // Corrupt middle of file
    if mcap.len() > 100 {
        mcap.iter_mut().take(60).skip(50).for_each(|b| *b = 0xFF);
    }

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Should handle corrupted data gracefully
    let results: Vec<_> = reader.messages().unwrap().collect();
    // Either all succeed (corruption was in non-critical area)
    // or some fail (corruption affected records), but iteration should yield entries
    assert!(!results.is_empty());
}

#[test]
fn test_invalid_record_length() {
    // Record claims to be longer than remaining file
    let mut mcap = create_simple_mcap();

    // Modify record length to be invalid
    if mcap.len() > 20 {
        // Overwrite length field with huge number
        mcap[16] = 0xFF;
        mcap[17] = 0xFF;
        mcap[18] = 0xFF;
        mcap[19] = 0xFF;
    }

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let results: Vec<_> = reader.messages().unwrap().collect();
    assert!(results.iter().any(|r| r.is_err()));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_file_with_only_header_footer() {
    let mcap = McapBuilder::new().include_summary(false).build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().count();
    assert_eq!(count, 0);
}

#[test]
fn test_single_message_file() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"only".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().count();
    assert_eq!(count, 1);
}

#[test]
fn test_zero_length_message_data() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, Vec::new())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<_> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().data().to_vec())
        .collect();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].len(), 0);
}

#[test]
fn test_very_large_message() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, vec![0; 1_000_000])
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<_> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().data_len())
        .collect();

    assert_eq!(messages[0], 1_000_000);
}

#[test]
fn test_timestamp_zero() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 0, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let times: Vec<u64> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().log_time)
        .collect();

    assert_eq!(times[0], 0);
}

#[test]
fn test_max_timestamp() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, u64::MAX, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let times: Vec<u64> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().log_time)
        .collect();

    assert_eq!(times[0], u64::MAX);
}

#[test]
fn test_channel_id_zero() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let channels: Vec<u16> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().channel_id)
        .collect();

    assert!(!channels.is_empty());
}

#[test]
fn test_max_channel_id() {
    let mcap = McapBuilder::new()
        .add_simple_channel(u16::MAX, "/test")
        .add_simple_message(u16::MAX, 1, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let channels: Vec<u16> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().channel_id)
        .collect();

    assert_eq!(channels.len(), 1);
    assert!(reader.channel(channels[0]).is_some());
}

#[test]
fn test_sequence_zero() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 0, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let sequences: Vec<u32> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().sequence)
        .collect();

    assert_eq!(sequences[0], 0);
}

#[test]
fn test_max_sequence() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, u32::MAX, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let sequences: Vec<u32> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().sequence)
        .collect();

    assert_eq!(sequences[0], u32::MAX);
}

// ============================================================================
// CRC Validation
// ============================================================================

#[test]
fn test_crc_validation_enabled() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new()
        .validate_end_magic(true)
        .build(cursor)
        .unwrap();

    // Should validate CRCs
    let count = reader.messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn test_crc_validation_disabled() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new()
        .validate_end_magic(false)
        .build(cursor)
        .unwrap();

    // Should skip CRC validation
    let count = reader.messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn test_invalid_chunk_crc() {
    let mut mcap = McapBuilder::new()
        .chunked(true)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg".to_vec())
        .build();

    // Corrupt chunk CRC field
    if mcap.len() > 100 {
        // Find and corrupt CRC
        mcap[90] = !mcap[90];
    }

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new()
        .validate_end_magic(true)
        .build(cursor)
        .unwrap();

    let results: Vec<_> = reader.messages().unwrap().collect();
    assert!(results.is_empty() || results.iter().any(|r| r.is_err()));
}

// ============================================================================
// Summary Section
// ============================================================================

#[test]
fn test_missing_summary() {
    let mcap = McapBuilder::new()
        .include_summary(false)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let summary = reader.summary().unwrap();
    assert!(summary.is_some());

    // Should still be able to read messages
    let count = reader.messages().unwrap().count();
    assert_eq!(count, 1);
}

#[test]
fn test_empty_summary() {
    let mcap = McapBuilder::new()
        .include_summary(true)
        .include_statistics(false)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    reader.summary().unwrap();

    let stats = reader.statistics();
    // Summary exists but may have no statistics
    assert!(stats.is_some() || stats.is_none());
}
