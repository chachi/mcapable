//! Chunk stream implementation.

use super::{
    FilteredChunksLinear, FilteredChunksViaIndex, ReaderAccess, Stream,
    filters::{cache_schema_or_channel, chunk_passes_filters, chunk_to_metadata},
};
use crate::error::Result;
use crate::format::MCAP_MAGIC_SIZE;
use crate::reader::Reader;
use crate::source::BytesSource;
use crate::types::Chunk;
use std::marker::PhantomData;

impl<'a> Stream<'a, Chunk> {
    /// Create a new chunk stream from a Reader.
    pub(crate) fn new_chunk_stream<R: BytesSource>(reader: &'a mut Reader<R>) -> Self {
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

    /// Apply a chunk metadata filter to the stream.
    ///
    /// The predicate receives chunk metadata before decompression.
    /// This avoids decompressing chunks that won't be used.
    ///
    /// # Examples
    ///
    /// Only decompress chunks with specific compression:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let stream = reader.chunks()
    ///     .filter(|meta| meta.compression == "zstd");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Skip large chunks:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let stream = reader.chunks()
    ///     .filter(|meta| meta.uncompressed_size < 1024 * 1024);
    /// # Ok(())
    /// # }
    /// ```
    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&crate::types::ChunkMetadata) -> bool + 'a,
    {
        self.chunk_filter = Some(Box::new(predicate));
        self
    }

    /// Get a chunk by index using random access.
    ///
    /// Uses ChunkIndex for O(log n) seeking when available in summary.
    /// Falls back to linear scan if no index is available.
    ///
    /// # Arguments
    ///
    /// * `index` - The zero-based index of the chunk to retrieve
    ///
    /// # Returns
    ///
    /// * `Ok(Some(chunk))` - The chunk at the given index
    /// * `Ok(None)` - Index is out of bounds
    /// * `Err(_)` - I/O or parsing error
    pub fn get(&mut self, index: usize) -> Result<Option<Chunk>> {
        use super::get_nth_from_chunks;

        let chunk_indexes = self.reader.chunk_indexes()?;
        if !chunk_indexes.is_empty() {
            let iter = FilteredChunksViaIndex::new(
                self.reader,
                chunk_indexes,
                self.time_range,
                &self.channel_predicate,
                &self.chunk_filter,
            )?;
            return get_nth_from_chunks(iter, index, |_, chunk, found, target| {
                if *found == target {
                    return Ok(Some(chunk));
                }
                *found += 1;
                Ok(None)
            });
        }

        let iter = FilteredChunksLinear::new(
            self.reader,
            self.time_range,
            &self.channel_predicate,
            &self.chunk_filter,
        );
        get_nth_from_chunks(iter, index, |_, chunk, found, target| {
            if *found == target {
                return Ok(Some(chunk));
            }
            *found += 1;
            Ok(None)
        })
    }
}

impl<'a> Iterator for Stream<'a, Chunk> {
    type Item = Result<Chunk>;

    fn next(&mut self) -> Option<Self::Item> {
        use super::io::read_record_block;
        use crate::types::Opcode;

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
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            };

            if block.opcode == Opcode::DataEnd {
                self.done = true;
                return None;
            }

            // Only process Chunk records
            if block.opcode != Opcode::Chunk {
                if matches!(block.opcode, Opcode::Schema | Opcode::Channel) {
                    cache_schema_or_channel(self.reader, block.opcode, block.data);
                }
                continue; // Skip non-chunk records
            }

            // Parse the chunk
            let chunk = match crate::parser::parse_chunk_record(block.data) {
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

            // Apply chunk_filter if set
            if let Some(ref filter) = self.chunk_filter {
                let metadata = chunk_to_metadata(&chunk);
                if !filter(&metadata) {
                    continue; // Skip chunk that doesn't match filter
                }
            }

            return Some(Ok(chunk));
        }
    }
}
