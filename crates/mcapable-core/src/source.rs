//! Data source abstraction for MCAP readers.
//!
//! This provides a minimal I/O surface area (seek + read-exact) that returns
//! `bytes::Bytes` so callers can build truly zero-copy pipelines when the
//! underlying data is already in memory.

use bytes::Bytes;
use polymock::Arena;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

/// A minimal seekable source that can return `Bytes` for read-exact operations.
///
/// Implementations may allocate (e.g. when backed by `Read + Seek`) or may be
/// fully zero-copy (e.g. when backed by in-memory `Bytes` via a cursor).
pub trait BytesSource {
    /// Read exactly `len` bytes from the current position and advance.
    fn read_exact_bytes(&mut self, len: usize) -> std::io::Result<Bytes>;

    /// Seek to a new position, like `std::io::Seek::seek`.
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64>;

    /// Current stream position.
    fn stream_position(&mut self) -> std::io::Result<u64> {
        self.seek(SeekFrom::Current(0))
    }
}

/// A [`BytesSource`] wrapper that tracks the current stream position in user space.
///
/// This avoids calling `seek(SeekFrom::Current(0))` for `stream_position()` on OS-backed
/// sources (which can be a syscall). The cached position is updated on every successful
/// `read_exact_bytes` and `seek`.
pub struct PositionTrackingSource<R: BytesSource> {
    inner: R,
    pos: u64,
}

impl<R: BytesSource> PositionTrackingSource<R> {
    /// Wrap `inner` with a known starting position.
    pub fn new(inner: R, starting_pos: u64) -> Self {
        Self {
            inner,
            pos: starting_pos,
        }
    }

    /// Consume the wrapper and return the inner source.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: BytesSource> BytesSource for PositionTrackingSource<R> {
    fn read_exact_bytes(&mut self, len: usize) -> std::io::Result<Bytes> {
        let out = self.inner.read_exact_bytes(len)?;
        self.pos = self.pos.saturating_add(len as u64);
        Ok(out)
    }

    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new = self.inner.seek(pos)?;
        self.pos = new;
        Ok(new)
    }

    fn stream_position(&mut self) -> std::io::Result<u64> {
        Ok(self.pos)
    }
}

/// A [`BytesSource`] wrapper that allocates read buffers from a bump arena.
///
/// This is intended for `Read + Seek` sources where per-read allocation is
/// costly. Allocations are fast bump allocations, and memory is reclaimed
/// when the arena is dropped.
pub struct ArenaBytesSource<R: Read + Seek> {
    inner: Option<R>,
    small: ArenaPool,
    large: ArenaPool,
}

struct ArenaPool {
    arena: Arc<Arena>,
    chunk_size: usize,
    bytes_since_reset: u64,
}

impl<R: Read + Seek> ArenaBytesSource<R> {
    const DEFAULT_SMALL_CHUNK_SIZE: usize = 8 * 1024;
    const DEFAULT_LARGE_CHUNK_SIZE: usize = 4 * 1024 * 1024;
    const DEFAULT_RESET_THRESHOLD_BYTES: u64 = 4 * 1024 * 1024;
    const DEFAULT_MAX_READ_EXACT_BYTES: usize = 512 * 1024 * 1024;

    /// Wrap a reader with a fresh arena.
    pub fn new(inner: R) -> Self {
        let small_chunk_size = Self::DEFAULT_SMALL_CHUNK_SIZE;
        let large_chunk_size = Self::DEFAULT_LARGE_CHUNK_SIZE;
        Self {
            inner: Some(inner),
            small: ArenaPool {
                arena: Arc::new(Arena::new(small_chunk_size)),
                chunk_size: small_chunk_size,
                bytes_since_reset: 0,
            },
            large: ArenaPool {
                arena: Arc::new(Arena::new(large_chunk_size)),
                chunk_size: large_chunk_size,
                bytes_since_reset: 0,
            },
        }
    }

