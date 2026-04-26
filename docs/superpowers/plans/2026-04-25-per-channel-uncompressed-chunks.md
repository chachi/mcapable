# Per-Channel Uncompressed Chunks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `mcapable-core`'s writer route specific channels' messages into their own dedicated chunk stream with caller-chosen `ChunkOptions` (typically `compression: None`), so already-compressed payloads (H264, JPEG, …) skip wasted recompression while other channels keep using the writer's default chunk stream.

**Architecture:** New `chunk_override: Option<ChunkOptions>` field on `ChannelSpec`. `WriterImpl` gains `override_streams: Vec<Option<ChunkState>>` indexed by `channel_id`. `ChannelWriter` caches a `has_chunk_override: bool` set at construction so the per-message hot path branches on a bool — never a hash lookup. Two `WriterImpl` write entry points (`write_raw_message_default` / `write_raw_message_override`) keep the default path bit-identical to today.

**Tech Stack:** Rust 2024, `cargo nextest` for tests, `criterion` (new dev-dep) for benches, `rand` (new dev-dep) for seeded high-entropy bench payloads.

**Spec:** `docs/superpowers/specs/2026-04-25-per-channel-uncompressed-chunks-design.md`

---

## File Map

**Modify:**
- `crates/mcapable-core/src/writer/api.rs` — add `chunk_override` field + builder methods on `ChannelSpec`; add `has_chunk_override` field on `ChannelWriter`; add `copy_channel_with_override` on `Writer`; add `PartialEq, Eq` derives on `ChunkOptions`.
- `crates/mcapable-core/src/writer/internal.rs` — add `override_streams: Vec<Option<ChunkState>>`; add `add_channel_spec_with_override` plumbing; split `write_raw_message` into two entry points; extend `flush_chunk_if_needed` and `finish` to drain override streams.
- `crates/mcapable-core/src/writer/builder.rs` — initialize `override_streams: Vec::new()` in `build_impl`.
- `crates/mcapable-core/tests/writer_roundtrip.rs` — append mixed-compression round-trip tests.
- `crates/mcapable-core/tests/writer_properties.rs` — append `prop_mixed_compression_roundtrip_preserves_messages`.
- `crates/mcapable-core/Cargo.toml` — add `criterion` and `rand` dev-deps; add `[[bench]]` entry.

**Create:**
- `crates/mcapable-core/tests/writer_uncompressed_overrides.rs` — focused unit tests for the new feature.
- `crates/mcapable-core/benches/mixed_compression.rs` — write + read criterion benches.
- `crates/mcapable-core/examples/mixed_compression.rs` — runnable demo printing sizes and elapsed wall time.

---

## Task 1: Add `PartialEq, Eq` to `ChunkOptions` and `chunk_override` field to `ChannelSpec`

**Files:**
- Modify: `crates/mcapable-core/src/writer/api.rs:74-95` (ChunkOptions derive)
- Modify: `crates/mcapable-core/src/writer/api.rs:120-154` (ChannelSpec)

- [ ] **Step 1: Write failing test**

Append at the bottom of `crates/mcapable-core/src/writer/api.rs`, inside a new `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::Compression;

    #[test]
    fn channel_spec_carries_chunk_override() {
        let opts = ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 1024,
            include_crc: false,
        };
        let spec = ChannelSpec::new("/cam", "h264").chunk_override(opts.clone());
        assert_eq!(spec.chunk_override, Some(opts));
    }

    #[test]
    fn channel_spec_uncompressed_chunks_helper_sets_compression_none() {
        let spec = ChannelSpec::new("/cam", "h264").uncompressed_chunks();
        let chunk = spec.chunk_override.expect("override set");
        assert!(chunk.compression.is_none());
    }

    #[test]
    fn channel_spec_default_has_no_override() {
        let spec = ChannelSpec::new("/cam", "h264");
        assert!(spec.chunk_override.is_none());
    }

    #[test]
    fn chunk_options_eq() {
        let a = ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 4096,
            include_crc: true,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p mcapable-core writer::api::tests --no-fail-fast`
Expected: compile error — `chunk_override` is not a field of `ChannelSpec`, `chunk_override`/`uncompressed_chunks` methods do not exist, `ChunkOptions` does not implement `PartialEq`.

- [ ] **Step 3: Add derives to `ChunkOptions`**

In `crates/mcapable-core/src/writer/api.rs`, change the derive on `ChunkOptions` from:

```rust
#[derive(Debug, Clone)]
pub struct ChunkOptions {
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkOptions {
```

- [ ] **Step 4: Add field and builder methods to `ChannelSpec`**

In `crates/mcapable-core/src/writer/api.rs`, change the `ChannelSpec` struct definition from:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSpec {
    /// Topic name, e.g. `/tf`.
    pub topic: ByteStr,
    /// Message encoding, e.g. `cdr`.
    pub message_encoding: ByteStr,
    /// Optional schema definition. When `None`, the channel uses `schema_id = 0`.
    pub schema: Option<SchemaSpec>,
    /// Channel metadata map.
    pub metadata: HashMap<ByteStr, ByteStr>,
}
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSpec {
    /// Topic name, e.g. `/tf`.
    pub topic: ByteStr,
    /// Message encoding, e.g. `cdr`.
    pub message_encoding: ByteStr,
    /// Optional schema definition. When `None`, the channel uses `schema_id = 0`.
    pub schema: Option<SchemaSpec>,
    /// Channel metadata map.
    pub metadata: HashMap<ByteStr, ByteStr>,
    /// When `Some`, messages on this channel land in their own dedicated
    /// chunk stream configured by these options. When `None`, messages flow
    /// into the writer's default chunk stream (or top-level if the writer
    /// was built without `.chunked(...)`).
    pub chunk_override: Option<ChunkOptions>,
}
```

In the existing `impl ChannelSpec` block, update `new` to initialize the new field, and append the two builder methods. The block becomes:

```rust
impl ChannelSpec {
    /// Create a new channel spec without a schema.
    pub fn new(topic: impl Into<ByteStr>, message_encoding: impl Into<ByteStr>) -> Self {
        Self {
            topic: topic.into(),
            message_encoding: message_encoding.into(),
            schema: None,
            metadata: HashMap::new(),
            chunk_override: None,
        }
    }

