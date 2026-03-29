# mcapable

A reimplementation of the Rust MCAP library with a cleaner, more uniform API design.

## Design Philosophy

This library reimagines the MCAP reader API with the following goals:

1. **Separation of Concerns**:
   - `Reader<R>` - Owns the data source and file metadata
   - `Stream<'a, T>` - Borrows from Reader to iterate over records

2. **Lazy Loading**: Everything is loaded on-demand:
   - Reader construction: Only validates magic bytes
   - Header: Loaded on first access
   - Summary: Loaded when needed (e.g. `.get()` random access)
   - Schemas/Channels: Loaded from summary when available, otherwise encountered during iteration

3. **Zero-Copy by Default**:
   - All parsing is done over `bytes::Bytes` buffers
   - Strings are represented as `ByteStr` (UTF-8 validated once, then borrowed)
   - Message payloads use `Bytes` slices over a shared backing buffer

4. **Minimal I/O Surface Area**:
   - `Reader<R>` is generic over `R: BytesSource` (seek + `read_exact_bytes` returning `Bytes`)
   - `BytesCursor` provides a fully zero-copy in-memory source
   - Any `Read + Seek` can be used (it will allocate into `Bytes` for reads)

5. **Borrow-Checked Safety**:
   - Stream creation requires `&mut Reader` to guarantee only one active stream at a time
   - Cache metadata you need before streaming if you want to access it during iteration

6. **Builder Pattern**: Readers are constructed through `reader::Builder`:
   - Minimal configuration (most behavior is automatic)
   - Type-safe reader construction

7. **Stream Types**: Different stream types for different use cases:
   - `Stream<'_, Record>` - All parsed record types
   - `Stream<'_, RawRecord>` - Unparsed record payloads
   - `Stream<'_, Chunk>` - Chunks only
   - `Stream<'_, RawMessage>` - Fast message iteration
   - `Stream<'_, Message>` - Full messages with metadata
   - `Stream<'_, MessageMetadata>` - Message headers without payload
   - `Stream<'_, RecordMetadata>` - Record headers without payload

## API Examples

### Reading Messages (High Level)

```rust
use mcapable::reader;
use std::fs::File;

let file = File::open("data.mcap")?;
// Reader creation is fast - only validates magic bytes!
let mut reader = reader::Builder::new().build(file)?;

// Header loaded lazily on first access
println!("Profile: {}", reader.header()?.profile);

// Cache channels before streaming if you want to access them while iterating.
let channels = reader.channels();

// Iterate over messages (Stream implements Iterator)
for message in reader.messages()? {
    let message = message?;
    println!("Channel {}: {} bytes at time {}",
             message.channel_id,
             message.data_len(),
             message.log_time);
}
```

### Reading Raw Messages (Faster)

When you don't need schema/channel metadata:

```rust
use mcapable::reader;
use std::fs::File;

let file = File::open("data.mcap")?;
let mut reader = reader::Builder::new().build(file)?;

for raw_msg in reader.raw_messages()? {
    let raw_msg = raw_msg?;
    // Process raw message data
}
```

### Reading Chunks

For analyzing file structure or custom chunk processing:

```rust
use mcapable::reader;
use std::fs::File;

let file = File::open("data.mcap")?;
let mut reader = reader::Builder::new().build(file)?;

for chunk in reader.chunks() {
    let chunk = chunk?;
    println!("Chunk: {} -> {} (compression: {})",
             chunk.message_start_time,
             chunk.message_end_time,
             chunk.compression);
}
```

### Filtering by Time Range

```rust
let file = File::open("data.mcap")?;
let mut reader = reader::Builder::new().build(file)?;

for message in reader.messages()?.time_range(start_time, end_time) {
    // Only messages in the time range
}
```

### Filtering by Channel

```rust
let file = File::open("data.mcap")?;
let mut reader = reader::Builder::new().build(file)?;

for message in reader.messages()?.filter_channel(|ch| ch.topic.starts_with("/camera")) {
    // Only messages on channels matching the predicate
}
```

### Multiple Streams (Sequential)

```rust
let file = File::open("data.mcap")?;
let mut reader = reader::Builder::new().build(file)?;

// First stream: count chunks
for chunk in reader.chunks() {
    let chunk = chunk?;
    // Process chunk
}

// Second stream: read messages from same reader
for message in reader.messages()? {
    let message = message?;
    // Process message
}
```

### Zero-Copy Construction

If the whole file is already in memory, you can build a zero-copy reader:

```rust
use mcapable::Reader;
let bytes = std::fs::read("data.mcap")?;
let mut reader = Reader::from_slice(&bytes)?;
println!("profile = {}", reader.header()?.profile);
```

## Benchmarks

Criterion benchmarks live in `benches/`.

- Run comparisons vs the official `mcap` crate: `cargo bench --bench vs_mcap`
- Run internal reader benchmarks: `cargo bench --bench reader_benchmark`

