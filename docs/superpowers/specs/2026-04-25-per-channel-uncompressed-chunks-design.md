# Per-Channel Uncompressed Chunks

**Date:** 2026-04-25
**Status:** Approved (pending implementation plan)
**Crate:** `mcapable-core`

## Problem

`mcapable-core`'s writer currently routes every message through a single in-flight `ChunkState` configured by one `ChunkOptions`. Files are therefore single-compression: every chunk uses the same algorithm.

For workloads that mix already-compressed payloads (H264 frames, JPEGs, length-prefixed protobuf-then-snappy blobs) with low-entropy textual telemetry, this forces a bad tradeoff: pick zstd/lz4 and waste CPU re-compressing data that won't shrink, or pick none and bloat the file by skipping compression on the data that *would* shrink.

The MCAP spec already permits a file to contain chunks of different compression types. We want the writer to take advantage of that: callers tag specific channels at creation time, and those channels' messages flow into their own chunk stream with caller-chosen options (typically `compression: None`), while every other channel continues into the writer's default chunk stream unchanged.

A benchmark accompanies the feature to confirm the win on a mixed workload.

## Non-goals

- No per-message routing decisions. Override is bound at channel creation and never changes.
- No automatic detection of "already compressed" payloads. The caller decides.
- No new public API on `ChannelWriter` for switching modes after creation.
- No reader-side changes — readers already handle multi-compression files.
- Channels with override do **not** share chunk streams with each other; each tagged channel gets its own dedicated stream. (Sharing was considered and rejected as silently surprising.)

## Design

### API surface

`ChannelSpec` grows one optional field:

```rust
pub struct ChannelSpec {
    pub topic: ByteStr,
    pub message_encoding: ByteStr,
    pub schema: Option<SchemaSpec>,
    pub metadata: HashMap<ByteStr, ByteStr>,
    /// When `Some`, messages on this channel land in their own dedicated
    /// chunk stream configured by these options. When `None`, messages
    /// flow into the writer's default chunk stream (or top-level if the
    /// writer was built without `.chunked(...)`).
    pub chunk_override: Option<ChunkOptions>,
}

impl ChannelSpec {
    pub fn chunk_override(mut self, options: ChunkOptions) -> Self;

    /// Convenience: same as `chunk_override(ChunkOptions { compression: None, .. })`.
    /// Other fields inherit from `ChunkOptions::default()`.
    pub fn uncompressed_chunks(self) -> Self;
}
```

`Writer` gains one sibling for the copy path:

```rust
impl<W: Write + Seek> Writer<W> {
    pub fn copy_channel_with_override(
        &mut self,
        channel: &Channel,
        options: ChunkOptions,
    ) -> Result<ChannelWriter<W>>;
}
```

`copy_channel` is unchanged; existing call sites in CLI `merge`/`filter` continue to work.

`ChannelWriter` gains a private cached routing bit:

```rust
pub struct ChannelWriter<W: Write + Seek> {
    pub(crate) inner: Rc<RefCell<WriterImpl<W>>>,
    pub(crate) channel_id: u16,
    pub(crate) next_sequence: u32,
    pub(crate) has_chunk_override: bool, // new
}
```

The bit is set once at `add_channel`/`copy_channel_with_override` time. Public methods on `ChannelWriter` are unchanged.

### Internal model

`WriterImpl` adds one field alongside the existing default chunk state:

```rust
pub(crate) struct WriterImpl<W> {
    // ...existing fields...
    pub(crate) chunk_state: Option<ChunkState>,            // default stream
    pub(crate) override_streams: Vec<Option<ChunkState>>,  // indexed by channel_id
    // ...
}
```

`Vec<Option<ChunkState>>` indexed by `channel_id` mirrors the existing `channel_stats: Vec<ChannelStats>` pattern. Channel IDs are assigned sequentially from 1, so the vec stays dense; a `Some` slot means that channel id has an override registered. `chunk_indexes: Vec<ChunkIndexInfo>` is shared across all streams — entries are appended in the order chunks flush to disk, which is what the summary writer already iterates.

