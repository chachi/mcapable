use std::cell::RefCell;
use std::io::{Seek, Write};
use std::rc::Rc;
use std::time::{Instant, SystemTime};

use bytes::Bytes;

use crate::error::{Error, Result};
use crate::support::HashMap;
use crate::types::{Channel, Metadata, Payload, RawMessage, Schema};
use crate::writer::WriterBuilder;
use crate::writer::api::{ChannelSpec, ChunkOptions, IntoPayloadBytes};
use crate::writer::internal::WriterImpl;
use crate::zero_copy::ByteStr;

use super::factory::SinkFactory;
use super::trigger::SplitTrigger;
use super::types::{ClosedFileContext, SplitContext, TriggerState};

pub(super) type SplitCallback = Box<dyn FnMut(&SplitContext)>;

/// Internal state shared via `Rc<RefCell<...>>` between the rolling writer
/// and all channel/attachment writers.
pub(crate) struct RollingInner<W: Write + Seek> {
    // Current writer implementation (replaced on each split)
    pub(crate) writer_impl: WriterImpl<W>,

    // Persist across splits (type-erased)
    pub(crate) factory: Box<dyn SinkFactory<Sink = W>>,
    pub(crate) trigger: Box<dyn SplitTrigger>,
    pub(crate) writer_builder: WriterBuilder,
    pub(crate) on_split: Option<SplitCallback>,

    // Schema/channel registry (stable across splits)
    pub(crate) registered_schemas: Vec<Schema>,
    pub(crate) registered_channels: Vec<Channel>,
    /// Per-channel override `ChunkOptions`, keyed by stable channel id.
    /// Re-applied to each fresh `WriterImpl` on split.
    pub(crate) registered_overrides: HashMap<u16, ChunkOptions>,

    // Per-file stats (reset on split)
    pub(crate) file_index: usize,
    pub(crate) file_message_count: u64,
    pub(crate) file_opened_at: Instant,
    pub(crate) file_log_time_start: u64,
    pub(crate) file_log_time_end: u64,
    pub(crate) file_has_messages: bool,
    pub(crate) finished: bool,
}

impl<W: Write + Seek> RollingInner<W> {
    fn estimated_file_size(&self) -> u64 {
        let on_disk = self.writer_impl.sink.position();
        let buffered = self
            .writer_impl
            .chunk_state
            .as_ref()
            .map(|s| s.uncompressed_len as u64)
            .unwrap_or(0);
        on_disk + buffered
    }

    fn update_stats(&mut self, log_time: u64) {
        self.file_message_count += 1;
        if !self.file_has_messages {
            self.file_log_time_start = log_time;
            self.file_log_time_end = log_time;
            self.file_has_messages = true;
        } else {
            self.file_log_time_start = self.file_log_time_start.min(log_time);
            self.file_log_time_end = self.file_log_time_end.max(log_time);
        }
    }

    fn trigger_state(&self, current_log_time: u64) -> TriggerState {
        let log_time_span = if self.file_has_messages {
            self.file_log_time_end
                .saturating_sub(self.file_log_time_start)
        } else {
            0
        };
        TriggerState {
            estimated_file_size: self.estimated_file_size(),
            message_count: self.file_message_count,
            wall_elapsed: self.file_opened_at.elapsed(),
            log_time_span,
            current_log_time,
        }
    }

    fn log_time_range(&self) -> Option<(u64, u64)> {
        if self.file_has_messages {
            Some((self.file_log_time_start, self.file_log_time_end))
        } else {
            None
        }
    }

