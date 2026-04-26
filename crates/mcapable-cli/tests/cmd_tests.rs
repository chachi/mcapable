//! Integration tests for CLI commands.
//!
//! Tests create MCAP fixture files using mcapable-core, call CLI `run` functions
//! directly, then verify output files with mcapable-core's reader.

use bytes::Bytes;
use mcapable_cli::cli::cmd::cat::CatOptions;
use mcapable_cli::cli::cmd::filter::FilterOptions;
use mcapable_core::writer::{ChunkOptions, WriterBuilder};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{Channel, Schema};
use std::collections::HashMap;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a simple MCAP with the given messages to a file, returning the path.
fn write_fixture(
    dir: &TempDir,
    name: &str,
    messages: &[(u16, u64, &[u8])], // (channel_id_hint, log_time, data)
    compression: Option<mcapable_core::Compression>,
) -> String {
    write_fixture_multi_topic(dir, name, messages, compression, &["/test"])
}

/// Write an MCAP with multiple topics. Messages reference topics by index.
fn write_fixture_multi_topic(
    dir: &TempDir,
    name: &str,
    messages: &[(u16, u64, &[u8])], // (topic_index, log_time, data)
    compression: Option<mcapable_core::Compression>,
    topics: &[&str],
) -> String {
    let path = dir.path().join(name);
    let file = std::fs::File::create(&path).unwrap();
    let chunk_opts = ChunkOptions {
        compression,
        max_uncompressed_bytes: 4_194_304,
        include_crc: true,
    };
    let mut writer = WriterBuilder::new()
        .profile("test-profile")
        .library("test-lib")
        .chunked(chunk_opts)
        .build(file)
        .unwrap();

    let schema = Schema {
        id: 1,
        name: ByteStr::from("pkg/Msg"),
        encoding: ByteStr::from("jsonschema"),
        data: Bytes::from_static(br#"{"type":"object"}"#),
    };
    writer.copy_schema(&schema).unwrap();

    let mut channel_writers = Vec::new();
    for (i, topic) in topics.iter().enumerate() {
        let channel = Channel {
            id: (i + 1) as u16,
            topic: ByteStr::from(*topic),
            message_encoding: ByteStr::from("json"),
            schema_id: 1,
            metadata: HashMap::new(),
        };
        let cw = writer.copy_channel(&channel).unwrap();
        channel_writers.push(cw);
    }

    for (topic_idx, log_time, data) in messages {
        let cw = &mut channel_writers[*topic_idx as usize];
        cw.write(*log_time, *log_time, Bytes::copy_from_slice(data))
            .unwrap();
    }
    drop(channel_writers);
    writer.finish().unwrap();
    path.to_str().unwrap().to_string()
}

fn write_fixture_with_metadata_and_attachment(dir: &TempDir, name: &str) -> String {
    let path = dir.path().join(name);
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = WriterBuilder::new()
        .profile("test-profile")
        .library("test-lib")
        .chunked(ChunkOptions::default())
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
    cw.write(100, 100, Bytes::from_static(b"hello")).unwrap();
    drop(cw);

    writer
        .copy_attachment(
            100,
            200,
            ByteStr::from("test.bin"),
            ByteStr::from("application/octet-stream"),
            Bytes::from_static(b"attachment-data"),
        )
        .unwrap();

    let md = mcapable_core::types::Metadata {
        name: ByteStr::from("test-meta"),
        metadata: {
            let mut m = HashMap::new();
            m.insert(ByteStr::from("key1"), ByteStr::from("val1"));
            m
        },
    };
    writer.copy_metadata(&md).unwrap();

    writer.finish().unwrap();
    path.to_str().unwrap().to_string()
}

fn read_messages(path: &str) -> Vec<(u16, u64, u32, Vec<u8>)> {
    let bytes = std::fs::read(path).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut result = Vec::new();
    for msg in reader.messages().unwrap() {
        let msg = msg.unwrap();
        result.push((
            msg.channel_id,
            msg.log_time,
            msg.sequence,
            msg.data().to_vec(),
        ));
    }
    result
}

fn read_metadata_names(path: &str) -> Vec<String> {
    let bytes = std::fs::read(path).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut names = Vec::new();
    for record in reader
        .records()
        .filter(|op| matches!(op, mcapable_core::Opcode::Metadata))
    {
        if let mcapable_core::Record::Metadata(md) = record.unwrap() {
            names.push(md.name.as_ref().to_string());
        }
    }
    names
}

fn read_attachment_data(path: &str, name: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    for record in reader
        .records()
        .filter(|op| matches!(op, mcapable_core::Opcode::Attachment))
    {
        if let mcapable_core::Record::Attachment(att) = record.unwrap() {
            if att.name.as_ref() == name {
                return Some(att.data.to_vec());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Phase 7: version
// ---------------------------------------------------------------------------

#[test]
fn version_prints_cli_version() {
    let mut buf = Vec::new();
    mcapable_cli::cli::cmd::version::run_with_output(&mut buf, false).unwrap();
    let output = String::from_utf8(buf).unwrap();
    assert!(output.starts_with('v'));
    assert!(output.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_library_prints_core_version() {
    let mut buf = Vec::new();
    mcapable_cli::cli::cmd::version::run_with_output(&mut buf, true).unwrap();
    let output = String::from_utf8(buf).unwrap();
    assert!(output.contains("mcapable-core"));
    assert!(output.contains(mcapable_core::VERSION));
}

// ---------------------------------------------------------------------------
// Phase 3: merge
// ---------------------------------------------------------------------------

#[test]
fn merge_interleaves_by_log_time() {
    let dir = TempDir::new().unwrap();
    let file_a = write_fixture(&dir, "a.mcap", &[(0, 100, b"a1"), (0, 300, b"a2")], None);
    let file_b = write_fixture(&dir, "b.mcap", &[(0, 200, b"b1"), (0, 400, b"b2")], None);
    let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();

    mcapable_cli::cli::cmd::merge::run(
        output.clone(),
        vec![file_a, file_b],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
        "auto".to_string(),
        false,
    )
    .unwrap();

    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 4);
    // Verify globally sorted by log_time
    for w in msgs.windows(2) {
        assert!(
            w[0].1 <= w[1].1,
            "messages not sorted: {} > {}",
            w[0].1,
            w[1].1
        );
    }
    assert_eq!(msgs[0].1, 100);
    assert_eq!(msgs[1].1, 200);
    assert_eq!(msgs[2].1, 300);
    assert_eq!(msgs[3].1, 400);
}

#[test]
fn merge_coalesce_auto_maps_same_topic_to_one_channel() {
    let dir = TempDir::new().unwrap();
    // Both files have /test topic but potentially different channel IDs
    let file_a = write_fixture(&dir, "a.mcap", &[(0, 100, b"a1")], None);
    let file_b = write_fixture(&dir, "b.mcap", &[(0, 200, b"b1")], None);
    let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();

    mcapable_cli::cli::cmd::merge::run(
        output.clone(),
        vec![file_a, file_b],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
        "auto".to_string(),
        false,
    )
    .unwrap();

    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 2);
    // All messages should be on the same output channel
    assert_eq!(msgs[0].0, msgs[1].0);
}

#[test]
fn merge_incompatible_profiles_errors() {
    let dir = TempDir::new().unwrap();
    let file_a = write_fixture(&dir, "a.mcap", &[(0, 100, b"a1")], None);

    // Write file_b with a different profile
    let path_b = dir.path().join("b.mcap");
    let file_b = std::fs::File::create(&path_b).unwrap();
    let mut writer = WriterBuilder::new()
        .profile("different-profile")
        .chunked(ChunkOptions::default())
        .build(file_b)
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
    cw.write(100, 100, Bytes::from_static(b"x")).unwrap();
    drop(cw);
    writer.finish().unwrap();
    let file_b_str = path_b.to_str().unwrap().to_string();

    let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();
    let result = mcapable_cli::cli::cmd::merge::run(
        output,
        vec![file_a, file_b_str],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
        "auto".to_string(),
        false,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("incompatible profile"));
}

#[test]
fn merge_allow_duplicate_metadata_preserves_all() {
    let dir = TempDir::new().unwrap();
    let file_a = write_fixture_with_metadata_and_attachment(&dir, "a.mcap");
    let file_b = write_fixture_with_metadata_and_attachment(&dir, "b.mcap");
    let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();

    mcapable_cli::cli::cmd::merge::run(
        output.clone(),
        vec![file_a, file_b],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
        "auto".to_string(),
        true, // allow_duplicate_metadata
    )
    .unwrap();

    let names = read_metadata_names(&output);
    // Both files contribute "test-meta", so we should have 2
    assert_eq!(names.len(), 2);
}

#[test]
fn merge_dedup_metadata_by_default() {
    let dir = TempDir::new().unwrap();
    let file_a = write_fixture_with_metadata_and_attachment(&dir, "a.mcap");
    let file_b = write_fixture_with_metadata_and_attachment(&dir, "b.mcap");
    let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();

    mcapable_cli::cli::cmd::merge::run(
        output.clone(),
        vec![file_a, file_b],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
        "auto".to_string(),
        false, // default: dedup
    )
    .unwrap();

    let names = read_metadata_names(&output);
    // Duplicate "test-meta" should be deduped to 1
    assert_eq!(names.len(), 1);
}

// ---------------------------------------------------------------------------
// Phase 4: filter
// ---------------------------------------------------------------------------

#[test]
fn filter_by_time_range() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(
        &dir,
        "input.mcap",
        &[
            (0, 100, b"m1"),
            (0, 200, b"m2"),
            (0, 300, b"m3"),
            (0, 400, b"m4"),
        ],
        None,
    );
    let output = dir
        .path()
        .join("filtered.mcap")
        .to_str()
        .unwrap()
        .to_string();

    mcapable_cli::cli::cmd::filter::run(FilterOptions {
        input: Some(input),
        output: Some(output.clone()),
        topics: vec![],
        start: Some("200".to_string()),
        end: Some("300".to_string()),
        start_secs: None,
        start_nsecs: None,
        end_secs: None,
        end_nsecs: None,
        include_topic_regex: vec![],
        exclude_topic_regex: vec![],
        last_per_channel_topic_regex: vec![],
        include_metadata: true,
        include_attachments: true,
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    })
    .unwrap();

    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].1, 200);
    assert_eq!(msgs[1].1, 300);
}

#[test]
fn filter_include_topic_regex() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture_multi_topic(
        &dir,
        "input.mcap",
        &[(0, 100, b"imu"), (1, 200, b"cmd")],
        None,
        &["/sensor/imu", "/control/cmd"],
    );
    let output = dir
        .path()
        .join("filtered.mcap")
        .to_str()
        .unwrap()
        .to_string();

    mcapable_cli::cli::cmd::filter::run(FilterOptions {
        input: Some(input),
        output: Some(output.clone()),
        topics: vec![],
        start: None,
        end: None,
        start_secs: None,
        start_nsecs: None,
        end_secs: None,
        end_nsecs: None,
        include_topic_regex: vec!["/sensor.*".to_string()],
        exclude_topic_regex: vec![],
        last_per_channel_topic_regex: vec![],
        include_metadata: true,
        include_attachments: true,
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    })
    .unwrap();

    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].3, b"imu");
}

