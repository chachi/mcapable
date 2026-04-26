# RapidHash HashMap Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace every `std::collections::HashMap` (and the few `std::collections::HashSet`) usages across the workspace with `rapidhash::RapidHashMap` / `RapidHashSet`, with a single `use rapidhash` import in `mcapable-core/src/support.rs` and a public re-export at `mcapable_core::collections`.

**Architecture:** Treat `mcapable-core::support` as the single rapidhash entry point. Add a public `collections` module that re-exports `HashMap`/`HashSet` so wrapper crates and tests import from `mcapable_core::collections` (or `crate::support` when inside `mcapable-core`). Stage the migration so every commit builds: first wire up the alias backed by `std` types, migrate every callsite onto the alias, fix `::new()` → `::default()` (works for both std and rapidhash), then flip the underlying type. Final commit changes only `support.rs`.

**Tech Stack:** Rust 2024 edition, `rapidhash` (random-seed `RapidHashMap` / `RapidHashSet`), existing `hashbrown` for the `no_std` branch.

**Reference spec:** `docs/superpowers/specs/2026-04-26-rapidhash-hashmap-migration-design.md`

---

## Pre-flight

The migration is mechanical. Use these commands repeatedly:

- Fast type-check: `just check`
- Run the full test suite: `cargo nextest run --workspace`
- Pre-commit gate: `just all`
- Find stragglers: `grep -rn 'std::collections::Hash\(Map\|Set\)' --include='*.rs' .`

The work happens in the existing checkout; no worktree handoff is required.

---

## Task 1: Add `rapidhash` dependency to `mcapable-core`

**Files:**
- Modify: `crates/mcapable-core/Cargo.toml`

The published `rapidhash` crate (1.x) gates `RapidHashMap` / `RapidHashSet` / `RapidRandomState` behind a `std` feature, and its `RapidHasher` is available without `std`. Verify the exact feature names from the crate's published metadata before wiring up.

- [ ] **Step 1: Look up the current `rapidhash` published version and feature flags**

Run: `cargo search rapidhash --limit 1`
Then verify the feature names (`std`, etc.) by inspecting the crate's published `Cargo.toml`:
Run: `cargo tree -p rapidhash --features rapidhash/std 2>/dev/null; cargo doc --no-deps -p rapidhash --open 2>/dev/null || true`

Note the version number (e.g. `1.4.0`) and the exact feature name(s) gating `RapidHashMap`/`RapidHashSet`. Use those exact strings in the next step.

- [ ] **Step 2: Add the dependency line**

Edit `crates/mcapable-core/Cargo.toml`. In the `[dependencies]` block (after the existing `hashbrown` line is a natural spot), add:

```toml
rapidhash = { version = "<resolved-version>", default-features = false }
```

Replace `<resolved-version>` with the version found in Step 1 (e.g. `"1.4"`).

- [ ] **Step 3: Plumb `rapidhash` features into the existing `std` feature**

In the same file, locate the `std = [ ... ]` feature list. Append the rapidhash std feature so std-only items become available when callers enable `mcapable-core/std`. Example:

```toml
std = [
  "alloc",
  "arcstr/std",
  "bytes/std",
  "crc32fast/std",
  "nom/std",
  "nom_locate/std",
  "dep:polymock",
  "strum/std",
  "thiserror/std",
  "rapidhash/std",
]
```

Use the exact feature name verified in Step 1. If rapidhash also gates `RapidRandomState` behind a separate feature (sometimes called `rapid_protected` or similar in this crate family), append that too.

- [ ] **Step 4: Verify the workspace still builds**

Run: `just check`
Expected: clean check, no errors. (No code references rapidhash yet — this just validates the manifest.)

- [ ] **Step 5: Commit**

```bash
git add crates/mcapable-core/Cargo.toml
git commit -m "chore(core): add rapidhash dependency"
```

---

