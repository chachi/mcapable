//! Low-level record iteration.
//!
//! Run with: cargo run -p mcapable --example records

mod common_bytes;
mod common_file;

use mcapable::{Record, reader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = common_file::write_sample_file()?;
    let mut reader = reader::Builder::new().build(file.reopen()?)?;

    // Count record types
    let mut header_count = 0;
    let mut footer_count = 0;
    let mut schema_count = 0;
    let mut channel_count = 0;
    let mut message_count = 0;
    let mut chunk_count = 0;
    let mut other_count = 0;

    for record in reader.records() {
        match record? {
            Record::Header(_) => header_count += 1,
            Record::Footer(_) => footer_count += 1,
            Record::Schema(_) => schema_count += 1,
            Record::Channel(_) => channel_count += 1,
            Record::Message(_) => message_count += 1,
            Record::Chunk(_) => chunk_count += 1,
            _ => other_count += 1,
        }
    }

    println!("Record counts:");
    println!("  Header:   {}", header_count);
    println!("  Footer:   {}", footer_count);
    println!("  Schema:   {}", schema_count);
    println!("  Channel:  {}", channel_count);
    println!("  Message:  {}", message_count);
    println!("  Chunk:    {}", chunk_count);
    println!("  Other:    {}", other_count);

    Ok(())
}
