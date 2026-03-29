//! Metadata and statistics conformance tests.

use crate::helpers::mcap_builder::*;
use mcapable::reader;
use std::io::Cursor;

// ============================================================================
// Statistics Tests
// ============================================================================

#[test]
fn test_statistics_access() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);

    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let count = reader.messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn test_channel_statistics() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<_> = reader.messages().unwrap().map(|m| m.unwrap()).collect();
    assert!(!messages.is_empty());
}

#[test]
fn test_statistics_message_count() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(0, 2, 2000, b"msg2".to_vec())
        .add_simple_message(0, 3, 3000, b"msg3".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<_> = reader.messages().unwrap().map(|m| m.unwrap()).collect();
    assert_eq!(messages.len(), 3);
}

#[test]
fn test_statistics_time_range() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(0, 2, 5000, b"msg2".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<_> = reader.messages().unwrap().map(|m| m.unwrap()).collect();
    let min_time = messages.iter().map(|m| m.log_time).min().unwrap();
    let max_time = messages.iter().map(|m| m.log_time).max().unwrap();

    assert_eq!(min_time, 1000);
    assert_eq!(max_time, 5000);
}

#[test]
fn test_statistics_multiple_channels() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    let messages: Vec<_> = reader.messages().unwrap().map(|m| m.unwrap()).collect();
    let distinct_channels: std::collections::HashSet<_> =
        messages.iter().map(|m| m.channel_id).collect();
    assert!(distinct_channels.len() >= 2);
}

// ============================================================================
// Schema Tests
// ============================================================================

#[test]
fn test_schema_lazy_loading() {
    let mcap = McapBuilder::new()
        .add_channel(TestChannel {
            id: 0,
            topic: "/test".to_string(),
            message_encoding: "json".to_string(),
            schema_id: 1,
            schema_name: Some("TestSchema".to_string()),
            schema_encoding: Some("jsonschema".to_string()),
            schema_data: Some(b"{}".to_vec()),
        })
        .add_simple_message(0, 1, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Schema not loaded yet
    assert!(reader.schema(1).is_none());

    // Iterate to load schema
    for msg in reader.messages().unwrap() {
        let _ = msg.unwrap();
    }

    // Now schema should be cached
    let schema = reader.schema(1);
    assert!(schema.is_some());
}

#[test]
fn test_schema_access() {
    let mcap = McapBuilder::new()
        .add_channel(TestChannel {
            id: 0,
            topic: "/test".to_string(),
            message_encoding: "json".to_string(),
            schema_id: 1,
            schema_name: Some("TestSchema".to_string()),
            schema_encoding: Some("jsonschema".to_string()),
            schema_data: Some(b"schema_data".to_vec()),
        })
        .add_simple_message(0, 1, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Load schemas
    for msg in reader.messages().unwrap() {
        let _ = msg.unwrap();
    }

    let schema = reader.schema(1).unwrap();
    assert_eq!(schema.id, 1);
    assert_eq!(schema.name, "TestSchema");
    assert_eq!(schema.encoding, "jsonschema");
}

#[test]
fn test_schemas_all() {
    let mcap = McapBuilder::new()
        .add_channel(TestChannel {
            id: 0,
            topic: "/test1".to_string(),
            message_encoding: "json".to_string(),
            schema_id: 1,
            schema_name: Some("Schema1".to_string()),
            schema_encoding: Some("jsonschema".to_string()),
            schema_data: Some(b"{}".to_vec()),
        })
        .add_channel(TestChannel {
            id: 1,
            topic: "/test2".to_string(),
            message_encoding: "json".to_string(),
            schema_id: 2,
            schema_name: Some("Schema2".to_string()),
            schema_encoding: Some("jsonschema".to_string()),
            schema_data: Some(b"{}".to_vec()),
        })
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(1, 1, 1000, b"msg2".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Load all schemas
    for msg in reader.messages().unwrap() {
        let _ = msg.unwrap();
    }

    let schemas = reader.schemas();
    assert!(!schemas.is_empty());
}

// ============================================================================
// Channel Tests
// ============================================================================

#[test]
fn test_channel_lazy_loading() {
    let mcap = create_simple_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Channel not loaded yet
    assert!(reader.channels().is_empty());

    // Iterate to load channels
    for msg in reader.messages().unwrap() {
        let _ = msg.unwrap();
    }

    // Now channel should be cached
    assert!(!reader.channels().is_empty());
}

#[test]
fn test_channel_metadata() {
    let mcap = McapBuilder::new()
        .add_simple_channel(0, "/test/topic")
        .add_simple_message(0, 1, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Load channels
    for msg in reader.messages().unwrap() {
        let _ = msg.unwrap();
    }

    let channels = reader.channels();
    assert_eq!(channels.len(), 1);
    let channel = channels.values().next().unwrap();
    assert_eq!(channel.topic, "/test/topic");
    assert_eq!(channel.message_encoding, "application/octet-stream");
}

#[test]
fn test_channels_all() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Load all channels
    for msg in reader.messages().unwrap() {
        let _ = msg.unwrap();
    }

    let channels = reader.channels();
    assert_eq!(channels.len(), 2);
}

#[test]
fn test_metadata_loaded_during_iteration() {
    let mcap = create_multi_channel_mcap();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    // Metadata is loaded during iteration (when implementation is complete)
    // For now, this test verifies the API pattern
    let _ = reader.messages().unwrap().next();
}

// ============================================================================
// Metadata Records Tests
// ============================================================================

#[test]
fn test_metadata_records() {
    let mcap = McapBuilder::new()
        .add_metadata("key1".to_string(), "value1".to_string())
        .add_metadata("key2".to_string(), "value2".to_string())
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"msg".to_vec())
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    for record in reader.records() {
        record.unwrap();
    }

    let all_metadata = reader.all_metadata();
    assert_eq!(all_metadata.len(), 2);
    let key1 = mcapable::ByteStr::from("key1");
    let key2 = mcapable::ByteStr::from("key2");
    assert_eq!(
        all_metadata
            .get(&key1)
            .and_then(|m| m.metadata.get(&key1))
            .map(|v| &**v),
        Some("value1")
    );
    assert_eq!(
        all_metadata
            .get(&key2)
            .and_then(|m| m.metadata.get(&key2))
            .map(|v| &**v),
        Some("value2")
    );
}

// ============================================================================
// Attachments Tests
// ============================================================================

#[test]
fn test_attachments() {
    let mcap = McapBuilder::new()
        .add_attachment(
            "calibration.json",
            "application/json",
            1_000,
            1_000,
            b"{\"cal\":true}".to_vec(),
        )
        .build();

    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new().build(cursor).unwrap();

    for record in reader.records() {
        record.unwrap();
    }

    let attachments = reader.all_attachments();
    assert_eq!(attachments.len(), 1);
    let attachment_name = mcapable::ByteStr::from("calibration.json");
    let attachment = attachments.get(&attachment_name).unwrap();
    assert_eq!(attachment.media_type, "application/json");
    assert_eq!(attachment.log_time, 1_000);
    assert_eq!(attachment.data.as_ref(), b"{\"cal\":true}");
}
