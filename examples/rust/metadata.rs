//! Working with schemas and channels.
//!
//! Run with: cargo run -p mcapable --example metadata

mod common_bytes;
mod common_file;

use mcapable::reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = common_file::write_sample_file()?;
    let mut reader = reader::Builder::new().build(file.reopen()?)?;

    // Before streaming: no schemas/channels cached yet
    println!("Before streaming:");
    println!("  Schemas: {}", reader.schemas().len());
    println!("  Channels: {}", reader.channels().len());

    // Stream creation preloads schemas/channels; drop the stream before accessing caches.
    let mut stream = reader.messages()?;
    let _ = stream.next();
    drop(stream);

    let channels = reader.channels();
    let schemas = reader.schemas();

    println!("\nMetadata available after caching:");
    println!("  Schemas: {}", schemas.len());
    println!("  Channels: {}", channels.len());

    // List all channels
    println!("\nChannels:");
    for (id, channel) in channels.iter() {
        println!(
            "  [{}] topic='{}' encoding='{}' schema_id={}",
            id, channel.topic, channel.message_encoding, channel.schema_id
        );
    }

    // List all schemas
    println!("\nSchemas:");
    for (id, schema) in schemas.iter() {
        println!(
            "  [{}] name='{}' encoding='{}' data_len={}",
            id,
            schema.name,
            schema.encoding,
            schema.data.len()
        );
    }

    let mut msg_count = 0;
    for message in reader.messages()? {
        let msg = message?;

        // Look up channel for this message from cached metadata
        if let Some(channel) = channels.get(&msg.channel_id) {
            // Look up schema for this channel from cached metadata
            if let Some(schema) = schemas.get(&channel.schema_id) {
                println!(
                    "  Message on '{}' using schema '{}'",
                    channel.topic, schema.name
                );
            }
        }

        msg_count += 1;
        if msg_count >= 3 {
            break;
        }
    }

    println!("\nProcessed {} messages", msg_count);

    Ok(())
}
