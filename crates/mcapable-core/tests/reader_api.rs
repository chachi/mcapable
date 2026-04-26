//! Tests for Reader API methods that lack coverage.

use bytes::Bytes;
use mcapable_core::collections::HashMap;
use mcapable_core::writer::{ChunkOptions, WriterBuilder};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{Channel, Schema};
use std::io::Cursor;

/// Build a fixture MCAP with schema, channel, messages, attachment, and metadata.
fn build_fixture() -> Vec<u8> {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .profile("ros2")
        .library("test-lib")
        .header_metadata("build", "debug")
        .chunked(ChunkOptions::default())
        .build(out)
        .unwrap();

    let schema = Schema {
        id: 1,
        name: ByteStr::from("pkg/Msg"),
        encoding: ByteStr::from("jsonschema"),
        data: Bytes::from_static(br#"{"type":"object"}"#),
    };
    writer.copy_schema(&schema).unwrap();

    let channel = Channel {
        id: 1,
        topic: ByteStr::from("/test"),
        message_encoding: ByteStr::from("json"),
        schema_id: 1,
        metadata: HashMap::new(),
    };
    let mut cw = writer.copy_channel(&channel).unwrap();
    cw.write(100, 200, Bytes::from_static(b"msg1")).unwrap();
    cw.write(300, 400, Bytes::from_static(b"msg2")).unwrap();
    drop(cw);

    writer
        .copy_attachment(
            500,
            600,
            ByteStr::from("data.bin"),
            ByteStr::from("application/octet-stream"),
            Bytes::from_static(b"attachment-bytes"),
        )
        .unwrap();

    let md = mcapable_core::types::Metadata {
        name: ByteStr::from("config"),
        metadata: {
            let mut m = HashMap::new();
            m.insert(ByteStr::from("env"), ByteStr::from("test"));
            m
        },
    };
    writer.copy_metadata(&md).unwrap();

    writer.finish().unwrap();
    writer.into_inner().into_inner()
}

#[test]
fn reader_profile() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let profile = reader.profile().unwrap();
    assert_eq!(profile.as_ref(), "ros2");
}

#[test]
fn reader_metadata_by_name() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    // Iterate records to populate metadata/attachment caches
    for record in reader.records() {
        let _ = record.unwrap();
    }

    let md = reader.metadata("config");
    assert!(md.is_some(), "should find metadata by name");
    let md = md.unwrap();
    assert_eq!(md.name, ByteStr::from("config"));
    assert_eq!(
        md.metadata.get(&ByteStr::from("env")),
        Some(&ByteStr::from("test"))
    );
}

#[test]
fn reader_metadata_by_name_not_found() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    for record in reader.records() {
        let _ = record.unwrap();
    }

    let md = reader.metadata("nonexistent");
    assert!(md.is_none());
}

#[test]
fn reader_attachment_by_name() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    for record in reader.records() {
        let _ = record.unwrap();
    }

    let att = reader.attachment("data.bin");
    assert!(att.is_some(), "should find attachment by name");
    let att = att.unwrap();
    assert_eq!(att.data.as_ref(), b"attachment-bytes");
}

#[test]
fn reader_attachment_by_name_not_found() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    for record in reader.records() {
        let _ = record.unwrap();
    }

    let att = reader.attachment("nonexistent.bin");
    assert!(att.is_none());
}

#[test]
fn reader_attachment_indexes() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let indexes = reader.attachment_indexes().unwrap();
    assert_eq!(indexes.len(), 1);
    assert_eq!(indexes[0].name, ByteStr::from("data.bin"));
}

#[test]
fn reader_metadata_indexes() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let indexes = reader.metadata_indexes().unwrap();
    assert_eq!(indexes.len(), 1);
    assert_eq!(indexes[0].name, ByteStr::from("config"));
}

#[test]
fn reader_metadata_entries() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let entries = reader.metadata_entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].metadata.name, ByteStr::from("config"));
}

#[test]
fn reader_attachment_entries() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let entries = reader.attachment_entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, ByteStr::from("data.bin"));
}

// --- raw_records stream (exercises stream/raw_record.rs) ---

#[test]
fn reader_raw_records_iterates_all_record_types() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let mut opcodes = Vec::new();
    for record in reader.raw_records() {
        let record = record.unwrap();
        opcodes.push(record.opcode);
    }

    // Should see header, schema, channel, chunk (or messages), attachment, metadata, data_end, footer
    assert!(
        opcodes.contains(&mcapable_core::Opcode::Header),
        "should contain Header"
    );
    assert!(
        opcodes.contains(&mcapable_core::Opcode::Schema),
        "should contain Schema"
    );
    assert!(
        opcodes.contains(&mcapable_core::Opcode::Channel),
        "should contain Channel"
    );
    assert!(
        opcodes.contains(&mcapable_core::Opcode::Footer),
        "should contain Footer"
    );
    assert!(opcodes.len() >= 4, "should have at least 4 records");
}

// --- record_metadata stream (exercises stream/record_metadata.rs) ---

