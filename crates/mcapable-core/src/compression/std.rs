use super::Compression;
use crate::support::{ToString, Vec};
use crate::{Error, Result};
use bytes::Bytes;

/// Decompress data using the specified compression algorithm.
///
/// # Arguments
///
/// * `compression` - The compression algorithm to use
/// * `compressed` - The compressed data bytes
/// * `expected_size` - The expected size of decompressed data (used for LZ4)
///
/// # Returns
///
/// The decompressed data, or an error if decompression fails.
///
/// # Examples
///
/// ```no_run
/// use mcapable_core::compression::{Compression, decompress};
/// use bytes::Bytes;
///
/// # fn example() -> Result<(), mcapable_core::Error> {
/// let compressed = Bytes::from(vec![/* ... */]);
/// let decompressed = decompress(Some(&Compression::Lz4), compressed, 1024)?;
/// # Ok(())
/// # }
/// ```
#[cfg(all(feature = "lz4", feature = "zstd"))]
pub fn decompress(
    compression: Option<&Compression>,
    compressed: Bytes,
    expected_size: u64,
) -> Result<Bytes> {
    match compression {
        None => Ok(compressed),
        Some(Compression::Lz4) => Ok(Bytes::from(decompress_lz4(
            compressed.as_ref(),
            expected_size,
        )?)),
        Some(Compression::Zstd) => Ok(Bytes::from(decompress_zstd(
            compressed.as_ref(),
            expected_size,
        )?)),
    }
}

#[cfg(all(feature = "lz4", not(feature = "zstd")))]
pub fn decompress(
    compression: Option<&Compression>,
    compressed: Bytes,
    expected_size: u64,
) -> Result<Bytes> {
    match compression {
        None => Ok(compressed),
        Some(Compression::Lz4) => Ok(Bytes::from(decompress_lz4(
            compressed.as_ref(),
            expected_size,
        )?)),
    }
}

#[cfg(all(not(feature = "lz4"), feature = "zstd"))]
pub fn decompress(
    compression: Option<&Compression>,
    compressed: Bytes,
    expected_size: u64,
) -> Result<Bytes> {
    match compression {
        None => Ok(compressed),
        Some(Compression::Zstd) => Ok(Bytes::from(decompress_zstd(
            compressed.as_ref(),
            expected_size,
        )?)),
    }
}

#[cfg(all(not(feature = "lz4"), not(feature = "zstd")))]
pub fn decompress(
    _compression: Option<&Compression>,
    compressed: Bytes,
    _expected_size: u64,
) -> Result<Bytes> {
    Ok(compressed)
}

#[cfg(all(feature = "lz4", feature = "zstd"))]
pub(crate) fn compress_segments_to_vec<'a, I>(
    compression: &Compression,
    segments: I,
    total_len_hint: Option<usize>,
) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let capacity = total_len_hint.unwrap_or(0);
    match compression {
        Compression::Lz4 => compress_lz4_frame_segments(segments, capacity),
        Compression::Zstd => compress_zstd_segments(segments, capacity),
    }
}

#[cfg(all(feature = "lz4", not(feature = "zstd")))]
pub(crate) fn compress_segments_to_vec<'a, I>(
    compression: &Compression,
    segments: I,
    total_len_hint: Option<usize>,
) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let capacity = total_len_hint.unwrap_or(0);
    match compression {
        Compression::Lz4 => compress_lz4_frame_segments(segments, capacity),
    }
}

#[cfg(all(not(feature = "lz4"), feature = "zstd"))]
pub(crate) fn compress_segments_to_vec<'a, I>(
    compression: &Compression,
    segments: I,
    total_len_hint: Option<usize>,
) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let capacity = total_len_hint.unwrap_or(0);
    match compression {
        Compression::Zstd => compress_zstd_segments(segments, capacity),
    }
}

#[cfg(all(not(feature = "lz4"), not(feature = "zstd")))]
pub(crate) fn compress_segments_to_vec<'a, I>(
    compression: &Compression,
    _segments: I,
    _total_len_hint: Option<usize>,
) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    match *compression {}
}

