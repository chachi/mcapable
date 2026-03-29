//! Record stream implementation.

use super::io::RecordBlock;
use super::{ReaderAccess, Stream, read_record_header_block, skip_record_payload};
use crate::error::Result;
use crate::format::MCAP_MAGIC_SIZE;
use crate::reader::Reader;
use crate::source::BytesSource;
use crate::types::{Opcode, Record};
use bytes::Bytes;
use std::marker::PhantomData;

impl<'a> Stream<'a, Record> {
    /// Create a new record stream from a Reader.
    pub(crate) fn new_record_stream<R: BytesSource>(reader: &'a mut Reader<R>) -> Self {
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

    /// Apply a record type filter to the stream.
    ///
    /// The predicate receives the record type and can filter before parsing contents.
    /// This avoids parsing record bodies for unwanted types.
    ///
    /// # Examples
    ///
    /// Only iterate over message and chunk records:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use mcapable_core::types::Opcode;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let stream = reader.records()
    ///     .filter(|rt| matches!(rt, Opcode::Message | Opcode::Chunk));
    /// # Ok(())
    /// # }
    /// ```
    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(Opcode) -> bool + 'a,
    {
        self.record_filter = Some(Box::new(predicate));
        self
    }
}

impl<'a> Iterator for Stream<'a, Record> {
    type Item = Result<Record>;

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

            // Apply record type filter if present (before reading the body).
            if let Some(ref filter) = self.record_filter
                && !filter(header.opcode)
            {
                if let Err(e) = skip_record_payload(self.reader, header.length) {
                    self.done = true;
                    return Some(Err(e));
                }
                continue;
            }

            let len: usize = match header.length.try_into() {
                Ok(v) => v,
                Err(_) => {
                    self.done = true;
                    return Some(Err(crate::Error::UnexpectedEof(header.offset)));
                }
            };
            let data = match self.reader.source().read_exact_bytes(len) {
                Ok(d) => d,
                Err(e) => {
                    self.done = true;
                    return Some(Err(e.into()));
                }
            };
            let block = RecordBlock {
                opcode: header.opcode,
                data,
                offset: header.offset,
            };

            // Parse based on opcode and create Record
            let record = match block.opcode {
                Opcode::Header => parse_header(block.data),
                Opcode::Footer => match parse_footer(block.data) {
                    Ok(footer) => {
                        self.done = true;
                        Ok(footer)
                    }
                    Err(e) => Err(e),
                },
                Opcode::Schema => parse_and_cache_schema(self.reader, block.data),
                Opcode::Channel => parse_and_cache_channel(self.reader, block.data),
                Opcode::Message => parse_message_owned(block.data),
                Opcode::Chunk => parse_chunk(block.data),
                Opcode::MessageIndex => parse_message_index(block.data),
                Opcode::ChunkIndex => parse_chunk_index(block.data),
                Opcode::Attachment => parse_and_cache_attachment(self.reader, block.data),
                Opcode::AttachmentIndex => parse_attachment_index(block.data),
                Opcode::Statistics => parse_statistics(block.data),
                Opcode::Metadata => parse_and_cache_metadata(self.reader, block.data),
                Opcode::MetadataIndex => parse_metadata_index(block.data),
                Opcode::SummaryOffset => parse_summary_offset(block.data),
                Opcode::DataEnd => parse_data_end(block.data),
            };

            return Some(record);
        }
    }
}

// Helper functions for parsing each record type
fn parse_header(data: Bytes) -> Result<Record> {
    crate::parser::parse_header_record(data).map(Record::Header)
}

fn parse_footer(data: Bytes) -> Result<Record> {
    crate::parser::parse_footer_record(data).map(Record::Footer)
}

fn parse_and_cache_schema(reader: &mut dyn ReaderAccess, data: Bytes) -> Result<Record> {
    crate::parser::parse_schema_record(data).map(|schema| {
        reader.cache_schema(schema.clone());
        Record::Schema(schema)
    })
}

fn parse_and_cache_channel(reader: &mut dyn ReaderAccess, data: Bytes) -> Result<Record> {
    crate::parser::parse_channel_record(data).map(|channel| {
        reader.cache_channel(channel.clone());
        Record::Channel(channel)
    })
}

fn parse_message_owned(data: Bytes) -> Result<Record> {
    crate::parser::parse_message_record(data).map(|raw_msg| Record::Message(raw_msg.into()))
}

fn parse_chunk(data: Bytes) -> Result<Record> {
    crate::parser::parse_chunk_record(data).map(Record::Chunk)
}

fn parse_message_index(data: Bytes) -> Result<Record> {
    crate::parser::parse_message_index_record(data).map(Record::MessageIndex)
}

fn parse_chunk_index(data: Bytes) -> Result<Record> {
    crate::parser::parse_chunk_index_record(data).map(Record::ChunkIndex)
}

fn parse_and_cache_attachment(reader: &mut dyn ReaderAccess, data: Bytes) -> Result<Record> {
    crate::parser::parse_attachment_record(data).map(|attachment| {
        reader.cache_attachment(attachment.clone());
        Record::Attachment(attachment)
    })
}

fn parse_attachment_index(data: Bytes) -> Result<Record> {
    crate::parser::parse_attachment_index_record(data).map(Record::AttachmentIndex)
}

fn parse_statistics(data: Bytes) -> Result<Record> {
    crate::parser::parse_statistics_record(data).map(Record::Statistics)
}

fn parse_and_cache_metadata(reader: &mut dyn ReaderAccess, data: Bytes) -> Result<Record> {
    crate::parser::parse_metadata_record(data).map(|metadata| {
        reader.cache_metadata(metadata.clone());
        Record::Metadata(metadata)
    })
}

fn parse_metadata_index(data: Bytes) -> Result<Record> {
    crate::parser::parse_metadata_index_record(data).map(Record::MetadataIndex)
}

fn parse_summary_offset(data: Bytes) -> Result<Record> {
    crate::parser::parse_summary_offset_record(data).map(|_| Record::SummaryOffset)
}

fn parse_data_end(data: Bytes) -> Result<Record> {
    crate::parser::parse_data_end_record(data).map(|_| Record::DataEnd)
}
