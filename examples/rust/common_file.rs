use std::io::Write;

use crate::common_bytes::sample_bytes;

pub fn write_sample_file() -> Result<tempfile::NamedTempFile, Box<dyn std::error::Error>> {
    let bytes = sample_bytes();
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(&bytes)?;
    Ok(file)
}
