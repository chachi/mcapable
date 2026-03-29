//! Filtering logic for streams.
//!
//! Helpers for applying time range, channel, and chunk filters efficiently.

use super::{ChannelPredicate, ReaderAccess};
use crate::format::RECORD_HEADER_SIZE;
use crate::records::try_decode_record_header;
use crate::support::HashMap;
use crate::types::{Chunk, ChunkMetadata, Opcode, Timestamp};
use bytes::Bytes;
use std::io::SeekFrom;

/// Cache schema or channel records during iteration.
///
/// When iterating over records, we want to cache schemas and channels
/// as we encounter them so they're available for message processing.
pub(super) fn cache_schema_or_channel(reader: &mut dyn ReaderAccess, opcode: Opcode, data: Bytes) {
    match opcode {
        Opcode::Schema => {
            if let Ok(schema) = crate::parser::parse_schema_record(data) {
                reader.cache_schema(schema);
            }
        }
        Opcode::Channel => {
            if let Ok(channel) = crate::parser::parse_channel_record(data) {
                reader.cache_channel(channel);
            }
        }
        _ => {}
    }
}

/// Check if a chunk might contain messages in the given time range.
///
/// Uses MessageIndex records (if available) to determine if any messages
/// in the chunk fall within the time range. This allows skipping chunks
/// without decompressing them.
///
/// Returns `true` if the chunk might have matching messages (or if we can't
/// determine for sure), `false` if we're certain it has no matches.
pub(super) fn chunk_might_have_messages_in_range_via_index(
    reader: &mut dyn ReaderAccess,
    chunk_offset: u64,
    start: Timestamp,
    end: Timestamp,
    channel_predicate: &Option<ChannelPredicate<'_>>,
) -> bool {
    let Some(index) = reader.get_chunk_index(chunk_offset) else {
        return true;
    };
    if index.message_index_offsets.is_empty() {
        return true;
    }

    let mut cache: HashMap<u64, Option<crate::types::MessageIndex>> = HashMap::new();

    for (channel_id, message_index_offset) in &index.message_index_offsets {
        if let Some(predicate) = channel_predicate
            && !reader.channel_predicate_allows(*channel_id, predicate)
        {
            continue;
        }

        if *message_index_offset == 0 {
            return true;
        }

        let msg_index = match cache.entry(*message_index_offset) {
            std::collections::hash_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let original_pos = match reader.source().stream_position() {
                    Ok(p) => p,
                    Err(_) => return true,
                };
                let parsed = match reader.source().seek(SeekFrom::Start(*message_index_offset)) {
                    Ok(_) => {
                        let header_buf = match reader.source().read_exact_bytes(RECORD_HEADER_SIZE)
                        {
                            Ok(buf) => buf,
                            Err(_) => {
                                let _ = reader.source().seek(SeekFrom::Start(original_pos));
                                return true;
                            }
                        };
                        let Some((opcode, length)) =
                            (match try_decode_record_header(header_buf.as_ref()) {
                                Ok(v) => v,
                                Err(_) => {
                                    let _ = reader.source().seek(SeekFrom::Start(original_pos));
                                    return true;
                                }
                            })
                        else {
                            let _ = reader.source().seek(SeekFrom::Start(original_pos));
                            return true;
                        };
                        if opcode != Opcode::MessageIndex {
                            Some(None)
                        } else {
                            let len: usize = match length.try_into() {
                                Ok(v) => v,
                                Err(_) => {
                                    let _ = reader.source().seek(SeekFrom::Start(original_pos));
                                    return true;
                                }
                            };
                            let body = match reader.source().read_exact_bytes(len) {
                                Ok(b) => b,
                                Err(_) => {
                                    let _ = reader.source().seek(SeekFrom::Start(original_pos));
                                    return true;
                                }
                            };
                            Some(crate::parser::parse_message_index_record(body).ok())
                        }
                    }
                    Err(_) => {
                        let _ = reader.source().seek(SeekFrom::Start(original_pos));
                        return true;
                    }
                };

                let _ = reader.source().seek(SeekFrom::Start(original_pos));
                if let Some(v) = parsed {
                    e.insert(v).clone()
                } else {
                    return true;
                }
            }
        };

        let Some(msg_index) = msg_index else {
            return true;
        };

        if msg_index.channel_id != *channel_id {
            return true;
        }

        if message_index_has_timestamp_in_range(&msg_index, start, end) {
            return true;
        }
    }

    false
}

/// Check if a MessageIndex has any records in the given time range.
pub(super) fn message_index_has_timestamp_in_range(
    msg_index: &crate::types::MessageIndex,
    start: Timestamp,
    end: Timestamp,
) -> bool {
    let records = msg_index.records.as_slice();
    if records.is_empty() {
        return false;
    }

    let idx = records.partition_point(|e| e.timestamp < start);
    if idx < records.len() && records[idx].timestamp <= end {
        return true;
    }

    records
        .iter()
        .any(|e| e.timestamp >= start && e.timestamp <= end)
}

/// Check if a message passes common filters (time range and channel).
///
/// Uses a cache to avoid repeatedly checking the same channel predicate.
/// The cache is a vector indexed by channel ID.
pub(super) fn passes_common_filters_cached(
    reader: &mut dyn ReaderAccess,
    time_range: Option<(Timestamp, Timestamp)>,
    channel_predicate: &Option<ChannelPredicate<'_>>,
    channel_predicate_cache: &mut Vec<u8>,
    channel_id: u16,
    log_time: Timestamp,
) -> bool {
    if let Some((start, end)) = time_range
        && (log_time < start || log_time > end)
    {
        return false;
    }

    if let Some(predicate) = channel_predicate {
        let cache_len = (u16::MAX as usize) + 1;
        if channel_predicate_cache.len() != cache_len {
            channel_predicate_cache.resize(cache_len, 2);
        }

        let idx = channel_id as usize;
        match channel_predicate_cache[idx] {
            0 => return false,
            1 => return true,
            _ => {}
        }
        let allows = reader.channel_predicate_allows(channel_id, predicate);
        channel_predicate_cache[idx] = if allows { 1 } else { 0 };
        return allows;
    }

    true
}

/// Check if a chunk passes time and channel filters.
///
/// Used to skip chunks that can't possibly contain matching messages.
pub(super) fn chunk_passes_filters(
    reader: &mut dyn ReaderAccess,
    chunk_offset: u64,
    chunk: &Chunk,
    time_range: Option<(Timestamp, Timestamp)>,
    channel_predicate: &Option<ChannelPredicate<'_>>,
) -> bool {
    if let Some((start, end)) = time_range
        && (chunk.message_end_time < start || chunk.message_start_time > end)
    {
        return false;
    }

    if let Some(predicate) = channel_predicate
        && let Some(index) = reader.get_chunk_index(chunk_offset)
    {
        let has_channel = index
            .message_index_offsets
            .keys()
            .any(|id| reader.channel_predicate_allows(*id, predicate));
        return has_channel;
    }

    true
}

/// Extract metadata from a Chunk record.
pub(super) fn chunk_to_metadata(chunk: &Chunk) -> ChunkMetadata {
    ChunkMetadata {
        message_start_time: chunk.message_start_time,
        message_end_time: chunk.message_end_time,
        uncompressed_size: chunk.uncompressed_size,
        uncompressed_crc: chunk.uncompressed_crc,
        compression: chunk.compression.clone(),
        compressed_size: chunk.records.len() as u64,
    }
}
