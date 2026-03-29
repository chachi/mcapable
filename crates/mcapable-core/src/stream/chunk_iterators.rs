//! Iterator types for filtered chunk access.
//!
//! Provides two strategies for iterating over chunks:
//! - `FilteredChunksViaIndex`: Uses ChunkIndex for fast random access
//! - `FilteredChunksLinear`: Sequential scan through the file

use super::filters::{cache_schema_or_channel, chunk_passes_filters, chunk_to_metadata};
use super::io::{read_chunk_at, read_record_block};
use super::{ChannelPredicate, ChunkFilterPredicate, ReaderAccess};
use crate::error::Result;
use crate::format::MCAP_MAGIC_SIZE;
use crate::types::{Chunk, ChunkIndex, Opcode, Timestamp};
use std::io::SeekFrom;
use std::sync::Arc;

/// Iterator over chunks using ChunkIndex for fast seeking.
///
/// When the MCAP file has a summary with ChunkIndex records, this iterator
/// can jump directly to relevant chunks without scanning the entire file.
pub(super) struct FilteredChunksViaIndex<'b, 'a> {
    pub(super) reader: &'b mut dyn ReaderAccess,
    pub(super) chunk_indexes: Arc<[ChunkIndex]>,
    pub(super) pos: usize,
    pub(super) time_range: Option<(Timestamp, Timestamp)>,
    pub(super) channel_predicate: &'b Option<ChannelPredicate<'a>>,
    pub(super) chunk_filter: &'b Option<ChunkFilterPredicate<'a>>,
}

impl<'b, 'a> FilteredChunksViaIndex<'b, 'a> {
    pub(super) fn new(
        reader: &'b mut dyn ReaderAccess,
        chunk_indexes: Arc<[ChunkIndex]>,
        time_range: Option<(Timestamp, Timestamp)>,
        channel_predicate: &'b Option<ChannelPredicate<'a>>,
        chunk_filter: &'b Option<ChunkFilterPredicate<'a>>,
    ) -> Result<Self> {
        if !chunk_indexes.is_empty() {
            reader.ensure_summary_metadata()?;
        }
        Ok(Self {
            reader,
            chunk_indexes,
            pos: 0,
            time_range,
            channel_predicate,
            chunk_filter,
        })
    }
}

impl<'b, 'a> Iterator for FilteredChunksViaIndex<'b, 'a> {
    type Item = Result<(u64, Chunk)>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.pos < self.chunk_indexes.len() {
            let chunk_index = &self.chunk_indexes[self.pos];
            self.pos += 1;

            if let Some((start, end)) = self.time_range
                && (chunk_index.message_end_time < start || chunk_index.message_start_time > end)
            {
                continue;
            }

            let chunk_offset = chunk_index.chunk_start_offset;
            let chunk = match read_chunk_at(self.reader, chunk_offset) {
                Ok(Some(chunk)) => chunk,
                Ok(None) => continue,
                Err(e) => return Some(Err(e)),
            };

            if !chunk_passes_filters(
                self.reader,
                chunk_offset,
                &chunk,
                self.time_range,
                self.channel_predicate,
            ) {
                continue;
            }

            if let Some(filter) = self.chunk_filter {
                let metadata = chunk_to_metadata(&chunk);
                if !filter(&metadata) {
                    continue;
                }
            }

            return Some(Ok((chunk_offset, chunk)));
        }

        None
    }
}

/// Iterator over chunks via linear scan.
///
/// When ChunkIndex is unavailable, this iterator reads the file sequentially
/// and returns matching chunks. Slower than indexed access but works for all files.
pub(super) struct FilteredChunksLinear<'b, 'a> {
    pub(super) reader: &'b mut dyn ReaderAccess,
    pub(super) time_range: Option<(Timestamp, Timestamp)>,
    pub(super) channel_predicate: &'b Option<ChannelPredicate<'a>>,
    pub(super) chunk_filter: &'b Option<ChunkFilterPredicate<'a>>,
    pub(super) done: bool,
}

impl<'b, 'a> FilteredChunksLinear<'b, 'a> {
    pub(super) fn new(
        reader: &'b mut dyn ReaderAccess,
        time_range: Option<(Timestamp, Timestamp)>,
        channel_predicate: &'b Option<ChannelPredicate<'a>>,
        chunk_filter: &'b Option<ChunkFilterPredicate<'a>>,
    ) -> Self {
        let _ = reader
            .source()
            .seek(SeekFrom::Start(MCAP_MAGIC_SIZE as u64));
        Self {
            reader,
            time_range,
            channel_predicate,
            chunk_filter,
            done: false,
        }
    }
}

impl<'b, 'a> Iterator for FilteredChunksLinear<'b, 'a> {
    type Item = Result<(u64, Chunk)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
            let block = match read_record_block(self.reader) {
                Ok(Some(block)) => block,
                Ok(None) => {
                    self.done = true;
                    return None;
                }
                Err(e) => return Some(Err(e)),
            };

            if matches!(block.opcode, Opcode::Schema | Opcode::Channel) {
                cache_schema_or_channel(self.reader, block.opcode, block.data);
                continue;
            }

            if block.opcode == Opcode::DataEnd {
                self.done = true;
                return None;
            }

            if block.opcode != Opcode::Chunk {
                continue;
            }

            let chunk = match crate::parser::parse_chunk_record(block.data) {
                Ok(chunk) => chunk,
                Err(_) => {
                    return Some(Err(crate::Error::ParseError(crate::ParseError::Opcode(
                        Opcode::Chunk,
                    ))));
                }
            };

            if !chunk_passes_filters(
                self.reader,
                block.offset,
                &chunk,
                self.time_range,
                self.channel_predicate,
            ) {
                continue;
            }

            if let Some(filter) = self.chunk_filter {
                let metadata = chunk_to_metadata(&chunk);
                if !filter(&metadata) {
                    continue;
                }
            }

            return Some(Ok((block.offset, chunk)));
        }
    }
}

/// Generic helper for finding the Nth item across multiple chunks.
///
/// Used by random access implementations to iterate through chunks and
/// count items until finding the target index.
pub(super) fn get_nth_from_chunks<O, I, F>(
    iter: I,
    index: usize,
    mut per_chunk: F,
) -> Result<Option<O>>
where
    I: Iterator<Item = Result<(u64, Chunk)>>,
    F: FnMut(u64, Chunk, &mut usize, usize) -> Result<Option<O>>,
{
    let mut found = 0usize;
    for result in iter {
        let (offset, chunk) = result?;
        if let Some(item) = per_chunk(offset, chunk, &mut found, index)? {
            return Ok(Some(item));
        }
    }
    Ok(None)
}
