use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, Write};
use std::path::PathBuf;

use crate::cli::input;

#[derive(Debug, clap::Subcommand)]
pub enum ListCommand {
    /// List schemas
    Schemas {
        #[arg(value_name = "file")]
        input: Option<String>,
    },
    /// List channels
    Channels {
        #[arg(value_name = "file")]
        input: Option<String>,
    },
    /// List metadata
    Metadata {
        #[arg(value_name = "file")]
        input: Option<String>,
    },
    /// List attachments
    Attachments {
        #[arg(value_name = "file")]
        input: Option<String>,
    },
    /// List chunks
    Chunks {
        #[arg(value_name = "file")]
        input: Option<String>,
    },
}

pub(crate) mod add;
pub(crate) mod cat;
pub(crate) mod completion;
pub(crate) mod compress;
pub(crate) mod convert;
pub(crate) mod decompress;
pub(crate) mod doctor;
pub(crate) mod du;
pub(crate) mod filter;
pub(crate) mod generate;
pub(crate) mod get;
pub(crate) mod info;
pub(crate) mod list;
pub(crate) mod merge;
pub(crate) mod recover;
pub(crate) mod sort;
pub(crate) mod table;
pub(crate) mod version;

fn open_reader_with_validate_end_magic(
    input: String,
    validate_end_magic: bool,
) -> Result<mcapable_core::reader::Reader<Box<dyn mcapable_core::source::BytesSource>>, String> {
    let spec = input::InputSpec::parse(&input);
    let source = input::open_source(&spec).map_err(|e| e.to_string())?;
    let reader = mcapable_core::reader::Builder::new()
        .validate_end_magic(validate_end_magic)
        .build(source)
        .map_err(|e| e.to_string())?;
    Ok(reader)
}

fn open_reader(
    input: String,
) -> Result<mcapable_core::reader::Reader<Box<dyn mcapable_core::source::BytesSource>>, String> {
    open_reader_with_validate_end_magic(input, true)
}

fn open_reader_allow_missing_end_magic(
    input: String,
) -> Result<mcapable_core::reader::Reader<Box<dyn mcapable_core::source::BytesSource>>, String> {
    open_reader_with_validate_end_magic(input, false)
}

