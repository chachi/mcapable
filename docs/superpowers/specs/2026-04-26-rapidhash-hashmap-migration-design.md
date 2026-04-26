# RapidHash HashMap Migration

## Motivation

The codebase uses `std::collections::HashMap` (and one `HashSet`) widely. We do
not need DoS-resistant hashing for any of these maps — they are populated from
trusted MCAP file contents or test fixtures — and we'd prefer the throughput of
a faster hasher. Switch to `rapidhash::RapidHashMap` / `RapidHashSet` so map
operations on hot paths (chunk decoding, channel/schema lookup, metadata
parsing) get a faster hash without changing the surrounding data flow.

## Goals

- Single `use rapidhash` site in the workspace, in `mcapable-core/src/support.rs`.
- Public re-export at `mcapable_core::collections::{HashMap, HashSet}`.
- Wrapper crates (`mcapable`, `mcapable-cpp`, `mcapable-ffi`, `mcapable-wasm`)
  consume the alias from `mcapable_core::collections` — they do **not** add
  `rapidhash` to their own `Cargo.toml`.
- Preserve the existing `std` / `no_std` split in `support.rs`.
- All existing tests continue to pass.

## Non-Goals

- Changing `BTreeMap` (kept for deterministic iteration order where used).
- Performance tuning beyond switching the hasher.
- Replacing `std::collections::hash_map::Entry` imports — `RapidHashMap` is a
  `std::collections::HashMap` with a custom hasher, so the `Entry` enum from
  `std` continues to work unchanged.
- Adding a benchmark for the change. Existing benches will exercise it
  incidentally; if a regression appears, address it separately.

## Design

### Variant choice

`rapidhash::RapidHashMap<K, V>` (alias for
`std::collections::HashMap<K, V, RapidRandomState>`). Random-seed variant, not
the fixed-seed `fast::` family. Fast enough for our needs and avoids the
fixed-seed footgun if a map is ever seeded from external input later.

### `mcapable-core/src/support.rs`

The single `use rapidhash` site. The `std` and `no_std` branches each pull in
rapidhash differently:

```rust
#[cfg(feature = "std")]
mod imp {
    pub use std::borrow::Borrow;
    pub use std::error;
    pub use std::fmt;
    pub use std::format;
    pub use std::ops;
    pub use std::result::Result;
    pub use std::str;
    pub use std::string::{String, ToString};
    pub use std::sync::Arc;
    pub use std::vec::Vec;
    pub use rapidhash::{RapidHashMap as HashMap, RapidHashSet as HashSet};
}

#[cfg(not(feature = "std"))]
mod imp {
    pub use alloc::format;
    pub use alloc::string::{String, ToString};
    pub use alloc::sync::Arc;
    pub use alloc::vec::Vec;
    pub use core::borrow::Borrow;
    pub use core::error;
    pub use core::fmt;
    pub use core::ops;
    pub use core::result::Result;
    pub use core::str;
    pub type HashMap<K, V> = hashbrown::HashMap<K, V, rapidhash::RapidHasher>;
    pub type HashSet<T> = hashbrown::HashSet<T, rapidhash::RapidHasher>;
}

pub use imp::*;
```

`RapidHasher` is `Default + BuildHasher`, so `HashMap::default()` and
`HashSet::default()` work in both branches.

### `mcapable-core/src/lib.rs`

Add a public re-export module. `support` stays private.

```rust
pub mod collections {
    //! HashMap / HashSet aliases backed by rapidhash.
    pub use crate::support::{HashMap, HashSet};
}
```

### Cargo.toml

`crates/mcapable-core/Cargo.toml`:

- Add `rapidhash` to `[dependencies]` with `default-features = false`. Pin to
  the current published 1.x release at implementation time.
