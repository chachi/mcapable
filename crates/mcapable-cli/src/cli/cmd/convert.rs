use std::path::PathBuf;

use super::{
    copy_attachments_and_metadata, copy_raw_messages, open_reader, preload_schemas_and_channels,
    write_schemas_and_channels, writer_from_header,
};

pub(crate) fn run(
    input: Option<String>,
    output: Option<String>,
    compression: Option<String>,
    chunk_size: Option<usize>,
) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let output = PathBuf::from(output.unwrap_or_else(|| "-".to_string()));
    let compression = compression
        .as_deref()
        .map(|s| match s {
            "zstd" => Ok(mcapable_core::Compression::Zstd),
            "lz4" => Ok(mcapable_core::Compression::Lz4),
            "none" => Ok(mcapable_core::Compression::Zstd), // This is a bit odd but matches original
            _ => Err(format!("unknown compression: {s}")),
        })
        .transpose()?;
    let chunk_size = chunk_size.unwrap_or(1048576);
    let unchunked = chunk_size == 0;

    let mut reader = open_reader(input)?;
    let header = reader.header().map_err(|e| e.to_string())?;

    preload_schemas_and_channels(&mut reader)?;
    let out = std::fs::File::create(&output)
        .map_err(|e| format!("failed to create {}: {e}", output.display()))?;

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
        })
    };

    let mut writer = writer_from_header(out, &header, chunk_options)?;

    let mut channel_writers = write_schemas_and_channels(&reader, &mut writer)?;
    copy_attachments_and_metadata(&mut reader, &mut writer)?;
    copy_raw_messages(&mut reader, &mut writer, &mut channel_writers, |_| true)?;

    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}
