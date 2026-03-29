#![allow(dead_code)]
//! Proptest generators for MCAP record types.
//!
//! These generators create random but valid MCAP data structures for
//! property-based testing.

use crate::support::HashMap;
use crate::types::{
    Attachment, Channel, ChannelMessageCount, Chunk, ChunkIndex, Header, Metadata, Opcode, Payload,
    RawMessage, Schema, Statistics,
};
use crate::zero_copy::ByteStr;
use bytes::Bytes;
use proptest::prelude::*;
use strum::IntoEnumIterator;

/// Generate valid MCAP magic bytes.
pub fn magic_bytes_strategy() -> impl Strategy<Value = [u8; 8]> {
    Just([0x89, b'M', b'C', b'A', b'P', 0x30, b'\r', b'\n'])
}

/// Generate valid record headers (opcode + length).
#[allow(dead_code)]
pub fn record_header_strategy() -> impl Strategy<Value = (u8, u64)> {
    let opcodes =
        prop::sample::select(Opcode::iter().collect::<Vec<_>>()).prop_map(|opcode| opcode as u8);
    (opcodes, 0u64..10000u64) // Reasonable length range
}

/// Generate valid ASCII/UTF-8 strings for MCAP.
pub fn mcap_string_strategy() -> impl Strategy<Value = ByteStr> {
    prop_oneof![
        Just(ByteStr::from("")),
        Just(ByteStr::from("test")),
        Just(ByteStr::from("/topic/name")),
        Just(ByteStr::from("schema_name")),
        "[a-zA-Z0-9_/.-]{1,50}".prop_map(ByteStr::from),
    ]
}

/// Generate valid metadata maps.
pub fn metadata_map_strategy() -> impl Strategy<Value = HashMap<ByteStr, ByteStr>> {
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
        prop_oneof![Just(""), Just("lz4"), Just("zstd")].prop_map(ByteStr::from),
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
#[allow(dead_code)]
pub fn chunk_index_strategy() -> impl Strategy<Value = ChunkIndex> {
    (
        any::<u64>(),
        any::<u64>(),
        any::<u64>(),
        1u64..100000u64,
        prop::collection::hash_map(any::<u16>(), any::<u64>(), 0..5),
        0u64..100000u64,
        prop_oneof![Just(""), Just("lz4"), Just("zstd")].prop_map(ByteStr::from),
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
            )| ChunkIndex {
                message_start_time,
                message_end_time,
                chunk_start_offset,
                chunk_length,
                message_index_offsets,
                message_index_length: 0,
                compression,
                uncompressed_size,
            },
        )
}

/// Generate valid Statistics records.
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
        prop::collection::vec(
            (any::<u16>(), any::<u64>()).prop_map(|(channel_id, message_count)| {
                ChannelMessageCount {
                    channel_id,
                    message_count,
                }
            }),
            0..5,
        ),
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
                channel_message_counts,
            },
        )
}

/// Generate valid Metadata records.
pub fn metadata_strategy() -> impl Strategy<Value = Metadata> {
    (mcap_string_strategy(), metadata_map_strategy())
        .prop_map(|(name, metadata)| Metadata { name, metadata })
}

/// Generate valid Attachment records.
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
                data: bytes::Bytes::from(data),
            },
        )
}
