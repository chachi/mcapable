use std::collections::HashSet;

use super::{
    copy_attachments_and_metadata, copy_raw_messages, open_reader, parse_topics,
    preload_schemas_and_channels, write_schemas_and_channels, writer_from_header,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    input: Option<String>,
    output: Option<String>,
    topics: Vec<String>,
    start: Option<u64>,
    end: Option<u64>,
    start_secs: Option<u64>,
    start_nsecs: Option<u32>,
    end_secs: Option<u64>,
    end_nsecs: Option<u32>,
) -> Result<(), String> {
    // Handle start time: prefer 'start' if provided, otherwise compute from start_secs/start_nsecs
    let start_time = if start.is_some() {
        start
    } else if let Some(secs) = start_secs {
        let nsecs = start_nsecs.unwrap_or(0);
        if nsecs >= 1_000_000_000 {
            return Err("start_nsecs must be < 1_000_000_000".to_string());
        }
        Some(
            secs.checked_mul(1_000_000_000)
                .and_then(|v| v.checked_add(nsecs as u64))
                .ok_or_else(|| "start timestamp overflow".to_string())?,
        )
    } else {
        None
    };

    // Handle end time: prefer 'end' if provided, otherwise compute from end_secs/end_nsecs
    let end_time = if end.is_some() {
        end
    } else if let Some(secs) = end_secs {
        let nsecs = end_nsecs.unwrap_or(0);
        if nsecs >= 1_000_000_000 {
            return Err("end_nsecs must be < 1_000_000_000".to_string());
        }
        Some(
            secs.checked_mul(1_000_000_000)
                .and_then(|v| v.checked_add(nsecs as u64))
                .ok_or_else(|| "end timestamp overflow".to_string())?,
        )
    } else {
        None
    };

    let input = input.ok_or_else(|| "input file required".to_string())?;
    let output = output.ok_or_else(|| "output file required".to_string())?;
    let topics_str = if topics.is_empty() {
        None
    } else {
        Some(topics.join(","))
    };
    let topic_filter = parse_topics(topics_str);

    let mut reader = open_reader(input)?;
    let header = reader.header().map_err(|e| e.to_string())?;

    preload_schemas_and_channels(&mut reader)?;
    let channels = reader.channels();

    let allowed_channel_ids: Option<HashSet<u16>> = topic_filter.as_ref().map(|topics| {
        channels
            .iter()
            .filter_map(|(id, ch)| topics.contains(ch.topic.as_str()).then_some(*id))
            .collect()
    });

    let out = std::fs::File::create(&output).map_err(|e| format!("failed to create {e}"))?;
    let mut writer = writer_from_header(
        out,
        &header,
        Some(mcapable_core::writer::ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 1048576,
        }),
    )?;

    let mut channel_writers = write_schemas_and_channels(&reader, &mut writer)?;
    copy_attachments_and_metadata(&mut reader, &mut writer)?;
    copy_raw_messages(&mut reader, &mut writer, &mut channel_writers, |msg| {
        if let Some(start) = start_time {
            if msg.log_time < start {
                return false;
            }
        }
        if let Some(end) = end_time {
            if msg.log_time > end {
                return false;
            }
        }
        if let Some(ids) = &allowed_channel_ids {
            if !ids.contains(&msg.channel_id) {
                return false;
            }
        }
        true
    })?;

    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}
