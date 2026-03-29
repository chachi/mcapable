use std::collections::HashMap;
use std::io::Write;

use super::{format_bytes, open_reader};

pub(crate) fn run(input: Option<String>) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    run_with_output(input, &mut stdout)
}

pub(crate) fn run_with_output<W: Write>(
    input: Option<String>,
    stdout: &mut W,
) -> Result<(), String> {
    let mut reader = open_reader(input.unwrap_or_else(|| "-".to_string()))?;

    let header = reader.header().map_err(|e| e.to_string())?;
    let summary = reader
        .summary()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "missing summary section (try `mcapable reindex`)".to_string())?;

    let schemas = summary.schemas.clone();
    let channels = summary.channels.clone();

    let (message_summary, per_channel_counts) = summarize_messages_from_summary(&summary);
    let (chunk_stats, compression_stats) = gather_chunk_stats_from_summary(&summary)?;

    print_field_padded(stdout, "library", header.library.as_str())?;
    print_field_padded(stdout, "profile", header.profile.as_str())?;
    print_field_padded(
        stdout,
        "messages",
        &format!("{}", message_summary.total_messages),
    )?;
    print_field_padded(
        stdout,
        "duration",
        &format_duration_ns(
            message_summary
                .end_ns
                .saturating_sub(message_summary.start_ns),
        ),
    )?;
    print_field_padded(
        stdout,
        "start",
        &format_time_with_epoch(message_summary.start_ns),
    )?;
    print_field_padded(
        stdout,
        "end",
        &format_time_with_epoch(message_summary.end_ns),
    )?;

    writeln!(stdout, "compression:").map_err(|e| e.to_string())?;
    for (compression, stats) in compression_stats {
        println_fixed(
            stdout,
            67,
            &format!(
                "\t{}: [{}/{} chunks] [{}/{} ({})] [{}] ",
                compression,
                stats.chunk_count,
                chunk_stats.total_chunks,
                format_bytes(stats.total_uncompressed),
                format_bytes(stats.total_compressed),
                format_percent_savings(stats.total_uncompressed, stats.total_compressed),
                format_bytes_per_sec(
                    stats.total_compressed,
                    message_summary.start_ns,
                    message_summary.end_ns
                ),
            ),
        )?;
    }

    writeln!(stdout, "chunks:").map_err(|e| e.to_string())?;
    writeln!(
        stdout,
        "\tmax uncompressed size: {}",
        format_bytes(chunk_stats.max_uncompressed)
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        stdout,
        "\tmax compressed size: {}",
        format_bytes(chunk_stats.max_compressed)
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        stdout,
        "\toverlaps: {}",
        if chunk_stats.overlaps { "yes" } else { "no" }
    )
    .map_err(|e| e.to_string())?;

    writeln!(stdout, "channels:").map_err(|e| e.to_string())?;
    print_channels(
        stdout,
        &channels,
        &schemas,
        &per_channel_counts,
        message_summary.start_ns,
        message_summary.end_ns,
    )?;

    writeln!(stdout, "channels: {}", channels.len()).map_err(|e| e.to_string())?;
    let (attachment_count, metadata_count) = if let Some(stats) = summary.statistics.as_deref() {
        (stats.attachment_count, stats.metadata_count)
    } else {
        (
            summary.attachment_indexes.len() as u32,
            summary.metadata_indexes.len() as u32,
        )
    };
    writeln!(stdout, "attachments: {attachment_count}").map_err(|e| e.to_string())?;
    writeln!(stdout, "metadata: {metadata_count}").map_err(|e| e.to_string())?;

    Ok(())
}

fn print_field_padded<W: Write>(stdout: &mut W, label: &str, value: &str) -> Result<(), String> {
    let prefix = format!("{label}:");
    let pad = 11usize.saturating_sub(prefix.len());
    println_fixed(stdout, 71, &format!("{prefix}{}{value}", " ".repeat(pad)))
}

fn println_fixed<W: Write>(stdout: &mut W, width: usize, s: &str) -> Result<(), String> {
    writeln!(stdout, "{}", pad_to_width(width, s)).map_err(|e| e.to_string())
}

fn pad_to_width(width: usize, s: &str) -> String {
    if s.len() >= width {
        return s.to_string();
    }
    let pad = width - s.len();
    format!("{s}{}", " ".repeat(pad))
}

