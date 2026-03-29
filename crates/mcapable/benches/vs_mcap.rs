//! Comparison benchmarks between mcapable and the official mcap crate.
//!
//! This measures relative performance for common operations:
//! - Message iteration
//! - Filtered iteration
//! - Setup / construction
//!
//! Run with: cargo bench --bench vs_mcap

use bytes::Bytes;
use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use memmap::Mmap;
use std::io::{Seek, SeekFrom, Write};
use tempfile::tempfile_in;

mod common;
use common::{build_mcap_with_mcap_crate, default_specs};

fn bench_disk_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/disk_roundtrip");

    for spec in default_specs() {
        let mcap_data = build_mcap_with_mcap_crate(&spec);
        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));

        group.bench_with_input(
            BenchmarkId::new("mcapable", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || {
                        let file = tempfile_in("./").unwrap();
                        (data.clone(), file)
                    },
                    |(data, mut file)| {
                        file.write_all(data.as_ref()).unwrap();
                        file.sync_all().unwrap();
                        let mmap = unsafe { Mmap::map(&file) }.unwrap();

                        let mut reader = mcapable::reader::Builder::new()
                            .build_bytes(Bytes::from_owner(mmap))
                            .unwrap();
                        let mut count: usize = 0;
                        for msg in reader.raw_messages().unwrap() {
                            let msg = msg.unwrap();
                            black_box(msg.data_len());
                            count += 1;
                        }
                        black_box(count);
                    },
                    BatchSize::PerIteration,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || {
                        let file = tempfile_in("./").unwrap();
                        (data.clone(), file)
                    },
                    |(data, mut file)| {
                        file.write_all(data.as_ref()).unwrap();
                        file.sync_all().unwrap();
                        file.seek(SeekFrom::Start(0)).unwrap();
                        let mmap = unsafe { Mmap::map(&file) }.unwrap();

                        let stream = mcap::read::MessageStream::new(mmap.as_ref()).unwrap();
                        let mut count: usize = 0;
                        for msg in stream {
                            let msg = msg.unwrap();
                            black_box(msg.data.as_ref().len());
                            count += 1;
                        }
                        black_box(count);
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

fn bench_message_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/message_iteration");

    for spec in default_specs() {
        let mcap_data = build_mcap_with_mcap_crate(&spec);
        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));

        group.bench_with_input(
            BenchmarkId::new("mcapable", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let mut reader =
                            mcapable::reader::Builder::new().build_bytes(data).unwrap();
                        let mut count: usize = 0;
                        for msg in reader.raw_messages().unwrap() {
                            let msg = msg.unwrap();
                            black_box(msg.data_len());
                            count += 1;
                        }
                        black_box(count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let stream = mcap::read::MessageStream::new(data.as_ref()).unwrap();
                        let mut count: usize = 0;
                        for msg in stream {
                            let msg = msg.unwrap();
                            black_box(msg.data.as_ref().len());
                            count += 1;
                        }
                        black_box(count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark filtered iteration comparison.
/// Note: mcapable uses channel ID filtering (pre-indexed), while mcap_crate
/// requires topic string comparison. This reflects real-world API differences.
fn bench_filtered_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/filtered_iteration");

    for spec in default_specs() {
        let mcap_data = build_mcap_with_mcap_crate(&spec);
        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));

        group.bench_with_input(
            BenchmarkId::new("mcapable", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let mut reader =
                            mcapable::reader::Builder::new().build_bytes(data).unwrap();

                        let mut count: usize = 0;
                        for msg in reader.raw_messages().unwrap().filter_channel(|ch| {
                            ch.topic == "/channel/0"
                                || ch.topic == "/channel/1"
                                || ch.topic == "/channel/2"
                        }) {
                            let msg = msg.unwrap();
                            black_box(msg.data_len());
                            count += 1;
                        }
                        black_box(count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let stream = mcap::read::MessageStream::new(data.as_ref()).unwrap();
                        let mut count: usize = 0;
                        for msg in stream {
                            let msg = msg.unwrap();
                            if msg.channel.topic == "/channel/0"
                                || msg.channel.topic == "/channel/1"
                                || msg.channel.topic == "/channel/2"
                            {
                                black_box(msg.data.as_ref().len());
                                count += 1;
                            }
                        }
                        black_box(count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_time_range_filtered_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/time_range_filter");

    for spec in default_specs() {
        let mcap_data = build_mcap_with_mcap_crate(&spec);
        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));

        let start = 1_000 + 10 * 100;
        let end = 1_000 + 10 * 500;

        group.bench_with_input(
            BenchmarkId::new("mcapable", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let mut reader =
                            mcapable::reader::Builder::new().build_bytes(data).unwrap();
                        let mut count: usize = 0;
                        for msg in reader.raw_messages().unwrap() {
                            let msg = msg.unwrap();
                            if msg.log_time >= start && msg.log_time <= end {
                                black_box(msg.data_len());
                                count += 1;
                            }
                        }
                        black_box(count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let stream = mcap::read::MessageStream::new(data.as_ref()).unwrap();
                        let mut count: usize = 0;
                        for msg in stream {
                            let msg = msg.unwrap();
                            if msg.log_time >= start && msg.log_time <= end {
                                black_box(msg.data.as_ref().len());
                                count += 1;
                            }
                        }
                        black_box(count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_combined_filtered_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/combined_filter");

    for spec in default_specs() {
        let mcap_data = build_mcap_with_mcap_crate(&spec);
        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));

        let start = 1_000 + 10 * 100;
        let end = 1_000 + 10 * 500;

        group.bench_with_input(
            BenchmarkId::new("mcapable", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let mut reader =
                            mcapable::reader::Builder::new().build_bytes(data).unwrap();

                        let mut count: usize = 0;
                        for msg in reader
                            .raw_messages()
                            .unwrap()
                            .time_range(start, end)
                            .filter_channel(|ch| ch.topic == "/channel/0")
                        {
                            let msg = msg.unwrap();
                            black_box(msg.data_len());
                            count += 1;
                        }
                        black_box(count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let stream = mcap::read::MessageStream::new(data.as_ref()).unwrap();
                        let mut count: usize = 0;
                        for msg in stream {
                            let msg = msg.unwrap();
                            if msg.log_time >= start
                                && msg.log_time <= end
                                && msg.channel.topic == "/channel/0"
                            {
                                black_box(msg.data.as_ref().len());
                                count += 1;
                            }
                        }
                        black_box(count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_setup_costs(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/setup");

    for spec in default_specs() {
        let mcap_data = build_mcap_with_mcap_crate(&spec);
        group.throughput(Throughput::Bytes(mcap_data.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("mcapable/build", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let reader = mcapable::reader::Builder::new().build_bytes(data).unwrap();
                        black_box(reader);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcapable/build+header", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let mut reader =
                            mcapable::reader::Builder::new().build_bytes(data).unwrap();
                        let header = reader.header().unwrap();
                        black_box(header);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcapable/build+summary", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let mut reader =
                            mcapable::reader::Builder::new().build_bytes(data).unwrap();
                        let summary = reader.summary().unwrap();
                        black_box(summary);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate/stream_new", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let stream = mcap::read::MessageStream::new(data.as_ref()).unwrap();
                        black_box(stream);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate/stream_new+first", spec.case_id()),
            &mcap_data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |data| {
                        let mut stream = mcap::read::MessageStream::new(data.as_ref()).unwrap();
                        let first = stream.next();
                        black_box(first);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_setup_costs,
    bench_disk_roundtrip,
    bench_message_iteration,
    bench_filtered_iteration,
    bench_time_range_filtered_iteration,
    bench_combined_filtered_iteration,
);

criterion_main!(benches);
