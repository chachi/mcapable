use bytes::Bytes;
use mcapable_core::Compression;
use mcapable_core::collections::HashMap;
use mcapable_core::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{Channel, Schema};
use std::io::Cursor;

fn assert_footer_and_summary(bytes: &[u8], expected_chunk_indexes: usize) {
    let mut reader = mcapable_core::reader::Reader::from_slice(bytes).unwrap();
    let footer = reader.footer().unwrap().unwrap();

    assert!(footer.summary_start > 0);
    assert!(footer.summary_offset_start >= footer.summary_start);
    assert!(footer.summary_offset_start <= bytes.len() as u64);

    let summary_start = footer.summary_start as usize;
    let summary_offset_start = footer.summary_offset_start as usize;
    let crc = mcapable_core::calculate_crc(&bytes[summary_start..summary_offset_start]);
    assert_eq!(crc, footer.summary_crc);

    let summary = reader.summary().unwrap().unwrap();
    assert_eq!(summary.chunk_indexes.len(), expected_chunk_indexes);
    assert!(!summary.schemas.is_empty());
    assert!(!summary.channels.is_empty());
}

#[test]
fn writer_round_trip_unchunked() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .profile("ros2")
        .library("mcapable-test")
        .header_metadata("k", "v")
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
    let mut channel_writer = writer.copy_channel(&channel).unwrap();
    channel_writer
        .write_with_sequence(100, 200, Bytes::from_static(b"abc"), 42)
        .unwrap();
    drop(channel_writer);
    writer.finish().unwrap();

    let out = writer.into_inner();
    let bytes = out.into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let header = reader.header().unwrap();
    assert_eq!(header.profile, ByteStr::from("ros2"));
    assert_eq!(header.library, ByteStr::from("mcapable-test"));
    let key = ByteStr::from("k");
    assert_eq!(header.metadata.get(&key).map(|v| v.as_str()), Some("v"));

    let mut got = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        got.push((
            raw.channel_id,
            raw.sequence,
            raw.log_time,
            raw.publish_time,
            raw.data_bytes(),
        ));
    }
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, 1);
    assert_eq!(got[0].1, 42);
    assert_eq!(got[0].2, 100);
    assert_eq!(got[0].3, 200);
    assert_eq!(got[0].4.as_ref(), b"abc");

    // Schemas/channels are loaded during stream creation; validate they were cached.
    let schemas = reader.schemas();
    assert_eq!(schemas.get(&1).unwrap(), &schema);
    let channels = reader.channels();
    assert_eq!(channels.get(&1).unwrap(), &channel);

    assert_footer_and_summary(&bytes, 0);
}

fn write_basic_file(chunk_options: Option<ChunkOptions>) -> Vec<u8> {
    let out = Cursor::new(Vec::new());
    let mut builder = WriterBuilder::new()
        .profile("ros2")
        .library("mcapable-test")
        .header_metadata("k", "v");
    if let Some(options) = chunk_options {
        builder = builder.chunked(options);
    }
    let mut writer = builder.build(out).unwrap();

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
    let mut channel_writer = writer.copy_channel(&channel).unwrap();
    channel_writer
        .write_with_sequence(100, 200, Bytes::from_static(b"abc"), 42)
        .unwrap();
    drop(channel_writer);
    writer.finish().unwrap();

    writer.into_inner().into_inner()
}

#[test]
fn writer_round_trip_chunked_none() {
    let bytes = write_basic_file(Some(ChunkOptions {
        compression: None,
        max_uncompressed_bytes: 1,
        include_crc: true,
    }));
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut got = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        got.push(raw.data_bytes());
    }
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].as_ref(), b"abc");

    assert_footer_and_summary(&bytes, 1);
}

#[test]
fn writer_round_trip_chunked_lz4() {
    let bytes = write_basic_file(Some(ChunkOptions {
        compression: Some(Compression::Lz4),
        max_uncompressed_bytes: 1,
        include_crc: true,
    }));
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut got = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        got.push(raw.data_bytes());
    }
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].as_ref(), b"abc");

    assert_footer_and_summary(&bytes, 1);
}

#[test]
fn writer_round_trip_chunked_zstd() {
    let bytes = write_basic_file(Some(ChunkOptions {
        compression: Some(Compression::Zstd),
        max_uncompressed_bytes: 1,
        include_crc: true,
    }));
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut got = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        got.push(raw.data_bytes());
    }
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].as_ref(), b"abc");

    assert_footer_and_summary(&bytes, 1);
}