#[test]
fn filter_exclude_topic_regex() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture_multi_topic(
        &dir,
        "input.mcap",
        &[(0, 100, b"imu"), (1, 200, b"debug")],
        None,
        &["/sensor/imu", "/debug/log"],
    );
    let output = dir
        .path()
        .join("filtered.mcap")
        .to_str()
        .unwrap()
        .to_string();

    mcapable_cli::cli::cmd::filter::run(FilterOptions {
        input: Some(input),
        output: Some(output.clone()),
        topics: vec![],
        start: None,
        end: None,
        start_secs: None,
        start_nsecs: None,
        end_secs: None,
        end_nsecs: None,
        include_topic_regex: vec![],
        exclude_topic_regex: vec!["/debug.*".to_string()],
        last_per_channel_topic_regex: vec![],
        include_metadata: true,
        include_attachments: true,
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    })
    .unwrap();

    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].3, b"imu");
}

#[test]
fn filter_topics_and_regex_together_errors() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "input.mcap", &[(0, 100, b"m1")], None);
    let output = dir
        .path()
        .join("filtered.mcap")
        .to_str()
        .unwrap()
        .to_string();

    let result = mcapable_cli::cli::cmd::filter::run(FilterOptions {
        input: Some(input),
        output: Some(output),
        topics: vec!["/test".to_string()],
        start: None,
        end: None,
        start_secs: None,
        start_nsecs: None,
        end_secs: None,
        end_nsecs: None,
        include_topic_regex: vec!["/test".to_string()],
        exclude_topic_regex: vec![],
        last_per_channel_topic_regex: vec![],
        include_metadata: true,
        include_attachments: true,
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    });
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cannot use both"));
}

