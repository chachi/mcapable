use std::collections::HashMap;
use std::path::PathBuf;

use super::{
    copy_attachments_and_metadata, copy_raw_messages, open_reader, preload_schemas_and_channels,
    writer_from_header,
};

pub(crate) fn run(output: String, inputs: Vec<String>) -> Result<(), String> {
    let output = PathBuf::from(output);
    let compression = None;
    let chunk_size = 1048576;
    let (first, rest) = inputs
        .split_first()
        .ok_or_else(|| "merge requires at least one input".to_string())?;

    let mut first_reader = open_reader(first.clone())?;
    let header = first_reader.header().map_err(|e| e.to_string())?;
    preload_schemas_and_channels(&mut first_reader)?;

    let mut schemas: HashMap<u16, mcapable_core::Schema> = first_reader
        .schemas()
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    let mut channels: HashMap<u16, mcapable_core::Channel> = first_reader
        .channels()
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();

    for input in rest {
        let mut reader = open_reader(input.clone())?;
        let other_header = reader.header().map_err(|e| e.to_string())?;
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
        for (id, channel) in reader.channels().iter() {
            match channels.get(id) {
                Some(existing) if existing != channel => {
                    return Err(format!("conflicting channel id {id} in {input}"));
                }
                Some(_) => {}
                None => {
                    channels.insert(*id, channel.clone());
                }
            }
        }
    }

    let out = std::fs::File::create(&output)
        .map_err(|e| format!("failed to create {}: {e}", output.display()))?;
    let mut writer = writer_from_header(
        out,
        &header,
        Some(mcapable_core::writer::ChunkOptions {
            compression,
            max_uncompressed_bytes: chunk_size,
        }),
    )?;

    for schema in schemas.values() {
        writer.copy_schema(schema).map_err(|e| e.to_string())?;
    }
    let mut channel_writers = HashMap::new();
    for channel in channels.values() {
        let channel_writer = writer.copy_channel(channel).map_err(|e| e.to_string())?;
        channel_writers.insert(channel.id, channel_writer);
    }

    for input in inputs {
        let mut reader = open_reader(input.clone())?;
        preload_schemas_and_channels(&mut reader)?;

        copy_attachments_and_metadata(&mut reader, &mut writer)?;
        copy_raw_messages(&mut reader, &mut writer, &mut channel_writers, |_| true)?;
    }

    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}
