use bytes::Bytes;
use mcapable_core::writer::{ChunkOptions, WriterBuilder};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{Channel, Payload, RawMessage, Schema};
use mcapable_core::{Compression, decompress, parse_compression};
use proptest::prelude::*;
use std::io::Cursor;

const MCAP_MAGIC: &[u8; 8] = b"\x89MCAP\x30\r\n";

fn writer_mode_strategy() -> impl Strategy<Value = (Option<Option<Compression>>, usize)> {
    prop_oneof![
        Just((None, 0usize)),                 // unchunked
        (Just(Some(None)), 1usize..256usize), // chunked, no compression
        (Just(Some(Some(Compression::Lz4))), 1usize..256usize),
        (Just(Some(Some(Compression::Zstd))), 1usize..256usize),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]

    #[test]
    fn prop_writer_roundtrip_and_crc_invariants(
        (chunk_mode, max_uncompressed_bytes) in writer_mode_strategy(),
        messages in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..256), 0..40),
    ) {
        let is_chunked = chunk_mode.is_some();
        let has_messages = !messages.is_empty();

        let out = Cursor::new(Vec::new());
        let mut builder = WriterBuilder::new()
            .profile("ros2")
            .library("mcapable-test");

        if let Some(chunk_compression) = chunk_mode {
            builder = builder.chunked(ChunkOptions {
                compression: chunk_compression,
                max_uncompressed_bytes,
                include_crc: true,
            });
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
            message_encoding: ByteStr::from("application/octet-stream"),
            schema_id: 1,
            metadata: Default::default(),
        };
        let mut channel_writer = writer.copy_channel(&channel).unwrap();

        let mut expected = Vec::with_capacity(messages.len());
        for (i, data) in messages.iter().enumerate() {
            let msg = RawMessage::new(
                1,
                i as u32,
                1000 + i as u64,
                1000 + i as u64,
                Payload::from_bytes(Bytes::copy_from_slice(data)),
            );
            expected.push((msg.channel_id, msg.sequence, msg.log_time, msg.publish_time, data.clone()));
            channel_writer
                .write_with_sequence(
                    msg.log_time,
                    msg.publish_time,
                    data.clone(),
                    msg.sequence,
                )
                .unwrap();
        }
        writer.finish().unwrap();
        drop(channel_writer);

        let bytes = writer.into_inner().into_inner();

        prop_assert!(bytes.starts_with(MCAP_MAGIC));
        prop_assert!(bytes.ends_with(MCAP_MAGIC));

        let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
        let header = reader.header().unwrap();
        prop_assert_eq!(header.profile, ByteStr::from("ros2"));
        prop_assert_eq!(header.library, ByteStr::from("mcapable-test"));

        let mut got = Vec::new();
        for raw in reader.raw_messages().unwrap() {
            let raw = raw.unwrap();
            got.push((
                raw.channel_id,
                raw.sequence,
                raw.log_time,
                raw.publish_time,
                raw.data_bytes().to_vec(),
            ));
        }
        prop_assert_eq!(got, expected);

        let footer = reader.footer().unwrap().unwrap();
        prop_assert!(footer.summary_start > 0);
        prop_assert!(footer.summary_offset_start >= footer.summary_start);
        prop_assert!(footer.summary_offset_start <= bytes.len() as u64);

        let summary_start = footer.summary_start as usize;
        let summary_offset_start = footer.summary_offset_start as usize;
        let crc = mcapable_core::calculate_crc(&bytes[summary_start..summary_offset_start]);
        prop_assert_eq!(crc, footer.summary_crc);

        if is_chunked && has_messages {
            let mut saw_chunk = false;
            for chunk in reader.chunks() {
                let chunk = chunk.unwrap();
                saw_chunk = true;

                let compression = parse_compression(chunk.compression.as_ref()).unwrap();
                let uncompressed = decompress(
                    compression.as_ref(),
                    chunk.records.clone(),
                    chunk.uncompressed_size,
                )
                .unwrap();
                prop_assert_eq!(uncompressed.len() as u64, chunk.uncompressed_size);
                prop_assert_eq!(mcapable_core::calculate_crc(uncompressed.as_ref()), chunk.uncompressed_crc);
            }
            prop_assert!(saw_chunk);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, .. ProptestConfig::default() })]

    /// Write a stream of messages tagged for one of three channels:
    /// - default (zstd)
    /// - override_a (uncompressed)
    /// - override_b (lz4 with a different threshold)
    /// Reopen and assert per-channel message sequences are preserved.
    #[test]
    fn prop_mixed_compression_roundtrip_preserves_messages(
        msgs in prop::collection::vec(
            (0u8..3u8, prop::collection::vec(any::<u8>(), 0..256)),
            0..50,
        ),
    ) {
        use mcapable_core::writer::{ChannelSpec, SchemaSpec};

        let out = Cursor::new(Vec::new());
        let mut writer = WriterBuilder::new()
            .chunked(ChunkOptions {
                compression: Some(Compression::Zstd),
                max_uncompressed_bytes: 128,
                include_crc: true,
            })
            .build(out)
            .unwrap();

        let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
        let mut ch_default = writer
            .add_channel(ChannelSpec::new("/default", "raw").schema(schema.clone()))
            .unwrap();
        let mut ch_a = writer
            .add_channel(
                ChannelSpec::new("/override_a", "raw")
                    .schema(schema.clone())
                    .uncompressed_chunks(),
            )
            .unwrap();
        let mut ch_b = writer
            .add_channel(
                ChannelSpec::new("/override_b", "raw")
                    .schema(schema)
                    .chunk_override(ChunkOptions {
                        compression: Some(Compression::Lz4),
                        max_uncompressed_bytes: 64,
                        include_crc: true,
                    }),
            )
            .unwrap();

        let default_id = ch_default.channel_id();
        let a_id = ch_a.channel_id();
        let b_id = ch_b.channel_id();

        let mut expected: Vec<(u16, Vec<u8>)> = Vec::new();
        let mut t = 1000u64;
        for (tag, data) in &msgs {
            t += 1;
            let (ch_id, write_res) = match tag {
                0 => (default_id, ch_default.write(t, t, data.clone())),
                1 => (a_id, ch_a.write(t, t, data.clone())),
                _ => (b_id, ch_b.write(t, t, data.clone())),
            };
            write_res.unwrap();
            expected.push((ch_id, data.clone()));
        }
        drop(ch_default);
        drop(ch_a);
        drop(ch_b);
        writer.finish().unwrap();

        let bytes = writer.into_inner().into_inner();

        let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
        let mut got: Vec<(u16, Vec<u8>)> = Vec::new();
        for raw in reader.raw_messages().unwrap() {
            let raw = raw.unwrap();
            got.push((raw.channel_id, raw.data_bytes().to_vec()));
        }

        // Group expected by channel; group got by channel; assert per-channel
        // sequences match (order within a channel must be preserved; relative
        // order between channels follows log_time, which is monotonic above).
        use std::collections::HashMap;
        let group = |v: &Vec<(u16, Vec<u8>)>| -> HashMap<u16, Vec<Vec<u8>>> {
            let mut m: HashMap<u16, Vec<Vec<u8>>> = HashMap::new();
            for (id, data) in v {
                m.entry(*id).or_default().push(data.clone());
            }
            m
        };
        prop_assert_eq!(group(&expected), group(&got));
    }
}