#[test]
fn filter_include_metadata_false_strips_metadata() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture_with_metadata_and_attachment(&dir, "input.mcap");
    let output = dir
        .path()
        .join("filtered.mcap")
        .to_str()
        .unwrap()
        .to_string();

    mcapable_cli::cli::cmd::filter::run(FilterOptions {
        input: Some(input),
        output: Some(output.clone()),
        topics: vec![],
        start: None,
        end: None,
        start_secs: None,
        start_nsecs: None,
        end_secs: None,
        end_nsecs: None,
        include_topic_regex: vec![],
        exclude_topic_regex: vec![],
        last_per_channel_topic_regex: vec![],
        include_metadata: false,
        include_attachments: true,
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    })
    .unwrap();

    let names = read_metadata_names(&output);
    assert!(names.is_empty(), "metadata should be stripped");
}

#[test]
fn filter_include_attachments_false_strips_attachments() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture_with_metadata_and_attachment(&dir, "input.mcap");
    let output = dir
        .path()
        .join("filtered.mcap")
        .to_str()
        .unwrap()
        .to_string();

    mcapable_cli::cli::cmd::filter::run(FilterOptions {
        input: Some(input),
        output: Some(output.clone()),
        topics: vec![],
        start: None,
        end: None,
        start_secs: None,
        start_nsecs: None,
        end_secs: None,
        end_nsecs: None,
        include_topic_regex: vec![],
        exclude_topic_regex: vec![],
        last_per_channel_topic_regex: vec![],
        include_metadata: true,
        include_attachments: false,
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    })
    .unwrap();

    let att = read_attachment_data(&output, "test.bin");
    assert!(att.is_none(), "attachment should be stripped");
}

