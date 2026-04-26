use crate::support::HashMap;
use bytes::Bytes;
use std::cell::RefCell;
use std::io::{Seek, Write};
use std::rc::Rc;

use crate::compression::Compression;
use crate::error::Result;
use crate::types::{
    Attachment, Channel, Chunk, Message, Metadata, Payload, RawMessage, Record, Schema,
};
use crate::zero_copy::ByteStr;

use super::constants::DEFAULT_CHUNK_MAX_UNCOMPRESSED_BYTES;
use super::internal::WriterImpl;

/// Input message payload for [`ChannelWriter::write`] and [`ChannelWriter::write_with_sequence`].
///
/// Implemented for:
/// - `Bytes` / `&Bytes` (zero-copy, cheap clone)
/// - `Vec<u8>` and `Box<[u8]>` (no copy; moved into `Bytes`)
/// - `&[u8]` (copied into `Bytes`)
pub trait IntoPayloadBytes {
    /// Convert this value into owned `Bytes`.
    fn into_payload_bytes(self) -> Bytes;
}

impl IntoPayloadBytes for Bytes {
    fn into_payload_bytes(self) -> Bytes {
        self
    }
}

impl IntoPayloadBytes for &Bytes {
    fn into_payload_bytes(self) -> Bytes {
        self.clone()
    }
}

impl IntoPayloadBytes for Vec<u8> {
    fn into_payload_bytes(self) -> Bytes {
        Bytes::from(self)
    }
}

impl IntoPayloadBytes for Box<[u8]> {
    fn into_payload_bytes(self) -> Bytes {
        Bytes::from(self.into_vec())
    }
}

impl IntoPayloadBytes for &[u8] {
    fn into_payload_bytes(self) -> Bytes {
        Bytes::copy_from_slice(self)
    }
}

/// Writer validation level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validation {
    /// Enforce record ordering and required metadata (recommended).
    Strict,
    /// Skip validation checks; may produce invalid MCAP if misused.
    Permissive,
}

impl Default for Validation {
    fn default() -> Self {
        Self::Strict
    }
}

/// Configuration for chunked message writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkOptions {
    /// Compression algorithm to apply to chunk record bodies.
    pub compression: Option<Compression>,
    /// Flush chunk when uncompressed buffer reaches this size.
    pub max_uncompressed_bytes: usize,
    /// Include CRC32 checksums in chunk records (default: `true`).
    ///
    /// When `false`, the chunk `uncompressed_crc` field is written as `0`
    /// and the per-message CRC computation is skipped entirely.
    pub include_crc: bool,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            compression: None,
            max_uncompressed_bytes: DEFAULT_CHUNK_MAX_UNCOMPRESSED_BYTES,
            include_crc: true,
        }
    }
}

/// A schema definition used when creating channels via [`Writer::add_channel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSpec {
    /// Schema name, e.g. `pkg/Msg`.
    pub name: ByteStr,
    /// Schema encoding, e.g. `jsonschema`.
    pub encoding: ByteStr,
    /// Schema data bytes.
    pub data: Bytes,
}

impl SchemaSpec {
    /// Create a new schema spec.
    pub fn new(name: impl Into<ByteStr>, encoding: impl Into<ByteStr>, data: Bytes) -> Self {
        Self {
            name: name.into(),
            encoding: encoding.into(),
            data,
        }
    }
}

/// A channel definition used when creating channels via [`Writer::add_channel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSpec {
    /// Topic name, e.g. `/tf`.
    pub topic: ByteStr,
    /// Message encoding, e.g. `cdr`.
    pub message_encoding: ByteStr,
    /// Optional schema definition. When `None`, the channel uses `schema_id = 0`.
    pub schema: Option<SchemaSpec>,
    /// Channel metadata map.
    pub metadata: HashMap<ByteStr, ByteStr>,
    /// When `Some`, messages on this channel land in their own dedicated
    /// chunk stream configured by these options. When `None`, messages flow
    /// into the writer's default chunk stream (or top-level if the writer
    /// was built without `.chunked(...)`).
    pub chunk_override: Option<ChunkOptions>,
}

