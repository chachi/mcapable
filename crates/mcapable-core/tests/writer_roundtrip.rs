use bytes::Bytes;
use mcapable_core::Compression;
use mcapable_core::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{Channel, Schema};
use std::collections::HashMap;
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
