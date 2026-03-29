//! Internal benchmarks for mcapable reader performance.
//!
//! Benchmarks cover:
//! - Magic byte validation
//! - Header parsing
//! - Record iteration
//! - Chunk decompression
//! - Message filtering
//! - Stream creation and iteration
//!
//! Run with: cargo bench --bench reader_benchmark

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use mcapable::reader;

mod common;
use common::{Compression, McapSpec, build_mcap_with_mcap_crate, default_specs};

/// Create a small MCAP file for basic benchmarks.
fn create_small_mcap() -> Bytes {
    let mut spec = default_specs()
        .into_iter()
        .find(|s| s.name == "small" && s.chunked && s.compression == Compression::None)
        .expect("missing default spec: small chunked none");
    spec.channels = 1;
    spec.messages = 3;
    spec.payload_bytes = 100;
    build_mcap_with_mcap_crate(&spec)
}

/// Create a medium MCAP file with multiple channels.
fn create_medium_mcap() -> Bytes {
    let mut spec = default_specs()
        .into_iter()
        .find(|s| s.name == "medium" && s.chunked && s.compression == Compression::None)
        .expect("missing default spec: medium chunked none");
    spec.channels = 5;
    spec.messages = 100;
    spec.payload_bytes = 200;
    build_mcap_with_mcap_crate(&spec)
}

/// Create a large MCAP file for stress testing.
fn create_large_mcap() -> Bytes {
    let mut spec = default_specs()
        .into_iter()
        .find(|s| s.name == "large" && s.chunked && s.compression == Compression::None)
        .expect("missing default spec: large chunked none");
    spec.channels = 10;
    spec.messages = 1_000;
    spec.payload_bytes = 512;
    spec.chunk_size = Some(4096);
    build_mcap_with_mcap_crate(&spec)
}

/// Benchmark reader construction and magic byte validation.
fn bench_reader_construction(c: &mut Criterion) {
    let mcap = create_small_mcap();

    c.bench_function("reader_construction", |b| {
        b.iter(|| {
            let reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
            black_box(reader);
        });
    });
}

/// Benchmark header loading.
fn bench_header_loading(c: &mut Criterion) {
    let mcap = create_small_mcap();

    c.bench_function("header_loading", |b| {
        b.iter(|| {
            let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
            let header = reader.header().unwrap();
            black_box(header);
        });
    });
}

/// Benchmark summary loading.
fn bench_summary_loading(c: &mut Criterion) {
    let mcap = create_medium_mcap();

    c.bench_function("summary_loading", |b| {
        b.iter(|| {
            let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
            let summary = reader.summary().unwrap();
            black_box(summary);
        });
    });
}

/// Benchmark record iteration.
fn bench_record_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_iteration");

    for &size in &["small", "medium", "large"] {
        let (spec, mcap) = match size {
            "small" => {
                let spec = McapSpec {
                    name: "small",
                    channels: 1,
                    messages: 3,
                    payload_bytes: 100,
                    chunked: true,
                    compression: Compression::None,
                    chunk_size: Some(1024 * 1024),
                };
                let bytes = build_mcap_with_mcap_crate(&spec);
                (spec, bytes)
            }
            "medium" => {
                let spec = McapSpec {
                    name: "medium",
                    channels: 5,
                    messages: 100,
                    payload_bytes: 200,
                    chunked: true,
                    compression: Compression::None,
                    chunk_size: Some(1024 * 1024),
                };
                let bytes = build_mcap_with_mcap_crate(&spec);
                (spec, bytes)
            }
            "large" => {
                let spec = McapSpec {
                    name: "large",
                    channels: 10,
                    messages: 1_000,
                    payload_bytes: 512,
                    chunked: true,
                    compression: Compression::None,
                    chunk_size: Some(4096),
                };
                let bytes = build_mcap_with_mcap_crate(&spec);
                (spec, bytes)
            }
            _ => unreachable!(),
        };

        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));
        group.bench_with_input(BenchmarkId::from_parameter(size), &mcap, |b, mcap| {
            b.iter(|| {
                let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
                let mut count: usize = 0;
                for record in reader.records() {
                    let _ = black_box(record);
                    count += 1;
                }
                black_box(count);
            });
        });
    }

    group.finish();
}

