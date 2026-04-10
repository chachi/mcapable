use std::path::PathBuf;

use super::{
    copy_raw_messages, file_created_ns, now_ns, parse_metadata_kv_pairs, parse_timestamp_ns,
    rewrite_mcap, AddCommand, CliResult, OutputOptions,
};

pub fn dispatch(command: AddCommand) -> Result<(), String> {
    match command {
        AddCommand::Attachment {
            input,
            output,
            file,
            name,
            content_type,
            log_time,
            creation_time,
            output_options,
        } => run_attachment(
            input,
            output,
            file,
            name,
            content_type,
            log_time,
            creation_time,
            output_options,
        ),
        AddCommand::Metadata {
            input,
            output,
            name,
            key,
            output_options,
        } => run_metadata(input, output, name, key, output_options),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_attachment(
    input: Option<String>,
    output: Option<String>,
    file: PathBuf,
    name: Option<String>,
    content_type: String,
    log_time: Option<String>,
    creation_time: Option<String>,
    output_options: OutputOptions,
) -> Result<(), String> {
    let input = input.ok_or_else(|| "input file required".to_string())?;
    let output = PathBuf::from(output.ok_or_else(|| "output file required".to_string())?);
    let chunk_options = output_options.to_chunk_options()?;

    let att_name: mcapable_core::zero_copy::ByteStr = name
        .unwrap_or_else(|| {
            file.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "attachment".to_string())
        })
        .into();
    let att_content_type: mcapable_core::zero_copy::ByteStr = content_type.into();
    let att_log_time = match log_time {
        Some(s) => parse_timestamp_ns(&s)?,
        None => now_ns()?,
    };
    let att_creation_time = match creation_time {
        Some(s) => parse_timestamp_ns(&s)?,
        None => file_created_ns(&file)?,
    };
    let att_data =
        std::fs::read(&file).map_err(|e| format!("failed to read {}: {e}", file.display()))?;

    rewrite_mcap(
        input,
        output,
        chunk_options,
        |reader, writer, channel_writers| {
            copy_raw_messages(reader, writer, channel_writers, |_| true)?;
            writer
                .copy_attachment(
                    att_log_time,
                    att_creation_time,
                    att_name,
                    att_content_type,
                    bytes::Bytes::from(att_data),
                )
                .cli()
        },
    )
}

fn run_metadata(
    input: Option<String>,
    output: Option<String>,
    name: String,
    key: Vec<String>,
    output_options: OutputOptions,
) -> Result<(), String> {
    let input = input.ok_or_else(|| "input file required".to_string())?;
    let output = PathBuf::from(output.ok_or_else(|| "output file required".to_string())?);
    let chunk_options = output_options.to_chunk_options()?;

    let metadata_map = parse_metadata_kv_pairs(key)?;
    let md = mcapable_core::types::Metadata {
        name: name.into(),
        metadata: metadata_map,
    };

    rewrite_mcap(
        input,
        output,
        chunk_options,
        |reader, writer, channel_writers| {
            copy_raw_messages(reader, writer, channel_writers, |_| true)?;
            writer.copy_metadata(&md).cli()
        },
    )
}
