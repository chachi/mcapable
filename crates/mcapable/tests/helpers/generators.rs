#![allow(dead_code)]
//! Proptest generators for MCAP record types.
//!
//! These generators create random but valid MCAP data structures for
//! property-based testing.

use bytes::Bytes;
use mcapable::*;
use proptest::prelude::*;
use std::collections::HashMap;
use strum::IntoEnumIterator;

/// Generate valid MCAP magic bytes.
pub fn magic_bytes_strategy() -> impl Strategy<Value = [u8; 8]> {
    Just([0x89, b'M', b'C', b'A', b'P', 0x30, b'\r', b'\n'])
}

/// Generate valid record headers (opcode + length).
// Not currently used but may be useful for future tests
#[allow(dead_code)]
pub fn record_header_strategy() -> impl Strategy<Value = (u8, u64)> {
    let opcodes =
        prop::sample::select(Opcode::iter().collect::<Vec<_>>()).prop_map(|opcode| opcode as u8);
    (opcodes, 0u64..10000u64) // Reasonable length range
}

/// Generate valid ASCII/UTF-8 strings for MCAP.
pub fn mcap_string_strategy() -> impl Strategy<Value = mcapable::ByteStr> {
    prop_oneof![
        Just(mcapable::ByteStr::from("")),
        Just(mcapable::ByteStr::from("test")),
        Just(mcapable::ByteStr::from("/topic/name")),
        Just(mcapable::ByteStr::from("schema_name")),
        "[a-zA-Z0-9_/.-]{1,50}".prop_map(mcapable::ByteStr::from),
    ]
}

/// Generate valid metadata maps.
pub fn metadata_map_strategy()
-> impl Strategy<Value = HashMap<mcapable::ByteStr, mcapable::ByteStr>> {
    prop::collection::hash_map(
        mcap_string_strategy(),
        mcap_string_strategy(),
        0..5, // 0-5 key-value pairs
    )
}

/// Generate valid Header records.
pub fn header_strategy() -> impl Strategy<Value = Header> {
    (
        mcap_string_strategy(),
        mcap_string_strategy(),
        metadata_map_strategy(),
    )
        .prop_map(|(profile, library, metadata)| Header {
            profile,
            library,
            metadata,
        })
}

/// Generate valid Schema records.
pub fn schema_strategy() -> impl Strategy<Value = Schema> {
    (
        any::<u16>(),
        mcap_string_strategy(),
        mcap_string_strategy(),
        prop::collection::vec(any::<u8>(), 0..100),
    )
        .prop_map(|(id, name, encoding, data)| Schema {
            id,
            name,
            encoding,
            data: Bytes::from(data),
        })
}

/// Generate valid Channel records.
pub fn channel_strategy() -> impl Strategy<Value = Channel> {
    (
        any::<u16>(),
        mcap_string_strategy(),
        mcap_string_strategy(),
        any::<u16>(),
        metadata_map_strategy(),
    )
        .prop_map(
            |(id, topic, message_encoding, schema_id, metadata)| Channel {
                id,
                topic,
                message_encoding,
                schema_id,
                metadata,
            },
        )
}

/// Generate valid RawMessage records.
pub fn message_strategy() -> impl Strategy<Value = RawMessage> {
    (
        any::<u16>(),
        any::<u32>(),
        any::<u64>(),
        any::<u64>(),
        prop::collection::vec(any::<u8>(), 0..1000),
    )
        .prop_map(|(channel_id, sequence, log_time, publish_time, data)| {
            RawMessage::new(
                channel_id,
                sequence,
                log_time,
                publish_time,
                Payload::from_bytes(bytes::Bytes::from(data)),
            )
        })
}

/// Generate valid Chunk records.
pub fn chunk_strategy() -> impl Strategy<Value = Chunk> {
    (
        any::<u64>(),
        any::<u64>(),
        0u64..100000u64,
        any::<u32>(),
        prop_oneof![Just(""), Just("lz4"), Just("zstd")].prop_map(mcapable::ByteStr::from),
        prop::collection::vec(any::<u8>(), 0..1000),
    )
        .prop_map(
            |(
                message_start_time,
                message_end_time,
                uncompressed_size,
                crc,
                compression,
                records,
            )| {
                Chunk {
                    message_start_time,
                    message_end_time,
                    uncompressed_size,
                    uncompressed_crc: crc,
                    compression,
                    records: Bytes::from(records),
                }
            },
        )
}

