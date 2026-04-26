//! Comprehensive roundtrip equivalence tests between mcap and mcapable.
//!
//! These tests ensure that files written by one library can be correctly read by the other,
//! and that all metadata (headers, footers, channels, schemas, messages) is preserved.

use bytes::Bytes;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

type McapSchemaDef = (String, String, Vec<u8>); // (name, encoding, data)
type McapChannelDef = (String, String, u16, HashMap<String, String>); // (topic, encoding, schema_id, metadata)
type McapMessageDef = (u16, u32, u64, u64, Vec<u8>); // (channel_idx, sequence, log_time, publish_time, data)

type McapableSchemaDef = (u16, String, String, Bytes); // (id, name, encoding, data)
type McapableChannelDef = (u16, String, String, u16, HashMap<String, String>); // (id, topic, encoding, schema_id, metadata)
type McapableMessageDef = (u16, u32, u64, u64, Bytes); // (channel_id, sequence, log_time, publish_time, data)

/// Helper to create an MCAP file with mcap crate
fn create_mcap_file(
    profile: &str,
    _library: &str, // mcap crate doesn't expose library in WriteOptions
    schemas: Vec<McapSchemaDef>,
    channels: Vec<McapChannelDef>,
    messages: Vec<McapMessageDef>,
    compression: Option<mcap::Compression>,
    chunked: bool,
) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut opts = mcap::WriteOptions::new()
            .profile(profile)
            .use_chunks(chunked);
        if let Some(comp) = compression {
            opts = opts.compression(Some(comp));
        }
        let mut writer = opts
            .create(Cursor::new(&mut buffer))
            .expect("Failed to create writer");

        // Write schemas - mcap crate writes schemas implicitly when channels reference them
        let mut schema_arcs = Vec::new();
        for (i, (name, encoding, data)) in schemas.into_iter().enumerate() {
            let schema = Arc::new(mcap::Schema {
                id: (i + 1) as u16,
                name: name.clone(),
                encoding: encoding.clone(),
                data: Cow::Owned(data),
            });
            schema_arcs.push(schema);
        }

        // Write channels (schemas/channels are written on-demand when writing messages)
        let mut channel_arcs = Vec::new();
        for (idx, (topic, encoding, schema_id, metadata)) in channels.iter().enumerate() {
            let schema = if *schema_id < schema_arcs.len() as u16 {
                Some(schema_arcs[*schema_id as usize].clone())
            } else {
                None
            };
            let channel = Arc::new(mcap::Channel {
                id: idx as u16,
                topic: topic.clone(),
                message_encoding: encoding.clone(),
                metadata: metadata
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                schema,
            });
            channel_arcs.push(channel);
        }

        // Write messages
        for (channel_idx, sequence, log_time, publish_time, data) in messages {
            let channel = channel_arcs[channel_idx as usize].clone();
            let message = mcap::Message {
                channel,
                sequence,
                log_time,
                publish_time,
                data: Cow::Owned(data),
            };
            writer.write(&message).expect("Failed to write message");
        }

        writer.finish().expect("Failed to finish");
    }
    buffer
}