pub(crate) fn compress_chunk_records(
    compression: &Compression,
    message_prefixes: &[u8],
    payloads: &[Bytes],
) -> Result<Vec<u8>> {
    let prefix_len = if payloads.is_empty() {
        0
    } else {
        message_prefixes.len() / payloads.len()
    };
    if prefix_len == 0 && !payloads.is_empty() {
        return Err(Error::InvalidRecord(
            "chunk record prefixes are missing".to_string(),
        ));
    }
    if prefix_len.saturating_mul(payloads.len()) != message_prefixes.len() {
        return Err(Error::InvalidRecord(
            "chunk record prefixes length is invalid".to_string(),
        ));
    }

    let total_payload: usize = payloads.iter().map(|p| p.len()).sum();
    compress_segments_to_vec(
        compression,
        ChunkRecordSegments::new(message_prefixes, payloads, prefix_len),
        Some(message_prefixes.len() + total_payload),
    )
}

#[cfg(feature = "zstd")]
fn compress_zstd_segments<'a, I>(segments: I, capacity: usize) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    zstd_impl::compress_segments(segments, capacity)
}

#[cfg(feature = "lz4")]
fn compress_lz4_frame_segments<'a, I>(segments: I, capacity: usize) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    lz4_impl::compress_segments(segments, capacity)
}

#[cfg(feature = "lz4")]
fn decompress_lz4(compressed: &[u8], expected_size: u64) -> Result<Vec<u8>> {
    lz4_impl::decompress(compressed, expected_size)
}

#[cfg(feature = "zstd")]
fn decompress_zstd(compressed: &[u8], expected_size: u64) -> Result<Vec<u8>> {
    zstd_impl::decompress(compressed, expected_size)
}

#[cfg(feature = "zstd")]
mod zstd_impl {
    use super::{Error, Result};
    use crate::support::Vec;
    use std::io::Write;

    pub fn compress_segments<'a, I>(segments: I, capacity: usize) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = &'a [u8]>,
    {
        let mut encoder = zstd::stream::write::Encoder::new(Vec::with_capacity(capacity), 0)
            .map_err(|e| Error::DecompressionError(format!("Zstd encoder init: {}", e)))?;
        for seg in segments {
            encoder
                .write_all(seg)
                .map_err(|e| Error::DecompressionError(format!("Zstd: {}", e)))?;
        }
        encoder
            .finish()
            .map_err(|e| Error::DecompressionError(format!("Zstd finish: {}", e)))
    }

    pub fn decompress(compressed: &[u8], _expected_size: u64) -> Result<Vec<u8>> {
        zstd::decode_all(compressed).map_err(|e| Error::DecompressionError(format!("Zstd: {}", e)))
    }
}

#[cfg(feature = "lz4")]
mod lz4_impl {
    use super::{Error, Result};
    use crate::support::Vec;
    use std::io::{Read, Write};

    pub fn compress_segments<'a, I>(segments: I, capacity: usize) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = &'a [u8]>,
    {
        let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::with_capacity(capacity));
        for seg in segments {
            encoder
                .write_all(seg)
                .map_err(|e| Error::DecompressionError(format!("LZ4 compression: {}", e)))?;
        }
        encoder
            .finish()
            .map_err(|e| Error::DecompressionError(format!("LZ4 compression finish: {}", e)))
    }

    pub fn decompress(compressed: &[u8], _expected_size: u64) -> Result<Vec<u8>> {
        let mut decoder = lz4_flex::frame::FrameDecoder::new(compressed);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .map_err(|e| Error::DecompressionError(format!("LZ4: {}", e)))?;
        Ok(decompressed)
    }
}

struct ChunkRecordSegments<'a> {
    message_prefixes: &'a [u8],
    payloads: &'a [Bytes],
    prefix_len: usize,
    index: usize,
    emit_prefix: bool,
}

impl<'a> ChunkRecordSegments<'a> {
    fn new(message_prefixes: &'a [u8], payloads: &'a [Bytes], prefix_len: usize) -> Self {
        Self {
            message_prefixes,
            payloads,
            prefix_len,
            index: 0,
            emit_prefix: true,
        }
    }
}

