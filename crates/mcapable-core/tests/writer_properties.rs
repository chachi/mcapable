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
