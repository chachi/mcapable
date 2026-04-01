//! Basic MCAP file writing with the Writer API.
//!
//! Run with: cargo run -p mcapable --example write_basic

use bytes::Bytes;
use mcapable::reader;
use mcapable::writer::{ChannelSpec, SchemaSpec, WriterBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary file to write to
    let file = tempfile::NamedTempFile::new()?;
    let path = file.path().to_owned();

    // Build a writer with default settings
    let mut writer = WriterBuilder::new().profile("example").build(file)?;

    // Define a schema for our messages
    let schema = SchemaSpec::new(
        "sensor/Temperature",
        "jsonschema",
        Bytes::from(r#"{"type":"object","properties":{"celsius":{"type":"number"}}}"#),
    );

    // Add a channel with the schema
    let mut temp_ch =
        writer.add_channel(ChannelSpec::new("/temperature", "json").schema(schema))?;
    println!("Created channel {} on /temperature", temp_ch.channel_id());

    // Write some messages with increasing timestamps (nanoseconds)
    for i in 0..20 {
        let log_time = i * 1_000_000_000; // 1 second apart
        let payload = format!(r#"{{"celsius":{}}}"#, 20.0 + i as f64 * 0.5);
        temp_ch.write(log_time, log_time, payload.as_bytes())?;
    }
    println!("Wrote 20 temperature messages");

    // Finalize the file (writes footer, summary, trailing magic)
    writer.finish()?;
    println!("File written to {:?}", path);

    // Read it back to verify
    let mut reader = reader::Builder::new().build(std::fs::File::open(&path)?)?;
    let header = reader.header()?;
    println!("\nVerification:");
    println!("  Profile: {}", header.profile);
    println!("  Library: {}", header.library);

    let mut count = 0;
    for msg in reader.messages()? {
        let msg = msg?;
        count += 1;
        if count <= 3 {
            let data = std::str::from_utf8(msg.data())?;
            println!(
                "  Message seq={} time={} data={}",
                msg.sequence, msg.log_time, data
            );
        }
    }
    println!("  Total messages: {}", count);

    Ok(())
}