    /// Set the channel schema.
    pub fn schema(mut self, schema: SchemaSpec) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Set the channel metadata map.
    pub fn metadata(mut self, metadata: HashMap<ByteStr, ByteStr>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Route this channel's messages into a dedicated chunk stream configured
    /// by `options`. Useful for writing already-compressed payloads as
    /// uncompressed chunks while the rest of the file uses compression.
    pub fn chunk_override(mut self, options: ChunkOptions) -> Self {
        self.chunk_override = Some(options);
        self
    }

    /// Convenience: route this channel into a dedicated chunk stream with
    /// `compression: None`. Other `ChunkOptions` fields take their defaults.
    pub fn uncompressed_chunks(mut self) -> Self {
        self.chunk_override = Some(ChunkOptions {
            compression: None,
            ..ChunkOptions::default()
        });
        self
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo nextest run -p mcapable-core writer::api::tests`
Expected: 4 passed.

- [ ] **Step 6: Verify the rest of the crate still builds and tests still pass**

Run: `just check`
Expected: clean (no errors, no clippy warnings).

Run: `cargo nextest run -p mcapable-core`
Expected: all existing tests still pass.

- [ ] **Step 7: Commit**

```bash
git add crates/mcapable-core/src/writer/api.rs
git commit -m "$(cat <<'EOF'
feat(writer): add ChannelSpec::chunk_override field

Adds an optional per-channel ChunkOptions override on ChannelSpec
plus builder methods chunk_override(opts) and uncompressed_chunks().
The field is currently inert — internal routing wiring lands in the
next commit.

Also derives PartialEq + Eq on ChunkOptions so ChannelSpec can keep
its existing PartialEq + Eq derives now that it embeds ChunkOptions.
EOF
)"
```

---

## Task 2: Plumb `has_chunk_override` through `ChannelWriter`

**Files:**
- Modify: `crates/mcapable-core/src/writer/api.rs:156-206` (ChannelWriter struct + Writer::add_channel + Writer::copy_channel)

This task is purely additive — the new field exists, defaults to `false`, and is unused on the hot path. Routing arrives in Task 4.

- [ ] **Step 1: Add field to `ChannelWriter`**

In `crates/mcapable-core/src/writer/api.rs`, change the struct definition from:

```rust
#[derive(Clone)]
pub struct ChannelWriter<W: Write + Seek> {
    pub(crate) inner: Rc<RefCell<WriterImpl<W>>>,
    pub(crate) channel_id: u16,
    pub(crate) next_sequence: u32,
}
```

to:

```rust
#[derive(Clone)]
pub struct ChannelWriter<W: Write + Seek> {
    pub(crate) inner: Rc<RefCell<WriterImpl<W>>>,
    pub(crate) channel_id: u16,
    pub(crate) next_sequence: u32,
    /// True iff this channel has a `chunk_override` registered in
    /// `WriterImpl::override_streams`. Cached so the per-message
    /// write path can dispatch with one bool branch and never hash.
    pub(crate) has_chunk_override: bool,
}
```

- [ ] **Step 2: Default the field at all construction sites in `Writer`**

In `Writer::add_channel` (around lines 264-271) update the returned struct from:

```rust
Ok(ChannelWriter {
    inner: Rc::clone(&self.inner),
    channel_id,
    next_sequence: 0,
})
```

to:

```rust
Ok(ChannelWriter {
    inner: Rc::clone(&self.inner),
    channel_id,
    next_sequence: 0,
    has_chunk_override: false,
})
```

In `Writer::copy_channel` (around lines 292-299) make the matching change:

```rust
Ok(ChannelWriter {
    inner: Rc::clone(&self.inner),
    channel_id: channel.id,
    next_sequence: 0,
    has_chunk_override: false,
})
```

- [ ] **Step 3: Verify**

Run: `just check`
Expected: clean.

Run: `cargo nextest run -p mcapable-core`
Expected: all existing tests still pass — no behavior change.

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/src/writer/api.rs
git commit -m "$(cat <<'EOF'
feat(writer): cache has_chunk_override bit on ChannelWriter

Adds a private bool field defaulted to false at all construction
sites. Routing wiring lands in the next commit; this commit is
purely additive scaffolding.
EOF
)"
```

---

## Task 3: Add `override_streams` storage to `WriterImpl`

**Files:**
- Modify: `crates/mcapable-core/src/writer/internal.rs:21-39` (WriterImpl struct)
- Modify: `crates/mcapable-core/src/writer/builder.rs:96-115` (build_impl)

Pure additive scaffolding. The vec exists, starts empty, and is never read or written until Task 4.

- [ ] **Step 1: Add field to `WriterImpl`**

In `crates/mcapable-core/src/writer/internal.rs`, locate the struct definition (line 21). Insert the new field directly below `chunk_state`:

```rust
pub(crate) struct WriterImpl<W: Write + Seek> {
    pub(crate) sink: PositionTrackingSink<W>,
    pub(crate) header: Header,
    pub(crate) wrote_header: bool,
    pub(crate) finished: bool,
    pub(crate) next_schema_id: u16,
    pub(crate) next_channel_id: u16,
    #[allow(dead_code)] // Will be used for strict ordering and validity checks.
    pub(crate) validation: Validation,
    pub(crate) always_write_summary: bool,
    pub(crate) chunk_state: Option<ChunkState>,
    /// Per-channel override chunk streams, indexed by `channel_id`.
    /// `Some(state)` means this channel has a `chunk_override` registered;
    /// its messages flow into `state` instead of `chunk_state`. The vec
    /// grows as channels are registered (mirrors `channel_stats`).
    pub(crate) override_streams: Vec<Option<ChunkState>>,
    pub(crate) schemas: HashMap<u16, Schema>,
    pub(crate) channels: HashMap<u16, Channel>,
    pub(crate) channel_stats: Vec<ChannelStats>,
    pub(crate) chunk_indexes: Vec<ChunkIndexInfo>,
    pub(crate) attachment_indexes: Vec<AttachmentIndexInfo>,
    pub(crate) metadata_indexes: Vec<MetadataIndexInfo>,
    pub(crate) schema_ids_by_key: HashMap<SchemaKey, u16>,
}
```

- [ ] **Step 2: Initialize in `WriterBuilder::build_impl`**

In `crates/mcapable-core/src/writer/builder.rs`, update `build_impl`:

```rust
pub(crate) fn build_impl<W: Write + Seek>(self, sink: W) -> Result<WriterImpl<W>> {
    Ok(WriterImpl {
        sink: PositionTrackingSink::new(sink)?,
        header: self.header,
        wrote_header: false,
        finished: false,
        next_schema_id: 1,
        next_channel_id: 1,
        validation: self.validation,
        always_write_summary: self.always_write_summary,
        chunk_state: self.chunk_options.map(ChunkState::new),
        override_streams: Vec::new(),
        schemas: HashMap::new(),
        channels: HashMap::new(),
        channel_stats: Vec::new(),
        chunk_indexes: Vec::new(),
        attachment_indexes: Vec::new(),
        metadata_indexes: Vec::new(),
        schema_ids_by_key: HashMap::new(),
    })
}
```

- [ ] **Step 3: Verify**

Run: `just check`
Expected: clean.

Run: `cargo nextest run -p mcapable-core`
Expected: all existing tests still pass.

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/src/writer/internal.rs crates/mcapable-core/src/writer/builder.rs
git commit -m "$(cat <<'EOF'
feat(writer): add empty override_streams vec to WriterImpl

Per-channel override chunk-stream storage, indexed by channel_id,
mirroring the existing channel_stats vec layout. Currently always
empty; routing and population land in the next commits.
EOF
)"
```

---

## Task 4: First end-to-end test — override channel produces an uncompressed chunk

This is the first task that actually wires routing. Failing test first; then implement registration, dispatch, and finish-flush in the minimum changes that make it pass.

**Files:**
- Create: `crates/mcapable-core/tests/writer_uncompressed_overrides.rs`
- Modify: `crates/mcapable-core/src/writer/api.rs` (ChannelWriter::write_with_sequence dispatch + Writer::add_channel)
- Modify: `crates/mcapable-core/src/writer/internal.rs` (add_channel_spec, write_raw_message split, finish)

- [ ] **Step 1: Write the failing test**

Create `crates/mcapable-core/tests/writer_uncompressed_overrides.rs`:

```rust
//! Tests for per-channel chunk_override (uncompressed-while-rest-is-compressed).

use bytes::Bytes;
use mcapable_core::Compression;
use mcapable_core::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};
use std::io::Cursor;