## Task 2: Add `HashSet` to `support.rs` and create the public `collections` module

This task introduces the alias surface without changing the underlying type. After this commit, `mcapable_core::collections::HashMap` exists and equals `std::collections::HashMap` — semantically identical, so nothing breaks.

**Files:**
- Modify: `crates/mcapable-core/src/support.rs`
- Modify: `crates/mcapable-core/src/lib.rs`

- [ ] **Step 1: Add `HashSet` re-exports to `support.rs`**

Replace the entire content of `crates/mcapable-core/src/support.rs` with:

```rust
#[cfg(feature = "std")]
mod imp {
    pub use std::borrow::Borrow;
    pub use std::collections::{HashMap, HashSet};
    pub use std::error;
    pub use std::fmt;
    pub use std::format;
    pub use std::ops;
    pub use std::result::Result;
    pub use std::str;
    pub use std::string::{String, ToString};
    pub use std::sync::Arc;
    pub use std::vec::Vec;
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
    pub use hashbrown::{HashMap, HashSet};
}

pub use imp::*;
```

(Difference from current: adds `HashSet` to both branches. `HashMap` still backed by `std::collections::HashMap` under `std`.)

- [ ] **Step 2: Add `pub mod collections` in `lib.rs`**

Open `crates/mcapable-core/src/lib.rs`. Find the `mod support;` line (around line 53). Insert immediately after it:

```rust
/// Hash-map / hash-set type aliases used throughout the workspace.
///
/// These exist so consumers can import `HashMap`/`HashSet` from a single
/// location instead of pulling them in directly from `std::collections`.
/// The underlying type is backed by `rapidhash` for fast non-cryptographic
/// hashing.
pub mod collections {
    pub use crate::support::{HashMap, HashSet};
}
```

- [ ] **Step 3: Verify build and tests still pass**

Run: `just check`
Expected: clean.
Run: `cargo nextest run --workspace`
Expected: all tests pass (no behavior change yet).

- [ ] **Step 4: Commit**

```bash
git add crates/mcapable-core/src/support.rs crates/mcapable-core/src/lib.rs
git commit -m "feat(core): add public collections module re-exporting HashMap/HashSet"
```

---

## Task 3: Migrate remaining `mcapable-core/src` imports onto `crate::support`

A handful of files in `mcapable-core/src` still import directly from `std::collections`. Switch them to the internal alias path now (most of `mcapable-core/src` already uses `crate::support`).

**Files:**
- Modify: `crates/mcapable-core/src/stream/schema_parser/idl/mod.rs`
- Modify: `crates/mcapable-core/src/stream/schema_parser/ros/ros1.rs`
- Modify: `crates/mcapable-core/src/stream/parsed/defaults.rs`
- Modify: `crates/mcapable-core/src/reader.rs` (doctest only)

- [ ] **Step 1: Update `idl/mod.rs`**

In `crates/mcapable-core/src/stream/schema_parser/idl/mod.rs:5`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use crate::support::HashMap;
```

- [ ] **Step 2: Update `ros1.rs`**

In `crates/mcapable-core/src/stream/schema_parser/ros/ros1.rs:5`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use crate::support::HashMap;
```

- [ ] **Step 3: Update `stream/parsed/defaults.rs` (mixed import)**

In `crates/mcapable-core/src/stream/parsed/defaults.rs:5`, replace:

```rust
use std::collections::{HashMap, hash_map::Entry};
```

with:

```rust
use std::collections::hash_map::Entry;
use crate::support::HashMap;
```

`hash_map::Entry` from `std` continues to work because `RapidHashMap` is `std::collections::HashMap` with a custom hasher.

- [ ] **Step 4: Update the doctest in `reader.rs`**

In `crates/mcapable-core/src/reader.rs` around line 807, find the doctest line:

```rust
/// # use std::collections::HashMap;
```

Replace it with:

```rust
/// # use mcapable_core::collections::HashMap;
```