#[test]
fn reader_record_metadata_counts_messages() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let mut message_count = 0;
    let stream = reader
        .record_metadata()
        .include_chunk_messages()
        .include_message_metadata();
    for record in stream {
        let record = record.unwrap();
        if record.message.is_some() {
            message_count += 1;
        }
    }
    assert_eq!(message_count, 2, "fixture has 2 messages");
}

// --- message_metadata stream (exercises stream/message_metadata.rs) ---

#[test]
fn reader_message_metadata_iterates_messages_with_channel_info() {
    let bytes = build_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let mut count = 0;
    for msg in reader.message_metadata().unwrap() {
        let msg = msg.unwrap();
        assert!(msg.channel_id > 0, "should have valid channel_id");
        assert!(msg.log_time > 0, "should have valid log_time");
        count += 1;
    }
    assert_eq!(count, 2, "fixture has 2 messages");
}

// --- parsed stream (exercises stream/parsed/builder.rs, stream.rs, defaults.rs) ---

/// Build a fixture with jsonschema-encoded JSON messages for parsed stream testing.
fn build_json_fixture() -> Vec<u8> {
    let out = std::io::Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .profile("test")
        .chunked(mcapable_core::writer::ChunkOptions::default())
        .build(out)
        .unwrap();

    let schema = Schema {
        id: 1,
        name: ByteStr::from("test/Msg"),
        encoding: ByteStr::from("jsonschema"),
        data: Bytes::from_static(br#"{"type":"object","properties":{"value":{"type":"integer"}}}"#),
    };
    writer.copy_schema(&schema).unwrap();

    let channel = Channel {
        id: 1,
        topic: ByteStr::from("/data"),
        message_encoding: ByteStr::from("json"),
        schema_id: 1,
        metadata: HashMap::new(),
    };
    let mut cw = writer.copy_channel(&channel).unwrap();
    cw.write(100, 200, Bytes::from_static(br#"{"value":42}"#))
        .unwrap();
    cw.write(300, 400, Bytes::from_static(br#"{"value":99}"#))
        .unwrap();
    drop(cw);
    writer.finish().unwrap();
    writer.into_inner().into_inner()
}

#[test]
fn parsed_stream_with_custom_parser() {
    let bytes = build_json_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let stream = reader
        .messages()
        .unwrap()
        .parsed::<serde_json::Value>()
        .parser(
            |schema| schema.encoding.as_ref() == "jsonschema",
            |data| {
                serde_json::from_slice(data.as_ref())
                    .map_err(|e| mcapable_core::Error::InvalidRecord(e.to_string()))
            },
        )
        .build();

    let mut results: Vec<serde_json::Value> = Vec::new();
    for item in stream {
        results.push(item.unwrap());
    }
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["value"], 42);
    assert_eq!(results[1]["value"], 99);
}

#[test]
fn parsed_stream_with_schema_encoding_parser() {
    let bytes = build_json_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let stream = reader
        .messages()
        .unwrap()
        .parsed::<String>()
        .parser_schema_encoding("jsonschema", |data| {
            String::from_utf8(data.to_vec())
                .map_err(|e| mcapable_core::Error::InvalidRecord(e.to_string()))
        })
        .build();

    let mut results: Vec<String> = Vec::new();
    for item in stream {
        results.push(item.unwrap());
    }
    assert_eq!(results.len(), 2);
    assert!(results[0].contains("42"));
    assert!(results[1].contains("99"));
}

#[test]
fn parsed_stream_with_message_encoding_parser() {
    let bytes = build_json_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let stream = reader
        .messages()
        .unwrap()
        .parsed::<serde_json::Value>()
        .parser_message_encoding("json", |data| {
            serde_json::from_slice(data.as_ref())
                .map_err(|e| mcapable_core::Error::InvalidRecord(e.to_string()))
        })
        .build();

    let results: Vec<serde_json::Value> = stream.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn parsed_stream_no_matching_parser_errors() {
    let bytes = build_json_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    // Register a parser that won't match (wrong encoding)
    let stream = reader
        .messages()
        .unwrap()
        .parsed::<String>()
        .parser(
            |schema| schema.encoding.as_ref() == "protobuf",
            |_data| Ok("unreachable".to_string()),
        )
        .build();

    // First message should error because no parser matches
    let first = stream.into_iter().next().unwrap();
    assert!(first.is_err());
    assert!(
        first
            .unwrap_err()
            .to_string()
            .contains("No parser registered")
    );
}

#[test]
fn parsed_stream_with_channel_parser() {
    let bytes = build_json_fixture();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let stream = reader
        .messages()
        .unwrap()
        .parsed::<String>()
        .parser_channel(
            |channel, _schema| channel.topic.as_ref() == "/data",
            |data| {
                String::from_utf8(data.to_vec())
                    .map_err(|e| mcapable_core::Error::InvalidRecord(e.to_string()))
            },
        )
        .build();

    let results: Vec<String> = stream.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(results.len(), 2);
}