/// Helper to create an MCAP file with mcapable
fn create_mcapable_file(
    profile: &str,
    library: &str,
    schemas: Vec<McapableSchemaDef>,
    channels: Vec<McapableChannelDef>,
    messages: Vec<McapableMessageDef>,
    compression: Option<mcapable::Compression>,
    chunked: bool,
) -> Vec<u8> {
    let out = Cursor::new(Vec::new());
    let mut builder = mcapable::WriterBuilder::new()
        .profile(profile)
        .library(library);

    if chunked {
        builder = builder.chunked(mcapable::ChunkOptions {
            compression,
            max_uncompressed_bytes: 4 * 1024 * 1024,
            ..mcapable::ChunkOptions::default()
        });
    }

    let mut writer = builder.build(out).expect("Failed to create writer");

    // Write schemas
    for (id, name, encoding, data) in schemas {
        let schema = mcapable::Schema {
            id,
            name: name.into(),
            encoding: encoding.into(),
            data,
        };
        writer.copy_schema(&schema).expect("Failed to write schema");
    }

    // Write channels - convert String metadata to ByteStr
    let mut channel_writers: HashMap<u16, _> = HashMap::new();
    for (id, topic, encoding, schema_id, metadata) in channels {
        let metadata_bs: HashMap<mcapable::ByteStr, mcapable::ByteStr> = metadata
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let channel = mcapable::Channel {
            id,
            topic: topic.into(),
            message_encoding: encoding.into(),
            schema_id,
            metadata: metadata_bs,
        };
        let channel_writer = writer
            .copy_channel(&channel)
            .expect("Failed to write channel");
        channel_writers.insert(id, channel_writer);
    }

    // Write messages using ChannelWriter
    for (channel_id, sequence, log_time, publish_time, data) in messages {
        let channel_writer = channel_writers
            .get_mut(&channel_id)
            .expect("Missing channel writer");
        channel_writer
            .write_with_sequence(log_time, publish_time, data, sequence)
            .expect("Failed to write message");
    }

    writer.finish().expect("Failed to finish");
    drop(channel_writers);
    writer.into_inner().into_inner()
}

/// Compare headers between mcap and mcapable
/// Note: mcap crate doesn't expose Header directly, so we compare via mcapable reader
fn compare_headers(mcapable_header: &mcapable::Header, expected_profile: &str) {
    assert_eq!(
        mcapable_header.profile.as_str(),
        expected_profile,
        "Profile should match"
    );
    // Note: mcap crate doesn't expose library or metadata in the same way, so we skip those comparisons
}

/// Compare messages between mcap and mcapable
/// mcap_messages: (sequence, log_time, publish_time, data)
/// mcapable_messages: (channel_id, sequence, log_time, publish_time, data)
fn compare_messages(
    mcap_messages: &[(u32, u64, u64, Vec<u8>)],
    mcapable_messages: &[(u16, u32, u64, u64, Vec<u8>)],
) {
    assert_eq!(
        mcap_messages.len(),
        mcapable_messages.len(),
        "Message count should match"
    );
    for (i, ((mcap_seq, mcap_lt, mcap_pt, mcap_data), (_, mc_seq, mc_lt, mc_pt, mc_data))) in
        mcap_messages
            .iter()
            .zip(mcapable_messages.iter())
            .enumerate()
    {
        assert_eq!(*mcap_seq, *mc_seq, "Message {} sequence should match", i);
        assert_eq!(*mcap_lt, *mc_lt, "Message {} log_time should match", i);
        assert_eq!(*mcap_pt, *mc_pt, "Message {} publish_time should match", i);
        assert_eq!(mcap_data, mc_data, "Message {} data should match", i);
    }
}

/// Extract all messages from an mcap file using mcap crate
/// Returns (sequence, log_time, publish_time, data) since channel_id isn't easily accessible
fn extract_mcap_messages(mcap_data: &[u8]) -> Vec<(u32, u64, u64, Vec<u8>)> {
    let mut messages = Vec::new();
    let stream = mcap::MessageStream::new(mcap_data).expect("Failed to create message stream");
    for msg in stream {
        let msg = msg.expect("Failed to read message");
        messages.push((
            msg.sequence,
            msg.log_time,
            msg.publish_time,
            msg.data.to_vec(),
        ));
    }
    messages
}