fn format_duration_ns(duration_ns: u64) -> String {
    let secs = duration_ns / 1_000_000_000;
    let nsecs = duration_ns % 1_000_000_000;
    let mins = secs / 60;
    let secs = secs % 60;
    if mins > 0 {
        format!("{mins}m{secs}.{nsecs:09}s")
    } else {
        format!("{secs}.{nsecs:09}s")
    }
}

fn format_time_with_epoch(ts_ns: u64) -> String {
    use chrono::{Local, TimeZone};
    let secs: i64 = (ts_ns / 1_000_000_000) as i64;
    let nsecs: u32 = (ts_ns % 1_000_000_000) as u32;
    let dt = Local
        .timestamp_opt(secs, nsecs)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).single().unwrap());
    let rfc3339 = dt.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
    format!("{rfc3339} ({secs}.{nsecs:09})")
}

fn format_percent_savings(uncompressed: u64, compressed: u64) -> String {
    if uncompressed == 0 {
        return "0.00%".to_string();
    }
    let u = uncompressed as f64;
    let c = compressed as f64;
    let pct = (1.0 - (c / u)) * 100.0;
    format!("{pct:.2}%")
}

fn format_bytes_per_sec(bytes: u64, start_ns: u64, end_ns: u64) -> String {
    let dur_s = (end_ns.saturating_sub(start_ns) as f64) / 1_000_000_000.0;
    if dur_s <= 0.0 {
        return "0.00 B/sec".to_string();
    }
    let bps = (bytes as f64) / dur_s;

    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bps >= GIB {
        format!("{:.2} GiB/sec", bps / GIB)
    } else if bps >= MIB {
        format!("{:.2} MiB/sec", bps / MIB)
    } else if bps >= KIB {
        format!("{:.2} KiB/sec", bps / KIB)
    } else {
        format!("{bps:.2} B/sec")
    }
}

#[derive(Debug, Default)]
struct ChunkStats {
    total_chunks: u64,
    max_uncompressed: u64,
    max_compressed: u64,
    overlaps: bool,
}

#[derive(Debug, Default, Clone)]
struct CompressionStats {
    chunk_count: u64,
    total_uncompressed: u64,
    total_compressed: u64,
}

#[derive(Debug, Clone, Copy)]
struct MessageSummary {
    total_messages: u64,
    start_ns: u64,
    end_ns: u64,
}

fn summarize_messages_from_summary(
    summary: &mcapable_core::Summary,
) -> (MessageSummary, HashMap<u16, u64>) {
    let mut per_channel: HashMap<u16, u64> = HashMap::new();

    let Some(stats) = summary.statistics.as_deref() else {
        return (
            MessageSummary {
                total_messages: 0,
                start_ns: 0,
                end_ns: 0,
            },
            per_channel,
        );
    };

    for count in &stats.channel_message_counts {
        per_channel.insert(count.channel_id, count.message_count);
    }

    (
        MessageSummary {
            total_messages: stats.message_count,
            start_ns: stats.message_start_time,
            end_ns: stats.message_end_time,
        },
        per_channel,
    )
}

fn chunk_compressed_size_from_index(ci: &mcapable_core::ChunkIndex) -> Result<u64, String> {
    ci.compressed_size().map_err(|e| e.to_string())
}

fn gather_chunk_stats_from_summary(
    summary: &mcapable_core::Summary,
) -> Result<(ChunkStats, HashMap<String, CompressionStats>), String> {
    let mut out = ChunkStats::default();
    let mut per_compression: HashMap<String, CompressionStats> = HashMap::new();
    let mut prev_end: Option<u64> = None;

    let mut chunk_indexes = summary.chunk_indexes.as_ref().to_vec();
    chunk_indexes.sort_by_key(|c| c.chunk_start_offset);

    for idx in &chunk_indexes {
        out.total_chunks += 1;
        out.max_uncompressed = out.max_uncompressed.max(idx.uncompressed_size);
        let compressed_size = chunk_compressed_size_from_index(idx)?;
        out.max_compressed = out.max_compressed.max(compressed_size);

        if let Some(prev_end) = prev_end {
            if idx.message_start_time < prev_end {
                out.overlaps = true;
            }
        }
        prev_end = Some(idx.message_end_time);

        let entry = per_compression
            .entry(idx.compression.as_str().to_string())
            .or_default();
        entry.chunk_count += 1;
        entry.total_uncompressed += idx.uncompressed_size;
        entry.total_compressed += compressed_size;
    }

    Ok((out, per_compression))
}

