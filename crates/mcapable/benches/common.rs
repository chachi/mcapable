//! Shared benchmark utilities for generating MCAP inputs.
//!
//! Keep this focused on deterministic, reproducible input generation so we can
//! compare apples-to-apples across mcapable vs the official mcap crate.

use bytes::Bytes;

#[path = "../tests/helpers/mod.rs"]
mod helpers;

pub use helpers::mcap_builder::Compression;
use helpers::mcap_builder::McapBuilder;

#[derive(Clone, Debug)]
pub struct McapSpec {
    pub name: &'static str,
    pub channels: u16,
    pub messages: u32,
    pub payload_bytes: usize,
    pub chunked: bool,
    pub compression: Compression,
    pub chunk_size: Option<usize>,
}

impl McapSpec {
    #[allow(dead_code)] // Used by other benchmark binaries.
    pub fn compression_str(&self) -> &'static str {
        match self.compression {
            Compression::None => "none",
            Compression::Lz4 => "lz4",
            Compression::Zstd => "zstd",
        }
    }

    #[allow(dead_code)] // Used by other benchmark binaries.
    pub fn case_id(&self) -> String {
        let chunking = if self.chunked { "chunked" } else { "unchunked" };
        format!("{}/{chunking}-{}", self.name, self.compression_str())
    }

    pub fn estimated_payload_bytes(&self) -> u64 {
        u64::from(self.messages).saturating_mul(self.payload_bytes as u64)
    }
}

#[allow(dead_code)] // Used by some benchmark binaries.
pub fn default_specs() -> Vec<McapSpec> {
    let base = [
        ("small", 5u16, 500u32, 64usize),
        ("medium", 10u16, 5_000u32, 128usize),
        ("large", 20u16, 20_000u32, 256usize),
    ];

    let mut out = Vec::new();
    for (name, channels, messages, payload_bytes) in base {
        out.push(McapSpec {
            name,
            channels,
            messages,
            payload_bytes,
            chunked: true,
            compression: Compression::None,
            chunk_size: Some(4 * 1024 * 1024),
        });
        out.push(McapSpec {
            name,
            channels,
            messages,
            payload_bytes,
            chunked: true,
            compression: Compression::Zstd,
            chunk_size: Some(4 * 1024 * 1024),
        });
        out.push(McapSpec {
            name,
            channels,
            messages,
            payload_bytes,
            chunked: false,
            compression: Compression::None,
            chunk_size: None,
        });
    }
    out
}

#[allow(dead_code)] // Used by some benchmark binaries.
pub fn build_mcap_with_mcap_crate(spec: &McapSpec) -> Bytes {
    let mut builder = McapBuilder::new()
        .chunked(spec.chunked)
        .chunk_size(spec.chunk_size)
        .compression(Some(spec.compression));

    for ch in 0..spec.channels {
        builder = builder.add_simple_channel(ch, &format!("/channel/{ch}"));
    }

    let payload = vec![0u8; spec.payload_bytes];
    for i in 0..spec.messages {
        let channel = (i % spec.channels as u32) as u16;
        builder = builder.add_simple_message(channel, i, 1_000 + (i as u64) * 10, payload.clone());
    }

    Bytes::from(builder.build())
}
