//! Comparison benchmarks between mcapable-core Writer and the official mcap crate writer.
//!
//! Run with: cargo bench --bench writer_vs_mcap

use bytes::Bytes;
use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use memmap::Mmap;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufWriter, Cursor, Seek, SeekFrom, Write};
use tempfile::tempfile_in;

mod common;
use common::{Compression, McapSpec};

fn writer_specs() -> Vec<McapSpec> {
    // Keep this small enough to be usable on laptops but still representative.
    let base = [
        ("small", 5u16, 15_000u32, 64usize),
        ("medium", 10u16, 5_000u32, 128usize),
        ("large", 20u16, 100u32, 524288usize),
    ];

    let mut out = Vec::new();
    for (name, channels, messages, payload_bytes) in base {
        out.push(McapSpec {
            name,
            channels,
            messages,
            payload_bytes,
            chunked: true,
            compression: Compression::None,
            chunk_size: Some(4 * 1024 * 1024),
        });
        out.push(McapSpec {
            name,
            channels,
            messages,
            payload_bytes,
            chunked: true,
            compression: Compression::Zstd,
            chunk_size: Some(4 * 1024 * 1024),
        });
        out.push(McapSpec {
            name,
            channels,
            messages,
            payload_bytes,
            chunked: true,
            compression: Compression::Lz4,
            chunk_size: Some(4 * 1024 * 1024),
        });
        out.push(McapSpec {
            name,
            channels,
            messages,
            payload_bytes,
            chunked: false,
            compression: Compression::None,
            chunk_size: None,
        });
    }
    out
}

fn to_mcapable_compression(compression: Compression) -> Option<mcapable::Compression> {
    match compression {
        Compression::None => None,
        Compression::Lz4 => Some(mcapable::Compression::Lz4),
        Compression::Zstd => Some(mcapable::Compression::Zstd),
    }
}

fn to_mcap_crate_compression(compression: Compression) -> Option<mcap::Compression> {
    match compression {
        Compression::None => None,
        Compression::Lz4 => Some(mcap::Compression::Lz4),
        Compression::Zstd => Some(mcap::Compression::Zstd),
    }
}

fn make_payloads(payload_bytes: usize, messages: u32) -> Vec<Vec<u8>> {
    let pool_len = (messages as usize).max(1);
    let mut seed: u32 = 0x1234_5678;
    let mut payloads = Vec::with_capacity(pool_len);

    for _ in 0..pool_len {
        let mut buf = vec![0u8; payload_bytes];
        for b in &mut buf {
            // Deterministic LCG for stable, non-zero-ish payload bytes.
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *b = (seed >> 24) as u8;
        }
        payloads.push(buf);
    }

    payloads
}

// Setup data that can be reused across iterations
#[derive(Clone)]
struct McapableSetup {
    channels: Vec<mcapable::Channel>,
    builder: mcapable::WriterBuilder,
}

#[derive(Clone)]
struct McapCrateSetup {
    topics: Vec<String>,
    message_encoding: String,
    metadata: BTreeMap<String, String>,
    opts: mcap::WriteOptions,
}

struct DeferredMcapableFinish<W: Write + Seek> {
    writer: mcapable::Writer<W>,
    #[allow(dead_code)] // Keep channel writers alive until after finish() runs in Drop.
    channel_writers: Vec<mcapable::ChannelWriter<W>>,
}

impl<W: Write + Seek> Drop for DeferredMcapableFinish<W> {
    fn drop(&mut self) {
        let _ = self.writer.finish();
    }
}

struct DeferredMcapFinish<W: Write + Seek> {
    writer: mcap::Writer<W>,
}

impl<W: Write + Seek> Drop for DeferredMcapFinish<W> {
    fn drop(&mut self) {
        let _ = self.writer.finish();
    }
}