#[test]
fn writer_round_trip_chunked_no_crc() {
    let bytes = write_basic_file(Some(ChunkOptions {
        compression: None,
        max_uncompressed_bytes: 1,
        include_crc: false,
    }));
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut got = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        got.push(raw.data_bytes());
    }
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].as_ref(), b"abc");
}

#[test]
fn writer_finish_is_idempotent() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new().build(out).unwrap();
    // Trigger header write via channel creation.
    writer
        .add_channel(
            ChannelSpec::new("/test", "application/octet-stream").schema(SchemaSpec::new(
                "test",
                "raw",
                Bytes::new(),
            )),
        )
        .unwrap();
    writer.finish().unwrap();
    writer.finish().unwrap();
}

#[test]
fn writer_write_after_finish_errors() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new().build(out).unwrap();
    writer.finish().unwrap();

    let schema = Schema {
        id: 1,
        name: ByteStr::from("x"),
        encoding: ByteStr::from("y"),
        data: Bytes::new(),
    };
    let err = writer.copy_schema(&schema).unwrap_err();
    assert!(format!("{err}").contains("after finish"));
}

#[test]
fn writer_header_is_idempotent() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new().build(out).unwrap();
    // Header is written automatically on first operation, so trigger it twice.
    writer
        .add_channel(
            ChannelSpec::new("/test", "application/octet-stream").schema(SchemaSpec::new(
                "test",
                "raw",
                Bytes::new(),
            )),
        )
        .unwrap();
    // Second operation should not re-write header.
    writer
        .add_channel(
            ChannelSpec::new("/test2", "application/octet-stream").schema(SchemaSpec::new(
                "test2",
                "raw",
                Bytes::new(),
            )),
        )
        .unwrap();
    writer.finish().unwrap();
}

#[test]
fn writer_channel_writer_sequences_increment() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new().build(out).unwrap();

    let mut ch = writer
        .add_channel(ChannelSpec::new("/test", "json").schema(SchemaSpec::new(
            "pkg/Msg",
            "jsonschema",
            Bytes::from_static(br#"{"type":"object"}"#),
        )))
        .unwrap();
    ch.write(100, 200, Bytes::from_static(b"one")).unwrap();
    ch.write(101, 201, Bytes::from_static(b"two")).unwrap();
    drop(ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let mut got = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        got.push((raw.sequence, raw.data_bytes()));
    }
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].0, 0);
    assert_eq!(got[0].1.as_ref(), b"one");
    assert_eq!(got[1].0, 1);
    assert_eq!(got[1].1.as_ref(), b"two");
}

