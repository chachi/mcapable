//! MCAP record types and data structures.

use crate::support::{Arc, HashMap, Vec};
use crate::zero_copy::ByteStr;
use bytes::Bytes;

/// Shared string type used throughout the public API.
pub type ArcStr = arcstr::ArcStr;

/// Timestamp in nanoseconds since epoch.
pub type Timestamp = u64;

/// MCAP record opcodes.
///
/// See <https://mcap.dev/spec/registry#well-known-record-types>
#[repr(u8)]
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    strum::Display,
    strum::EnumIter,
    strum::EnumString,
    strum::FromRepr,
)]
pub enum Opcode {
    /// Header record (0x01)
    #[strum(serialize = "header")]
    Header = 0x01,
    /// Footer record (0x02)
    #[strum(serialize = "footer")]
    Footer = 0x02,
    /// Schema record (0x03)
    #[strum(serialize = "schema")]
    Schema = 0x03,
    /// Channel record (0x04)
    #[strum(serialize = "channel")]
    Channel = 0x04,
    /// Message record (0x05)
    #[strum(serialize = "message")]
    Message = 0x05,
    /// Chunk record (0x06)
    #[strum(serialize = "chunk")]
    Chunk = 0x06,
    /// MessageIndex record (0x07)
    #[strum(serialize = "message index")]
    MessageIndex = 0x07,
    /// ChunkIndex record (0x08)
    #[strum(serialize = "chunk index")]
    ChunkIndex = 0x08,
    /// Attachment record (0x09)
    #[strum(serialize = "attachment")]
    Attachment = 0x09,
    /// AttachmentIndex record (0x0A)
    #[strum(serialize = "attachment index")]
    AttachmentIndex = 0x0A,
    /// Statistics record (0x0B)
    #[strum(serialize = "statistics")]
    Statistics = 0x0B,
    /// Metadata record (0x0C)
    #[strum(serialize = "metadata")]
    Metadata = 0x0C,
    /// MetadataIndex record (0x0D)
    #[strum(serialize = "metadata index")]
    MetadataIndex = 0x0D,
    /// SummaryOffset record (0x0E)
    #[strum(serialize = "summary offset")]
    SummaryOffset = 0x0E,
    /// DataEnd record (0x0F)
    #[strum(serialize = "data end")]
    DataEnd = 0x0F,
}

impl Opcode {
    /// Get the opcode value as a byte.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Metadata about a chunk (without compressed data).
///
/// Used for filtering chunks before decompression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkMetadata {
    /// Start time of messages in this chunk.
    pub message_start_time: Timestamp,
    /// End time of messages in this chunk.
    pub message_end_time: Timestamp,
    /// Uncompressed size of chunk data.
    pub uncompressed_size: u64,
    /// CRC32 of uncompressed chunk data.
    pub uncompressed_crc: u32,
    /// Compression algorithm used.
    pub compression: ByteStr,
    /// Size of compressed data.
    pub compressed_size: u64,
}

/// Header of a message (without payload data).
///
/// Used for filtering messages before loading/processing the full message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageHeader {
    /// Channel ID.
    pub channel_id: u16,
    /// Sequence number.
    pub sequence: u32,
    /// Log timestamp.
    pub log_time: Timestamp,
    /// Publish timestamp.
    pub publish_time: Timestamp,
    /// Size of the message data payload.
    pub data_size: u64,
}

/// Metadata about a message (without payload bytes).
///
/// Similar to `MessageHeader` but optimized for fast iteration when only
/// metadata is needed. Used by streams that skip reading payload data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageMetadata {
    /// Channel ID.
    pub channel_id: u16,
    /// Sequence number.
    pub sequence: u32,
    /// Log timestamp.
    pub log_time: Timestamp,
    /// Publish timestamp.
    pub publish_time: Timestamp,
    /// Size of the message data payload in bytes.
    pub data_size: u64,
}

impl From<MessageHeader> for MessageMetadata {
    fn from(header: MessageHeader) -> Self {
        Self {
            channel_id: header.channel_id,
            sequence: header.sequence,
            log_time: header.log_time,
            publish_time: header.publish_time,
            data_size: header.data_size,
        }
    }
}

