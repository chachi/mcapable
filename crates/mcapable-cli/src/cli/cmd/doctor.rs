use std::io::Write;

use super::open_reader_allow_missing_end_magic;

pub(crate) fn run(input: Option<String>, verbose: bool) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    let strict_message_order = false; // TODO: get from global CLI flag
    run_with_output(input, strict_message_order, verbose, &mut stdout)
}

pub(crate) fn run_with_output<W: Write>(
    input: Option<String>,
    strict_message_order: bool,
    verbose: bool,
    stdout: &mut W,
) -> Result<(), String> {
    let mut reader = open_reader_allow_missing_end_magic(input.unwrap_or_else(|| "-".to_string()))?;

    let data_section_schemas = reader.data_section_schemas().map_err(|e| e.to_string())?;
    for schema in data_section_schemas {
        if schema.encoding.as_ref().is_empty() && schema.data.is_empty() {
            writeln!(
                stdout,
                "Schema with ID: {}, Name: {:?} has empty Encoding and Data fields",
                schema.id,
                schema.name.as_ref()
            )
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(summary) = reader.summary().map_err(|e| e.to_string())? {
        for schema in summary.schemas.values() {
            if schema.encoding.as_ref().is_empty() && schema.data.is_empty() {
                writeln!(
                    stdout,
                    "Schema with ID: {}, Name: {:?} has empty Encoding and Data fields",
                    schema.id,
                    schema.name.as_ref()
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    let mut record_count: u64 = 0;
    for rec in reader.record_metadata() {
        let rec = match rec {
            Ok(rec) => rec,
            Err(_) => break,
        };
        let _ = rec;
        record_count += 1;
    }

    if strict_message_order {
        let mut prev: Option<u64> = None;
        for msg in reader.raw_messages().map_err(|e| e.to_string())? {
            let msg = msg.map_err(|e| e.to_string())?;
            strict_check_monotonic(&mut prev, msg.log_time)?;
        }
    }

    if verbose {
        writeln!(stdout, "records_ok: {record_count}").map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn strict_check_monotonic(prev: &mut Option<u64>, next: u64) -> Result<(), String> {
    if let Some(p) = *prev {
        if next < p {
            return Err(format!(
                "strict message order violated: {} then {}",
                p, next
            ));
        }
    }
    *prev = Some(next);
    Ok(())
}
