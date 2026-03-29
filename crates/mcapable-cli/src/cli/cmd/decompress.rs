use std::path::PathBuf;

use super::rewrite_mcap_to_file;

pub(crate) fn run(
    input: Option<String>,
    output: Option<String>,
    chunk_size: usize,
) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let output = PathBuf::from(output.unwrap_or_else(|| "-".to_string()));
    rewrite_mcap_to_file(input, output, None, chunk_size)
}