/// Build a writer with a default zstd chunk stream, write a few messages
/// to a normal channel and a few to a channel tagged `uncompressed_chunks()`,
/// then assert the resulting file contains chunks of both compression types.
#[test]
fn override_channel_produces_uncompressed_chunk_alongside_default_zstd() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 64, // tiny, so each message flushes
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));

    let mut default_ch = writer
        .add_channel(ChannelSpec::new("/telemetry", "raw").schema(schema.clone()))
        .unwrap();
    let mut override_ch = writer
        .add_channel(
            ChannelSpec::new("/video", "h264")
                .schema(schema.clone())
                .uncompressed_chunks(),
        )
        .unwrap();

    // Write enough bytes on each channel to force a flush.
    for i in 0..4u64 {
        default_ch
            .write(1000 + i, 1000 + i, vec![b'a'; 80])
            .unwrap();
        override_ch
            .write(1000 + i, 1000 + i, vec![b'b'; 80])
            .unwrap();
    }
    drop(default_ch);
    drop(override_ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();

    // Open the file and inspect chunk indexes.
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary present");

    let mut zstd_count = 0;
    let mut none_count = 0;
    for ci in &summary.chunk_indexes {
        if ci.compression.as_ref() == "zstd" {
            zstd_count += 1;
        } else if ci.compression.as_ref().is_empty() {
            none_count += 1;
        } else {
            panic!("unexpected compression: {:?}", ci.compression);
        }
    }
    assert!(zstd_count > 0, "expected at least one zstd chunk");
    assert!(none_count > 0, "expected at least one uncompressed chunk");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p mcapable-core --test writer_uncompressed_overrides`
Expected: FAIL — `none_count` is 0 because override is wired in spec only and the writer still routes everything through the default stream.

- [ ] **Step 3: Wire registration in `WriterImpl::add_channel_spec`**

In `crates/mcapable-core/src/writer/internal.rs`, replace `add_channel_spec` (around lines 243-259) with a version that returns both the channel id and whether an override was registered. The new signature:

```rust
pub(crate) fn add_channel_spec(&mut self, spec: ChannelSpec) -> Result<(u16, bool)> {
    let schema_id = match spec.schema {
        Some(schema) => self.schema_id_for_spec(schema)?,
        None => 0,
    };

    let id = self.allocate_channel_id();
    let channel = Channel {
        id,
        topic: spec.topic,
        message_encoding: spec.message_encoding,
        schema_id,
        metadata: spec.metadata,
    };
    self.write_channel_internal(&channel)?;

    let has_override = if let Some(opts) = spec.chunk_override {
        let idx = id as usize;
        if self.override_streams.len() <= idx {
            self.override_streams
                .resize_with(idx.saturating_add(1), || None);
        }
        self.override_streams[idx] = Some(ChunkState::new(opts));
        true
    } else {
        false
    };

    Ok((id, has_override))
}
```

- [ ] **Step 4: Update `Writer::add_channel` to forward the new bool**

In `crates/mcapable-core/src/writer/api.rs`, replace `Writer::add_channel` (around lines 264-271) with:

```rust
pub fn add_channel(&mut self, spec: ChannelSpec) -> Result<ChannelWriter<W>> {
    let (channel_id, has_chunk_override) =
        self.inner.borrow_mut().add_channel_spec(spec)?;
    Ok(ChannelWriter {
        inner: Rc::clone(&self.inner),
        channel_id,
        next_sequence: 0,
        has_chunk_override,
    })
}
```

- [ ] **Step 5: Split `WriterImpl::write_raw_message` into default and override entry points**

In `crates/mcapable-core/src/writer/internal.rs`, replace the existing `write_raw_message` (around lines 743-775) with three functions: a shared prelude, the default path, and the override path. The original `write_raw_message` is renamed `write_raw_message_default`. Code:

```rust
pub(crate) fn write_raw_message_default(&mut self, message: &RawMessage) -> Result<()> {
    if self.finished {
        return Err(Error::InvalidRecord(
            "cannot write message after finish".to_string(),
        ));
    }
    self.write_header()?;
    self.update_channel_stats(message.channel_id, message.log_time);

    if let Some(state) = &mut self.chunk_state {
        state.push_message(
            message.channel_id,
            message.sequence,
            message.log_time,
            message.publish_time,
            message.data_bytes(),
        );
        self.flush_chunk_if_needed(false)?;
        return Ok(());
    }

    let payload_len = (crate::format::MESSAGE_HEADER_SIZE + message.data_len()) as u64;
    let mut prefix = [0u8; MESSAGE_RECORD_PREFIX_LEN];
    prefix[0] = Opcode::Message.as_u8();
    prefix[1..9].copy_from_slice(&payload_len.to_le_bytes());
    prefix[9..11].copy_from_slice(&message.channel_id.to_le_bytes());
    prefix[11..15].copy_from_slice(&message.sequence.to_le_bytes());
    prefix[15..23].copy_from_slice(&message.log_time.to_le_bytes());
    prefix[23..31].copy_from_slice(&message.publish_time.to_le_bytes());

    write_all_vectored2(&mut self.sink, &prefix, message.data())?;
    Ok(())
}

pub(crate) fn write_raw_message_override(&mut self, message: &RawMessage) -> Result<()> {
    if self.finished {
        return Err(Error::InvalidRecord(
            "cannot write message after finish".to_string(),
        ));
    }
    self.write_header()?;
    self.update_channel_stats(message.channel_id, message.log_time);

    let idx = message.channel_id as usize;
    debug_assert!(
        idx < self.override_streams.len() && self.override_streams[idx].is_some(),
        "write_raw_message_override called for channel {} without registered override stream",
        message.channel_id,
    );

    {
        let state = self.override_streams[idx]
            .as_mut()
            .expect("override stream must be registered");
        state.push_message(
            message.channel_id,
            message.sequence,
            message.log_time,
            message.publish_time,
            message.data_bytes(),
        );
    }

    self.flush_override_if_needed(idx, false)?;
    Ok(())
}
```

Add the `flush_override_if_needed` helper alongside the existing `flush_chunk_if_needed`. Insert it directly below `flush_chunk_if_needed` (around line 110):

```rust
fn flush_override_if_needed(&mut self, channel_idx: usize, force: bool) -> Result<()> {
    let Some(state) = self
        .override_streams
        .get_mut(channel_idx)
        .and_then(|slot| slot.as_mut())
    else {
        return Ok(());
    };
    if !state.should_flush(force) {
        return Ok(());
    }

    let flush = state.take_for_flush();
    let sink_position = self.sink.position();
    let prepared = prepare_chunk_for_write(sink_position, &flush)?;
    self.chunk_indexes.push(prepared.index);

    write_all_vectored2(
        &mut self.sink,
        &prepared.record_header,
        &prepared.record_prefix,
    )?;
    if prepared.write_uncompressed_records {
        write_all_vectored_chunk_records(
            &mut self.sink,
            &flush.message_prefixes,
            &flush.payloads,
            MESSAGE_RECORD_PREFIX_LEN,
        )?;
    } else if let Some(compressed) = prepared.compressed_body {
        self.sink.write_all(&compressed)?;
    }

    if let Some(slot) = self.override_streams.get_mut(channel_idx) {
        if let Some(state) = slot.as_mut() {
            state.recycle_buffers(flush.message_prefixes, flush.payloads);
        }
    }
    Ok(())
}
```

- [ ] **Step 6: Update `ChannelWriter::write_with_sequence` to dispatch on the cached bool**

In `crates/mcapable-core/src/writer/api.rs`, replace the body of `write_with_sequence` (around lines 190-205) with:

```rust
pub fn write_with_sequence<D: IntoPayloadBytes>(
    &mut self,
    log_time: u64,
    publish_time: u64,
    data: D,
    sequence: u32,
) -> Result<()> {
    let bytes = data.into_payload_bytes();
    let msg = RawMessage::new(
        self.channel_id,
        sequence,
        log_time,
        publish_time,
        Payload::from_bytes(bytes),
    );
    let mut inner = self.inner.borrow_mut();
    if self.has_chunk_override {
        inner.write_raw_message_override(&msg)
    } else {
        inner.write_raw_message_default(&msg)
    }
}
```

- [ ] **Step 7: Update other internal callers of the renamed/changed methods**

Two call sites in the rolling writer also touch the renamed `write_raw_message` and the now-tuple-returning `add_channel_spec`. Both need updates so the crate keeps compiling.

Run: `grep -rn "write_raw_message\|add_channel_spec" crates/mcapable-core/src/`

Expected hits include `crates/mcapable-core/src/writer/rolling/writer.rs:176` (`write_raw_message`) and `crates/mcapable-core/src/writer/rolling/writer.rs:195` (`add_channel_spec`).

In `crates/mcapable-core/src/writer/rolling/writer.rs`:

- Around line 176, change:

  ```rust
  self.writer_impl.write_raw_message(&RawMessage::new(
      channel_id,
      sequence,
      log_time,
      publish_time,
      Payload::from_bytes(data),
  ))?;
  ```

  to:

  ```rust
  self.writer_impl.write_raw_message_default(&RawMessage::new(
      channel_id,
      sequence,
      log_time,
      publish_time,
      Payload::from_bytes(data),
  ))?;
  ```

  (The rolling writer does not yet support per-channel overrides; messages always take the default chunk path.)

- Around line 195, change:

  ```rust
  let channel_id = self.writer_impl.add_channel_spec(spec)?;
  ```

  to:

  ```rust
  if spec.chunk_override.is_some() {
      return Err(Error::InvalidRecord(
          "rolling writer does not yet support ChannelSpec::chunk_override; \
           use Writer::add_channel for per-channel uncompressed chunks"
              .to_string(),
      ));
  }
  let (channel_id, _has_override) = self.writer_impl.add_channel_spec(spec)?;
  ```

  Confirm `Error` is in scope at the top of `rolling/writer.rs`. If `use crate::error::Error;` is missing, add it (the file already imports `Result`).

Run: `grep -rn "write_raw_message\|add_channel_spec" crates/mcapable-core/src/` again.
Expected: only definitions and the now-updated call sites — no stale `write_raw_message(` invocations.

- [ ] **Step 8: Force-flush all override streams on `finish`**

In `crates/mcapable-core/src/writer/internal.rs`, locate `finish` (the function that calls `flush_chunk_if_needed(true)?` near line 989). Just before that line, add a loop that drains every populated override stream:

```rust
for idx in 0..self.override_streams.len() {
    self.flush_override_if_needed(idx, true)?;
}
self.flush_chunk_if_needed(true)?;
```

(The order — overrides first, then default — is arbitrary for correctness; chunks are indexed. Putting overrides before the default keeps the on-disk position of any final default chunk last, matching test expectations in later tasks.)

- [ ] **Step 9: Run the test from Step 1**

Run: `cargo nextest run -p mcapable-core --test writer_uncompressed_overrides`
Expected: PASS — file now contains both `zstd` and `""` chunks.

- [ ] **Step 10: Verify the rest of the suite still passes**

Run: `just check`
Expected: clean.

Run: `cargo nextest run -p mcapable-core`
Expected: all tests pass (existing single-stream tests are unaffected; overridden writes weren't possible before, so no behavior regression is possible).

- [ ] **Step 11: Commit**

```bash
git add crates/mcapable-core/src/writer/api.rs crates/mcapable-core/src/writer/internal.rs crates/mcapable-core/tests/writer_uncompressed_overrides.rs
git commit -m "$(cat <<'EOF'
feat(writer): route override channels to dedicated chunk streams

Channels created with ChannelSpec::chunk_override(opts) now flow
into a dedicated ChunkState in WriterImpl::override_streams,
indexed by channel_id. ChannelWriter caches a has_chunk_override
bool at construction so the per-message hot path branches on a
bool — never a HashMap lookup.

WriterImpl::write_raw_message is split into _default and _override
entry points; the default path is unchanged from before. finish()
drains any populated override streams before the default.
EOF
)"
```

---

## Task 5: Add defensive override-stream flushes at top-level record boundaries

**Background:** the existing default chunk code calls `flush_chunk_if_needed(false)` before writing any top-level Schema/Channel/Attachment/Metadata/raw record. Given that `write_raw_message_default` already flushes eagerly on push, these calls are mostly defensive and rarely fire — but the spec calls for parity treatment of override streams. This task adds equivalent defensive calls for override streams. **Observable behavior is unchanged** (Task 8's round-trip tests exercise the path); this task is about consistency with the existing pattern.

**Files:**
- Modify: `crates/mcapable-core/src/writer/internal.rs`

- [ ] **Step 1: Add the all-overrides helper**

In `crates/mcapable-core/src/writer/internal.rs`, add a small private helper directly below `flush_override_if_needed`:

```rust
/// Give every populated override stream a chance to flush at its threshold.
/// Cheap when nothing is at threshold (one `should_flush` check per slot).
fn flush_overrides_if_needed_all(&mut self, force: bool) -> Result<()> {
    for idx in 0..self.override_streams.len() {
        self.flush_override_if_needed(idx, force)?;
    }
    Ok(())
}
```

- [ ] **Step 2: Add the helper call next to every existing `flush_chunk_if_needed(false)?`**

Run: `grep -n "flush_chunk_if_needed(false)" crates/mcapable-core/src/writer/internal.rs`

Expected hits at lines roughly: 275, 338, 409, 552, 694, 724, 791, 813 (the grep output prints exact lines).

For *each* hit, immediately *before* the existing `self.flush_chunk_if_needed(false)?;` line, insert:

```rust
self.flush_overrides_if_needed_all(false)?;
```

Do **not** modify the `finish` call site (line ~989) — Step 8 of Task 4 already added the explicit drain there with `force=true`.

- [ ] **Step 3: Verify**

Run: `just check`
Expected: clean.

Run: `cargo nextest run -p mcapable-core`
Expected: all tests pass — Task 8's mixed-compression round-trip tests cover the scenarios that exercise these helpers (writing a top-level record between override messages).

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/src/writer/internal.rs
git commit -m "$(cat <<'EOF'
feat(writer): defensive override-stream flushes at top-level boundaries

Mirrors the existing default-stream pattern: before writing a
top-level Schema/Channel/Attachment/Metadata/raw record, give every
populated override stream a chance to flush if it has reached its
threshold. With the eager per-write flush in
write_raw_message_override these calls rarely fire, but the parity
matches the spec and keeps the call sites predictable for future
maintenance.
EOF
)"
```

---

## Task 6: `Writer::copy_channel_with_override`

**Files:**
- Modify: `crates/mcapable-core/src/writer/api.rs` — add new method on `Writer`.
- Modify: `crates/mcapable-core/src/writer/internal.rs` — extend `write_channel_internal` (or add a sibling) to populate `override_streams` when called via the new path.

- [ ] **Step 1: Write failing test** — append to `crates/mcapable-core/tests/writer_uncompressed_overrides.rs`:

```rust
/// `copy_channel_with_override` lets pipelines (merge/filter) impose an
/// override on a channel they're copying from another file.
#[test]
fn copy_channel_with_override_routes_to_override_stream() {
    use mcapable_core::Channel;
    use mcapable_core::zero_copy::ByteStr;

    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 64,
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let channel = Channel {
        id: 7,
        topic: ByteStr::from("/copied"),
        message_encoding: ByteStr::from("h264"),
        schema_id: 0,
        metadata: Default::default(),
    };

    let opts = ChunkOptions {
        compression: None,
        max_uncompressed_bytes: 32,
        include_crc: true,
    };

    let mut ch = writer.copy_channel_with_override(&channel, opts).unwrap();
    ch.write(1, 1, vec![b'z'; 64]).unwrap();
    ch.write(2, 2, vec![b'z'; 64]).unwrap();
    drop(ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    let none_chunks: usize = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    assert!(none_chunks > 0, "expected at least one uncompressed chunk for copied channel");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo nextest run -p mcapable-core --test writer_uncompressed_overrides copy_channel_with_override_routes_to_override_stream`
Expected: compile error — `copy_channel_with_override` does not exist.

- [ ] **Step 3: Add `WriterImpl::register_channel_override`**

In `crates/mcapable-core/src/writer/internal.rs`, add a small helper (place it directly below `add_channel_spec`):

```rust
/// Register an override `ChunkState` for an already-known channel id.
/// Used by `Writer::copy_channel_with_override`.
pub(crate) fn register_channel_override(&mut self, channel_id: u16, options: ChunkOptions) {
    let idx = channel_id as usize;
    if self.override_streams.len() <= idx {
        self.override_streams
            .resize_with(idx.saturating_add(1), || None);
    }
    self.override_streams[idx] = Some(ChunkState::new(options));
}
```

Note: this requires importing `ChunkOptions` at the top of `internal.rs`. Check the imports near line 10:

```rust
use super::api::{ChannelSpec, SchemaSpec, Validation};
```

Update to:

```rust
use super::api::{ChannelSpec, ChunkOptions, SchemaSpec, Validation};
```

- [ ] **Step 4: Add `Writer::copy_channel_with_override`**

In `crates/mcapable-core/src/writer/api.rs`, just below the existing `copy_channel` (around line 299), add:

```rust
/// Copy a channel from another MCAP file *and* route its messages into a
/// dedicated chunk stream configured by `options`. Equivalent to
/// `add_channel(ChannelSpec::...chunk_override(options))` for the spec
/// path; this variant is for pipelines that only have a parsed `Channel`.
///
/// Preserves the channel id from `channel`.
pub fn copy_channel_with_override(
    &mut self,
    channel: &Channel,
    options: ChunkOptions,
) -> Result<ChannelWriter<W>> {
    let mut inner = self.inner.borrow_mut();
    inner.write_channel_internal(channel)?;
    inner.register_channel_override(channel.id, options);
    drop(inner);
    Ok(ChannelWriter {
        inner: Rc::clone(&self.inner),
        channel_id: channel.id,
        next_sequence: 0,
        has_chunk_override: true,
    })
}
```

- [ ] **Step 5: Run the test**

Run: `cargo nextest run -p mcapable-core --test writer_uncompressed_overrides copy_channel_with_override_routes_to_override_stream`
Expected: PASS.

- [ ] **Step 6: Run full suite + check**

Run: `just check`
Expected: clean.

Run: `cargo nextest run -p mcapable-core`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/mcapable-core/src/writer/api.rs crates/mcapable-core/src/writer/internal.rs crates/mcapable-core/tests/writer_uncompressed_overrides.rs
git commit -m "$(cat <<'EOF'
feat(writer): add Writer::copy_channel_with_override

Lets pipelines (e.g. merge/filter, recover/reindex) impose a
chunk_override on a channel they're copying from another file
without going through ChannelSpec.
EOF
)"
```

---

## Task 7: Focused unit tests

**Files:**
- Modify: `crates/mcapable-core/tests/writer_uncompressed_overrides.rs` — add three tests called out in the spec.

- [ ] **Step 1: Append all three tests**

Append at the end of `crates/mcapable-core/tests/writer_uncompressed_overrides.rs`:

```rust
/// Override chunks must use the empty compression string (per MCAP spec for
/// "no compression").
#[test]
fn override_stream_uses_empty_compression_string() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 64,
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut ch = writer
        .add_channel(
            ChannelSpec::new("/cam", "h264")
                .schema(schema)
                .uncompressed_chunks(),
        )
        .unwrap();
    for i in 0..4u64 {
        ch.write(i, i, vec![b'a'; 80]).unwrap();
    }
    drop(ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    for ci in &summary.chunk_indexes {
        assert!(
            ci.compression.as_ref().is_empty(),
            "expected empty compression for override stream, got {:?}",
            ci.compression,
        );
    }
}

