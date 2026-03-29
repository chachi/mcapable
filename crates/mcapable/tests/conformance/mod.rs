//! MCAP conformance tests.
//!
//! These tests verify that mcapable conforms to the MCAP specification
//! and handles all required features correctly.
//!
//! Test categories:
//! - reader_basics: Basic reader functionality and lazy loading
//! - message_ordering: Message ordering across chunks
//! - filtering: Time, channel, and header-based filtering
//! - filtering_properties: Property tests for filtering correctness
//! - compression: LZ4, Zstd, and uncompressed handling
//! - chunks: Chunk iteration and decompression
//! - metadata: Statistics, schemas, channels, attachments
//! - error_handling: Malformed files and edge cases
//! - stream_types: Records, chunks, raw_messages, messages
//! - stream_properties: Property tests for stream iteration
//! - round_trip: Property tests for parse/build round-trips

mod broken_files_properties;
mod chunks;
mod compression;
mod error_handling;
mod filtering;
mod filtering_properties;
mod message_ordering;
mod metadata;
mod reader_basics;
mod round_trip;
mod stream_properties;
mod stream_types;