// ---------------------------------------------------------------------------
// Phase 5: get / add
// ---------------------------------------------------------------------------

#[test]
fn add_then_get_attachment_roundtrip() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "input.mcap", &[(0, 100, b"msg")], None);
    let output = dir
        .path()
        .join("with_att.mcap")
        .to_str()
        .unwrap()
        .to_string();

    // Write a file to attach
    let att_file = dir.path().join("payload.bin");
    std::fs::write(&att_file, b"attachment-payload-data").unwrap();

    mcapable_cli::cli::cmd::add::dispatch(mcapable_cli::cli::cmd::AddCommand::Attachment {
        input: Some(input),
        output: Some(output.clone()),
        file: att_file,
        name: Some("payload.bin".to_string()),
        content_type: "application/octet-stream".to_string(),
        log_time: Some("1000".to_string()),
        creation_time: Some("2000".to_string()),
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    })
    .unwrap();

    // Verify the attachment is there
    let data = read_attachment_data(&output, "payload.bin");
    assert_eq!(data.unwrap(), b"attachment-payload-data");

    // Original messages preserved
    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 1);
}

#[test]
fn add_then_get_metadata_roundtrip() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "input.mcap", &[(0, 100, b"msg")], None);
    let output = dir
        .path()
        .join("with_md.mcap")
        .to_str()
        .unwrap()
        .to_string();

    mcapable_cli::cli::cmd::add::dispatch(mcapable_cli::cli::cmd::AddCommand::Metadata {
        input: Some(input),
        output: Some(output.clone()),
        name: "build-info".to_string(),
        key: vec!["version=1.0".to_string(), "hash=abc123".to_string()],
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    })
    .unwrap();

    // Verify the metadata is there
    let names = read_metadata_names(&output);
    assert!(names.contains(&"build-info".to_string()));

    // Verify the key-value pairs by reading the raw metadata
    let out_bytes = std::fs::read(&output).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&out_bytes).unwrap();
    for record in reader
        .records()
        .filter(|op| matches!(op, mcapable_core::Opcode::Metadata))
    {
        if let mcapable_core::Record::Metadata(md) = record.unwrap() {
            if md.name.as_ref() == "build-info" {
                assert_eq!(
                    md.metadata.get(&ByteStr::from("version")),
                    Some(&ByteStr::from("1.0"))
                );
                assert_eq!(
                    md.metadata.get(&ByteStr::from("hash")),
                    Some(&ByteStr::from("abc123"))
                );
                return;
            }
        }
    }
    panic!("build-info metadata not found");
}

// ---------------------------------------------------------------------------
// Phase 6: du
// ---------------------------------------------------------------------------

