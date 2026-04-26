use crate::support::HashMap;
use bytes::Bytes;
use std::io::{Seek, Write};

use crate::compression::calculate_crc;
use crate::error::{Error, Result};
use crate::types::{Channel, Chunk, Header, Metadata, Opcode, RawMessage, Schema};
use crate::zero_copy::ByteStr;

use super::api::{ChannelSpec, ChunkOptions, SchemaSpec, Validation};
use super::chunk::{ChunkState, prepare_chunk_for_write};
use super::constants::MESSAGE_RECORD_PREFIX_LEN;
use super::encode;
use super::io::{PositionTrackingSink, write_all_vectored_chunk_records, write_all_vectored2};
use super::types::{
    AttachmentIndexInfo, ChannelStats, ChunkIndexInfo, MetadataIndexInfo, SchemaKey, SummaryGroup,
    should_write_summary,
};

/// Internal writer implementation that contains all state and logic.
pub(crate) struct WriterImpl<W: Write + Seek> {
    pub(crate) sink: PositionTrackingSink<W>,
    pub(crate) header: Header,
    pub(crate) wrote_header: bool,
    pub(crate) finished: bool,
    pub(crate) next_schema_id: u16,
    pub(crate) next_channel_id: u16,
    #[allow(dead_code)] // Will be used for strict ordering and validity checks.
    pub(crate) validation: Validation,
    pub(crate) always_write_summary: bool,
    pub(crate) chunk_state: Option<ChunkState>,
    /// Per-channel override chunk streams, indexed by `channel_id`.
    /// `Some(state)` means this channel has a `chunk_override` registered;
    /// its messages flow into `state` instead of `chunk_state`. The vec
    /// grows as channels are registered (mirrors `channel_stats`).
    pub(crate) override_streams: Vec<Option<ChunkState>>,
    pub(crate) schemas: HashMap<u16, Schema>,
    pub(crate) channels: HashMap<u16, Channel>,
    pub(crate) channel_stats: Vec<ChannelStats>,
    pub(crate) chunk_indexes: Vec<ChunkIndexInfo>,
    pub(crate) attachment_indexes: Vec<AttachmentIndexInfo>,
    pub(crate) metadata_indexes: Vec<MetadataIndexInfo>,
    pub(crate) schema_ids_by_key: HashMap<SchemaKey, u16>,
}

impl<W: Write + Seek> WriterImpl<W> {
    fn update_channel_stats(&mut self, channel_id: u16, log_time: u64) {
        let idx = channel_id as usize;
        if self.channel_stats.len() <= idx {
            self.channel_stats
                .resize_with(idx.saturating_add(1), ChannelStats::default);
        }
        let entry = &mut self.channel_stats[idx];
        entry.message_count = entry.message_count.saturating_add(1);
        if !entry.has_messages {
            entry.message_start_time = log_time;
            entry.message_end_time = log_time;
            entry.has_messages = true;
        } else {
            entry.message_start_time = entry.message_start_time.min(log_time);
            entry.message_end_time = entry.message_end_time.max(log_time);
        }
    }

    fn write_record_with_optional_crc(
        &mut self,
        opcode: Opcode,
        payload: &[u8],
        crc: Option<&mut crc32fast::Hasher>,
    ) -> Result<()> {
        let mut header = [0u8; crate::format::RECORD_HEADER_SIZE];
        header[0] = opcode.as_u8();
        header[1..].copy_from_slice(&(payload.len() as u64).to_le_bytes());

        if let Some(hasher) = crc {
            hasher.update(&header);
            hasher.update(payload);
        }

        write_all_vectored2(&mut self.sink, &header, payload)?;
        Ok(())
    }

    fn flush_chunk_if_needed(&mut self, force: bool) -> Result<()> {
        let Some(state) = self.chunk_state.as_mut() else {
            return Ok(());
        };
        if !state.should_flush(force) {
            return Ok(());
        }

        let flush = state.take_for_flush();
        let sink_position = self.sink.position();
        let prepared = prepare_chunk_for_write(sink_position, &flush)?;
        self.chunk_indexes.push(prepared.index);

        write_all_vectored2(
            &mut self.sink,
            &prepared.record_header,
            &prepared.record_prefix,
        )?;
        if prepared.write_uncompressed_records {
            write_all_vectored_chunk_records(
                &mut self.sink,
                &flush.message_prefixes,
                &flush.payloads,
                MESSAGE_RECORD_PREFIX_LEN,
            )?;
        } else if let Some(compressed) = prepared.compressed_body {
            self.sink.write_all(&compressed)?;
        }

        state.recycle_buffers(flush.message_prefixes, flush.payloads);
        Ok(())
    }

    fn flush_override_if_needed(&mut self, channel_idx: usize, force: bool) -> Result<()> {
        let Some(state) = self
            .override_streams
            .get_mut(channel_idx)
            .and_then(|slot| slot.as_mut())
        else {
            return Ok(());
        };
        if !state.should_flush(force) {
            return Ok(());
        }

        let flush = state.take_for_flush();
        let sink_position = self.sink.position();
        let prepared = prepare_chunk_for_write(sink_position, &flush)?;
        self.chunk_indexes.push(prepared.index);

        write_all_vectored2(
            &mut self.sink,
            &prepared.record_header,
            &prepared.record_prefix,
        )?;
        if prepared.write_uncompressed_records {
            write_all_vectored_chunk_records(
                &mut self.sink,
                &flush.message_prefixes,
                &flush.payloads,
                MESSAGE_RECORD_PREFIX_LEN,
            )?;
        } else if let Some(compressed) = prepared.compressed_body {
            self.sink.write_all(&compressed)?;
        }

        self.override_streams[channel_idx]
            .as_mut()
            .expect("override slot must still be Some after take_for_flush")
            .recycle_buffers(flush.message_prefixes, flush.payloads);
        Ok(())
    }

