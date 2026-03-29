# Wrapper Overview

This directory tracks cross-language bindings and the parity checklist for the
Rust core API. Each wrapper should expose the same reader/stream surface as the
Rust crate, plus parsed message support where the language supports dynamic
objects.

## Wrapper locations
- Python: `crates/mcapable-py`
- C++: `crates/mcapable-cpp`
- TypeScript (WASM): `crates/mcapable-wasm`
- C ABI: `crates/mcapable-ffi`

## Examples
Each language has a matching set of examples under `examples/`:
- Rust: `examples/rust`
- Python: `examples/python`
- C++: `examples/cpp`
- TypeScript: `examples/typescript`
- C: `examples/c`

## Parity checklist
- [x] ReaderBuilder + Reader.fromBytes
- [x] Metadata access: header, schemas, channels
- [x] Streams: messages, raw messages, chunks, records
- [x] Parsed stream: JSON to native object, bytes fallback
- [x] Stream filters: time range + channel id
- [x] Single-stream borrow semantics (explicit handoff to Reader)

## Release checklist
- Rust core: `just all`
- Python: `maturin build` in `crates/mcapable-py`
- C++: `cargo build -p mcapable-cpp` + run `examples/cpp/*`
- TypeScript: `npm run build` + `npm test` in `crates/mcapable-wasm`
- C ABI: `cargo build -p mcapable-ffi` + run `examples/c/*`
