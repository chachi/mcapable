//! Core Reader that owns the data source and lazily loads file metadata.

use crate::compression::decompress;
use crate::error::Result;
use crate::format::{FOOTER_TOTAL_SIZE, MCAP_MAGIC_SIZE, RECORD_HEADER_SIZE};
use crate::records::ChunkRecordIterator;
use crate::source::{BytesCursor, BytesSource, PositionTrackingSource};
use crate::stream::Stream;
use crate::support::HashMap;
use crate::types::{
    Attachment, AttachmentEntry, AttachmentIndex, Channel, Chunk, ChunkIndex, Footer, Header,
    Message, MessageIndex, MessageMetadata, Metadata, MetadataEntry, MetadataIndex, Opcode,
    RawMessage, RawRecord, Record, RecordMetadata, Schema, Statistics, Summary,
};
use crate::zero_copy::ByteStr;
use bytes::Bytes;
use std::sync::Arc;

/// Main MCAP reader that owns the data source and lazily loads file metadata.
///
/// The Reader is responsible for:
/// - Owning the underlying [`BytesSource`] data source
/// - Lazily loading and caching file metadata (header, schemas, channels, summary)
/// - Creating `Stream` iterators that borrow from the reader
///
/// # Lazy Loading
///
/// The Reader does minimal work on construction:
/// - Validates MCAP magic bytes
/// - Stores the data source
///
/// Everything else is loaded on-demand:
/// - Header: Loaded on first access to `header()`
/// - Summary: Loaded when needed for seeking or explicitly requested
/// - Schemas/Channels: Loaded as encountered during iteration
///
/// This allows for efficient usage patterns and minimal I/O overhead.
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
/// // No I/O has happened yet except magic byte validation!
///
/// // Header loaded on first access
/// println!("Profile: {}", reader.header()?.profile);
///
/// // Create streams to iterate (schemas/channels loaded on-demand)
/// for message in reader.messages() {
///     let message = message?;
///     println!("Message at time {}", message.log_time);
/// }
/// # Ok(())
/// # }
/// ```
pub struct Reader<R: BytesSource> {
    /// The underlying seekable source.
    pub(crate) reader: PositionTrackingSource<R>,
    /// Cached file length, computed lazily.
    #[allow(dead_code)] // Reserved for future bounds checks without seeking per record.
    file_end: Option<u64>,
    /// Lazily loaded file header.
    header: Option<Header>,
    /// Lazily loaded footer.
    footer: Option<Footer>,
    /// Lazily loaded schemas (schema_id -> Schema).
    pub(crate) schemas: Arc<HashMap<u16, Schema>>,
    /// Lazily loaded channels (channel_id -> Channel).
    pub(crate) channels: Arc<HashMap<u16, Channel>>,
    /// Lazily loaded summary.
    summary: Option<Summary>,
    /// Whether summary has been loaded.
    summary_loaded: bool,
    /// Whether footer has been loaded.
    footer_loaded: bool,
    /// Lazily loaded metadata records (name -> Metadata).
    metadata: Arc<HashMap<ByteStr, Metadata>>,
    /// Lazily loaded attachments (name -> Attachment).
    attachments: Arc<HashMap<ByteStr, Attachment>>,
}

impl<R: BytesSource> Reader<R> {
    #[allow(dead_code)] // Reserved for future bounds checks without seeking per record.
    pub(crate) fn file_end(&mut self) -> Result<u64> {
        use std::io::SeekFrom;

        if let Some(end) = self.file_end {
            return Ok(end);
        }

        let cur = self.reader.stream_position()?;
        let end = self.reader.seek(SeekFrom::End(0))?;
        self.reader.seek(SeekFrom::Start(cur))?;
        self.file_end = Some(end);
        Ok(end)
    }

    /// Get the file header, loading it lazily if needed.
    ///
    /// The header is loaded on the first call and cached for subsequent calls.
    pub fn header(&mut self) -> Result<Header> {
        use crate::Error;
        use crate::parser::parse_header_record;
        use crate::records::decode_record_header;
        use std::io::SeekFrom;

        // Check if already loaded
        if let Some(ref header) = self.header {
            return Ok(header.clone());
        }

        // Seek to start (after magic bytes)
        self.reader.seek(SeekFrom::Start(MCAP_MAGIC_SIZE as u64))?;

        // Read record header (opcode + length)
        let header_buf = self.reader.read_exact_bytes(RECORD_HEADER_SIZE)?;
        let (opcode, length) = decode_record_header(header_buf.as_ref())?;

        if opcode != Opcode::Header {
            return Err(Error::InvalidRecord(format!(
                "Expected Header record (opcode {:#x}), got opcode {:#x}",
                Opcode::Header.as_u8(),
                opcode.as_u8()
            )));
        }

        // Read header data
        let len: usize = length
            .try_into()
            .map_err(|_| Error::InvalidRecord("Record length exceeds addressable memory".into()))?;
        let data = self.reader.read_exact_bytes(len)?;
        let header = parse_header_record(data)?;

        // Cache and return the header (avoiding double clone)
        self.header = Some(header);
        Ok(self.header.as_ref().unwrap().clone())
    }

