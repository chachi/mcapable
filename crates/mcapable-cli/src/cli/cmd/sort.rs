use std::path::PathBuf;

use super::{
    copy_attachments_and_metadata, open_reader, preload_schemas_and_channels,
    write_schemas_and_channels, writer_from_header,
};

pub(crate) fn run(input: Option<String>, output: Option<String>) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let output = PathBuf::from(output.unwrap_or_else(|| "-".to_string()));
    let compression = None;
    let chunk_size = 1048576;

    let mut reader = open_reader(input)?;
    let header = reader.header().map_err(|e| e.to_string())?;

    preload_schemas_and_channels(&mut reader)?;
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

    let mut channel_writers = write_schemas_and_channels(&reader, &mut writer)?;
    copy_attachments_and_metadata(&mut reader, &mut writer)?;

    let mut messages: Vec<(usize, mcapable_core::RawMessage)> = Vec::new();
    for (idx, msg) in reader
        .raw_messages()
        .map_err(|e| e.to_string())?
        .enumerate()
    {
        let msg = msg.map_err(|e| e.to_string())?;
        messages.push((idx, msg));
    }

    messages.sort_by(|(a_idx, a), (b_idx, b)| {
        a.log_time
            .cmp(&b.log_time)
            .then_with(|| a.channel_id.cmp(&b.channel_id))
            .then_with(|| a.sequence.cmp(&b.sequence))
            .then_with(|| a.publish_time.cmp(&b.publish_time))
            .then_with(|| a_idx.cmp(b_idx))
    });

    for (_, msg) in messages {
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
            .map_err(|e| e.to_string())?;
    }

    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}