    /// Wrap a reader with a configured "large" arena chunk size.
    pub fn with_chunk_size(inner: R, chunk_size: usize) -> Self {
        let small_chunk_size = Self::DEFAULT_SMALL_CHUNK_SIZE;
        Self {
            inner: Some(inner),
            small: ArenaPool {
                arena: Arc::new(Arena::new(small_chunk_size)),
                chunk_size: small_chunk_size,
                bytes_since_reset: 0,
            },
            large: ArenaPool {
                arena: Arc::new(Arena::new(chunk_size)),
                chunk_size,
                bytes_since_reset: 0,
            },
        }
    }

    /// Consume the wrapper and return the inner reader.
    pub fn into_inner(mut self) -> R {
        self.inner
            .take()
            .expect("inner must be present for into_inner()")
    }
}

impl<R: Read + Seek> BytesSource for ArenaBytesSource<R> {
    fn read_exact_bytes(&mut self, len: usize) -> std::io::Result<Bytes> {
        if len == 0 {
            return Ok(Bytes::new());
        }
        // Avoid allocating absurdly large buffers for corrupted record lengths.
        if len > Self::DEFAULT_MAX_READ_EXACT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "read_exact_bytes length too large",
            ));
        }
        enum PoolKind {
            Small,
            Large,
            OneOff,
        }

        fn maybe_reset_pool(reset_threshold_bytes: u64, pool: &mut ArenaPool) {
            if pool.bytes_since_reset < reset_threshold_bytes {
                return;
            }
            let strong = Arc::strong_count(&pool.arena);
            // If no outstanding `Bytes` slices reference the current arena, we can drop it and
            // start fresh. This prevents unbounded memory growth from bump allocations.
            if strong == 1 {
                pool.arena = Arc::new(Arena::new(pool.chunk_size));
                pool.bytes_since_reset = 0;
            }
        }

        let (kind, arena) = if len <= self.small.chunk_size {
            self.small.bytes_since_reset = self.small.bytes_since_reset.saturating_add(len as u64);
            maybe_reset_pool(Self::DEFAULT_RESET_THRESHOLD_BYTES, &mut self.small);
            (PoolKind::Small, Arc::clone(&self.small.arena))
        } else if len <= self.large.chunk_size {
            self.large.bytes_since_reset = self.large.bytes_since_reset.saturating_add(len as u64);
            maybe_reset_pool(Self::DEFAULT_RESET_THRESHOLD_BYTES, &mut self.large);
            (PoolKind::Large, Arc::clone(&self.large.arena))
        } else {
            (PoolKind::OneOff, Arc::new(Arena::new(len)))
        };
        let mut buf = arena.alloc(len);
        let mut filled = 0usize;
        while filled < len {
            let n = match self
                .inner
                .as_mut()
                .expect("inner must be present")
                .read(&mut buf[filled..])
            {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "failed to fill whole buffer",
                    ));
                }
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            filled += n;
        }
        let _ = kind; // left for potential future instrumentation
        let owner = ArenaBytesOwner {
            arena,
            bytes: buf.freeze(),
        };
        Ok(Bytes::from_owner(owner))
    }

    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        Seek::seek(self.inner.as_mut().expect("inner must be present"), pos)
    }
}

struct ArenaBytesOwner {
    #[allow(dead_code)] // Keeps the arena alive for the Bytes owner.
    arena: Arc<Arena>,
    bytes: polymock::Bytes,
}

impl AsRef<[u8]> for ArenaBytesOwner {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl<T: Read + Seek> BytesSource for T {
    fn read_exact_bytes(&mut self, len: usize) -> std::io::Result<Bytes> {
        if len == 0 {
            return Ok(Bytes::new());
        }

        // Avoid panicking on absurd lengths (e.g. corrupted record lengths) by using `try_reserve`.
        let mut buf = Vec::new();
        buf.try_reserve(len).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "read_exact_bytes length too large",
            )
        })?;
        buf.resize(len, 0u8);
        self.read_exact(&mut buf)?;
        Ok(Bytes::from(buf))
    }

    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        Seek::seek(self, pos)
    }
}

impl BytesSource for Box<dyn BytesSource> {
    fn read_exact_bytes(&mut self, len: usize) -> std::io::Result<Bytes> {
        (**self).read_exact_bytes(len)
    }

    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        (**self).seek(pos)
    }

    fn stream_position(&mut self) -> std::io::Result<u64> {
        (**self).stream_position()
    }
}

