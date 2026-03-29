use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use crate::cli::input;

use super::format_bytes;
use super::table::{render_table, TableData};

pub(crate) fn run(input: Option<String>) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    run_with_output(input, &mut stdout)
}

pub(crate) fn run_with_output<W: Write>(
    input: Option<String>,
    stdout: &mut W,
) -> Result<(), String> {
    let spec = input::InputSpec::parse(input.as_deref().unwrap_or("-"));
    let source = input::open_source(&spec).map_err(|e| e.to_string())?;

    let mut reader = mcapable_core::reader::Builder::new()
        .build(source)
        .map_err(|e| e.to_string())?;

    let summary_channels = reader
        .summary()
        .map_err(|e| e.to_string())?
        .map(|summary| summary.channels.clone())
        .unwrap_or_default();
    let (total_content_bytes, top_level, topics, total_msg_bytes) =
        du_stats_single_pass(&mut reader, summary_channels).map_err(|e| e.to_string())?;
    let total_file_bytes = total_content_bytes.saturating_add(16);

    writeln!(stdout, "Top level record stats:\n").map_err(|e| e.to_string())?;
    let table = build_top_level_table(total_file_bytes, &top_level);
    writeln!(stdout, "{}", render_table(&table)).map_err(|e| e.to_string())?;

    writeln!(stdout, "\nMessage size stats:\n").map_err(|e| e.to_string())?;
    let table = build_message_size_table(topics, total_msg_bytes);
    writeln!(stdout, "{}", render_table(&table)).map_err(|e| e.to_string())?;
    Ok(())
}

type DuStats = (
    u64,                                           // total_content_bytes
    HashMap<mcapable_core::Opcode, u64>,           // top_level record sizes
    Vec<(mcapable_core::zero_copy::ByteStr, u64)>, // topics with sizes
    u64,                                           // total_msg_bytes
);

fn du_stats_single_pass(
    reader: &mut mcapable_core::reader::Reader<Box<dyn mcapable_core::source::BytesSource>>,
    summary_channels: Arc<HashMap<u16, mcapable_core::Channel>>,
) -> Result<DuStats, mcapable_core::Error> {
    let mut totals: HashMap<mcapable_core::Opcode, u64> = HashMap::new();
    let mut total_bytes: u64 = 0;
    let mut by_channel_id: HashMap<u16, u64> = HashMap::new();
    let mut total_msg_bytes: u64 = 0;

    let mut stream = reader
        .record_metadata()
        .include_chunk_messages()
        .include_message_metadata();
    if summary_channels.is_empty() {
        stream = stream.include_channel_metadata();
    }

    for rec in stream {
        let rec = rec?;
        if rec.source == mcapable_core::RecordSource::File {
            total_bytes = total_bytes.saturating_add(rec.length);
            *totals.entry(rec.opcode).or_insert(0) += rec.length;
        }
        if let Some(msg) = rec.message {
            let size = msg.data_size;
            total_msg_bytes = total_msg_bytes.saturating_add(size);
            let entry = by_channel_id.entry(msg.channel_id).or_insert(0);
            *entry = entry.saturating_add(size);
        }
    }

    let channels = if summary_channels.is_empty() {
        reader.channels()
    } else {
        summary_channels
    };

    let mut by_topic: HashMap<mcapable_core::zero_copy::ByteStr, (u64, u16)> = HashMap::new();
    for (channel_id, bytes) in by_channel_id {
        let Some(ch) = channels.get(&channel_id) else {
            continue;
        };
        let entry = by_topic.entry(ch.topic.clone()).or_insert((0, channel_id));
        entry.0 = entry.0.saturating_add(bytes);
        if channel_id < entry.1 {
            entry.1 = channel_id;
        }
    }

    let mut topics: Vec<(mcapable_core::zero_copy::ByteStr, u64, u16)> = by_topic
        .into_iter()
        .map(|(topic, (bytes, min_channel_id))| (topic, bytes, min_channel_id))
        .collect();
    topics.sort_by(
        |(a_topic, a_bytes, a_min_id), (b_topic, b_bytes, b_min_id)| {
            b_bytes
                .cmp(a_bytes)
                .then_with(|| a_min_id.cmp(b_min_id))
                .then_with(|| a_topic.cmp(b_topic))
        },
    );
    let topics = topics
        .into_iter()
        .map(|(topic, bytes, _)| (topic, bytes))
        .collect();

    Ok((total_bytes, totals, topics, total_msg_bytes))
}

