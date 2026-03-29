//! Working with chunks directly.
//!
//! Run with: cargo run -p mcapable --example chunks

mod common_bytes;
mod common_file;

use mcapable::reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = common_file::write_sample_file()?;
    let mut reader = reader::Builder::new().build(file.reopen()?)?;

    println!("Chunk details:");
    for (i, chunk) in reader.chunks().enumerate() {
        let chunk = chunk?;
        let compression_ratio = if chunk.uncompressed_size > 0 {
            chunk.records.len() as f64 / chunk.uncompressed_size as f64
        } else {
            1.0
        };

        println!("Chunk {}:", i);
        println!(
            "  Time range: {} - {}",
            chunk.message_start_time, chunk.message_end_time
        );
        println!("  Compression: {}", chunk.compression);
        println!("  Compressed size: {} bytes", chunk.records.len());
        println!("  Uncompressed size: {} bytes", chunk.uncompressed_size);
        println!("  Compression ratio: {:.2}%", compression_ratio * 100.0);
        println!("  CRC32: 0x{:08x}", chunk.uncompressed_crc);

        if i >= 4 {
            println!("... (showing first 5 chunks)");
            break;
        }
    }

    Ok(())
}