    /// Convenience wrapper to fetch the header profile string.
    pub fn profile(&mut self) -> Result<ByteStr> {
        Ok(self.header()?.profile)
    }

    /// Read footer from the end of the file if present.
    fn read_footer_at_end(&mut self) -> Result<Option<Footer>> {
        use crate::parser::parse_footer_record;
        use crate::records::try_decode_record_header;
        use std::io::SeekFrom;

        let cur = self.reader.stream_position()?;

        let result = (|| {
            // Seek backwards from end to find Footer.
            // File ends with: footer_header + footer_data + magic = FOOTER_TOTAL_SIZE bytes.
            if let Err(e) = self.reader.seek(SeekFrom::End(-(FOOTER_TOTAL_SIZE as i64))) {
                if e.kind() == std::io::ErrorKind::InvalidInput {
                    return Ok(None);
                }
                return Err(e.into());
            }

            let buf = match self.reader.read_exact_bytes(RECORD_HEADER_SIZE) {
                Ok(buf) => buf,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => return Err(e.into()),
            };

            let Some((opcode, length)) = try_decode_record_header(buf.as_ref())? else {
                return Ok(None);
            };

            if opcode != Opcode::Footer {
                return Ok(None);
            }

            let len: usize = length.try_into().map_err(|_| {
                crate::Error::InvalidRecord("Record length exceeds addressable memory".into())
            })?;
            let data = self.reader.read_exact_bytes(len)?;
            let footer = parse_footer_record(data)?;
            Ok(Some(footer))
        })();

        if result.is_ok() {
            self.reader
                .seek(SeekFrom::Start(cur))
                .map_err(crate::Error::from)?;
        } else {
            let _ = self.reader.seek(SeekFrom::Start(cur));
        }

        result
    }

    /// Get the footer, loading it lazily if present.
    ///
    /// Returns Ok(None) when the footer opcode is missing (no summary info).
    pub fn footer(&mut self) -> Result<Option<Footer>> {
        if self.footer_loaded {
            return Ok(self.footer.clone());
        }

        self.footer_loaded = true;
        self.footer = self.read_footer_at_end()?;
        Ok(self.footer.clone()) // Still need one clone here to return owned value
    }

    /// Get the summary information, loading it lazily if needed.
    ///
    /// The summary is loaded on the first call and cached for subsequent calls.
    /// This involves seeking to the end of the file and reading the summary section.
    pub fn summary(&mut self) -> Result<Option<Summary>> {
        use crate::Error;
        use crate::parser::{
            parse_attachment_index_record, parse_channel_record, parse_chunk_index_record,
            parse_message_index_record, parse_metadata_index_record, parse_schema_record,
            parse_statistics_record,
        };
        use crate::records::try_decode_record_header;
        use std::io::SeekFrom;

        // Check if already attempted to load
        if self.summary_loaded {
            return Ok(self.summary.clone());
        }

        let cur = self.reader.stream_position()?;

        // Mark as loaded (even if we fail, don't retry)
        self.summary_loaded = true;

        let footer = match self.footer()? {
            Some(footer) => footer,
            None => {
                self.reader.seek(SeekFrom::Start(cur))?;
                return Ok(None);
            }
        };

        if footer.summary_start == 0 {
            // No summary section
            self.reader.seek(SeekFrom::Start(cur))?;
            return Ok(None);
        }

        let end = self.reader.seek(SeekFrom::End(0))?;
        if footer.summary_start >= end {
            let err = Error::InvalidSummary(format!(
                "summary_start {} is beyond end of file {end}",
                footer.summary_start
            ));
            self.reader.seek(SeekFrom::Start(cur))?;
            return Err(err);
        }

        // Seek to summary section and parse all records
        if let Err(e) = self.reader.seek(SeekFrom::Start(footer.summary_start)) {
            self.reader.seek(SeekFrom::Start(cur))?;
            return Err(Error::InvalidSummary(format!(
                "failed to seek to summary_start {}: {e}",
                footer.summary_start
            )));
        }

        let mut statistics: Option<Statistics> = None;
        let mut schemas = HashMap::new();
        let mut channels = HashMap::new();
        let mut chunk_indexes = Vec::new();
        let mut message_indexes = Vec::new();
        let mut attachment_indexes = Vec::new();
        let mut metadata_indexes = Vec::new();

        // Read all records in summary section until we hit SummaryOffset or end
        loop {
            let header_buf = match self.reader.read_exact_bytes(RECORD_HEADER_SIZE) {
                Ok(buf) => buf,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            };

            let Some((opcode, length)) = (match try_decode_record_header(header_buf.as_ref()) {
                Ok(v) => v,
                Err(_) => break,
            }) else {
                break;
            };

            // SummaryOffset marks end of summary
            if opcode == Opcode::SummaryOffset {
                break;
            }

            let len: usize = length.try_into().map_err(|_| {
                crate::Error::InvalidRecord("Record length exceeds addressable memory".into())
            })?;
            let data = self.reader.read_exact_bytes(len)?;

            match opcode {
                Opcode::Schema => {
                    // Schema
                    if let Ok(schema) = parse_schema_record(data.clone()) {
                        schemas.insert(schema.id, schema);
                    }
                }
                Opcode::Channel => {
                    // Channel
                    if let Ok(channel) = parse_channel_record(data.clone()) {
                        channels.insert(channel.id, channel);
                    }
                }
                Opcode::ChunkIndex => {
                    // ChunkIndex
                    if let Ok(chunk_index) = parse_chunk_index_record(data.clone()) {
                        chunk_indexes.push(chunk_index);
                    }
                }
                Opcode::MessageIndex => {
                    // MessageIndex
                    if let Ok(message_index) = parse_message_index_record(data.clone()) {
                        message_indexes.push(message_index);
                    }
                }
                Opcode::Statistics => {
                    // Statistics
                    if let Ok(stats) = parse_statistics_record(data.clone()) {
                        statistics = Some(stats);
                    }
                }
                Opcode::AttachmentIndex => {
                    if let Ok(idx) = parse_attachment_index_record(data.clone()) {
                        attachment_indexes.push(idx);
                    }
                }
                Opcode::MetadataIndex => {
                    if let Ok(idx) = parse_metadata_index_record(data.clone()) {
                        metadata_indexes.push(idx);
                    }
                }
                // Ignore other record types in summary
                _ => {}
            }
        }

        chunk_indexes.sort_by_key(|c| c.chunk_start_offset);

        // Cache and return the summary (avoiding double clone)
        self.summary = Some(Summary {
            statistics: statistics.map(Arc::new),
            schemas: Arc::new(schemas),
            channels: Arc::new(channels),
            chunk_indexes: Arc::from(chunk_indexes),
            message_indexes: Arc::from(message_indexes),
            attachment_indexes: Arc::from(attachment_indexes),
            metadata_indexes: Arc::from(metadata_indexes),
        });
        self.reader.seek(SeekFrom::Start(cur))?;
        Ok(self.summary.clone())
    }