fn du_record_order() -> &'static [Option<mcapable_core::Opcode>] {
    use mcapable_core::Opcode;
    &[
        Some(Opcode::Header),
        Some(Opcode::Metadata),
        Some(Opcode::MessageIndex),
        Some(Opcode::DataEnd),
        Some(Opcode::Channel),
        Some(Opcode::Statistics),
        None, // unknown
        Some(Opcode::SummaryOffset),
        Some(Opcode::Chunk),
        Some(Opcode::Schema),
        Some(Opcode::ChunkIndex),
        Some(Opcode::Footer),
        Some(Opcode::Attachment),
        Some(Opcode::AttachmentIndex),
        Some(Opcode::Message),
    ]
}

fn build_top_level_table(
    total_file_bytes: u64,
    top_level: &HashMap<mcapable_core::Opcode, u64>,
) -> TableData {
    let mut known_ops = std::collections::HashSet::new();
    for &opcode_opt in du_record_order() {
        if let Some(op) = opcode_opt {
            known_ops.insert(op);
        }
    }
    let unknown_bytes: u64 = top_level
        .iter()
        .filter(|(op, _)| !known_ops.contains(op))
        .map(|(_, bytes)| *bytes)
        .sum();

    let mut rows = Vec::new();
    rows.push(vec![
        "------".to_string(),
        "---------".to_string(),
        "---------------------".to_string(),
    ]);

    for &opcode_opt in du_record_order() {
        let (label, sum_bytes) = match opcode_opt {
            Some(op) => match top_level.get(&op).copied() {
                Some(v) => (op.to_string(), v),
                None => continue,
            },
            None => {
                if unknown_bytes == 0 {
                    continue;
                }
                ("unknown".to_string(), unknown_bytes)
            }
        };
        let pct = if total_file_bytes == 0 {
            0.0f32
        } else {
            (sum_bytes as f32) / (total_file_bytes as f32) * 100.0
        };
        let pct = format!("{pct:.6}");
        rows.push(vec![label, sum_bytes.to_string(), pct]);
    }

    TableData::new(
        vec![
            "record".to_string(),
            "sum bytes".to_string(),
            "% of total file bytes".to_string(),
        ],
        rows,
    )
}

fn build_message_size_table(
    topics: Vec<(mcapable_core::zero_copy::ByteStr, u64)>,
    total_msg_bytes: u64,
) -> TableData {
    let mut rows = Vec::new();
    rows.push(vec![
        "-----".to_string(),
        "------------------------".to_string(),
        "---------------------------------------".to_string(),
    ]);
    for (topic, sum_bytes) in topics {
        let pct = if total_msg_bytes == 0 {
            0.0f32
        } else {
            (sum_bytes as f32) / (total_msg_bytes as f32) * 100.0
        };
        let pct = format!("{pct:.6}");
        rows.push(vec![
            topic.as_str().to_string(),
            format_bytes(sum_bytes),
            pct,
        ]);
    }
    TableData::new(
        vec![
            "topic".to_string(),
            "sum bytes (uncompressed)".to_string(),
            "% of total message bytes (uncompressed)".to_string(),
        ],
        rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_level_table_includes_unknown_when_present() {
        let mut totals = HashMap::new();
        totals.insert(mcapable_core::Opcode::Header, 10);
        totals.insert(mcapable_core::Opcode::Message, 20);
        totals.insert(mcapable_core::Opcode::DataEnd, 1);
        let table = build_top_level_table(31, &totals);
        assert!(table.rows.iter().any(|row| row[0] == "header"));
        assert!(table.rows.iter().any(|row| row[0] == "message"));
        assert!(table.rows.iter().all(|row| row.len() == 3));
    }

    #[test]
    fn message_size_table_formats_percent() {
        let topics = vec![("topic".into(), 100)];
        let table = build_message_size_table(topics, 200);
        assert_eq!(
            table.headers,
            vec![
                "topic",
                "sum bytes (uncompressed)",
                "% of total message bytes (uncompressed)"
            ]
        );
        assert_eq!(table.rows[1][2], "50.000000");
    }
}