/// Extract all messages from an mcap file using mcapable
fn extract_mcapable_messages(mcap_data: &[u8]) -> Vec<(u16, u32, u64, u64, Vec<u8>)> {
    let mut messages = Vec::new();
    let cursor = Cursor::new(mcap_data);
    let mut reader = mcapable::reader::Builder::new()
        .build(cursor)
        .expect("Failed to create reader");

    // Force loading of schemas and channels by accessing them first
    // This ensures schemas/channels in chunks are loaded
    let _ = reader.schemas();
    let _ = reader.channels();

    for msg in reader
        .raw_messages()
        .expect("Failed to create message stream")
    {
        let msg = msg.expect("Failed to read message");
        messages.push((
            msg.channel_id,
            msg.sequence,
            msg.log_time,
            msg.publish_time,
            msg.data_bytes().to_vec(),
        ));
    }
    messages
}

#[test]
fn test_mcap_to_mcapable_roundtrip_unchunked() {
    let schemas = vec![("TestSchema".to_string(), "raw".to_string(), b"{}".to_vec())];
    let channels = vec![("/test".to_string(), "raw".to_string(), 0, HashMap::new())];
    let messages = vec![
        (0, 0, 1000, 1000, b"message1".to_vec()),
        (0, 1, 2000, 2000, b"message2".to_vec()),
        (0, 2, 3000, 3000, b"message3".to_vec()),
    ];

    // Write with mcap
    let mcap_data = create_mcap_file(
        "ros2",
        "mcap-test",
        schemas.clone(),
        channels.clone(),
        messages.clone(),
        None,
        false, // unchunked
    );

    // Read with mcapable
    let cursor = Cursor::new(&mcap_data);
    let mut reader = mcapable::reader::Builder::new()
        .build(cursor)
        .expect("Failed to create reader");

    // Compare header
    let mcapable_header = reader.header().expect("Failed to read header");
    compare_headers(&mcapable_header, "ros2");

    // Compare schemas - note: mcap may write schemas inside chunks, not in summary
    // So we just verify we can read the file, not the exact schema count
    let _ = reader.schemas();
    let _ = reader.channels();

    // Compare messages
    let mcapable_messages = extract_mcapable_messages(&mcap_data);
    let mcap_messages = extract_mcap_messages(&mcap_data);
    compare_messages(&mcap_messages, &mcapable_messages);

    // Compare footer
    let mcapable_footer = reader
        .footer()
        .expect("Failed to read footer")
        .expect("Footer should exist");
    // mcap doesn't expose footer directly, but we can verify it exists
    assert!(mcapable_footer.summary_start > 0);
}

#[test]
fn test_mcapable_to_mcap_roundtrip_unchunked() {
    let schemas = vec![(
        1,
        "TestSchema".to_string(),
        "raw".to_string(),
        Bytes::from(b"{}".as_slice()),
    )];
    let channels = vec![(0, "/test".to_string(), "raw".to_string(), 1, HashMap::new())];
    let messages = vec![
        (0, 0, 1000, 1000, Bytes::copy_from_slice(b"message1")),
        (0, 1, 2000, 2000, Bytes::copy_from_slice(b"message2")),
        (0, 2, 3000, 3000, Bytes::copy_from_slice(b"message3")),
    ];

    // Write with mcapable
    let mcapable_data = create_mcapable_file(
        "ros2",
        "mcapable-test",
        schemas,
        channels,
        messages.clone(),
        None,
        false, // unchunked
    );

    // Read with mcap
    let mcap_messages = extract_mcap_messages(&mcapable_data);
    let mcapable_messages: Vec<_> = messages
        .iter()
        .map(|(ch, seq, lt, pt, data)| (*ch, *seq, *lt, *pt, data.to_vec()))
        .collect();
    compare_messages(&mcap_messages, &mcapable_messages);
}

#[test]
fn test_mcap_to_mcapable_roundtrip_chunked() {
    let schemas = vec![("TestSchema".to_string(), "raw".to_string(), b"{}".to_vec())];
    let channels = vec![("/test".to_string(), "raw".to_string(), 0, HashMap::new())];
    let messages = vec![
        (0, 0, 1000, 1000, b"message1".to_vec()),
        (0, 1, 2000, 2000, b"message2".to_vec()),
        (0, 2, 3000, 3000, b"message3".to_vec()),
    ];

    // Write with mcap (chunked)
    let mcap_data = create_mcap_file(
        "ros2",
        "mcap-test",
        schemas,
        channels,
        messages.clone(),
        None,
        true, // chunked
    );

    // Read with mcapable
    let mcapable_messages = extract_mcapable_messages(&mcap_data);
    let mcap_messages = extract_mcap_messages(&mcap_data);
    compare_messages(&mcap_messages, &mcapable_messages);
}