    /// Get message indexes from the summary.
    ///
    /// Returns an empty vector when no summary is available.
    pub fn message_indexes(&mut self) -> Result<Arc<[MessageIndex]>> {
        Ok(self
            .summary()?
            .map(|summary| summary.message_indexes.clone())
            .unwrap_or_else(|| Arc::from([])))
    }

    /// Get chunk indexes from the summary.
    ///
    /// Returns an empty vector when no summary is available.
    pub fn chunk_indexes(&mut self) -> Result<Arc<[ChunkIndex]>> {
        Ok(self
            .summary()?
            .map(|summary| summary.chunk_indexes.clone())
            .unwrap_or_else(|| Arc::from([])))
    }

    /// Get a schema by ID.
    ///
    /// Schemas are automatically loaded when creating message streams.
    /// This method returns schemas that are currently cached.
    pub fn schema(&self, id: u16) -> Option<Schema> {
        self.schemas.get(&id).cloned()
    }

    /// Get a channel by ID.
    ///
    /// Channels are automatically loaded when creating message streams.
    /// This method returns channels that are currently cached.
    pub fn channel(&self, id: u16) -> Option<Channel> {
        self.channels.get(&id).cloned()
    }

    /// Get all currently loaded schemas.
    ///
    /// Schemas are automatically cached when message streams are created.
    /// Cache this result before iterating to access metadata during iteration.
    pub fn schemas(&self) -> Arc<HashMap<u16, Schema>> {
        self.schemas.clone()
    }

    /// Get all currently loaded channels.
    ///
    /// Channels are automatically cached when message streams are created.
    /// Cache this result before iterating to access metadata during iteration.
    pub fn channels(&self) -> Arc<HashMap<u16, Channel>> {
        self.channels.clone()
    }

    /// Get file-level statistics from the summary.
    ///
    /// Returns `None` if summary hasn't been loaded or doesn't contain a Statistics record.
    pub fn statistics(&self) -> Option<Arc<Statistics>> {
        self.summary.as_ref().and_then(|s| s.statistics.clone())
    }

    /// Get attachment indexes from the summary.
    ///
    /// Returns an empty slice when no summary is available.
    pub fn attachment_indexes(&mut self) -> Result<Arc<[AttachmentIndex]>> {
        Ok(self
            .summary()?
            .map(|summary| summary.attachment_indexes.clone())
            .unwrap_or_else(|| Arc::from([])))
    }

    /// Get metadata indexes from the summary.
    ///
    /// Returns an empty slice when no summary is available.
    pub fn metadata_indexes(&mut self) -> Result<Arc<[MetadataIndex]>> {
        Ok(self
            .summary()?
            .map(|summary| summary.metadata_indexes.clone())
            .unwrap_or_else(|| Arc::from([])))
    }

    /// Get a metadata record by name.
    ///
    /// Metadata records are loaded on-demand during iteration. This method only
    /// returns metadata that has already been encountered.
    ///
    /// To ensure all metadata is loaded, iterate through records first.
    pub fn metadata(&self, name: &str) -> Option<Metadata> {
        let key = ByteStr::from(name);
        self.metadata.get(&key).cloned()
    }

