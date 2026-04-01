//! Custom sink factories and timestamp-based file naming.
//!
//! Demonstrates TimestampFiles, FnSinkFactory for fully custom output,
//! and max_files() for automatic cleanup of old files.
//!
//! Run with: cargo run -p mcapable --example rolling_custom

use bytes::Bytes;
use std::fs::File;
use std::io::BufWriter;

use mcapable::Compression;
use mcapable::writer::rolling::{
    FnSinkFactory, MessageCount, RollingWriterBuilder, SequentialFiles, TimestampFiles,
};
use mcapable::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- Example 1: SequentialFiles with max_files cleanup ---
    println!("=== SequentialFiles with max_files(3) ===\n");
    {
        let dir = tempfile::tempdir()?;

        // Keep only the 3 most recent files — oldest are deleted automatically
        let factory = SequentialFiles::new(dir.path(), "log").max_files(3);
        let trigger = MessageCount::new(10);

        let mut rolling = RollingWriterBuilder::new(factory, trigger)
            .writer_builder(
                WriterBuilder::new()
                    .profile("example")
                    .chunked(ChunkOptions {
                        compression: Some(Compression::Zstd),
                        ..ChunkOptions::default()
                    }),
            )
            .build()?;

        let schema = SchemaSpec::new("test/Msg", "jsonschema", Bytes::from(r#"{}"#));
        let mut ch = rolling.add_channel(ChannelSpec::new("/test", "json").schema(schema))?;

        // Write 60 messages — creates 6 files, but only 3 survive
        for i in 0..60 {
            let log_time = i * 100_000_000;
            ch.write(log_time, log_time, format!(r#"{{"i":{i}}}"#).as_bytes())?;
        }
        rolling.finish()?;

        let mut files: Vec<_> = std::fs::read_dir(dir.path())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "mcap"))
            .collect();
        files.sort_by_key(|e| e.file_name());

        println!("Files on disk (oldest auto-deleted):");
        for entry in &files {
            println!("  {}", entry.file_name().to_string_lossy());
        }
        println!("Count: {} (max_files=3)\n", files.len());
    }

    // --- Example 2: TimestampFiles for time-stamped naming ---
    println!("=== TimestampFiles ===\n");
    {
        let dir = tempfile::tempdir()?;

        let factory = TimestampFiles::new(dir.path(), "capture");
        // Use force_split to control timing precisely
        let trigger = MessageCount::new(100); // Won't fire — we split manually

        let mut rolling = RollingWriterBuilder::new(factory, trigger).build()?;
        let mut ch = rolling.add_channel(ChannelSpec::new("/sensor", "json"))?;

        ch.write(0, 0, r#"{"val":1}"#.as_bytes())?;

        // Small delay so the next file gets a different timestamp
        std::thread::sleep(std::time::Duration::from_secs(1));

        rolling.force_split()?;
        ch.write(1_000_000_000, 1_000_000_000, r#"{"val":2}"#.as_bytes())?;
        rolling.finish()?;

        let mut files: Vec<_> = std::fs::read_dir(dir.path())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "mcap"))
            .collect();
        files.sort_by_key(|e| e.file_name());

        println!("Timestamp-named files:");
        for entry in &files {
            println!("  {}", entry.file_name().to_string_lossy());
        }
        println!();
    }

    // --- Example 3: FnSinkFactory for fully custom output ---
    println!("=== FnSinkFactory (custom naming) ===\n");
    {
        let dir = tempfile::tempdir()?;
        let dir_path = dir.path().to_owned();

        // Use a closure to produce files with any naming scheme you want
        let factory = FnSinkFactory::new(move |ctx| {
            let path = dir_path.join(format!("segment-{:03}.mcap", ctx.file_index));
            println!("  Creating: {}", path.display());
            Ok(BufWriter::new(File::create(path)?))
        });

        let trigger = MessageCount::new(15);
        let mut rolling = RollingWriterBuilder::new(factory, trigger).build()?;

        let mut ch = rolling.add_channel(ChannelSpec::new("/data", "json"))?;

        for i in 0..50 {
            let log_time = i * 1_000_000_000;
            ch.write(log_time, log_time, format!(r#"{{"v":{i}}}"#).as_bytes())?;
        }
        rolling.finish()?;

        let mut files: Vec<_> = std::fs::read_dir(dir.path())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "mcap"))
            .collect();
        files.sort_by_key(|e| e.file_name());

        println!("\nCustom-named files:");
        for entry in &files {
            println!("  {}", entry.file_name().to_string_lossy());
        }
    }

    Ok(())
}
