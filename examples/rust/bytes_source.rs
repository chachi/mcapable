//! Building a zero-copy Reader from in-memory bytes.
//!
//! This example demonstrates using `BytesCursor` via `Reader::from_slice` to
//! avoid extra copies when the full MCAP is already in memory.
//!
//! Run with: `cargo run -p mcapable --example bytes_source`

mod common_bytes;

use mcapable::Reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let buf = common_bytes::sample_bytes();
    let mut reader = Reader::from_slice(&buf)?;

    let header = reader.header()?;
    println!("Profile: {}", header.profile);
    println!("Library: {}", header.library);

    let mut count = 0usize;
    for msg in reader.raw_messages()? {
        let msg = msg?;
        count += 1;
        if count <= 3 {
            println!(
                "Message: channel={} time={} size={}",
                msg.channel_id,
                msg.log_time,
                msg.data_len()
            );
        }
    }
    println!("Total messages: {count}");
    Ok(())
}
