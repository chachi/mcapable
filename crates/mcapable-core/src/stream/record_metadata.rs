//! RecordMetadata stream implementation.

use super::{
    ChunkMessageState, ReaderAccess, Stream, read_record_header_block, skip_record_payload,
};
use crate::compression::decompress;
use crate::error::Result;
use crate::format::{MCAP_MAGIC_SIZE, MESSAGE_HEADER_SIZE, RECORD_HEADER_SIZE};
use crate::reader::Reader;
use crate::source::BytesSource;
use crate::types::{MessageMetadata, Opcode, RecordMetadata, RecordSource};
use std::marker::PhantomData;

/// Helper to create a RecordMetadata structure.
fn create_record_metadata(
    opcode: Opcode,
    length: u64,
    offset: u64,
    source: RecordSource,
    message: Option<MessageMetadata>,
) -> Result<RecordMetadata> {
    let total_len = (RECORD_HEADER_SIZE as u64)
        .checked_add(length)
        .ok_or(crate::Error::UnexpectedEof(offset))?;
    Ok(RecordMetadata {
        opcode,
        length,
        total_len,
        offset,
        source,
        message,
    })
}

/// Parse message metadata from header bytes.
fn parse_message_metadata(buf: &[u8], data_size: u64) -> MessageMetadata {
    let channel_id = u16::from_le_bytes([buf[0], buf[1]]);
    let sequence = u32::from_le_bytes([buf[2], buf[3], buf[4], buf[5]]);
    let log_time = u64::from_le_bytes([
        buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12], buf[13],
    ]);
    let publish_time = u64::from_le_bytes([
        buf[14], buf[15], buf[16], buf[17], buf[18], buf[19], buf[20], buf[21],
    ]);
    MessageMetadata {
        channel_id,
        sequence,
        log_time,
        publish_time,
        data_size,
    }
}

impl<'a> Stream<'a, RecordMetadata> {
    /// Create a new record metadata stream from a Reader.
    pub(crate) fn new_record_metadata_stream<R: BytesSource>(reader: &'a mut Reader<R>) -> Self {
        use std::io::SeekFrom;

        // Reset to beginning of data section (after magic bytes).
        let _ = reader.reader.seek(SeekFrom::Start(MCAP_MAGIC_SIZE as u64));
        Self {
            reader: reader as &'a mut dyn ReaderAccess,
            done: false,
            time_range: None,
            channel_predicate: None,
            channel_predicate_cache: Vec::new(),
            chunk_filter: None,
            message_filter: None,
            record_filter: None,
            chunk_state: None,
            record_metadata_include_chunk_messages: false,
            record_metadata_include_message_metadata: false,
            record_metadata_include_channel_metadata: false,
            chunk_message_state: None,
            _phantom: PhantomData,
        }
    }

    /// Apply a record type filter to the stream.
    ///
    /// The predicate receives the record type and can filter before reading contents.
    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(Opcode) -> bool + 'a,
    {
        self.record_filter = Some(Box::new(predicate));
        self
    }

    /// Include message metadata for records inside chunk payloads.
    pub fn include_chunk_messages(mut self) -> Self {
        self.record_metadata_include_chunk_messages = true;
        self.record_metadata_include_message_metadata = true;
        self
    }

    /// Include message header metadata for top-level Message records.
    pub fn include_message_metadata(mut self) -> Self {
        self.record_metadata_include_message_metadata = true;
        self
    }

    /// Parse and cache channel records during RecordMetadata iteration.
    pub fn include_channel_metadata(mut self) -> Self {
        self.record_metadata_include_channel_metadata = true;
        self
    }

    fn next_chunk_message_metadata(&mut self) -> Option<Result<RecordMetadata>> {
        let state = self.chunk_message_state.as_mut()?;
        loop {
            if state.position >= state.data.len() {
                self.chunk_message_state = None;
                return None;
            }

            let location =
                match crate::records::get_record_location(state.data.as_ref(), state.position) {
                    Ok(loc) => loc,
                    Err(_) => {
                        self.chunk_message_state = None;
                        return Some(Err(crate::Error::ParseError(crate::ParseError::Opcode(
                            Opcode::Chunk,
                        ))));
                    }
                };
            state.position = location.next_record_start;

            let Some(opcode) = Opcode::from_repr(location.header.opcode) else {
                continue;
            };
            if opcode != Opcode::Message {
                continue;
            }

            let total_len = match (RECORD_HEADER_SIZE as u64).checked_add(location.header.length) {
                Some(v) => v,
                None => {
                    self.chunk_message_state = None;
                    return Some(Err(crate::Error::UnexpectedEof(
                        location.header_start as u64,
                    )));
                }
            };

            let message = if self.record_metadata_include_message_metadata {
                let content = &state.data[location.content_start..location.next_record_start];
                let header = match crate::parser::parse_message_header_from_content(content) {
                    Ok(header) => header,
                    Err(e) => return Some(Err(e)),
                };
                Some(MessageMetadata::from(header))
            } else {
                None
            };

            return Some(Ok(RecordMetadata {
                opcode,
                length: location.header.length,
                total_len,
                offset: location.header_start as u64,
                source: RecordSource::Chunk {
                    chunk_offset: state.chunk_offset,
                },
                message,
            }));
        }
    }