    fn perform_split(&mut self) -> Result<ClosedFileContext> {
        // Finalize the current file
        self.writer_impl.finish()?;
        let file_size = self.writer_impl.sink.position();

        let closed = ClosedFileContext {
            file_index: self.file_index,
            file_size,
            message_count: self.file_message_count,
            log_time_range: self.log_time_range(),
        };

        let new_file_index = self.file_index + 1;
        let trigger_name = Some(self.trigger.name().to_string());

        let split_ctx = SplitContext {
            file_index: new_file_index,
            trigger_name,
            split_time: SystemTime::now(),
            prev_log_time_range: self.log_time_range(),
        };

        // Call the on_split callback
        if let Some(ref mut cb) = self.on_split {
            cb(&split_ctx);
        }

        // Create new sink and writer
        let sink = self.factory.create_sink(&split_ctx).map_err(Error::Io)?;
        self.writer_impl = self.writer_builder.clone().build_impl(sink)?;

        // Re-emit all schemas and channels into the new file
        for schema in &self.registered_schemas {
            self.writer_impl.write_schema_internal(schema)?;
        }
        for channel in &self.registered_channels {
            self.writer_impl.write_channel_internal(channel)?;
        }
        // Re-register per-channel overrides into the fresh WriterImpl.
        for (channel_id, opts) in &self.registered_overrides {
            self.writer_impl
                .register_channel_override(*channel_id, opts.clone());
        }

        // Reset per-file stats
        self.file_index = new_file_index;
        self.file_message_count = 0;
        self.file_opened_at = Instant::now();
        self.file_log_time_start = 0;
        self.file_log_time_end = 0;
        self.file_has_messages = false;
        self.trigger.reset();

        // Notify factory about the closed file
        self.factory.on_file_closed(&closed);

        Ok(closed)
    }

    fn maybe_split(&mut self, current_log_time: u64) -> Result<()> {
        let state = self.trigger_state(current_log_time);
        if self.trigger.should_split(&state) {
            self.perform_split()?;
        }
        Ok(())
    }

    fn write_message_internal(
        &mut self,
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        data: Bytes,
        has_chunk_override: bool,
    ) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot write to a finished rolling writer".to_string(),
            ));
        }

        // Check trigger BEFORE writing: split if the previous write pushed us
        // over the threshold. This avoids creating an empty trailing file when
        // the trigger fires on the last message.
        self.maybe_split(log_time)?;

        let msg = RawMessage::new(
            channel_id,
            sequence,
            log_time,
            publish_time,
            Payload::from_bytes(data),
        );
        if has_chunk_override {
            self.writer_impl.write_raw_message_override(&msg)?;
        } else {
            self.writer_impl.write_raw_message_default(&msg)?;
        }

        self.update_stats(log_time);
        Ok(())
    }

    fn add_channel_internal(&mut self, spec: ChannelSpec) -> Result<(u16, bool)> {
        if self.finished {
            return Err(Error::InvalidRecord(
                "cannot add channel to a finished rolling writer".to_string(),
            ));
        }

        // Capture the override before the spec is consumed by add_channel_spec.
        let override_opts = spec.chunk_override.clone();

        let (channel_id, has_override) = self.writer_impl.add_channel_spec(spec)?;

        // Snapshot the schema and channel for re-emission on future splits
        if let Some(channel) = self.writer_impl.channels.get(&channel_id) {
            let channel = channel.clone();
            if channel.schema_id != 0
                && let Some(schema) = self.writer_impl.schemas.get(&channel.schema_id)
                && !self.registered_schemas.iter().any(|s| s.id == schema.id)
            {
                self.registered_schemas.push(schema.clone());
            }
            self.registered_channels.push(channel);
        }

        if let Some(opts) = override_opts {
            self.registered_overrides.insert(channel_id, opts);
        }

        Ok((channel_id, has_override))
    }
}

/// A writer that automatically splits output across multiple MCAP files.
///
/// The rolling writer wraps the existing [`super::super::Writer`] and transparently handles
/// file rotation based on configurable triggers. Each output file is a fully
/// self-contained, valid MCAP file with its own header, schemas, channels,
/// summary, and footer.
///
/// Channel IDs are stable across files: a channel registered once will use
/// the same ID in every file.
pub struct RollingWriter<W: Write + Seek> {
    pub(crate) inner: Rc<RefCell<RollingInner<W>>>,
}

