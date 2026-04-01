//! MCAP writing APIs.
//!
//! The writer module is intentionally incremental:
//! - Phase 1 writes valid unchunked MCAPs (header, schemas/channels, messages, footer).
//! - Later phases add chunking, compression, summary/index generation, and reindex support.
//!
//! The public API is designed to avoid panics on drop; use [`Writer::finish`]
//! to finalize the file when required.

#[path = "writer/api.rs"]
mod api;
#[path = "writer/builder.rs"]
mod builder;
#[path = "writer/chunk.rs"]
mod chunk;
#[path = "writer/constants.rs"]
mod constants;
#[path = "writer/encode.rs"]
mod encode;
#[path = "writer/internal.rs"]
mod internal;
#[path = "writer/io.rs"]
mod io;
#[path = "writer/types.rs"]
mod types;

#[path = "writer/rolling/mod.rs"]
pub mod rolling;

pub use api::{
    AttachmentWriter, ChannelSpec, ChannelWriter, ChunkOptions, IntoPayloadBytes, SchemaSpec,
    Validation, Writer,
};
pub use builder::{WriterBuilder, default_library_string};
