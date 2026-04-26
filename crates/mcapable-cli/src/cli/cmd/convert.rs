use std::path::PathBuf;

use super::{copy_raw_messages, parse_compression_option, rewrite_mcap};

pub fn run(
    input: Option<String>,
    output: Option<String>,
    compression: Option<String>,
    chunk_size: Option<usize>,
    include_crc: bool,
) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let output = PathBuf::from(output.unwrap_or_else(|| "-".to_string()));
    let compression = compression
        .as_deref()
        .map(parse_compression_option)
        .transpose()?
        .flatten();
    let chunk_size = chunk_size.unwrap_or(1048576);
    let unchunked = chunk_size == 0;

    let chunk_options = if unchunked {
        if compression.is_some() {
            return Err(
                "invalid flags: --unchunked requires --compression none (no chunk compression)"
                    .to_string(),
            );
        }
        None
    } else {
        Some(mcapable_core::writer::ChunkOptions {
            compression,
            max_uncompressed_bytes: chunk_size,
            include_crc,
        })
    };

    rewrite_mcap(
        input,
        output,
        chunk_options,
        |reader, writer, channel_writers| {
            copy_raw_messages(reader, writer, channel_writers, |_| true)
        },
    )
}