    /// Give every populated override stream a chance to flush at its threshold.
    /// Cheap when nothing is at threshold (one `should_flush` check per slot).
    fn flush_overrides_if_needed_all(&mut self, force: bool) -> Result<()> {
        for idx in 0..self.override_streams.len() {
            self.flush_override_if_needed(idx, force)?;
        }
        Ok(())
    }

    /// Insert (or replace) an override `ChunkState` at the given channel-id
    /// slot, growing `override_streams` if needed. Silently overwrites any
    /// previously registered override for the same channel.
    fn set_override_slot(&mut self, idx: usize, state: ChunkState) {
        if self.override_streams.len() <= idx {
            self.override_streams
                .resize_with(idx.saturating_add(1), || None);
        }
        self.override_streams[idx] = Some(state);
    }

    fn encode_schema_payload(schema: &Schema) -> Result<Vec<u8>> {
        let mut payload = Vec::new();
        encode::push_le_u16(&mut payload, schema.id);
        encode::push_len_prefixed_str(&mut payload, &schema.name)?;
        encode::push_len_prefixed_str(&mut payload, &schema.encoding)?;
        let data_len: u32 = schema
            .data
            .len()
            .try_into()
            .map_err(|_| Error::InvalidRecord("schema data length exceeds u32".to_string()))?;
        encode::push_le_u32(&mut payload, data_len);
        payload.extend_from_slice(schema.data.as_ref());
        Ok(payload)
    }

    fn encode_channel_payload(channel: &Channel) -> Result<Vec<u8>> {
        let mut payload = Vec::new();
        encode::push_le_u16(&mut payload, channel.id);
        encode::push_le_u16(&mut payload, channel.schema_id);
        encode::push_len_prefixed_str(&mut payload, &channel.topic)?;
        encode::push_len_prefixed_str(&mut payload, &channel.message_encoding)?;
        encode::push_metadata_map(&mut payload, &channel.metadata)?;
        Ok(payload)
    }

    fn write_chunk_index_record(
        &mut self,
        info: &ChunkIndexInfo,
        crc: &mut crc32fast::Hasher,
    ) -> Result<()> {
        let mut payload = Vec::new();
        encode::push_le_u64(&mut payload, info.message_start_time);
        encode::push_le_u64(&mut payload, info.message_end_time);
        encode::push_le_u64(&mut payload, info.chunk_start_offset);
        encode::push_le_u64(&mut payload, info.chunk_length);

        encode::push_le_u32(&mut payload, 0);
        encode::push_le_u64(&mut payload, 0);

        encode::push_len_prefixed_str(&mut payload, &info.compression)?;
        encode::push_le_u64(&mut payload, info.compressed_size);
        encode::push_le_u64(&mut payload, info.uncompressed_size);

        self.write_record_with_optional_crc(Opcode::ChunkIndex, &payload, Some(crc))
    }

    fn write_statistics_record(&mut self, crc: &mut crc32fast::Hasher) -> Result<()> {
        let mut total_message_count = 0u64;
        let mut earliest_time = u64::MAX;
        let mut latest_time = 0u64;
        let mut channel_message_counts: HashMap<u16, u64> = HashMap::default();

        for (idx, stats) in self.channel_stats.iter().enumerate() {
            if idx == 0 {
                continue;
            }
            if stats.has_messages {
                let channel_id = u16::try_from(idx)
                    .map_err(|_| Error::InvalidRecord("invalid channel id".to_string()))?;
                total_message_count += stats.message_count;
                earliest_time = earliest_time.min(stats.message_start_time);
                latest_time = latest_time.max(stats.message_end_time);
                channel_message_counts.insert(channel_id, stats.message_count);
            }
        }

        let msg_start_time = if earliest_time == u64::MAX {
            0
        } else {
            earliest_time
        };
        let msg_end_time = latest_time;

        let mut payload = Vec::new();
        encode::push_le_u64(&mut payload, total_message_count);
        encode::push_le_u16(&mut payload, self.schemas.len() as u16);
        encode::push_le_u32(&mut payload, self.channels.len() as u32);
        encode::push_le_u32(&mut payload, self.attachment_indexes.len() as u32);
        encode::push_le_u32(&mut payload, self.metadata_indexes.len() as u32);
        encode::push_le_u32(&mut payload, self.chunk_indexes.len() as u32);
        encode::push_le_u64(&mut payload, msg_start_time);
        encode::push_le_u64(&mut payload, msg_end_time);

        let mut map_body = Vec::new();
        for (channel_id, count) in &channel_message_counts {
            encode::push_le_u16(&mut map_body, *channel_id);
            encode::push_le_u64(&mut map_body, *count);
        }
        encode::push_len_prefixed_bytes(&mut payload, &map_body)?;

        self.write_record_with_optional_crc(Opcode::Statistics, &payload, Some(crc))
    }

    fn allocate_schema_id(&mut self) -> u16 {
        let id = self.next_schema_id;
        self.next_schema_id = self.next_schema_id.saturating_add(1);
        id
    }

    fn allocate_channel_id(&mut self) -> u16 {
        let id = self.next_channel_id;
        self.next_channel_id = self.next_channel_id.saturating_add(1);
        id
    }