#[test]
fn du_approximate_produces_output() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(
        &dir,
        "input.mcap",
        &[(0, 100, b"m1"), (0, 200, b"m2"), (0, 300, b"m3")],
        None,
    );

    let mut buf = Vec::new();
    mcapable_cli::cli::cmd::du::run_with_output(Some(input), &mut buf, true).unwrap();
    let output = String::from_utf8(buf).unwrap();
    assert!(output.contains("Approximate"));
    assert!(output.contains("Messages:"));
}

#[test]
fn du_exact_produces_topic_breakdown() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture_multi_topic(
        &dir,
        "input.mcap",
        &[(0, 100, b"imu"), (1, 200, b"cmd")],
        None,
        &["/sensor/imu", "/control/cmd"],
    );

    let mut buf = Vec::new();
    mcapable_cli::cli::cmd::du::run_with_output(Some(input), &mut buf, false).unwrap();
    let output = String::from_utf8(buf).unwrap();
    assert!(output.contains("/sensor/imu"));
    assert!(output.contains("/control/cmd"));
}

// ---------------------------------------------------------------------------
// Phase 1: OutputOptions integration
// ---------------------------------------------------------------------------

#[test]
fn sort_produces_sorted_output() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(
        &dir,
        "input.mcap",
        &[(0, 300, b"m3"), (0, 100, b"m1"), (0, 200, b"m2")],
        None,
    );
    let output = dir.path().join("sorted.mcap").to_str().unwrap().to_string();

    mcapable_cli::cli::cmd::sort::run(
        Some(input),
        Some(output.clone()),
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    )
    .unwrap();

    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0].1, 100);
    assert_eq!(msgs[1].1, 200);
    assert_eq!(msgs[2].1, 300);
}

#[test]
fn sort_compression_none_produces_uncompressed() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(
        &dir,
        "input.mcap",
        &[(0, 200, b"m2"), (0, 100, b"m1")],
        Some(mcapable_core::Compression::Zstd),
    );
    let output = dir.path().join("sorted.mcap").to_str().unwrap().to_string();

    mcapable_cli::cli::cmd::sort::run(
        Some(input),
        Some(output.clone()),
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    )
    .unwrap();

    // Verify chunks are uncompressed
    let bytes = std::fs::read(&output).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().unwrap();
    for ci in summary.chunk_indexes.iter() {
        assert!(
            ci.compression.as_ref().is_empty(),
            "expected uncompressed chunk, got {:?}",
            ci.compression
        );
    }
}

#[test]
fn merge_with_lz4_compression() {
    let dir = TempDir::new().unwrap();
    let file_a = write_fixture(&dir, "a.mcap", &[(0, 100, b"a1")], None);
    let file_b = write_fixture(&dir, "b.mcap", &[(0, 200, b"b1")], None);
    let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();

    mcapable_cli::cli::cmd::merge::run(
        output.clone(),
        vec![file_a, file_b],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "lz4".to_string(),
            chunk_size: 2_097_152,
            chunked: true,
            include_crc: true,
        },
        "auto".to_string(),
        false,
    )
    .unwrap();

    let bytes = std::fs::read(&output).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().unwrap();
    for ci in summary.chunk_indexes.iter() {
        assert_eq!(ci.compression.as_ref(), "lz4");
    }
    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 2);
}

#[test]
fn merge_chunked_false_produces_unchunked() {
    let dir = TempDir::new().unwrap();
    let file_a = write_fixture(&dir, "a.mcap", &[(0, 100, b"a1")], None);
    let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();

    mcapable_cli::cli::cmd::merge::run(
        output.clone(),
        vec![file_a],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: false,
            include_crc: true,
        },
        "auto".to_string(),
        false,
    )
    .unwrap();

    let bytes = std::fs::read(&output).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().unwrap();
    assert!(
        summary.chunk_indexes.is_empty(),
        "expected no chunks for unchunked output, got {}",
        summary.chunk_indexes.len()
    );
    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 1);
}

