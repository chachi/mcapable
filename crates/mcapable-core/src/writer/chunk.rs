use bytes::Bytes;

use crate::error::{Error, Result};
use crate::types::Opcode;
use crate::zero_copy::ByteStr;

use super::constants::MESSAGE_RECORD_PREFIX_LEN;
use super::types::ChunkIndexInfo;

#[derive(Debug)]
pub(crate) struct ChunkState {
    pub(crate) options: super::api::ChunkOptions,
    pub(crate) message_prefixes: Vec<u8>,
    pub(crate) payloads: Vec<Bytes>,
    pub(crate) uncompressed_len: usize,
    pub(crate) uncompressed_crc: Option<crc32fast::Hasher>,
    pub(crate) message_start_time: u64,
    pub(crate) message_end_time: u64,
    pub(crate) has_messages: bool,
}

pub(crate) struct ChunkFlushData {
    pub(crate) options: super::api::ChunkOptions,
    pub(crate) message_start_time: u64,
    pub(crate) message_end_time: u64,
    pub(crate) message_prefixes: Vec<u8>,
    pub(crate) payloads: Vec<Bytes>,
    pub(crate) uncompressed_size: u64,
    pub(crate) uncompressed_crc: u32,
}

impl ChunkState {
    pub(crate) fn new(options: super::api::ChunkOptions) -> Self {
        let crc_hasher = if options.include_crc {
            Some(crc32fast::Hasher::new())
        } else {
            None
        };
        Self {
            options,
            message_prefixes: Vec::new(),
            payloads: Vec::new(),
            uncompressed_len: 0,
            uncompressed_crc: crc_hasher,
            message_start_time: 0,
            message_end_time: 0,
            has_messages: false,
        }
    }

    pub(crate) fn should_flush(&self, force: bool) -> bool {
        self.has_messages && (force || self.uncompressed_len >= self.options.max_uncompressed_bytes)
    }

    pub(crate) fn push_message(
        &mut self,
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        payload: Bytes,
    ) {
        let payload_len_bytes = payload.len();
        let payload_len = (crate::format::MESSAGE_HEADER_SIZE + payload.len()) as u64;
        let mut prefix = [0u8; MESSAGE_RECORD_PREFIX_LEN];
        prefix[0] = Opcode::Message.as_u8();
        prefix[1..9].copy_from_slice(&payload_len.to_le_bytes());
        prefix[9..11].copy_from_slice(&channel_id.to_le_bytes());
        prefix[11..15].copy_from_slice(&sequence.to_le_bytes());
        prefix[15..23].copy_from_slice(&log_time.to_le_bytes());
        prefix[23..31].copy_from_slice(&publish_time.to_le_bytes());

        if let Some(hasher) = &mut self.uncompressed_crc {
            hasher.update(&prefix);
            hasher.update(payload.as_ref());
        }

        self.message_prefixes.extend_from_slice(&prefix);
        self.payloads.push(payload);
        self.uncompressed_len = self
            .uncompressed_len
            .saturating_add(prefix.len())
            .saturating_add(payload_len_bytes);

        if !self.has_messages {
            self.message_start_time = log_time;
            self.message_end_time = log_time;
            self.has_messages = true;
        } else {
            self.message_start_time = self.message_start_time.min(log_time);
            self.message_end_time = self.message_end_time.max(log_time);
        }
    }

    pub(crate) fn take_for_flush(&mut self) -> ChunkFlushData {
        let options = self.options.clone();
        let message_start_time = self.message_start_time;
        let message_end_time = self.message_end_time;
        let message_prefixes = std::mem::take(&mut self.message_prefixes);
        let payloads = std::mem::take(&mut self.payloads);
        let uncompressed_size: u64 = std::mem::take(&mut self.uncompressed_len) as u64;
        let uncompressed_crc = match self.uncompressed_crc.take() {
            Some(hasher) => hasher.finalize(),
            None => 0,
        };
        if self.options.include_crc {
            self.uncompressed_crc = Some(crc32fast::Hasher::new());
        }
        self.has_messages = false;

        ChunkFlushData {
            options,
            message_start_time,
            message_end_time,
            message_prefixes,
            payloads,
            uncompressed_size,
            uncompressed_crc,
        }
    }

    pub(crate) fn recycle_buffers(&mut self, message_prefixes: Vec<u8>, payloads: Vec<Bytes>) {
        self.message_prefixes = message_prefixes;
        self.message_prefixes.clear();
        self.payloads = payloads;
        self.payloads.clear();
    }
}

pub(crate) struct PreparedChunk {
    pub(crate) index: ChunkIndexInfo,
    pub(crate) record_header: [u8; crate::format::RECORD_HEADER_SIZE],
    pub(crate) record_prefix: Vec<u8>,
    pub(crate) compressed_body: Option<Vec<u8>>,
    pub(crate) write_uncompressed_records: bool,
}

