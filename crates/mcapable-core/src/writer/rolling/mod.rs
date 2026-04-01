//! Rolling writer for automatic MCAP file splitting.
//!
//! The rolling writer wraps the existing [`super::Writer`] and transparently splits
//! output across multiple MCAP files based on configurable triggers. Each output
//! file is a fully self-contained, valid MCAP file.
//!
//! # Example
//!
//! ```no_run
//! use mcapable_core::writer::*;
//! use mcapable_core::writer::rolling::*;
//! use std::time::Duration;
//!
//! let factory = SequentialFiles::new("./output", "recording");
//! let trigger = AnyTrigger::new()
//!     .or(MaxSize::new(50_000_000))
//!     .or(WallDuration::new(Duration::from_secs(60)));
//!
//! let mut rolling = RollingWriterBuilder::new(factory, trigger)
//!     .writer_builder(WriterBuilder::new().chunked(ChunkOptions::default()))
//!     .build()
//!     .unwrap();
//!
//! let mut ch = rolling.add_channel(
//!     ChannelSpec::new("/topic", "cdr")
//! ).unwrap();
//!
//! ch.write(1000, 1000, &b"hello"[..]).unwrap();
//! rolling.finish().unwrap();
//! ```

mod builder;
mod factory;
mod trigger;
mod types;
mod writer;

pub use builder::RollingWriterBuilder;
pub use factory::{FnSinkFactory, SequentialFiles, SinkFactory, TimestampFiles};
pub use trigger::{
    AnyTrigger, FnTrigger, LogDuration, MaxSize, MessageCount, SplitTrigger, WallDuration,
};
pub use types::{ClosedFileContext, SplitContext, TriggerState};
pub use writer::{RollingAttachmentWriter, RollingChannelWriter, RollingWriter};
