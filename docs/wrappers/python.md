# Python Wrapper Plan

Goals
- Provide a Pythonic Reader/Stream API for reading MCAP files.
- Support basic writing via Writer/ChannelWriter.
- Avoid exposing internal Rust types; map to Python types and dataclasses.

Module layout (proposed)
- `mcapable.Reader`, `mcapable.ReaderBuilder`
- `mcapable.Writer`, `mcapable.WriterBuilder`, `mcapable.ChannelWriter`
- `mcapable.Stream` (iterator), typed by `Record`, `RawMessage`, `Message`, `Chunk`
- `mcapable.types` module for records and metadata types

Type mapping
- `ByteStr` -> Python `str` (UTF-8). Convert on boundary.
- `Bytes`/payload -> Python `bytes` (zero-copy where possible).
- `Arc<...>` -> Python owned copy or shared reference; avoid lifetime leaks.

Reader API mapping
- `ReaderBuilder.new()` -> `ReaderBuilder()`
- `ReaderBuilder.validate_end_magic(bool)` -> `ReaderBuilder.validate_end_magic(bool)`
- `ReaderBuilder.build(file_like)` -> `Reader.from_file(path_or_file)`
- `Reader.from_bytes(bytes)` -> `Reader.from_bytes(bytes)`
- `Reader.header()`, `profile()`, `summary()`, `footer()` -> Python properties or methods.
- `Reader.schemas()`, `channels()` -> dicts keyed by id.
- `Reader.metadata(name)`, `all_metadata()` -> Metadata objects/dict.
- `Reader.attachment(name)`, `all_attachments()` -> Attachment objects/dict.
- `Reader.message_indexes()`, `chunk_indexes()`, `attachment_indexes()`, `metadata_indexes()` -> lists.

Stream API mapping
- `Reader.records()` -> `Stream<Record>`
- `Reader.raw_messages()` -> `Stream<RawMessage>`
- `Reader.messages()` -> `Stream<Message>`
- `Reader.chunks()` -> `Stream<Chunk>`

Filters (Python-friendly)
- `Stream.time_range(start, end)` -> same semantics.
- `Stream.filter_channels(ids: list[int])` -> Rust predicate on channel id.
- `Stream.filter_topic_prefix(prefix: str)` -> predicate on `Channel.topic`.
- `Stream.filter_schema_id(schema_id: int)` -> predicate on `Channel.schema_id`.
Note: avoid Python callback predicates initially to keep GIL + lifetime handling simple.

Iteration semantics
- Streams borrow the Reader mutably; only one active stream at a time.
- Enforce at runtime with an internal borrow flag; raise a Python exception if violated.

Writer API mapping (MVP)
- `WriterBuilder.new()` -> `WriterBuilder()`
- `WriterBuilder.profile(str)`, `.library(str)`, `.header_metadata(k, v)`
- `WriterBuilder.chunked(ChunkOptions)` -> accept `ChunkOptions(compression=None|\"lz4\"|\"zstd\", max_uncompressed_bytes=int)`
- `WriterBuilder.build(file_like)` -> `Writer.from_file(path_or_file)`
- `Writer.copy_schema(schema)`, `Writer.copy_channel(channel)` -> return `ChannelWriter`
- `ChannelWriter.write(log_time, publish_time, data, sequence=None)`

Errors
- Map `Error` to Python exceptions (ValueError for InvalidRecord, IOError for IO, etc.).
- Preserve error string for diagnostics.

Out of scope (initial pass)
- ParsedStream API (serde-based parsing).
- Custom Python predicates for filtering.