/// MCAP file header information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Profile name (e.g., "ros1", "ros2", "").
    pub profile: ByteStr,
    /// Library that created the file (free-form).
    pub library: ByteStr,
    /// Arbitrary metadata about the file.
    pub metadata: HashMap<ByteStr, ByteStr>,
}

/// Footer with summary section offsets and CRC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    /// Start offset of the summary section.
    pub summary_start: u64,
    /// Start offset of the SummaryOffset record.
    pub summary_offset_start: u64,
    /// CRC32 of the summary section.
    pub summary_crc: u32,
}

/// Schema definition for message encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    /// Unique schema identifier.
    pub id: u16,
    /// Schema name.
    pub name: ByteStr,
    /// Encoding format (e.g., "protobuf", "ros1msg", "jsonschema").
    pub encoding: ByteStr,
    /// Schema data.
    pub data: Bytes,
}

/// Channel represents a stream of messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    /// Unique channel identifier.
    pub id: u16,
    /// Channel topic name.
    pub topic: ByteStr,
    /// Message encoding (e.g., "protobuf", "ros1", "cdr").
    pub message_encoding: ByteStr,
    /// Schema ID (0 if no schema).
    pub schema_id: u16,
    /// Arbitrary metadata about the channel.
    pub metadata: HashMap<ByteStr, ByteStr>,
}

/// A message with parsed channel and schema information.
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Channel this message belongs to.
    pub channel_id: u16,
    /// Sequence number within the channel.
    pub sequence: u32,
    /// Message publish timestamp.
    pub log_time: Timestamp,
    /// Message receive/record timestamp.
    pub publish_time: Timestamp,
    /// Payload bytes, backed by a shared buffer.
    payload: Payload,
}

/// A raw message without schema resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMessage {
    /// Channel ID.
    pub channel_id: u16,
    /// Sequence number.
    pub sequence: u32,
    /// Log timestamp.
    pub log_time: Timestamp,
    /// Publish timestamp.
    pub publish_time: Timestamp,
    /// Payload bytes, backed by a shared buffer.
    payload: Payload,
}

/// Message payload bytes backed by a shared buffer.
///
/// `backing` can be the full decompressed chunk buffer, with `start..end`
/// pointing at the message payload portion within that backing buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    backing: Bytes,
    start: u32,
    end: u32,
}

impl Payload {
    /// Create a payload backed directly by this `Bytes` value.
    pub fn from_bytes(bytes: Bytes) -> Self {
        let len: u32 = bytes.len().try_into().expect("payload length exceeds u32");
        Self {
            backing: bytes,
            start: 0,
            end: len,
        }
    }

    /// Create a payload that references a subrange of a shared backing buffer.
    pub fn from_backing_range(backing: Bytes, start: usize, end: usize) -> Self {
        debug_assert!(start <= end);
        debug_assert!(end <= backing.len());
        Self {
            backing,
            start: start.try_into().expect("start exceeds u32"),
            end: end.try_into().expect("end exceeds u32"),
        }
    }

    /// Return the payload as a borrowed byte slice.
    pub fn as_slice(&self) -> &[u8] {
        let start = self.start as usize;
        let end = self.end as usize;
        &self.backing[start..end]
    }

    /// Return the payload as a `Bytes` slice that shares the backing buffer.
    pub fn as_bytes(&self) -> Bytes {
        let start = self.start as usize;
        let end = self.end as usize;
        self.backing.slice(start..end)
    }

    /// Payload length in bytes.
    pub fn len(&self) -> usize {
        (self.end - self.start) as usize
    }

    /// Returns true if this payload is empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl RawMessage {
    /// Construct a raw message from the decoded record fields.
    pub fn new(
        channel_id: u16,
        sequence: u32,
        log_time: Timestamp,
        publish_time: Timestamp,
        payload: Payload,
    ) -> Self {
        Self {
            channel_id,
            sequence,
            log_time,
            publish_time,
            payload,
        }
    }

    /// Returns the message payload bytes.
    pub fn data(&self) -> &[u8] {
        self.payload.as_slice()
    }

    /// Returns the message payload as a `Bytes` slice (shared backing).
    pub fn data_bytes(&self) -> Bytes {
        self.payload.as_bytes()
    }

    /// Message payload length in bytes.
    pub fn data_len(&self) -> usize {
        self.payload.len()
    }
}