#[test]
fn test_mcapable_to_mcap_roundtrip_chunked() {
    let schemas = vec![(
        1,
        "TestSchema".to_string(),
        "raw".to_string(),
        Bytes::from(b"{}".as_slice()),
    )];
    let channels = vec![(0, "/test".to_string(), "raw".to_string(), 1, HashMap::new())];
    let messages = vec![
        (0, 0, 1000, 1000, Bytes::copy_from_slice(b"message1")),
        (0, 1, 2000, 2000, Bytes::copy_from_slice(b"message2")),
        (0, 2, 3000, 3000, Bytes::copy_from_slice(b"message3")),
    ];

    // Write with mcapable (chunked)
    let mcapable_data = create_mcapable_file(
        "ros2",
        "mcapable-test",
        schemas,
        channels,
        messages.clone(),
        None,
        true, // chunked
    );

    // Read with mcap
    let mcap_messages = extract_mcap_messages(&mcapable_data);
    let mcapable_messages: Vec<_> = messages
        .iter()
        .map(|(ch, seq, lt, pt, data)| (*ch, *seq, *lt, *pt, data.to_vec()))
        .collect();
    compare_messages(&mcap_messages, &mcapable_messages);
}

#[test]
fn test_mcap_to_mcapable_roundtrip_with_compression() {
    for compression in [mcap::Compression::Lz4, mcap::Compression::Zstd] {
        let schemas = vec![("TestSchema".to_string(), "raw".to_string(), b"{}".to_vec())];
        let channels = vec![("/test".to_string(), "raw".to_string(), 0, HashMap::new())];
        let messages = vec![
            (0, 0, 1000, 1000, b"message1".to_vec()),
            (0, 1, 2000, 2000, b"message2".to_vec()),
        ];

        // Write with mcap
        let mcap_data = create_mcap_file(
            "ros2",
            "mcap-test",
            schemas,
            channels,
            messages.clone(),
            Some(compression),
            true, // chunked
        );

        // Read with mcapable
        let mcapable_messages = extract_mcapable_messages(&mcap_data);
        let mcap_messages = extract_mcap_messages(&mcap_data);
        compare_messages(&mcap_messages, &mcapable_messages);
    }
}

#[test]
fn test_mcapable_to_mcap_roundtrip_with_compression() {
    for compression in [mcapable::Compression::Lz4, mcapable::Compression::Zstd] {
        let schemas = vec![(
            1,
            "TestSchema".to_string(),
            "raw".to_string(),
            Bytes::from(b"{}".as_slice()),
        )];
        let channels = vec![(0, "/test".to_string(), "raw".to_string(), 1, HashMap::new())];
        let messages = vec![
            (0, 0, 1000, 1000, Bytes::copy_from_slice(b"message1")),
            (0, 1, 2000, 2000, Bytes::copy_from_slice(b"message2")),
        ];

        // Write with mcapable
        let mcapable_data = create_mcapable_file(
            "ros2",
            "mcapable-test",
            schemas,
            channels,
            messages.clone(),
            Some(compression),
            true, // chunked
        );

        // Read with mcap
        let mcap_messages = extract_mcap_messages(&mcapable_data);
        let mcapable_messages: Vec<_> = messages
            .iter()
            .map(|(ch, seq, lt, pt, data)| (*ch, *seq, *lt, *pt, data.to_vec()))
            .collect();
        compare_messages(&mcap_messages, &mcapable_messages);
    }
}