/// Override stream's flush threshold is independent of the default's:
/// a small override threshold flushes frequently while the default
/// accumulates; a small default threshold flushes frequently while
/// the override accumulates.
#[test]
fn override_stream_flushes_on_its_own_threshold() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 1 << 20, // huge — won't flush during the test
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut default_ch = writer
        .add_channel(ChannelSpec::new("/telemetry", "raw").schema(schema.clone()))
        .unwrap();
    let mut override_ch = writer
        .add_channel(
            ChannelSpec::new("/cam", "h264")
                .schema(schema)
                .chunk_override(ChunkOptions {
                    compression: None,
                    max_uncompressed_bytes: 32, // tiny — flushes per message
                    include_crc: true,
                }),
        )
        .unwrap();

    for i in 0..6u64 {
        default_ch.write(i, i, vec![b'a'; 16]).unwrap();
        override_ch.write(i, i, vec![b'b'; 64]).unwrap();
    }
    drop(default_ch);
    drop(override_ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    let override_chunks = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    // Override flushes per message (6 writes); finish-flush leaves at most one
    // extra. Default accumulates and flushes once at finish.
    assert!(
        override_chunks >= 6,
        "expected at least 6 override chunks (one per message), got {override_chunks}",
    );
}

/// Multiple tagged channels with partial fills all flush on `finish()`.
#[test]
fn finish_flushes_all_override_streams() {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 1 << 20,
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut ch_a = writer
        .add_channel(
            ChannelSpec::new("/cam_a", "h264")
                .schema(schema.clone())
                .uncompressed_chunks(),
        )
        .unwrap();
    let mut ch_b = writer
        .add_channel(
            ChannelSpec::new("/cam_b", "h264")
                .schema(schema)
                .uncompressed_chunks(),
        )
        .unwrap();

    // Partial fills — well under any reasonable threshold.
    ch_a.write(1, 1, vec![b'a'; 16]).unwrap();
    ch_b.write(2, 2, vec![b'b'; 16]).unwrap();
    drop(ch_a);
    drop(ch_b);

    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    let override_chunks = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    assert_eq!(
        override_chunks, 2,
        "expected one chunk per non-empty override stream (2 streams, 2 chunks)",
    );
}
```

- [ ] **Step 2: Run all three**

Run: `cargo nextest run -p mcapable-core --test writer_uncompressed_overrides`
Expected: all 5 tests in this file pass (the original 2 from Tasks 4-5 plus the 3 new ones).

- [ ] **Step 3: Commit**

```bash
git add crates/mcapable-core/tests/writer_uncompressed_overrides.rs
git commit -m "$(cat <<'EOF'
test(writer): focused unit tests for chunk_override behaviour

