//! Message stream implementation.

use super::{
    FilteredChunksLinear, FilteredChunksViaIndex, ReaderAccess, Stream, next_message_common,
    passes_common_filters_cached,
};
use crate::compression::decompress;
use crate::error::Result;
use crate::format::MCAP_MAGIC_SIZE;
use crate::reader::Reader;
use crate::records::RecordIterator;
use crate::source::BytesSource;
use crate::types::{Message, MessageHeader, Opcode, RawMessage};
use std::marker::PhantomData;

impl<'a> Stream<'a, Message> {
    /// Create a new message stream from a Reader.
    pub(crate) fn new_message_stream<R: BytesSource>(reader: &'a mut Reader<R>) -> Self {
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
    /// The predicate receives message header before loading schema/channel metadata.
    /// This can avoid metadata lookups for messages that won't be used.
    ///
    /// # Examples
    ///
    /// Filter by sequence number:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let stream = reader.messages()
    ///     .filter(|hdr| hdr.sequence % 10 == 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&MessageHeader) -> bool + 'a,
    {
        self.message_filter = Some(Box::new(predicate));
        self
    }

    fn should_include(&mut self, msg: &Message) -> bool {
        passes_common_filters_cached(
            self.reader,
            self.time_range,
            &self.channel_predicate,
            &mut self.channel_predicate_cache,
            msg.channel_id,
            msg.log_time,
        )
    }
}

impl<'a> Iterator for Stream<'a, Message> {
    type Item = Result<Message>;

    fn next(&mut self) -> Option<Self::Item> {
        next_message_common(
            self,
            |raw_msg| raw_msg.into(),
            |s, msg| s.should_include(msg),
        )
    }
}

// ============================================================================
// Random access methods (Index-like API)
// ============================================================================

impl<'a> Stream<'a, Message> {
    /// Get a message by index using random access.
    ///
    /// Uses ChunkIndex for O(log n) seeking when available in summary.
    /// Falls back to linear scan if no index is available.
    ///
    /// # Arguments
    ///
    /// * `index` - The zero-based index of the message to retrieve
    ///
    /// # Returns
    ///
    /// * `Ok(Some(message))` - The message at the given index
    /// * `Ok(None)` - Index is out of bounds
    /// * `Err(_)` - I/O or parsing error
    pub fn get(&mut self, index: usize) -> Result<Option<Message>> {
        let chunk_indexes = self.reader.chunk_indexes()?;
        let time_range = self.time_range;
        let channel_predicate = &self.channel_predicate;
        let chunk_filter = &self.chunk_filter;
        let message_filter = &self.message_filter;

        let mut found = 0usize;
        let mut channel_predicate_cache: Vec<u8> = Vec::new();

        if !chunk_indexes.is_empty() {
            let mut iter = FilteredChunksViaIndex::new(
                self.reader,
                chunk_indexes,
                time_range,
                channel_predicate,
                chunk_filter,
            )?;

            while let Some(result) = iter.next() {
                let (_, chunk) = result?;
                let compression = crate::compression::parse_compression(&chunk.compression)?;
                let backing = decompress(
                    compression.as_ref(),
                    chunk.records.clone(),
                    chunk.uncompressed_size,
                )?;

                for record in RecordIterator::new(&backing, 0) {
                    let loc = match record {
                        Ok(loc) => loc,
                        Err(_) => break,
                    };
                    let opcode = match loc.header.opcode_typed() {
                        Some(opcode) => opcode,
                        None => continue,
                    };
                    if opcode != Opcode::Message {
                        continue;
                    }

                    let content_start = loc.content_start;
                    let content_end = loc.next_record_start;
                    let (header, payload) = match crate::parser::parse_message_from_backing_range(
                        &backing,
                        content_start,
                        content_end,
                    ) {
                        Ok(parsed) => parsed,
                        Err(_) => continue,
                    };
                    let msg: Message = RawMessage::new(
                        header.channel_id,
                        header.sequence,
                        header.log_time,
                        header.publish_time,
                        payload,
                    )
                    .into();

                    if !passes_common_filters_cached(
                        iter.reader,
                        time_range,
                        channel_predicate,
                        &mut channel_predicate_cache,
                        msg.channel_id,
                        msg.log_time,
                    ) {
                        continue;
                    }

                    if let Some(filter) = message_filter
                        && !filter(&header)
                    {
                        continue;
                    }

                    if found == index {
                        return Ok(Some(msg));
                    }
                    found += 1;
                }
            }

            return Ok(None);
        }

        let mut iter =
            FilteredChunksLinear::new(self.reader, time_range, channel_predicate, chunk_filter);

        while let Some(result) = iter.next() {
            let (_, chunk) = result?;
            let compression = crate::compression::parse_compression(&chunk.compression)?;
            let backing = decompress(
                compression.as_ref(),
                chunk.records.clone(),
                chunk.uncompressed_size,
            )?;

            for record in RecordIterator::new(&backing, 0) {
                let loc = match record {
                    Ok(loc) => loc,
                    Err(_) => break,
                };
                let opcode = match loc.header.opcode_typed() {
                    Some(opcode) => opcode,
                    None => continue,
                };
                if opcode != Opcode::Message {
                    continue;
                }

                let content_start = loc.content_start;
                let content_end = loc.next_record_start;
                let (header, payload) = match crate::parser::parse_message_from_backing_range(
                    &backing,
                    content_start,
                    content_end,
                ) {
                    Ok(parsed) => parsed,
                    Err(_) => continue,
                };
                let msg: Message = RawMessage::new(
                    header.channel_id,
                    header.sequence,
                    header.log_time,
                    header.publish_time,
                    payload,
                )
                .into();

                if !passes_common_filters_cached(
                    iter.reader,
                    time_range,
                    channel_predicate,
                    &mut channel_predicate_cache,
                    msg.channel_id,
                    msg.log_time,
                ) {
                    continue;
                }

                if let Some(filter) = message_filter
                    && !filter(&header)
                {
                    continue;
                }

                if found == index {
                    return Ok(Some(msg));
                }
                found += 1;
            }
        }

        Ok(None)
    }
}
