//! Stream types conformance tests.
//!
//! Tests for different stream types: records, chunks, raw_messages, messages.

use crate::helpers::mcap_builder::*;
use mcapable::reader;
use std::io::Cursor;

// ============================================================================
// Record Stream Tests
// ============================================================================

#[test]
fn test_record_stream() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.records().count();
    assert!(count > 0);
}

#[test]
fn test_record_stream_types() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let mut has_header = false;
    let mut has_chunk = false;

    for record_result in reader.records() {
        let record = record_result.unwrap();
        match record {
            mcapable::Record::Header(_) => has_header = true,
            // In chunked files, messages are inside chunks
            mcapable::Record::Chunk(_) => has_chunk = true,
            _ => {}
        }
    }

    assert!(has_header);
    // Since create_simple_mcap() is chunked, we should see chunks not messages
    assert!(has_chunk);
}

// ============================================================================
// Chunk Stream Tests
// ============================================================================

#[test]
fn test_chunk_stream() {
    let mcap = McapBuilder::new()
        .chunked(true)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, vec![0; 100])
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let chunk_count = reader.chunks().count();
    assert!(chunk_count > 0);
}

#[test]
fn test_chunk_stream_data() {
    let mcap = McapBuilder::new()
        .chunked(true)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"data".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    for chunk_result in reader.chunks() {
        let chunk = chunk_result.unwrap();
        // Chunk should have records
        assert!(!chunk.records.is_empty());
        // Should have time bounds
        assert!(chunk.message_start_time <= chunk.message_end_time);
    }
}

// ============================================================================
// RawMessage Stream Tests
// ============================================================================

#[test]
fn test_raw_message_stream() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.raw_messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn test_raw_message_no_metadata_lookup() {
    // RawMessage should not require channel/schema lookup
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    for raw_msg_result in reader.raw_messages().unwrap() {
        let raw_msg = raw_msg_result.unwrap();
        // Should have basic fields
        // channel_id is u16, data.len() is usize - both always >= 0
        assert!(raw_msg.channel_id < u16::MAX);
        assert!(raw_msg.data_len() < usize::MAX);
    }
}

#[test]
fn test_raw_message_vs_message_count() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap.clone());
    let mut reader1 = reader::Builder::new().build(cursor).unwrap();
    let raw_count = reader1.raw_messages().unwrap().count();

    let cursor = Cursor::new(mcap);
    let mut reader2 = reader::Builder::new().build(cursor).unwrap();
    let msg_count = reader2.messages().unwrap().count();

    // Should return same number of messages
    assert_eq!(raw_count, msg_count);
}

// ============================================================================
// Message Stream Tests
// ============================================================================

#[test]
fn test_message_stream() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn test_message_with_metadata() {
    let mcap = McapBuilder::new()
        .add_channel(TestChannel {
            id: 0,
            topic: "/test/topic".to_string(),
            message_encoding: "json".to_string(),
            schema_id: 1,
            schema_name: Some("TestSchema".to_string()),
            schema_encoding: Some("jsonschema".to_string()),
            schema_data: Some(b"{}".to_vec()),
        })
        .add_simple_message(0, 1, 1000, b"data".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    for msg_result in reader.messages().unwrap() {
        let msg = msg_result.unwrap();
        // Verify message fields
        assert_eq!(msg.channel_id, 0);
        assert_eq!(msg.sequence, 1);
        assert_eq!(msg.log_time, 1000);
        assert_eq!(msg.data(), b"data");
    }

    // Channels should be cached
    let channel = reader.channel(0).unwrap();
    assert_eq!(channel.topic, "/test/topic");
}

// ============================================================================
// Multiple Concurrent Streams
// ============================================================================

#[test]
fn test_multiple_message_streams() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Create multiple streams from same reader
    let count1 = reader.messages().unwrap().count();
    let count2 = reader.messages().unwrap().count();

    // Should return same count
    assert_eq!(count1, count2);
}

#[test]
fn test_different_stream_types_same_reader() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Different stream types from same reader
    let record_count = reader.records().count();
    let message_count = reader.messages().unwrap().count();
    let raw_count = reader.raw_messages().unwrap().count();

    assert!(record_count >= message_count);
    assert_eq!(message_count, raw_count);
}

#[test]
fn test_filtered_vs_unfiltered_streams() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let total_count = reader.messages().unwrap().count();
    let filtered_count = reader
        .messages()
        .unwrap()
        .filter_channel(|ch| ch.id == 0)
        .count();

    assert!(filtered_count < total_count);
    assert!(filtered_count > 0);
}

// ============================================================================
// Stream Filtering Combinations
// ============================================================================

#[test]
fn test_stream_filter_chaining() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader
        .messages()
        .unwrap()
        .time_range(1000, 3000)
        .filter_channel(|ch| matches!(ch.id, 0 | 1))
        .filter(|hdr| hdr.sequence > 0)
        .count();

    // All filters should be applied
    assert!(count > 0);
}

#[test]
fn test_empty_filter_result() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Filter that excludes everything
    let count = reader.messages().unwrap().time_range(0, 0).count();

    assert_eq!(count, 0);
}

// ============================================================================
// Stream Reuse
// ============================================================================

#[test]
fn test_stream_exhaustion_and_reuse() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Exhaust first stream
    {
        let mut iter1 = reader.messages().unwrap();
        while iter1.next().is_some() {}
    } // iter1 dropped here, releasing mutable borrow

    // Create new stream
    let count = reader.messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn test_partial_iteration() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(0, 2, 2000, b"msg2".to_vec())
        .add_simple_message(0, 3, 3000, b"msg3".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Partially iterate
    {
        let mut iter = reader.messages().unwrap();
        let _first = iter.next().unwrap();
        // iter dropped at end of scope
    }

    // Create new stream and fully iterate
    let count = reader.messages().unwrap().count();
    assert_eq!(count, 3);
}
