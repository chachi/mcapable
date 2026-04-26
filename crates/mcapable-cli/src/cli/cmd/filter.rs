use mcapable_core::collections::{HashMap, HashSet};

use super::{
    copy_attachments_and_metadata_filtered, copy_raw_messages, open_reader, parse_topics,
    preload_schemas_and_channels, resolve_time, write_schemas_and_channels, writer_from_header,
    CliResult, OutputOptions,
};

pub struct FilterOptions {
    pub input: Option<String>,
    pub output: Option<String>,
    pub topics: Vec<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub start_secs: Option<u64>,
    pub start_nsecs: Option<u32>,
    pub end_secs: Option<u64>,
    pub end_nsecs: Option<u32>,
    pub include_topic_regex: Vec<String>,
    pub exclude_topic_regex: Vec<String>,
    pub last_per_channel_topic_regex: Vec<String>,
    pub include_metadata: bool,
    pub include_attachments: bool,
    pub output_options: OutputOptions,
}

pub fn run(opts: FilterOptions) -> Result<(), String> {
    let FilterOptions {
        input,
        output,
        topics,
        start,
        end,
        start_secs,
        start_nsecs,
        end_secs,
        end_nsecs,
        include_topic_regex,
        exclude_topic_regex,
        last_per_channel_topic_regex,
        include_metadata,
        include_attachments,
        output_options,
    } = opts;

    if !topics.is_empty() && !include_topic_regex.is_empty() {
        return Err(
            "cannot use both --topics and --include-topic-regex at the same time".to_string(),
        );
    }

    let start_time = resolve_time(start, start_secs, start_nsecs)?;
    let end_time = resolve_time(end, end_secs, end_nsecs)?;

    let last_per_channel_regexes: Vec<regex::Regex> = last_per_channel_topic_regex
        .iter()
        .map(|r| regex::Regex::new(r).map_err(|e| format!("invalid last-per-channel regex: {e}")))
        .collect::<Result<_, _>>()?;

    let include_regexes: Vec<regex::Regex> = include_topic_regex
        .iter()
        .map(|r| regex::Regex::new(r).map_err(|e| format!("invalid include regex: {e}")))
        .collect::<Result<_, _>>()?;
    let exclude_regexes: Vec<regex::Regex> = exclude_topic_regex
        .iter()
        .map(|r| regex::Regex::new(r).map_err(|e| format!("invalid exclude regex: {e}")))
        .collect::<Result<_, _>>()?;

    let input = input.ok_or_else(|| "input file required".to_string())?;
    let output = output.ok_or_else(|| "output file required".to_string())?;
    let topics_str = if topics.is_empty() {
        None
    } else {
        Some(topics.join(","))
    };
    let topic_filter = parse_topics(topics_str);

    let mut reader = open_reader(input)?;
    let header = reader.header().cli()?;

    preload_schemas_and_channels(&mut reader)?;
    let channels = reader.channels();

    // Build the set of allowed channel IDs from the various topic filters
    let allowed_channel_ids: Option<HashSet<u16>> = {
        let has_topic_filter =
            topic_filter.is_some() || !include_regexes.is_empty() || !exclude_regexes.is_empty();
        if has_topic_filter {
            let ids: HashSet<u16> = channels
                .iter()
                .filter_map(|(id, ch)| {
                    let topic = ch.topic.as_str();
                    // Glob-based filter
                    if let Some(ref topics) = topic_filter {
                        if !topics.contains(topic) {
                            return None;
                        }
                    }
                    // Regex include filter
                    if !include_regexes.is_empty()
                        && !include_regexes.iter().any(|r| r.is_match(topic))
                    {
                        return None;
                    }
                    // Regex exclude filter
                    if exclude_regexes.iter().any(|r| r.is_match(topic)) {
                        return None;
                    }
                    Some(*id)
                })
                .collect();
            Some(ids)
        } else {
            None
        }
    };

    let chunk_options = output_options.to_chunk_options()?;
    let out = std::fs::File::create(&output).map_err(|e| format!("failed to create {e}"))?;
    let mut writer = writer_from_header(out, &header, chunk_options)?;

    let mut channel_writers = write_schemas_and_channels(&reader, &mut writer)?;

    copy_attachments_and_metadata_filtered(
        &mut reader,
        &mut writer,
        include_attachments,
        include_metadata,
        |_| true,
    )?;

    // --last-per-channel-topic-regex: find the last message before start_time for matching channels
    if !last_per_channel_regexes.is_empty() {
        if let Some(start) = start_time {
            let matching_channel_ids: HashSet<u16> = channels
                .iter()
                .filter(|(_, ch)| {
                    let topic = ch.topic.as_str();
                    last_per_channel_regexes.iter().any(|r| r.is_match(topic))
                })
                .map(|(id, _)| *id)
                .collect();

            if !matching_channel_ids.is_empty() {
                // Track the latest pre-start message per channel
                let mut last_before_start: HashMap<u16, mcapable_core::RawMessage> =
                    HashMap::default();
                for msg in reader.raw_messages().cli()? {
                    let msg = msg.cli()?;
                    if msg.log_time >= start {
                        break;
                    }
                    if matching_channel_ids.contains(&msg.channel_id) {
                        last_before_start.insert(msg.channel_id, msg);
                    }
                }
                // Write the collected pre-start messages
                for (_, msg) in last_before_start {
                    let channel_writer = channel_writers
                        .get_mut(&msg.channel_id)
                        .ok_or_else(|| format!("missing channel_id {}", msg.channel_id))?;
                    channel_writer
                        .write_with_sequence(
                            msg.log_time,
                            msg.publish_time,
                            msg.data_bytes(),
                            msg.sequence,
                        )
                        .cli()?;
                }
            }
        }
    }

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

    writer.finish().cli()?;
    Ok(())
}