- override stream uses empty compression string
- override stream flushes on its own threshold (independent of default)
- finish() flushes every populated override stream
EOF
)"
```

---

## Task 8: Reader-side mixed-compression round-trip tests

**Files:**
- Modify: `crates/mcapable-core/tests/writer_roundtrip.rs` — append three round-trip tests covering the reader paths.

- [ ] **Step 1: Append the round-trip tests**

Append at the end of `crates/mcapable-core/tests/writer_roundtrip.rs`:

```rust
/// Helper: write a file with one default-stream channel and one override
/// channel, returning (bytes, expected_default_messages, expected_override_messages).
fn write_mixed_compression_file(
    default_compression: Option<Compression>,
    override_compression: Option<Compression>,
) -> (Vec<u8>, Vec<Bytes>, Vec<Bytes>) {
    let out = Cursor::new(Vec::new());
    let mut writer = WriterBuilder::new()
        .chunked(ChunkOptions {
            compression: default_compression,
            max_uncompressed_bytes: 64,
            include_crc: true,
        })
        .build(out)
        .unwrap();

    let schema_spec =
        mcapable_core::writer::SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut default_ch = writer
        .add_channel(
            mcapable_core::writer::ChannelSpec::new("/telemetry", "raw").schema(schema_spec.clone()),
        )
        .unwrap();
    let mut override_ch = writer
        .add_channel(
            mcapable_core::writer::ChannelSpec::new("/video", "h264")
                .schema(schema_spec)
                .chunk_override(ChunkOptions {
                    compression: override_compression,
                    max_uncompressed_bytes: 64,
                    include_crc: true,
                }),
        )
        .unwrap();

    let mut default_msgs = Vec::new();
    let mut override_msgs = Vec::new();
    for i in 0..6u64 {
        let dm = Bytes::from(vec![b'a'; 80]);
        let om = Bytes::from(vec![(b'A' + i as u8); 80]);
        default_ch.write(1000 + i, 1000 + i, dm.clone()).unwrap();
        override_ch.write(1000 + i, 1000 + i, om.clone()).unwrap();
        default_msgs.push(dm);
        override_msgs.push(om);
    }
    drop(default_ch);
    drop(override_ch);
    writer.finish().unwrap();

    let bytes = writer.into_inner().into_inner();
    (bytes, default_msgs, override_msgs)
}

