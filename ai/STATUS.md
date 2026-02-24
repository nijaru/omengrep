## Current State

| Metric    | Value                          | Updated    |
| --------- | ------------------------------ | ---------- |
| Package   | omengrep 0.0.1 (binary: og)    | 2026-02-16 |
| Models    | LateOn-Code-edge (48d, single) | 2026-02-16 |
| omendb    | 0.0.30 (multi-vector+compact)  | 2026-02-23 |
| Manifest  | v10 (mtime field added)        | 2026-02-23 |
| Toolchain | nightly-2025-12-04             | 2026-02-14 |
| Tests     | 14 integration (26 total)      | 2026-02-23 |

## Architecture

```
Build:  Scan -> Extract (tree-sitter, nested dedup) -> Split identifiers (keyword filter) -> Embed (ort, LateOn-Code-edge) -> Store (omendb multi-vector compact) + index_text (BM25)
Search: scan_metadata (stat only) -> check_and_update (mtime pre-check, read only changed) -> Embed query -> BM25+semantic merge -> Code-aware boost -> Results
MCP:    og mcp (JSON-RPC/stdio) -> og_search, og_similar, og_status tools
```

## Recent Changes (2026-02-23) — Optimization Sprint

### Search Performance

- **mtime fast path**: manifest v10 stores file mtime; search uses `scan_metadata()` (stat-only walk) + `check_and_update()` that loads manifest once and only reads content for files with changed mtime
- **No redundant manifest loads**: `get_stale_files_with_manifest()` takes `&Manifest` param, caller controls load lifecycle
- Unchanged codebase: search startup ~0.13s total (was reading all file contents)

### Search Quality

- **Nested block dedup**: parent blocks (class containing methods) removed when children cover the range
- **Python**: `decorated_definition` captured for @decorator'd functions
- **Elixir**: only captures def/defp/defmacro/defmodule, not every call
- **YAML/JSON**: fallback_head instead of noisy per-pair tree-sitter extraction
- **TOML**: only captures `(table)`, dropped individual `(pair)`
- **BM25 stop-words**: filters language keywords (pub, fn, let, def, class, etc.) from split identifiers
- **Boost tuning**: function/method 1.3x > class/struct 1.2x (was reversed), SHORT_WHITELIST expanded
- **find_similar**: now applies boost_results using reference name

### Build Performance

- **No block.clone()**: PreparedBlock stores `(file_idx, block_idx)` indices into all_blocks
- **Extractor reuse**: rayon `map_init()` reuses tree-sitter parsers per thread
- **Lazy regexes**: markdown fence_re/header_re compiled once via LazyLock

### Skipped

- Phase 4a (Mutex removal on ONNX session): `ort::Session::run` requires `&mut self`, Mutex is correct for `&self` trait + Send+Sync. Uncontended lock cost is negligible.

## Active Work

### Publish to crates.io (tk-4f2n)

- Blocked on omendb crates.io publish (user is omendb maintainer)
- omendb 0.0.30 in use (write regression fixed)
- Release pipeline ready: `.github/workflows/release.yml`
- Homebrew formula ready: `nijaru/homebrew-tap/Formula/og.rb`
- Tag `v0.1.0` when unblocked

## Benchmarks

Performance bench: `benches/omendb.rs` (divan)

| Benchmark       | 0.0.28 median | 0.0.30 median | delta   |
| --------------- | ------------- | ------------- | ------- |
| search_hybrid   | 395.8 µs      | 392.3 µs      | -1%     |
| search_semantic | 454.8 µs      | 422.0 µs      | **-7%** |
| store_write     | 5.49 ms       | 5.25 ms       | -4%     |

Quality bench: `bench/quality.py` (CodeSearchNet)

| Metric    | Before (2026-02-22) | After (TBD) |
| --------- | ------------------- | ----------- |
| MRR@10    | 0.0082              | —           |
| Recall@10 | 0.08                | —           |

## Remaining Work

- **Quality re-bench** — rebuild index + run quality.py to measure extraction/BM25/boost impact
- **Rebuild indexes** — manifest v10 forces rebuild on first use (expected)
- **MCP testing** — deferred, CLI is sufficient for now

## Competitive Context

Primary competitor: **ColGrep** (LightOn, Feb 2026). Same model (LateOn-Code-edge), same architecture.
Uses NextPlaid (PLAID) vs omendb (MuVERA). ColGrep never published MRR/recall numbers — gap we can fill.

See `ai/research/benchmark-methodology.md` for full competitive analysis.

## omendb Notes (user is maintainer)

1. **Write regression** in 0.0.29 — fixed in 0.0.30 (cache counter in memory)
2. **Custom tantivy tokenizer** — for camelCase/snake_case splitting in BM25
3. **Native sparse vector support** — for future SPLADE integration

## Key Files

| File                    | Purpose                             |
| ----------------------- | ----------------------------------- |
| `src/cli/search.rs`     | SearchParams, search + file refs    |
| `src/cli/build.rs`      | Build/update index (shared helper)  |
| `src/index/mod.rs`      | SemanticIndex (omendb multi-vector) |
| `src/index/manifest.rs` | Manifest v10 (mtime field)          |
| `src/index/walker.rs`   | scan + scan_metadata (stat-only)    |
| `src/embedder/onnx.rs`  | ORT inference (query vs doc paths)  |
| `src/tokenize.rs`       | BM25 identifier splitting           |
| `src/boost.rs`          | Code-aware ranking boosts           |
| `src/extractor/mod.rs`  | Extraction + nested block dedup     |
| `benches/omendb.rs`     | Performance benchmark (divan)       |
| `tests/cli.rs`          | Integration tests (assert_cmd)      |
