//! Tests for per-channel chunk_override (uncompressed-while-rest-is-compressed).

use bytes::Bytes;
use mcapable_core::Compression;
use mcapable_core::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};
use std::io::Cursor;

/// Build a writer with a default zstd chunk stream, write a few messages
/// to a normal channel and a few to a channel tagged `uncompressed_chunks()`,
/// then assert the resulting file contains chunks of both compression types.
#[test]
fn override_channel_produces_uncompressed_chunk_alongside_default_zstd() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 64, // tiny, so each message flushes
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));

    let mut default_ch = writer
        .add_channel(ChannelSpec::new("/telemetry", "raw").schema(schema.clone()))
        .unwrap();
    let mut override_ch = writer
        .add_channel(
            ChannelSpec::new("/video", "h264")
                .schema(schema.clone())
                .uncompressed_chunks(),
        )
        .unwrap();

    // Write enough bytes on each channel to force a flush.
    for i in 0..4u64 {
        default_ch
            .write(1000 + i, 1000 + i, vec![b'a'; 80])
            .unwrap();
        override_ch
            .write(1000 + i, 1000 + i, vec![b'b'; 80])
            .unwrap();
    }
    drop(default_ch);
    drop(override_ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();

    // Open the file and inspect chunk indexes.
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary present");

    let mut zstd_count = 0;
    let mut none_count = 0;
    for ci in summary.chunk_indexes.iter() {
        if ci.compression.as_ref() == "zstd" {
            zstd_count += 1;
        } else if ci.compression.as_ref().is_empty() {
            none_count += 1;
        } else {
            panic!("unexpected compression: {:?}", ci.compression);
        }
    }
    assert!(zstd_count > 0, "expected at least one zstd chunk");
    assert!(none_count > 0, "expected at least one uncompressed chunk");
}

/// `copy_channel_with_override` lets pipelines (merge/filter) impose an
/// override on a channel they're copying from another file.
#[test]
fn copy_channel_with_override_routes_to_override_stream() {
    use mcapable_core::Channel;
    use mcapable_core::zero_copy::ByteStr;

    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 64,
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let channel = Channel {
        id: 7,
        topic: ByteStr::from("/copied"),
        message_encoding: ByteStr::from("h264"),
        schema_id: 0,
        metadata: Default::default(),
    };

    let opts = ChunkOptions {
        compression: None,
        max_uncompressed_bytes: 32,
        include_crc: true,
    };

    let mut ch = writer.copy_channel_with_override(&channel, opts).unwrap();
    ch.write(1, 1, vec![b'z'; 64]).unwrap();
    ch.write(2, 2, vec![b'z'; 64]).unwrap();
    drop(ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    let none_chunks: usize = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    assert!(
        none_chunks > 0,
        "expected at least one uncompressed chunk for copied channel"
    );
    let zstd_chunks: usize = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref() == "zstd")
        .count();
    assert_eq!(
        zstd_chunks, 0,
        "no messages were written to the default zstd channel; expected 0 zstd chunks",
    );
}