(Doctest examples represent the public API to consumers, so this uses the public re-export, not `crate::support`.)

- [ ] **Step 5: Verify**

Run: `just check`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/mcapable-core/src
git commit -m "refactor(core): route remaining src HashMap imports through support module"
```

---

## Task 4: Migrate `mcapable-core` test imports onto `mcapable_core::collections`

Tests sit outside the crate, so they use the public path.

**Files:**
- Modify: `crates/mcapable-core/tests/reader_api.rs`
- Modify: `crates/mcapable-core/tests/writer_properties.rs`
- Modify: `crates/mcapable-core/tests/writer_roundtrip.rs`
- Modify: `crates/mcapable-core/tests/rolling_writer.rs`

- [ ] **Step 1: Update `reader_api.rs`**

In `crates/mcapable-core/tests/reader_api.rs:7`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use mcapable_core::collections::HashMap;
```

- [ ] **Step 2: Update `writer_properties.rs`**

In `crates/mcapable-core/tests/writer_properties.rs:195`, replace:

```rust
        use std::collections::HashMap;
```

with:

```rust
        use mcapable_core::collections::HashMap;
```

- [ ] **Step 3: Update `writer_roundtrip.rs`**

In `crates/mcapable-core/tests/writer_roundtrip.rs:6`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use mcapable_core::collections::HashMap;
```

- [ ] **Step 4: Update `rolling_writer.rs` (inline use)**

In `crates/mcapable-core/tests/rolling_writer.rs:649`, replace:

```rust
    let mut meta_map = std::collections::HashMap::new();
```

with:

```rust
    let mut meta_map = mcapable_core::collections::HashMap::new();
```

(The `::new()` is fine for now because the alias still resolves to `std::collections::HashMap`. Task 8 fixes constructors workspace-wide.)

- [ ] **Step 5: Verify**

Run: `cargo nextest run -p mcapable-core`
Expected: all `mcapable-core` tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/mcapable-core/tests
git commit -m "refactor(core): route test HashMap imports through collections re-export"
```

---

## Task 5: Migrate `mcapable` crate tests/benches/helpers

The top-level `mcapable` crate has no `HashMap` use in `src/` itself, only in tests, helpers, and benches.

**Files:**
- Modify: `crates/mcapable/tests/roundtrip_equivalence.rs`
- Modify: `crates/mcapable/tests/comparison_tests.rs`
- Modify: `crates/mcapable/tests/helpers/mcap_builder.rs`
- Modify: `crates/mcapable/tests/helpers/generators.rs`
- Modify: `crates/mcapable/tests/conformance/metadata.rs`
- Modify: `crates/mcapable/benches/writer_vs_mcap.rs`

- [ ] **Step 1: Update `roundtrip_equivalence.rs`**

In `crates/mcapable/tests/roundtrip_equivalence.rs:8`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use mcapable_core::collections::HashMap;
```

- [ ] **Step 2: Update `comparison_tests.rs` (inline import)**

In `crates/mcapable/tests/comparison_tests.rs:213`, replace:

```rust
    use std::collections::HashMap;
```

with:

```rust
    use mcapable_core::collections::HashMap;
```

(Leave the unrelated `use std::collections::BTreeMap;` at line 7 alone.)

- [ ] **Step 3: Update `mcap_builder.rs` (mixed import)**

In `crates/mcapable/tests/helpers/mcap_builder.rs:7`, replace:

```rust
use std::collections::{BTreeMap, HashMap};
```

with:

```rust
use std::collections::BTreeMap;
use mcapable_core::collections::HashMap;
```

- [ ] **Step 4: Update `generators.rs`**

In `crates/mcapable/tests/helpers/generators.rs:10`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use mcapable_core::collections::HashMap;
```

- [ ] **Step 5: Update `conformance/metadata.rs` (inline `HashSet`)**