    fn add_schema(&mut self, name: ByteStr, encoding: ByteStr, data: Bytes) -> Result<u16> {
        let id = self.allocate_schema_id();
        let schema = Schema {
            id,
            name,
            encoding,
            data,
        };
        self.write_schema_internal(&schema)?;
        Ok(id)
    }

    fn schema_id_for_spec(&mut self, spec: SchemaSpec) -> Result<u16> {
        let key = SchemaKey {
            name: spec.name.clone(),
            encoding: spec.encoding.clone(),
            data: spec.data.clone(),
        };
        if let Some(id) = self.schema_ids_by_key.get(&key) {
            return Ok(*id);
        }
        let id = self.add_schema(spec.name, spec.encoding, spec.data)?;
        self.schema_ids_by_key.insert(key, id);
        Ok(id)
    }

    pub(crate) fn add_channel_spec(&mut self, spec: ChannelSpec) -> Result<(u16, bool)> {
        let schema_id = match spec.schema {
            Some(schema) => self.schema_id_for_spec(schema)?,
            None => 0,
        };

        let id = self.allocate_channel_id();
        let channel = Channel {
            id,
            topic: spec.topic,
            message_encoding: spec.message_encoding,
            schema_id,
            metadata: spec.metadata,
        };
        self.write_channel_internal(&channel)?;

        let has_override = if let Some(opts) = spec.chunk_override {
            self.set_override_slot(id as usize, ChunkState::new(opts));
            true
        } else {
            false
        };

        Ok((id, has_override))
    }

    /// Register an override `ChunkState` for an already-known channel id.
    /// Silently overwrites any previously registered override for the same channel.
    pub(crate) fn register_channel_override(&mut self, channel_id: u16, options: ChunkOptions) {
        self.set_override_slot(channel_id as usize, ChunkState::new(options));
    }