impl Message {
    /// Construct a message from the decoded record fields.
    pub fn new(
        channel_id: u16,
        sequence: u32,
        log_time: Timestamp,
        publish_time: Timestamp,
        payload: Payload,
    ) -> Self {
        Self {
            channel_id,
            sequence,
            log_time,
            publish_time,
            payload,
        }
    }

    /// Returns the message payload bytes.
    pub fn data(&self) -> &[u8] {
        self.payload.as_slice()
    }

    /// Returns the message payload as a `Bytes` slice (shared backing).
    pub fn data_bytes(&self) -> Bytes {
        self.payload.as_bytes()
    }

    /// Message payload length in bytes.
    pub fn data_len(&self) -> usize {
        self.payload.len()
    }
}

impl From<RawMessage> for Message {
    fn from(raw: RawMessage) -> Self {
        Self {
            channel_id: raw.channel_id,
            sequence: raw.sequence,
            log_time: raw.log_time,
            publish_time: raw.publish_time,
            payload: raw.payload,
        }
    }
}

/// A chunk of compressed messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Start time of messages in this chunk.
    pub message_start_time: Timestamp,
    /// End time of messages in this chunk.
    pub message_end_time: Timestamp,
    /// Uncompressed size of chunk data.
    pub uncompressed_size: u64,
    /// CRC32 of uncompressed chunk data.
    pub uncompressed_crc: u32,
    /// Compression algorithm used.
    pub compression: ByteStr,
    /// Compressed chunk records.
    pub records: Bytes,
}

/// Metadata key-value pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// Metadata name.
    pub name: ByteStr,
    /// Metadata entries.
    pub metadata: HashMap<ByteStr, ByteStr>,
}

/// Attachment (auxiliary file embedded in MCAP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// Creation timestamp.
    pub log_time: Timestamp,
    /// Creation timestamp for writing.
    pub create_time: Timestamp,
    /// Attachment name.
    pub name: ByteStr,
    /// Media/MIME type.
    pub media_type: ByteStr,
    /// Attachment data.
    pub data: Bytes,
}

/// Message index entry mapping timestamp to file offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageIndexEntry {
    /// Message log timestamp.
    pub timestamp: Timestamp,
    /// Offset to message record.
    pub offset: u64,
}

/// Per-channel message index for efficient seeking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageIndex {
    /// Channel this index applies to.
    pub channel_id: u16,
    /// Indexed message timestamps and offsets.
    pub records: Vec<MessageIndexEntry>,
}

/// Index entry for a chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkIndex {
    /// Start time of messages in chunk.
    pub message_start_time: Timestamp,
    /// End time of messages in chunk.
    pub message_end_time: Timestamp,
    /// File offset of chunk.
    pub chunk_start_offset: u64,
    /// Length of chunk in bytes.
    pub chunk_length: u64,
    /// Per-channel message index offsets.
    pub message_index_offsets: HashMap<u16, u64>,
    /// Length of the message index records in bytes.
    pub message_index_length: u64,
    /// Uncompressed size.
    pub uncompressed_size: u64,
    /// Compression algorithm.
    pub compression: ByteStr,
}

impl ChunkIndex {
    /// Compute the compressed payload size (excluding record header and fixed fields).
    pub fn compressed_size(&self) -> Result<u64, crate::Error> {
        const FIXED_CHUNK_FIELDS: u64 = 8 + 8 + 8 + 4 + 4 + 8;
        let compression_len: u64 =
            self.compression.as_bytes().len().try_into().map_err(|_| {
                crate::Error::InvalidRecord("compression length exceeds u64".into())
            })?;
        let overhead = (crate::format::RECORD_HEADER_SIZE as u64)
            .saturating_add(FIXED_CHUNK_FIELDS)
            .saturating_add(compression_len);
        if self.chunk_length < overhead {
            return Err(crate::Error::InvalidRecord(
                "chunk_length smaller than header overhead".into(),
            ));
        }
        Ok(self.chunk_length - overhead)
    }
}

/// Message count for a channel, as stored in a `Statistics` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelMessageCount {
    /// Channel ID.
    pub channel_id: u16,
    /// Number of messages on this channel.
    pub message_count: u64,
}

