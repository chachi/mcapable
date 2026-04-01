//! Writing metadata, attachments, and customizing the header.
//!
//! Run with: cargo run -p mcapable --example write_metadata

use bytes::Bytes;
use mcapable::reader;
use mcapable::writer::{ChannelSpec, SchemaSpec, WriterBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = tempfile::NamedTempFile::new()?;
    let path = file.path().to_owned();

    // Customize the header with profile, library, and metadata
    let mut writer = WriterBuilder::new()
        .profile("ros2")
        .library("my-recorder v1.0")
        .header_metadata("recording_id", "abc-123")
        .header_metadata("robot_name", "atlas")
        .build(file)?;

    // Add a channel and write a few messages
    let schema = SchemaSpec::new("std_msgs/String", "ros2msg", Bytes::from("string data"));
    let mut ch = writer.add_channel(ChannelSpec::new("/chatter", "cdr").schema(schema))?;

    for i in 0..5 {
        let log_time = i * 1_000_000_000;
        ch.write(log_time, log_time, format!("hello {i}").as_bytes())?;
    }

    // Write an attachment (e.g., a calibration file or config snapshot)
    writer.attachment_writer().write(
        0,                                            // log_time
        0,                                            // create_time
        "calibration.yaml".into(),                    // name
        "text/yaml".into(),                           // media_type
        Bytes::from("camera:\n  fx: 500\n  fy: 500"), // data
    )?;
    println!("Wrote attachment: calibration.yaml");

    // Write a second attachment
    writer.attachment_writer().write(
        0,
        0,
        "config.json".into(),
        "application/json".into(),
        Bytes::from(r#"{"mode":"autonomous"}"#),
    )?;
    println!("Wrote attachment: config.json");

    writer.finish()?;

    // Read back and inspect
    let mut reader = reader::Builder::new().build(std::fs::File::open(&path)?)?;

    let header = reader.header()?;
    println!("\nHeader:");
    println!("  Profile: {}", header.profile);
    println!("  Library: {}", header.library);
    println!("  Metadata:");
    for (k, v) in &header.metadata {
        println!("    {}: {}", k, v);
    }

    // Count messages
    let mut count = 0;
    for msg in reader.messages()? {
        msg?;
        count += 1;
    }
    println!("\nMessages: {}", count);

    // Show summary info
    if let Some(summary) = reader.summary()?
        && let Some(stats) = &summary.statistics
    {
        println!("Attachment count: {}", stats.attachment_count);
    }

    Ok(())
}