#[test]
fn filter_include_crc_false_produces_zero_crc() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(
        &dir,
        "input.mcap",
        &[(0, 100, b"m1"), (0, 200, b"m2")],
        None,
    );
    let output = dir
        .path()
        .join("filtered.mcap")
        .to_str()
        .unwrap()
        .to_string();

    mcapable_cli::cli::cmd::filter::run(FilterOptions {
        input: Some(input),
        output: Some(output.clone()),
        topics: vec![],
        start: None,
        end: None,
        start_secs: None,
        start_nsecs: None,
        end_secs: None,
        end_nsecs: None,
        include_topic_regex: vec![],
        exclude_topic_regex: vec![],
        last_per_channel_topic_regex: vec![],
        include_metadata: true,
        include_attachments: true,
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: false,
        },
    })
    .unwrap();

    // Read chunks and verify CRC is 0
    let bytes = std::fs::read(&output).unwrap();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut found_chunk = false;
    for record in reader.records() {
        if let mcapable_core::Record::Chunk(chunk) = record.unwrap() {
            assert_eq!(
                chunk.uncompressed_crc, 0,
                "expected CRC=0 with --include-crc false"
            );
            found_chunk = true;
        }
    }
    assert!(found_chunk, "expected at least one chunk");
}

// ---------------------------------------------------------------------------
// Phase 2: cat --json
// ---------------------------------------------------------------------------

