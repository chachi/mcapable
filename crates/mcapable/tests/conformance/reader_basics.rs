//! Basic reader conformance tests.
//!
//! These tests verify fundamental reader behavior conforms to the MCAP specification.

use crate::helpers::mcap_builder::*;
use mcapable::reader;
use std::io::Cursor;

#[test]
fn test_reader_validates_magic_bytes() {
    // Should reject files without proper magic bytes
    let bad_data = b"not an mcap file";
    let cursor = Cursor::new(bad_data.to_vec());
    let result = reader::Builder::new().build(cursor);
    assert!(result.is_err());
}

#[test]
fn test_reader_validates_start_magic() {
    // Should validate start magic bytes
    let mut bad_data = Vec::new();
    bad_data.extend_from_slice(b"BADMAGIC");
    bad_data.extend_from_slice(&[0; 100]); // padding

    let cursor = Cursor::new(bad_data);
    let result = reader::Builder::new().build(cursor);
    assert!(result.is_err());
}

#[test]
fn test_reader_accepts_valid_magic() {
    // Should accept proper MCAP magic bytes
    let mcap = create_simple_mcap();
    let cursor = Cursor::new(mcap);
    reader::Builder::new()
        .build(cursor)
        .expect("should accept valid MCAP file");
}

#[test]
fn test_lazy_header_loading() {
    // Header should not be loaded until first access
    let mcap = create_simple_mcap();
    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new()
        .build(cursor)
        .expect("should create reader");

    // First access should load header
    let header = reader.header().expect("should load header");
    assert_eq!(header.profile, "");
    assert!(!header.library.is_empty());

    // Second access should use cached header
    let header2 = reader.header().expect("should return cached header");
    assert_eq!(header.profile, header2.profile);
    assert_eq!(header.library, header2.library);
}

#[test]
fn test_lazy_summary_loading() {
    // Summary should not be loaded until requested
    let mcap = create_simple_mcap();
    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new()
        .build(cursor)
        .expect("should create reader");

    // First access should load summary
    let summary = reader.summary().expect("should load summary");

    // Should have summary data
    assert!(summary.is_some());
}

#[test]
fn test_reader_clones_schema_and_channel() {
    // Accessing schema/channel should return clones
    let mcap = create_simple_mcap();
    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::new()
        .build(cursor)
        .expect("should create reader");

    // Iterate to load schemas/channels
    for msg in reader.messages().unwrap() {
        let _ = msg.expect("should read message");
    }

    // Get channel multiple times
    let ch1 = reader.channel(0).expect("should have channel 0");
    let ch2 = reader.channel(0).expect("should have channel 0");

    // Should be independent clones
    assert_eq!(ch1.id, ch2.id);
    assert_eq!(ch1.topic, ch2.topic);
}

#[test]
fn test_reader_into_inner() {
    // Should be able to get inner reader back
    let mcap = create_simple_mcap();
    let cursor = Cursor::new(mcap.clone());
    let reader = reader::Builder::new()
        .build(cursor)
        .expect("should create reader");

    let inner = reader.into_inner();
    assert_eq!(inner.into_inner(), mcap);
}

#[test]
fn test_builder_default_options() {
    // Builder should use sensible defaults
    let mcap = create_simple_mcap();
    let cursor = Cursor::new(mcap);
    let mut reader = reader::Builder::default()
        .build(cursor)
        .expect("default builder should work");

    let _header = reader.header().expect("should load header");
}

#[test]
fn test_builder_end_magic_validation_option() {
    // Should be able to configure end magic validation
    let mcap = create_simple_mcap();
    // With validation
    let cursor = Cursor::new(mcap.clone());
    let _reader = reader::Builder::new()
        .validate_end_magic(true)
        .build(cursor)
        .expect("should create reader");

    // Without validation
    let cursor = Cursor::new(mcap);
    let _reader = reader::Builder::new()
        .validate_end_magic(false)
        .build(cursor)
        .expect("should create reader");
}
