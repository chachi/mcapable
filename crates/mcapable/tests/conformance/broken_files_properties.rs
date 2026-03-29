//! Property-based tests for broken/partially-written MCAP files.
//!
//! These tests focus on robustness and error behavior: missing end magic, missing
//! footer/summary, truncated bodies, and missing indexes.

use crate::helpers::mcap_builder::McapBuilder;
use mcapable::reader;
use proptest::prelude::*;
use std::io::Cursor;

fn valid_mcap_bytes(message_count: usize) -> Vec<u8> {
    let mut builder = McapBuilder::new().chunked(true);
    builder = builder.add_simple_channel(0, "/test");
    for i in 0..message_count {
        builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
    }
    builder.build()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, .. ProptestConfig::default() })]

    #[test]
    fn prop_missing_end_magic_fails_when_validation_enabled(msgs in 1usize..20usize) {
        let mut bytes = valid_mcap_bytes(msgs);
        prop_assume!(bytes.len() > 8);
        bytes.truncate(bytes.len().saturating_sub(8));

        let cursor = Cursor::new(bytes);
        let res = reader::Builder::new().validate_end_magic(true).build(cursor);
        prop_assert!(res.is_err());
    }

    #[test]
    fn prop_missing_end_magic_ok_when_validation_disabled(msgs in 1usize..20usize) {
        let mut bytes = valid_mcap_bytes(msgs);
        prop_assume!(bytes.len() > 16);
        bytes.truncate(bytes.len().saturating_sub(8));

        let cursor = Cursor::new(bytes);
        let mut r = reader::Builder::new().validate_end_magic(false).build(cursor).unwrap();

        // Robustness: record iteration should not panic/hang.
        prop_assert!(r.records().next().is_some());

        // Missing footer/summary/index is fine: these should not error.
        let _ = r.footer().unwrap();
        let _ = r.summary().unwrap();
        let _ = r.chunk_indexes().unwrap();
        let _ = r.message_indexes().unwrap();
    }

    #[test]
    fn prop_truncation_does_not_panic(msgs in 1usize..20usize, cut in 8usize..10_000usize) {
        let bytes = valid_mcap_bytes(msgs);
        prop_assume!(bytes.len() > 8);
        let cut = cut.min(bytes.len());
        let bytes = bytes[..cut].to_vec();

        let cursor = Cursor::new(bytes);
        let res = reader::Builder::new().validate_end_magic(false).build(cursor);
        if let Ok(mut r) = res {
            let _ = r.records().take(10).collect::<Vec<_>>();
        }
    }
}

#[test]
fn missing_footer_keeps_iteration_working_and_indexes_empty() {
    let bytes = valid_mcap_bytes(5);
    assert!(bytes.len() > 37);

    // Drop the footer record header+body, keep trailing magic.
    let start = bytes.len() - 37;
    let end = bytes.len() - 8;
    let mut broken = bytes[..start].to_vec();
    broken.extend_from_slice(&bytes[end..]);

    let cursor = Cursor::new(broken);
    let mut r = reader::Builder::new()
        .validate_end_magic(true)
        .build(cursor)
        .unwrap();

    assert!(r.footer().unwrap().is_none());
    assert!(r.summary().unwrap().is_none());
    assert!(r.chunk_indexes().unwrap().is_empty());
    assert!(r.message_indexes().unwrap().is_empty());

    // Still able to read messages by scanning.
    let count = r.messages().unwrap().count();
    assert!(count > 0);
}

#[test]
fn invalid_summary_offset_surfaces_invalid_summary_error() {
    let mut bytes = valid_mcap_bytes(5);

    // Locate footer record and rewrite summary_start to a bogus value.
    let mut reader = mcapable::Reader::from_slice(&bytes).unwrap();
    let mut footer_offset = None;
    for rec in reader.record_metadata() {
        let rec = rec.expect("record metadata should succeed");
        if rec.opcode == mcapable::Opcode::Footer {
            footer_offset = Some(rec.offset);
            break;
        }
    }
    let footer_offset = footer_offset.expect("footer must exist in valid file");
    let payload_start = footer_offset as usize + mcapable_core::format::RECORD_HEADER_SIZE;

    let bogus = u64::MAX / 2;
    bytes[payload_start..payload_start + 8].copy_from_slice(&bogus.to_le_bytes());

    let cursor = Cursor::new(bytes);
    let mut r = reader::Builder::new()
        .validate_end_magic(true)
        .build(cursor)
        .unwrap();
    let err = r.summary().unwrap_err();
    assert!(matches!(err, mcapable::Error::InvalidSummary(_)));
}