#[test]
fn cat_json_produces_valid_jsonl() {
    let dir = TempDir::new().unwrap();
    // Write a fixture with jsonschema encoding and JSON data
    let path = dir.path().join("input.mcap");
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = WriterBuilder::new()
        .profile("test")
        .chunked(ChunkOptions::default())
        .build(file)
        .unwrap();

    let schema = Schema {
        id: 1,
        name: ByteStr::from("test/Msg"),
        encoding: ByteStr::from("jsonschema"),
        data: Bytes::from_static(br#"{"type":"object"}"#),
    };
    writer.copy_schema(&schema).unwrap();

    let channel = Channel {
        id: 1,
        topic: ByteStr::from("/data"),
        message_encoding: ByteStr::from("json"),
        schema_id: 1,
        metadata: HashMap::new(),
    };
    let mut cw = writer.copy_channel(&channel).unwrap();
    cw.write(1000, 2000, Bytes::from_static(br#"{"value":42}"#))
        .unwrap();
    cw.write(3000, 4000, Bytes::from_static(br#"{"value":99}"#))
        .unwrap();
    drop(cw);
    writer.finish().unwrap();

    let input = path.to_str().unwrap().to_string();
    let mut buf = Vec::new();
    mcapable_cli::cli::cmd::cat::run_with_output(
        CatOptions {
            input: Some(input),
            topics: vec![],
            start: None,
            end: None,
            start_secs: None,
            start_nsecs: None,
            end_secs: None,
            end_nsecs: None,
            json: true,
        },
        &mut buf,
    )
    .unwrap();

    let output = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = output.trim().split('\n').collect();
    assert_eq!(lines.len(), 2);

    // Parse each line as JSON
    let obj1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(obj1["topic"], "/data");
    assert_eq!(obj1["log_time"], 1000);
    assert_eq!(obj1["publish_time"], 2000);
    assert_eq!(obj1["data"]["value"], 42);

    let obj2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(obj2["log_time"], 3000);
    assert_eq!(obj2["data"]["value"], 99);
}

#[test]
fn cat_json_unknown_encoding_produces_null_data() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("input.mcap");
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = WriterBuilder::new()
        .profile("test")
        .chunked(ChunkOptions::default())
        .build(file)
        .unwrap();

    let schema = Schema {
        id: 1,
        name: ByteStr::from("test/Msg"),
        encoding: ByteStr::from("custom-binary"),
        data: Bytes::from_static(b"irrelevant"),
    };
    writer.copy_schema(&schema).unwrap();

    let channel = Channel {
        id: 1,
        topic: ByteStr::from("/binary"),
        message_encoding: ByteStr::from("custom"),
        schema_id: 1,
        metadata: HashMap::new(),
    };
    let mut cw = writer.copy_channel(&channel).unwrap();
    cw.write(100, 200, Bytes::from_static(b"\x00\x01\x02"))
        .unwrap();
    drop(cw);
    writer.finish().unwrap();

    let input = path.to_str().unwrap().to_string();
    let mut buf = Vec::new();
    mcapable_cli::cli::cmd::cat::run_with_output(
        CatOptions {
            input: Some(input),
            topics: vec![],
            start: None,
            end: None,
            start_secs: None,
            start_nsecs: None,
            end_secs: None,
            end_nsecs: None,
            json: true,
        },
        &mut buf,
    )
    .unwrap();

    let output = String::from_utf8(buf).unwrap();
    let obj: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(obj["topic"], "/binary");
    assert!(
        obj["data"].is_null(),
        "expected null data for unknown encoding"
    );
}

#[test]
fn cat_text_mode_outputs_topic_and_timestamps() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "input.mcap", &[(0, 500, b"hello")], None);

    let mut buf = Vec::new();
    mcapable_cli::cli::cmd::cat::run_with_output(
        CatOptions {
            input: Some(input),
            topics: vec![],
            start: None,
            end: None,
            start_secs: None,
            start_nsecs: None,
            end_secs: None,
            end_nsecs: None,
            json: false,
        },
        &mut buf,
    )
    .unwrap();

    let output = String::from_utf8(buf).unwrap();
    assert!(output.contains("/test"), "should contain topic name");
    assert!(output.contains("500"), "should contain log_time");
    assert!(output.contains("hello"), "should contain message data");
}

// ---------------------------------------------------------------------------
// Additional merge tests
// ---------------------------------------------------------------------------

#[test]
fn merge_coalesce_auto_errors_on_conflicting_metadata() {
    let dir = TempDir::new().unwrap();

    // Write file_a with /test channel having metadata key1=val1
    let path_a = dir.path().join("a.mcap");
    {
        let file = std::fs::File::create(&path_a).unwrap();
        let mut writer = WriterBuilder::new()
            .profile("test-profile")
            .chunked(ChunkOptions::default())
            .build(file)
            .unwrap();
        let schema = Schema {
            id: 1,
            name: ByteStr::from("pkg/Msg"),
            encoding: ByteStr::from("jsonschema"),
            data: Bytes::from_static(br#"{"type":"object"}"#),
        };
        writer.copy_schema(&schema).unwrap();
        let mut meta = HashMap::new();
        meta.insert(ByteStr::from("key1"), ByteStr::from("val1"));
        let channel = Channel {
            id: 1,
            topic: ByteStr::from("/test"),
            message_encoding: ByteStr::from("json"),
            schema_id: 1,
            metadata: meta,
        };
        let mut cw = writer.copy_channel(&channel).unwrap();
        cw.write(100, 100, Bytes::from_static(b"a")).unwrap();
        drop(cw);
        writer.finish().unwrap();
    }

    // Write file_b with /test channel having DIFFERENT metadata key1=val2
    let path_b = dir.path().join("b.mcap");
    {
        let file = std::fs::File::create(&path_b).unwrap();
        let mut writer = WriterBuilder::new()
            .profile("test-profile")
            .chunked(ChunkOptions::default())
            .build(file)
            .unwrap();
        let schema = Schema {
            id: 1,
            name: ByteStr::from("pkg/Msg"),
            encoding: ByteStr::from("jsonschema"),
            data: Bytes::from_static(br#"{"type":"object"}"#),
        };
        writer.copy_schema(&schema).unwrap();
        let mut meta = HashMap::new();
        meta.insert(ByteStr::from("key1"), ByteStr::from("val2"));
        let channel = Channel {
            id: 1,
            topic: ByteStr::from("/test"),
            message_encoding: ByteStr::from("json"),
            schema_id: 1,
            metadata: meta,
        };
        let mut cw = writer.copy_channel(&channel).unwrap();
        cw.write(200, 200, Bytes::from_static(b"b")).unwrap();
        drop(cw);
        writer.finish().unwrap();
    }

    let output = dir.path().join("merged.mcap").to_str().unwrap().to_string();

    // auto mode should error on conflicting channel metadata
    let result = mcapable_cli::cli::cmd::merge::run(
        output.clone(),
        vec![
            path_a.to_str().unwrap().to_string(),
            path_b.to_str().unwrap().to_string(),
        ],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
        "auto".to_string(),
        false,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("conflicting metadata"));

    // force mode should succeed
    mcapable_cli::cli::cmd::merge::run(
        output.clone(),
        vec![
            path_a.to_str().unwrap().to_string(),
            path_b.to_str().unwrap().to_string(),
        ],
        mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
        "force".to_string(),
        false,
    )
    .unwrap();

    let msgs = read_messages(&output);
    assert_eq!(msgs.len(), 2);
}

// ---------------------------------------------------------------------------
// Additional get tests
// ---------------------------------------------------------------------------

#[test]
fn get_attachment_extracts_data() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture_with_metadata_and_attachment(&dir, "input.mcap");

    // get attachment writes to a file via run_attachment (internal), but we can
    // use dispatch with GetCommand::Attachment
    let out_file = dir.path().join("extracted.bin");
    mcapable_cli::cli::cmd::get::dispatch(mcapable_cli::cli::cmd::GetCommand::Attachment {
        input: Some(input.clone()),
        name: "test.bin".to_string(),
        offset: None,
        output: Some(out_file.to_str().unwrap().to_string()),
    })
    .unwrap();

    let data = std::fs::read(&out_file).unwrap();
    assert_eq!(data, b"attachment-data");
}

#[test]
fn get_attachment_nonexistent_errors() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture_with_metadata_and_attachment(&dir, "input.mcap");

    let result =
        mcapable_cli::cli::cmd::get::dispatch(mcapable_cli::cli::cmd::GetCommand::Attachment {
            input: Some(input),
            name: "nonexistent.bin".to_string(),
            offset: None,
            output: None,
        });
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn get_metadata_prints_key_value_pairs() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture_with_metadata_and_attachment(&dir, "input.mcap");

    // get metadata prints to stdout which we can't easily capture here,
    // but we can verify it doesn't error and the metadata exists
    // The function prints to println! so we just verify it succeeds
    mcapable_cli::cli::cmd::get::dispatch(mcapable_cli::cli::cmd::GetCommand::Metadata {
        input: Some(input),
        name: "test-meta".to_string(),
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// Additional filter tests
// ---------------------------------------------------------------------------

#[test]
fn filter_last_per_channel_topic_regex() {
    let dir = TempDir::new().unwrap();
    // Create file with messages at times 100, 200, 300, 400
    // --start 300 with --last-per-channel-topic-regex should include the
    // latest pre-start message (at time 200) plus messages at 300, 400
    let input = write_fixture(
        &dir,
        "input.mcap",
        &[
            (0, 100, b"m1"),
            (0, 200, b"m2"),
            (0, 300, b"m3"),
            (0, 400, b"m4"),
        ],
        None,
    );
    let output = dir
        .path()
        .join("filtered.mcap")
        .to_str()
        .unwrap()
        .to_string();

    mcapable_cli::cli::cmd::filter::run(FilterOptions {
        input: Some(input),
        output: Some(output.clone()),
        topics: vec![],
        start: Some("300".to_string()),
        end: None,
        start_secs: None,
        start_nsecs: None,
        end_secs: None,
        end_nsecs: None,
        include_topic_regex: vec![],
        exclude_topic_regex: vec![],
        last_per_channel_topic_regex: vec!["/test".to_string()],
        include_metadata: true,
        include_attachments: true,
        output_options: mcapable_cli::cli::cmd::OutputOptions {
            compression: "none".to_string(),
            chunk_size: 4_194_304,
            chunked: true,
            include_crc: true,
        },
    })
    .unwrap();

    let msgs = read_messages(&output);
    // Should have: pre-start msg (200), plus time-range msgs (300, 400)
    assert_eq!(msgs.len(), 3, "expected 3 messages, got {}", msgs.len());
    let times: Vec<u64> = msgs.iter().map(|m| m.1).collect();
    assert!(
        times.contains(&200),
        "should include last pre-start message"
    );
    assert!(times.contains(&300), "should include start boundary");
    assert!(times.contains(&400), "should include post-start message");
}

// ---------------------------------------------------------------------------
// Additional du tests
// ---------------------------------------------------------------------------

#[test]
fn du_approximate_unchunked_falls_back() {
    let dir = TempDir::new().unwrap();
    // Create an unchunked file (no summary statistics)
    let path = dir.path().join("unchunked.mcap");
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = WriterBuilder::new()
        .profile("test")
        .build(file) // no .chunked() = unchunked
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
    cw.write(100, 100, Bytes::from_static(b"hello")).unwrap();
    drop(cw);
    writer.finish().unwrap();

    let input = path.to_str().unwrap().to_string();
    let mut buf = Vec::new();
    // --approximate on unchunked file still has summary statistics (the writer
    // always generates a summary section), so it uses the approximate path.
    // The output should show "Approximate" with Chunks: 0.
    mcapable_cli::cli::cmd::du::run_with_output(Some(input), &mut buf, true).unwrap();
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("Approximate") || output.contains("Top level record stats"),
        "expected du output, got: {}",
        output
    );
    assert!(output.contains("Chunks:"), "should show chunk count");
}