#[test]
fn writer_attachment_round_trip() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new().build(out).unwrap();

    {
        let mut attachments = writer.attachment_writer();
        attachments
            .write(
                123,
                456,
                ByteStr::from("calib.json"),
                ByteStr::from("application/json"),
                Bytes::from_static(br#"{"k":"v"}"#),
            )
            .unwrap();
    }
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let mut found = None;
    for rec in reader.records() {
        if let mcapable_core::Record::Attachment(att) = rec.unwrap() {
            found = Some(att);
            break;
        }
    }

    let att = found.expect("expected attachment record");
    assert_eq!(att.log_time, 123);
    assert_eq!(att.create_time, 456);
    assert_eq!(att.name, ByteStr::from("calib.json"));
    assert_eq!(att.media_type, ByteStr::from("application/json"));
    assert_eq!(att.data.as_ref(), br#"{"k":"v"}"#);
}

#[test]
fn writer_copy_record_round_trip() {
    // Write a source file with schema, channel, message, attachment, metadata
    let source_bytes = {
        let out = Cursor::new(Vec::new());
        let mut writer = WriterBuilder::new()
            .profile("test")
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
        cw.write(100, 200, Bytes::from_static(b"hello")).unwrap();
        drop(cw);

        writer
            .copy_attachment(
                300,
                400,
                ByteStr::from("file.bin"),
                ByteStr::from("application/octet-stream"),
                Bytes::from_static(b"attachment-data"),
            )
            .unwrap();

        let md = mcapable_core::types::Metadata {
            name: ByteStr::from("build"),
            metadata: {
                let mut m = HashMap::new();
                m.insert(ByteStr::from("version"), ByteStr::from("1.0"));
                m
            },
        };
        writer.copy_metadata(&md).unwrap();
        writer.finish().unwrap();
        writer.into_inner().into_inner()
    };

    // Read all records and copy them to a new writer via copy_record
    let mut source_reader = mcapable_core::reader::Reader::from_slice(&source_bytes).unwrap();

    let out = Cursor::new(Vec::new());
    let mut dest_writer = WriterBuilder::new().profile("test").build(out).unwrap();

    for record in source_reader.records() {
        let record = record.unwrap();
        dest_writer.copy_record(&record).unwrap();
    }
    dest_writer.finish().unwrap();
    let dest_bytes = dest_writer.into_inner().into_inner();

    // Verify the destination file has the same data
    let mut dest_reader = mcapable_core::reader::Reader::from_slice(&dest_bytes).unwrap();
    let mut msg_count = 0;
    for msg in dest_reader.raw_messages().unwrap() {
        let msg = msg.unwrap();
        assert_eq!(msg.data_bytes().as_ref(), b"hello");
        msg_count += 1;
    }
    assert_eq!(msg_count, 1);

    // Verify attachment was copied
    let mut found_attachment = false;
    let mut found_metadata = false;
    for rec in dest_reader.records() {
        match rec.unwrap() {
            mcapable_core::Record::Attachment(att) => {
                assert_eq!(att.name, ByteStr::from("file.bin"));
                assert_eq!(att.data.as_ref(), b"attachment-data");
                found_attachment = true;
            }
            mcapable_core::Record::Metadata(md) => {
                assert_eq!(md.name, ByteStr::from("build"));
                found_metadata = true;
            }
            _ => {}
        }
    }
    assert!(found_attachment, "attachment should be copied");
    assert!(found_metadata, "metadata should be copied");
}

#[test]
fn writer_copy_metadata_round_trip() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new().build(out).unwrap();

    let md = mcapable_core::types::Metadata {
        name: ByteStr::from("config"),
        metadata: {
            let mut m = HashMap::new();
            m.insert(ByteStr::from("key1"), ByteStr::from("val1"));
            m.insert(ByteStr::from("key2"), ByteStr::from("val2"));
            m
        },
    };
    writer.copy_metadata(&md).unwrap();
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    let mut found = false;
    for rec in reader.records() {
        if let mcapable_core::Record::Metadata(md) = rec.unwrap() {
            assert_eq!(md.name, ByteStr::from("config"));
            assert_eq!(md.metadata.len(), 2);
            found = true;
        }
    }
    assert!(found, "metadata record should be present");
}

#[test]
fn writer_into_inner_returns_sink() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new().build(out).unwrap();
    writer.finish().unwrap();
    let inner = writer.into_inner();
    // The inner should be a Cursor<Vec<u8>> with valid MCAP data
    let bytes = inner.into_inner();
    assert!(bytes.len() > 16, "should have at least header + footer");
    // Verify it starts with MCAP magic
    assert_eq!(&bytes[..8], b"\x89MCAP0\r\n");
}

#[test]
fn writer_validation_permissive() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .validation(mcapable_core::writer::Validation::Permissive)
        .build(out)
        .unwrap();

    // In permissive mode, we can write without schemas/channels
    // Just verify it doesn't panic
    writer.finish().unwrap();
    let bytes = writer.into_inner().into_inner();
    assert!(bytes.len() > 16);
}

#[test]
fn writer_copy_chunk_record_preserves_data() {
    // Build a source MCAP with chunked messages
    let source_bytes = {
        let out = Cursor::new(Vec::new());
        let mut writer = WriterBuilder::new()
            .profile("test")
            .chunked(ChunkOptions {
                compression: Some(Compression::Zstd),
                max_uncompressed_bytes: 4_194_304,
                include_crc: true,
            })
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
        cw.write(100, 200, Bytes::from_static(b"hello")).unwrap();
        cw.write(300, 400, Bytes::from_static(b"world")).unwrap();
        drop(cw);
        writer.finish().unwrap();
        writer.into_inner().into_inner()
    };

    // Read records and copy chunk records without re-compression
    let mut source_reader = mcapable_core::reader::Reader::from_slice(&source_bytes).unwrap();

    let out = Cursor::new(Vec::new());
    let mut dest_writer = WriterBuilder::new().profile("test").build(out).unwrap();

    for record in source_reader.records() {
        let record = record.unwrap();
        dest_writer.copy_record(&record).unwrap();
    }
    dest_writer.finish().unwrap();
    let dest_bytes = dest_writer.into_inner().into_inner();

    // Verify messages are preserved
    let mut dest_reader = mcapable_core::reader::Reader::from_slice(&dest_bytes).unwrap();
    let mut msgs = Vec::new();
    for msg in dest_reader.raw_messages().unwrap() {
        let msg = msg.unwrap();
        msgs.push((msg.log_time, msg.data_bytes().to_vec()));
    }
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].0, 100);
    assert_eq!(msgs[0].1, b"hello");
    assert_eq!(msgs[1].0, 300);
    assert_eq!(msgs[1].1, b"world");

    // Verify summary has chunk index entries (chunk was preserved, not re-chunked)
    let summary = dest_reader.summary().unwrap().unwrap();
    assert!(!summary.chunk_indexes.is_empty());
}

