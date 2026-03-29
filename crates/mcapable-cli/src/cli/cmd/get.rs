use std::io::Write;

use super::open_reader;

pub(crate) fn run(index: usize, input: Option<String>) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let mut reader = open_reader(input)?;

    let mut messages = reader.messages().map_err(|e| e.to_string())?;
    let msg = messages
        .nth(index)
        .ok_or_else(|| format!("message at index {index} not found"))?
        .map_err(|e| e.to_string())?;

    let mut stdout = std::io::stdout().lock();
    stdout.write_all(msg.data()).map_err(|e| e.to_string())?;

    Ok(())
}
