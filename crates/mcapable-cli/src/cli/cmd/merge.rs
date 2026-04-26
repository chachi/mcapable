use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::{
    copy_attachments_and_metadata_filtered, open_reader, preload_schemas_and_channels,
    writer_from_header, CliResult, OutputOptions,
};

pub fn run(
    output: String,
    inputs: Vec<String>,
    output_options: OutputOptions,
    coalesce_channels: String,
    allow_duplicate_metadata: bool,
) -> Result<(), String> {
    let output = PathBuf::from(output);
    let chunk_options = output_options.to_chunk_options()?;
    let (first, rest) = inputs
        .split_first()
        .ok_or_else(|| "merge requires at least one input".to_string())?;

    let mut first_reader = open_reader(first.clone())?;
    let header = first_reader.header().cli()?;
    preload_schemas_and_channels(&mut first_reader)?;

    let mut schemas: HashMap<u16, mcapable_core::Schema> = first_reader
        .schemas()
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();

    let mut registry = ChannelRegistry::new();

    // Add channels from first file
    for (id, ch) in first_reader.channels().iter() {
        registry.assign(&coalesce_channels, 0, *id, ch)?;
    }

    for (file_idx_offset, input) in rest.iter().enumerate() {
        let file_idx = file_idx_offset + 1;
        let mut reader = open_reader(input.clone())?;
        let other_header = reader.header().cli()?;
        if other_header.profile != header.profile {
            return Err(format!(
                "incompatible profile for {}: expected {}, got {}",
                input, header.profile, other_header.profile
            ));
        }
        preload_schemas_and_channels(&mut reader)?;

        for (id, schema) in reader.schemas().iter() {
            match schemas.get(id) {
                Some(existing) if existing != schema => {
                    return Err(format!("conflicting schema id {id} in {input}"));
                }
                Some(_) => {}
                None => {
                    schemas.insert(*id, schema.clone());
                }
            }
        }
        for (id, ch) in reader.channels().iter() {
            registry.assign(&coalesce_channels, file_idx, *id, ch)?;
        }
    }

    let out = std::fs::File::create(&output)
        .map_err(|e| format!("failed to create {}: {e}", output.display()))?;
    let mut writer = writer_from_header(out, &header, chunk_options)?;

    for schema in schemas.values() {
        writer.copy_schema(schema).cli()?;
    }
    let mut channel_writers = HashMap::new();
    for channel in registry.output_channels.values() {
        let channel_writer = writer.copy_channel(channel).cli()?;
        channel_writers.insert(channel.id, channel_writer);
    }

    // Collect all messages and metadata from all files
    let mut all_messages: Vec<(usize, mcapable_core::RawMessage)> = Vec::new();
    let mut seen_metadata_names: HashSet<String> = HashSet::new();

    for (file_idx, input) in inputs.iter().enumerate() {
        let mut reader = open_reader(input.clone())?;
        preload_schemas_and_channels(&mut reader)?;

        copy_attachments_and_metadata_filtered(&mut reader, &mut writer, true, true, |md| {
            let name = md.name.as_ref().to_string();
            allow_duplicate_metadata || seen_metadata_names.insert(name)
        })?;

        for msg in reader.raw_messages().cli()? {
            let msg = msg.cli()?;
            all_messages.push((file_idx, msg));
        }
    }

    // Sort by (log_time, file_index) for stable interleaving
    all_messages.sort_by(|(a_idx, a), (b_idx, b)| {
        a.log_time
            .cmp(&b.log_time)
            .then_with(|| a_idx.cmp(b_idx))
            .then_with(|| a.channel_id.cmp(&b.channel_id))
            .then_with(|| a.sequence.cmp(&b.sequence))
    });

    for (file_idx, msg) in all_messages {
        let out_channel_id = registry
            .channel_map
            .get(&(file_idx, msg.channel_id))
            .copied()
            .unwrap_or(msg.channel_id);
        let channel_writer = channel_writers
            .get_mut(&out_channel_id)
            .ok_or_else(|| format!("missing channel_id {out_channel_id}"))?;
        channel_writer
            .write_with_sequence(
                msg.log_time,
                msg.publish_time,
                msg.data_bytes(),
                msg.sequence,
            )
            .cli()?;
    }

    writer.finish().cli()?;
    Ok(())
}

/// Tracks channel assignment state across multiple input files for merging.
struct ChannelRegistry {
    channel_map: HashMap<(usize, u16), u16>,
    output_channels: HashMap<u16, mcapable_core::Channel>,
    coalesce_index: HashMap<(String, u16), u16>,
    next_id: u16,
}

impl ChannelRegistry {
    fn new() -> Self {
        Self {
            channel_map: HashMap::new(),
            output_channels: HashMap::new(),
            coalesce_index: HashMap::new(),
            next_id: 0,
        }
    }

    fn assign(
        &mut self,
        mode: &str,
        file_idx: usize,
        original_id: u16,
        channel: &mcapable_core::Channel,
    ) -> Result<u16, String> {
        match mode {
            "auto" | "force" => {
                let key = (channel.topic.as_ref().to_string(), channel.schema_id);
                if let Some(&existing_out_id) = self.coalesce_index.get(&key) {
                    let existing = &self.output_channels[&existing_out_id];
                    if mode == "auto" && existing.metadata != channel.metadata {
                        return Err(format!(
                            "conflicting metadata for topic {} (use --coalesce-channels force to ignore)",
                            channel.topic
                        ));
                    }
                    self.channel_map
                        .insert((file_idx, original_id), existing_out_id);
                    Ok(existing_out_id)
                } else {
                    let out_id = self.next_id;
                    self.next_id = self.next_id.checked_add(1).ok_or("too many channels")?;
                    let mut out_channel = channel.clone();
                    out_channel.id = out_id;
                    self.output_channels.insert(out_id, out_channel);
                    self.coalesce_index.insert(key, out_id);
                    self.channel_map.insert((file_idx, original_id), out_id);
                    Ok(out_id)
                }
            }
            "none" => {
                match self.output_channels.entry(original_id) {
                    std::collections::hash_map::Entry::Occupied(entry) => {
                        if entry.get() != channel {
                            return Err(format!(
                                "conflicting channel id {} (use --coalesce-channels auto to coalesce)",
                                original_id
                            ));
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(channel.clone());
                    }
                }
                self.channel_map
                    .insert((file_idx, original_id), original_id);
                if original_id >= self.next_id {
                    self.next_id = original_id.saturating_add(1);
                }
                Ok(original_id)
            }
            _ => Err(format!("unknown coalesce-channels mode: {mode}")),
        }
    }
}
