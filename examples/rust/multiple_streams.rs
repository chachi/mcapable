//! Creating multiple streams from the same reader.
//!
//! Run with: cargo run -p mcapable --example multiple_streams

mod common_bytes;
mod common_file;

use mcapable::reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = common_file::write_sample_file()?;
    let mut reader = reader::Builder::new().build(file.reopen()?)?;

    // Key feature: you can run multiple *sequential* iterations from one reader.
    // Only one stream can be active at a time because stream creation needs `&mut reader`.

    // First pass: analyze chunks
    println!("Chunk analysis:");
    let mut chunk_count = 0;
    let mut total_uncompressed = 0u64;
    for chunk in reader.chunks() {
        let chunk = chunk?;
        chunk_count += 1;
        total_uncompressed += chunk.uncompressed_size;
    }
    println!("  {} chunks", chunk_count);
    println!("  {} bytes uncompressed total", total_uncompressed);

    // Second pass: count messages per channel
    println!("\nMessage counts per channel:");
    let mut channel_counts: mcapable_core::collections::HashMap<u16, usize> =
        mcapable_core::collections::HashMap::default();
    for message in reader.messages()? {
        let msg = message?;
        *channel_counts.entry(msg.channel_id).or_default() += 1;
    }
    for (channel_id, count) in &channel_counts {
        println!("  channel {}: {} messages", channel_id, count);
    }

    // Third pass: get time range
    println!("\nTime range analysis:");
    let mut min_time = u64::MAX;
    let mut max_time = 0u64;
    for message in reader.raw_messages()? {
        let msg = message?;
        min_time = min_time.min(msg.log_time);
        max_time = max_time.max(msg.log_time);
    }
    if min_time <= max_time {
        println!("  start: {}", min_time);
        println!("  end:   {}", max_time);
        println!("  duration: {}", max_time - min_time);
    }

    // Different stream types for different use cases
    println!("\nStream type comparison:");

    // Record stream: all record types
    let record_count = reader.records().count();
    println!("  Records: {}", record_count);

    // Chunk stream: only chunks (compressed)
    let chunk_count = reader.chunks().count();
    println!("  Chunks: {}", chunk_count);

    // Raw message stream: messages without metadata lookups
    let raw_msg_count = reader.raw_messages()?.count();
    println!("  Raw messages: {}", raw_msg_count);

    // Message stream: messages with full metadata
    let msg_count = reader.messages()?.count();
    println!("  Messages: {}", msg_count);

    Ok(())
}
