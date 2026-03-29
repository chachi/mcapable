//! Zero-copy primitives for working with MCAP buffers.
//!
//! This module centralizes the building blocks for parsing and representing
//! byte-backed string/data slices without allocating new `String`/`Vec<u8>`.

use crate::support;
use crate::support::{Borrow, String};
use bytes::Bytes;

/// A UTF-8 string backed by a `bytes::Bytes` slice.
///
/// This is intended for *validated* UTF-8 data. Construction is either:
/// - validated (`from_utf8`), or
/// - unchecked (`from_utf8_unchecked`) for use after `nom` has already validated.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteStr(Bytes);

impl ByteStr {
    /// Construct a `ByteStr` by validating that `bytes` is UTF-8.
    pub fn from_utf8(bytes: Bytes) -> crate::Result<Self> {
        support::str::from_utf8(bytes.as_ref())
            .map_err(|e| crate::Error::InvalidRecord(support::format!("Invalid UTF-8: {e}")))?;
        Ok(Self(bytes))
    }

    /// Construct a `ByteStr` without validating UTF-8.
    ///
    /// # Safety
    /// The caller must ensure that `bytes` contains valid UTF-8.
    #[allow(dead_code)] // Used when constructing ByteStr from nom-validated UTF-8 spans.
    unsafe fn from_utf8_unchecked(bytes: Bytes) -> Self {
        Self(bytes)
    }

    /// Returns the underlying bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Returns the underlying `Bytes` slice.
    pub fn bytes(&self) -> &Bytes {
        &self.0
    }

    /// Returns this string as `&str`.
    ///
    /// This is `unsafe` internally because we assume UTF-8 validity was already
    /// checked at construction time.
    pub fn as_str(&self) -> &str {
        unsafe { support::str::from_utf8_unchecked(self.0.as_ref()) }
    }

    /// Returns a compact copy of this string, detaching it from any large backing buffer.
    ///
    /// This is useful when caching metadata parsed from a read-ahead buffer: small `ByteStr` fields
    /// should not keep an entire large buffer alive.
    pub fn to_compact(&self) -> Self {
        Self::from_utf8(Bytes::copy_from_slice(self.0.as_ref()))
            .expect("ByteStr must contain valid UTF-8")
    }
}

impl From<String> for ByteStr {
    fn from(value: String) -> Self {
        Self(Bytes::from(value))
    }
}

impl From<&str> for ByteStr {
    fn from(value: &str) -> Self {
        Self(Bytes::copy_from_slice(value.as_bytes()))
    }
}

impl PartialEq<&str> for ByteStr {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<str> for ByteStr {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<String> for ByteStr {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl support::ops::Deref for ByteStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for ByteStr {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for ByteStr {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl support::fmt::Display for ByteStr {
    fn fmt(&self, f: &mut support::fmt::Formatter<'_>) -> support::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `nom_locate` span type used for extracting offsets from byte buffers.
pub type Span<'a> = nom_locate::LocatedSpan<&'a [u8], &'a Bytes>;

/// Create a root parsing span over a `Bytes` buffer.
pub fn root_span(bytes: &Bytes) -> Span<'_> {
    Span::new_extra(bytes.as_ref(), bytes)
}

/// Convert a parsed span back into a `Bytes` slice from the original buffer.
pub fn bytes_from_span(span: Span<'_>) -> Bytes {
    let start = span.location_offset();
    let end = start + span.fragment().len();
    span.extra.slice(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_str_valid_utf8() {
        let s = ByteStr::from_utf8(Bytes::from_static(b"hello")).unwrap();
        assert_eq!(s.as_str(), "hello");
        assert_eq!(&*s, "hello");
        assert_eq!(s.as_bytes(), b"hello");
        assert_eq!(s.to_string(), "hello");
    }

    #[test]
    fn byte_str_rejects_invalid_utf8() {
        let bytes = Bytes::from_static(&[0xff, 0xfe]);
        let err = ByteStr::from_utf8(bytes).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidRecord(_)));
    }

    #[test]
    fn bytes_from_span_round_trip() {
        let root = Bytes::from_static(b"abcdef");
        let span = root_span(&root);
        let (_, taken) =
            nom::bytes::complete::take::<_, _, nom::error::Error<_>>(3usize)(span).unwrap();
        assert_eq!(bytes_from_span(taken), Bytes::from_static(b"abc"));
    }
}
