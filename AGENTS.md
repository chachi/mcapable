# Agent Instructions for mcapable

## Project Overview

Reimplementation of the Rust MCAP library with a lazy-loading Reader/Stream architecture.

## Build System

**Use `cargo check` (or `just check`) for fast iteration** - it's much faster and uses less battery than building.

Only use `cargo build` or `cargo run` when you actually need to run something.

```bash
just check        # Fast type checking (use this during development!)
just all          # Format, lint, test (run before committing!)
```

Other commands (less commonly needed):
```bash
just              # List available commands
just build        # Debug build (only when you need to run it)
just release      # Release build (only for production/benchmarks)
just fmt-check    # Check formatting (CI only)
just watch        # Watch mode for check
just watch-test   # Watch mode for tests
```

## Testing

**ALWAYS use `cargo nextest` instead of `cargo test`** for running tests:

```bash
cargo nextest run                    # Run all tests
cargo nextest run --test conformance_tests  # Run specific test suite
cargo nextest list                   # List all tests
cargo nextest run <filter>           # Run tests matching filter
```

Why nextest?
- Faster parallel execution
- Better output formatting
- Cleaner test isolation
- Industry standard for Rust projects

Note: `just all` uses `cargo test` internally for compatibility, but when running tests manually, always use `cargo nextest`.

## Code Standards

**Always run `just all` before committing.** This ensures:
- Code is formatted (`cargo fmt`)
- No clippy warnings (`cargo clippy -- -D warnings`)
- All tests pass

### Dead Code
- Use `#[allow(dead_code)]` with comment explaining why for stubs
- Example: `#[allow(dead_code)] // Will be used during implementation`

### Documentation
- All public items need doc comments
- Use `///` for item docs, `//!` for module docs
- Include examples in doc comments where useful

## Git Workflow

**Commit relevant changes as you go**, not just at the end:

1. After completing a logical unit of work (new feature, bug fix, refactor)
2. Run `just all` to ensure everything passes
3. Stage only relevant files: source code, examples, Cargo.toml, tests
4. **Do NOT commit:** temporary notes, scratch files, `.md` documentation drafts
5. Write clear commit messages that explain what and why

Example workflow:
```bash
# Work on feature
just all                           # Verify it works
git add src/ examples/ Cargo.toml  # Stage relevant files
git commit -m "Add feature X"      # Commit with clear message
```

Good commit message structure:
- First line: Brief summary (50 chars or less)
- Blank line
- Detailed explanation of what changed and why
- Note any breaking changes or important details

## Architecture

### Core Types

**Reader<R>** - Owns data source, lazy loads metadata
- Requires `&mut self` for stream creation to ensure safety via borrow checker
- No interior mutability (RefCell) - explicit mutability requirements
- Caches: header, schemas, channels, summary
- Methods: `.messages()`, `.raw_messages()`, `.chunks()`, `.records()`
- Metadata access: `&self` methods like `.schema()`, `.channel()`

**Stream<'a, T>** - Mutably borrows from Reader, iterates
- Generic over output type T: Record<'a>, Chunk, RawMessage<'a>, Message<'a>
- Uses `&'a mut dyn ReaderAccess` to hide R generic
- Implements `Iterator` for sequential access
- Supports `.get(n)` for random access (using summary indexes)
- Supports filtering: `.time_range()`, `.channels()`
- **Only one active stream at a time** due to `&mut self` requirement

### Accessing Metadata During Iteration

Since streams require `&mut self`, you cannot call Reader methods during iteration.
**Pattern: Cache metadata before streaming**

```rust
// Cache metadata (automatically loaded internally)
let channels = reader.channels();
let schemas = reader.schemas();

// Now iterate with cached metadata
for msg in reader.messages() {
    let m = msg?;
    if let Some(ch) = channels.get(&m.channel_id) {
        println!("Topic: {}", ch.topic);
    }
}
```

Metadata is automatically loaded internally when needed. Just cache the
results before streaming to avoid borrowing conflicts.

### Lazy Loading

Reader does minimal work on construction:
- Only validates MCAP magic bytes
- Everything else loaded on-demand and cached

Load triggers:
- `header()` - loads header
- `summary()` - loads summary from end of file
- Stream creation - automatically preloads schemas/channels for iteration

## File Structure

```
src/
├── lib.rs      # Public API exports
├── error.rs    # Error types
├── types.rs    # MCAP data types
├── reader.rs   # Reader<R> and reader::Builder
├── stream.rs   # Stream<'a, T> and ReaderAccess trait
└── main.rs     # Dev testing (will be removed)

examples/
├── basic.rs           # Basic usage
├── filtering.rs       # Time/channel filtering
├── metadata.rs        # Working with schemas/channels
├── multiple_streams.rs # Multiple iterations
├── chunks.rs          # Working with chunks
└── records.rs         # Low-level record iteration
```

## Adding Examples

Examples go in `examples/` directory. Run with:

```bash
cargo run -p mcapable --example basic
cargo run -p mcapable --example filtering
```

Each example should:
- Be self-contained
- Have clear comments explaining what it demonstrates
- Handle errors properly (use `?` operator)
- Use realistic patterns

## Implementation TODOs

Search for `todo!()` to find unimplemented sections.

## Test Organization

- Unit tests go in same file as code (`#[cfg(test)]` module)
- Integration tests go in `tests/` directory
- Example files double as integration tests

**Use `cargo nextest run` to run tests** (see Testing section above for details).

For comprehensive validation before commit:
```bash
just all            # Format, lint, and test everything
```

## Pre-commit

Install pre-commit hooks:
```bash
pre-commit install
```

Hooks automatically run `just all` on commit (format, lint, test).

## Dependencies

Current:
- `thiserror` - Error derive macro

Future:
- Compression: `zstd`, `lz4`
- CRC: `crc32fast` or similar
- Binary parsing: `byteorder` or use std

## Conventions

### Naming
- Types: PascalCase
- Functions/methods: snake_case
- Constants: SCREAMING_SNAKE_CASE
- Use namespaces: `reader::Builder` not `ReaderBuilder`

### Error Handling
- Return `Result<T>` (uses our Error type)
- Use `?` for propagation
- Add context with `.map_err()` when useful

### Imports
- Group: std, external crates, crate modules
- Use `use crate::` for internal imports

### Comments
- `//` for implementation notes
- `///` for doc comments
- `// TODO:` for incomplete work
- `#[allow(...)]` with explanation comment

## MCAP Spec Coverage

All record types from https://mcap.dev/reference must be supported:
- Header, Footer, Schema, Channel, Message, Chunk
- MessageIndex, ChunkIndex, AttachmentIndex, MetadataIndex
- Attachment, Metadata, Statistics
- SummaryOffset, DataEnd
- Compression: zstd, lz4, none