    /// Get all currently loaded metadata records.
    ///
    /// Note: This only returns metadata that has been encountered so far.
    pub fn all_metadata(&self) -> Arc<HashMap<ByteStr, Metadata>> {
        self.metadata.clone()
    }

    /// Get an attachment by name.
    ///
    /// Attachments are loaded on-demand during iteration. This method only
    /// returns attachments that have already been encountered.
    ///
    /// To ensure all attachments are loaded, iterate through records first.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// // After iterating through records...
    /// if let Some(attachment) = reader.attachment("calibration.json") {
    ///     println!("Found attachment: {} bytes", attachment.data.len());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn attachment(&self, name: &str) -> Option<Attachment> {
        let key = ByteStr::from(name);
        self.attachments.get(&key).cloned()
    }

    /// Get all currently loaded attachments.
    ///
    /// Note: This only returns attachments that have been encountered so far.
    pub fn all_attachments(&self) -> Arc<HashMap<ByteStr, Attachment>> {
        self.attachments.clone()
    }

    /// Load and return all metadata records with offsets and lengths.
    ///
    /// If summary metadata indexes are available, they are used for faster lookups.
    pub fn metadata_entries(&mut self) -> Result<Vec<MetadataEntry>> {
        if let Some(summary) = self.summary()?
            && !summary.metadata_indexes.is_empty()
        {
            return self.metadata_entries_from_indexes(&summary.metadata_indexes);
        }
        self.metadata_entries_from_records()
    }

    /// Load and return all attachment entries (without reading attachment data).
    ///
    /// If attachment indexes are available, they are used for faster lookups.
    pub fn attachment_entries(&mut self) -> Result<Vec<AttachmentEntry>> {
        if let Some(summary) = self.summary()? {
            if let Some(stats) = summary.statistics.as_ref()
                && stats.attachment_count == 0
            {
                return Ok(Vec::new());
            }
            if !summary.attachment_indexes.is_empty() {
                return Ok(summary
                    .attachment_indexes
                    .iter()
                    .map(|idx| AttachmentEntry {
                        name: idx.name.clone(),
                        media_type: idx.media_type.clone(),
                        log_time: idx.log_time,
                        create_time: idx.create_time,
                        data_size: idx.data_size,
                        offset: idx.offset,
                    })
                    .collect());
            }
        }
        self.attachment_entries_from_records()
    }

    /// Load schema records from the data section, including schemas embedded in chunks.
    ///
    /// This scans data section records in order, stopping at `DataEnd`. Chunk records are
    /// decompressed (if needed) and scanned for schema records in chunk order.
    pub fn data_section_schemas(&mut self) -> Result<Vec<Schema>> {
        let mut out = Vec::new();
        for record in self.records() {
            let record = record?;
            match record {
                Record::Schema(schema) => out.push(schema),
                Record::Chunk(chunk) => {
                    let compression =
                        crate::compression::parse_compression(chunk.compression.as_ref())?;
                    let decompressed =
                        decompress(compression.as_ref(), chunk.records, chunk.uncompressed_size)?;
                    for record in ChunkRecordIterator::new(decompressed.as_ref()) {
                        let (opcode, payload) = record.map_err(|_| {
                            crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Chunk))
                        })?;
                        if opcode == Opcode::Schema {
                            let schema = crate::parser::parse_schema_record(
                                Bytes::copy_from_slice(payload),
                            )?;
                            out.push(schema);
                        }
                    }
                }
                Record::DataEnd => break,
                _ => {}
            }
        }
        Ok(out)
    }

    /// Load channel records from the data section.
    ///
    /// This scans data section records in order, stopping at `DataEnd`.
    pub fn data_section_channels(&mut self) -> Result<Vec<Channel>> {
        let mut out = Vec::new();
        for record in self.records() {
            let record = record?;
            match record {
                Record::Channel(channel) => out.push(channel),
                Record::DataEnd => break,
                _ => {}
            }
        }
        Ok(out)
    }

    /// Get the underlying reader back (consuming self).
    pub fn into_inner(self) -> R {
        self.reader.into_inner()
    }

    /// Cache a schema (used by streams via ReaderAccess trait).
    #[allow(dead_code)] // Called via trait object in stream.rs
    pub(crate) fn cache_schema(&mut self, schema: Schema) {
        fn compact_schema(mut schema: Schema) -> Schema {
            schema.name = schema.name.to_compact();
            schema.encoding = schema.encoding.to_compact();
            schema.data = bytes::Bytes::copy_from_slice(schema.data.as_ref());
            schema
        }
        Arc::make_mut(&mut self.schemas).insert(schema.id, compact_schema(schema));
    }

    /// Cache a channel (used by streams via ReaderAccess trait).
    #[allow(dead_code)] // Called via trait object in stream.rs
    pub(crate) fn cache_channel(&mut self, channel: Channel) {
        fn compact_channel(mut channel: Channel) -> Channel {
            channel.topic = channel.topic.to_compact();
            channel.message_encoding = channel.message_encoding.to_compact();
            channel.metadata = channel
                .metadata
                .into_iter()
                .map(|(k, v)| (k.to_compact(), v.to_compact()))
                .collect();
            channel
        }
        Arc::make_mut(&mut self.channels).insert(channel.id, compact_channel(channel));
    }

    /// Cache metadata (used by streams via ReaderAccess trait).
    #[allow(dead_code)] // Called via trait object in stream.rs
    pub(crate) fn cache_metadata(&mut self, metadata: Metadata) {
        fn compact_metadata(mut metadata: Metadata) -> Metadata {
            metadata.name = metadata.name.to_compact();
            metadata.metadata = metadata
                .metadata
                .into_iter()
                .map(|(k, v)| (k.to_compact(), v.to_compact()))
                .collect();
            metadata
        }
        let metadata = compact_metadata(metadata);
        Arc::make_mut(&mut self.metadata).insert(metadata.name.clone(), metadata);
    }

    /// Cache attachment (used by streams via ReaderAccess trait).
    #[allow(dead_code)] // Called via trait object in stream.rs
    pub(crate) fn cache_attachment(&mut self, attachment: Attachment) {
        fn compact_attachment(mut attachment: Attachment) -> Attachment {
            attachment.name = attachment.name.to_compact();
            attachment.media_type = attachment.media_type.to_compact();
            // Intentionally do not copy `attachment.data` (can be large).
            attachment
        }
        let attachment = compact_attachment(attachment);
        Arc::make_mut(&mut self.attachments).insert(attachment.name.clone(), attachment);
    }

    /// Load all metadata (schemas and channels) if not already loaded.
    ///
    /// This scans the file for all schema and channel records and caches them.
    /// Called automatically by stream creation methods.
    fn load_metadata_if_needed(&mut self) -> Result<()> {
        use crate::parser::{parse_channel_record, parse_schema_record};
        use crate::records::try_decode_record_header;
        use std::io::SeekFrom;

        // If we already have schemas or channels, assume we've loaded
        if !self.schemas.is_empty() || !self.channels.is_empty() {
            return Ok(());
        }

        // Try loading from summary first (faster - O(1) seek to known location)
        if let Some(summary) = self.summary()? {
            self.schemas = summary.schemas.clone();
            self.channels = summary.channels.clone();
            return Ok(());
        }

        // No summary - scan data section for schemas and channels (slower - O(n) scan)
        // Start after header (at position MCAP_MAGIC_SIZE, since we're already past magic bytes)
        self.reader.seek(SeekFrom::Start(MCAP_MAGIC_SIZE as u64))?;

        loop {
            let header_buf = match self.reader.read_exact_bytes(RECORD_HEADER_SIZE) {
                Ok(buf) => buf,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            };

            let Some((opcode, length)) = (match try_decode_record_header(header_buf.as_ref()) {
                Ok(v) => v,
                Err(_) => break,
            }) else {
                break;
            };

            match opcode {
                Opcode::Schema => {
                    // Schema
                    let len: usize = length.try_into().map_err(|_| {
                        crate::Error::InvalidRecord(
                            "Record length exceeds addressable memory".into(),
                        )
                    })?;
                    let data = self.reader.read_exact_bytes(len)?;
                    if let Ok(schema) = parse_schema_record(data) {
                        Arc::make_mut(&mut self.schemas).insert(schema.id, schema);
                    }
                }
                Opcode::Channel => {
                    // Channel
                    let len: usize = length.try_into().map_err(|_| {
                        crate::Error::InvalidRecord(
                            "Record length exceeds addressable memory".into(),
                        )
                    })?;
                    let data = self.reader.read_exact_bytes(len)?;
                    if let Ok(channel) = parse_channel_record(data) {
                        Arc::make_mut(&mut self.channels).insert(channel.id, channel);
                    }
                }
                Opcode::DataEnd => {
                    // DataEnd - stop scanning
                    break;
                }
                Opcode::Footer => {
                    // Footer - we've hit the end
                    break;
                }
                _ => {
                    // Skip other record types
                    self.reader.seek(SeekFrom::Current(length as i64))?;
                }
            }
        }

        Ok(())
    }

    /// Get a stream to iterate over all records.
    ///
    /// This is the lowest-level stream type that returns all record types.
    ///
    /// Requires `&mut self` to ensure exclusive access during iteration.
    pub fn records(&mut self) -> Stream<'_, Record> {
        // No need to preload metadata for raw record iteration
        Stream::new_record_stream(self)
    }

    /// Get a stream to iterate over raw record payloads.
    ///
    /// This avoids parsing record contents, letting callers opt into parsing.
    pub fn raw_records(&mut self) -> Stream<'_, RawRecord> {
        Stream::new_raw_record_stream(self)
    }

    /// Get a stream to iterate over record headers without parsing payloads.
    pub fn record_metadata(&mut self) -> Stream<'_, RecordMetadata> {
        Stream::new_record_metadata_stream(self)
    }

    /// Get a stream to iterate over chunks.
    ///
    /// Chunks are returned with compressed data. Use `.get(n)` for random
    /// access by index when summary data is available.
    ///
    /// Requires `&mut self` to ensure exclusive access during iteration.
    pub fn chunks(&mut self) -> Stream<'_, Chunk> {
        // No need to preload metadata for chunk iteration
        Stream::new_chunk_stream(self)
    }

    /// Get a stream to iterate over raw messages.
    ///
    /// Raw messages are decompressed from chunks but do not include
    /// schema or channel metadata lookups.
    ///
    /// Requires `&mut self` to ensure exclusive access during iteration.
    pub fn raw_messages(&mut self) -> Result<Stream<'_, RawMessage>> {
        // Preload metadata so it's available for filters and iteration
        self.load_metadata_if_needed()?;
        Ok(Stream::new_raw_message_stream(self))
    }

    /// Get a stream to iterate over messages.
    ///
    /// Messages include full schema and channel metadata.
    /// Metadata (schemas and channels) is automatically preloaded and cached
    /// before iteration begins, allowing immutable access during streaming.
    /// Use `.get(n)` for random access by index when summary data is available.
    ///
    /// Requires `&mut self` to ensure exclusive access during iteration.
    pub fn messages(&mut self) -> Result<Stream<'_, Message>> {
        // Preload metadata so it's available during iteration
        self.load_metadata_if_needed()?;
        Ok(Stream::new_message_stream(self))
    }

    /// Get a stream to iterate over message metadata without reading payload data.
    ///
    /// This is faster than `raw_messages()` or `messages()` when you only need
    /// message metadata (channel_id, timestamps, data_size) and don't need the
    /// actual message payload bytes. Perfect for calculating message statistics.
    ///
    /// Metadata (schemas and channels) is automatically preloaded and cached
    /// before iteration begins.
    ///
    /// Requires `&mut self` to ensure exclusive access during iteration.
    ///
    /// # Examples
    ///
    /// Calculate total message size per channel:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # use mcapable_core::collections::HashMap;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let mut sizes: HashMap<u16, u64> = HashMap::new();
    /// for metadata in reader.message_metadata()? {
    ///     let m = metadata?;
    ///     *sizes.entry(m.channel_id).or_insert(0) += m.data_size;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn message_metadata(&mut self) -> Result<Stream<'_, MessageMetadata>> {
        // Preload metadata so it's available for filters and iteration
        self.load_metadata_if_needed()?;
        Ok(Stream::new_message_metadata_stream(self))
    }

    fn metadata_entries_from_indexes(
        &mut self,
        indexes: &[MetadataIndex],
    ) -> Result<Vec<MetadataEntry>> {
        let mut out = Vec::with_capacity(indexes.len());
        for idx in indexes {
            let (opcode, payload) = self.read_record_at_offset(idx.offset)?;
            if opcode != Opcode::Metadata {
                return Err(crate::Error::InvalidRecord(format!(
                    "expected metadata record at offset {}, got {opcode:?}",
                    idx.offset
                )));
            }
            let metadata = crate::parser::parse_metadata_record(payload)?;
            out.push(MetadataEntry {
                metadata,
                offset: idx.offset,
                length: idx.length,
            });
        }
        Ok(out)
    }

    fn metadata_entries_from_records(&mut self) -> Result<Vec<MetadataEntry>> {
        let mut out = Vec::new();
        for record in self.raw_records().filter(|op| op == Opcode::Metadata) {
            let record = record?;
            let metadata = crate::parser::parse_metadata_record(record.payload())?;
            out.push(MetadataEntry {
                metadata,
                offset: record.offset,
                length: record.total_len,
            });
        }
        Ok(out)
    }

    fn attachment_entries_from_records(&mut self) -> Result<Vec<AttachmentEntry>> {
        let mut out = Vec::new();
        for record in self.raw_records().filter(|op| op == Opcode::Attachment) {
            let record = record?;
            let attachment = crate::parser::parse_attachment_record(record.payload())?;
            out.push(AttachmentEntry {
                name: attachment.name,
                media_type: attachment.media_type,
                log_time: attachment.log_time,
                create_time: attachment.create_time,
                data_size: attachment.data.len() as u64,
                offset: record.offset,
            });
        }
        Ok(out)
    }

    fn read_record_at_offset(&mut self, offset: u64) -> Result<(Opcode, bytes::Bytes)> {
        use crate::records::decode_record_header;
        use std::io::SeekFrom;

        let cur = self.reader.stream_position()?;
        let result = (|| {
            self.reader.seek(SeekFrom::Start(offset))?;
            let header = self.reader.read_exact_bytes(RECORD_HEADER_SIZE)?;
            let (opcode, length) = decode_record_header(header.as_ref())?;
            let len: usize = length.try_into().map_err(|_| {
                crate::Error::InvalidRecord("Record length exceeds addressable memory".into())
            })?;
            let payload = self.reader.read_exact_bytes(len)?;
            Ok((opcode, payload))
        })();

        if result.is_ok() {
            self.reader.seek(SeekFrom::Start(cur))?;
        } else {
            let _ = self.reader.seek(SeekFrom::Start(cur));
        }

        result
    }
}