impl<'a> Iterator for ChunkRecordSegments<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.payloads.len() {
            return None;
        }

        if self.emit_prefix {
            let start = self.index * self.prefix_len;
            let end = start + self.prefix_len;
            self.emit_prefix = false;
            Some(&self.message_prefixes[start..end])
        } else {
            let out = self.payloads[self.index].as_ref();
            self.index += 1;
            self.emit_prefix = true;
            Some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompress_none() {
        let data = Bytes::from_static(b"hello world");
        let result = decompress(None, data.clone(), data.len() as u64).unwrap();
        assert_eq!(result, data);
    }
}

#[cfg(all(test, feature = "lz4"))]
mod lz4_tests {
    use super::*;

    #[test]
    fn test_decompress_lz4() {
        let original = b"hello world".repeat(100);
        let compressed = compress_segments_to_vec(
            &Compression::Lz4,
            std::iter::once(original.as_slice()),
            Some(original.len()),
        )
        .unwrap();

        let decompressed = decompress(
            Some(&Compression::Lz4),
            Bytes::from(compressed),
            original.len() as u64,
        )
        .unwrap();
        assert_eq!(decompressed.as_ref(), original.as_slice());
    }

    #[test]
    fn test_decompress_lz4_invalid() {
        let bad_data = Bytes::from_static(b"not compressed data");
        let result = decompress(Some(&Compression::Lz4), bad_data, 1000);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::DecompressionError(_)));
    }

    #[test]
    fn test_compress_round_trip_lz4() {
        let original = b"hello world".repeat(10);
        let compressed = compress_segments_to_vec(
            &Compression::Lz4,
            std::iter::once(original.as_slice()),
            None,
        )
        .unwrap();
        let decompressed = decompress(
            Some(&Compression::Lz4),
            Bytes::from(compressed),
            original.len() as u64,
        )
        .unwrap();
        assert_eq!(decompressed.as_ref(), original.as_slice());
    }
}

#[cfg(all(test, feature = "zstd"))]
mod zstd_tests {
    use super::*;

    #[test]
    fn test_decompress_zstd() {
        let original = b"hello world".repeat(100);
        let compressed = zstd::encode_all(&original[..], 3).unwrap();

        let decompressed = decompress(
            Some(&Compression::Zstd),
            Bytes::from(compressed),
            original.len() as u64,
        )
        .unwrap();
        assert_eq!(decompressed.as_ref(), original.as_slice());
    }

    #[test]
    fn test_decompress_zstd_invalid() {
        let bad_data = Bytes::from_static(b"not compressed data");
        let result = decompress(Some(&Compression::Zstd), bad_data, 1000);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::DecompressionError(_)));
    }

    #[test]
    fn test_compress_round_trip_zstd() {
        let original = b"hello world".repeat(10);
        let compressed = compress_segments_to_vec(
            &Compression::Zstd,
            std::iter::once(original.as_slice()),
            None,
        )
        .unwrap();
        let decompressed = decompress(
            Some(&Compression::Zstd),
            Bytes::from(compressed),
            original.len() as u64,
        )
        .unwrap();
        assert_eq!(decompressed.as_ref(), original.as_slice());
    }
}

#[cfg(all(test, feature = "lz4", feature = "zstd"))]
mod lz4_zstd_tests {
    use super::*;

    #[test]
    fn test_large_data_compression() {
        let large_data = b"A".repeat(10000);

        let lz4_compressed = compress_segments_to_vec(
            &Compression::Lz4,
            std::iter::once(large_data.as_slice()),
            None,
        )
        .unwrap();
        let lz4_compressed_len = lz4_compressed.len();
        let lz4_decompressed = decompress(
            Some(&Compression::Lz4),
            Bytes::from(lz4_compressed),
            large_data.len() as u64,
        )
        .unwrap();
        assert_eq!(lz4_decompressed.as_ref(), large_data.as_slice());
        assert!(lz4_compressed_len < large_data.len());

        let zstd_compressed = zstd::encode_all(&large_data[..], 3).unwrap();
        let zstd_compressed_len = zstd_compressed.len();
        let zstd_decompressed = decompress(
            Some(&Compression::Zstd),
            Bytes::from(zstd_compressed),
            large_data.len() as u64,
        )
        .unwrap();
        assert_eq!(zstd_decompressed.as_ref(), large_data.as_slice());
        assert!(zstd_compressed_len < large_data.len());
    }
}