`WriterImpl` exposes two distinct write entry points so the default path stays free of any override-related work:

```rust
pub(crate) fn write_raw_message_default(&mut self, msg: &RawMessage) -> Result<()>;
pub(crate) fn write_raw_message_override(&mut self, msg: &RawMessage) -> Result<()>;
```

`ChannelWriter::write_with_sequence` dispatches with one bool branch on the cached `has_chunk_override`. The branch is well-predicted because a given `ChannelWriter` always takes the same arm. The default path performs no hash lookups, no vec indexing for overrides — it is byte-equivalent to the existing fast path.

The override path performs one bounds-checked vec index (`&mut self.override_streams[channel_id as usize]`) and unwraps the `Some(_)` — invariant: a `ChannelWriter` whose `has_chunk_override` is `true` always corresponds to a populated slot, established at `add_channel` time. A debug assertion guards this invariant.

### Routing rules

Per-message:

1. `has_chunk_override == true` → push into `override_streams[channel_id].as_mut().unwrap()`, then check that stream's flush threshold.
2. `has_chunk_override == false` and `chunk_state.is_some()` → existing default-chunk path.
3. `has_chunk_override == false` and `chunk_state.is_none()` → existing top-level message path.

Override stream flushing is independent: each stream uses its own `max_uncompressed_bytes` and `include_crc`. Streams do not coordinate with each other or with the default.

### Force-flush points

Today's `flush_chunk_if_needed(false)` calls (before writing top-level Schema/Channel/Attachment/Metadata records) become "let the default flush if needed *and* let each override flush if needed." The intent is unchanged — give size-thresholded chunks a chance to land before another top-level record is interleaved on disk — and the cost is one cheap `should_flush` check per stream slot.

`finish()` flushes the default first, then iterates `override_streams` and drains every `Some` slot with messages, in ascending channel-id order for deterministic output (test snapshots).

### On-disk layout (illustrative)

For a writer with default zstd chunks and one channel tagged `uncompressed_chunks()`:

```
Header
Schema(1)
Channel(1, default)
Schema(2)
Channel(2, override)
Chunk(zstd, msgs of ch1)              ← default flushed at its threshold
Chunk(no compression, msgs of ch2)    ← override flushed at its threshold
Chunk(zstd, msgs of ch1)
Chunk(no compression, msgs of ch2)
...
DataEnd
[Summary: Schema, Channel, ChunkIndex×N (mixed compressions), Statistics, ...]
Footer
```

`ChunkOptions` does **not** require `Eq` or `Hash` — channels never share streams, so there's no equality logic to implement.

### Error handling

- `ChannelSpec::chunk_override` with options that would be invalid for the default chunk (e.g., absurdly small `max_uncompressed_bytes`) are accepted; same validation as the default writer (i.e., none beyond the existing `Validation` mode).
- The invariant "`has_chunk_override == true` ⇒ `override_streams[channel_id]` is `Some`" is established at registration time and never broken (no API removes streams). Guarded by a `debug_assert!` in the override write path; release builds rely on the invariant.
- All existing error paths (write-after-finish, etc.) apply identically to both write entry points.

## Testing

### Unit tests (`writer/internal.rs` or `writer/chunk.rs`)

- `override_stream_flushes_on_its_own_threshold` — small messages to default, large messages to override; assert the override chunk appears in `chunk_indexes` before the default one even though it was registered later.
- `override_stream_uses_empty_compression_string` — assert override chunk's `ChunkIndexInfo.compression == ""` and default's is `"zstd"`.
- `finish_flushes_all_override_streams` — multiple tagged channels, partial fills, finish; assert one chunk per non-empty stream lands.
- `default_path_unchanged_when_no_overrides` — output of the new code with zero overrides is byte-identical to baseline (sanity check on the dispatch refactor).

