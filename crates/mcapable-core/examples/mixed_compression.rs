//! Runnable demo for the per-channel chunk_override feature.
//!
//! Writes one MCAP file per mode (all-compressed, mixed, all-uncompressed)
//! to /tmp and prints elapsed wall time + output size for each. Mirrors
//! the workload used by the criterion benches but doesn't depend on
//! criterion, so it can be run quickly by hand:
//!
//! ```sh
//! cargo run -p mcapable-core --example mixed_compression --release
//! ```

use bytes::Bytes;
use mcapable_core::Compression;
use mcapable_core::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};
use rand::SeedableRng;
use rand::rngs::SmallRng;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::time::Instant;

const VIDEO_PAYLOAD_BYTES: usize = 200 * 1024;
const VIDEO_HZ: u64 = 30;
const TELEMETRY_PAYLOAD_BYTES: usize = 256;
const TELEMETRY_HZ: u64 = 100;
const DURATION_SECS: u64 = 5;

#[derive(Clone)]
struct TestMsg {
    is_video: bool,
    log_time_ns: u64,
    payload: Bytes,
}

fn build_messages() -> Vec<TestMsg> {
    use rand::RngCore;
    let mut rng = SmallRng::seed_from_u64(0xC0FFEE);
    let video_count = VIDEO_HZ * DURATION_SECS;
    let telemetry_count = TELEMETRY_HZ * DURATION_SECS;
    let mut msgs = Vec::with_capacity((video_count + telemetry_count) as usize);
    for i in 0..video_count {
        let mut buf = vec![0u8; VIDEO_PAYLOAD_BYTES];
        rng.fill_bytes(&mut buf);
        msgs.push(TestMsg {
            is_video: true,
            log_time_ns: i * (1_000_000_000 / VIDEO_HZ),
            payload: Bytes::from(buf),
        });
    }
    for i in 0..telemetry_count {
        msgs.push(TestMsg {
            is_video: false,
            log_time_ns: i * (1_000_000_000 / TELEMETRY_HZ),
            payload: Bytes::from(vec![b'X'; TELEMETRY_PAYLOAD_BYTES]),
        });
    }
    msgs.sort_by_key(|m| m.log_time_ns);
    msgs
}

fn write_file(
    path: &PathBuf,
    msgs: &[TestMsg],
    video_override: Option<ChunkOptions>,
    default_compression: Option<Compression>,
) {
    let file = BufWriter::new(File::create(path).unwrap());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: default_compression,
            max_uncompressed_bytes: 4 * 1024 * 1024,
            include_crc: true,
        })
        .build(file)
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut video_spec = ChannelSpec::new("/video", "h264").schema(schema.clone());
    if let Some(opts) = video_override {
        video_spec = video_spec.chunk_override(opts);
    }
    let mut video_ch = writer.add_channel(video_spec).unwrap();
    let mut telemetry_ch = writer
        .add_channel(ChannelSpec::new("/telemetry", "raw").schema(schema))
        .unwrap();

    for m in msgs {
        let data = m.payload.clone();
        if m.is_video {
            video_ch.write(m.log_time_ns, m.log_time_ns, data).unwrap();
        } else {
            telemetry_ch
                .write(m.log_time_ns, m.log_time_ns, data)
                .unwrap();
        }
    }
    drop(video_ch);
    drop(telemetry_ch);
    writer.finish().unwrap();
    // Drop the writer to flush BufWriter via into_inner.
    let _ = writer.into_inner();
}

fn main() {
    let msgs = build_messages();
    let total_in: u64 = msgs.iter().map(|m| m.payload.len() as u64).sum();
    println!("input messages: {}, input bytes: {}", msgs.len(), total_in);

    let cases: &[(&str, Option<Compression>, Option<ChunkOptions>)] = &[
        ("all_compressed_zstd", Some(Compression::Zstd), None),
        (
            "mixed_video_uncompressed",
            Some(Compression::Zstd),
            Some(ChunkOptions {
                compression: None,
                max_uncompressed_bytes: 4 * 1024 * 1024,
                include_crc: true,
            }),
        ),
        ("all_uncompressed", None, None),
    ];

    for (label, default_compression, video_override) in cases {
        let path = PathBuf::from(format!("/tmp/mcapable_{label}.mcap"));
        let start = Instant::now();
        write_file(
            &path,
            &msgs,
            video_override.clone(),
            default_compression.clone(),
        );
        let elapsed = start.elapsed();
        let size = std::fs::metadata(&path).unwrap().len();
        println!(
            "{:<28} write {:>7.3}s, output {:>12} bytes  ({:>6.2}x input)",
            label,
            elapsed.as_secs_f64(),
            size,
            size as f64 / total_in as f64,
        );
    }
}