In `crates/mcapable/tests/conformance/metadata.rs:77`, replace:

```rust
    let distinct_channels: std::collections::HashSet<_> =
```

with:

```rust
    let distinct_channels: mcapable_core::collections::HashSet<_> =
```

- [ ] **Step 6: Update `writer_vs_mcap.rs` (mixed import)**

In `crates/mcapable/benches/writer_vs_mcap.rs:10`, replace:

```rust
use std::collections::{BTreeMap, HashMap};
```

with:

```rust
use std::collections::BTreeMap;
use mcapable_core::collections::HashMap;
```

- [ ] **Step 7: Verify**

Run: `cargo nextest run -p mcapable`
Expected: all `mcapable` tests pass.
Run: `cargo check -p mcapable --benches`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/mcapable
git commit -m "refactor(mcapable): route tests/benches HashMap imports through collections"
```

---

## Task 6: Migrate `mcapable-cli` source and tests

`mcapable-cli` has multiple command modules using `HashMap` (and a few `HashSet`).

**Files:**
- Modify: `crates/mcapable-cli/src/cli/cmd/du.rs`
- Modify: `crates/mcapable-cli/src/cli/cmd/cat.rs`
- Modify: `crates/mcapable-cli/src/cli/cmd/list.rs`
- Modify: `crates/mcapable-cli/src/cli/cmd/info.rs`
- Modify: `crates/mcapable-cli/src/cli/cmd/recover.rs`
- Modify: `crates/mcapable-cli/src/cli/cmd/merge.rs`
- Modify: `crates/mcapable-cli/src/cli/cmd/mod.rs`
- Modify: `crates/mcapable-cli/src/cli/cmd/filter.rs`
- Modify: `crates/mcapable-cli/tests/cmd_tests.rs`
- Modify: `crates/mcapable-cli/tests/cmd_properties.rs`

- [ ] **Step 1: Update single-import files**

For each of these files, replace the existing line:

```rust
use std::collections::HashMap;
```

with:

```rust
use mcapable_core::collections::HashMap;
```

Files: `du.rs:1`, `cat.rs:1`, `list.rs:2`, `info.rs:1`, `recover.rs:36`, `tests/cmd_tests.rs:12`, `tests/cmd_properties.rs:11`.

- [ ] **Step 2: Update mixed `HashMap, HashSet` imports**

For each of `merge.rs:1`, `mod.rs:1`, `filter.rs:1`, replace:

```rust
use std::collections::{HashMap, HashSet};
```

with:

```rust
use mcapable_core::collections::{HashMap, HashSet};
```

- [ ] **Step 3: Update inline `HashSet::new` in `du.rs`**

In `crates/mcapable-cli/src/cli/cmd/du.rs:157`, replace:

```rust
    let mut known_ops = std::collections::HashSet::new();
```

with (use the now-imported `HashSet` alias from Step 1, plus a typed default — pick the type already inferred at this site):

```rust
    let mut known_ops = HashSet::new();
```

If after Step 1 `HashSet` is not yet in scope in `du.rs` (the original import was `HashMap` only), update `du.rs:1` instead to:

```rust
use mcapable_core::collections::{HashMap, HashSet};
```

- [ ] **Step 4: Verify**

Run: `cargo nextest run -p mcapable-cli`
Expected: all CLI tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/mcapable-cli
git commit -m "refactor(cli): route HashMap/HashSet imports through mcapable_core::collections"
```

---

## Task 7: Migrate wrapper crates (`cpp`, `ffi`, `wasm`, `swift`)

These crates expose FFI / wasm bindings. Each has a single `use std::collections::HashMap` at the top of `src/lib.rs`.

**Files:**
- Modify: `crates/mcapable-cpp/src/lib.rs`
- Modify: `crates/mcapable-ffi/src/lib.rs`
- Modify: `crates/mcapable-wasm/src/lib.rs`
- Modify: `crates/mcapable-swift/src/lib.rs`

