# TypeScript Wrapper Plan

Platform choice
- Primary target: `wasm32-unknown-unknown` with `wasm-bindgen` for browser + Node.js.
- Use `wasm-pack` for JS glue and TypeScript bindings.

Module layout (proposed)
- `@mcapable/wasm` package exporting `Reader`, `ReaderBuilder`, and record types.
- Build output includes `.d.ts` types from `wasm-bindgen`.

Type mapping
- `ByteStr` -> `string`
- `Bytes`/payload -> `Uint8Array`
- `Vec<T>` -> `Array<T>`
- `HashMap<ByteStr, ByteStr>` -> `Array<[string, string]>`

Reader API mapping
- `ReaderBuilder.new()` -> `new ReaderBuilder()`
- `ReaderBuilder.validate_end_magic(bool)` -> `validateEndMagic(bool)`
- `Reader.fromBytes(Uint8Array)` -> `Reader.fromBytes(bytes)`
- `Reader.fromPath` not supported in browser; Node-only helper can be added later.
- `Reader.header()`, `schemas()`, `channels()`, `metadata()` -> methods returning JS objects.

Stream API mapping
- `Reader.records()` -> `RecordStream`
- `Reader.rawMessages()` -> `RawMessageStream`
- `Reader.messages()` -> `MessageStream`
- `Reader.chunks()` -> `ChunkStream`
- Streams expose `next()` returning `{ done, value }` for async iteration.

Filters
- `stream.timeRange(start, end)` -> filter in Rust.
- `stream.filterChannelIds(ids: number[])` -> filter by channel id.

Errors
- Map Rust `Error` to JS `Error` with message + name.
- Convert IO errors into `McapError` subclasses when supported.

Out of scope (initial pass)
- File-based Reader for browser (requires file handle API).
- Streaming I/O adapters for Node.
