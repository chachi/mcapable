use std::io::Write;

use super::{open_reader, CliResult, GetCommand};

pub fn dispatch(command: GetCommand) -> Result<(), String> {
    match command {
        GetCommand::Message { index, input } => run_message(index, input),
        GetCommand::Attachment {
            input,
            name,
            offset,
            output,
        } => run_attachment(input, name, offset, output),
        GetCommand::Metadata { input, name } => run_metadata(input, name),
    }
}

fn run_message(index: usize, input: Option<String>) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let mut reader = open_reader(input)?;

    let mut messages = reader.messages().cli()?;
    let msg = messages
        .nth(index)
        .ok_or_else(|| format!("message at index {index} not found"))?
        .cli()?;

    let mut stdout = std::io::stdout().lock();
    stdout.write_all(msg.data()).cli()?;

    Ok(())
}

fn run_attachment(
    input: Option<String>,
    name: String,
    offset_filter: Option<u64>,
    output: Option<String>,
) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let mut reader = open_reader(input)?;

    for record in reader
        .records()
        .filter(|op| matches!(op, mcapable_core::Opcode::Attachment))
    {
        let record = record.cli()?;
        if let mcapable_core::Record::Attachment(att) = record {
            if att.name.as_ref() != name {
                continue;
            }
            if let Some(expected_offset) = offset_filter {
                // Use create_time as a proxy for offset disambiguation
                if att.create_time != expected_offset {
                    continue;
                }
            }
            // Found matching attachment — write it
            if let Some(path) = output {
                std::fs::write(&path, att.data.as_ref())
                    .map_err(|e| format!("failed to write {path}: {e}"))?;
            } else {
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(att.data.as_ref()).cli()?;
            }
            return Ok(());
        }
    }

    Err(format!("attachment {name:?} not found"))
}

fn run_metadata(input: Option<String>, name: String) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let mut reader = open_reader(input)?;

    for record in reader
        .records()
        .filter(|op| matches!(op, mcapable_core::Opcode::Metadata))
    {
        let record = record.cli()?;
        if let mcapable_core::Record::Metadata(md) = record {
            if md.name.as_ref() != name {
                continue;
            }
            // Print key-value pairs
            for (k, v) in &md.metadata {
                println!("{}={}", k, v);
            }
            return Ok(());
        }
    }

    Err(format!("metadata {name:?} not found"))
}
