use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, Write};
use std::path::PathBuf;

use crate::cli::input;

/// Extension trait to convert any `Result<T, E: Display>` into `Result<T, String>`.
///
/// Eliminates the pervasive `.map_err(|e| e.to_string())` pattern in CLI code.
pub(crate) trait CliResult<T> {
    fn cli(self) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> CliResult<T> for Result<T, E> {
    fn cli(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}

/// Shared output options for commands that write MCAP files.
#[derive(Debug, Clone, clap::Args)]
pub struct OutputOptions {
    /// Compression algorithm (zstd, lz4, none).
    #[arg(long, default_value = "zstd", value_parser = ["zstd", "lz4", "none"])]
    pub compression: String,

    /// Target chunk size in bytes.
    #[arg(long, default_value = "4194304")]
    pub chunk_size: usize,

    /// Produce chunked output.
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    pub chunked: bool,

    /// Include CRC checksums in chunks.
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    pub include_crc: bool,
}

impl OutputOptions {
    /// Convert to writer `ChunkOptions`. Returns `None` when `!self.chunked`.
    pub fn to_chunk_options(&self) -> Result<Option<mcapable_core::writer::ChunkOptions>, String> {
        if !self.chunked {
            return Ok(None);
        }
        let compression = parse_compression_option(&self.compression)?;
        Ok(Some(mcapable_core::writer::ChunkOptions {
            compression,
            max_uncompressed_bytes: self.chunk_size,
            include_crc: self.include_crc,
        }))
    }
}

/// Parse a compression string ("zstd", "lz4", "none") into an `Option<Compression>`.
pub(crate) fn parse_compression_option(
    s: &str,
) -> Result<Option<mcapable_core::Compression>, String> {
    match s {
        "zstd" => Ok(Some(mcapable_core::Compression::Zstd)),
        "lz4" => Ok(Some(mcapable_core::Compression::Lz4)),
        "none" => Ok(None),
        other => Err(format!("unknown compression: {other}")),
    }
}

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

#[derive(Debug, clap::Subcommand)]
pub enum GetCommand {
    /// Retrieve a specific message by index.
    Message {
        /// Index of the message to retrieve.
        #[arg(value_name = "index")]
        index: usize,

        /// Path to the MCAP file.
        #[arg(value_name = "file")]
        input: Option<String>,
    },
    /// Extract an attachment by name.
    Attachment {
        /// Path to the MCAP file.
        #[arg(value_name = "file")]
        input: Option<String>,

        /// Name of the attachment to extract.
        #[arg(long, short = 'n')]
        name: String,

        /// Offset of the attachment (for disambiguation).
        #[arg(long)]
        offset: Option<u64>,

        /// Output file path (stdout if not specified).
        #[arg(long, short = 'o')]
        output: Option<String>,
    },
    /// Retrieve metadata by name.
    Metadata {
        /// Path to the MCAP file.
        #[arg(value_name = "file")]
        input: Option<String>,

        /// Name of the metadata record to retrieve.
        #[arg(long, short = 'n')]
        name: String,
    },
}

#[derive(Debug, clap::Subcommand)]
pub enum AddCommand {
    /// Add an attachment to an MCAP file.
    Attachment {
        /// Input MCAP file.
        #[arg(value_name = "file")]
        input: Option<String>,

        /// Output MCAP file.
        #[arg(value_name = "output")]
        output: Option<String>,

        /// Path to the file to attach.
        #[arg(long, short = 'f')]
        file: std::path::PathBuf,

        /// Name for the attachment (defaults to filename).
        #[arg(long, short = 'n')]
        name: Option<String>,

        /// Content type of the attachment.
        #[arg(long, default_value = "application/octet-stream")]
        content_type: String,

        /// Log time (nanoseconds or RFC3339). Defaults to now.
        #[arg(long)]
        log_time: Option<String>,

        /// Creation time (nanoseconds or RFC3339). Defaults to file ctime.
        #[arg(long)]
        creation_time: Option<String>,

        #[command(flatten)]
        output_options: OutputOptions,
    },
    /// Add a metadata record to an MCAP file.
    Metadata {
        /// Input MCAP file.
        #[arg(value_name = "file")]
        input: Option<String>,

        /// Output MCAP file.
        #[arg(value_name = "output")]
        output: Option<String>,

        /// Name of the metadata record.
        #[arg(long, short = 'n')]
        name: String,

        /// Key-value pairs (repeatable, format: key=value).
        #[arg(long, short = 'k')]
        key: Vec<String>,

        #[command(flatten)]
        output_options: OutputOptions,
    },
}

pub mod add;
pub mod cat;
pub mod completion;
pub mod compress;
pub mod convert;
pub mod decompress;
pub mod doctor;
pub mod du;
pub mod filter;
pub mod generate;
pub mod get;
pub mod info;
pub mod list;
pub mod merge;
pub mod recover;
pub mod sort;
pub mod table;
pub mod version;

fn open_reader_with_validate_end_magic(
    input: String,
    validate_end_magic: bool,
) -> Result<mcapable_core::reader::Reader<Box<dyn mcapable_core::source::BytesSource>>, String> {
    let spec = input::InputSpec::parse(&input);
    let source = input::open_source(&spec).cli()?;
    let reader = mcapable_core::reader::Builder::new()
        .validate_end_magic(validate_end_magic)
        .build(source)
        .cli()?;
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
    include_crc: bool,
) -> Result<(), String> {
    rewrite_mcap(
        input,
        output,
        Some(mcapable_core::writer::ChunkOptions {
            compression,
            max_uncompressed_bytes,
            include_crc,
        }),
        |reader, writer, channel_writers| {
            copy_raw_messages(reader, writer, channel_writers, |_| true)
        },
    )
}

/// Open an input MCAP, create an output writer, copy schemas/channels/attachments/metadata,
/// then call `message_handler` for custom message processing, then finish.
///
/// This is the shared pipeline for commands that rewrite MCAP files (sort, convert, add, etc.).
pub(crate) fn rewrite_mcap(
    input: String,
    output: PathBuf,
    chunk_options: Option<mcapable_core::writer::ChunkOptions>,
    message_handler: impl FnOnce(
        &mut mcapable_core::reader::Reader<Box<dyn mcapable_core::source::BytesSource>>,
        &mut mcapable_core::writer::Writer<std::fs::File>,
        &mut HashMap<u16, mcapable_core::writer::ChannelWriter<std::fs::File>>,
    ) -> Result<(), String>,
) -> Result<(), String> {
    let mut reader = open_reader(input)?;
    let header = reader.header().cli()?;

    preload_schemas_and_channels(&mut reader)?;

    let out = std::fs::File::create(&output)
        .map_err(|e| format!("failed to create {}: {e}", output.display()))?;
    let mut writer = writer_from_header(out, &header, chunk_options)?;

    let mut channel_writers = write_schemas_and_channels(&reader, &mut writer)?;
    copy_attachments_and_metadata(&mut reader, &mut writer)?;

    message_handler(&mut reader, &mut writer, &mut channel_writers)?;

    writer.finish().cli()?;
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
    builder.build(out).cli()
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
        let _ = record.cli()?;
    }
    Ok(())
}

fn write_schemas_and_channels<R: mcapable_core::source::BytesSource, W: Write + Seek>(
    reader: &mcapable_core::reader::Reader<R>,
    writer: &mut mcapable_core::writer::Writer<W>,
) -> Result<HashMap<u16, mcapable_core::writer::ChannelWriter<W>>, String> {
    for schema in reader.schemas().values() {
        writer.copy_schema(schema).cli()?;
    }
    let mut channel_writers = HashMap::new();
    for channel in reader.channels().values() {
        let channel_writer = writer.copy_channel(channel).cli()?;
        channel_writers.insert(channel.id, channel_writer);
    }
    Ok(channel_writers)
}

fn copy_attachments_and_metadata<R: mcapable_core::source::BytesSource, W: Write + Seek>(
    reader: &mut mcapable_core::reader::Reader<R>,
    writer: &mut mcapable_core::writer::Writer<W>,
) -> Result<(), String> {
    copy_attachments_and_metadata_filtered(reader, writer, true, true, |_| true)
}

fn copy_attachments_and_metadata_filtered<R, W>(
    reader: &mut mcapable_core::reader::Reader<R>,
    writer: &mut mcapable_core::writer::Writer<W>,
    include_attachments: bool,
    include_metadata: bool,
    mut metadata_filter: impl FnMut(&mcapable_core::types::Metadata) -> bool,
) -> Result<(), String>
where
    R: mcapable_core::source::BytesSource,
    W: Write + Seek,
{
    for record in reader.records().filter(|op| {
        matches!(
            op,
            mcapable_core::Opcode::Attachment | mcapable_core::Opcode::Metadata
        )
    }) {
        match record.cli()? {
            mcapable_core::Record::Attachment(att) => {
                if include_attachments {
                    writer
                        .copy_attachment(
                            att.log_time,
                            att.create_time,
                            att.name,
                            att.media_type,
                            att.data,
                        )
                        .cli()?;
                }
            }
            mcapable_core::Record::Metadata(md) => {
                if include_metadata && metadata_filter(&md) {
                    writer.copy_metadata(&md).cli()?;
                }
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
    for msg in reader.raw_messages().cli()? {
        let msg = msg.cli()?;
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
            .cli()?;
    }
    Ok(())
}

pub(crate) fn parse_metadata_kv_pairs(
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
    reader.read_to_end(&mut bytes).cli()?;
    Ok(bytes)
}

pub(crate) fn now_ns() -> Result<u64, String> {
    let ts = chrono::Utc::now()
        .timestamp_nanos_opt()
        .ok_or_else(|| "failed to get current timestamp".to_string())?;
    u64::try_from(ts).map_err(|_| "current timestamp is negative".to_string())
}

/// Resolve a time argument from --start/--end (String, supports nanoseconds or RFC3339)
/// and --start-secs/--start-nsecs fallback.
pub(crate) fn resolve_time(
    primary: Option<String>,
    secs: Option<u64>,
    nsecs: Option<u32>,
) -> Result<Option<u64>, String> {
    if let Some(s) = primary {
        return Ok(Some(parse_timestamp_ns(&s)?));
    }
    if let Some(secs) = secs {
        let nsecs = nsecs.unwrap_or(0);
        if nsecs >= 1_000_000_000 {
            return Err("nsecs must be < 1_000_000_000".to_string());
        }
        return Ok(Some(
            secs.checked_mul(1_000_000_000)
                .and_then(|v| v.checked_add(nsecs as u64))
                .ok_or_else(|| "timestamp overflow".to_string())?,
        ));
    }
    Ok(None)
}

pub(crate) fn parse_timestamp_ns(s: &str) -> Result<u64, String> {
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

pub(crate) fn file_created_ns(path: &PathBuf) -> Result<u64, String> {
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

    // --- parse_compression_option ---

    #[test]
    fn parse_compression_option_zstd() {
        let result = parse_compression_option("zstd").unwrap();
        assert_eq!(result, Some(mcapable_core::Compression::Zstd));
    }

    #[test]
    fn parse_compression_option_lz4() {
        let result = parse_compression_option("lz4").unwrap();
        assert_eq!(result, Some(mcapable_core::Compression::Lz4));
    }

    #[test]
    fn parse_compression_option_none() {
        let result = parse_compression_option("none").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn parse_compression_option_unknown_is_err() {
        let result = parse_compression_option("brotli");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown compression"));
    }

    // --- parse_timestamp_ns ---

    #[test]
    fn parse_timestamp_ns_integer() {
        assert_eq!(parse_timestamp_ns("0").unwrap(), 0);
        assert_eq!(parse_timestamp_ns("1000000000").unwrap(), 1_000_000_000);
        assert_eq!(
            parse_timestamp_ns("18446744073709551615").unwrap(),
            u64::MAX
        );
    }

    #[test]
    fn parse_timestamp_ns_rfc3339_utc() {
        let ns = parse_timestamp_ns("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ns, 1_704_067_200_000_000_000);
    }

    #[test]
    fn parse_timestamp_ns_rfc3339_with_offset() {
        // 2024-06-15T12:30:00+05:30 = 2024-06-15T07:00:00Z
        let ns = parse_timestamp_ns("2024-06-15T12:30:00+05:30").unwrap();
        let expected = chrono::DateTime::parse_from_rfc3339("2024-06-15T07:00:00Z")
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64;
        assert_eq!(ns, expected);
    }

    #[test]
    fn parse_timestamp_ns_invalid_is_err() {
        assert!(parse_timestamp_ns("not a timestamp").is_err());
        assert!(parse_timestamp_ns("").is_err());
        assert!(parse_timestamp_ns("-1").is_err());
    }

    // --- resolve_time ---

    #[test]
    fn resolve_time_none_returns_none() {
        assert_eq!(resolve_time(None, None, None).unwrap(), None);
    }

    #[test]
    fn resolve_time_primary_integer() {
        assert_eq!(
            resolve_time(Some("1000000000".to_string()), None, None).unwrap(),
            Some(1_000_000_000)
        );
    }

    #[test]
    fn resolve_time_primary_rfc3339() {
        let result = resolve_time(Some("2024-01-01T00:00:00Z".to_string()), None, None).unwrap();
        assert_eq!(result, Some(1_704_067_200_000_000_000));
    }

    #[test]
    fn resolve_time_secs_and_nsecs() {
        assert_eq!(
            resolve_time(None, Some(1), Some(500_000_000)).unwrap(),
            Some(1_500_000_000)
        );
    }

    #[test]
    fn resolve_time_secs_without_nsecs() {
        assert_eq!(
            resolve_time(None, Some(2), None).unwrap(),
            Some(2_000_000_000)
        );
    }

    #[test]
    fn resolve_time_nsecs_overflow_is_err() {
        assert!(resolve_time(None, Some(1), Some(1_000_000_000)).is_err());
    }

    #[test]
    fn resolve_time_primary_takes_precedence_over_secs() {
        // When primary is provided, secs/nsecs are ignored
        let result = resolve_time(Some("5000".to_string()), Some(999), Some(999)).unwrap();
        assert_eq!(result, Some(5000));
    }

    #[test]
    fn resolve_time_invalid_primary_is_err() {
        assert!(resolve_time(Some("garbage".to_string()), None, None).is_err());
    }

    // --- format_bytes ---

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_below_kib() {
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_kib() {
        assert_eq!(format_bytes(1024), "1.00 KiB");
    }

    #[test]
    fn format_bytes_mib() {
        assert_eq!(format_bytes(1_048_576), "1.00 MiB");
    }

    #[test]
    fn format_bytes_gib() {
        assert_eq!(format_bytes(1_073_741_824), "1.00 GiB");
    }

    // --- parse_metadata_kv_pairs ---

    #[test]
    fn parse_metadata_kv_pairs_single() {
        let map = parse_metadata_kv_pairs(vec!["a=b".to_string()]).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(&mcapable_core::zero_copy::ByteStr::from("a")),
            Some(&mcapable_core::zero_copy::ByteStr::from("b"))
        );
    }

    #[test]
    fn parse_metadata_kv_pairs_value_with_equals() {
        // split_once only splits on first '=', so "a=b=c" -> ("a", "b=c")
        let map = parse_metadata_kv_pairs(vec!["a=b=c".to_string()]).unwrap();
        assert_eq!(
            map.get(&mcapable_core::zero_copy::ByteStr::from("a")),
            Some(&mcapable_core::zero_copy::ByteStr::from("b=c"))
        );
    }

    #[test]
    fn parse_metadata_kv_pairs_empty_vec() {
        let map = parse_metadata_kv_pairs(vec![]).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn parse_metadata_kv_pairs_missing_equals_is_err() {
        assert!(parse_metadata_kv_pairs(vec!["no_equals_here".to_string()]).is_err());
    }

    // --- now_ns ---

    #[test]
    fn now_ns_returns_reasonable_value() {
        let ns = now_ns().unwrap();
        // Should be after 2024-01-01 and before 2100-01-01
        assert!(ns > 1_704_067_200_000_000_000);
        assert!(ns < 4_102_444_800_000_000_000);
    }

    // --- file_created_ns ---

    #[test]
    fn file_created_ns_returns_recent_timestamp() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, b"hello").unwrap();

        let ns = file_created_ns(&path).unwrap();
        let now = now_ns().unwrap();
        // File was just created, so its timestamp should be within 5 seconds of now
        assert!(ns <= now, "file timestamp {ns} should be <= now {now}");
        assert!(
            now - ns < 5_000_000_000,
            "file timestamp {ns} should be within 5s of now {now}"
        );
    }

    #[test]
    fn file_created_ns_nonexistent_is_err() {
        let path = PathBuf::from("/nonexistent/path/file.txt");
        assert!(file_created_ns(&path).is_err());
    }
}
