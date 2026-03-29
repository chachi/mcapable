//! Filtering streams by time range and channels.
//!
//! Run with: cargo run -p mcapable --example filtering

mod common_bytes;
mod common_file;

use mcapable::reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = common_file::write_sample_file()?;
    let mut reader = reader::Builder::new().build(file.reopen()?)?;

    // Filter by time range
    // Only messages with log_time in [start, end] are returned
    println!("Messages in time range [0, 10]:");
    let mut count = 0;
    for message in reader.messages()?.time_range(0, 10) {
        let msg = message?;
        println!("  time={} channel={}", msg.log_time, msg.channel_id);
        count += 1;
        if count >= 5 {
            break;
        }
    }

    // Filter by channels
    // Only messages on specified channel IDs are returned
    println!("\nMessages on channels [1]:");
    count = 0;
    for message in reader.messages()?.filter_channel(|ch| ch.id == 1) {
        let msg = message?;
        println!("  time={} channel={}", msg.log_time, msg.channel_id);
        count += 1;
        if count >= 5 {
            break;
        }
    }

    // Combine filters - both must match
    println!("\nMessages on channel 1 in time range [0, 10]:");
    count = 0;
    for message in reader
        .messages()?
        .time_range(0, 10)
        .filter_channel(|ch| ch.id == 1)
    {
        let msg = message?;
        println!("  time={} channel={}", msg.log_time, msg.channel_id);
        count += 1;
        if count >= 5 {
            break;
        }
    }

    Ok(())
}