/// Benchmark chunk iteration.
fn bench_chunk_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunk_iteration");

    for &size in &["medium", "large"] {
        let mcap = match size {
            "medium" => create_medium_mcap(),
            "large" => create_large_mcap(),
            _ => unreachable!(),
        };

        group.bench_with_input(BenchmarkId::from_parameter(size), &mcap, |b, mcap| {
            b.iter(|| {
                let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
                let mut count: usize = 0;
                for chunk in reader.chunks() {
                    let _ = black_box(chunk);
                    count += 1;
                }
                black_box(count);
            });
        });
    }

    group.finish();
}

/// Benchmark message iteration (with decompression).
fn bench_message_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_iteration");

    for &size in &["small", "medium", "large"] {
        let mcap = match size {
            "small" => create_small_mcap(),
            "medium" => create_medium_mcap(),
            "large" => create_large_mcap(),
            _ => unreachable!(),
        };

        group.throughput(Throughput::Bytes(mcap.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &mcap, |b, mcap| {
            b.iter(|| {
                let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
                let mut count: usize = 0;
                for msg in reader.raw_messages().unwrap() {
                    let msg = msg.unwrap();
                    black_box(msg.data_len());
                    count += 1;
                }
                black_box(count);
            });
        });
    }

    group.finish();
}

/// Benchmark time-range filtering.
fn bench_time_filtering(c: &mut Criterion) {
    let mcap = create_large_mcap();

    c.bench_function("time_range_filter", |b| {
        b.iter(|| {
            let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
            let mut count: usize = 0;
            for msg in reader.raw_messages().unwrap().time_range(5000, 7000) {
                let msg = msg.unwrap();
                black_box(msg.data_len());
                count += 1;
            }
            black_box(count);
        });
    });
}

/// Benchmark channel filtering.
fn bench_channel_filtering(c: &mut Criterion) {
    let mcap = create_large_mcap();

    c.bench_function("channel_filter", |b| {
        b.iter(|| {
            let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
            let mut count: usize = 0;
            for msg in reader
                .raw_messages()
                .unwrap()
                .filter_channel(|ch| matches!(ch.id, 0..=2))
            {
                let msg = msg.unwrap();
                black_box(msg.data_len());
                count += 1;
            }
            black_box(count);
        });
    });
}

/// Benchmark combined filtering.
fn bench_combined_filtering(c: &mut Criterion) {
    let mcap = create_large_mcap();

    c.bench_function("combined_filters", |b| {
        b.iter(|| {
            let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();
            let mut count: usize = 0;
            for msg in reader
                .raw_messages()
                .unwrap()
                .time_range(5000, 7000)
                .filter_channel(|ch| matches!(ch.id, 0..=2))
            {
                let msg = msg.unwrap();
                if msg.sequence % 2 == 0 {
                    black_box(msg.data_len());
                    count += 1;
                }
            }
            black_box(count);
        });
    });
}

/// Benchmark metadata access during iteration.
fn bench_metadata_access(c: &mut Criterion) {
    let mcap = create_medium_mcap();

    c.bench_function("metadata_access", |b| {
        b.iter(|| {
            let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();

            // Cache metadata
            let channels = reader.channels();

            // Iterate with metadata lookups
            for msg_result in reader.messages().unwrap() {
                let msg = msg_result.unwrap();
                let channel = channels.get(&msg.channel_id);
                black_box(channel);
            }
        });
    });
}

/// Benchmark repeated iteration (stream reuse).
fn bench_repeated_iteration(c: &mut Criterion) {
    let mcap = create_medium_mcap();

    c.bench_function("repeated_iteration", |b| {
        b.iter(|| {
            let mut reader = reader::Builder::new().build_bytes(mcap.clone()).unwrap();

            // Iterate 3 times
            for _ in 0..3 {
                let mut count: usize = 0;
                for msg in reader.raw_messages().unwrap() {
                    let msg = msg.unwrap();
                    black_box(msg.data_len());
                    count += 1;
                }
                black_box(count);
            }
        });
    });
}

criterion_group!(
    benches,
    bench_reader_construction,
    bench_header_loading,
    bench_summary_loading,
    bench_record_iteration,
    bench_chunk_iteration,
    bench_message_iteration,
    bench_time_filtering,
    bench_channel_filtering,
    bench_combined_filtering,
    bench_metadata_access,
    bench_repeated_iteration,
);

criterion_main!(benches);
