//! Benchmarks for the per-channel chunk_override feature.
//!
//! Workload: ~30 Hz "video" channel with random 200 KB payloads (high
//! entropy — incompressible) interleaved with ~100 Hz "telemetry" channel
//! with repeating 256 B payloads (highly compressible). Three write modes:
//! all-compressed (baseline), mixed (video uncompressed via override,
//! telemetry compressed), all-uncompressed (upper bound on speed).
//!
//! Run with: `cargo bench -p mcapable-core --bench mixed_compression`

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use mcapable_core::Compression;
use mcapable_core::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};
use rand::SeedableRng;
use rand::rngs::SmallRng;
use std::io::Cursor;

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

/// Build the workload once and return it. Total payload bytes is the
/// throughput axis the bench reports against.
fn build_messages() -> (Vec<TestMsg>, u64) {
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
        // Highly compressible repeating pattern.
        let buf = vec![b'X'; TELEMETRY_PAYLOAD_BYTES];
        msgs.push(TestMsg {
            is_video: false,
            log_time_ns: i * (1_000_000_000 / TELEMETRY_HZ),
            payload: Bytes::from(buf),
        });
    }
    msgs.sort_by_key(|m| m.log_time_ns);

    let total_bytes: u64 = msgs.iter().map(|m| m.payload.len() as u64).sum();
    (msgs, total_bytes)
}

#[derive(Clone, Copy)]
enum Mode {
    AllCompressedZstd,
    MixedVideoUncompressed,
    AllUncompressed,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::AllCompressedZstd => "all_compressed_zstd",
            Mode::MixedVideoUncompressed => "mixed_video_uncompressed",
            Mode::AllUncompressed => "all_uncompressed",
        }
    }
}

/// Write `msgs` to an in-memory MCAP file in the given mode.
/// Returns the file bytes (used by the read benches).
fn write_one(msgs: &[TestMsg], mode: Mode) -> Vec<u8> {
    let (default_compression, video_override) = match mode {
        Mode::AllCompressedZstd => (Some(Compression::Zstd), None::<ChunkOptions>),
        Mode::MixedVideoUncompressed => (
            Some(Compression::Zstd),
            Some(ChunkOptions {
                compression: None,
                max_uncompressed_bytes: 4 * 1024 * 1024,
                include_crc: true,
            }),
        ),
        Mode::AllUncompressed => (None, None),
    };

    let out = Cursor::new(Vec::with_capacity(64 * 1024 * 1024));
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: default_compression,
            max_uncompressed_bytes: 4 * 1024 * 1024,
            include_crc: true,
        })
        .build(out)
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
    writer.into_inner().into_inner()
}

fn bench_writes(c: &mut Criterion) {
    let (msgs, total_bytes) = build_messages();
    let mut group = c.benchmark_group("write_mixed_compression");
    group.throughput(Throughput::Bytes(total_bytes));

    for mode in [
        Mode::AllCompressedZstd,
        Mode::MixedVideoUncompressed,
        Mode::AllUncompressed,
    ] {
        // One sample run to print output size alongside time.
        let sample_bytes = write_one(&msgs, mode);
        eprintln!(
            "write[{}]: input={} bytes, output={} bytes",
            mode.label(),
            total_bytes,
            sample_bytes.len(),
        );

        group.bench_with_input(BenchmarkId::from_parameter(mode.label()), &mode, |b, &m| {
            b.iter(|| {
                let bytes = write_one(black_box(&msgs), m);
                let _ = black_box(bytes);
            });
        });
    }
    group.finish();
}

fn bench_reads(c: &mut Criterion) {
    let (msgs, total_bytes) = build_messages();

    // Pre-build one file per mode (outside the measured loop).
    let files = [
        (
            Mode::AllCompressedZstd,
            write_one(&msgs, Mode::AllCompressedZstd),
        ),
        (
            Mode::MixedVideoUncompressed,
            write_one(&msgs, Mode::MixedVideoUncompressed),
        ),
        (
            Mode::AllUncompressed,
            write_one(&msgs, Mode::AllUncompressed),
        ),
    ];

    let mut group = c.benchmark_group("read_mixed_compression");
    group.throughput(Throughput::Bytes(total_bytes));

    for (mode, bytes) in &files {
        group.bench_with_input(
            BenchmarkId::from_parameter(mode.label()),
            bytes.as_slice(),
            |b, file_bytes| {
                b.iter(|| {
                    let mut reader = mcapable_core::reader::Reader::from_slice(file_bytes).unwrap();
                    let mut total = 0usize;
                    for raw in reader.raw_messages().unwrap() {
                        let raw = raw.unwrap();
                        total = total.wrapping_add(raw.data_len());
                    }
                    black_box(total);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_writes, bench_reads);
criterion_main!(benches);
