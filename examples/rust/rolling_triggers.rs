//! Different split triggers and composite triggers for the RollingWriter.
//!
//! Demonstrates MessageCount, AnyTrigger, on_split callbacks, and force_split().
//!
//! Run with: cargo run -p mcapable --example rolling_triggers

use bytes::Bytes;
use mcapable::writer::rolling::{
    AnyTrigger, MaxSize, MessageCount, RollingWriterBuilder, SequentialFiles,
};
use mcapable::writer::{ChannelSpec, SchemaSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;

    // Composite trigger: split at 50 messages OR 4 KB, whichever comes first
    let factory = SequentialFiles::new(dir.path(), "data");
    let trigger = AnyTrigger::new()
        .or(MessageCount::new(50))
        .or(MaxSize::new(4_000));

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        // Callback fires on each split
        .on_split(|ctx| {
            println!(
                "  Split -> file {} (trigger: {})",
                ctx.file_index,
                ctx.trigger_name.as_deref().unwrap_or("initial"),
            );
        })
        .build()?;

    let schema = SchemaSpec::new(
        "example/Msg",
        "jsonschema",
        Bytes::from(r#"{"type":"object"}"#),
    );
    let mut ch = rolling.add_channel(ChannelSpec::new("/data", "json").schema(schema))?;

    // Write 120 messages — should trigger splits based on message count
    println!("Writing 120 messages (split every 50):");
    for i in 0..120 {
        let log_time = i * 1_000_000_000;
        let data = format!(r#"{{"seq":{i}}}"#);
        ch.write(log_time, log_time, data.as_bytes())?;
    }

    println!(
        "\nCurrent file index: {}, messages in current file: {}",
        rolling.current_file_index(),
        rolling.current_file_message_count(),
    );

    // Force a manual split regardless of trigger state
    println!("\nForcing manual split:");
    let closed = rolling.force_split()?;
    println!(
        "  Closed file {}: {} bytes, {} messages",
        closed.file_index, closed.file_size, closed.message_count,
    );

    // Write a few more into the new file
    for i in 120..125 {
        let log_time = i * 1_000_000_000;
        ch.write(log_time, log_time, format!(r#"{{"seq":{i}}}"#).as_bytes())?;
    }

    rolling.finish()?;

    // List output files
    let mut files: Vec<_> = std::fs::read_dir(dir.path())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "mcap"))
        .collect();
    files.sort_by_key(|e| e.file_name());
    println!("\nTotal files: {}", files.len());
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