The `vs_mcap` benchmarks are structured using Criterion “comparing functions” patterns and
benchmark multiple MCAP sizes with `Throughput::Bytes` and `iter_batched` to separate setup
from iteration cost.

## Architecture Details

### Reader Structure

```
Reader<R: BytesSource>
├── reader: PositionTrackingSource<R>      // Seekable data source
├── header: Option<Header>                 // Lazily loaded
├── footer: Option<Footer>                 // Lazily loaded
├── schemas: Arc<HashMap<u16, Schema>>     // Lazily loaded, shared
├── channels: Arc<HashMap<u16, Channel>>   // Lazily loaded, shared
├── summary: Option<Summary>               // Lazily loaded
├── metadata: Arc<HashMap<ByteStr, Metadata>>
└── attachments: Arc<HashMap<ByteStr, Attachment>>
```

### Stream Structure

```
Stream<'a, T>
├── reader: &'a mut dyn ReaderAccess     // Trait object hides R
├── time_range: Option<(Timestamp, Timestamp)>
├── channel_predicate: Option<ChannelPredicate<'a>>
├── chunk_filter: Option<ChunkFilterPredicate<'a>>
├── message_filter: Option<MessageFilterPredicate<'a>>
├── record_filter: Option<RecordFilterPredicate<'a>>
├── chunk_state: Option<ChunkState>      // Decompression state
└── _phantom: PhantomData<T>             // Output type
```

### BytesSource Trait

```rust
pub trait BytesSource {
    fn read_exact_bytes(&mut self, len: usize) -> io::Result<Bytes>;
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
}

// Blanket impl: any Read + Seek automatically implements BytesSource
impl<T: Read + Seek> BytesSource for T { ... }
```

This minimal trait returns `Bytes` from reads, enabling zero-copy pipelines when the data is already in memory (via `BytesCursor`). The `ReaderAccess` trait object hides the concrete `BytesSource` type from `Stream`.

### Stream Types

Each Stream type implements `Iterator<Item = Result<T>>`:

- **Record Stream** (`.records()`): All parsed record types
- **RawRecord Stream** (`.raw_records()`): Unparsed record payloads
- **Chunk Stream** (`.chunks()`): Chunk records only
- **RawMessage Stream** (`.raw_messages()`): Decompresses chunks, yields `RawMessage`s
- **Message Stream** (`.messages()`): Full messages with metadata resolution
- **MessageMetadata Stream** (`.message_metadata()`): Message headers without payload
- **RecordMetadata Stream** (`.record_metadata()`): Record headers without payload

### Filtering

Streams support chainable filtering:
- Time range filtering via `.time_range(start, end)`
- Channel filtering via `.filter_channel(|ch| ...)` with a predicate closure
- Chunk-level filtering via `.chunk_filter(...)` (evaluated before decompression)
- Message header filtering via `.message_filter(...)`
- Record type filtering via `.record_filter(...)`
- Filters can be combined

## Configuration Options

### reader::Builder

```rust
reader::Builder::new()
    .validate_end_magic(bool)  // Validate magic bytes at end of file (default: true)
    .build(reader)?
```

Note: Summary and metadata loading is always lazy - triggered on-demand.

## Status

Current status:
- ✅ Reader implementation (lazy loading, metadata caching)
- ✅ Stream implementation (all 7 stream types)
- ✅ Writer implementation (chunked, compressed output)
- ✅ MCAP format parsing (all record types)
- ✅ Compression support (zstd, lz4 via feature flags)
- ✅ CRC32 validation
- ✅ Summary/index support (lazy-loaded from footer)
- ✅ Zero-copy in-memory reads (BytesCursor)
- ✅ Tests (conformance, round-trip, property-based)
- ✅ Documentation (rustdoc on all public items)
- ✅ Random access via `.get(n)` on Chunk and Message streams

## Differences from Original foxglove/mcap

1. **Reader/Stream Separation**: Reader owns the data, Streams borrow to iterate.

2. **Trait Object Hiding**: Stream uses `&dyn ReaderAccess` to hide the concrete `BytesSource` type.

3. **Builder-Only Construction**: Forces users to think about configuration options upfront.

4. **BytesSource Trait**: Any `Read + Seek` works via blanket impl; in-memory `Bytes` enables true zero-copy.

5. **Method-Based Streams**: `reader.messages()` instead of separate types.

6. **Integrated Filtering**: Built-in predicate-based filter support via chainable methods.

## Advantages

| Feature | foxglove/mcap | mcapable |
|---------|---------------|-----------|
| Ownership | Coupled | Separated (Reader/Stream) |
| Type Hiding | Exposed generics | ReaderAccess trait object |
| Data Sources | Read + Seek | Any BytesSource (Read + Seek, or zero-copy Bytes) |
| Configuration | Direct construction | Builder pattern |
| Filtering | Manual | Built-in chainable predicates |
| Multiple Iterations | Requires multiple readers | Multiple streams from one reader |