fn bench_writer_to_mem(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/writer_throughput");

    for spec in writer_specs() {
        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));

        // Setup for mcapable: create channels and builder once
        let mcapable_setup = McapableSetup {
            channels: (0..spec.channels)
                .map(|ch| mcapable::Channel {
                    id: ch + 1,
                    topic: format!("/channel/{ch}").as_str().into(),
                    message_encoding: "application/octet-stream".into(),
                    schema_id: 0,
                    metadata: HashMap::new(),
                })
                .collect(),
            builder: if spec.chunked {
                mcapable::WriterBuilder::new().chunked(mcapable::ChunkOptions {
                    compression: to_mcapable_compression(spec.compression),
                    max_uncompressed_bytes: spec.chunk_size.unwrap_or(4 * 1024 * 1024),
                })
            } else {
                mcapable::WriterBuilder::new()
            },
        };

        // Setup for mcap_crate: create channels and options once
        let mcap_crate_setup = McapCrateSetup {
            topics: (0..spec.channels)
                .map(|ch| format!("/channel/{ch}"))
                .collect(),
            message_encoding: "application/octet-stream".to_string(),
            metadata: BTreeMap::new(),
            opts: {
                let mut opts = mcap::WriteOptions::new()
                    .use_chunks(spec.chunked)
                    .compression(to_mcap_crate_compression(spec.compression));
                opts = opts.chunk_size(spec.chunk_size.map(|s| s as u64));
                opts
            },
        };

        group.bench_with_input(
            BenchmarkId::new("mcapable", spec.case_id()),
            &(spec.clone(), mcapable_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        // Setup: create writer and write channels (extracted from loop)
                        let out = Cursor::new(Vec::new());
                        let mut writer = setup.builder.clone().build(out).unwrap();
                        let mut channel_writers = Vec::with_capacity(setup.channels.len());
                        for channel in &setup.channels {
                            channel_writers.push(writer.copy_channel(channel).unwrap());
                        }
                        let payloads = make_payloads(spec.payload_bytes, spec.messages);
                        (spec.clone(), writer, channel_writers, payloads)
                    },
                    |(spec, mut writer, mut channel_writers, mut payloads)| {
                        // Write messages using ChannelWriter (this is what we're benchmarking)
                        for i in 0..spec.messages {
                            let channel_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let log_time = 1_000 + (i as u64) * 10;
                            channel_writers[channel_idx]
                                .write(
                                    log_time,
                                    log_time,
                                    std::mem::take(&mut payloads[payload_idx]),
                                )
                                .unwrap();
                        }

                        writer.finish().unwrap();
                        drop(channel_writers);
                        let out_len = writer.into_inner().into_inner().len();
                        black_box(out_len);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcapable_writes_only", spec.case_id()),
            &(spec.clone(), mcapable_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        let out = Cursor::new(Vec::new());
                        let mut writer = setup.builder.clone().build(out).unwrap();
                        let mut channel_writers = Vec::with_capacity(setup.channels.len());
                        for channel in &setup.channels {
                            channel_writers.push(writer.copy_channel(channel).unwrap());
                        }
                        let payloads = make_payloads(spec.payload_bytes, spec.messages);
                        (writer, channel_writers, payloads)
                    },
                    |(writer, mut channel_writers, mut payloads)| {
                        for i in 0..spec.messages {
                            let channel_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let log_time = 1_000 + (i as u64) * 10;
                            channel_writers[channel_idx]
                                .write(
                                    log_time,
                                    log_time,
                                    std::mem::take(&mut payloads[payload_idx]),
                                )
                                .unwrap();
                        }

                        // Return a "large drop" output so finish() + drop is not timed.
                        DeferredMcapableFinish {
                            writer,
                            channel_writers,
                        }
                    },
                    // Ensure we don't batch large writer outputs and OOM.
                    BatchSize::PerIteration,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcapable_slice", spec.case_id()),
            &(spec.clone(), mcapable_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        let out = Cursor::new(Vec::new());
                        let mut writer = setup.builder.clone().build(out).unwrap();
                        let mut channel_writers = Vec::with_capacity(setup.channels.len());
                        for channel in &setup.channels {
                            channel_writers.push(writer.copy_channel(channel).unwrap());
                        }
                        let payloads = make_payloads(spec.payload_bytes, spec.messages);
                        (spec.clone(), writer, channel_writers, payloads)
                    },
                    |(spec, mut writer, mut channel_writers, payloads)| {
                        for i in 0..spec.messages {
                            let channel_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let log_time = 1_000 + (i as u64) * 10;
                            channel_writers[channel_idx]
                                .write(log_time, log_time, payloads[payload_idx].as_slice())
                                .unwrap();
                        }

                        writer.finish().unwrap();
                        drop(channel_writers);
                        let out_len = writer.into_inner().into_inner().len();
                        black_box(out_len);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcapable_slice_writes_only", spec.case_id()),
            &(spec.clone(), mcapable_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        let out = Cursor::new(Vec::new());
                        let mut writer = setup.builder.clone().build(out).unwrap();
                        let mut channel_writers = Vec::with_capacity(setup.channels.len());
                        for channel in &setup.channels {
                            channel_writers.push(writer.copy_channel(channel).unwrap());
                        }
                        let payloads = make_payloads(spec.payload_bytes, spec.messages);
                        (writer, channel_writers, payloads)
                    },
                    |(writer, mut channel_writers, payloads)| {
                        for i in 0..spec.messages {
                            let channel_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let log_time = 1_000 + (i as u64) * 10;
                            channel_writers[channel_idx]
                                .write(log_time, log_time, payloads[payload_idx].as_slice())
                                .unwrap();
                        }

                        DeferredMcapableFinish {
                            writer,
                            channel_writers,
                        }
                    },
                    BatchSize::PerIteration,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate", spec.case_id()),
            &(spec.clone(), mcap_crate_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        let payloads = make_payloads(spec.payload_bytes, spec.messages);
                        (spec.clone(), setup, payloads)
                    },
                    |(spec, setup, payloads)| {
                        // Setup: create writer and add channels (extracted from loop)
                        let mut out = Cursor::new(Vec::new());
                        let mut writer = setup.opts.clone().create(&mut out).unwrap();
                        let channel_ids: Vec<u16> = setup
                            .topics
                            .iter()
                            .map(|topic| {
                                writer
                                    .add_channel(0, topic, &setup.message_encoding, &setup.metadata)
                                    .unwrap()
                            })
                            .collect();
                        // Write messages using write_to_known_channel (this is what we're benchmarking)
                        for i in 0..spec.messages {
                            let ch_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let header = mcap::records::MessageHeader {
                                channel_id: channel_ids[ch_idx],
                                sequence: i,
                                log_time: 1_000 + (i as u64) * 10,
                                publish_time: 1_000 + (i as u64) * 10,
                            };
                            writer
                                .write_to_known_channel(&header, payloads[payload_idx].as_slice())
                                .unwrap();
                        }

                        writer.finish().unwrap();
                        drop(writer);

                        let out_len = out.into_inner().len();
                        black_box(out_len);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate_writes_only", spec.case_id()),
            &(spec.clone(), mcap_crate_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        let out = Cursor::new(Vec::new());
                        let mut writer = setup.opts.clone().create(out).unwrap();
                        let channel_ids: Vec<u16> = setup
                            .topics
                            .iter()
                            .map(|topic| {
                                writer
                                    .add_channel(0, topic, &setup.message_encoding, &setup.metadata)
                                    .unwrap()
                            })
                            .collect();
                        let payloads = make_payloads(spec.payload_bytes, spec.messages);
                        (writer, channel_ids, payloads)
                    },
                    |(mut writer, channel_ids, payloads)| {
                        for i in 0..spec.messages {
                            let ch_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let header = mcap::records::MessageHeader {
                                channel_id: channel_ids[ch_idx],
                                sequence: i,
                                log_time: 1_000 + (i as u64) * 10,
                                publish_time: 1_000 + (i as u64) * 10,
                            };
                            writer
                                .write_to_known_channel(&header, payloads[payload_idx].as_slice())
                                .unwrap();
                        }

                        // Return a "large drop" output so finish() + drop is not timed.
                        DeferredMcapFinish { writer }
                    },
                    // Ensure we don't batch large writer outputs and OOM.
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

fn bench_writer_disk_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/writer_disk_roundtrip");

    for spec in writer_specs() {
        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));

        let mcapable_setup = McapableSetup {
            channels: (0..spec.channels)
                .map(|ch| mcapable::Channel {
                    id: ch + 1,
                    topic: format!("/channel/{ch}").as_str().into(),
                    message_encoding: "application/octet-stream".into(),
                    schema_id: 0,
                    metadata: HashMap::new(),
                })
                .collect(),
            builder: if spec.chunked {
                mcapable::WriterBuilder::new().chunked(mcapable::ChunkOptions {
                    compression: to_mcapable_compression(spec.compression),
                    max_uncompressed_bytes: spec.chunk_size.unwrap_or(4 * 1024 * 1024),
                })
            } else {
                mcapable::WriterBuilder::new()
            },
        };

        let mcap_crate_setup = McapCrateSetup {
            topics: (0..spec.channels)
                .map(|ch| format!("/channel/{ch}"))
                .collect(),
            message_encoding: "application/octet-stream".to_string(),
            metadata: BTreeMap::new(),
            opts: {
                let mut opts = mcap::WriteOptions::new()
                    .use_chunks(spec.chunked)
                    .compression(to_mcap_crate_compression(spec.compression));
                opts = opts.chunk_size(spec.chunk_size.map(|s| s as u64));
                opts
            },
        };

        group.bench_with_input(
            BenchmarkId::new("mcapable", spec.case_id()),
            &(spec.clone(), mcapable_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        (
                            tempfile_in("./").unwrap(),
                            make_payloads(spec.payload_bytes, spec.messages),
                        )
                    },
                    |(file, mut payloads)| {
                        let mut writer = setup.builder.clone().build(file).unwrap();
                        let mut channel_writers = Vec::with_capacity(setup.channels.len());
                        for channel in &setup.channels {
                            channel_writers.push(writer.copy_channel(channel).unwrap());
                        }

                        for i in 0..spec.messages {
                            let channel_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let log_time = 1_000 + (i as u64) * 10;
                            channel_writers[channel_idx]
                                .write(
                                    log_time,
                                    log_time,
                                    std::mem::take(&mut payloads[payload_idx]),
                                )
                                .unwrap();
                        }
                        writer.finish().unwrap();
                        drop(channel_writers);

                        let file = writer.into_inner();
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
            BenchmarkId::new("mcapable_bufwriter", spec.case_id()),
            &(spec.clone(), mcapable_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        (
                            tempfile_in("./").unwrap(),
                            make_payloads(spec.payload_bytes, spec.messages),
                        )
                    },
                    |(file, mut payloads)| {
                        let buf = BufWriter::new(file);
                        let mut writer = setup.builder.clone().build(buf).unwrap();
                        let mut channel_writers = Vec::with_capacity(setup.channels.len());
                        for channel in &setup.channels {
                            channel_writers.push(writer.copy_channel(channel).unwrap());
                        }

                        for i in 0..spec.messages {
                            let channel_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let log_time = 1_000 + (i as u64) * 10;
                            channel_writers[channel_idx]
                                .write(
                                    log_time,
                                    log_time,
                                    std::mem::take(&mut payloads[payload_idx]),
                                )
                                .unwrap();
                        }
                        writer.finish().unwrap();
                        drop(channel_writers);

                        let mut buf = writer.into_inner();
                        buf.flush().unwrap();
                        let file = buf.into_inner().unwrap();
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
            &(spec.clone(), mcap_crate_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        (
                            tempfile_in("./").unwrap(),
                            make_payloads(spec.payload_bytes, spec.messages),
                        )
                    },
                    |(mut file, payloads)| {
                        let mut writer = setup.opts.clone().create(&mut file).unwrap();
                        let channel_ids: Vec<u16> = setup
                            .topics
                            .iter()
                            .map(|topic| {
                                writer
                                    .add_channel(0, topic, &setup.message_encoding, &setup.metadata)
                                    .unwrap()
                            })
                            .collect();

                        for i in 0..spec.messages {
                            let ch_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let header = mcap::records::MessageHeader {
                                channel_id: channel_ids[ch_idx],
                                sequence: i,
                                log_time: 1_000 + (i as u64) * 10,
                                publish_time: 1_000 + (i as u64) * 10,
                            };
                            writer
                                .write_to_known_channel(&header, payloads[payload_idx].as_slice())
                                .unwrap();
                        }

                        writer.finish().unwrap();
                        drop(writer);

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

        group.bench_with_input(
            BenchmarkId::new("mcap_crate_bufwriter", spec.case_id()),
            &(spec.clone(), mcap_crate_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        (
                            tempfile_in("./").unwrap(),
                            make_payloads(spec.payload_bytes, spec.messages),
                        )
                    },
                    |(file, payloads)| {
                        let mut buf = BufWriter::new(file);
                        let mut writer = setup.opts.clone().create(&mut buf).unwrap();
                        let channel_ids: Vec<u16> = setup
                            .topics
                            .iter()
                            .map(|topic| {
                                writer
                                    .add_channel(0, topic, &setup.message_encoding, &setup.metadata)
                                    .unwrap()
                            })
                            .collect();

                        for i in 0..spec.messages {
                            let ch_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let header = mcap::records::MessageHeader {
                                channel_id: channel_ids[ch_idx],
                                sequence: i,
                                log_time: 1_000 + (i as u64) * 10,
                                publish_time: 1_000 + (i as u64) * 10,
                            };
                            writer
                                .write_to_known_channel(&header, payloads[payload_idx].as_slice())
                                .unwrap();
                        }

                        writer.finish().unwrap();
                        drop(writer);

                        buf.flush().unwrap();
                        let mut file = buf.into_inner().unwrap();
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

fn bench_writer_disk_write_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("vs_mcap/writer_disk_write_only");

    for spec in writer_specs() {
        group.throughput(Throughput::Bytes(spec.estimated_payload_bytes()));

        let mcapable_setup = McapableSetup {
            channels: (0..spec.channels)
                .map(|ch| mcapable::Channel {
                    id: ch + 1,
                    topic: format!("/channel/{ch}").as_str().into(),
                    message_encoding: "application/octet-stream".into(),
                    schema_id: 0,
                    metadata: HashMap::new(),
                })
                .collect(),
            builder: if spec.chunked {
                mcapable::WriterBuilder::new().chunked(mcapable::ChunkOptions {
                    compression: to_mcapable_compression(spec.compression),
                    max_uncompressed_bytes: spec.chunk_size.unwrap_or(4 * 1024 * 1024),
                })
            } else {
                mcapable::WriterBuilder::new()
            },
        };

        let mcap_crate_setup = McapCrateSetup {
            topics: (0..spec.channels)
                .map(|ch| format!("/channel/{ch}"))
                .collect(),
            message_encoding: "application/octet-stream".to_string(),
            metadata: BTreeMap::new(),
            opts: {
                let mut opts = mcap::WriteOptions::new()
                    .use_chunks(spec.chunked)
                    .compression(to_mcap_crate_compression(spec.compression));
                opts = opts.chunk_size(spec.chunk_size.map(|s| s as u64));
                opts
            },
        };

        group.bench_with_input(
            BenchmarkId::new("mcapable", spec.case_id()),
            &(spec.clone(), mcapable_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        (
                            tempfile_in("./").unwrap(),
                            make_payloads(spec.payload_bytes, spec.messages),
                        )
                    },
                    |(file, mut payloads)| {
                        let mut writer = setup.builder.clone().build(file).unwrap();
                        let mut channel_writers = Vec::with_capacity(setup.channels.len());
                        for channel in &setup.channels {
                            channel_writers.push(writer.copy_channel(channel).unwrap());
                        }

                        for i in 0..spec.messages {
                            let channel_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let log_time = 1_000 + (i as u64) * 10;
                            channel_writers[channel_idx]
                                .write(
                                    log_time,
                                    log_time,
                                    std::mem::take(&mut payloads[payload_idx]),
                                )
                                .unwrap();
                        }
                        writer.finish().unwrap();
                        drop(channel_writers);

                        let file = writer.into_inner();
                        file.sync_all().unwrap();
                        let len = file.metadata().unwrap().len();
                        black_box(len);
                    },
                    BatchSize::PerIteration,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcapable_bufwriter", spec.case_id()),
            &(spec.clone(), mcapable_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        (
                            tempfile_in("./").unwrap(),
                            make_payloads(spec.payload_bytes, spec.messages),
                        )
                    },
                    |(file, mut payloads)| {
                        let buf = BufWriter::new(file);
                        let mut writer = setup.builder.clone().build(buf).unwrap();
                        let mut channel_writers = Vec::with_capacity(setup.channels.len());
                        for channel in &setup.channels {
                            channel_writers.push(writer.copy_channel(channel).unwrap());
                        }

                        for i in 0..spec.messages {
                            let channel_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let log_time = 1_000 + (i as u64) * 10;
                            channel_writers[channel_idx]
                                .write(
                                    log_time,
                                    log_time,
                                    std::mem::take(&mut payloads[payload_idx]),
                                )
                                .unwrap();
                        }
                        writer.finish().unwrap();
                        drop(channel_writers);

                        let mut buf = writer.into_inner();
                        buf.flush().unwrap();
                        let file = buf.into_inner().unwrap();
                        file.sync_all().unwrap();
                        let len = file.metadata().unwrap().len();
                        black_box(len);
                    },
                    BatchSize::PerIteration,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate", spec.case_id()),
            &(spec.clone(), mcap_crate_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        (
                            tempfile_in("./").unwrap(),
                            make_payloads(spec.payload_bytes, spec.messages),
                        )
                    },
                    |(mut file, payloads)| {
                        let mut writer = setup.opts.clone().create(&mut file).unwrap();
                        let channel_ids: Vec<u16> = setup
                            .topics
                            .iter()
                            .map(|topic| {
                                writer
                                    .add_channel(0, topic, &setup.message_encoding, &setup.metadata)
                                    .unwrap()
                            })
                            .collect();

                        for i in 0..spec.messages {
                            let ch_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let header = mcap::records::MessageHeader {
                                channel_id: channel_ids[ch_idx],
                                sequence: i,
                                log_time: 1_000 + (i as u64) * 10,
                                publish_time: 1_000 + (i as u64) * 10,
                            };
                            writer
                                .write_to_known_channel(&header, payloads[payload_idx].as_slice())
                                .unwrap();
                        }

                        writer.finish().unwrap();
                        drop(writer);

                        file.sync_all().unwrap();
                        let len = file.metadata().unwrap().len();
                        black_box(len);
                    },
                    BatchSize::PerIteration,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mcap_crate_bufwriter", spec.case_id()),
            &(spec.clone(), mcap_crate_setup.clone()),
            |b, (spec, setup)| {
                b.iter_batched(
                    || {
                        (
                            tempfile_in("./").unwrap(),
                            make_payloads(spec.payload_bytes, spec.messages),
                        )
                    },
                    |(file, payloads)| {
                        let mut buf = BufWriter::new(file);
                        let mut writer = setup.opts.clone().create(&mut buf).unwrap();
                        let channel_ids: Vec<u16> = setup
                            .topics
                            .iter()
                            .map(|topic| {
                                writer
                                    .add_channel(0, topic, &setup.message_encoding, &setup.metadata)
                                    .unwrap()
                            })
                            .collect();

                        for i in 0..spec.messages {
                            let ch_idx = (i % spec.channels as u32) as usize;
                            let payload_idx = i as usize;
                            let header = mcap::records::MessageHeader {
                                channel_id: channel_ids[ch_idx],
                                sequence: i,
                                log_time: 1_000 + (i as u64) * 10,
                                publish_time: 1_000 + (i as u64) * 10,
                            };
                            writer
                                .write_to_known_channel(&header, payloads[payload_idx].as_slice())
                                .unwrap();
                        }

                        writer.finish().unwrap();
                        drop(writer);

                        buf.flush().unwrap();
                        let file = buf.into_inner().unwrap();
                        file.sync_all().unwrap();
                        let len = file.metadata().unwrap().len();
                        black_box(len);
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_writer_to_mem,
    bench_writer_disk_roundtrip,
    bench_writer_disk_write_only
);
criterion_main!(benches);
