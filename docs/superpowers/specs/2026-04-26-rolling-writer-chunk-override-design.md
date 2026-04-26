# Rolling Writer: Per-Channel Chunk Overrides

**Date:** 2026-04-26
**Status:** Approved (pending implementation plan)
**Crate:** `mcapable-core`
**Predecessor:** `2026-04-25-per-channel-uncompressed-chunks-design.md`

## Problem

When per-channel chunk overrides shipped (PR #3, branch `chachi/per-channel-uncompressed-chunks`), the `RollingWriter` was deliberately scoped out: it currently rejects `ChannelSpec.chunk_override` with `Err(InvalidRecord("rolling writer does not yet support ChannelSpec::chunk_override; use Writer::add_channel for per-channel uncompressed chunks"))`. The rationale at the time was to keep the initial PR focused. This spec adds the missing support.

The user-visible feature is identical to the base writer: callers tag specific channels with `ChannelSpec::chunk_override(opts)` (or the convenience `.uncompressed_chunks()`), and those channels' messages flow into a dedicated chunk stream. The implementation challenge is that `RollingWriter` builds a fresh `WriterImpl` on every split, so override registrations need to be re-applied per file.

## Non-goals

- No new public API on `RollingWriter` or `RollingChannelWriter` (the existing `add_channel(spec)` already accepts the new field; only its rejection path goes away).
- No `copy_channel_with_override` on `RollingWriter` — the rolling writer doesn't have a `copy_channel` method to begin with, so the parsed-`Channel` path is N/A.
- No new benchmark. Per-file behavior is identical to the base writer once registration is wired; the existing `mixed_compression` bench already covers it.
- No change to file-isolation guarantees: every output file remains a fully self-contained MCAP file.

## Design

### Split-time semantics (load-bearing)

Each output file is a fully self-contained MCAP file. When `perform_split()` fires, the existing `writer_impl.finish()` call drains every populated override stream (the loop added in Task 4 of the predecessor branch, ascending channel id). The fresh `WriterImpl` built for the next file then re-registers each tracked override (empty `ChunkState`). Override chunks therefore live entirely within a single file — they cannot span file boundaries, by construction.

### State on `RollingInner`

One new field, alongside the existing schema/channel registries that already persist across splits:

```rust
pub(crate) struct RollingInner<W: Write + Seek> {
    // ...existing fields...
    pub(crate) registered_schemas: Vec<Schema>,
    pub(crate) registered_channels: Vec<Channel>,
    /// Per-channel override `ChunkOptions`, keyed by stable channel id.
    /// Re-applied to each fresh `WriterImpl` on split.
    pub(crate) registered_overrides: HashMap<u16, ChunkOptions>,
    // ...
}
```

`HashMap` (not parallel `Vec<Option<...>>`) because tagged channels are sparse — the map only stores entries for channels that actually have an override. Per-message dispatch goes through a cached bool on `RollingChannelWriter`, so the only `HashMap` access is at split time.

### Channel registration

`RollingInner::add_channel_internal` stops rejecting `chunk_override` and instead remembers it for re-registration. Signature changes to return `(channel_id, has_override)` so `RollingWriter::add_channel` can plumb the bool into the returned `RollingChannelWriter`:

```rust
fn add_channel_internal(&mut self, spec: ChannelSpec) -> Result<(u16, bool)> {
    if self.finished {
        return Err(Error::InvalidRecord(
            "cannot add channel to a finished rolling writer".to_string(),
        ));
    }

    // Capture the override before the spec is consumed by add_channel_spec.
    let override_opts = spec.chunk_override.clone();

    let (channel_id, has_override) = self.writer_impl.add_channel_spec(spec)?;

    // Existing schema/channel snapshot logic (unchanged).
    if let Some(channel) = self.writer_impl.channels.get(&channel_id) {
        let channel = channel.clone();
        if channel.schema_id != 0
            && let Some(schema) = self.writer_impl.schemas.get(&channel.schema_id)
            && !self.registered_schemas.iter().any(|s| s.id == schema.id)
        {
            self.registered_schemas.push(schema.clone());
        }
        self.registered_channels.push(channel);
    }

    if let Some(opts) = override_opts {
        self.registered_overrides.insert(channel_id, opts);
    }

    Ok((channel_id, has_override))
}
```

The previous rejection branch (`if spec.chunk_override.is_some() { return Err(...) }`) is removed entirely.

### Split-time re-registration

Inside `perform_split`, immediately after the existing schema/channel re-emission loop, add:

```rust
// Re-register per-channel overrides into the fresh WriterImpl.
for (channel_id, opts) in &self.registered_overrides {
    self.writer_impl.register_channel_override(*channel_id, opts.clone());
}
```

`register_channel_override` is `pub(crate)` on `WriterImpl`, added in Task 6 of the predecessor branch. Iteration order doesn't matter — it's pure slot assignment into `override_streams[channel_id]`.

### Hot path

`RollingChannelWriter` gains the cached bool, mirroring `ChannelWriter`:

```rust
pub struct RollingChannelWriter<W: Write + Seek> {
    inner: Rc<RefCell<RollingInner<W>>>,
    channel_id: u16,
    next_sequence: u32,
    has_chunk_override: bool, // new
}
```

`RollingInner::write_message_internal` takes one new bool parameter and branches once on it. The base writer split into two methods (`_default` / `_override`) to keep the hot-path branch-free for the byte-equivalence promise; the rolling writer doesn't need that separation because it already pays an unavoidable `maybe_split()` check per write — one extra well-predicted branch is noise:

```rust
fn write_message_internal(
    &mut self,
    channel_id: u16,
    sequence: u32,
    log_time: u64,
    publish_time: u64,
    data: Bytes,
    has_chunk_override: bool, // new
) -> Result<()> {
    if self.finished {
        return Err(Error::InvalidRecord(
            "cannot write to a finished rolling writer".to_string(),
        ));
    }

    self.maybe_split(log_time)?;

    let msg = RawMessage::new(
        channel_id,
        sequence,
        log_time,
        publish_time,
        Payload::from_bytes(data),
    );
    if has_chunk_override {
        self.writer_impl.write_raw_message_override(&msg)?;
    } else {
        self.writer_impl.write_raw_message_default(&msg)?;
    }

    self.update_stats(log_time);
    Ok(())
}
```

`RollingChannelWriter::write_with_sequence` passes `self.has_chunk_override` through. `RollingWriter::add_channel` destructures `add_channel_internal`'s tuple and forwards the bool into the new `RollingChannelWriter` field.

### Builder initialization

Wherever `RollingInner` is constructed (rolling builder), add `registered_overrides: HashMap::new()` alongside the existing two registries.

### Error handling

No new error paths. The previously-existing `Err(InvalidRecord("rolling writer does not yet support..."))` path is deleted.

The `register_channel_override` invariant established in the base writer (silent overwrite on second registration for the same id) carries over: if a caller somehow ends up calling `add_channel_internal` twice for the same channel id (impossible via the public API today — channel ids are sequentially allocated by `WriterImpl::allocate_channel_id`), the second `registered_overrides.insert(...)` overwrites the first. Same property as the base writer.

## Testing

### Replace the existing rejection test

`rolling_writer_rejects_chunk_override` (added to `crates/mcapable-core/tests/rolling_writer.rs` in Task 4 of the predecessor branch) is now obsolete and is replaced.

### New positive tests (`tests/rolling_writer.rs`)

- `rolling_writer_routes_override_channel_to_uncompressed_chunks` — register one default channel + one `.uncompressed_chunks()` channel, write enough messages on the override channel to flush its chunks, finish; open the produced file and assert it contains both `"zstd"` and `""` chunks via `summary.chunk_indexes`.

- `rolling_writer_re_registers_override_after_split` — register one default channel + one `.uncompressed_chunks()` channel; write a few override messages, force a split, write more override messages, finish; collect the file bytes from each split and assert each independently contains override chunks (proves the registration survived the split).

- `rolling_writer_override_chunks_do_not_span_files` — write enough override messages to keep the override stream's `ChunkState` partially filled, force a split mid-stream, finish; assert (a) the first file contains a complete override chunk that landed at split-time-finish, and (b) the second file's first override chunk starts cleanly.

### Property test (optional, only if existing rolling proptests resist updating)

The existing `prop_rolling_*` proptests don't construct specs with `chunk_override`, so they continue passing unchanged. No new proptest is required for this spec; the base writer's `prop_mixed_compression_roundtrip_preserves_messages` already covers per-channel routing semantics that the rolling writer transitively depends on.

## PR-body update

The open PR (https://github.com/chachi/mcapable/pull/3) currently says: *"Rolling writer rejects `chunk_override` for now with a clear error pointing at `Writer::add_channel`."* That line will be updated when this spec is implemented to reflect that the rolling writer now supports overrides too.

## Open questions

None.
