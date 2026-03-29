use std::io::Write;

use super::{open_reader, parse_topics, preload_schemas_and_channels};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    input: Option<String>,
    topics: Vec<String>,
    start: Option<u64>,
    end: Option<u64>,
    start_secs: Option<u64>,
    start_nsecs: Option<u32>,
    end_secs: Option<u64>,
    end_nsecs: Option<u32>,
) -> Result<(), String> {
    // Handle start time
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

    // Handle end time
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

    let mut stream = reader.messages().map_err(|e| e.to_string())?;
    if let Some(start) = start_time {
        stream = stream.filter(move |hdr| hdr.log_time >= start);
    }
    if let Some(end) = end_time {
        stream = stream.filter(move |hdr| hdr.log_time <= end);
    }
    if let Some(topic_filter) = &topic_filter {
        stream = stream.filter_channel(|ch| topic_filter.contains(ch.topic.as_str()));
    }

    let mut stdout = std::io::stdout().lock();
    for msg in stream {
        let msg = msg.map_err(|e| e.to_string())?;
        let topic = channels
            .get(&msg.channel_id)
            .map(|c| c.topic.as_str())
            .unwrap_or("<unknown>");
        println!(
            "{} {} {} {} {}",
            topic, msg.channel_id, msg.sequence, msg.log_time, msg.publish_time
        );
        stdout.write_all(msg.data()).map_err(|e| e.to_string())?;
    }

    Ok(())
}
