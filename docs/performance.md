# Performance Notes

This document captures performance improvement approaches discovered during CLI profiling so
future implementors can keep large-file workflows fast.

## Profiling setup

- Use the `profiling` Cargo profile for debug symbols:
  `cargo build --profile profiling --bin mcapable`
- Prefer `tools/cli_profile.py` with the profiling binary for comparing `mcapable` vs `mcap`.

## High-impact improvement patterns

- Avoid full-file scans when summary data exists; prefer summary indexes and metadata.
- Eliminate eager "preload" passes; use lazy on-demand metadata access that first checks summary
  caches and only falls back to filtered record iteration when summary is missing.
- When listing records by index, avoid scanning the data section if a summary is present but the
  corresponding index list is empty (match `mcap` behavior and skip the expensive fallback).
- When a full scan is unavoidable, always filter by opcode to avoid parsing/decompression work
  for unrelated records.
- Prefer Record-level iteration when only record headers/payloads are needed; avoid chunk
  decompression unless required.
- Minimize duplicate passes over the data section; combine multiple needs into a single filtered
  iteration when possible.
- For list commands that need message index lengths, precompute lengths via a single sequential
  scan of the summary section instead of per-offset seeks.

## Known slow CLI paths

- `recover`: still worth benchmarking when reindexing chunked files; ensure summary/index
  generation stays close to `mcap` on large files.