/// Builder for configuring and constructing MCAP readers.
///
/// The builder only configures behavior; actual I/O is deferred until needed.
///
/// # Examples
///
/// ```no_run
/// use mcapable_core::reader;
/// use std::fs::File;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let file = File::open("data.mcap")?;
/// let mut reader = reader::Builder::new()
///     .validate_end_magic(true)
///     .build(file)?;
///
/// // No I/O has happened yet except magic byte validation!
///
/// // Header loaded lazily on first access
/// println!("Profile: {}", reader.header()?.profile);
///
/// // Schemas/channels loaded lazily during iteration
/// for message in reader.messages() {
///     let message = message?;
///     println!("Got message");
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Builder {
    /// Whether to validate magic bytes at end of file.
    validate_end_magic: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    /// Create a new builder with default settings.
    ///
    /// Defaults:
    /// - Validate end magic: true
    ///
    /// Everything is loaded lazily:
    /// - Header: Loaded on first access
    /// - Summary: Loaded when needed for seeking
    /// - Schemas/Channels: Loaded as encountered during iteration
    pub fn new() -> Self {
        Self {
            validate_end_magic: true,
        }
    }

    /// Configure whether to validate magic bytes at the end of the file.
    ///
    /// When enabled (default), the reader will check that the file ends with
    /// valid MCAP magic bytes during construction, providing more thorough
    /// validation of file integrity.
    ///
    /// Default: true
    pub fn validate_end_magic(mut self, validate: bool) -> Self {
        self.validate_end_magic = validate;
        self
    }

    /// Build a reader from a data source.
    ///
    /// This performs minimal work:
    /// - Validates MCAP magic bytes
    /// - Stores the data source
    ///
    /// All other data (header, summary, schemas, channels) is loaded lazily.
    pub fn build<R: BytesSource>(self, mut reader: R) -> Result<Reader<R>> {
        use crate::Error;
        use crate::format::MCAP_MAGIC;
        use std::io::SeekFrom;

        // Validate magic bytes at start
        let magic = reader
            .read_exact_bytes(MCAP_MAGIC_SIZE)
            .map_err(|_| Error::InvalidMagic)?;
        if magic.as_ref() != MCAP_MAGIC {
            return Err(Error::InvalidMagic);
        }

        // Optionally validate magic bytes at end (more thorough validation)
        if self.validate_end_magic {
            reader
                .seek(SeekFrom::End(-(MCAP_MAGIC_SIZE as i64)))
                .map_err(|_| Error::InvalidMagic)?;
            let magic = reader
                .read_exact_bytes(MCAP_MAGIC_SIZE)
                .map_err(|_| Error::InvalidMagic)?;
            if magic.as_ref() != MCAP_MAGIC {
                return Err(Error::InvalidMagic);
            }
        }

        // Reset to position after start magic bytes (ready to read header)
        reader.seek(SeekFrom::Start(MCAP_MAGIC_SIZE as u64))?;

        Ok(Reader {
            reader: PositionTrackingSource::new(reader, MCAP_MAGIC_SIZE as u64),
            file_end: None,
            header: None,
            footer: None,
            schemas: Arc::new(HashMap::new()),
            channels: Arc::new(HashMap::new()),
            summary: None,
            summary_loaded: false,
            footer_loaded: false,
            metadata: Arc::new(HashMap::new()),
            attachments: Arc::new(HashMap::new()),
        })
    }

    /// Build a reader from an in-memory `Bytes` buffer.
    ///
    /// This enables fully zero-copy reads (record bodies are returned as slices
    /// of the original buffer) while still supporting seeking.
    pub fn build_bytes(self, bytes: bytes::Bytes) -> Result<Reader<BytesCursor>> {
        self.build(BytesCursor::new(bytes))
    }

    /// Build a reader from an in-memory byte slice.
    ///
    /// This copies the slice into `Bytes` once, then uses a `BytesCursor` so
    /// record bodies can be sliced without further copies.
    pub fn build_slice(self, bytes: &[u8]) -> Result<Reader<BytesCursor>> {
        self.build_bytes(bytes::Bytes::copy_from_slice(bytes))
    }
}

