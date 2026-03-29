//! Stream types for iterating over MCAP data.
//!
//! Streams borrow from a Reader and iterate over specific record types.
//! The underlying data source type is hidden behind a trait object.

use crate::error::Result;
use crate::types::{Channel, ChunkMetadata, MessageHeader, Opcode, Schema, Timestamp};
use bytes::Bytes;
use std::marker::PhantomData;

// Submodules
mod chunk;
mod chunk_iterators;
mod filters;
mod io;
mod iter_core;
mod message;
mod message_metadata;
mod parsed;
mod raw_message;
mod raw_record;
mod reader_access;
mod record;
mod record_metadata;
#[cfg(feature = "parsers")]
mod schema_defaults;
#[cfg(feature = "parsers")]
pub mod schema_parser;

// Re-exports
pub(super) use iter_core::{ChunkMessageState, ChunkState, next_message_common};
#[allow(unused_imports)] // Re-exported for stream::ParsedStream access.
pub use parsed::{ParsedStream, ParsedStreamBuilder};
pub(crate) use reader_access::ReaderAccess;

// Internal imports
use chunk_iterators::{FilteredChunksLinear, FilteredChunksViaIndex, get_nth_from_chunks};
use filters::passes_common_filters_cached;
use io::{read_record_header_block, skip_record_payload};

/// Predicate function for filtering by channel.
///
/// Takes a channel reference, returns true if the message should be included.
/// The Stream will handle lazy loading of channel metadata when needed.
pub(crate) type ChannelPredicate<'a> = Box<dyn Fn(&Channel) -> bool + 'a>;

/// Predicate function for filtering chunks by metadata.
///
/// Evaluated before decompression to avoid unnecessary work.
pub(crate) type ChunkFilterPredicate<'a> = Box<dyn Fn(&ChunkMetadata) -> bool + 'a>;

/// Predicate function for filtering messages by header.
///
/// Evaluated before loading full message data.
pub(crate) type MessageFilterPredicate<'a> = Box<dyn Fn(&MessageHeader) -> bool + 'a>;

/// Predicate function for filtering records by type.
///
/// Evaluated before parsing record contents.
pub(crate) type RecordFilterPredicate<'a> = Box<dyn Fn(Opcode) -> bool + 'a>;

/// Predicate function for matching channels/schemas to parsers.
///
/// Used by ParsedStream to determine which parser to use for a message.
pub(crate) type ParserPredicate<'a> = Box<dyn Fn(&Channel, Option<&Schema>) -> bool + 'a>;

/// Parser function that converts message data to a typed value.
///
/// Takes raw message bytes and returns a parsed result.
pub(crate) type ParserFn<'a, T> = Box<dyn Fn(Bytes) -> Result<T> + 'a>;

/// A stream that borrows from a Reader to iterate over MCAP records.
///
/// The Stream type is generic over the output type `T` but hides the
/// underlying [`BytesSource`] type behind a trait object. This provides
/// a clean API and allows for flexibility in the data source.
///
/// Streams work with the lazy Reader - they will trigger loading of metadata
/// (schemas, channels) as needed during iteration.
///
/// `Stream` implements `Iterator` for sequential access. Some stream types
/// also provide a `.get(index)` method for random access using summary
/// indexes when available.
///
/// # Type Parameters
///
/// - `'a` - Lifetime of the borrow from the Reader
/// - `T` - Output type (Record, Chunk, RawMessage, or Message)
///
/// # Examples
///
/// ```no_run
/// use mcapable_core::reader;
/// use std::fs::File;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let file = File::open("data.mcap")?;
/// let mut reader = reader::Builder::new().build(file)?;
///
/// // Sequential access via Iterator
/// for message in reader.messages()? {
///     let message = message?;
///     // Schemas/channels loaded lazily as encountered
///     println!("Message at {}", message.log_time);
/// }
///
/// // Random access via .get()
/// let msg = reader.messages()?.get(100)?;
/// # Ok(())
/// # }
/// ```
pub struct Stream<'a, T> {
    /// Mutable reference to the Reader (via ReaderAccess trait).
    pub(super) reader: &'a mut dyn ReaderAccess,
    /// Whether iteration is complete.
    pub(super) done: bool,
    /// Optional time range filter.
    pub(super) time_range: Option<(Timestamp, Timestamp)>,
    /// Optional channel predicate filter.
    pub(super) channel_predicate: Option<ChannelPredicate<'a>>,
    pub(super) channel_predicate_cache: Vec<u8>,
    /// Optional chunk metadata filter (evaluated before decompression).
    pub(super) chunk_filter: Option<ChunkFilterPredicate<'a>>,
    /// Optional message header filter (evaluated before loading data).
    pub(super) message_filter: Option<MessageFilterPredicate<'a>>,
    /// Optional record type filter (evaluated before parsing).
    pub(super) record_filter: Option<RecordFilterPredicate<'a>>,
    /// Current chunk state for message iteration.
    pub(super) chunk_state: Option<ChunkState>,
    /// Whether to emit metadata for messages inside chunk payloads.
    pub(super) record_metadata_include_chunk_messages: bool,
    /// Whether to parse message headers for RecordMetadata entries.
    pub(super) record_metadata_include_message_metadata: bool,
    /// Whether to parse and cache channel records during RecordMetadata iteration.
    pub(super) record_metadata_include_channel_metadata: bool,
    /// State for iterating message records inside a chunk payload.
    pub(super) chunk_message_state: Option<ChunkMessageState>,
    /// Phantom data for output type.
    pub(super) _phantom: PhantomData<T>,
}

impl<'a, T> Stream<'a, T> {
    /// Apply a time range filter to the stream.
    ///
    /// Only records with timestamps in the range [start, end] will be returned.
    pub fn time_range(mut self, start: Timestamp, end: Timestamp) -> Self {
        self.time_range = Some((start, end));
        self
    }

    /// Apply a channel filter predicate to the stream.
    ///
    /// The predicate receives a reference to the Channel metadata.
    /// Only messages where the predicate returns true will be included.
    ///
    /// The Reader will lazy-load channel metadata as needed during iteration.
    ///
    /// # Examples
    ///
    /// Filter by topic prefix:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let stream = reader.messages()
    ///     .filter_channel(|ch| ch.topic.starts_with("/camera"));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Filter by schema ID:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let stream = reader.messages()
    ///     .filter_channel(|ch| ch.schema_id == 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn filter_channel<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Channel) -> bool + 'a,
    {
        self.channel_predicate = Some(Box::new(predicate));
        if self.channel_predicate_cache.len() == (u16::MAX as usize) + 1 {
            self.channel_predicate_cache.fill(2);
        } else {
            self.channel_predicate_cache = vec![2; (u16::MAX as usize) + 1];
        }
        self
    }
}
