use crate::support::HashMap;
use std::cell::RefCell;
use std::io::{Seek, Write};
use std::rc::Rc;

use crate::error::Result;
use crate::types::Header;
use crate::zero_copy::ByteStr;

use super::api::{ChunkOptions, Validation, Writer};
use super::chunk::ChunkState;
use super::internal::WriterImpl;
use super::io::PositionTrackingSink;

/// Builder for constructing a [`Writer`].
#[derive(Debug, Clone)]
pub struct WriterBuilder {
    header: Header,
    validation: Validation,
    chunk_options: Option<ChunkOptions>,
    always_write_summary: bool,
}

impl Default for WriterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WriterBuilder {
    /// Create a new writer builder with default header fields.
    pub fn new() -> Self {
        Self {
            header: Header {
                profile: ByteStr::from(""),
                library: default_library_string(),
                metadata: HashMap::new(),
            },
            validation: Validation::Strict,
            chunk_options: None,
            always_write_summary: false,
        }
    }

    /// Set the header profile string.
    pub fn profile(mut self, profile: impl Into<ByteStr>) -> Self {
        self.header.profile = profile.into();
        self
    }

    /// Set the header library string.
    pub fn library(mut self, library: impl Into<ByteStr>) -> Self {
        self.header.library = library.into();
        self
    }

    /// Add a header metadata key/value entry.
    pub fn header_metadata(mut self, key: impl Into<ByteStr>, value: impl Into<ByteStr>) -> Self {
        self.header.metadata.insert(key.into(), value.into());
        self
    }

    /// Configure validation checks.
    pub fn validation(mut self, validation: Validation) -> Self {
        self.validation = validation;
        self
    }

    /// Enable writing messages into `Chunk` records instead of top-level `Message` records.
    pub fn chunked(mut self, options: ChunkOptions) -> Self {
        self.chunk_options = Some(options);
        self
    }

    /// Force writing a summary section, even if it would otherwise be empty.
    ///
    /// This is useful for tooling like `recover`/`reindex` where a footer with a valid summary is
    /// desired even for partially recovered inputs.
    pub fn always_write_summary(mut self, enabled: bool) -> Self {
        self.always_write_summary = enabled;
        self
    }

    /// Build a writer around an output sink.
    ///
    /// The sink must support seeking to allow writing a footer and (later) summary sections.
    pub fn build<W: Write + Seek>(self, sink: W) -> Result<Writer<W>> {
        Ok(Writer {
            inner: Rc::new(RefCell::new(self.build_impl(sink)?)),
        })
    }

    /// Build just the internal writer implementation (no Rc/RefCell wrapping).
    ///
    /// Used by the rolling writer to manage `WriterImpl` lifecycle directly.
    pub(crate) fn build_impl<W: Write + Seek>(self, sink: W) -> Result<WriterImpl<W>> {
        Ok(WriterImpl {
            sink: PositionTrackingSink::new(sink)?,
            header: self.header,
            wrote_header: false,
            finished: false,
            next_schema_id: 1,
            next_channel_id: 1,
            validation: self.validation,
            always_write_summary: self.always_write_summary,
            chunk_state: self.chunk_options.map(ChunkState::new),
            override_streams: Vec::new(),
            schemas: HashMap::new(),
            channels: HashMap::new(),
            channel_stats: Vec::new(),
            chunk_indexes: Vec::new(),
            attachment_indexes: Vec::new(),
            metadata_indexes: Vec::new(),
            schema_ids_by_key: HashMap::new(),
        })
    }
}

pub fn default_library_string() -> ByteStr {
    ByteStr::from(format!("mcapable {}", env!("CARGO_PKG_VERSION")))
}