    pub(crate) fn write_attachment_internal(
        &mut self,
        log_time: u64,
        create_time: u64,
        name: ByteStr,
        media_type: ByteStr,
        data: Bytes,
    ) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write attachment after finish".to_string(),
            ));
        }
        self.write_header()?;
        self.flush_overrides_if_needed_all(false)?;
        self.flush_chunk_if_needed(false)?;

        // Detach small string fields from any large backing buffers (e.g. record-copy tooling).
        let name = name.to_compact();
        let media_type = media_type.to_compact();

        let data_len: u64 = data
            .len()
            .try_into()
            .map_err(|_| Error::InvalidRecord("attachment data length exceeds u64".to_string()))?;
        let crc = calculate_crc(data.as_ref());

        let start = self.sink.position();

        // Attachment payload is potentially large; avoid building a single contiguous Vec by
        // writing the fixed fields, then the data, then the crc.
        let name_len: u32 =
            name.as_ref().len().try_into().map_err(|_| {
                Error::InvalidRecord("attachment name length exceeds u32".to_string())
            })?;
        let media_len: u32 = media_type.as_ref().len().try_into().map_err(|_| {
            Error::InvalidRecord("attachment media_type length exceeds u32".to_string())
        })?;

        let fixed_len = 8 + 8 + 4 + (name_len as usize) + 4 + (media_len as usize) + 8 + 4;
        let payload_len = fixed_len
            .checked_add(data.len())
            .ok_or_else(|| Error::InvalidRecord("attachment length overflow".to_string()))?;

        let length = (crate::format::RECORD_HEADER_SIZE as u64)
            .checked_add(payload_len as u64)
            .ok_or_else(|| Error::InvalidRecord("attachment length overflow".to_string()))?;
        self.attachment_indexes.push(AttachmentIndexInfo {
            offset: start,
            length,
            log_time,
            create_time,
            data_size: data_len,
            name: name.clone(),
            media_type: media_type.clone(),
        });

        let mut fixed = Vec::with_capacity(crate::format::RECORD_HEADER_SIZE + fixed_len);
        encode::push_record_header(&mut fixed, Opcode::Attachment, payload_len as u64);
        encode::push_le_u64(&mut fixed, log_time);
        encode::push_le_u64(&mut fixed, create_time);
        encode::push_len_prefixed_str(&mut fixed, &name)?;
        encode::push_len_prefixed_str(&mut fixed, &media_type)?;
        encode::push_le_u64(&mut fixed, data_len);

        // Write header+fixed fields + data bytes + crc without copying `data` into `fixed`.
        write_all_vectored2(&mut self.sink, &fixed, data.as_ref())?;
        self.sink.write_all(&crc.to_le_bytes())?;
        Ok(())
    }

    pub(crate) fn write_metadata_internal(&mut self, metadata: &Metadata) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write metadata after finish".to_string(),
            ));
        }
        self.write_header()?;
        self.flush_overrides_if_needed_all(false)?;
        self.flush_chunk_if_needed(false)?;

        // Detach small string fields from any large backing buffers (e.g. record-copy tooling).
        let mut metadata = metadata.clone();
        metadata.name = metadata.name.to_compact();
        metadata.metadata = metadata
            .metadata
            .into_iter()
            .map(|(k, v)| (k.to_compact(), v.to_compact()))
            .collect();

        let start = self.sink.position();

        let mut payload = Vec::new();
        encode::push_len_prefixed_str(&mut payload, &metadata.name)?;
        encode::push_metadata_map(&mut payload, &metadata.metadata)?;

        let length = (crate::format::RECORD_HEADER_SIZE as u64)
            .checked_add(payload.len() as u64)
            .ok_or_else(|| Error::InvalidRecord("metadata length overflow".to_string()))?;
        self.metadata_indexes.push(MetadataIndexInfo {
            offset: start,
            length,
            name: metadata.name.clone(),
        });

        encode::write_record(&mut self.sink, Opcode::Metadata, &payload)?;
        Ok(())
    }

    fn write_attachment_index_record(
        &mut self,
        info: &AttachmentIndexInfo,
        crc: &mut crc32fast::Hasher,
    ) -> Result<()> {
        let mut payload = Vec::new();
        encode::push_le_u64(&mut payload, info.offset);
        encode::push_le_u64(&mut payload, info.length);
        encode::push_le_u64(&mut payload, info.log_time);
        encode::push_le_u64(&mut payload, info.create_time);
        encode::push_le_u64(&mut payload, info.data_size);
        encode::push_len_prefixed_str(&mut payload, &info.name)?;
        encode::push_len_prefixed_str(&mut payload, &info.media_type)?;
        self.write_record_with_optional_crc(Opcode::AttachmentIndex, &payload, Some(crc))
    }

    fn write_metadata_index_record(
        &mut self,
        info: &MetadataIndexInfo,
        crc: &mut crc32fast::Hasher,
    ) -> Result<()> {
        let mut payload = Vec::new();
        encode::push_le_u64(&mut payload, info.offset);
        encode::push_le_u64(&mut payload, info.length);
        encode::push_len_prefixed_str(&mut payload, &info.name)?;
        self.write_record_with_optional_crc(Opcode::MetadataIndex, &payload, Some(crc))
    }

    pub(crate) fn write_raw_record_internal(
        &mut self,
        opcode: Opcode,
        payload: Bytes,
    ) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write record after finish".to_string(),
            ));
        }

        // Ensure header is written and any buffered chunk is flushed before copying raw records.
        self.write_header()?;
        self.flush_overrides_if_needed_all(false)?;
        self.flush_chunk_if_needed(false)?;

        match opcode {
            Opcode::Schema => {
                let mut schema = crate::parser::parse_schema_record(payload.clone())?;
                schema.name = schema.name.to_compact();
                schema.encoding = schema.encoding.to_compact();
                schema.data = Bytes::copy_from_slice(schema.data.as_ref());
                self.next_schema_id = self.next_schema_id.max(schema.id.saturating_add(1));
                self.schemas.insert(schema.id, schema);
            }
            Opcode::Channel => {
                let mut channel = crate::parser::parse_channel_record(payload.clone())?;
                channel.topic = channel.topic.to_compact();
                channel.message_encoding = channel.message_encoding.to_compact();
                channel.metadata = channel
                    .metadata
                    .into_iter()
                    .map(|(k, v)| (k.to_compact(), v.to_compact()))
                    .collect();
                self.next_channel_id = self.next_channel_id.max(channel.id.saturating_add(1));
                self.channels.insert(channel.id, channel);
            }
            Opcode::Message => {
                let header = crate::parser::parse_message_header_from_content(payload.as_ref())?;
                self.update_channel_stats(header.channel_id, header.log_time);
            }
            Opcode::Chunk => {
                let start = self.sink.position();
                let chunk = crate::parser::parse_chunk_record(payload.clone())?;
                let chunk_len = (crate::format::RECORD_HEADER_SIZE as u64)
                    .checked_add(payload.len() as u64)
                    .ok_or_else(|| Error::InvalidRecord("chunk length overflow".to_string()))?;
                let compressed_size = u64::try_from(chunk.records.len()).map_err(|_| {
                    Error::InvalidRecord("chunk compressed size exceeds u64".to_string())
                })?;
                self.chunk_indexes.push(ChunkIndexInfo {
                    message_start_time: chunk.message_start_time,
                    message_end_time: chunk.message_end_time,
                    chunk_start_offset: start,
                    chunk_length: chunk_len,
                    compression: chunk.compression.to_compact(),
                    compressed_size,
                    uncompressed_size: chunk.uncompressed_size,
                });

                // Best-effort message statistics without decompression:
                // if the chunk is uncompressed, scan its record stream and count messages.
                if chunk.compression.as_str().is_empty() || chunk.compression.as_str() == "none" {
                    for loc in crate::records::RecordIterator::new(chunk.records.as_ref(), 0) {
                        let loc = match loc {
                            Ok(l) => l,
                            Err(_) => break,
                        };
                        if loc.header.opcode != Opcode::Message.as_u8() {
                            continue;
                        }
                        let (hdr, _payload) = match crate::parser::parse_message_from_backing_range(
                            &chunk.records,
                            loc.content_start,
                            loc.next_record_start,
                        ) {
                            Ok(v) => v,
                            Err(_) => break,
                        };
                        self.update_channel_stats(hdr.channel_id, hdr.log_time);
                    }
                }
            }
            Opcode::Attachment => {
                let start = self.sink.position();
                let length = (crate::format::RECORD_HEADER_SIZE as u64)
                    .checked_add(payload.len() as u64)
                    .ok_or_else(|| {
                        Error::InvalidRecord("attachment length overflow".to_string())
                    })?;

                let (log_time, create_time, name, media_type, data_size) =
                    parse_attachment_index_fields(payload.as_ref())?;
                self.attachment_indexes.push(AttachmentIndexInfo {
                    offset: start,
                    length,
                    log_time,
                    create_time,
                    data_size,
                    name: name.to_compact(),
                    media_type: media_type.to_compact(),
                });
            }
            Opcode::Metadata => {
                let start = self.sink.position();
                let length = (crate::format::RECORD_HEADER_SIZE as u64)
                    .checked_add(payload.len() as u64)
                    .ok_or_else(|| Error::InvalidRecord("metadata length overflow".to_string()))?;
                let name = parse_metadata_name(payload.as_ref())?;
                self.metadata_indexes.push(MetadataIndexInfo {
                    offset: start,
                    length,
                    name: name.to_compact(),
                });
            }
            _ => {}
        }

        // For Chunk, we already captured the start offset. For other opcodes, we don't need the
        // offset (yet) for summary bookkeeping.
        let payload_bytes = payload.as_ref();
        self.write_record_with_optional_crc(opcode, payload_bytes, None)?;
        Ok(())
    }

    pub(crate) fn write_raw_record_bytes_internal(&mut self, record: Bytes) -> Result<()> {
        use crate::records::decode_record_header;

        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write record after finish".to_string(),
            ));
        }

        if record.len() < crate::format::RECORD_HEADER_SIZE {
            return Err(Error::InvalidRecord(
                "raw record is smaller than record header".to_string(),
            ));
        }

        let (opcode, length) =
            decode_record_header(&record.as_ref()[..crate::format::RECORD_HEADER_SIZE])?;
        let expected_len = (crate::format::RECORD_HEADER_SIZE as u64)
            .checked_add(length)
            .ok_or_else(|| Error::InvalidRecord("record length overflow".to_string()))?;
        let actual_len: u64 = record
            .len()
            .try_into()
            .map_err(|_| Error::InvalidRecord("record length exceeds u64".to_string()))?;
        if expected_len != actual_len {
            return Err(Error::InvalidRecord(format!(
                "raw record length mismatch: header says {expected_len}, buffer has {actual_len}"
            )));
        }

        // Ensure header is written and any buffered chunk is flushed before copying raw records.
        self.write_header()?;
        self.flush_overrides_if_needed_all(false)?;
        self.flush_chunk_if_needed(false)?;

        let start = self.sink.position();
        let payload = record.slice(crate::format::RECORD_HEADER_SIZE..);

        match opcode {
            Opcode::Schema => {
                let mut schema = crate::parser::parse_schema_record(payload.clone())?;
                schema.name = schema.name.to_compact();
                schema.encoding = schema.encoding.to_compact();
                schema.data = Bytes::copy_from_slice(schema.data.as_ref());
                self.next_schema_id = self.next_schema_id.max(schema.id.saturating_add(1));
                self.schemas.insert(schema.id, schema);
            }
            Opcode::Channel => {
                let mut channel = crate::parser::parse_channel_record(payload.clone())?;
                channel.topic = channel.topic.to_compact();
                channel.message_encoding = channel.message_encoding.to_compact();
                channel.metadata = channel
                    .metadata
                    .into_iter()
                    .map(|(k, v)| (k.to_compact(), v.to_compact()))
                    .collect();
                self.next_channel_id = self.next_channel_id.max(channel.id.saturating_add(1));
                self.channels.insert(channel.id, channel);
            }
            Opcode::Message => {
                let header = crate::parser::parse_message_header_from_content(payload.as_ref())?;
                self.update_channel_stats(header.channel_id, header.log_time);
            }
            Opcode::Chunk => {
                let chunk = crate::parser::parse_chunk_record(payload.clone())?;
                let chunk_len = (crate::format::RECORD_HEADER_SIZE as u64)
                    .checked_add(payload.len() as u64)
                    .ok_or_else(|| Error::InvalidRecord("chunk length overflow".to_string()))?;
                let compressed_size = u64::try_from(chunk.records.len()).map_err(|_| {
                    Error::InvalidRecord("chunk compressed size exceeds u64".to_string())
                })?;
                self.chunk_indexes.push(ChunkIndexInfo {
                    message_start_time: chunk.message_start_time,
                    message_end_time: chunk.message_end_time,
                    chunk_start_offset: start,
                    chunk_length: chunk_len,
                    compression: chunk.compression.to_compact(),
                    compressed_size,
                    uncompressed_size: chunk.uncompressed_size,
                });

                // Best-effort message statistics without decompression:
                // if the chunk is uncompressed, scan its record stream and count messages.
                if chunk.compression.as_str().is_empty() || chunk.compression.as_str() == "none" {
                    for loc in crate::records::RecordIterator::new(chunk.records.as_ref(), 0) {
                        let loc = match loc {
                            Ok(l) => l,
                            Err(_) => break,
                        };
                        if loc.header.opcode != Opcode::Message.as_u8() {
                            continue;
                        }
                        let (hdr, _payload) = match crate::parser::parse_message_from_backing_range(
                            &chunk.records,
                            loc.content_start,
                            loc.next_record_start,
                        ) {
                            Ok(v) => v,
                            Err(_) => break,
                        };
                        self.update_channel_stats(hdr.channel_id, hdr.log_time);
                    }
                }
            }
            Opcode::Attachment => {
                let length = (crate::format::RECORD_HEADER_SIZE as u64)
                    .checked_add(payload.len() as u64)
                    .ok_or_else(|| {
                        Error::InvalidRecord("attachment length overflow".to_string())
                    })?;

                let (log_time, create_time, name, media_type, data_size) =
                    parse_attachment_index_fields(payload.as_ref())?;
                self.attachment_indexes.push(AttachmentIndexInfo {
                    offset: start,
                    length,
                    log_time,
                    create_time,
                    data_size,
                    name: name.to_compact(),
                    media_type: media_type.to_compact(),
                });
            }
            Opcode::Metadata => {
                let length = (crate::format::RECORD_HEADER_SIZE as u64)
                    .checked_add(payload.len() as u64)
                    .ok_or_else(|| Error::InvalidRecord("metadata length overflow".to_string()))?;
                let name = parse_metadata_name(payload.as_ref())?;
                self.metadata_indexes.push(MetadataIndexInfo {
                    offset: start,
                    length,
                    name: name.to_compact(),
                });
            }
            _ => {}
        }

        self.sink.write_all(record.as_ref())?;
        Ok(())
    }

    fn write_header(&mut self) -> Result<()> {
        if self.wrote_header {
            return Ok(());
        }
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write header after finish".to_string(),
            ));
        }

        let mut payload = Vec::new();
        encode::push_len_prefixed_str(&mut payload, &self.header.profile)?;
        encode::push_len_prefixed_str(&mut payload, &self.header.library)?;
        encode::push_metadata_map(&mut payload, &self.header.metadata)?;

        encode::write_magic(&mut self.sink)?;
        encode::write_record(&mut self.sink, Opcode::Header, &payload)?;
        self.wrote_header = true;
        Ok(())
    }

    pub(crate) fn write_schema_internal(&mut self, schema: &Schema) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write schema after finish".to_string(),
            ));
        }
        if schema.id == 0 {
            return Err(Error::InvalidRecord(
                "schema ID 0 is invalid (use 0 in channel.schema_id to indicate no schema)"
                    .to_string(),
            ));
        }
        self.write_header()?;
        self.flush_overrides_if_needed_all(false)?;
        self.flush_chunk_if_needed(false)?;

        let mut schema = schema.clone();
        schema.name = schema.name.to_compact();
        schema.encoding = schema.encoding.to_compact();
        schema.data = Bytes::copy_from_slice(schema.data.as_ref());

        self.next_schema_id = self.next_schema_id.max(schema.id.saturating_add(1));
        self.schemas.insert(schema.id, schema.clone());
        self.schema_ids_by_key.insert(
            SchemaKey {
                name: schema.name.clone(),
                encoding: schema.encoding.clone(),
                data: schema.data.clone(),
            },
            schema.id,
        );

        let payload = Self::encode_schema_payload(&schema)?;
        encode::write_record(&mut self.sink, Opcode::Schema, &payload)?;
        Ok(())
    }

    pub(crate) fn write_channel_internal(&mut self, channel: &Channel) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write channel after finish".to_string(),
            ));
        }
        self.write_header()?;
        self.flush_overrides_if_needed_all(false)?;
        self.flush_chunk_if_needed(false)?;

        let mut channel = channel.clone();
        channel.topic = channel.topic.to_compact();
        channel.message_encoding = channel.message_encoding.to_compact();
        channel.metadata = channel
            .metadata
            .into_iter()
            .map(|(k, v)| (k.to_compact(), v.to_compact()))
            .collect();

        self.next_channel_id = self.next_channel_id.max(channel.id.saturating_add(1));
        self.channels.insert(channel.id, channel.clone());

        let payload = Self::encode_channel_payload(&channel)?;
        encode::write_record(&mut self.sink, Opcode::Channel, &payload)?;
        Ok(())
    }

    pub(crate) fn write_raw_message_default(&mut self, message: &RawMessage) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write message after finish".to_string(),
            ));
        }
        self.write_header()?;
        self.update_channel_stats(message.channel_id, message.log_time);

        if let Some(state) = &mut self.chunk_state {
            state.push_message(
                message.channel_id,
                message.sequence,
                message.log_time,
                message.publish_time,
                message.data_bytes(),
            );
            // Keep override streams interleaved with default flushes; cheap when none registered (zero-iter loop).
            self.flush_overrides_if_needed_all(false)?;
            self.flush_chunk_if_needed(false)?;
            return Ok(());
        }

        let payload_len = (crate::format::MESSAGE_HEADER_SIZE + message.data_len()) as u64;
        let mut prefix = [0u8; MESSAGE_RECORD_PREFIX_LEN];
        prefix[0] = Opcode::Message.as_u8();
        prefix[1..9].copy_from_slice(&payload_len.to_le_bytes());
        prefix[9..11].copy_from_slice(&message.channel_id.to_le_bytes());
        prefix[11..15].copy_from_slice(&message.sequence.to_le_bytes());
        prefix[15..23].copy_from_slice(&message.log_time.to_le_bytes());
        prefix[23..31].copy_from_slice(&message.publish_time.to_le_bytes());

        write_all_vectored2(&mut self.sink, &prefix, message.data())?;
        Ok(())
    }

    pub(crate) fn write_raw_message_override(&mut self, message: &RawMessage) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write message after finish".to_string(),
            ));
        }
        self.write_header()?;
        self.update_channel_stats(message.channel_id, message.log_time);

        let idx = message.channel_id as usize;
        debug_assert!(
            idx < self.override_streams.len() && self.override_streams[idx].is_some(),
            "write_raw_message_override called for channel {} without registered override stream",
            message.channel_id,
        );

        {
            let state = self.override_streams[idx].as_mut().unwrap_or_else(|| {
                panic!(
                    "override stream must be registered for channel_id {}",
                    message.channel_id,
                )
            });
            state.push_message(
                message.channel_id,
                message.sequence,
                message.log_time,
                message.publish_time,
                message.data_bytes(),
            );
        }

        self.flush_override_if_needed(idx, false)?;
        Ok(())
    }

    pub(crate) fn write_message_record_internal(
        &mut self,
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        payload: &[u8],
    ) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write message after finish".to_string(),
            ));
        }
        self.write_header()?;
        self.flush_overrides_if_needed_all(false)?;
        self.flush_chunk_if_needed(false)?;
        self.update_channel_stats(channel_id, log_time);

        let payload_len = (crate::format::MESSAGE_HEADER_SIZE + payload.len()) as u64;
        let mut prefix = [0u8; MESSAGE_RECORD_PREFIX_LEN];
        prefix[0] = Opcode::Message.as_u8();
        prefix[1..9].copy_from_slice(&payload_len.to_le_bytes());
        prefix[9..11].copy_from_slice(&channel_id.to_le_bytes());
        prefix[11..15].copy_from_slice(&sequence.to_le_bytes());
        prefix[15..23].copy_from_slice(&log_time.to_le_bytes());
        prefix[23..31].copy_from_slice(&publish_time.to_le_bytes());
        write_all_vectored2(&mut self.sink, &prefix, payload)?;
        Ok(())
    }

    pub(crate) fn write_chunk_record_internal(&mut self, chunk: &Chunk) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write chunk after finish".to_string(),
            ));
        }
        self.write_header()?;
        self.flush_overrides_if_needed_all(false)?;
        self.flush_chunk_if_needed(false)?;

        let start = self.sink.position();

        let compression_len: u32 = chunk.compression.as_ref().len().try_into().map_err(|_| {
            Error::InvalidRecord("chunk compression length exceeds u32".to_string())
        })?;

        let records_len_u64: u64 =
            chunk.records.len().try_into().map_err(|_| {
                Error::InvalidRecord("chunk compressed length exceeds u64".to_string())
            })?;

        // Chunk payload excludes the record header.
        let payload_fixed_len = 8 + 8 + 8 + 4 + 4 + (compression_len as usize) + 8;
        let payload_len = payload_fixed_len
            .checked_add(chunk.records.len())
            .ok_or_else(|| Error::InvalidRecord("chunk length overflow".to_string()))?;

        let chunk_len = (crate::format::RECORD_HEADER_SIZE as u64)
            .checked_add(payload_len as u64)
            .ok_or_else(|| Error::InvalidRecord("chunk length overflow".to_string()))?;

        self.chunk_indexes.push(ChunkIndexInfo {
            message_start_time: chunk.message_start_time,
            message_end_time: chunk.message_end_time,
            chunk_start_offset: start,
            chunk_length: chunk_len,
            compression: chunk.compression.to_compact(),
            compressed_size: records_len_u64,
            uncompressed_size: chunk.uncompressed_size,
        });

        // Best-effort message statistics without decompression:
        // if the chunk is uncompressed, scan its record stream and count messages.
        let compression = chunk.compression.as_str();
        if compression.is_empty() || compression == "none" {
            for loc in crate::records::RecordIterator::new(chunk.records.as_ref(), 0) {
                let loc = match loc {
                    Ok(l) => l,
                    Err(_) => break,
                };
                if loc.header.opcode != Opcode::Message.as_u8() {
                    continue;
                }
                let (hdr, _payload) = match crate::parser::parse_message_from_backing_range(
                    &chunk.records,
                    loc.content_start,
                    loc.next_record_start,
                ) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                self.update_channel_stats(hdr.channel_id, hdr.log_time);
            }
        }

        let mut fixed = Vec::with_capacity(crate::format::RECORD_HEADER_SIZE + payload_fixed_len);
        encode::push_record_header(&mut fixed, Opcode::Chunk, payload_len as u64);
        encode::push_le_u64(&mut fixed, chunk.message_start_time);
        encode::push_le_u64(&mut fixed, chunk.message_end_time);
        encode::push_le_u64(&mut fixed, chunk.uncompressed_size);
        encode::push_le_u32(&mut fixed, chunk.uncompressed_crc);
        encode::push_len_prefixed_str(&mut fixed, &chunk.compression)?;
        encode::push_le_u64(&mut fixed, records_len_u64);

        write_all_vectored2(&mut self.sink, &fixed, chunk.records.as_ref())?;
        Ok(())
    }

    fn write_summary(&mut self) -> Result<(u64, u64, u32)> {
        let summary_start = self.sink.position();
        let mut crc = crc32fast::Hasher::new();
        let mut groups: Vec<SummaryGroup> = Vec::new();

        let schema_group_start = self.sink.position();
        let schemas: Vec<Schema> = self.schemas.values().cloned().collect();
        for schema in schemas {
            let payload = Self::encode_schema_payload(&schema)?;
            self.write_record_with_optional_crc(Opcode::Schema, &payload, Some(&mut crc))?;
        }
        let schema_group_end = self.sink.position();
        if schema_group_end != schema_group_start {
            groups.push(SummaryGroup {
                opcode: Opcode::Schema,
                start: schema_group_start,
                length: schema_group_end - schema_group_start,
            });
        }

        let channel_group_start = self.sink.position();
        let channels: Vec<Channel> = self.channels.values().cloned().collect();
        for channel in channels {
            let payload = Self::encode_channel_payload(&channel)?;
            self.write_record_with_optional_crc(Opcode::Channel, &payload, Some(&mut crc))?;
        }
        let channel_group_end = self.sink.position();
        if channel_group_end != channel_group_start {
            groups.push(SummaryGroup {
                opcode: Opcode::Channel,
                start: channel_group_start,
                length: channel_group_end - channel_group_start,
            });
        }

        let stats_group_start = self.sink.position();
        self.write_statistics_record(&mut crc)?;
        let stats_group_end = self.sink.position();
        if stats_group_end != stats_group_start {
            groups.push(SummaryGroup {
                opcode: Opcode::Statistics,
                start: stats_group_start,
                length: stats_group_end - stats_group_start,
            });
        }

        let chunk_index_group_start = self.sink.position();
        let chunk_indexes: Vec<ChunkIndexInfo> = self.chunk_indexes.clone();
        for info in &chunk_indexes {
            self.write_chunk_index_record(info, &mut crc)?;
        }
        let chunk_index_group_end = self.sink.position();
        if chunk_index_group_end != chunk_index_group_start {
            groups.push(SummaryGroup {
                opcode: Opcode::ChunkIndex,
                start: chunk_index_group_start,
                length: chunk_index_group_end - chunk_index_group_start,
            });
        }

        let attachment_index_group_start = self.sink.position();
        let attachment_indexes: Vec<AttachmentIndexInfo> = self.attachment_indexes.clone();
        for info in &attachment_indexes {
            self.write_attachment_index_record(info, &mut crc)?;
        }
        let attachment_index_group_end = self.sink.position();
        if attachment_index_group_end != attachment_index_group_start {
            groups.push(SummaryGroup {
                opcode: Opcode::AttachmentIndex,
                start: attachment_index_group_start,
                length: attachment_index_group_end - attachment_index_group_start,
            });
        }

        let metadata_index_group_start = self.sink.position();
        let metadata_indexes: Vec<MetadataIndexInfo> = self.metadata_indexes.clone();
        for info in &metadata_indexes {
            self.write_metadata_index_record(info, &mut crc)?;
        }
        let metadata_index_group_end = self.sink.position();
        if metadata_index_group_end != metadata_index_group_start {
            groups.push(SummaryGroup {
                opcode: Opcode::MetadataIndex,
                start: metadata_index_group_start,
                length: metadata_index_group_end - metadata_index_group_start,
            });
        }

        let summary_offset_start = self.sink.position();
        for group in groups {
            let mut payload = Vec::with_capacity(1 + 8 + 8);
            payload.push(group.opcode.as_u8());
            payload.extend_from_slice(&group.start.to_le_bytes());
            payload.extend_from_slice(&group.length.to_le_bytes());
            self.write_record_with_optional_crc(Opcode::SummaryOffset, &payload, None)?;
        }

        let summary_crc = crc.finalize();
        Ok((summary_start, summary_offset_start, summary_crc))
    }

    pub(crate) fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        self.write_header()?;
        for idx in 0..self.override_streams.len() {
            self.flush_override_if_needed(idx, true)?;
        }
        self.flush_chunk_if_needed(true)?;

        let data_end = encode::encode_data_end(0);
        encode::write_record(&mut self.sink, Opcode::DataEnd, &data_end)?;

        let should_write = self.always_write_summary
            || should_write_summary(
                &self.schemas,
                &self.channels,
                &self.chunk_indexes,
                &self.attachment_indexes,
                &self.metadata_indexes,
                &self.channel_stats,
            );

        let (summary_start, summary_offset_start, summary_crc) = if should_write {
            self.write_summary()?
        } else {
            (0, 0, 0)
        };

        let footer = encode::encode_footer(summary_start, summary_offset_start, summary_crc);
        encode::write_record(&mut self.sink, Opcode::Footer, &footer)?;
        encode::write_magic(&mut self.sink)?;

        self.sink.flush()?;
        self.finished = true;
        Ok(())
    }
}

