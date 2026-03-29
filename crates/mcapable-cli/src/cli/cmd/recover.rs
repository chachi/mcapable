//! Recover a readable MCAP file from a truncated or partially-corrupted input.
//!
//! The primary goal of `recover` is to produce a *structurally valid MCAP* with a *proper summary
//! section* (statistics and indexes) so downstream tools can seek and introspect efficiently.
//!
//! ## Intended behavior (design)
//!
//! - Read the input as a sequence of records.
//! - Copy as many well-formed *data section* records as possible into a new output file.
//!   - Stop on truncation/corruption (short read), or when the input footer is reached.
//! - Ensure the output data section is properly terminated with `DataEnd`.
//! - Build a *new* summary section from the recovered data section:
//!   - `Schema` + `Channel` records for metadata lookup
//!   - `ChunkIndex` (and optionally `MessageIndex`) for seeking/filtering
//!   - `AttachmentIndex` / `MetadataIndex` for fast listing
//!   - `Statistics` for message counts and time bounds
//!   - `SummaryOffset` as the terminator/index of summary records
//! - Write a new `Footer` with correct `summary_start` / `summary_offset_start` and `summary_crc`,
//!   followed by trailing MCAP magic bytes.
//!
//! ## Performance constraints (design)
//!
//! `recover` should avoid decompressing and recompressing chunks where possible. The current fast
//! path copies `Chunk` records directly (preserving chunk boundaries) and rebuilds indexes from
//! the recovered record stream.
//!
//! ## Current implementation notes
//!
//! The implementation copies raw records from the input data section into a new file, then writes
//! a fresh `DataEnd`, summary section (including indexes/statistics), and `Footer`. Summary CRC is
//! computed as part of summary generation.
//!
//! Note: the CLI flags `--compression` and `--chunk-size` are currently ignored by `recover`.
//! `recover` preserves existing chunk/message record boundaries instead of re-chunking.

use std::collections::HashMap;
use std::path::PathBuf;

use super::open_reader_allow_missing_end_magic;

pub(crate) fn run(input: Option<String>, output: Option<String>) -> Result<(), String> {
    let input = input.unwrap_or_else(|| "-".to_string());
    let output = PathBuf::from(output.unwrap_or_else(|| "-".to_string()));

    let mut reader = open_reader_allow_missing_end_magic(input)?;
    let header = reader.header().map_err(|e| e.to_string())?;

    let out = std::fs::File::create(&output)
        .map_err(|e| format!("failed to create {}: {e}", output.display()))?;

    let mut builder = mcapable_core::writer::WriterBuilder::new()
        .profile(header.profile.clone())
        .library(recover_library_string(&header.library))
        .validation(mcapable_core::writer::Validation::Permissive)
        .always_write_summary(true);
    for (k, v) in &header.metadata {
        builder = builder.header_metadata(k.clone(), v.clone());
    }
    let mut writer = builder.build(out).map_err(|e| e.to_string())?;

    // Best-effort: if the input summary is readable, prime schema/channel definitions from it.
    // This allows recovery from files where those records are only present in the summary section
    // (e.g. synthetic fixtures or severe corruption in the early data section).
    let mut wrote_schema_ids: HashMap<u16, ()> = HashMap::new();
    let mut wrote_channel_ids: HashMap<u16, ()> = HashMap::new();
    if let Ok(Some(summary)) = reader.summary() {
        for schema in summary.schemas.values() {
            wrote_schema_ids.insert(schema.id, ());
            writer.copy_schema(schema).map_err(|e| e.to_string())?;
        }
        for channel in summary.channels.values() {
            wrote_channel_ids.insert(channel.id, ());
            let _ = writer.copy_channel(channel).map_err(|e| e.to_string())?;
        }
    }

    // Copy as many data section records as possible in order.
    //
    // Stop on truncation/corruption, and stop at `DataEnd` (don't copy summary/footer records).
    for record in reader.records() {
        let record = match record {
            Ok(r) => r,
            Err(_) => break,
        };
        match &record {
            mcapable_core::Record::Header(_) => continue,
            mcapable_core::Record::DataEnd => break,
            mcapable_core::Record::Footer(_) => break,
            mcapable_core::Record::Schema(s) => {
                if wrote_schema_ids.contains_key(&s.id) {
                    continue;
                }
                wrote_schema_ids.insert(s.id, ());
            }
            mcapable_core::Record::Channel(c) => {
                if wrote_channel_ids.contains_key(&c.id) {
                    continue;
                }
                wrote_channel_ids.insert(c.id, ());
            }
            _ => {}
        }

        writer.copy_record(&record).map_err(|e| e.to_string())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn recover_library_string(
    previous: &mcapable_core::zero_copy::ByteStr,
) -> mcapable_core::zero_copy::ByteStr {
    let current = mcapable_core::writer::default_library_string();
    if previous.as_ref().is_empty() {
        current
    } else {
        mcapable_core::zero_copy::ByteStr::from(format!("{current}; {previous}"))
    }
}
