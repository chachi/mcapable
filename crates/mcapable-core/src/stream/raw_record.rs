//! RawRecord stream implementation.

use super::{ReaderAccess, Stream, read_record_header_block, skip_record_payload};
use crate::error::Result;
use crate::format::{MCAP_MAGIC_SIZE, RECORD_HEADER_SIZE};
use crate::reader::Reader;
use crate::source::BytesSource;
use crate::types::{Opcode, RawRecord};
use std::io::SeekFrom;
use std::marker::PhantomData;

impl<'a> Stream<'a, RawRecord> {
    /// Create a new raw record stream from a Reader.
    pub(crate) fn new_raw_record_stream<R: BytesSource>(reader: &'a mut Reader<R>) -> Self {
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
}

impl<'a> Iterator for Stream<'a, RawRecord> {
    type Item = Result<RawRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
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

            if let Some(ref filter) = self.record_filter
                && !filter(header.opcode)
            {
                if let Err(e) = skip_record_payload(self.reader, header.length) {
                    self.done = true;
                    return Some(Err(e));
                }
                continue;
            }

            let total_len_u64 = (RECORD_HEADER_SIZE as u64)
                .checked_add(header.length)
                .ok_or(crate::Error::UnexpectedEof(header.offset));
            let total_len_u64 = match total_len_u64 {
                Ok(v) => v,
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            };
            let total_len: usize = match total_len_u64.try_into() {
                Ok(v) => v,
                Err(_) => {
                    self.done = true;
                    return Some(Err(crate::Error::UnexpectedEof(header.offset)));
                }
            };

            // `read_record_header_block` advances past the record header; seek back and read the
            // full contiguous record so callers can copy it back out with a single write call.
            if let Err(e) = self.reader.source().seek(SeekFrom::Start(header.offset)) {
                self.done = true;
                return Some(Err(e.into()));
            }
            let data = match self.reader.source().read_exact_bytes(total_len) {
                Ok(d) => d,
                Err(e) => {
                    self.done = true;
                    return Some(Err(e.into()));
                }
            };
            // Seek back to resume linear scanning.
            if let Err(e) = self
                .reader
                .source()
                .seek(SeekFrom::Start(header.data_start + header.length))
            {
                self.done = true;
                return Some(Err(e.into()));
            }

            let block = RawRecord {
                opcode: header.opcode,
                data,
                total_len: total_len_u64,
                offset: header.offset,
            };
            return Some(Ok(block));
        }
    }
}