#[test]
fn writer_round_trip_mixed_compression_zstd_default_uncompressed_override() {
    let (bytes, default_msgs, override_msgs) =
        write_mixed_compression_file(Some(Compression::Zstd), None);

    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let channels = reader.channels().clone();
    let topic_for = |id: u16| channels.get(&id).map(|c| c.topic.as_ref().to_string());

    let mut got_telemetry: Vec<Bytes> = Vec::new();
    let mut got_video: Vec<Bytes> = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        match topic_for(raw.channel_id).as_deref() {
            Some("/telemetry") => got_telemetry.push(raw.data_bytes()),
            Some("/video") => got_video.push(raw.data_bytes()),
            other => panic!("unexpected topic: {other:?}"),
        }
    }
    assert_eq!(got_telemetry, default_msgs);
    assert_eq!(got_video, override_msgs);
}

#[test]
fn writer_round_trip_mixed_compression_lz4_default_uncompressed_override() {
    let (bytes, default_msgs, override_msgs) =
        write_mixed_compression_file(Some(Compression::Lz4), None);

    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let channels = reader.channels().clone();
    let topic_for = |id: u16| channels.get(&id).map(|c| c.topic.as_ref().to_string());

    let mut got_telemetry = Vec::new();
    let mut got_video = Vec::new();
    for raw in reader.raw_messages().unwrap() {
        let raw = raw.unwrap();
        match topic_for(raw.channel_id).as_deref() {
            Some("/telemetry") => got_telemetry.push(raw.data_bytes()),
            Some("/video") => got_video.push(raw.data_bytes()),
            other => panic!("unexpected topic: {other:?}"),
        }
    }
    assert_eq!(got_telemetry, default_msgs);
    assert_eq!(got_video, override_msgs);
}

#[test]
fn reader_chunk_stream_sees_both_compressions_in_mixed_file() {
    let (bytes, _, _) = write_mixed_compression_file(Some(Compression::Zstd), None);

    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let mut zstd_chunks = 0usize;
    let mut none_chunks = 0usize;
    for chunk_res in reader.chunks().unwrap() {
        let chunk = chunk_res.unwrap();
        match chunk.compression.as_ref() {
            "zstd" => zstd_chunks += 1,
            "" => none_chunks += 1,
            other => panic!("unexpected compression {other:?}"),
        }
    }
    assert!(zstd_chunks > 0, "expected at least one zstd chunk");
    assert!(none_chunks > 0, "expected at least one uncompressed chunk");
}

