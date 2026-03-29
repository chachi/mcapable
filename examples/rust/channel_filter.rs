//! Filtering messages with `filter_channel` predicates.
//!
//! This example demonstrates filtering based on channel metadata (topic, schema, encoding),
//! which is often more expressive than filtering by numeric channel IDs.
//!
//! Run with: `cargo run -p mcapable --example channel_filter`

mod common_bytes;
mod common_file;

use mcapable::reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = common_file::write_sample_file()?;
    let mut reader = reader::Builder::new().build(file.reopen()?)?;

    // Example 1: filter by topic prefix.
    let mut count = 0usize;
    for msg in reader
        .messages()?
        .filter_channel(|ch| ch.topic.starts_with("/example"))
    {
        let msg = msg?;
        count += 1;
        if count <= 5 {
            println!(
                "camera msg: channel={} time={}",
                msg.channel_id, msg.log_time
            );
        }
    }
    println!("camera messages: {count}");

    Ok(())
}
