use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;

use super::table::{render_table, TableData};
use super::{open_reader, ListCommand};

pub(crate) fn dispatch(command: ListCommand) -> Result<(), String> {
    run(command)
}

pub(crate) fn run(command: ListCommand) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    run_with_output(command, &mut stdout)
}

pub(crate) fn run_with_output<W: Write>(
    command: ListCommand,
    stdout: &mut W,
) -> Result<(), String> {
    match command {
        ListCommand::Schemas { input } => {
            let mut reader = open_reader(input.unwrap_or_else(|| "-".to_string()))?;
            let summary_schemas = reader
                .summary()
                .map_err(|e| e.to_string())?
                .map(|summary| summary.schemas.clone())
                .unwrap_or_default();
            let schemas = if summary_schemas.is_empty() {
                let mut out = HashMap::new();
                for schema in reader.data_section_schemas().map_err(|e| e.to_string())? {
                    out.insert(schema.id, schema);
                }
                Cow::Owned(out)
            } else {
                Cow::Borrowed(summary_schemas.as_ref())
            };
            let table = build_schema_table(schemas.as_ref());
            writeln!(stdout, "{}", render_table(&table)).map_err(|e| e.to_string())?;
        }
        ListCommand::Channels { input } => {
            let mut reader = open_reader(input.unwrap_or_else(|| "-".to_string()))?;
            let summary_channels = reader
                .summary()
                .map_err(|e| e.to_string())?
                .map(|summary| summary.channels.clone())
                .unwrap_or_default();
            let channels = if summary_channels.is_empty() {
                let mut out = HashMap::new();
                for channel in reader.data_section_channels().map_err(|e| e.to_string())? {
                    out.insert(channel.id, channel);
                }
                Cow::Owned(out)
            } else {
                Cow::Borrowed(summary_channels.as_ref())
            };
            let table = build_channel_table(channels.as_ref());
            writeln!(stdout, "{}", render_table(&table)).map_err(|e| e.to_string())?;
        }
        ListCommand::Metadata { input } => {
            let input_value = input.unwrap_or_else(|| "-".to_string());
            let mut reader = open_reader(input_value)?;
            let entries = reader.metadata_entries().map_err(|e| e.to_string())?;
            let rows = entries.into_iter().map(|entry| {
                let metadata_str = format_metadata_map(&entry.metadata.metadata);
                (
                    entry.metadata.name.as_str().to_string(),
                    entry.offset,
                    entry.length,
                    metadata_str,
                )
            });
            let table = build_metadata_table(rows);
            writeln!(stdout, "{}", render_table(&table)).map_err(|e| e.to_string())?;
        }
        ListCommand::Attachments { input } => {
            let input_value = input.unwrap_or_else(|| "-".to_string());
            let mut reader = open_reader(input_value)?;
            let entries = reader.attachment_entries().map_err(|e| e.to_string())?;
            let rows = entries.into_iter().map(|entry| {
                (
                    entry.name.as_str().to_string(),
                    entry.media_type.as_str().to_string(),
                    entry.log_time,
                    entry.create_time,
                    entry.data_size,
                    entry.offset,
                )
            });
            let table = build_attachment_table(rows);
            writeln!(stdout, "{}", render_table(&table)).map_err(|e| e.to_string())?;
        }
        ListCommand::Chunks { input } => {
            let input_value = input.unwrap_or_else(|| "-".to_string());
            let mut reader = open_reader(input_value)?;
            let indexes = reader.chunk_indexes().map_err(|e| e.to_string())?;
            let footer = reader.footer().map_err(|e| e.to_string())?;
            let summary_present = footer
                .as_ref()
                .map(|footer| footer.summary_start != 0)
                .unwrap_or(false);
            let mut rows = Vec::new();
            if !indexes.is_empty() {
                for idx in indexes.iter() {
                    let compressed_size = idx.compressed_size().map_err(|e| e.to_string())?;
                    let ratio = if idx.uncompressed_size == 0 {
                        0.0f32
                    } else {
                        (compressed_size as f32) / (idx.uncompressed_size as f32)
                    };
                    rows.push(ChunkRow {
                        offset: idx.chunk_start_offset.to_string(),
                        length: idx.chunk_length.to_string(),
                        start: idx.message_start_time.to_string(),
                        end: idx.message_end_time.to_string(),
                        compression: idx.compression.as_ref().to_string(),
                        compressed_size: compressed_size.to_string(),
                        uncompressed_size: idx.uncompressed_size.to_string(),
                        ratio: format!("{ratio:.6}"),
                        message_index_length: idx.message_index_length.to_string(),
                    });
                }
            } else if summary_present {
                return Ok(());
            } else {
                for chunk in reader.chunks() {
                    let chunk = chunk.map_err(|e| e.to_string())?;
                    let ratio = if chunk.uncompressed_size == 0 {
                        0.0f32
                    } else {
                        (chunk.records.len() as f32) / (chunk.uncompressed_size as f32)
                    };
                    rows.push(ChunkRow {
                        offset: "0".to_string(),
                        length: "0".to_string(),
                        start: chunk.message_start_time.to_string(),
                        end: chunk.message_end_time.to_string(),
                        compression: chunk.compression.as_ref().to_string(),
                        compressed_size: chunk.records.len().to_string(),
                        uncompressed_size: chunk.uncompressed_size.to_string(),
                        ratio: format!("{ratio:.6}"),
                        message_index_length: "0".to_string(),
                    });
                }
            }
            let table = build_chunk_table(rows);
            writeln!(stdout, "{}", render_table(&table)).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn format_metadata_map(
    metadata: &HashMap<mcapable_core::zero_copy::ByteStr, mcapable_core::zero_copy::ByteStr>,
) -> String {
    let mut out = String::from("{");
    let mut first = true;
    for (k, v) in metadata {
        if !first {
            out.push(',');
        }
        first = false;
        out.push('"');
        escape_json_str(&mut out, k.as_ref());
        out.push('"');
        out.push(':');
        out.push('"');
        escape_json_str(&mut out, v.as_ref());
        out.push('"');
    }
    out.push('}');
    out
}

fn escape_json_str(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
}

#[derive(Debug, Clone)]
struct ChunkRow {
    offset: String,
    length: String,
    start: String,
    end: String,
    compression: String,
    compressed_size: String,
    uncompressed_size: String,
    ratio: String,
    message_index_length: String,
}

fn build_schema_table(schemas: &HashMap<u16, mcapable_core::Schema>) -> TableData {
    let mut rows = Vec::new();
    for schema in schemas.values() {
        let id = schema.id.to_string();
        let name = schema.name.as_ref().to_string();
        let encoding = schema.encoding.as_ref().to_string();
        let data = String::from_utf8_lossy(schema.data.as_ref());
        let lines: Vec<String> = data
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect();
        let mut iter = lines.into_iter();
        let first = iter.next().unwrap_or_default();
        rows.push(vec![id, name, encoding, first]);
        for line in iter {
            rows.push(vec![String::new(), String::new(), String::new(), line]);
        }
    }

    TableData::new(
        vec![
            "id".to_string(),
            "name".to_string(),
            "encoding".to_string(),
            "data".to_string(),
        ],
        rows,
    )
}

fn build_channel_table(channels: &HashMap<u16, mcapable_core::Channel>) -> TableData {
    let mut rows = Vec::new();
    for channel in channels.values() {
        rows.push(vec![
            channel.id.to_string(),
            channel.schema_id.to_string(),
            channel.topic.as_ref().to_string(),
            channel.message_encoding.as_ref().to_string(),
            format_metadata_map(&channel.metadata),
        ]);
    }
    TableData::new(
        vec![
            "id".to_string(),
            "schemaId".to_string(),
            "topic".to_string(),
            "messageEncoding".to_string(),
            "metadata".to_string(),
        ],
        rows,
    )
}

fn build_metadata_table<I>(rows: I) -> TableData
where
    I: IntoIterator<Item = (String, u64, u64, String)>,
{
    let rows = rows
        .into_iter()
        .map(|(name, offset, length, metadata)| {
            vec![name, offset.to_string(), length.to_string(), metadata]
        })
        .collect();
    TableData::new(
        vec![
            "name".to_string(),
            "offset".to_string(),
            "length".to_string(),
            "metadata".to_string(),
        ],
        rows,
    )
}

fn build_attachment_table<I>(rows: I) -> TableData
where
    I: IntoIterator<Item = (String, String, u64, u64, u64, u64)>,
{
    let rows = rows
        .into_iter()
        .map(
            |(name, media_type, log_time, create_time, content_len, offset)| {
                vec![
                    name,
                    media_type,
                    log_time.to_string(),
                    create_time.to_string(),
                    content_len.to_string(),
                    offset.to_string(),
                ]
            },
        )
        .collect();
    TableData::new(
        vec![
            "name".to_string(),
            "media type".to_string(),
            "log time".to_string(),
            "creation time".to_string(),
            "content length".to_string(),
            "offset".to_string(),
        ],
        rows,
    )
}

fn build_chunk_table(rows: Vec<ChunkRow>) -> TableData {
    let rows = rows
        .into_iter()
        .map(|row| {
            vec![
                row.offset,
                row.length,
                row.start,
                row.end,
                row.compression,
                row.compressed_size,
                row.uncompressed_size,
                row.ratio,
                row.message_index_length,
            ]
        })
        .collect();
    TableData::new(
        vec![
            "offset".to_string(),
            "length".to_string(),
            "start".to_string(),
            "end".to_string(),
            "compression".to_string(),
            "compressed size".to_string(),
            "uncompressed size".to_string(),
            "compression ratio".to_string(),
            "message index length".to_string(),
        ],
        rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_table_splits_multiline_data() {
        let mut schemas = HashMap::new();
        schemas.insert(
            1,
            mcapable_core::Schema {
                id: 1,
                name: "test".into(),
                encoding: "ros1msg".into(),
                data: "line1\nline2\r\nline3".as_bytes().into(),
            },
        );
        let table = build_schema_table(&schemas);
        assert_eq!(table.headers, vec!["id", "name", "encoding", "data"]);
        assert_eq!(
            table.rows,
            vec![
                vec![
                    "1".to_string(),
                    "test".to_string(),
                    "ros1msg".to_string(),
                    "line1".to_string()
                ],
                vec![
                    String::new(),
                    String::new(),
                    String::new(),
                    "line2".to_string()
                ],
                vec![
                    String::new(),
                    String::new(),
                    String::new(),
                    "line3".to_string()
                ],
            ]
        );
    }

    #[test]
    fn channel_table_includes_metadata() {
        let mut channels = HashMap::new();
        let mut metadata = HashMap::new();
        metadata.insert("k\"".into(), "v\\".into());
        channels.insert(
            1,
            mcapable_core::Channel {
                id: 1,
                schema_id: 2,
                topic: "/topic".into(),
                message_encoding: "cdr".into(),
                metadata,
            },
        );
        let table = build_channel_table(&channels);
        assert_eq!(
            table.rows[0],
            vec![
                "1".to_string(),
                "2".to_string(),
                "/topic".to_string(),
                "cdr".to_string(),
                "{\"k\\\"\":\"v\\\\\"}".to_string()
            ]
        );
    }

    #[test]
    fn metadata_table_formats_rows() {
        let table = build_metadata_table(vec![(
            "name".to_string(),
            12,
            34,
            "{\"a\":\"b\"}".to_string(),
        )]);
        assert_eq!(table.headers, vec!["name", "offset", "length", "metadata"]);
        assert_eq!(
            table.rows,
            vec![vec![
                "name".to_string(),
                "12".to_string(),
                "34".to_string(),
                "{\"a\":\"b\"}".to_string()
            ]]
        );
    }

    #[test]
    fn attachment_table_formats_rows() {
        let table = build_attachment_table(vec![(
            "file".to_string(),
            "application/octet-stream".to_string(),
            10,
            20,
            30,
            40,
        )]);
        assert_eq!(
            table.headers,
            vec![
                "name",
                "media type",
                "log time",
                "creation time",
                "content length",
                "offset"
            ]
        );
        assert_eq!(
            table.rows,
            vec![vec![
                "file".to_string(),
                "application/octet-stream".to_string(),
                "10".to_string(),
                "20".to_string(),
                "30".to_string(),
                "40".to_string()
            ]]
        );
    }

    #[test]
    fn chunk_table_formats_rows() {
        let table = build_chunk_table(vec![ChunkRow {
            offset: "1".to_string(),
            length: "2".to_string(),
            start: "3".to_string(),
            end: "4".to_string(),
            compression: "none".to_string(),
            compressed_size: "5".to_string(),
            uncompressed_size: "6".to_string(),
            ratio: "0.123000".to_string(),
            message_index_length: "7".to_string(),
        }]);
        assert_eq!(
            table.headers,
            vec![
                "offset",
                "length",
                "start",
                "end",
                "compression",
                "compressed size",
                "uncompressed size",
                "compression ratio",
                "message index length"
            ]
        );
        assert_eq!(
            table.rows,
            vec![vec![
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "none".to_string(),
                "5".to_string(),
                "6".to_string(),
                "0.123000".to_string(),
                "7".to_string()
            ]]
        );
    }
}