impl ChannelSpec {
    /// Create a new channel spec without a schema.
    pub fn new(topic: impl Into<ByteStr>, message_encoding: impl Into<ByteStr>) -> Self {
        Self {
            topic: topic.into(),
            message_encoding: message_encoding.into(),
            schema: None,
            metadata: HashMap::new(),
            chunk_override: None,
        }
    }

    /// Set the channel schema.
    pub fn schema(mut self, schema: SchemaSpec) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Set the channel metadata map.
    pub fn metadata(mut self, metadata: HashMap<ByteStr, ByteStr>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Route this channel's messages into a dedicated chunk stream configured
    /// by `options`. Useful for writing already-compressed payloads as
    /// uncompressed chunks while the rest of the file uses compression.
    pub fn chunk_override(mut self, options: ChunkOptions) -> Self {
        self.chunk_override = Some(options);
        self
    }

    /// Convenience: route this channel into a dedicated chunk stream with
    /// `compression: None`. Other `ChunkOptions` fields take their defaults.
    pub fn uncompressed_chunks(mut self) -> Self {
        // `compression: None` is explicit — guards intent against any future
        // change to `ChunkOptions::default()`'s compression default.
        self.chunk_override = Some(ChunkOptions {
            compression: None,
            ..ChunkOptions::default()
        });
        self
    }
}

/// A per-channel helper for writing messages with automatic sequence numbering.
#[derive(Clone)]
pub struct ChannelWriter<W: Write + Seek> {
    pub(crate) inner: Rc<RefCell<WriterImpl<W>>>,
    pub(crate) channel_id: u16,
    pub(crate) next_sequence: u32,
    /// True iff this channel has a `chunk_override` registered in
    /// `WriterImpl::override_streams`. Cached so the per-message
    /// write path can dispatch with one bool branch and never hash.
    pub(crate) has_chunk_override: bool,
}

impl<W: Write + Seek> ChannelWriter<W> {
    /// Returns the channel id written to the file.
    pub fn channel_id(&self) -> u16 {
        self.channel_id
    }

    /// Set the starting sequence number (default is 0).
    pub fn starting_sequence(mut self, sequence: u32) -> Self {
        self.next_sequence = sequence;
        self
    }

    /// Write a message to this channel with the next sequence number.
    pub fn write<D: IntoPayloadBytes>(
        &mut self,
        log_time: u64,
        publish_time: u64,
        data: D,
    ) -> Result<()> {
        let seq = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.write_with_sequence(log_time, publish_time, data, seq)
    }

    /// Write a message to this channel with a specific sequence number.
    /// Useful when copying messages and preserving original sequence numbers.
    pub fn write_with_sequence<D: IntoPayloadBytes>(
        &mut self,
        log_time: u64,
        publish_time: u64,
        data: D,
        sequence: u32,
    ) -> Result<()> {
        let bytes = data.into_payload_bytes();
        let msg = RawMessage::new(
            self.channel_id,
            sequence,
            log_time,
            publish_time,
            Payload::from_bytes(bytes),
        );
        let mut inner = self.inner.borrow_mut();
        if self.has_chunk_override {
            inner.write_raw_message_override(&msg)
        } else {
            inner.write_raw_message_default(&msg)
        }
    }
}

/// A helper for writing attachments.
#[derive(Clone)]
pub struct AttachmentWriter<W: Write + Seek> {
    pub(crate) inner: Rc<RefCell<WriterImpl<W>>>,
}

impl<W: Write + Seek> AttachmentWriter<W> {
    /// Write an attachment record (opcode 0x09).
    pub fn write(
        &mut self,
        log_time: u64,
        create_time: u64,
        name: ByteStr,
        media_type: ByteStr,
        data: Bytes,
    ) -> Result<()> {
        self.inner.borrow_mut().write_attachment_internal(
            log_time,
            create_time,
            name,
            media_type,
            data,
        )
    }
}

/// MCAP writer.
///
/// The writer uses an internal ref-counted implementation to allow
/// multiple [`ChannelWriter`] instances to exist simultaneously, while
/// avoiding per-message locking overhead.
#[derive(Clone)]
pub struct Writer<W: Write + Seek> {
    pub(crate) inner: Rc<RefCell<WriterImpl<W>>>,
}

impl<W: Write + Seek> Writer<W> {
    /// Consume the writer and return the underlying sink.
    ///
    /// This does not automatically finalize the MCAP file.
    pub fn into_inner(self) -> W {
        Rc::try_unwrap(self.inner)
            .unwrap_or_else(|_| {
                panic!(
                    "Writer is still referenced by ChannelWriter or AttachmentWriter. Drop all writers before calling into_inner()."
                )
            })
            .into_inner()
            .sink
            .into_inner()
    }