### Integration / round-trip tests (`crates/mcapable-core/tests/writer_roundtrip.rs`)

Following the existing `writer_round_trip_chunked_*` family:

- `writer_round_trip_mixed_compression_zstd_default_uncompressed_override` — write 2 channels (default zstd, override none), reopen with `Reader`, iterate messages, assert payloads + ordering + sequences match.
- Same with `lz4` default + `none` override, and `none` default + `none` override (override behaves identically when it matches).
- `reader_chunk_stream_sees_both_compressions` — open the mixed file with `Stream<Chunk>`, collect chunks, assert both compression strings appear in the expected counts.
- `summary_chunk_indexes_round_trip_for_mixed_file` — write mixed file, reopen, assert `reader.summary().chunk_indexes` length matches what was emitted and each entry's `compression`/`compressed_size`/`uncompressed_size` matches the on-disk chunk.

### Property test (`crates/mcapable-core/tests/writer_properties.rs`)

- `prop_mixed_compression_roundtrip_preserves_messages` — proptest generates a message stream with `(channel_tag, log_time, payload)` triples where `channel_tag ∈ {default, override_a, override_b}`, payload sizes/contents arbitrary. Write, reopen, assert message set and per-channel ordering by sequence is preserved.

### CLI tests

No new CLI tests required. `filter` and `merge` already round-trip arbitrary chunk compressions, and the override path is invisible to them. Existing `cmd_properties.rs` continues to cover those flows.

## Benchmarks

New file: `crates/mcapable-core/benches/mixed_compression.rs` using `criterion`. `Cargo.toml` gets a `[[bench]]` entry and a `criterion` dev-dependency.

### Workload generator

Called once per bench iteration's setup (not measured):

- Channel A "video": random 200 KB payloads at 30 Hz, seeded `SmallRng` for determinism (incompressible).
- Channel B "telemetry": repeating-pattern 256 B payloads at 100 Hz (highly compressible).
- Total simulated duration: enough to produce ~50 MB of video and ~250 KB of telemetry — small enough to keep bench iterations fast, large enough to amortize fixed costs.
- Messages interleaved by `log_time`.

### Write benchmarks (`BenchmarkGroup` `write_mixed_compression`)

| Bench | Default chunk | Channel A (video) | Channel B (telemetry) |
|---|---|---|---|
| `write_all_compressed_zstd` | zstd | (default) | (default) |
| `write_mixed_video_uncompressed` | zstd | override: none | (default) |
| `write_all_uncompressed` | none | (default) | (default) |

All write into `Cursor<Vec<u8>>` (in-memory; I/O is not the variable under test). Throughput counter is `Throughput::Bytes(total_input_payload_bytes)` so reports show MB/s directly.

The middle row is the feature under test; the others are bookends. Output sizes are reported alongside wall time so the size/CPU tradeoff is visible.

### Read benchmarks (`BenchmarkGroup` `read_mixed_compression`)

For each of the three files: write once into a `Vec<u8>` outside the measured iteration, then bench the iteration that constructs `Reader::new(Cursor::new(&bytes))` and drains `reader.messages()`, summing payload lengths into a `black_box` accumulator. Throughput counter uses input payload bytes (same across all three), so wall time is the directly comparable axis.

| Read bench | File source |
|---|---|
| `read_all_compressed_zstd` | output of `write_all_compressed_zstd` |
| `read_mixed_video_uncompressed` | output of `write_mixed_video_uncompressed` |
| `read_all_uncompressed` | output of `write_all_uncompressed` |

### Companion example

`crates/mcapable-core/examples/mixed_compression.rs` mirrors the bench shape with a runnable demo: writes one file per mode under `/tmp/`, prints sizes and elapsed wall time. Lets you sanity-check the feature by hand without invoking criterion.

## Open questions

None. Design is approved through Section 5; remaining decisions are implementation-level (concrete file layout, exact unit test names) and belong in the implementation plan.