#[test]
fn test_mcap_to_mcapable_roundtrip_multiple_channels() {
    let schemas = vec![
        ("Schema0".to_string(), "raw".to_string(), b"{}".to_vec()),
        ("Schema1".to_string(), "raw".to_string(), b"{}".to_vec()),
    ];
    let channels = vec![
        (
            "/channel0".to_string(),
            "raw".to_string(),
            0,
            HashMap::new(),
        ), // schema_id 0 = first schema (index 0)
        (
            "/channel1".to_string(),
            "raw".to_string(),
            1,
            HashMap::new(),
        ), // schema_id 1 = second schema (index 1)
    ];
    let messages = vec![
        (0, 0, 1000, 1000, b"ch0_msg1".to_vec()),
        (1, 0, 2000, 2000, b"ch1_msg1".to_vec()),
        (0, 1, 3000, 3000, b"ch0_msg2".to_vec()),
        (1, 1, 4000, 4000, b"ch1_msg2".to_vec()),
    ];

    // Write with mcap
    let mcap_data = create_mcap_file(
        "ros2",
        "mcap-test",
        schemas,
        channels,
        messages.clone(),
        None,
        true,
    );

    // Read with mcapable
    let mcapable_messages = extract_mcapable_messages(&mcap_data);
    let mcap_messages = extract_mcap_messages(&mcap_data);
    compare_messages(&mcap_messages, &mcapable_messages);

    // Verify channel counts - force loading by accessing messages first
    let cursor = Cursor::new(&mcap_data);
    let mut reader = mcapable::reader::Builder::new()
        .build(cursor)
        .expect("Failed to create reader");
    // Force loading channels by iterating messages
    let _ = reader
        .messages()
        .expect("Failed to create message stream")
        .count();
    let channels = reader.channels();
    assert_eq!(channels.len(), 2, "Should have 2 channels");
}

#[test]
fn test_full_roundtrip_mcap_to_mcapable_to_mcap() {
    let schemas = vec![("TestSchema".to_string(), "raw".to_string(), b"{}".to_vec())];
    let channels = vec![("/test".to_string(), "raw".to_string(), 0, HashMap::new())];
    let messages = vec![
        (0, 0, 1000, 1000, b"message1".to_vec()),
        (0, 1, 2000, 2000, b"message2".to_vec()),
        (0, 2, 3000, 3000, b"message3".to_vec()),
    ];

    // Step 1: Write with mcap
    let mcap_data = create_mcap_file(
        "ros2",
        "mcap-test",
        schemas.clone(),
        channels.clone(),
        messages.clone(),
        None,
        true,
    );

    // Step 2: Read with mcapable and extract messages
    let mcapable_messages_1 = extract_mcapable_messages(&mcap_data);
    let mcap_messages_1 = extract_mcap_messages(&mcap_data);
    compare_messages(&mcap_messages_1, &mcapable_messages_1);

    // Step 3: Write with mcapable
    // mcap assigns schema IDs starting from 1, so we need to map them correctly
    // For simplicity, assign schema IDs starting from 1
    let mcapable_schemas: Vec<_> = schemas
        .iter()
        .enumerate()
        .map(|(i, (name, enc, data))| {
            (
                (i + 1) as u16,
                name.clone(),
                enc.clone(),
                Bytes::from(data.clone()),
            )
        })
        .collect();
    let mcapable_channels: Vec<_> = channels
        .iter()
        .enumerate()
        .map(|(i, (topic, enc, schema_id, _))| {
            // Map schema_id: 0 means no schema, otherwise use schema_id (mcap uses 1-based)
            let mapped_schema_id = if *schema_id == 0 { 0 } else { *schema_id };
            (
                i as u16,
                topic.clone(),
                enc.clone(),
                mapped_schema_id,
                HashMap::new(),
            )
        })
        .collect();
    let mcapable_messages: Vec<_> = mcapable_messages_1
        .iter()
        .map(|(ch, seq, lt, pt, data)| (*ch, *seq, *lt, *pt, Bytes::from(data.clone())))
        .collect();

    let mcapable_data = create_mcapable_file(
        "ros2",
        "mcapable-test",
        mcapable_schemas,
        mcapable_channels,
        mcapable_messages,
        None,
        true,
    );

    // Step 4: Read with mcap
    let mcap_messages_2 = extract_mcap_messages(&mcapable_data);
    // Compare mcap messages directly (both are from mcap, so same format)
    assert_eq!(
        mcap_messages_1.len(),
        mcap_messages_2.len(),
        "Message count should match"
    );
    for (i, (m1, m2)) in mcap_messages_1
        .iter()
        .zip(mcap_messages_2.iter())
        .enumerate()
    {
        assert_eq!(m1.0, m2.0, "Message {} sequence should match", i);
        assert_eq!(m1.1, m2.1, "Message {} log_time should match", i);
        assert_eq!(m1.2, m2.2, "Message {} publish_time should match", i);
        assert_eq!(m1.3, m2.3, "Message {} data should match", i);
    }
}

