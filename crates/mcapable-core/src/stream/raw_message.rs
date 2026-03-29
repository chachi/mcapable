//! RawMessage stream implementation.

use super::{ReaderAccess, Stream, next_message_common, passes_common_filters_cached};
use crate::error::Result;
use crate::format::MCAP_MAGIC_SIZE;
use crate::reader::Reader;
use crate::source::BytesSource;
use crate::types::{MessageHeader, RawMessage};
use std::marker::PhantomData;

impl<'a> Stream<'a, RawMessage> {
    /// Create a new raw message stream from a Reader.
    pub(crate) fn new_raw_message_stream<R: BytesSource>(reader: &'a mut Reader<R>) -> Self {
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
    /// The predicate receives message header before loading the full message data.
    /// This can avoid loading message payloads that won't be used.
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
    /// let stream = reader.raw_messages()
    ///     .filter(|hdr| hdr.sequence % 10 == 0);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Filter by data size:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let stream = reader.raw_messages()
    ///     .filter(|hdr| hdr.data_size < 1024);
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

    fn should_include(&mut self, msg: &RawMessage) -> bool {
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

impl<'a> Iterator for Stream<'a, RawMessage> {
    type Item = Result<RawMessage>;

    fn next(&mut self) -> Option<Self::Item> {
        next_message_common(self, |raw| raw, |s, msg| s.should_include(msg))
    }
}