- Plumb the appropriate rapidhash feature(s) into the existing `std` feature
  list. The implementation step must check `cargo doc -p rapidhash` (or the
  crate's README) for the exact feature names that gate `RapidHashMap`,
  `RapidHashSet`, and `RapidRandomState` — they are likely named `std` and
  `rapid_protected` (or similar) but should be verified, not guessed.

No other `Cargo.toml` files change.

### Mechanical edits

The following changes are mechanical and apply uniformly:

1. **Imports.** Replace `use std::collections::HashMap` (and `HashSet`) with:
   - Inside `mcapable-core/src/**`: `use crate::support::{HashMap, HashSet}`
     (matching the existing pattern in files that already use `support`).
   - Inside `mcapable-core` tests/benches and all other crates' src/tests/benches:
     `use mcapable_core::collections::{HashMap, HashSet}`.
   - Mixed imports like `use std::collections::{BTreeMap, HashMap}` split:
     keep `BTreeMap` from std, take `HashMap` from the new path.

2. **Construction.** Every `HashMap::new()` becomes `HashMap::default()`. Every
   `HashMap::with_capacity(n)` becomes
   `HashMap::with_capacity_and_hasher(n, Default::default())`. Same for
   `HashSet`. The inherent `::new` method only exists on the
   `RandomState`-specialized type.

3. **Doctest in `crates/mcapable-core/src/reader.rs` (around line 807)**: the
   `use std::collections::HashMap;` line in the example becomes the new path.
   Verify the example still compiles after the swap.

4. **`HashSet` site.** `crates/mcapable-core/src/conformance/metadata.rs:77`
   uses `std::collections::HashSet` inline; switch to the alias.

### Files affected

Source (must use the internal `crate::support` path):

- `crates/mcapable-core/src/types.rs`
- `crates/mcapable-core/src/reader.rs`
- `crates/mcapable-core/src/parser.rs`
- `crates/mcapable-core/src/parser_properties.rs`
- `crates/mcapable-core/src/test_generators.rs`
- `crates/mcapable-core/src/stream/filters.rs`
- `crates/mcapable-core/src/stream/parsed/defaults.rs`
- `crates/mcapable-core/src/stream/schema_parser/idl/mod.rs`
- `crates/mcapable-core/src/stream/schema_parser/ros/ros1.rs`
- `crates/mcapable-core/src/writer/api.rs`
- `crates/mcapable-core/src/writer/encode.rs`
- `crates/mcapable-core/src/writer/internal.rs`
- `crates/mcapable-core/src/writer/types.rs`
- `crates/mcapable-core/src/writer/rolling/builder.rs`
- `crates/mcapable-core/src/writer/rolling/writer.rs`

Source in wrapper crates (must use the public `mcapable_core::collections` path):

- `crates/mcapable-cpp/src/lib.rs`
- `crates/mcapable-ffi/src/lib.rs`
- `crates/mcapable-wasm/src/lib.rs`
- `crates/mcapable-swift/src/lib.rs` (verify — listed as a HashMap-using file
  in the audit)

Tests / benches:

- `crates/mcapable-core/tests/reader_api.rs`
- `crates/mcapable-core/tests/writer_properties.rs`
- `crates/mcapable-core/tests/writer_roundtrip.rs`
- `crates/mcapable-core/tests/rolling_writer.rs`
- `crates/mcapable-core/src/conformance/metadata.rs`
- `crates/mcapable/tests/roundtrip_equivalence.rs`
- `crates/mcapable/tests/comparison_tests.rs`
- `crates/mcapable/tests/helpers/mcap_builder.rs`
- `crates/mcapable/tests/helpers/generators.rs`
- `crates/mcapable/benches/writer_vs_mcap.rs`
- `crates/mcapable-cli/tests/cmd_tests.rs`

This list is from a `grep -rln HashMap --include='*.rs'` audit at design time.
The implementation pass should re-run the grep to catch any drift.

### Public API impact

These public types change their hash-map type parameter from
`std::collections::HashMap<..>` to `RapidHashMap<..>` (still a
`std::collections::HashMap`, just with `RapidRandomState`):

- `mcapable_core::types::Schema.metadata`
- `mcapable_core::types::Channel.metadata`
- `mcapable_core::types::Metadata.metadata`
- `mcapable_core::types::ChunkIndex.message_index_offsets`
- `mcapable_core::types::Summary.{schemas, channels}`
- `mcapable_core::reader::Reader::{schemas, channels, all_metadata, all_attachments}`

Downstream code that constructs these literally
(e.g. `Schema { metadata: HashMap::new(), .. }`) breaks until updated to use
`HashMap::default()` or import the new alias. The pre-1.0 wrapper crates and
test suites are the only in-tree consumers; we update them in the same change.

The commit message must call out the public-API change.

### Verification

- `just check` (fast iteration during the migration).
- `just all` before commit (fmt + clippy `-D warnings` + tests).
- `cargo nextest run` across the workspace.
- Spot-check the reader doctest at `reader.rs:807` compiles.
- `grep -rn 'std::collections::HashMap\|std::collections::HashSet' --include='*.rs'`
  returns no hits except inside docstrings that intentionally name the std
  type.

### Risks

- **Rapidhash feature flag names.** The Cargo.toml plumbing depends on the
  exact feature names rapidhash exposes for `std` and `RapidRandomState`. The
  implementation step must verify these against the published crate before
  wiring them up.
- **`with_capacity` translation noise.** Every site needs the
  `with_capacity_and_hasher(n, Default::default())` rewrite. Mechanical but
  easy to miss one — rely on `cargo check` to flag it.
- **Doctest breakage.** Doctests aren't covered by `cargo check`; they need
  `cargo test --doc` (or `just all`) to validate.
