use std::path::PathBuf;

use super::rewrite_mcap_to_file;

pub(crate) fn run(
    input: Option<String>,
    output: Option<String>,
    compression: String,
    chunk_size: usize,
) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let output = PathBuf::from(output.unwrap_or_else(|| "-".to_string()));
    let compression = match compression.as_str() {
        "zstd" => Some(mcapable_core::Compression::Zstd),
        "lz4" => Some(mcapable_core::Compression::Lz4),
        "none" => None,
        _ => return Err(format!("unknown compression: {compression}")),
    };
    rewrite_mcap_to_file(input, output, compression, chunk_size)
}
