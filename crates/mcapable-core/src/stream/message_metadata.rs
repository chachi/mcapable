//! MessageMetadata stream implementation.
//!
//! Fast iteration without reading payload data.

use super::io::read_record_block;
use super::{
    ChunkState, ReaderAccess, Stream,
    filters::{
        cache_schema_or_channel, chunk_might_have_messages_in_range_via_index, chunk_passes_filters,
    },
    passes_common_filters_cached,
};
use crate::compression::decompress;
use crate::error::Result;
use crate::format::MCAP_MAGIC_SIZE;
use crate::parser::{parse_chunk_record, parse_message_header_from_content};
use crate::reader::Reader;
use crate::records::RecordIterator;
use crate::source::BytesSource;
use crate::types::{MessageHeader, MessageMetadata, Opcode};
use std::marker::PhantomData;

impl<'a> Stream<'a, MessageMetadata> {
    /// Create a new message metadata stream from a Reader.
    pub(crate) fn new_message_metadata_stream<R: BytesSource>(reader: &'a mut Reader<R>) -> Self {
        use std::io::SeekFrom;

        // Reset to beginning of data section (after magic bytes)
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

    /// Apply a message header filter to the stream.
    ///
    /// The predicate receives message header before reading the payload.
    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&MessageHeader) -> bool + 'a,
    {
        self.message_filter = Some(Box::new(predicate));
        self
    }

    fn should_include(&mut self, metadata: &MessageMetadata) -> bool {
        passes_common_filters_cached(
            self.reader,
            self.time_range,
            &self.channel_predicate,
            &mut self.channel_predicate_cache,
            metadata.channel_id,
            metadata.log_time,
        )
    }
}

impl<'a> Iterator for Stream<'a, MessageMetadata> {
    type Item = Result<MessageMetadata>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
            // Process messages from current chunk if available
            if let Some(mut chunk_state) = self.chunk_state.take() {
                let iter = RecordIterator::new(chunk_state.data.as_ref(), chunk_state.position);

                for next in iter {
                    let loc = match next {
                        Ok(loc) => loc,
                        Err(_) => break,
                    };

                    chunk_state.position = loc.next_record_start;

                    let opcode = match loc.header.opcode_typed() {
                        Some(op) => op,
                        None => continue,
                    };
                    if opcode != Opcode::Message {
                        continue;
                    }

                    // Parse just the message header, not the payload
                    let content_start = loc.content_start;
                    let content_end = loc.next_record_start;
                    if content_end <= content_start || content_start >= chunk_state.data.len() {
                        break;
                    }

                    let header = match parse_message_header_from_content(
                        &chunk_state.data[content_start..content_end],
                    ) {
                        Ok(h) => h,
                        Err(_) => break,
                    };

                    if let Some(filter) = &self.message_filter
                        && !filter(&header)
                    {
                        continue;
                    }

                    let metadata: MessageMetadata = header.into();
                    if !self.should_include(&metadata) {
                        continue;
                    }

                    self.chunk_state = Some(chunk_state);
                    return Some(Ok(metadata));
                }

                self.chunk_state = None;
                continue;
            }

            // Read next top-level record
            let block = match read_record_block(self.reader) {
                Ok(Some(block)) => block,
                Ok(None) => {
                    self.done = true;
                    return None;
                }
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            };

            if block.opcode == Opcode::Schema || block.opcode == Opcode::Channel {
                cache_schema_or_channel(self.reader, block.opcode, block.data);
                continue;
            }

            if block.opcode == Opcode::DataEnd {
                self.done = true;
                return None;
            }

            if block.opcode == Opcode::Message {
                // Parse header without allocating payload
                if let Some(filter) = &self.message_filter {
                    let header = match parse_message_header_from_content(block.data.as_ref()) {
                        Ok(header) => header,
                        Err(e) => {
                            self.done = true;
                            return Some(Err(e));
                        }
                    };
                    if !filter(&header) {
                        continue;
                    }
                    let metadata: MessageMetadata = header.into();
                    if !self.should_include(&metadata) {
                        continue;
                    }
                    return Some(Ok(metadata));
                } else {
                    let header = match parse_message_header_from_content(block.data.as_ref()) {
                        Ok(header) => header,
                        Err(e) => {
                            self.done = true;
                            return Some(Err(e));
                        }
                    };
                    let metadata: MessageMetadata = header.into();
                    if !self.should_include(&metadata) {
                        continue;
                    }
                    return Some(Ok(metadata));
                }
            }

            if block.opcode != Opcode::Chunk {
                continue;
            }

            let chunk = match parse_chunk_record(block.data) {
                Ok(chunk) => chunk,
                Err(e) => return Some(Err(e)),
            };

            if !chunk_passes_filters(
                self.reader,
                block.offset,
                &chunk,
                self.time_range,
                &self.channel_predicate,
            ) {
                continue;
            }

            if let Some((start, end)) = self.time_range
                && !chunk_might_have_messages_in_range_via_index(
                    self.reader,
                    block.offset,
                    start,
                    end,
                    &self.channel_predicate,
                )
            {
                continue;
            }

            let compression = match crate::compression::parse_compression(&chunk.compression) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };

            let decompressed = match decompress(
                compression.as_ref(),
                chunk.records.clone(),
                chunk.uncompressed_size,
            ) {
                Ok(d) => d,
                Err(e) => return Some(Err(e)),
            };

            self.chunk_state = Some(ChunkState {
                data: decompressed,
                position: 0,
            });
        }
    }
}
