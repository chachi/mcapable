# C++ Wrapper Plan

Goals
- Provide a modern C++ RAII API over the Rust Reader/Stream/Writer surfaces.
- Use `cxx` for a type-safe bridge (no C ABI for the primary wrapper).
- Keep lifetimes explicit: one active stream per reader, with `unique_ptr` ownership.

Module layout (proposed)
- `mcapable::Reader`, `mcapable::ReaderBuilder`
- `mcapable::Writer`, `mcapable::WriterBuilder`, `mcapable::ChannelWriter`
- `mcapable::Stream<T>` where `T` is `Record`, `RawMessage`, `Message`, `Chunk`
- `mcapable::types` namespace for record and metadata types

Type mapping
- `ByteStr` -> `std::string` (UTF-8)
- `Bytes`/payload -> `rust::Vec<uint8_t>`
- `Vec<T>` -> `rust::Vec<T>` for FFI boundary, C++ wrapper converts to `std::vector<T>`
- `HashMap<ByteStr, ByteStr>` -> `std::vector<std::pair<std::string, std::string>>`

Reader API mapping
- `ReaderBuilder::new()` -> `ReaderBuilder()`
- `ReaderBuilder::validate_end_magic(bool)` -> `ReaderBuilder::validate_end_magic(bool)`
- `Reader::from_path(std::string)` -> constructs `Reader`
- `Reader::from_bytes(rust::Vec<uint8_t>)` -> constructs `Reader`
- `Reader::header()`, `summary()`, `footer()` -> value-returning accessors
- `Reader::schemas()`, `channels()` -> `std::vector<Schema/Channel>` or `std::unordered_map` in wrapper
- `Reader::metadata(name)`, `all_metadata()` -> `Metadata` types
- `Reader::attachment(name)`, `all_attachments()` -> `Attachment` types

Stream API mapping
- `Reader::records()` -> `Stream<Record>`
- `Reader::raw_messages()` -> `Stream<RawMessage>`
- `Reader::messages()` -> `Stream<Message>`
- `Reader::chunks()` -> `Stream<Chunk>`
- `Stream::next()` -> `std::optional<T>`
- `Stream::time_range(start, end)` -> returns `Stream&`
- `Stream::filter_channel(predicate)` -> C++ predicate; bridge uses function pointer + context

Iteration semantics
- Streams hold an exclusive mutable borrow of `Reader` (enforced in Rust),
  so the C++ wrapper uses `std::unique_ptr<Stream<T>>` and disables copying.

Writer API mapping (MVP)
- `WriterBuilder::new()` -> `WriterBuilder()`
- `WriterBuilder::profile(std::string)` / `library(std::string)`
- `WriterBuilder::chunked(ChunkOptions)` -> expose `ChunkOptions` struct
- `Writer::copy_schema`, `Writer::copy_channel` -> `ChannelWriter`
- `ChannelWriter::write(log_time, publish_time, data, sequence)`

Errors
- Map Rust `Error` to C++ exceptions derived from `std::runtime_error`.
- Provide `McapError` base type with subclasses for parse/compression/IO.

Out of scope (initial pass)
- Zero-copy payload views into chunk buffers.
- Advanced builder helpers for C++-specific I/O.
