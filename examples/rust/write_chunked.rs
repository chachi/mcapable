//! Chunked writing with compression for efficient MCAP files.
//!
//! When chunking is enabled, messages are grouped into Chunk records with
//! optional compression. This produces smaller files and enables efficient
//! random access via chunk indexes.
//!
//! Run with: cargo run -p mcapable --example write_chunked

use bytes::Bytes;
use mcapable::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};
use mcapable::{Compression, reader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = tempfile::NamedTempFile::new()?;
    let path = file.path().to_owned();

    // Enable chunked writing with zstd compression
    let mut writer = WriterBuilder::new()
        .profile("example")
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 1024 * 64, // Flush chunk every 64 KB
            ..ChunkOptions::default()
        })
        .build(file)?;

    // Create two channels — messages will be interleaved in the same chunks
    let imu_schema = SchemaSpec::new(
        "sensor/Imu",
        "jsonschema",
        Bytes::from(r#"{"type":"object"}"#),
    );
    let gps_schema = SchemaSpec::new(
        "sensor/Gps",
        "jsonschema",
        Bytes::from(r#"{"type":"object"}"#),
    );

    let mut imu_ch = writer.add_channel(ChannelSpec::new("/imu", "json").schema(imu_schema))?;
    let mut gps_ch = writer.add_channel(ChannelSpec::new("/gps", "json").schema(gps_schema))?;

    println!(
        "Channels: /imu (id={}), /gps (id={})",
        imu_ch.channel_id(),
        gps_ch.channel_id()
    );

    // Write interleaved messages — IMU at 100Hz, GPS at 10Hz
    let mut msg_count = 0;
    for i in 0..1000 {
        let log_time = i * 10_000_000; // 10ms apart (100Hz)

        let imu_data = format!(
            r#"{{"accel_x":{:.2},"accel_y":{:.2},"accel_z":{:.2}}}"#,
            0.01 * i as f64,
            -0.02 * i as f64,
            9.81
        );
        imu_ch.write(log_time, log_time, imu_data.as_bytes())?;
        msg_count += 1;

        // GPS every 10th message
        if i % 10 == 0 {
            let gps_data = format!(
                r#"{{"lat":{:.6},"lon":{:.6}}}"#,
                37.7749 + i as f64 * 0.00001,
                -122.4194 + i as f64 * 0.00001
            );
            gps_ch.write(log_time, log_time, gps_data.as_bytes())?;
            msg_count += 1;
        }
    }

    writer.finish()?;

    let file_size = std::fs::metadata(&path)?.len();
    println!(
        "Wrote {} messages, file size: {} bytes",
        msg_count, file_size
    );

    // Read back and count chunks
    let mut reader = reader::Builder::new().build(std::fs::File::open(&path)?)?;
    let mut chunk_count = 0;
    for chunk in reader.chunks() {
        let chunk = chunk?;
        chunk_count += 1;
        if chunk_count <= 3 {
            println!(
                "  Chunk {}: compression={} uncompressed_size={}",
                chunk_count, chunk.compression, chunk.uncompressed_size,
            );
        }
    }
    println!("Total chunks: {}", chunk_count);

    Ok(())
}
