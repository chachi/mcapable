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

/// Override chunks must use the empty compression string (per MCAP spec for
/// "no compression").
#[test]
fn override_stream_uses_empty_compression_string() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 64,
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut ch = writer
        .add_channel(
            ChannelSpec::new("/cam", "h264")
                .schema(schema)
                .uncompressed_chunks(),
        )
        .unwrap();
    for i in 0..4u64 {
        ch.write(i, i, vec![b'a'; 80]).unwrap();
    }
    drop(ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    assert!(
        !summary.chunk_indexes.is_empty(),
        "expected at least one chunk index from 4×80-byte writes",
    );
    for ci in summary.chunk_indexes.iter() {
        assert!(
            ci.compression.as_ref().is_empty(),
            "expected empty compression for override stream, got {:?}",
            ci.compression,
        );
    }
}

/// Override stream's flush threshold is independent of the default's:
/// a small override threshold flushes frequently while the default
/// accumulates; a small default threshold flushes frequently while
/// the override accumulates.
#[test]
fn override_stream_flushes_on_its_own_threshold() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 1 << 20, // huge — won't flush during the test
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
            ChannelSpec::new("/cam", "h264")
                .schema(schema)
                .chunk_override(ChunkOptions {
                    compression: None,
                    max_uncompressed_bytes: 32, // tiny — flushes per message
                    include_crc: true,
                }),
        )
        .unwrap();

    for i in 0..6u64 {
        default_ch.write(i, i, vec![b'a'; 16]).unwrap();
        override_ch.write(i, i, vec![b'b'; 64]).unwrap();
    }
    drop(default_ch);
    drop(override_ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    let override_chunks = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    let default_chunks = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref() == "zstd")
        .count();
    // Override stream's tiny threshold flushes per message (6 writes); finish
    // leaves at most one extra empty chunk. Default's huge threshold means
    // it produces exactly one zstd chunk at finish.
    assert!(
        override_chunks >= 6,
        "expected at least 6 override chunks (one per message), got {override_chunks}",
    );
    assert_eq!(
        default_chunks, 1,
        "default stream should flush exactly once at finish, got {default_chunks}",
    );

    // The default's only chunk should be the LAST entry in chunk_indexes — it
    // landed at finish() while overrides flushed eagerly per-message during
    // the write loop. This distinguishes "override flushed eagerly" from
    // "override flushed late in finish".
    let last = summary
        .chunk_indexes
        .iter()
        .last()
        .expect("at least one chunk");
    assert_eq!(
        last.compression.as_ref(),
        "zstd",
        "the last chunk should be the default zstd one (flushed at finish)",
    );
}

/// Default chunk path remains byte-equivalent regardless of whether the
/// caller knows about chunk_override. Guards against future refactors
/// that accidentally regress the "zero overhead when not used" promise.
#[test]
fn default_path_unchanged_when_no_overrides() {
    fn write_one(extra_no_op_override: bool) -> Vec<u8> {
        let out = Cursor::new(Vec::new());
        let mut writer = WriterBuilder::new()
            .chunked(ChunkOptions {
                compression: Some(Compression::Zstd),
                max_uncompressed_bytes: 64,
                include_crc: true,
            })
            .build(out)
            .unwrap();

        let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
        let mut spec = ChannelSpec::new("/x", "raw").schema(schema);
        if extra_no_op_override {
            // Explicitly setting chunk_override to None should be a no-op.
            spec.chunk_override = None;
        }
        let mut ch = writer.add_channel(spec).unwrap();
        for i in 0..4u64 {
            ch.write(i, i, vec![b'a'; 80]).unwrap();
        }
        drop(ch);
        writer.finish().unwrap();
        writer.into_inner().into_inner()
    }

    let baseline = write_one(false);
    let with_explicit_none = write_one(true);
    assert_eq!(
        baseline, with_explicit_none,
        "explicitly setting chunk_override to None must produce a byte-identical file",
    );
}

/// Multiple tagged channels with partial fills all flush on `finish()`.
#[test]
fn finish_flushes_all_override_streams() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 1 << 20,
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut ch_a = writer
        .add_channel(
            ChannelSpec::new("/cam_a", "h264")
                .schema(schema.clone())
                .uncompressed_chunks(),
        )
        .unwrap();
    let mut ch_b = writer
        .add_channel(
            ChannelSpec::new("/cam_b", "h264")
                .schema(schema)
                .uncompressed_chunks(),
        )
        .unwrap();

    // Partial fills — well under any reasonable threshold.
    ch_a.write(1, 1, vec![b'a'; 16]).unwrap();
    ch_b.write(2, 2, vec![b'b'; 16]).unwrap();
    drop(ch_a);
    drop(ch_b);

    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    let override_chunks = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    assert_eq!(
        override_chunks, 2,
        "expected one chunk per non-empty override stream (2 streams, 2 chunks)",
    );
}
