//! MCAP reader/writer library built on a lazy Reader/Stream architecture.
//!
//! Most APIs live under module entrypoints:
//! - `reader`: IO-backed Reader and Builder.
//! - `stream`: Stream iterators and parsed stream helpers.
//! - `writer`: Writer APIs.
//! - `types`: MCAP record and metadata types.
//!
//! Common types are available at the crate root (`Error`, `Result`, `Compression`, `ByteStr`).

pub use mcapable_core::Compression;
pub use mcapable_core::zero_copy::ByteStr;
pub use mcapable_core::{Error, ParseError, Result};
pub use mcapable_core::{reader, stream, types, writer};

#[doc(hidden)]
pub use mcapable_core::reader::Reader;
#[doc(hidden)]
pub use mcapable_core::writer::{
    AttachmentWriter, ChannelSpec, ChannelWriter, ChunkOptions, SchemaSpec,
    Validation as WriterValidation, Writer, WriterBuilder,
};
#[doc(hidden)]
pub use mcapable_core::*;
#[doc(hidden)]
pub use mcapable_core::{ParsedStream, ParsedStreamBuilder, Stream};
