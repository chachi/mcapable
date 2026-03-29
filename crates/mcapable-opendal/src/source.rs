use bytes::Bytes;
use mcapable_core::source::BytesSource;
use opendal::BlockingOperator;
use std::io::SeekFrom;

use crate::Result;
use crate::parse_uri_blocking;

/// A [`BytesSource`] backed by an OpenDAL object.
///
/// This uses range reads to satisfy `read_exact_bytes` without downloading the
/// entire object.
#[derive(Clone)]
pub struct OpendalBytesSource {
    op: BlockingOperator,
    path: String,
    len: u64,
    position: u64,
}

impl OpendalBytesSource {
    /// Create a new source for the given OpenDAL object.
    pub fn new(op: BlockingOperator, path: impl Into<String>) -> std::io::Result<Self> {
        let path = path.into();
        let meta = op
            .stat(&path)
            .map_err(|e| std::io::Error::other(format!("opendal stat failed: {e}")))?;
        let len = meta.content_length();
        Ok(Self {
            op,
            path,
            len,
            position: 0,
        })
    }

    /// Build a source from a URI like `s3://bucket/key` or `https://host/path`.
    ///
    /// Supported schemes: `s3`, `gcs`, `gs`, `http`, `https`.
    pub fn from_uri(uri: &str) -> Result<Self> {
        let parsed = parse_uri_blocking(uri)?;
        let source = Self::new(parsed.operator, parsed.path)
            .map_err(|err| opendal::Error::new(opendal::ErrorKind::Unexpected, err.to_string()))?;
        Ok(source)
    }

    /// Total object length in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Returns true if the object is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl BytesSource for OpendalBytesSource {
    fn read_exact_bytes(&mut self, len: usize) -> std::io::Result<Bytes> {
        let len_u64: u64 = len
            .try_into()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "len overflow"))?;
        let end = self
            .position
            .checked_add(len_u64)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "len overflow"))?;

        if end > self.len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "unexpected EOF",
            ));
        }

        let buf = self
            .op
            .read_with(&self.path)
            .range(self.position..end)
            .call()
            .map_err(|e| std::io::Error::other(format!("opendal read failed: {e}")))?;

        if buf.len() != len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "unexpected EOF",
            ));
        }

        self.position = end;
        Ok(buf.to_bytes())
    }

    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let len: i128 = self.len.into();
        let cur: i128 = self.position.into();

        let next: i128 = match pos {
            SeekFrom::Start(off) => off.into(),
            SeekFrom::End(off) => len.checked_add(off.into()).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "pos overflow")
            })?,
            SeekFrom::Current(off) => cur.checked_add(off.into()).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "pos overflow")
            })?,
        };

        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ));
        }
        let next_u64: u64 = next
            .try_into()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "pos overflow"))?;
        if next_u64 > self.len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek beyond end",
            ));
        }

        self.position = next_u64;
        Ok(next_u64)
    }
}