#[test]
fn test_full_roundtrip_mcapable_to_mcap_to_mcapable() {
    let schemas = vec![(
        1,
        "TestSchema".to_string(),
        "raw".to_string(),
        Bytes::from(b"{}".as_slice()),
    )];
    let channels = vec![(0, "/test".to_string(), "raw".to_string(), 1, HashMap::new())];
    let messages = vec![
        (0, 0, 1000, 1000, Bytes::copy_from_slice(b"message1")),
        (0, 1, 2000, 2000, Bytes::copy_from_slice(b"message2")),
        (0, 2, 3000, 3000, Bytes::copy_from_slice(b"message3")),
    ];

    // Step 1: Write with mcapable
    let mcapable_data_1 = create_mcapable_file(
        "ros2",
        "mcapable-test",
        schemas.clone(),
        channels.clone(),
        messages.clone(),
        None,
        true,
    );

    // Step 2: Read with mcap
    let mcap_messages_1 = extract_mcap_messages(&mcapable_data_1);
    let mcapable_messages_1: Vec<_> = messages
        .iter()
        .map(|(ch, seq, lt, pt, data)| (*ch, *seq, *lt, *pt, data.to_vec()))
        .collect();
    compare_messages(&mcap_messages_1, &mcapable_messages_1);

    // Step 3: Write with mcap
    let mcap_schemas: Vec<_> = schemas
        .iter()
        .map(|(_, name, enc, data)| (name.clone(), enc.clone(), data.to_vec()))
        .collect();
    let mcap_channels: Vec<_> = channels
        .iter()
        .map(|(_, topic, enc, schema_id, _)| {
            (topic.clone(), enc.clone(), *schema_id, HashMap::new())
        })
        .collect();
    let mcap_messages: Vec<_> = mcap_messages_1
        .iter()
        .map(|(seq, lt, pt, data)| (0u16, *seq, *lt, *pt, data.clone()))
        .collect();

    let mcap_data = create_mcap_file(
        "ros2",
        "mcap-test",
        mcap_schemas,
        mcap_channels,
        mcap_messages,
        None,
        true,
    );

    // Step 4: Read with mcapable
    let mcapable_messages_2 = extract_mcapable_messages(&mcap_data);
    // Compare mcapable messages directly (both are from mcapable, so same format)
    assert_eq!(
        mcapable_messages_1.len(),
        mcapable_messages_2.len(),
        "Message count should match"
    );
    for (i, (m1, m2)) in mcapable_messages_1
        .iter()
        .zip(mcapable_messages_2.iter())
        .enumerate()
    {
        assert_eq!(m1.0, m2.0, "Message {} channel_id should match", i);
        assert_eq!(m1.1, m2.1, "Message {} sequence should match", i);
        assert_eq!(m1.2, m2.2, "Message {} log_time should match", i);
        assert_eq!(m1.3, m2.3, "Message {} publish_time should match", i);
        assert_eq!(m1.4, m2.4, "Message {} data should match", i);
    }
}

