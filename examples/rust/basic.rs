//! Basic usage of mcapable Reader and Stream.
//!
//! Run with: cargo run -p mcapable --example basic

mod common_bytes;
mod common_file;

use mcapable::reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = common_file::write_sample_file()?;
    let mut reader = reader::Builder::new().build(file.reopen()?)?;

    // Header loaded lazily on first access
    let header = reader.header()?;
    println!("MCAP Profile: {}", header.profile);
    if !header.metadata.is_empty() {
        println!("Header metadata:");
        for (k, v) in &header.metadata {
            println!("  {}: {}", k, v);
        }
    }

    // Iterate over messages
    // Schemas and channels are cached as they're encountered
    println!("\nMessages:");
    let mut count = 0;
    for message in reader.messages()? {
        let msg = message?;
        println!(
            "  [{}] channel={} seq={} size={}",
            msg.log_time,
            msg.channel_id,
            msg.sequence,
            msg.data().len()
        );
        count += 1;
        if count >= 10 {
            println!("  ... (showing first 10)");
            break;
        }
    }

    // Show what metadata was loaded during iteration
    println!("\nLoaded {} schemas", reader.schemas().len());
    println!("Loaded {} channels", reader.channels().len());

    Ok(())
}