impl<W: Write + Seek> RollingWriter<W> {
    /// Register a channel and return a writer for it.
    ///
    /// The channel will be automatically re-registered in each new file on split.
    /// The returned [`RollingChannelWriter`] remains valid across file splits.
    pub fn add_channel(&mut self, spec: ChannelSpec) -> Result<RollingChannelWriter<W>> {
        let (channel_id, has_chunk_override) =
            self.inner.borrow_mut().add_channel_internal(spec)?;
        Ok(RollingChannelWriter {
            inner: Rc::clone(&self.inner),
            channel_id,
            next_sequence: 0,
            has_chunk_override,
        })
    }

    /// Create an attachment writer.
    ///
    /// Attachments are written to the current file only and are NOT re-emitted on split.
    pub fn attachment_writer(&mut self) -> RollingAttachmentWriter<W> {
        RollingAttachmentWriter {
            inner: Rc::clone(&self.inner),
        }
    }

    /// Write metadata to the current file.
    ///
    /// Metadata is written to the current file only and is NOT re-emitted on split.
    pub fn write_metadata(&mut self, metadata: &Metadata) -> Result<()> {
        self.inner
            .borrow_mut()
            .writer_impl
            .write_metadata_internal(metadata)
    }

    /// Force a split to a new file, regardless of trigger state.
    pub fn force_split(&mut self) -> Result<ClosedFileContext> {
        self.inner.borrow_mut().perform_split()
    }

    /// Finalize the current file and close the rolling writer.
    ///
    /// No more data can be written after this call.
    pub fn finish(&mut self) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        if inner.finished {
            return Ok(());
        }
        inner.writer_impl.finish()?;
        inner.finished = true;
        Ok(())
    }

    /// Returns the zero-based index of the current file.
    pub fn current_file_index(&self) -> usize {
        self.inner.borrow().file_index
    }

    /// Returns the number of messages written to the current file.
    pub fn current_file_message_count(&self) -> u64 {
        self.inner.borrow().file_message_count
    }
}

/// A per-channel writer that remains valid across file splits.
///
/// This type mirrors [`super::super::ChannelWriter`] but operates through the
/// rolling writer's shared state, so writes are transparently directed to
/// the correct file.
#[derive(Clone)]
pub struct RollingChannelWriter<W: Write + Seek> {
    inner: Rc<RefCell<RollingInner<W>>>,
    channel_id: u16,
    next_sequence: u32,
    has_chunk_override: bool,
}

impl<W: Write + Seek> RollingChannelWriter<W> {
    /// Returns the stable channel ID used across all files.
    pub fn channel_id(&self) -> u16 {
        self.channel_id
    }

    /// Set the starting sequence number (default is 0).
    pub fn starting_sequence(mut self, sequence: u32) -> Self {
        self.next_sequence = sequence;
        self
    }

    /// Write a message, automatically splitting to a new file if a trigger fires.
    ///
    /// The message is always written before the split check, so no data is lost.
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

    /// Write a message with a specific sequence number.
    pub fn write_with_sequence<D: IntoPayloadBytes>(
        &mut self,
        log_time: u64,
        publish_time: u64,
        data: D,
        sequence: u32,
    ) -> Result<()> {
        let bytes = data.into_payload_bytes();
        self.inner.borrow_mut().write_message_internal(
            self.channel_id,
            sequence,
            log_time,
            publish_time,
            bytes,
            self.has_chunk_override,
        )
    }
}

/// A helper for writing attachments through the rolling writer.
///
/// Attachments are written to the current file only and are NOT re-emitted on split.
#[derive(Clone)]
pub struct RollingAttachmentWriter<W: Write + Seek> {
    inner: Rc<RefCell<RollingInner<W>>>,
}

impl<W: Write + Seek> RollingAttachmentWriter<W> {
    /// Write an attachment to the current file.
    pub fn write(
        &mut self,
        log_time: u64,
        create_time: u64,
        name: ByteStr,
        media_type: ByteStr,
        data: Bytes,
    ) -> Result<()> {
        self.inner
            .borrow_mut()
            .writer_impl
            .write_attachment_internal(log_time, create_time, name, media_type, data)
    }
}
