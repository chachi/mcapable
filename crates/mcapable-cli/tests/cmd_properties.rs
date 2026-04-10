//! Property-based tests for CLI commands.
//!
//! These tests verify invariants across randomized inputs using proptest.

use bytes::Bytes;
use mcapable_core::writer::{ChunkOptions, WriterBuilder};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{Channel, Schema};
use proptest::prelude::*;
use std::collections::HashMap;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_mcap_with_messages(dir: &TempDir, name: &str, times: &[u64]) -> String {
    let path = dir.path().join(name);
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = WriterBuilder::new()
        .profile("test")
        .chunked(ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 4_194_304,
            include_crc: true,
        })
        .build(file)
        .unwrap();

    let schema = Schema {
        id: 1,
        name: ByteStr::from("pkg/Msg"),
        encoding: ByteStr::from("jsonschema"),
        data: Bytes::from_static(br#"{"type":"object"}"#),
    };
    writer.copy_schema(&schema).unwrap();

    let channel = Channel {
        id: 1,
        topic: ByteStr::from("/test"),
        message_encoding: ByteStr::from("json"),
        schema_id: 1,
        metadata: HashMap::new(),
    };
    let mut cw = writer.copy_channel(&channel).unwrap();
    for &t in times {
        cw.write(t, t, Bytes::from_static(b"x")).unwrap();
    }
    drop(cw);
    writer.finish().unwrap();
    path.to_str().unwrap().to_string()
}

fn read_log_times(path: &str) -> Vec<u64> {
    let bytes = std::fs::read(path).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut times = Vec::new();
    for msg in reader.messages().unwrap() {
        times.push(msg.unwrap().log_time);
    }
    times
}

fn default_output_options() -> mcapable_cli::cli::cmd::OutputOptions {
    mcapable_cli::cli::cmd::OutputOptions {
        compression: "none".to_string(),
        chunk_size: 4_194_304,
        chunked: true,
        include_crc: true,
    }
}

// ---------------------------------------------------------------------------
// Property: sort always produces sorted output
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_sort_output_is_sorted(
        times in prop::collection::vec(0u64..1_000_000_000, 0..50),
    ) {
        let dir = TempDir::new().unwrap();
        let input = write_mcap_with_messages(&dir, "input.mcap", &times);
        let output = dir.path().join("sorted.mcap").to_str().unwrap().to_string();

        mcapable_cli::cli::cmd::sort::run(
            Some(input),
            Some(output.clone()),
            default_output_options(),
        )
        .unwrap();

        let result = read_log_times(&output);
        assert_eq!(result.len(), times.len());
        for w in result.windows(2) {
            prop_assert!(w[0] <= w[1], "not sorted: {} > {}", w[0], w[1]);
        }
    }
}

// ---------------------------------------------------------------------------
// Property: merge always produces sorted output
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_merge_output_is_sorted(
        times_a in prop::collection::vec(0u64..1_000_000_000, 0..30),
        times_b in prop::collection::vec(0u64..1_000_000_000, 0..30),
    ) {
        let dir = TempDir::new().unwrap();
        let file_a = write_mcap_with_messages(&dir, "a.mcap", &times_a);
        let file_b = write_mcap_with_messages(&dir, "b.mcap", &times_b);
        let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();

        mcapable_cli::cli::cmd::merge::run(
            output.clone(),
            vec![file_a, file_b],
            default_output_options(),
            "auto".to_string(),
            false,
        )
        .unwrap();

        let result = read_log_times(&output);
        prop_assert_eq!(result.len(), times_a.len() + times_b.len());
        for w in result.windows(2) {
            prop_assert!(w[0] <= w[1], "merge not sorted: {} > {}", w[0], w[1]);
        }
    }
}

// ---------------------------------------------------------------------------
// Property: filter by time range only includes messages within range
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_filter_time_range_sound(
        times in prop::collection::vec(0u64..1_000_000_000, 1..50),
        start in 0u64..500_000_000,
        end in 500_000_000u64..1_000_000_000,
    ) {
        let dir = TempDir::new().unwrap();
        let input = write_mcap_with_messages(&dir, "input.mcap", &times);
        let output = dir.path().join("filtered.mcap").to_str().unwrap().to_string();

        mcapable_cli::cli::cmd::filter::run(
            Some(input),
            Some(output.clone()),
            vec![],
            Some(start.to_string()),
            Some(end.to_string()),
            None, None, None, None,
            vec![], vec![], vec![],
            true, true,
            default_output_options(),
        )
        .unwrap();

        let result = read_log_times(&output);
        let expected_count = times.iter().filter(|&&t| t >= start && t <= end).count();
        prop_assert_eq!(result.len(), expected_count);
        for t in &result {
            prop_assert!(*t >= start, "message {} before start {}", t, start);
            prop_assert!(*t <= end, "message {} after end {}", t, end);
        }
    }
}