- [ ] **Step 1: Update `mcapable-cpp/src/lib.rs`**

In `crates/mcapable-cpp/src/lib.rs:11`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use mcapable_core::collections::HashMap;
```

- [ ] **Step 2: Update `mcapable-ffi/src/lib.rs`**

In `crates/mcapable-ffi/src/lib.rs:8`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use mcapable_core::collections::HashMap;
```

- [ ] **Step 3: Update `mcapable-wasm/src/lib.rs`**

In `crates/mcapable-wasm/src/lib.rs`, find the `&std::collections::HashMap<` reference at line 159 and replace it:

```rust
    metadata: &std::collections::HashMap<
```

with:

```rust
    metadata: &mcapable_core::collections::HashMap<
```

(`mcapable-wasm` references `std::collections::HashMap` inline, not via a top `use`. The change is at the use site.)

- [ ] **Step 4: Update `mcapable-swift/src/lib.rs`**

In `crates/mcapable-swift/src/lib.rs:12`, replace:

```rust
use std::collections::HashMap;
```

with:

```rust
use mcapable_core::collections::HashMap;
```

- [ ] **Step 5: Verify**

Run: `cargo check --workspace`
Expected: clean.
Run: `cargo nextest run --workspace`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/mcapable-cpp crates/mcapable-ffi crates/mcapable-wasm crates/mcapable-swift
git commit -m "refactor(wrappers): route HashMap imports through mcapable_core::collections"
```

---

## Task 8: Replace `::new()` constructors with `::default()` workspace-wide

`RapidHashMap`/`RapidHashSet` are `std::HashMap`/`HashSet` specialized with a custom hasher; they don't have an inherent `::new()` method, only `Default::default()`. Make this change *before* flipping the alias so each step independently builds.

`std::collections::HashMap::default()` exists and behaves identically to `::new()`, so this commit is a no-op semantically while the alias still points at std types.

- [ ] **Step 1: Audit current `::new()` usage**

Run:
```bash
grep -rn 'HashMap::new()\|HashSet::new()' --include='*.rs' .
```

Expected: ~98 hits across `mcapable-core/src`, `mcapable-core/tests`, `mcapable/tests`, `mcapable/benches`, `mcapable-cli/src`, `mcapable-cli/tests`, and the wrapper crates. Save the file list — every hit needs to be rewritten.

- [ ] **Step 2: Apply the find/replace**

For every line containing `HashMap::new()`, replace `HashMap::new()` with `HashMap::default()`. Same for `HashSet::new()` → `HashSet::default()`.

A safe scripted approach (run from the repo root):

```bash
git ls-files '*.rs' | xargs sed -i.bak -E 's/(Hash(Map|Set))::new\(\)/\1::default()/g'
find . -name '*.bak' -delete
```

(macOS BSD `sed` requires the `-i.bak` form; the `find` step removes the backups.)

If any sites still legitimately need `::new()` — there are none expected — they would only be on imported types that are not the rapidhash alias, in which case the grep audit in Step 1 would have shown a different type before the `::new`. None observed in this codebase.

- [ ] **Step 3: Verify**

Run: `grep -rn 'HashMap::new()\|HashSet::new()' --include='*.rs' .`
Expected: no hits.
Run: `just check`
Expected: clean.
Run: `cargo nextest run --workspace`
Expected: all tests pass.

- [ ] **Step 4: Run formatter and clippy**

Run: `just all`
Expected: passes (fmt, clippy, tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: prefer HashMap::default() over HashMap::new() ahead of rapidhash switch"
```

---

## Task 9: Flip `support.rs` aliases to `rapidhash`

This is the load-bearing commit — it changes the underlying hasher across the workspace.

**Files:**
- Modify: `crates/mcapable-core/src/support.rs`

- [ ] **Step 1: Replace `support.rs`**