    /// Read and parse chunk payload, setting up state for message iteration.
    fn decompress_and_setup_chunk(&mut self, header: &super::io::RecordHeaderBlock) -> Result<()> {
        let len: usize = header
            .length
            .try_into()
            .map_err(|_| crate::Error::UnexpectedEof(header.offset))?;
        let data = self.reader.source().read_exact_bytes(len)?;
        let chunk = crate::parser::parse_chunk_record(data)?;
        let compression = crate::compression::parse_compression(chunk.compression.as_ref())?;
        let decompressed =
            decompress(compression.as_ref(), chunk.records, chunk.uncompressed_size)?;
        self.chunk_message_state = Some(ChunkMessageState {
            data: decompressed,
            position: 0,
            chunk_offset: header.offset,
        });
        Ok(())
    }

    /// Handle a channel record, parsing and caching it.
    fn handle_channel_record(
        &mut self,
        header: &super::io::RecordHeaderBlock,
    ) -> Result<RecordMetadata> {
        let len: usize = header
            .length
            .try_into()
            .map_err(|_| crate::Error::UnexpectedEof(header.offset))?;
        let data = self.reader.source().read_exact_bytes(len)?;
        let channel = crate::parser::parse_channel_record(data)?;
        self.reader.cache_channel(channel);
        create_record_metadata(
            header.opcode,
            header.length,
            header.offset,
            RecordSource::File,
            None,
        )
    }

    /// Handle a message record, extracting metadata without loading payload.
    fn handle_message_record(
        &mut self,
        header: &super::io::RecordHeaderBlock,
    ) -> Result<RecordMetadata> {
        if header.length < MESSAGE_HEADER_SIZE as u64 {
            return Err(crate::Error::ParseError(crate::ParseError::Opcode(
                Opcode::Message,
            )));
        }
        let header_bytes = self.reader.source().read_exact_bytes(MESSAGE_HEADER_SIZE)?;
        let data_size = header.length - MESSAGE_HEADER_SIZE as u64;
        let message = Some(parse_message_metadata(header_bytes.as_ref(), data_size));
        let remaining = header.length - MESSAGE_HEADER_SIZE as u64;
        if remaining > 0 {
            skip_record_payload(self.reader, remaining)?;
        }
        create_record_metadata(
            header.opcode,
            header.length,
            header.offset,
            RecordSource::File,
            message,
        )
    }
}

impl<'a> Iterator for Stream<'a, RecordMetadata> {
    type Item = Result<RecordMetadata>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
            // First check for pending chunk messages.
            if let Some(item) = self.next_chunk_message_metadata() {
                return Some(item);
            }

            // Read next record header.
            let header = match read_record_header_block(self.reader) {
                Ok(Some(h)) => h,
                Ok(None) => {
                    self.done = true;
                    return None;
                }
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            };

            // Check filters.
            let wants_record = self
                .record_filter
                .as_ref()
                .is_none_or(|filter| filter(header.opcode));
            let wants_messages = self.record_metadata_include_chunk_messages
                && self
                    .record_filter
                    .as_ref()
                    .is_none_or(|filter| filter(Opcode::Message));

            // Handle chunk records specially.
            if header.opcode == Opcode::Chunk && self.record_metadata_include_chunk_messages {
                if !wants_record && !wants_messages {
                    if let Err(e) = skip_record_payload(self.reader, header.length) {
                        self.done = true;
                        return Some(Err(e));
                    }
                    continue;
                }

                if wants_messages {
                    if let Err(e) = self.decompress_and_setup_chunk(&header) {
                        self.done = true;
                        return Some(Err(e));
                    }
                } else if let Err(e) = skip_record_payload(self.reader, header.length) {
                    self.done = true;
                    return Some(Err(e));
                }

                if !wants_record {
                    continue;
                }

                return Some(create_record_metadata(
                    header.opcode,
                    header.length,
                    header.offset,
                    RecordSource::File,
                    None,
                ));
            }

            // Skip unwanted records.
            if !wants_record {
                if let Err(e) = skip_record_payload(self.reader, header.length) {
                    self.done = true;
                    return Some(Err(e));
                }
                continue;
            }

            // Handle special record types.
            let result = if header.opcode == Opcode::Channel
                && self.record_metadata_include_channel_metadata
            {
                self.handle_channel_record(&header)
            } else if header.opcode == Opcode::Message
                && self.record_metadata_include_message_metadata
            {
                self.handle_message_record(&header)
            } else {
                // Generic record - skip payload and return metadata.
                if let Err(e) = skip_record_payload(self.reader, header.length) {
                    self.done = true;
                    return Some(Err(e));
                }
                create_record_metadata(
                    header.opcode,
                    header.length,
                    header.offset,
                    RecordSource::File,
                    None,
                )
            };

            match result {
                Ok(metadata) => return Some(Ok(metadata)),
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            }
        }
    }
}