    /// Add a channel and return a writer for writing messages to it.
    ///
    /// The channel ID is automatically assigned. Use the returned `ChannelWriter`
    /// to write messages to this channel.
    pub fn add_channel(&mut self, spec: ChannelSpec) -> Result<ChannelWriter<W>> {
        let (channel_id, has_chunk_override) = self.inner.borrow_mut().add_channel_spec(spec)?;
        Ok(ChannelWriter {
            inner: Rc::clone(&self.inner),
            channel_id,
            next_sequence: 0,
            has_chunk_override,
        })
    }

    /// Create an attachment writer.
    pub fn attachment_writer(&mut self) -> AttachmentWriter<W> {
        AttachmentWriter {
            inner: Rc::clone(&self.inner),
        }
    }

    /// Copy a schema from another MCAP file.
    ///
    /// Use this when copying schemas from an existing file. For new schemas,
    /// use [`Writer::add_channel`] with [`SchemaSpec`] instead.
    pub fn copy_schema(&mut self, schema: &Schema) -> Result<()> {
        self.inner.borrow_mut().write_schema_internal(schema)
    }

    /// Copy a channel from another MCAP file.
    ///
    /// This preserves the channel id from `channel`. The returned [`ChannelWriter`]
    /// can be used to write messages to the copied channel.
    pub fn copy_channel(&mut self, channel: &Channel) -> Result<ChannelWriter<W>> {
        self.inner.borrow_mut().write_channel_internal(channel)?;
        Ok(ChannelWriter {
            inner: Rc::clone(&self.inner),
            channel_id: channel.id,
            next_sequence: 0,
            has_chunk_override: false,
        })
    }

    /// Copy a channel from another MCAP file *and* route its messages into a
    /// dedicated chunk stream configured by `options`. Equivalent to
    /// `add_channel(ChannelSpec::...chunk_override(options))` for the spec
    /// path; this variant is for pipelines that only have a parsed `Channel`.
    ///
    /// Preserves the channel id from `channel`. Calling this twice for the
    /// same channel id silently overwrites the first override registration.
    pub fn copy_channel_with_override(
        &mut self,
        channel: &Channel,
        options: ChunkOptions,
    ) -> Result<ChannelWriter<W>> {
        let mut inner = self.inner.borrow_mut();
        inner.write_channel_internal(channel)?;
        inner.register_channel_override(channel.id, options);
        drop(inner);
        Ok(ChannelWriter {
            inner: Rc::clone(&self.inner),
            channel_id: channel.id,
            next_sequence: 0,
            has_chunk_override: true,
        })
    }

    /// Copy an attachment from another MCAP file.
    ///
    /// Use this when copying attachments from an existing file. For new attachments,
    /// use `attachment_writer().write()` instead.
    pub fn copy_attachment(
        &mut self,
        log_time: u64,
        create_time: u64,
        name: ByteStr,
        media_type: ByteStr,
        data: Bytes,
    ) -> Result<()> {
        self.inner.borrow_mut().write_attachment_internal(
            log_time,
            create_time,
            name,
            media_type,
            data,
        )
    }

    /// Copy metadata from another MCAP file.
    ///
    /// Use this when copying metadata from an existing file.
    pub fn copy_metadata(&mut self, metadata: &Metadata) -> Result<()> {
        self.inner.borrow_mut().write_metadata_internal(metadata)
    }

    /// Copy a raw record payload as-is into the file, updating summary bookkeeping.
    ///
    /// This is intended for tooling that wants to preserve existing record bytes (e.g. `recover`
    /// or a future `reindex`) while still producing a valid summary/footer.
    ///
    /// Notes:
    /// - This writes a single `(opcode, length, payload)` record to the data section.
    /// - The writer will still write its own `DataEnd`, summary section, `Footer`, and trailing
    ///   magic bytes when [`Writer::finish`] is called.
    /// - Callers should generally avoid copying `DataEnd`/summary/footer records from an input
    ///   file; instead, let `finish()` produce fresh ones.
    pub fn copy_raw_record(&mut self, opcode: crate::types::Opcode, payload: Bytes) -> Result<()> {
        self.inner
            .borrow_mut()
            .write_raw_record_internal(opcode, payload)
    }

