//! Filtering conformance tests.
//!
//! Tests for time range, channel, and header-based filtering.

use crate::helpers::mcap_builder::*;
use mcapable::reader;
use std::io::Cursor;

// ============================================================================
// Time Range Filtering
// ============================================================================

#[test]
fn test_time_range_filter_basic() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(0, 2, 2000, b"msg2".to_vec())
        .add_simple_message(0, 3, 3000, b"msg3".to_vec())
        .add_simple_message(0, 4, 4000, b"msg4".to_vec())
        .add_simple_message(0, 5, 5000, b"msg5".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let times: Vec<u64> = reader
        .messages()
        .unwrap()
        .time_range(2000, 4000)
        .map(|m| m.unwrap().log_time)
        .collect();

    assert_eq!(times, vec![2000, 3000, 4000]);
}

#[test]
fn test_time_range_filter_inclusive() {
    // Time range should include boundaries [start, end]
    let mcap = McapBuilder::new()
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
        .time_range(2000, 2000)
        .map(|m| m.unwrap().log_time)
        .collect();

    assert_eq!(times, vec![2000]);
}

#[test]
fn test_time_range_filter_empty_result() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(0, 2, 5000, b"msg2".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().time_range(2000, 4000).count();

    assert_eq!(count, 0);
}

#[test]
fn test_time_range_with_multiple_channels() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().time_range(1000, 2500).count();

    // Should include messages from multiple channels in range
    assert!(count > 0);
}

// ============================================================================
// Channel Filtering
// ============================================================================

#[test]
fn test_channel_filter_by_id() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let channels: Vec<u16> = reader
        .messages()
        .unwrap()
        .filter_channel(|ch| matches!(ch.id, 0 | 1))
        .map(|m| m.unwrap().channel_id)
        .collect();

    // Should only contain channels 0 and 1 (create_multi_channel_mcap creates channels 0 and 1)
    assert!(channels.iter().all(|&c| c == 0 || c == 1));
    assert!(channels.contains(&0));
    assert!(channels.contains(&1));
}

#[test]
fn test_channel_filter_single_channel() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let channels: Vec<u16> = reader
        .messages()
        .unwrap()
        .filter_channel(|ch| ch.id == 1)
        .map(|m| m.unwrap().channel_id)
        .collect();

    assert!(channels.iter().all(|&c| c == 1));
}

#[test]
fn test_channel_filter_by_topic_prefix() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/camera/left")
        .add_simple_channel(1, "/camera/right")
        .add_simple_channel(2, "/lidar/points")
        .add_simple_message(0, 1, 1000, b"left".to_vec())
        .add_simple_message(1, 1, 1000, b"right".to_vec())
        .add_simple_message(2, 1, 1000, b"lidar".to_vec())
        .build();

    let cursor = Cursor::new(mcap.clone());
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Filter by topic prefix - channel_filter works with actual channel metadata
    // Note: We can't cache channels before streaming due to &mut self requirement
    // So we collect messages first, then verify topics
    let messages: Vec<_> = reader
        .messages()
        .unwrap()
        .filter_channel(|ch| ch.topic.starts_with("/camera"))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // Verify we got the right number of messages
    assert_eq!(messages.len(), 2);

    // Verify topics match (need to check channels after iteration)
    let cursor = Cursor::new(mcap);
    let reader = reader::Builder::new().build(cursor).unwrap();
    let channels = reader.channels();
    for msg in messages {
        if let Some(channel) = channels.get(&msg.channel_id) {
            assert!(channel.topic.starts_with("/camera"));
        }
    }
}

#[test]
fn test_channel_filter_by_schema() {
    // Create channels with schemas - need to provide schema data for schema_id != 0
    let mcap = McapBuilder::new()
        .add_channel(TestChannel {
            id: 0,
            topic: "/test1".to_string(),
            message_encoding: "json".to_string(),
            schema_id: 1,
            schema_name: Some("Schema1".to_string()),
            schema_encoding: Some("ros2msg".to_string()),
            schema_data: Some(b"schema1_data".to_vec()),
        })
        .add_channel(TestChannel {
            id: 1,
            topic: "/test2".to_string(),
            message_encoding: "json".to_string(),
            schema_id: 2,
            schema_name: Some("Schema2".to_string()),
            schema_encoding: Some("ros2msg".to_string()),
            schema_data: Some(b"schema2_data".to_vec()),
        })
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(1, 1, 1000, b"msg2".to_vec())
        .build();

    let cursor = Cursor::new(mcap.clone());
    let reader = reader::Builder::new().build(cursor).unwrap();

    // Get all channels to find which one has schema_id == 1
    let channels = reader.channels();
    let has_schema_1 = channels.values().any(|ch| ch.schema_id == 1);

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();
    let count = reader
        .messages()
        .unwrap()
        .filter_channel(|ch| ch.schema_id == 1)
        .count();

    // We expect 1 message with schema_id == 1 (from channel 0)
    assert_eq!(
        count, 1,
        "Should have 1 message with schema_id == 1, got {} (has_schema_1: {})",
        count, has_schema_1
    );
}

