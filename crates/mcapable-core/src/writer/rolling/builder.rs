use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Instant, SystemTime};

use crate::error::Result;
use crate::support::HashMap;
use crate::writer::WriterBuilder;

use super::factory::SinkFactory;
use super::trigger::SplitTrigger;
use super::types::SplitContext;
use super::writer::{RollingInner, RollingWriter, SplitCallback};

/// Builder for constructing a [`RollingWriter`].
///
/// The builder is generic over the concrete factory and trigger types. These are
/// type-erased (boxed) when [`build()`](RollingWriterBuilder::build) is called, so
/// [`RollingWriter`] and [`super::RollingChannelWriter`] are only generic over the
/// sink type `W`.
///
/// # Example
///
/// ```no_run
/// use mcapable_core::writer::*;
/// use mcapable_core::writer::rolling::*;
/// use std::time::Duration;
///
/// let factory = SequentialFiles::new("./output", "recording");
/// let trigger = AnyTrigger::new()
///     .or(MaxSize::new(50_000_000))
///     .or(WallDuration::new(Duration::from_secs(60)));
///
/// let mut writer = RollingWriterBuilder::new(factory, trigger)
///     .writer_builder(WriterBuilder::new().chunked(ChunkOptions::default()))
///     .on_split(|ctx| println!("Split to file {}", ctx.file_index))
///     .build()
///     .unwrap();
/// ```
pub struct RollingWriterBuilder<F: SinkFactory, T: SplitTrigger> {
    factory: F,
    trigger: T,
    writer_builder: WriterBuilder,
    on_split: Option<SplitCallback>,
}

impl<F: SinkFactory + 'static, T: SplitTrigger + 'static> RollingWriterBuilder<F, T> {
    /// Create a new rolling writer builder with the given sink factory and split trigger.
    pub fn new(factory: F, trigger: T) -> Self {
        Self {
            factory,
            trigger,
            writer_builder: WriterBuilder::new(),
            on_split: None,
        }
    }

    /// Set the writer builder used to configure each inner Writer.
    ///
    /// This controls profile, library, chunking, compression, validation,
    /// and other settings applied to every file in the sequence.
    pub fn writer_builder(mut self, builder: WriterBuilder) -> Self {
        self.writer_builder = builder;
        self
    }

    /// Register a callback invoked after each split.
    pub fn on_split(mut self, callback: impl FnMut(&SplitContext) + 'static) -> Self {
        self.on_split = Some(Box::new(callback));
        self
    }

    /// Build the rolling writer, creating the first file immediately.
    pub fn build(mut self) -> Result<RollingWriter<F::Sink>> {
        let initial_context = SplitContext {
            file_index: 0,
            trigger_name: None,
            split_time: SystemTime::now(),
            prev_log_time_range: None,
        };

        let sink = self
            .factory
            .create_sink(&initial_context)
            .map_err(crate::error::Error::Io)?;

        let writer_impl = self.writer_builder.clone().build_impl(sink)?;

        let inner = RollingInner {
            writer_impl,
            factory: Box::new(self.factory),
            trigger: Box::new(self.trigger),
            writer_builder: self.writer_builder,
            on_split: self.on_split,
            registered_schemas: Vec::new(),
            registered_channels: Vec::new(),
            registered_overrides: HashMap::new(),
            file_index: 0,
            file_message_count: 0,
            file_opened_at: Instant::now(),
            file_log_time_start: 0,
            file_log_time_end: 0,
            file_has_messages: false,
            finished: false,
        };

        Ok(RollingWriter {
            inner: Rc::new(RefCell::new(inner)),
        })
    }
}
