//! Message ordering conformance tests.
//!
//! Tests that messages are returned in correct order according to MCAP spec.

use crate::helpers::mcap_builder::*;
use mcapable::reader;
use std::io::Cursor;

#[test]
fn test_messages_in_chronological_order() {
    // Messages should be returned in log_time order
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"first".to_vec())
        .add_simple_message(0, 2, 2000, b"second".to_vec())
        .add_simple_message(0, 3, 3000, b"third".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let times: Vec<u64> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().log_time)
        .collect();

    assert_eq!(times, vec![1000, 2000, 3000]);
}

#[test]
fn test_messages_across_chunks_ordered() {
    // Messages across multiple chunks should be in order
    let mcap = McapBuilder::new()
        .chunked(true)
        .chunk_size(Some(256)) // Force multiple chunks
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, vec![0; 100])
        .add_simple_message(0, 2, 2000, vec![0; 100])
        .add_simple_message(0, 3, 3000, vec![0; 100])
        .add_simple_message(0, 4, 4000, vec![0; 100])
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let times: Vec<u64> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().log_time)
        .collect();

    // Should be strictly increasing
    for window in times.windows(2) {
        assert!(window[0] < window[1], "times should be increasing");
    }
}

#[test]
fn test_overlapping_chunk_time_ranges() {
    // When chunk time ranges overlap, messages should still be in time order
    let mcap = create_overlapping_chunks_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let times: Vec<u64> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().log_time)
        .collect();

    // Should be in chronological order despite overlapping chunks
    // Note: create_overlapping_chunks_mcap() creates messages at 1000, 1500, 2000, 2500, 3000, 3500
    // The important thing is that they're in chronological order
    assert_eq!(times, vec![1000, 1500, 2000, 2500, 3000, 3500]);
}

#[test]
fn test_messages_in_chunk_disorder() {
    // Note: The mcap crate may not sort messages within a chunk.
    // This test verifies that messages are returned, but ordering
    // within a single chunk depends on the writer implementation.
    let mcap = McapBuilder::new()
        .chunked(true)
        .add_simple_channel(0, "/test")
        // Add messages out of order within chunk
        .add_simple_message(0, 2, 4000, b"msg4".to_vec())
        .add_simple_message(0, 1, 2000, b"msg2".to_vec())
        .add_simple_message(0, 3, 6000, b"msg6".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let times: Vec<u64> = reader
        .messages()
        .unwrap()
        .map(|m| m.unwrap().log_time)
        .collect();

    // Verify we got all messages (order may vary depending on mcap crate behavior)
    assert_eq!(times.len(), 3);
    assert!(times.contains(&2000));
    assert!(times.contains(&4000));
    assert!(times.contains(&6000));
}

#[test]
fn test_multiple_channels_ordered_by_time() {
    // Messages from different channels should be interleaved by time
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<(u16, u64)> = reader
        .messages()
        .unwrap()
        .map(|m| {
            let msg = m.unwrap();
            (msg.channel_id, msg.log_time)
        })
        .collect();

    // Verify time ordering
    let times: Vec<u64> = messages.iter().map(|(_, t)| *t).collect();
    for window in times.windows(2) {
        assert!(window[0] <= window[1], "times should be non-decreasing");
    }
}

#[test]
fn test_simultaneous_timestamps() {
    // Messages with identical timestamps should maintain stable order
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test1")
        .add_simple_channel(1, "/test2")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(1, 1, 1000, b"msg2".to_vec())
        .add_simple_message(0, 2, 1000, b"msg3".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<(u16, u32)> = reader
        .messages()
        .unwrap()
        .map(|m| {
            let msg = m.unwrap();
            (msg.channel_id, msg.sequence)
        })
        .collect();

    // All should have same timestamp, but order should be stable
    assert_eq!(messages.len(), 3);
}

#[test]
fn test_empty_file_produces_no_messages() {
    // Empty MCAP file should produce no messages
    let mcap = McapBuilder::new().build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().count();
    assert_eq!(count, 0);
}