#[test]
fn test_channel_filter_empty_result() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader
        .messages()
        .unwrap()
        .filter_channel(|ch| ch.id == 99) // Non-existent channel
        .count();

    assert_eq!(count, 0);
}

// ============================================================================
// Combined Filtering
// ============================================================================

#[test]
fn test_combined_time_and_channel_filter() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<(u16, u64)> = reader
        .messages()
        .unwrap()
        .time_range(1000, 3000)
        .filter_channel(|ch| matches!(ch.id, 0 | 1))
        .map(|m| {
            let msg = m.unwrap();
            (msg.channel_id, msg.log_time)
        })
        .collect();

    // All messages should satisfy both filters
    for (ch, time) in messages {
        assert!(ch == 0 || ch == 1);
        assert!((1000..=3000).contains(&time));
    }
}

#[test]
fn test_combined_time_channel_and_predicate() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/camera/left")
        .add_simple_channel(1, "/camera/right")
        .add_simple_channel(2, "/lidar/points")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(1, 1, 1500, b"msg2".to_vec())
        .add_simple_message(2, 1, 2000, b"msg3".to_vec())
        .add_simple_message(0, 2, 2500, b"msg4".to_vec())
        .add_simple_message(1, 2, 3000, b"msg5".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader
        .messages()
        .unwrap()
        .time_range(1000, 2500)
        .filter_channel(|ch| ch.topic.starts_with("/camera"))
        .count();

    assert_eq!(count, 3); // camera left@1000, camera right@1500, camera left@2500
}

// ============================================================================
// Header-Based Filtering
// ============================================================================

#[test]
fn test_message_header_filter_by_sequence() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(0, 2, 2000, b"msg2".to_vec())
        .add_simple_message(0, 3, 3000, b"msg3".to_vec())
        .add_simple_message(0, 4, 4000, b"msg4".to_vec())
        .add_simple_message(0, 5, 5000, b"msg5".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Sample every other message
    let sequences: Vec<u32> = reader
        .messages()
        .unwrap()
        .filter(|hdr| hdr.sequence % 2 == 1)
        .map(|m| m.unwrap().sequence)
        .collect();

    assert_eq!(sequences, vec![1, 3, 5]);
}

#[test]
fn test_message_header_filter_by_data_size() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, vec![0; 10])
        .add_simple_message(0, 2, 2000, vec![0; 100])
        .add_simple_message(0, 3, 3000, vec![0; 1000])
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Only small messages
    let count = reader
        .messages()
        .unwrap()
        .filter(|hdr| hdr.data_size < 50)
        .count();

    assert_eq!(count, 1);
}

#[test]
fn test_chunk_metadata_filter_by_compression() {
    let mcap = McapBuilder::new()
        .compression(Some(Compression::Zstd))
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let chunks: Vec<mcapable::ByteStr> = reader
        .chunks()
        .filter(|meta| meta.compression == "zstd")
        .map(|c| c.unwrap().compression)
        .collect();

    assert!(chunks.iter().all(|c| c == "zstd"));
}

#[test]
fn test_chunk_metadata_filter_by_size() {
    let mcap = McapBuilder::new()
        .chunked(true)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, vec![0; 1000])
        .add_simple_message(0, 2, 2000, vec![0; 1000])
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Filter out large chunks
    let count = reader
        .chunks()
        .filter(|meta| meta.uncompressed_size < 10000)
        .count();

    assert!(count > 0);
}

#[test]
fn test_record_type_filter() {
    // Create an unchunked MCAP so messages appear as standalone Message records
    let mcap = create_unchunked_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    use mcapable::Opcode;

    // Only message records (in unchunked files, messages appear as Message records)
    let count = reader
        .records()
        .filter(|rt| matches!(rt, Opcode::Message))
        .count();

    assert!(
        count > 0,
        "Should have at least one Message record in unchunked file"
    );
}
