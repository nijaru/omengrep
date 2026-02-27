## Current State

| Metric    | Value                          | Updated    |
| --------- | ------------------------------ | ---------- |
| Package   | omengrep 0.0.1 (binary: og)    | 2026-02-24 |
| Models    | LateOn-Code-edge (48d, single) | 2026-02-16 |
| omendb    | 0.0.30 (multi-vector+compact)  | 2026-02-23 |
| Manifest  | v10 (mtime field)              | 2026-02-23 |
| Toolchain | nightly-2025-12-04             | 2026-02-14 |
| Tests     | 14 integration (26 total)      | 2026-02-23 |

## Architecture

```
Build:  Scan -> Extract (tree-sitter, nested dedup) -> Split identifiers (keyword filter) -> Embed (ort, LateOn-Code-edge) -> Store (omendb multi-vector compact) + index_text (BM25)
Search: scan_metadata (stat only) -> check_and_update (mtime pre-check, read only changed) -> Embed query -> BM25+semantic merge -> Code-aware boost -> Results
MCP:    og mcp (JSON-RPC/stdio) -> og_search, og_similar, og_status tools
```

## Active Work

None.

## Remaining Work

- **Quality re-bench** — rebuild index + run quality.py to measure extraction/BM25/boost impact
- **MCP testing** — deferred, CLI is sufficient for now

## Benchmarks

Performance bench: `benches/omendb.rs` (divan)

| Benchmark       | Baseline (0.0.30) | Current  | delta |
| --------------- | ----------------- | -------- | ----- |
| search_hybrid   | 392.3 us          | 404.5 us | +3%   |
| search_semantic | 422.0 us          | 539.4 us | +28%  |
| store_write     | 5.25 ms           | 6.169 ms | +18%  |

Quality bench: `bench/quality.py` (CodeSearchNet)

| Metric    | Before (2026-02-22) | After (2026-02-24) |
| --------- | ------------------- | ------------------ |
| MRR@10    | 0.0082              | 0.0062             |
| Recall@10 | 0.08                | 0.06               |

## Competitive Context

Primary competitor: **ColGrep** (LightOn, Feb 2026). Same model, same architecture.
Uses NextPlaid (PLAID) vs omendb (MuVERA). ColGrep never published MRR/recall numbers.

See `ai/research/benchmark-methodology.md` for full competitive analysis.

## omendb Notes (user is maintainer)

1. **Custom tantivy tokenizer** — for camelCase/snake_case splitting in BM25
2. **Native sparse vector support** — for future SPLADE integration

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
