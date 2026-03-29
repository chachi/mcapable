#![cfg_attr(not(feature = "std"), no_std)]
//! A reimplemented MCAP library with a lazy Reader/Stream architecture.
//!
//! The API separates ownership from iteration:
//! - `reader::Reader<R>` owns the data source and cached metadata.
//! - `stream::Stream<'a, T>` borrows from `Reader` to iterate records.
//!
//! # Feature Flags
//!
//! - `std` (default): Enables IO-based reader, stream, and writer APIs.
//! - `alloc`: Enables core types and parsing in alloc-only builds.
//!
//! In `no_std` + `alloc` builds (`default-features = false, features = ["alloc"]`),
//! the API surface is limited to alloc-friendly types and parsing helpers.
//!
//! # Modules
//!
//! - `reader`: IO-backed Reader and Builder.
//! - `stream`: Stream iterators and parsed stream helpers.
//! - `writer`: MCAP writer APIs.
//! - `types`: MCAP record and metadata types.
//! - `compression`: Compression helpers and CRC utilities.
//! - `source`: BytesSource adapters.
//! - `zero_copy`: Bytes/ByteStr utilities for zero-copy parsing.
//!
//! # Example
//!
//! ```no_run
//! use mcapable_core::reader;
//! use std::fs::File;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let file = File::open("data.mcap")?;
//! let mut reader = reader::Builder::new().build(file)?;
//!
//! for message in reader.messages() {
//!     let message = message?;
//!     println!("Message on channel {} at time {}",
//!              message.channel_id, message.log_time);
//! }
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod compression;
pub mod error;
pub mod format;
#[cfg(feature = "std")]
mod std;
mod support;
pub mod types;
pub mod zero_copy;

#[cfg(feature = "std")]
pub use compression::decompress;
pub use compression::{Compression, calculate_crc, parse_compression, verify_crc};
pub use error::{Error, ParseError, Result};
#[cfg(feature = "std")]
pub(crate) use std::{parser, records};
#[cfg(feature = "std")]
pub use std::{reader, source, stream, writer};
#[cfg(feature = "std")]
pub use stream::{ParsedStream, ParsedStreamBuilder, Stream};
pub use types::*;

#[cfg(test)]
mod parser_properties;
#[cfg(test)]
mod test_generators;