/// File-level statistics (record opcode 0x0B).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    /// Total number of `Message` records in the file.
    pub message_count: u64,
    /// Number of schemas in the file.
    pub schema_count: u16,
    /// Number of channels in the file.
    pub channel_count: u32,
    /// Number of attachments in the file.
    pub attachment_count: u32,
    /// Number of metadata records in the file.
    pub metadata_count: u32,
    /// Number of chunks in the file.
    pub chunk_count: u32,
    /// Earliest message timestamp.
    pub message_start_time: Timestamp,
    /// Latest message timestamp.
    pub message_end_time: Timestamp,
    /// Message counts per channel.
    pub channel_message_counts: Vec<ChannelMessageCount>,
}

/// MetadataIndex record (opcode 0x0D).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataIndex {
    pub offset: u64,
    pub length: u64,
    pub name: ByteStr,
}

/// AttachmentIndex record (opcode 0x0A).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentIndex {
    pub offset: u64,
    pub length: u64,
    pub log_time: Timestamp,
    pub create_time: Timestamp,
    pub data_size: u64,
    pub name: ByteStr,
    pub media_type: ByteStr,
}

/// Summary information for the entire MCAP file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// File-level statistics, if present.
    pub statistics: Option<Arc<Statistics>>,
    /// All schemas in the file.
    pub schemas: Arc<HashMap<u16, Schema>>,
    /// All channels in the file.
    pub channels: Arc<HashMap<u16, Channel>>,
    /// Chunk indexes.
    pub chunk_indexes: Arc<[ChunkIndex]>,
    /// Message indexes per channel.
    pub message_indexes: Arc<[MessageIndex]>,
    /// Attachment indexes in the file.
    pub attachment_indexes: Arc<[AttachmentIndex]>,
    /// Metadata indexes in the file.
    pub metadata_indexes: Arc<[MetadataIndex]>,
}

/// Raw record payload with opcode metadata.
///
/// Useful for low-level tooling that wants to defer parsing or copy records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRecord {
    /// Record opcode.
    pub opcode: Opcode,
    /// Full record bytes including the 9-byte record header and the payload.
    ///
    /// This is intended for fast, lossless copying of records (e.g. recovery tools).
    pub data: Bytes,
    /// Total record length in bytes, including header + payload.
    pub total_len: u64,
    /// Byte offset of the record header within the source.
    pub offset: u64,
}

/// Metadata record with its file offset and length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataEntry {
    pub metadata: Metadata,
    pub offset: u64,
    pub length: u64,
}

/// Attachment record summary with its file offset and size metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentEntry {
    pub name: ByteStr,
    pub media_type: ByteStr,
    pub log_time: Timestamp,
    pub create_time: Timestamp,
    pub data_size: u64,
    pub offset: u64,
}

/// Record header metadata without parsing the payload.
///
/// When configured, message metadata can be included without loading full payloads.
/// For chunk-contained messages, offsets are relative to the chunk payload and
/// `source` will indicate the parent chunk offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordMetadata {
    pub opcode: Opcode,
    pub length: u64,
    pub total_len: u64,
    pub offset: u64,
    /// Source location for the record metadata.
    pub source: RecordSource,
    /// Message metadata when available (only for message records).
    pub message: Option<MessageMetadata>,
}

/// Origin of a record metadata entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordSource {
    /// Record lives in the file data section; offset is file-relative.
    File,
    /// Record lives inside a chunk payload; offset is chunk-relative.
    Chunk { chunk_offset: u64 },
}

impl RawRecord {
    /// Returns the full record bytes (header + payload).
    pub fn bytes(&self) -> &Bytes {
        &self.data
    }

    /// Returns just the record payload bytes (excluding the 9-byte record header).
    pub fn payload(&self) -> Bytes {
        self.data.slice(crate::format::RECORD_HEADER_SIZE..)
    }
}

/// Represents any record type in an MCAP file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    Header(Header),
    Footer(Footer),
    Schema(Schema),
    Channel(Channel),
    Message(Message),
    Chunk(Chunk),
    MessageIndex(MessageIndex),
    ChunkIndex(ChunkIndex),
    Attachment(Attachment),
    AttachmentIndex(AttachmentIndex),
    Statistics(Statistics),
    Metadata(Metadata),
    MetadataIndex(MetadataIndex),
    SummaryOffset,
    DataEnd,
}