#[test]
fn summary_chunk_indexes_round_trip_for_mixed_file() {
    let (bytes, _, _) = write_mixed_compression_file(Some(Compression::Zstd), None);

    let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    // Every chunk index entry's offset+length must lie within the file.
    for ci in &summary.chunk_indexes {
        let end = ci.chunk_start_offset + ci.chunk_length;
        assert!(
            end as usize <= bytes.len(),
            "chunk index entry exceeds file: offset={}, length={}, file_len={}",
            ci.chunk_start_offset,
            ci.chunk_length,
            bytes.len(),
        );
    }

    // At least one zstd and one empty-compression entry must appear.
    let mut compressions: Vec<String> = summary
        .chunk_indexes
        .iter()
        .map(|ci| ci.compression.as_ref().to_string())
        .collect();
    compressions.sort();
    compressions.dedup();
    assert!(compressions.contains(&"zstd".to_string()));
    assert!(compressions.contains(&"".to_string()));
}
```

- [ ] **Step 2: Run the new tests**

Run: `cargo nextest run -p mcapable-core --test writer_roundtrip mixed`
Expected: all 4 new mixed-compression tests pass.

- [ ] **Step 3: Run the full suite**

Run: `cargo nextest run -p mcapable-core`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/tests/writer_roundtrip.rs
git commit -m "$(cat <<'EOF'
test(writer): mixed-compression round-trip coverage

Verifies that files with mixed chunk compression types (default zstd
or lz4 + per-channel uncompressed override) round-trip correctly
through Reader: messages preserved by topic, chunks visible at both
compressions, summary chunk indexes well-formed.
EOF
)"
```

---

## Task 9: Property test for mixed-compression round-trip

**Files:**
- Modify: `crates/mcapable-core/tests/writer_properties.rs` — append a proptest case using both default and override channels.

- [ ] **Step 1: Append the property test**

Append at the end of `crates/mcapable-core/tests/writer_properties.rs` (inside the file, outside the existing `proptest! { ... }` block — start a new one to keep the existing `ProptestConfig` change minimal):

```rust
proptest! {
    #![proptest_config(ProptestConfig { cases: 32, .. ProptestConfig::default() })]

    /// Write a stream of messages tagged for one of three channels:
    /// - default (zstd)
    /// - override_a (uncompressed)
    /// - override_b (lz4 with a different threshold)
    /// Reopen and assert per-channel ordering and payloads survive.
    #[test]
    fn prop_mixed_compression_roundtrip_preserves_messages(
        msgs in prop::collection::vec(
            (0u8..3u8, prop::collection::vec(any::<u8>(), 0..256)),
            0..50,
        ),
    ) {
        use mcapable_core::writer::{ChannelSpec, SchemaSpec};
        use mcapable_core::Compression;

        let out = Cursor::new(Vec::new());
        let mut writer = WriterBuilder::new()
            .chunked(ChunkOptions {
                compression: Some(Compression::Zstd),
                max_uncompressed_bytes: 128,
                include_crc: true,
            })
            .build(out)
            .unwrap();

        let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
        let mut ch_default = writer
            .add_channel(ChannelSpec::new("/default", "raw").schema(schema.clone()))
            .unwrap();
        let mut ch_a = writer
            .add_channel(
                ChannelSpec::new("/override_a", "raw")
                    .schema(schema.clone())
                    .uncompressed_chunks(),
            )
            .unwrap();
        let mut ch_b = writer
            .add_channel(
                ChannelSpec::new("/override_b", "raw")
                    .schema(schema)
                    .chunk_override(ChunkOptions {
                        compression: Some(Compression::Lz4),
                        max_uncompressed_bytes: 64,
                        include_crc: true,
                    }),
            )
            .unwrap();

        let default_id = ch_default.channel_id();
        let a_id = ch_a.channel_id();
        let b_id = ch_b.channel_id();

        let mut expected: Vec<(u16, Vec<u8>)> = Vec::new();
        let mut t = 1000u64;
        for (tag, data) in &msgs {
            t += 1;
            let (ch_id, write_res) = match tag {
                0 => (default_id, ch_default.write(t, t, data.clone())),
                1 => (a_id, ch_a.write(t, t, data.clone())),
                _ => (b_id, ch_b.write(t, t, data.clone())),
            };
            write_res.unwrap();
            expected.push((ch_id, data.clone()));
        }
        drop(ch_default);
        drop(ch_a);
        drop(ch_b);
        writer.finish().unwrap();

        let bytes = writer.into_inner().into_inner();

        let mut reader = mcapable_core::reader::Reader::from_slice(&bytes).unwrap();
        let mut got: Vec<(u16, Vec<u8>)> = Vec::new();
        for raw in reader.raw_messages().unwrap() {
            let raw = raw.unwrap();
            got.push((raw.channel_id, raw.data_bytes().to_vec()));
        }

        // Group expected by channel; group got by channel; assert per-channel
        // sequences match (order within a channel must be preserved; relative
        // order between channels follows log_time, which is monotonic above).
        use std::collections::HashMap;
        let mut group = |v: &Vec<(u16, Vec<u8>)>| -> HashMap<u16, Vec<Vec<u8>>> {
            let mut m: HashMap<u16, Vec<Vec<u8>>> = HashMap::new();
            for (id, data) in v {
                m.entry(*id).or_default().push(data.clone());
            }
            m
        };
        prop_assert_eq!(group(&expected), group(&got));
    }
}
```

- [ ] **Step 2: Run the property test**

Run: `cargo nextest run -p mcapable-core --test writer_properties prop_mixed_compression`
Expected: PASS (32 cases).

- [ ] **Step 3: Run the full suite**

Run: `cargo nextest run -p mcapable-core`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/tests/writer_properties.rs
git commit -m "$(cat <<'EOF'
test(writer): proptest mixed-compression roundtrip

Generates random message streams across one default zstd channel and
two override channels (uncompressed + lz4) and asserts that, after
write+read, per-channel message sequences are preserved.
EOF
)"
```

---

## Task 10: Add `criterion` and `rand` dev-deps + `[[bench]]` entry

**Files:**
- Modify: `crates/mcapable-core/Cargo.toml`

- [ ] **Step 1: Update `[dev-dependencies]` and add `[[bench]]`**

In `crates/mcapable-core/Cargo.toml`, replace the existing `[dev-dependencies]` block (lines 69-72):

```toml
[dev-dependencies]
proptest = "1.9"
tempfile = "3.23"
serde_json = "1"
```

with:

```toml
[dev-dependencies]
proptest = "1.9"
tempfile = "3.23"
serde_json = "1"
criterion = "0.5"
rand = "0.8"

[[bench]]
name = "mixed_compression"
harness = false
```

- [ ] **Step 2: Verify Cargo accepts the manifest**

Run: `cargo check -p mcapable-core --benches`
Expected: Cargo errors with "couldn't find file `crates/mcapable-core/benches/mixed_compression.rs`" — the file is created in Task 11. The manifest itself parses cleanly.

(If you also want to confirm the manifest parses without the bench-target lookup, run `cargo metadata -p mcapable-core --no-deps --format-version=1 > /dev/null` and expect a clean exit.)

- [ ] **Step 3: Commit**

```bash
git add crates/mcapable-core/Cargo.toml
git commit -m "$(cat <<'EOF'
build(mcapable-core): add criterion + rand dev-deps and bench target

criterion drives the mixed_compression bench; rand seeds the
high-entropy 'video' workload used to demonstrate that re-running
zstd over already-compressed bytes wastes CPU.

