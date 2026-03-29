no_std Support Guide

Overview
- mcapable-core supports `no_std` + `alloc` builds for parsing/types.
- IO-based Reader/Stream/Writer APIs are `std`-only.
- Compression backends (lz4/zstd) are optional and `std`-only.

Feature flags
- `std` (default): Enables IO-based APIs and std implementations.
- `alloc`: Enables core types/parsing in `no_std` environments.
- `compression`: Enables both lz4 and zstd backends (std-only).
- `lz4`, `zstd`: Enable individual backends (std-only).

Build examples
- no_std + alloc check:
  `cargo check -p mcapable-core --no-default-features --features alloc`
- wasm32 (no_std + alloc):
  `cargo check -p mcapable-core --no-default-features --features alloc --target wasm32-unknown-unknown`
- embedded (no_std + alloc):
  `cargo check -p mcapable-core --no-default-features --features alloc --target thumbv7em-none-eabihf`

Code organization conventions
- Prefer `crate::support` for shared types/macros (`Vec`, `String`, `HashMap`, `Arc`, `format`, etc).
  This keeps `std` vs `alloc/core` decisions centralized.
- Keep `std`-only APIs behind `crates/mcapable-core/src/std.rs` (the `std` module).
  Use a single `cfg(feature = "std")` at the module boundary when possible.
- Avoid sprinkling `cfg(feature = "std")` on individual items; favor module-level gating.
- When adding new code:
  - Put std-only code in a std-gated module.
  - Use `crate::support::*` for container types and formatting.
  - Keep public API paths stable via re-exports in `lib.rs`.

Quick checklist for new modules
- Is it IO-bound? -> std-only module under `std`.
- Does it use `Vec`, `String`, `HashMap`, `Arc`? -> import from `crate::support`.
- Does it require compression backends? -> guard with `feature = "compression"` or `lz4`/`zstd` inside std-only module.

Justfile helpers
- `just check-wasm` runs the wasm32 no_std + alloc build.
- `just check-nostd` runs an embedded no_std + alloc build.
- `just check-nostd-all` runs both checks.
