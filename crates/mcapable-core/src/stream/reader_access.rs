use crate::error::Result;
use crate::reader::Reader;
use crate::source::BytesSource;
use crate::types::{Attachment, Channel, ChunkIndex, Metadata, Schema};
use std::sync::Arc;

/// Helper to access Reader data that Stream needs.
///
/// This trait allows Stream to access the necessary Reader fields without
/// exposing the generic R parameter.
#[allow(dead_code)] // Will be used during implementation
pub(crate) trait ReaderAccess {
    /// Get the underlying seekable source.
    fn source(&mut self) -> &mut dyn BytesSource;

    /// Get the cached file end offset, computing it lazily.
    fn file_end(&mut self) -> Result<u64>;

    /// Get a schema by ID (returns cloned).
    fn get_schema(&self, id: u16) -> Option<Schema>;

    /// Get a channel by ID (returns cloned).
    fn get_channel(&self, id: u16) -> Option<Channel>;

    /// Get a channel by ID (borrowed).
    fn get_channel_ref(&self, id: u16) -> Option<&Channel>;

    /// Cache a schema encountered during iteration.
    fn cache_schema(&mut self, schema: Schema);

    /// Cache a channel encountered during iteration.
    fn cache_channel(&mut self, channel: Channel);

    /// Cache metadata encountered during iteration.
    fn cache_metadata(&mut self, metadata: Metadata);

    /// Cache an attachment encountered during iteration.
    fn cache_attachment(&mut self, attachment: Attachment);

    /// Get a chunk index by chunk start offset.
    fn get_chunk_index(&mut self, offset: u64) -> Option<ChunkIndex>;

    /// Load schemas/channels from summary if caches are empty.
    fn ensure_summary_metadata(&mut self) -> Result<()>;

    /// Get all chunk indexes from summary (empty when none).
    fn chunk_indexes(&mut self) -> Result<Arc<[ChunkIndex]>>;

    /// Check whether a channel ID passes the current predicate.
    fn channel_predicate_allows(
        &self,
        channel_id: u16,
        predicate: &super::ChannelPredicate<'_>,
    ) -> bool {
        self.get_channel_ref(channel_id)
            .map(predicate)
            .unwrap_or(false)
    }
}

// Implement ReaderAccess for any Reader<R>
impl<R: BytesSource> ReaderAccess for Reader<R> {
    fn source(&mut self) -> &mut dyn BytesSource {
        &mut self.reader as &mut dyn BytesSource
    }

    fn file_end(&mut self) -> Result<u64> {
        Reader::file_end(self)
    }

    fn get_schema(&self, id: u16) -> Option<Schema> {
        self.schema(id)
    }

    fn get_channel(&self, id: u16) -> Option<Channel> {
        self.channel(id)
    }

    fn get_channel_ref(&self, id: u16) -> Option<&Channel> {
        self.channels.get(&id)
    }

    fn cache_schema(&mut self, schema: Schema) {
        Reader::cache_schema(self, schema);
    }

    fn cache_channel(&mut self, channel: Channel) {
        Reader::cache_channel(self, channel);
    }

    fn cache_metadata(&mut self, metadata: Metadata) {
        Reader::cache_metadata(self, metadata);
    }

    fn cache_attachment(&mut self, attachment: Attachment) {
        Reader::cache_attachment(self, attachment);
    }

    fn get_chunk_index(&mut self, offset: u64) -> Option<ChunkIndex> {
        self.summary().ok().flatten().and_then(|summary| {
            summary
                .chunk_indexes
                .iter()
                .find(|c| c.chunk_start_offset == offset)
                .cloned()
        })
    }

    fn ensure_summary_metadata(&mut self) -> Result<()> {
        if self.schemas.is_empty()
            && self.channels.is_empty()
            && let Some(summary) = self.summary()?
        {
            self.schemas = summary.schemas.clone();
            self.channels = summary.channels.clone();
        }
        Ok(())
    }

    fn chunk_indexes(&mut self) -> Result<Arc<[ChunkIndex]>> {
        Reader::chunk_indexes(self)
    }
}