fn rewrite_mcap_to_file(
    input: String,
    output: PathBuf,
    compression: Option<mcapable_core::Compression>,
    max_uncompressed_bytes: usize,
) -> Result<(), String> {
    let mut reader = open_reader(input)?;
    let header = reader.header().map_err(|e| e.to_string())?;

    preload_schemas_and_channels(&mut reader)?;

    let out = std::fs::File::create(&output)
        .map_err(|e| format!("failed to create {}: {e}", output.display()))?;
    let mut writer = writer_from_header(
        out,
        &header,
        Some(mcapable_core::writer::ChunkOptions {
            compression,
            max_uncompressed_bytes,
        }),
    )?;

    let mut channel_writers = write_schemas_and_channels(&reader, &mut writer)?;
    copy_attachments_and_metadata(&mut reader, &mut writer)?;
    copy_raw_messages(&mut reader, &mut writer, &mut channel_writers, |_| true)?;

    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn writer_from_header<W: Write + Seek>(
    out: W,
    header: &mcapable_core::Header,
    chunk_options: Option<mcapable_core::writer::ChunkOptions>,
) -> Result<mcapable_core::writer::Writer<W>, String> {
    let mut builder = mcapable_core::writer::WriterBuilder::new()
        .profile(header.profile.clone())
        .library(header.library.clone());
    for (k, v) in &header.metadata {
        builder = builder.header_metadata(k.clone(), v.clone());
    }
    if let Some(options) = chunk_options {
        builder = builder.chunked(options);
    }
    builder.build(out).map_err(|e| e.to_string())
}

fn preload_schemas_and_channels<R: mcapable_core::source::BytesSource>(
    reader: &mut mcapable_core::reader::Reader<R>,
) -> Result<(), String> {
    preload_records(reader, |op| {
        matches!(
            op,
            mcapable_core::Opcode::Schema | mcapable_core::Opcode::Channel
        )
    })
}

fn preload_records<R: mcapable_core::source::BytesSource>(
    reader: &mut mcapable_core::reader::Reader<R>,
    want: impl Fn(mcapable_core::Opcode) -> bool,
) -> Result<(), String> {
    for record in reader.records().filter(want) {
        let _ = record.map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_schemas_and_channels<R: mcapable_core::source::BytesSource, W: Write + Seek>(
    reader: &mcapable_core::reader::Reader<R>,
    writer: &mut mcapable_core::writer::Writer<W>,
) -> Result<HashMap<u16, mcapable_core::writer::ChannelWriter<W>>, String> {
    for schema in reader.schemas().values() {
        writer.copy_schema(schema).map_err(|e| e.to_string())?;
    }
    let mut channel_writers = HashMap::new();
    for channel in reader.channels().values() {
        let channel_writer = writer.copy_channel(channel).map_err(|e| e.to_string())?;
        channel_writers.insert(channel.id, channel_writer);
    }
    Ok(channel_writers)
}

fn copy_attachments_and_metadata<R: mcapable_core::source::BytesSource, W: Write + Seek>(
    reader: &mut mcapable_core::reader::Reader<R>,
    writer: &mut mcapable_core::writer::Writer<W>,
) -> Result<(), String> {
    for record in reader.records().filter(|op| {
        matches!(
            op,
            mcapable_core::Opcode::Attachment | mcapable_core::Opcode::Metadata
        )
    }) {
        match record.map_err(|e| e.to_string())? {
            mcapable_core::Record::Attachment(att) => writer
                .copy_attachment(
                    att.log_time,
                    att.create_time,
                    att.name,
                    att.media_type,
                    att.data,
                )
                .map_err(|e| e.to_string())?,
            mcapable_core::Record::Metadata(md) => {
                writer.copy_metadata(&md).map_err(|e| e.to_string())?
            }
            _ => {}
        }
    }
    Ok(())
}

fn copy_raw_messages<R: mcapable_core::source::BytesSource, W: Write + Seek>(
    reader: &mut mcapable_core::reader::Reader<R>,
    _writer: &mut mcapable_core::writer::Writer<W>,
    channel_writers: &mut HashMap<u16, mcapable_core::writer::ChannelWriter<W>>,
    mut allow: impl FnMut(&mcapable_core::RawMessage) -> bool,
) -> Result<(), String> {
    for msg in reader.raw_messages().map_err(|e| e.to_string())? {
        let msg = msg.map_err(|e| e.to_string())?;
        if !allow(&msg) {
            continue;
        }
        let Some(channel_writer) = channel_writers.get_mut(&msg.channel_id) else {
            return Err(format!("missing channel_id {}", msg.channel_id));
        };
        channel_writer
            .write_with_sequence(
                msg.log_time,
                msg.publish_time,
                msg.data_bytes(),
                msg.sequence,
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[allow(dead_code)]
fn parse_metadata_kv_pairs(
    kvs: Vec<String>,
) -> Result<HashMap<mcapable_core::zero_copy::ByteStr, mcapable_core::zero_copy::ByteStr>, String> {
    let mut map = HashMap::new();
    for kv in kvs {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| format!("invalid --key value (expected key=value): {kv}"))?;
        map.insert(k.to_string().into(), v.to_string().into());
    }
    Ok(map)
}

#[allow(dead_code)]
fn read_to_vec(reader: &mut dyn Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

#[allow(dead_code)]
fn now_ns() -> Result<u64, String> {
    let ts = chrono::Utc::now()
        .timestamp_nanos_opt()
        .ok_or_else(|| "failed to get current timestamp".to_string())?;
    u64::try_from(ts).map_err(|_| "current timestamp is negative".to_string())
}

#[allow(dead_code)]
fn parse_timestamp_ns(s: &str) -> Result<u64, String> {
    if let Ok(ns) = s.parse::<u64>() {
        return Ok(ns);
    }
    let dt = chrono::DateTime::parse_from_rfc3339(s)
        .map_err(|e| format!("invalid timestamp {s:?}: {e}"))?;
    let ts = dt
        .timestamp_nanos_opt()
        .ok_or_else(|| format!("timestamp out of range: {s:?}"))?;
    u64::try_from(ts).map_err(|_| format!("timestamp is negative: {s:?}"))
}

#[allow(dead_code)]
fn file_created_ns(path: &PathBuf) -> Result<u64, String> {
    use std::time::UNIX_EPOCH;
    let meta =
        std::fs::metadata(path).map_err(|e| format!("failed to stat {}: {e}", path.display()))?;
    let t = meta
        .created()
        .or_else(|_| meta.modified())
        .map_err(|e| format!("failed to get file time for {}: {e}", path.display()))?;
    let d = t
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("file time before unix epoch for {}: {e}", path.display()))?;
    let ns: u128 = d.as_nanos();
    u64::try_from(ns).map_err(|_| format!("file timestamp too large for {}", path.display()))
}

fn parse_topics(topics: Option<String>) -> Option<HashSet<String>> {
    let topics = topics?;
    let items: HashSet<String> = topics
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.2} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_topics_trims_and_ignores_empty() {
        assert!(parse_topics(None).is_none());
        assert!(parse_topics(Some(" , ".to_string())).is_none());

        let topics = parse_topics(Some(" /a, /b,,/c ".to_string())).unwrap();
        assert!(topics.contains("/a"));
        assert!(topics.contains("/b"));
        assert!(topics.contains("/c"));
    }
}