/// Generate valid ChunkIndex records.
// Not currently used but may be useful for future tests
#[allow(dead_code)]
pub fn chunk_index_strategy() -> impl Strategy<Value = ChunkIndex> {
    (
        any::<u64>(),
        any::<u64>(),
        any::<u64>(),
        1u64..100000u64,
        prop::collection::hash_map(any::<u16>(), any::<u64>(), 0..5),
        0u64..100000u64,
        prop_oneof![Just(""), Just("lz4"), Just("zstd")].prop_map(mcapable::ByteStr::from),
    )
        .prop_map(
            |(
                message_start_time,
                message_end_time,
                chunk_start_offset,
                chunk_length,
                message_index_offsets,
                uncompressed_size,
                compression,
            )| {
                ChunkIndex {
                    message_start_time,
                    message_end_time,
                    chunk_start_offset,
                    chunk_length,
                    message_index_offsets,
                    message_index_length: 0,
                    uncompressed_size,
                    compression,
                }
            },
        )
}

/// Generate valid Statistics records.
// Not currently used but may be useful for future tests
#[allow(dead_code)]
pub fn statistics_strategy() -> impl Strategy<Value = Statistics> {
    (
        any::<u64>(),
        any::<u16>(),
        any::<u32>(),
        any::<u32>(),
        any::<u32>(),
        any::<u32>(),
        any::<u64>(),
        any::<u64>(),
        prop::collection::vec((any::<u16>(), any::<u64>()), 0..10),
    )
        .prop_map(
            |(
                message_count,
                schema_count,
                channel_count,
                attachment_count,
                metadata_count,
                chunk_count,
                message_start_time,
                message_end_time,
                channel_message_counts,
            )| Statistics {
                message_count,
                schema_count,
                channel_count,
                attachment_count,
                metadata_count,
                chunk_count,
                message_start_time,
                message_end_time,
                channel_message_counts: channel_message_counts
                    .into_iter()
                    .map(|(channel_id, message_count)| ChannelMessageCount {
                        channel_id,
                        message_count,
                    })
                    .collect(),
            },
        )
}

/// Generate valid Metadata records.
// Not currently used but may be useful for future tests
#[allow(dead_code)]
pub fn metadata_strategy() -> impl Strategy<Value = Metadata> {
    (mcap_string_strategy(), metadata_map_strategy())
        .prop_map(|(name, metadata)| Metadata { name, metadata })
}

/// Generate valid Attachment records.
// Not currently used but may be useful for future tests
#[allow(dead_code)]
pub fn attachment_strategy() -> impl Strategy<Value = Attachment> {
    (
        any::<u64>(),
        any::<u64>(),
        mcap_string_strategy(),
        mcap_string_strategy(),
        prop::collection::vec(any::<u8>(), 0..1000),
    )
        .prop_map(
            |(log_time, create_time, name, media_type, data)| Attachment {
                log_time,
                create_time,
                name,
                media_type,
                data: Bytes::from(data),
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn test_magic_bytes_always_valid(bytes in magic_bytes_strategy()) {
            assert_eq!(bytes, [0x89, b'M', b'C', b'A', b'P', 0x30, b'\r', b'\n']);
        }

        #[test]
        fn test_header_generation(header in header_strategy()) {
            // Headers should always be valid
            assert!(header.profile.len() <= 100);
            assert!(header.library.len() <= 100);
            assert!(header.metadata.len() <= 10);
        }

        #[test]
        fn test_schema_generation(schema in schema_strategy()) {
            // Schemas should have reasonable sizes
            assert!(schema.name.len() <= 100);
            assert!(schema.encoding.len() <= 100);
            assert!(schema.data.len() <= 100);
        }

        #[test]
        fn test_channel_generation(channel in channel_strategy()) {
            // Channels should have reasonable field values
            assert!(channel.topic.len() <= 100);
            assert!(channel.message_encoding.len() <= 100);
            assert!(channel.metadata.len() <= 10);
        }

        #[test]
        fn test_message_generation(msg in message_strategy()) {
            // Messages should have reasonable data sizes
            assert!(msg.data_len() <= 1000);
        }

        #[test]
        fn test_chunk_generation(chunk in chunk_strategy()) {
            // Chunks should have valid compression strings
            assert!(
                chunk.compression.is_empty()
                    || chunk.compression == "lz4"
                    || chunk.compression == "zstd"
            );
        }
    }
}