fn parse_metadata_name(payload: &[u8]) -> Result<ByteStr> {
    let (name, _rest) = parse_len_prefixed_str(payload)
        .ok_or_else(|| Error::InvalidRecord("invalid metadata record".to_string()))?;
    Ok(name)
}

fn parse_attachment_index_fields(payload: &[u8]) -> Result<(u64, u64, ByteStr, ByteStr, u64)> {
    let (log_time, rest) = parse_le_u64(payload)
        .ok_or_else(|| Error::InvalidRecord("invalid attachment record".to_string()))?;
    let (create_time, rest) = parse_le_u64(rest)
        .ok_or_else(|| Error::InvalidRecord("invalid attachment record".to_string()))?;
    let (name, rest) = parse_len_prefixed_str(rest)
        .ok_or_else(|| Error::InvalidRecord("invalid attachment record".to_string()))?;
    let (media_type, rest) = parse_len_prefixed_str(rest)
        .ok_or_else(|| Error::InvalidRecord("invalid attachment record".to_string()))?;
    let (data_size, rest) = parse_le_u64(rest)
        .ok_or_else(|| Error::InvalidRecord("invalid attachment record".to_string()))?;

    let data_size_usize: usize = data_size.try_into().map_err(|_| {
        Error::InvalidRecord("attachment data length exceeds addressable memory".to_string())
    })?;
    if rest.len() < data_size_usize.saturating_add(4) {
        return Err(Error::InvalidRecord(
            "invalid attachment record".to_string(),
        ));
    }

    Ok((log_time, create_time, name, media_type, data_size))
}

fn parse_le_u32(input: &[u8]) -> Option<(u32, &[u8])> {
    let bytes = input.get(..4)?;
    Some((u32::from_le_bytes(bytes.try_into().ok()?), &input[4..]))
}

fn parse_le_u64(input: &[u8]) -> Option<(u64, &[u8])> {
    let bytes = input.get(..8)?;
    Some((u64::from_le_bytes(bytes.try_into().ok()?), &input[8..]))
}

fn parse_len_prefixed_str(input: &[u8]) -> Option<(ByteStr, &[u8])> {
    let (len, rest) = parse_le_u32(input)?;
    let len: usize = len.try_into().ok()?;
    let bytes = rest.get(..len)?;
    let s = std::str::from_utf8(bytes).ok()?;
    Some((ByteStr::from(s), &rest[len..]))
}