pub(crate) fn prepare_chunk_for_write(
    sink_position: u64,
    flush: &ChunkFlushData,
) -> Result<PreparedChunk> {
    let ChunkFlushData {
        options,
        message_start_time,
        message_end_time,
        message_prefixes: _,
        payloads: _,
        uncompressed_size,
        uncompressed_crc,
    } = flush;

    let (compression_str, compressed_body, compressed_size, write_uncompressed_records) =
        match options.compression.as_ref() {
            None => (String::new(), None, *uncompressed_size, true),
            Some(compression) => {
                let compression_str = compression.to_string();
                let compressed = crate::compression::compress_chunk_records(
                    compression,
                    &flush.message_prefixes,
                    &flush.payloads,
                )?;
                let compressed_size = compressed.len() as u64;
                (compression_str, Some(compressed), compressed_size, false)
            }
        };

    let compression_bytes = compression_str.as_bytes();
    let compression_len: u32 = compression_bytes
        .len()
        .try_into()
        .map_err(|_| Error::InvalidRecord("compression string exceeds u32".to_string()))?;

    let payload_len = 8u64 + 8 + 8 + 4 + 4 + compression_len as u64 + 8 + compressed_size;

    let chunk_start_offset = sink_position;
    let chunk_length = crate::format::RECORD_HEADER_SIZE as u64 + payload_len;

    let index = ChunkIndexInfo {
        message_start_time: *message_start_time,
        message_end_time: *message_end_time,
        chunk_start_offset,
        chunk_length,
        compression: ByteStr::from(compression_str.as_str()),
        compressed_size,
        uncompressed_size: *uncompressed_size,
    };

    let mut record_header = [0u8; crate::format::RECORD_HEADER_SIZE];
    record_header[0] = Opcode::Chunk.as_u8();
    record_header[1..].copy_from_slice(&payload_len.to_le_bytes());

    let mut record_prefix = Vec::with_capacity(32 + compression_bytes.len() + 8);
    record_prefix.extend_from_slice(&message_start_time.to_le_bytes());
    record_prefix.extend_from_slice(&message_end_time.to_le_bytes());
    record_prefix.extend_from_slice(&uncompressed_size.to_le_bytes());
    record_prefix.extend_from_slice(&uncompressed_crc.to_le_bytes());
    record_prefix.extend_from_slice(&compression_len.to_le_bytes());
    record_prefix.extend_from_slice(compression_bytes);
    record_prefix.extend_from_slice(&compressed_size.to_le_bytes());

    Ok(PreparedChunk {
        index,
        record_header,
        record_prefix,
        compressed_body,
        write_uncompressed_records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_state_tracks_len_and_crc() {
        let mut state = ChunkState::new(super::super::api::ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 10_000,
            include_crc: true,
        });

        state.push_message(1, 0, 10, 10, Bytes::from_static(b"abc"));
        state.push_message(1, 1, 20, 20, Bytes::from_static(b""));
        assert!(state.has_messages);
        assert_eq!(state.message_start_time, 10);
        assert_eq!(state.message_end_time, 20);

        let flush = state.take_for_flush();
        let mut expected = Vec::new();
        for i in 0..flush.payloads.len() {
            expected.extend_from_slice(
                &flush.message_prefixes
                    [i * MESSAGE_RECORD_PREFIX_LEN..(i + 1) * MESSAGE_RECORD_PREFIX_LEN],
            );
            expected.extend_from_slice(flush.payloads[i].as_ref());
        }
        assert_eq!(flush.uncompressed_size as usize, expected.len());
        assert_eq!(
            flush.uncompressed_crc,
            crate::compression::calculate_crc(&expected)
        );
    }

    #[test]
    fn chunk_state_crc_is_zero_when_disabled() {
        let mut state = ChunkState::new(super::super::api::ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 10_000,
            include_crc: false,
        });

        state.push_message(1, 0, 10, 10, Bytes::from_static(b"abc"));
        state.push_message(1, 1, 20, 20, Bytes::from_static(b"xyz"));
        assert!(state.uncompressed_crc.is_none());

        let flush = state.take_for_flush();
        assert_eq!(flush.uncompressed_crc, 0);
        assert!(flush.uncompressed_size > 0);
    }

    #[test]
    fn prepared_chunk_crc_field_is_zero_when_disabled() {
        let mut state = ChunkState::new(super::super::api::ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 10_000,
            include_crc: false,
        });

        state.push_message(1, 0, 100, 100, Bytes::from_static(b"hello"));
        let flush = state.take_for_flush();
        let prepared = prepare_chunk_for_write(0, &flush).unwrap();

        // CRC field is at bytes 24..28 of the record prefix
        // (after message_start_time:8 + message_end_time:8 + uncompressed_size:8)
        let crc_bytes = &prepared.record_prefix[24..28];
        let crc_value = u32::from_le_bytes(crc_bytes.try_into().unwrap());
        assert_eq!(crc_value, 0);
    }
}