    /// Copy a full record (header + payload) into the output without re-encoding.
    ///
    /// This is intended for fast, lossless record copying from a [`crate::types::RawRecord`] stream.
    pub fn copy_raw_record_bytes(&mut self, record: Bytes) -> Result<()> {
        self.inner
            .borrow_mut()
            .write_raw_record_bytes_internal(record)
    }

    /// Copy a parsed [`Record`] into the output, updating summary bookkeeping.
    ///
    /// This is intended for tooling like `recover`/`reindex` that wants to rebuild a correct
    /// summary section without decompressing/recompressing chunks or re-chunking messages.
    ///
    /// Notes:
    /// - `Header` records are ignored (the writer always emits its own header).
    /// - `DataEnd`, summary records, and `Footer` records should generally *not* be copied; let
    ///   [`Writer::finish`] write fresh ones.
    pub fn copy_record(&mut self, record: &Record) -> Result<()> {
        match record {
            Record::Header(_) => Ok(()),
            Record::Schema(schema) => self.copy_schema(schema),
            Record::Channel(channel) => {
                let _ = self.copy_channel(channel)?;
                Ok(())
            }
            Record::Message(message) => self.copy_message_record(message),
            Record::Chunk(chunk) => self.copy_chunk_record(chunk),
            Record::Attachment(att) => self.copy_attachment_record(att),
            Record::Metadata(md) => self.copy_metadata(md),
            _ => Ok(()),
        }
    }

    /// Copy a top-level `Message` record without re-chunking.
    pub fn copy_message_record(&mut self, message: &Message) -> Result<()> {
        self.inner.borrow_mut().write_message_record_internal(
            message.channel_id,
            message.sequence,
            message.log_time,
            message.publish_time,
            message.data(),
        )
    }

    /// Copy a `Chunk` record without decompressing/recompressing the chunk body.
    pub fn copy_chunk_record(&mut self, chunk: &Chunk) -> Result<()> {
        self.inner.borrow_mut().write_chunk_record_internal(chunk)
    }

    /// Copy an `Attachment` record without copying the attachment data into a new buffer.
    pub fn copy_attachment_record(&mut self, att: &Attachment) -> Result<()> {
        self.copy_attachment(
            att.log_time,
            att.create_time,
            att.name.clone(),
            att.media_type.clone(),
            att.data.clone(),
        )
    }

    /// Finalize the file (DataEnd, Footer, trailing magic bytes).
    pub fn finish(&mut self) -> Result<()> {
        self.inner.borrow_mut().finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::Compression;

    #[test]
    fn channel_spec_carries_chunk_override() {
        let opts = ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 1024,
            include_crc: false,
        };
        let spec = ChannelSpec::new("/cam", "h264").chunk_override(opts.clone());
        assert_eq!(spec.chunk_override, Some(opts));
    }

    #[test]
    fn channel_spec_uncompressed_chunks_helper_sets_compression_none() {
        let spec = ChannelSpec::new("/cam", "h264").uncompressed_chunks();
        let chunk = spec.chunk_override.expect("override set");
        assert!(chunk.compression.is_none());
    }

    #[test]
    fn channel_spec_default_has_no_override() {
        let spec = ChannelSpec::new("/cam", "h264");
        assert!(spec.chunk_override.is_none());
    }

    #[test]
    fn chunk_options_eq() {
        let a = ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 4096,
            include_crc: true,
        };
        let b = a.clone();
        assert_eq!(a, b);

        // Confirm PartialEq actually discriminates field changes — guards
        // against a hypothetical hand-rolled impl that ignored a field.
        let differs_compression = ChunkOptions {
            compression: None,
            ..a.clone()
        };
        let differs_size = ChunkOptions {
            max_uncompressed_bytes: 0,
            ..a.clone()
        };
        let differs_crc = ChunkOptions {
            include_crc: false,
            ..a.clone()
        };
        assert_ne!(a, differs_compression);
        assert_ne!(a, differs_size);
        assert_ne!(a, differs_crc);
    }
}