fn print_channels<W: Write>(
    stdout: &mut W,
    channels: &std::sync::Arc<HashMap<u16, mcapable_core::Channel>>,
    schemas: &std::sync::Arc<HashMap<u16, mcapable_core::Schema>>,
    per_channel_counts: &HashMap<u16, u64>,
    start_ns: u64,
    end_ns: u64,
) -> Result<(), String> {
    for row in build_channel_rows(channels, schemas, per_channel_counts, start_ns, end_ns) {
        let id_sep = if row.id < 10 { "  " } else { " " };
        let hz_field = format!("({:.2} Hz)", row.hz);
        println_fixed(
            stdout,
            122,
            &format!(
                "\t({}){}{:<40} {:>6} msgs {:<11}   : {} [{}]",
                row.id,
                id_sep,
                row.topic,
                row.message_count,
                hz_field,
                row.schema_name,
                row.encoding
            ),
        )?;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
struct ChannelRow {
    id: u16,
    topic: String,
    message_count: u64,
    hz: f64,
    schema_name: String,
    encoding: String,
}

fn build_channel_rows(
    channels: &HashMap<u16, mcapable_core::Channel>,
    schemas: &HashMap<u16, mcapable_core::Schema>,
    per_channel_counts: &HashMap<u16, u64>,
    start_ns: u64,
    end_ns: u64,
) -> Vec<ChannelRow> {
    let duration_s = if end_ns > start_ns {
        (end_ns - start_ns) as f64 / 1_000_000_000.0
    } else {
        0.0
    };

    let mut rows = Vec::new();
    for (id, ch) in channels.iter() {
        let (schema_name, encoding) = if ch.schema_id == 0 {
            ("", ch.message_encoding.as_str())
        } else {
            let schema = schemas.get(&ch.schema_id);
            (
                schema.map(|s| s.name.as_str()).unwrap_or(""),
                schema.map(|s| s.encoding.as_str()).unwrap_or(""),
            )
        };
        let message_count = *per_channel_counts.get(id).unwrap_or(&0);
        let hz = if duration_s > 0.0 {
            (message_count as f64) / duration_s
        } else {
            0.0
        };
        rows.push(ChannelRow {
            id: *id,
            topic: ch.topic.as_ref().to_string(),
            message_count,
            hz,
            schema_name: schema_name.to_string(),
            encoding: encoding.to_string(),
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_to_width_extends_to_requested_length() {
        let out = pad_to_width(5, "ab");
        assert_eq!(out.len(), 5);
        assert_eq!(&out[..2], "ab");

        let out = pad_to_width(2, "abcd");
        assert_eq!(out, "abcd");
    }

    #[test]
    fn format_bps_floors_like_mcap() {
        // 101 bytes over 10 seconds -> 10.1 B/sec -> floor -> 10.00
        assert_eq!(format_bytes_per_sec(101, 0, 10_000_000_000), "10.10 B/sec");
    }

    #[test]
    fn build_channel_rows_uses_schema_info() {
        let mut channels = HashMap::new();
        channels.insert(
            1,
            mcapable_core::Channel {
                id: 1,
                schema_id: 2,
                topic: "/topic".into(),
                message_encoding: "cdr".into(),
                metadata: HashMap::new(),
            },
        );
        let mut schemas = HashMap::new();
        schemas.insert(
            2,
            mcapable_core::Schema {
                id: 2,
                name: "schema".into(),
                encoding: "ros1msg".into(),
                data: Vec::new().into(),
            },
        );
        let mut counts = HashMap::new();
        counts.insert(1, 10);

        let rows = build_channel_rows(&channels, &schemas, &counts, 0, 1_000_000_000);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].schema_name, "schema");
        assert_eq!(rows[0].encoding, "ros1msg");
        assert_eq!(rows[0].message_count, 10);
        assert!((rows[0].hz - 10.0).abs() < 0.0001);
    }
}