impl Reader<BytesCursor> {
    /// Construct a zero-copy reader over an in-memory `Bytes` buffer.
    pub fn from_bytes(bytes: bytes::Bytes) -> Result<Self> {
        Builder::new().build_bytes(bytes)
    }

    /// Construct a zero-copy reader over an in-memory byte slice.
    ///
    /// This copies the slice into a shared `Bytes` buffer once.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        Builder::new().build_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::MCAP_MAGIC;
    use bytes::Bytes;

    fn record(opcode: Opcode, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(crate::format::RECORD_HEADER_SIZE + payload.len());
        out.push(opcode.as_u8());
        out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn summary_parses_statistics_and_indexes() {
        let summary_start = crate::format::MCAP_MAGIC_SIZE as u64;

        let mut stats_payload = Vec::new();
        stats_payload.extend_from_slice(&1202840u64.to_le_bytes()); // message_count
        stats_payload.extend_from_slice(&12u16.to_le_bytes()); // schema_count
        stats_payload.extend_from_slice(&32u32.to_le_bytes()); // channel_count
        stats_payload.extend_from_slice(&2u32.to_le_bytes()); // attachment_count
        stats_payload.extend_from_slice(&1u32.to_le_bytes()); // metadata_count
        stats_payload.extend_from_slice(&14490u32.to_le_bytes()); // chunk_count
        stats_payload.extend_from_slice(&1669703463001080535u64.to_le_bytes()); // start
        stats_payload.extend_from_slice(&1669704213999570541u64.to_le_bytes()); // end

        let mut counts = Vec::new();
        counts.extend_from_slice(&1u16.to_le_bytes());
        counts.extend_from_slice(&156658u64.to_le_bytes());
        counts.extend_from_slice(&2u16.to_le_bytes());
        counts.extend_from_slice(&37551u64.to_le_bytes());
        stats_payload.extend_from_slice(&(counts.len() as u32).to_le_bytes());
        stats_payload.extend_from_slice(&counts);

        let mut attachment_index_payload = Vec::new();
        attachment_index_payload.extend_from_slice(&999u64.to_le_bytes()); // offset
        attachment_index_payload.extend_from_slice(&111u64.to_le_bytes()); // length
        attachment_index_payload.extend_from_slice(&1000u64.to_le_bytes()); // log_time
        attachment_index_payload.extend_from_slice(&2000u64.to_le_bytes()); // create_time
        attachment_index_payload.extend_from_slice(&777u64.to_le_bytes()); // data_size
        attachment_index_payload.extend_from_slice(&4u32.to_le_bytes()); // name
        attachment_index_payload.extend_from_slice(b"file");
        attachment_index_payload.extend_from_slice(&9u32.to_le_bytes()); // media_type
        attachment_index_payload.extend_from_slice(b"image/png");

        let mut metadata_index_payload = Vec::new();
        metadata_index_payload.extend_from_slice(&555u64.to_le_bytes()); // offset
        metadata_index_payload.extend_from_slice(&222u64.to_le_bytes()); // length
        metadata_index_payload.extend_from_slice(&4u32.to_le_bytes()); // name
        metadata_index_payload.extend_from_slice(b"meta");

        let mut file = Vec::new();
        file.extend_from_slice(&MCAP_MAGIC);
        file.extend_from_slice(&record(Opcode::Statistics, &stats_payload));
        file.extend_from_slice(&record(Opcode::AttachmentIndex, &attachment_index_payload));
        file.extend_from_slice(&record(Opcode::MetadataIndex, &metadata_index_payload));

        // SummaryOffset header ends summary parsing; reader doesn't read its body.
        file.push(Opcode::SummaryOffset.as_u8());
        file.extend_from_slice(&0u64.to_le_bytes());

        let summary_offset_start = file.len() as u64;
        let summary_crc = 0u32;
        let mut footer_payload = Vec::new();
        footer_payload.extend_from_slice(&summary_start.to_le_bytes());
        footer_payload.extend_from_slice(&summary_offset_start.to_le_bytes());
        footer_payload.extend_from_slice(&summary_crc.to_le_bytes());
        file.extend_from_slice(&record(Opcode::Footer, &footer_payload));
        file.extend_from_slice(&MCAP_MAGIC);

        let mut reader = Reader::from_bytes(Bytes::from(file)).unwrap();
        let summary = reader.summary().unwrap().unwrap();

        let stats = summary.statistics.as_deref().unwrap();
        assert_eq!(stats.message_count, 1202840);
        assert_eq!(stats.schema_count, 12);
        assert_eq!(stats.channel_count, 32);
        assert_eq!(stats.attachment_count, 2);
        assert_eq!(stats.metadata_count, 1);
        assert_eq!(stats.chunk_count, 14490);
        assert_eq!(stats.message_start_time, 1669703463001080535);
        assert_eq!(stats.message_end_time, 1669704213999570541);
        assert_eq!(stats.channel_message_counts.len(), 2);
        assert_eq!(stats.channel_message_counts[0].channel_id, 1);
        assert_eq!(stats.channel_message_counts[0].message_count, 156658);
        assert_eq!(stats.channel_message_counts[1].channel_id, 2);
        assert_eq!(stats.channel_message_counts[1].message_count, 37551);

        assert_eq!(summary.attachment_indexes.len(), 1);
        let aidx = &summary.attachment_indexes[0];
        assert_eq!(aidx.offset, 999);
        assert_eq!(aidx.length, 111);
        assert_eq!(aidx.log_time, 1000);
        assert_eq!(aidx.create_time, 2000);
        assert_eq!(aidx.data_size, 777);
        assert_eq!(aidx.name.as_str(), "file");
        assert_eq!(aidx.media_type.as_str(), "image/png");

        assert_eq!(summary.metadata_indexes.len(), 1);
        let midx = &summary.metadata_indexes[0];
        assert_eq!(midx.offset, 555);
        assert_eq!(midx.length, 222);
        assert_eq!(midx.name.as_str(), "meta");
    }
}
