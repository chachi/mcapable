use mcapable_core::collections::HashMap;
use std::io::Write;

use super::{open_reader, parse_topics, preload_schemas_and_channels, resolve_time, CliResult};

pub struct CatOptions {
    pub input: Option<String>,
    pub topics: Vec<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub start_secs: Option<u64>,
    pub start_nsecs: Option<u32>,
    pub end_secs: Option<u64>,
    pub end_nsecs: Option<u32>,
    pub json: bool,
}

pub fn run(opts: CatOptions) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    run_with_output(opts, &mut stdout)
}

pub fn run_with_output<W: Write>(opts: CatOptions, out: &mut W) -> Result<(), String> {
    let CatOptions {
        input,
        topics,
        start,
        end,
        start_secs,
        start_nsecs,
        end_secs,
        end_nsecs,
        json,
    } = opts;

    let start_time = resolve_time(start, start_secs, start_nsecs)?;
    let end_time = resolve_time(end, end_secs, end_nsecs)?;

    let topics_str = if topics.is_empty() {
        None
    } else {
        Some(topics.join(","))
    };
    let topic_filter = parse_topics(topics_str);

    let file = input.unwrap_or_else(|| "-".to_string());
    let mut reader = open_reader(file)?;
    preload_schemas_and_channels(&mut reader)?;
    let channels = reader.channels();
    let schemas = reader.schemas();

    // Build per-schema parsers for JSON mode
    let schema_parsers: HashMap<u16, mcapable_core::stream::schema_parser::SchemaParser> = if json {
        let mut parsers = HashMap::new();
        for (id, schema) in schemas.iter() {
            match mcapable_core::stream::schema_parser::SchemaParser::from_schema(schema) {
                Ok(parser) => {
                    parsers.insert(*id, parser);
                }
                Err(e) => {
                    eprintln!("warning: cannot create parser for schema {id}: {e}");
                }
            }
        }
        parsers
    } else {
        HashMap::new()
    };

    let mut stream = reader.messages().cli()?;
    if let Some(start) = start_time {
        stream = stream.filter(move |hdr| hdr.log_time >= start);
    }
    if let Some(end) = end_time {
        stream = stream.filter(move |hdr| hdr.log_time <= end);
    }
    if let Some(topic_filter) = &topic_filter {
        stream = stream.filter_channel(|ch| topic_filter.contains(ch.topic.as_str()));
    }

    for msg in stream {
        let msg = msg.cli()?;
        let channel = channels.get(&msg.channel_id);
        let topic = channel.map(|c| c.topic.as_str()).unwrap_or("<unknown>");

        if json {
            let data_json = decode_message_json(&msg, channel, &schemas, &schema_parsers);
            let obj = serde_json::json!({
                "topic": topic,
                "sequence": msg.sequence,
                "log_time": msg.log_time,
                "publish_time": msg.publish_time,
                "data": data_json,
            });
            serde_json::to_writer(&mut *out, &obj).cli()?;
            out.write_all(b"\n").cli()?;
        } else {
            writeln!(
                out,
                "{} {} {} {} {}",
                topic, msg.channel_id, msg.sequence, msg.log_time, msg.publish_time
            )
            .cli()?;
            out.write_all(msg.data()).cli()?;
        }
    }

    Ok(())
}

fn decode_message_json(
    msg: &mcapable_core::Message,
    channel: Option<&mcapable_core::Channel>,
    schemas: &std::sync::Arc<HashMap<u16, mcapable_core::Schema>>,
    schema_parsers: &HashMap<u16, mcapable_core::stream::schema_parser::SchemaParser>,
) -> serde_json::Value {
    let data = msg.data();
    let schema_id = channel.map(|c| c.schema_id).unwrap_or(0);
    let message_encoding = channel.map(|c| c.message_encoding.as_ref()).unwrap_or("");

    // Try schema-based parsing first (protobuf, flatbuffer, ros1msg, etc.)
    if schema_id != 0 {
        if let Some(parser) = schema_parsers.get(&schema_id) {
            match parser.parse_json(bytes::Bytes::copy_from_slice(data)) {
                Ok(value) => return value,
                Err(e) => {
                    eprintln!(
                        "warning: failed to decode message on channel {}: {e}",
                        msg.channel_id
                    );
                }
            }
        }
    }

    // Try message-encoding-based parsing
    if message_encoding == "json" {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) {
            return value;
        }
    }

    // For jsonschema encoding without a schema parser, try raw JSON parse
    if let Some(schema) = schemas.get(&schema_id) {
        if schema.encoding.as_ref() == "jsonschema" {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) {
                return value;
            }
        }
    }

    // Fallback: null
    serde_json::Value::Null
}