The bench file itself is added in the next commit.
EOF
)"
```

---

## Task 11: Mixed-compression write benchmarks

**Files:**
- Create: `crates/mcapable-core/benches/mixed_compression.rs` (write benches only — read benches added in Task 12).

- [ ] **Step 1: Create the bench file with the workload generator and three write benches**

Write `crates/mcapable-core/benches/mixed_compression.rs`:

```rust
//! Benchmarks for the per-channel chunk_override feature.
//!
//! Workload: ~30 Hz "video" channel with random 200 KB payloads (high
//! entropy — incompressible) interleaved with ~100 Hz "telemetry" channel
//! with repeating 256 B payloads (highly compressible). Three write modes:
//! all-compressed (baseline), mixed (video uncompressed via override,
//! telemetry compressed), all-uncompressed (upper bound on speed).

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
            telemetry_ch.write(m.log_time_ns, m.log_time_ns, data).unwrap();
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
                black_box(bytes.len());
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_writes);
criterion_main!(benches);
```

- [ ] **Step 2: Run the benches once to verify they execute**

Run: `cargo bench -p mcapable-core --bench mixed_compression -- --quick`
Expected: criterion runs the three write benches, prints the input/output size lines, and reports times. (The `--quick` flag uses a small sample size so this should complete in well under a minute.)

- [ ] **Step 3: Verify check + tests still pass (the bench file is part of the crate now)**

Run: `just check`
Expected: clean.

Run: `cargo nextest run -p mcapable-core`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/benches/mixed_compression.rs
git commit -m "$(cat <<'EOF'
bench(writer): add mixed_compression write benchmarks

Three modes — all-zstd, mixed (video uncompressed via override),
all-uncompressed — over a 5 s synthetic workload of incompressible
30 Hz video frames (200 KB random) interleaved with compressible
100 Hz telemetry (256 B repeating).

Reports throughput in bytes/sec (input size) plus prints output file
size per mode so the size/CPU tradeoff is visible.
EOF
)"
```

---

## Task 12: Mixed-compression read benchmarks

**Files:**
- Modify: `crates/mcapable-core/benches/mixed_compression.rs` — add a second criterion group for reads.

- [ ] **Step 1: Append the read bench function and update `criterion_group!`**

In `crates/mcapable-core/benches/mixed_compression.rs`, add the new function above the existing `criterion_group!` macro:

```rust
fn bench_reads(c: &mut Criterion) {
    let (msgs, total_bytes) = build_messages();

    // Pre-build one file per mode (outside the measured loop).
    let files = [
        (Mode::AllCompressedZstd, write_one(&msgs, Mode::AllCompressedZstd)),
        (Mode::MixedVideoUncompressed, write_one(&msgs, Mode::MixedVideoUncompressed)),
        (Mode::AllUncompressed, write_one(&msgs, Mode::AllUncompressed)),
    ];

    let mut group = c.benchmark_group("read_mixed_compression");
    group.throughput(Throughput::Bytes(total_bytes));

    for (mode, bytes) in &files {
        group.bench_with_input(
            BenchmarkId::from_parameter(mode.label()),
            bytes.as_slice(),
            |b, file_bytes| {
                b.iter(|| {
                    let mut reader =
                        mcapable_core::reader::Reader::from_slice(file_bytes).unwrap();
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
```

Replace the existing `criterion_group!(benches, bench_writes);` line with:

```rust
criterion_group!(benches, bench_writes, bench_reads);
```

- [ ] **Step 2: Run the benches once**

Run: `cargo bench -p mcapable-core --bench mixed_compression -- --quick`
Expected: criterion now runs both groups (`write_mixed_compression` and `read_mixed_compression`), three benches each.

- [ ] **Step 3: Verify check + tests still pass**

Run: `just check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/benches/mixed_compression.rs
git commit -m "$(cat <<'EOF'
bench(reader): add mixed_compression read benchmarks

For each of the three pre-written files (all-zstd, mixed,
all-uncompressed), measures wall time to construct a Reader and
drain every message via raw_messages(). Throughput axis is total
input payload bytes so wall-time is directly comparable across
modes.
EOF
)"
```

---

## Task 13: Runnable example

**Files:**
- Create: `crates/mcapable-core/examples/mixed_compression.rs`

- [ ] **Step 1: Write the example**

Create `crates/mcapable-core/examples/mixed_compression.rs`:

```rust
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

fn write_file(path: &PathBuf, msgs: &[TestMsg], video_override: Option<ChunkOptions>, default_compression: Option<Compression>) {
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
            telemetry_ch.write(m.log_time_ns, m.log_time_ns, data).unwrap();
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
        write_file(&path, &msgs, video_override.clone(), *default_compression);
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
```

- [ ] **Step 2: Run the example**

Run: `cargo run -p mcapable-core --example mixed_compression --release`
Expected: prints input size and three lines, one per mode, with write time and output size. Files written under `/tmp/mcapable_*.mcap`. The `mixed_video_uncompressed` line should show output size close to `all_uncompressed` (since random video can't shrink) and write time better than `all_compressed_zstd`.

- [ ] **Step 3: Verify check still clean**

Run: `just check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/examples/mixed_compression.rs
git commit -m "$(cat <<'EOF'
feat(examples): runnable demo for chunk_override feature

Mirrors the criterion bench shape but doesn't depend on criterion;
writes /tmp/mcapable_*.mcap for each mode and prints elapsed +
output size so the win is observable by hand:

    cargo run -p mcapable-core --example mixed_compression --release
EOF
)"
```

---

## Task 14: Final verification + summary commit

**Files:** none changed by this task — it's a clean-state check.

- [ ] **Step 1: Run the full check**

Run: `just all`
Expected: format clean, clippy clean (`-D warnings` enforced), all tests pass.

- [ ] **Step 2: Confirm there are no uncommitted changes**

Run: `git status`
Expected: working tree clean.

- [ ] **Step 3: Print the commit log for the feature**

Run: `git log --oneline ^dev HEAD`
Expected: roughly 10 commits — one per task above (Tasks 1, 2, 3 may be on the same branch already; the per-task commits should show the feature progression).

- [ ] **Step 4: (No-op commit)** No additional commit unless verification surfaced something.

If anything failed in Steps 1-2, drop into a debug cycle (use `superpowers:systematic-debugging`), fix, and commit per the same TDD pattern (failing test → fix → green → commit).

---

## Notes on the design that this plan follows

- The default write path (`write_raw_message_default`) is byte-equivalent to today's writer — readers and existing callers see no change in behavior or output for files written without any overrides.
- `ChannelWriter::has_chunk_override` is set once and read once per write; it stays in the L1 cache line of the channel writer struct.
- `override_streams: Vec<Option<ChunkState>>` is dense in practice (channel ids start at 1 and grow contiguously). Lookup is one bounds-checked index, no hash.
- Override registration is one-shot and append-only — there is no API to remove an override after registration, so the `debug_assert!` invariant in `write_raw_message_override` cannot be violated by external callers.
- The order of chunks in `chunk_indexes` reflects on-disk order (whatever stream flushed first). The summary writer iterates `chunk_indexes` directly, so this is correct without any new sorting step.
