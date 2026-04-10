use std::path::PathBuf;

use super::{rewrite_mcap, CliResult, OutputOptions};

pub fn run(
    input: Option<String>,
    output: Option<String>,
    output_options: OutputOptions,
) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let output = PathBuf::from(output.unwrap_or_else(|| "-".to_string()));
    let chunk_options = output_options.to_chunk_options()?;

    rewrite_mcap(
        input,
        output,
        chunk_options,
        |reader, _writer, channel_writers| {
            let mut messages: Vec<(usize, mcapable_core::RawMessage)> = Vec::new();
            for (idx, msg) in reader.raw_messages().cli()?.enumerate() {
                let msg = msg.cli()?;
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
                    .cli()?;
            }
            Ok(())
        },
    )
}