#[test]
fn writer_copy_message_record_unchunked() {
    // Write an unchunked source with a message
    let source_bytes = {
        let out = Cursor::new(Vec::new());
        let mut writer = WriterBuilder::new().profile("test").build(out).unwrap();

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
        cw.write_with_sequence(100, 200, Bytes::from_static(b"data1"), 42)
            .unwrap();
        drop(cw);
        writer.finish().unwrap();
        writer.into_inner().into_inner()
    };

    // Read messages and copy via copy_message_record
    let mut source_reader = mcapable_core::reader::Reader::from_slice(&source_bytes).unwrap();

    let out = Cursor::new(Vec::new());
    let mut dest_writer = WriterBuilder::new().profile("test").build(out).unwrap();

    // Copy schemas and channels first
    for record in source_reader.records() {
        let record = record.unwrap();
        dest_writer.copy_record(&record).unwrap();
    }
    dest_writer.finish().unwrap();
    let dest_bytes = dest_writer.into_inner().into_inner();

    let mut dest_reader = mcapable_core::reader::Reader::from_slice(&dest_bytes).unwrap();
    let mut count = 0;
    for msg in dest_reader.raw_messages().unwrap() {
        let msg = msg.unwrap();
        assert_eq!(msg.sequence, 42);
        assert_eq!(msg.log_time, 100);
        assert_eq!(msg.publish_time, 200);
        assert_eq!(msg.data_bytes().as_ref(), b"data1");
        count += 1;
    }
    assert_eq!(count, 1);
}

/// Helper: write a file with one default-stream channel and one override
/// channel, returning (bytes, expected_default_messages, expected_override_messages).
fn write_mixed_compression_file(
    default_compression: Option<Compression>,
    override_compression: Option<Compression>,
) -> (Vec<u8>, Vec<Bytes>, Vec<Bytes>) {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: default_compression,
            max_uncompressed_bytes: 64,
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema_spec =
        mcapable_core::writer::SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut default_ch = writer
        .add_channel(
            mcapable_core::writer::ChannelSpec::new("/telemetry", "raw")
                .schema(schema_spec.clone()),
        )
        .unwrap();
    let mut override_ch = writer
        .add_channel(
            mcapable_core::writer::ChannelSpec::new("/video", "h264")
                .schema(schema_spec)
                .chunk_override(ChunkOptions {
                    compression: override_compression,
                    max_uncompressed_bytes: 64,
                    include_crc: true,
                }),
        )
        .unwrap();

    let mut default_msgs = Vec::new();
    let mut override_msgs = Vec::new();
    for i in 0..6u64 {
        let dm = Bytes::from(vec![b'a'; 80]);
        let om = Bytes::from(vec![b'A' + i as u8; 80]);
        default_ch.write(1000 + i, 1000 + i, dm.clone()).unwrap();
        override_ch.write(1000 + i, 1000 + i, om.clone()).unwrap();
        default_msgs.push(dm);
        override_msgs.push(om);
    }
    drop(default_ch);
    drop(override_ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    (bytes, default_msgs, override_msgs)
}

fn assert_mixed_round_trip_messages(
    bytes: Vec<u8>,
    default_msgs: Vec<Bytes>,
    override_msgs: Vec<Bytes>,
) {
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();

    // Channels populate lazily only during raw_messages() consumption, so we
    // can't pre-cache them before opening the stream. Collect (channel_id, data)
    // pairs first, drop the stream, then resolve topics from the now-populated
    // channel map.
    let mut got: Vec<(u16, Bytes)> = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        got.push((raw.channel_id, raw.data_bytes()));
    }

    let channels = reader.channels();
    let topic_for = |id: u16| channels.get(&id).map(|c| c.topic.as_ref().to_string());

    let mut got_telemetry: Vec<Bytes> = Vec::new();
    let mut got_video: Vec<Bytes> = Vec::new();
    for (ch_id, data) in got {
        match topic_for(ch_id).as_deref() {
            Some("/telemetry") => got_telemetry.push(data),
            Some("/video") => got_video.push(data),
            other => panic!("unexpected topic: {other:?}"),
        }
    }
    assert_eq!(got_telemetry, default_msgs);
    assert_eq!(got_video, override_msgs);
}

