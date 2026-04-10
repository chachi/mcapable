use std::path::PathBuf;

use super::{parse_compression_option, rewrite_mcap_to_file, OutputOptions};

pub fn run(
    input: Option<String>,
    output: Option<String>,
    output_options: OutputOptions,
) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let output = PathBuf::from(output.unwrap_or_else(|| "-".to_string()));
    let compression = parse_compression_option(&output_options.compression)?;
    rewrite_mcap_to_file(
        input,
        output,
        compression,
        output_options.chunk_size,
        output_options.include_crc,
    )
}