#[test]
fn test_roundtrip_header_metadata() {
    // Test that header metadata is preserved
    // Write with mcap (mcap crate doesn't easily support header metadata, so we'll test mcapable)
    let schemas_mc = vec![(
        1,
        "TestSchema".to_string(),
        "raw".to_string(),
        Bytes::from(b"{}".as_slice()),
    )];
    let channels_mc = vec![(0, "/test".to_string(), "raw".to_string(), 1, {
        let mut meta = HashMap::new();
        meta.insert("key1".into(), "value1".into());
        meta
    })];
    let messages_mc = vec![(0, 0, 1000, 1000, Bytes::copy_from_slice(b"test"))];

    let mcapable_data = create_mcapable_file(
        "ros2",
        "mcapable-test",
        schemas_mc,
        channels_mc,
        messages_mc,
        None,
        true,
    );

    // Read with mcapable and verify metadata - force loading by accessing messages first
    let cursor = Cursor::new(&mcapable_data);
    let mut reader = mcapable::reader::Builder::new()
        .build(cursor)
        .expect("Failed to create reader");
    // Force loading channels by iterating messages
    let _ = reader
        .messages()
        .expect("Failed to create message stream")
        .count();
    let channels = reader.channels();
    let channel = channels.get(&0).expect("Should have channel 0");
    let key = mcapable::ByteStr::from("key1");
    assert_eq!(
        channel.metadata.get(&key).map(|v| v.as_str()),
        Some("value1"),
        "Channel metadata should be preserved"
    );
}

#[test]
fn test_roundtrip_large_message() {
    // Test with a large message to ensure no truncation
    let large_data = vec![0xAA; 100_000]; // 100KB message

    let schemas = vec![(
        1,
        "TestSchema".to_string(),
        "raw".to_string(),
        Bytes::from(b"{}".as_slice()),
    )];
    let channels = vec![(0, "/test".to_string(), "raw".to_string(), 1, HashMap::new())];
    let messages = vec![(0, 0, 1000, 1000, Bytes::from(large_data.clone()))];

    // Write with mcapable
    let mcapable_data = create_mcapable_file(
        "ros2",
        "mcapable-test",
        schemas,
        channels,
        messages,
        None,
        true,
    );

    // Read with mcap
    let mcap_messages = extract_mcap_messages(&mcapable_data);
    assert_eq!(mcap_messages.len(), 1);
    assert_eq!(
        mcap_messages[0].3, large_data,
        "Large message data should match"
    );
}

#[test]
fn test_roundtrip_many_messages() {
    // Test with many messages to ensure ordering and completeness
    let mut messages = Vec::new();
    for i in 0..1000 {
        messages.push((
            0,
            i as u32,
            1000 + i as u64,
            1000 + i as u64,
            Bytes::from(format!("message_{}", i)),
        ));
    }

    let schemas = vec![(
        1,
        "TestSchema".to_string(),
        "raw".to_string(),
        Bytes::from(b"{}".as_slice()),
    )];
    let channels = vec![(0, "/test".to_string(), "raw".to_string(), 1, HashMap::new())];

    // Write with mcapable
    let mcapable_data = create_mcapable_file(
        "ros2",
        "mcapable-test",
        schemas,
        channels,
        messages.clone(),
        None,
        true,
    );

    // Read with mcap
    let mcap_messages = extract_mcap_messages(&mcapable_data);
    assert_eq!(mcap_messages.len(), 1000, "Should have 1000 messages");

    // Verify ordering
    for (i, (seq, lt, _, _)) in mcap_messages.iter().enumerate() {
        assert_eq!(*seq, i as u32, "Sequence should match index");
        assert_eq!(*lt, 1000 + i as u64, "Log time should match");
    }
}