#[test]
fn writer_round_trip_mixed_compression_zstd_default_uncompressed_override() {
    let (bytes, default_msgs, override_msgs) =
        write_mixed_compression_file(Some(Compression::Zstd), None);
    assert_mixed_round_trip_messages(bytes, default_msgs, override_msgs);
}

#[test]
fn writer_round_trip_mixed_compression_lz4_default_uncompressed_override() {
    let (bytes, default_msgs, override_msgs) =
        write_mixed_compression_file(Some(Compression::Lz4), None);
    assert_mixed_round_trip_messages(bytes, default_msgs, override_msgs);
}

#[test]
fn writer_round_trip_mixed_compression_none_default_uncompressed_override() {
    let (bytes, default_msgs, override_msgs) = write_mixed_compression_file(None, None);
    assert_mixed_round_trip_messages(bytes, default_msgs, override_msgs);
}

#[test]
fn reader_chunk_stream_sees_both_compressions_in_mixed_file() {
    let (bytes, _, _) = write_mixed_compression_file(Some(Compression::Zstd), None);

    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut zstd_chunks = 0usize;
    let mut none_chunks = 0usize;
    for chunk_res in reader.chunks() {
        let chunk = chunk_res.unwrap();
        match chunk.compression.as_ref() {
            "zstd" => zstd_chunks += 1,
            "" => none_chunks += 1,
            other => panic!("unexpected compression {other:?}"),
        }
    }
    assert!(zstd_chunks > 0, "expected at least one zstd chunk");
    assert!(none_chunks > 0, "expected at least one uncompressed chunk");
}

#[test]
fn summary_chunk_indexes_round_trip_for_mixed_file() {
    let (bytes, _, _) = write_mixed_compression_file(Some(Compression::Zstd), None);

    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    // Snapshot index entries before reborrowing reader for chunks().
    let indexes: Vec<_> = summary.chunk_indexes.iter().cloned().collect();

    // Every chunk index entry's offset+length must lie within the file.
    for ci in &indexes {
        let end = ci.chunk_start_offset + ci.chunk_length;
        assert!(
            end as usize <= bytes.len(),
            "chunk index entry exceeds file: offset={}, length={}, file_len={}",
            ci.chunk_start_offset,
            ci.chunk_length,
            bytes.len(),
        );
    }

    // At least one zstd and one empty-compression entry must appear.
    let mut compressions: Vec<String> = indexes
        .iter()
        .map(|ci| ci.compression.as_ref().to_string())
        .collect();
    compressions.sort();
    compressions.dedup();
    assert!(compressions.contains(&"zstd".to_string()));
    assert!(compressions.contains(&"".to_string()));

    // Cross-check each chunk record against its index entry.
    let actual_chunks: Vec<_> = reader.chunks().map(|r| r.unwrap()).collect();
    assert_eq!(
        actual_chunks.len(),
        indexes.len(),
        "summary chunk_indexes count must match actual chunks",
    );
    // Build (compression, uncompressed_size) -> count multisets and compare.
    use std::collections::BTreeMap;
    let mut idx_buckets: BTreeMap<(String, u64), usize> = BTreeMap::new();
    for ci in &indexes {
        *idx_buckets
            .entry((ci.compression.as_ref().to_string(), ci.uncompressed_size))
            .or_insert(0) += 1;
    }
    let mut chunk_buckets: BTreeMap<(String, u64), usize> = BTreeMap::new();
    for ch in &actual_chunks {
        *chunk_buckets
            .entry((ch.compression.as_ref().to_string(), ch.uncompressed_size))
            .or_insert(0) += 1;
    }
    assert_eq!(
        idx_buckets, chunk_buckets,
        "summary chunk indexes must match actual chunks by (compression, uncompressed_size)",
    );
}