/// An in-memory `Bytes` cursor that implements [`BytesSource`] without copying.
#[derive(Debug, Clone)]
pub struct BytesCursor {
    bytes: Bytes,
    position: usize,
}

impl BytesCursor {
    /// Create a new cursor over an in-memory `Bytes` buffer.
    pub fn new(bytes: Bytes) -> Self {
        Self { bytes, position: 0 }
    }

    /// Consume the cursor and return the underlying buffer.
    pub fn into_inner(self) -> Bytes {
        self.bytes
    }
}

impl From<Bytes> for BytesCursor {
    fn from(bytes: Bytes) -> Self {
        Self::new(bytes)
    }
}

impl BytesSource for BytesCursor {
    fn read_exact_bytes(&mut self, len: usize) -> std::io::Result<Bytes> {
        let end = self
            .position
            .checked_add(len)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "len overflow"))?;
        if end > self.bytes.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "unexpected EOF",
            ));
        }
        let out = self.bytes.slice(self.position..end);
        self.position = end;
        Ok(out)
    }

    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let len: i64 =
            self.bytes.len().try_into().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "len overflow")
            })?;
        let cur: i64 = self
            .position
            .try_into()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "pos overflow"))?;

        let next: i64 = match pos {
            SeekFrom::Start(off) => off.try_into().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "pos overflow")
            })?,
            SeekFrom::End(off) => len.checked_add(off).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "pos overflow")
            })?,
            SeekFrom::Current(off) => cur.checked_add(off).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "pos overflow")
            })?,
        };

        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ));
        }
        let next_usize: usize = next
            .try_into()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "pos overflow"))?;
        if next_usize > self.bytes.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek beyond end",
            ));
        }

        self.position = next_usize;
        Ok(next as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bytes_source_blanket_impl_reads_bytes() {
        let mut cursor = Cursor::new(b"abcdef".to_vec());
        let out = cursor.read_exact_bytes(3).unwrap();
        assert_eq!(out.as_ref(), b"abc");

        std::io::Seek::seek(&mut cursor, SeekFrom::Start(4)).unwrap();
        let out = cursor.read_exact_bytes(2).unwrap();
        assert_eq!(out.as_ref(), b"ef");
    }

    #[test]
    fn bytes_cursor_slices_share_backing() {
        let root = Bytes::from_static(b"abcdef");
        let mut cursor = BytesCursor::new(root.clone());

        let a = cursor.read_exact_bytes(2).unwrap();
        let b = cursor.read_exact_bytes(2).unwrap();

        let base = root.as_ref().as_ptr() as usize;
        let end = base + root.len();
        let a_ptr = a.as_ref().as_ptr() as usize;
        let b_ptr = b.as_ref().as_ptr() as usize;
        assert!(a_ptr >= base && a_ptr < end);
        assert!(b_ptr >= base && b_ptr < end);
        assert_eq!(a.as_ref(), b"ab");
        assert_eq!(b.as_ref(), b"cd");
    }

    #[test]
    fn bytes_cursor_seek_and_read() {
        let root = Bytes::from_static(b"abcdef");
        let mut cursor = BytesCursor::new(root);

        BytesSource::seek(&mut cursor, SeekFrom::Start(3)).unwrap();
        let out = cursor.read_exact_bytes(2).unwrap();
        assert_eq!(out.as_ref(), b"de");
    }

    #[test]
    fn arena_bytes_source_reads_and_keeps_slices() {
        let cursor = Cursor::new(b"abcdef".to_vec());
        let mut source = ArenaBytesSource::new(cursor);

        let first = source.read_exact_bytes(3).unwrap();
        let second = source.read_exact_bytes(3).unwrap();

        assert_eq!(first.as_ref(), b"abc");
        assert_eq!(second.as_ref(), b"def");
    }

    #[test]
    fn arena_bytes_source_handles_large_reads() {
        let cursor = Cursor::new(b"abcdef".to_vec());
        let mut source = ArenaBytesSource::with_chunk_size(cursor, 2);

        let out = source.read_exact_bytes(6).unwrap();
        assert_eq!(out.as_ref(), b"abcdef");
    }
}