Replace the entire content of `crates/mcapable-core/src/support.rs` with:

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

This is the *only* file in the workspace that mentions `rapidhash`.

- [ ] **Step 2: Verify the `std` build**

Run: `just check`
Expected: clean. If errors mention "no method named `new`" on a HashMap, Task 8 missed a site — go fix it and re-run.

- [ ] **Step 3: Verify the `no_std` build (if exercised in CI)**

Run:
```bash
cargo check -p mcapable-core --no-default-features --features alloc
```
Expected: clean. If this configuration isn't currently part of CI it may already be broken for unrelated reasons; in that case note the result but do not block on it.

- [ ] **Step 4: Run the full test suite**

Run: `cargo nextest run --workspace`
Expected: all tests pass.

- [ ] **Step 5: Run doctests (not covered by `cargo check`)**

Run: `cargo test --workspace --doc`
Expected: all doctests pass. Pay attention to the `reader.rs` doctest updated in Task 3.

- [ ] **Step 6: Run the full pre-commit gate**

Run: `just all`
Expected: fmt + clippy + tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/mcapable-core/src/support.rs
git commit -m "$(cat <<'EOF'
feat(core): switch HashMap/HashSet to rapidhash

mcapable_core::collections::HashMap is now rapidhash::RapidHashMap
(std::collections::HashMap<K, V, RapidRandomState>) and HashSet is
rapidhash::RapidHashSet. Public API change: types like Schema.metadata,
Channel.metadata, ChunkIndex.message_index_offsets, and the maps returned
by Reader::schemas / Reader::channels / Reader::all_metadata /
Reader::all_attachments now use the rapidhash random-state hasher.
Construct via Default::default() rather than HashMap::new().
EOF
)"
```

---

## Task 10: Final sweep and cleanup

A safety net in case the migration left a stray `std::collections::HashMap`/`HashSet` import or constructor somewhere unexpected.

- [ ] **Step 1: Grep for stragglers**

Run:
```bash
grep -rn 'std::collections::HashMap\|std::collections::HashSet\|std::collections::hash_map' --include='*.rs' .
```
Expected hits:
- `crates/mcapable-core/src/stream/parsed/defaults.rs` — only `std::collections::hash_map::Entry`, which is intentional.
- No other hits.

If any other file still references `std::collections::HashMap` or `HashSet`, route it through `mcapable_core::collections` (or `crate::support` if inside `mcapable-core`).

- [ ] **Step 2: Verify only one `use rapidhash` site exists**

Run:
```bash
grep -rn 'rapidhash' --include='*.rs' .
```
Expected: every hit lives in `crates/mcapable-core/src/support.rs`.
If any other file imports `rapidhash` directly, replace it with the alias path.

- [ ] **Step 3: Verify Cargo.toml dependency is only in core**

Run:
```bash
grep -rn 'rapidhash' --include='Cargo.toml' .
```
Expected: hits only in `crates/mcapable-core/Cargo.toml` and `Cargo.lock`.

- [ ] **Step 4: Run the full gate**

Run: `just all`
Expected: clean.
Run: `cargo nextest run --workspace`
Expected: clean.

- [ ] **Step 5: If Step 1 / 2 found stragglers, commit fixes**

```bash
git add -A
git commit -m "refactor: route final HashMap/HashSet stragglers through collections"
```

If no stragglers: this task ends without a commit.

---

## Done When

- `grep -rn 'std::collections::HashMap\|std::collections::HashSet' --include='*.rs' .` returns no hits except for intentional `hash_map::Entry` imports.
- `rapidhash` appears in source only inside `crates/mcapable-core/src/support.rs`.
- `rapidhash` appears in `Cargo.toml` only inside `crates/mcapable-core/Cargo.toml`.
- `just all` and `cargo nextest run --workspace` both succeed.
- Doctest at `crates/mcapable-core/src/reader.rs` (~ line 807) uses the new alias path.
