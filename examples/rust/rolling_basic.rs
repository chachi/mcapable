//! Basic RollingWriter usage with size-based file splitting.
//!
//! The RollingWriter automatically splits output across multiple MCAP files.
//! Each file is fully self-contained with its own header, schemas, channels,
//! and footer.
//!
//! Run with: cargo run -p mcapable --example rolling_basic

use bytes::Bytes;
use mcapable::writer::rolling::{MaxSize, RollingWriterBuilder, SequentialFiles};
use mcapable::writer::{ChannelSpec, SchemaSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let dir_path = dir.path().to_owned();

    // Create a rolling writer that splits at ~2 KB per file
    let factory = SequentialFiles::new(dir.path(), "recording");
    let trigger = MaxSize::new(2_000); // 2 KB

    let mut rolling = RollingWriterBuilder::new(factory, trigger).build()?;

    // Add a channel — it will be re-registered in each new file automatically
    let schema = SchemaSpec::new(
        "sensor/Data",
        "jsonschema",
        Bytes::from(r#"{"type":"object"}"#),
    );
    let mut ch = rolling.add_channel(ChannelSpec::new("/sensor", "json").schema(schema))?;

    // Write enough messages to trigger several splits
    for i in 0..200 {
        let log_time = i * 1_000_000_000;
        let data = format!(r#"{{"value":{i},"padding":"{}"}}"#, "x".repeat(50));
        ch.write(log_time, log_time, data.as_bytes())?;
    }

    rolling.finish()?;

    // List the output files
    let mut files: Vec<_> = std::fs::read_dir(&dir_path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "mcap"))
        .collect();
    files.sort_by_key(|e| e.file_name());

    println!("Output directory: {:?}", dir_path);
    println!("Files created: {}", files.len());
    for entry in &files {
        let meta = entry.metadata()?;
        println!(
            "  {} ({} bytes)",
            entry.file_name().to_string_lossy(),
            meta.len()
        );
    }

    Ok(())
}
