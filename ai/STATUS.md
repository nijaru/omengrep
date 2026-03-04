## Current State

| Metric    | Value                          | Updated    |
| --------- | ------------------------------ | ---------- |
| Package   | omengrep 0.0.1 (binary: og)    | 2026-02-24 |
| Models    | LateOn-Code-edge (48d, single) | 2026-02-16 |
| omendb    | 0.0.30 (multi-vector+compact)  | 2026-02-23 |
| Manifest  | v10 (mtime field)              | 2026-02-23 |
| Toolchain | nightly-2025-12-04             | 2026-02-14 |
| Tests     | 14 integration (26 total)      | 2026-02-23 |
| Boost     | Fixed (divide not multiply)    | 2026-03-03 |

## Architecture

```
Build:  Scan -> Extract (tree-sitter, nested dedup) -> Split identifiers (keyword filter) -> Embed (ort, LateOn-Code-edge) -> Store (omendb multi-vector compact) + index_text (BM25)
Search: scan_metadata (stat only) -> check_and_update (mtime pre-check, read only changed) -> Embed query -> BM25+semantic merge -> Code-aware boost -> Results
MCP:    og mcp (JSON-RPC/stdio) -> og_search, og_similar, og_status tools
```

## Active Work

None.

## Boost Fix (2026-03-03)

**Root bug**: `boost_results()` used `score *= boost` but omendb MaxSim scores are **negative** (less negative = more similar). Multiplying a negative score by a positive boost >1 makes it more negative = worse rank. Fix: `score /= boost` for negative scores.

**Also added**: Content-match boost for NL queries. For queries without camelCase/snake_case (NL docstrings), count how many query terms appear in the block content. Functions whose body contains the query vocabulary get up to 2x additional boost. This is key for docstring→function retrieval.

**Key finding**: `[Q]`/`[D]` ColBERT prefixes for LateOn-Code-edge are tokenized correctly (IDs 50368/50369) but hurt performance (2% vs 6% R@10). The model separation of query/doc spaces doesn't help for NL→code when scores are already near the negative boundary.

## Remaining Work

- **MCP** — deferred, CLI sufficient for now

## Benchmarks

Performance bench: `benches/omendb.rs` (divan)

| Benchmark       | Baseline (0.0.30) | Current  | delta |
| --------------- | ----------------- | -------- | ----- |
| search_hybrid   | 392.3 us          | 404.5 us | +3%   |
| search_semantic | 422.0 us          | 539.4 us | +28%  |
| store_write     | 5.25 ms           | 6.169 ms | +18%  |

Quality bench: `bench/quality.py` (CodeSearchNet, 2000 corpus seed=42)

| Run                  | Queries | MRR@10 | R@1  | R@5  | R@10 | Date       |
| -------------------- | ------- | ------ | ---- | ---- | ---- | ---------- |
| baseline             | 100     | 0.0082 | 0.00 | 0.00 | 0.08 | 2026-02-22 |
| after a2a0a02 bundle | 100     | 0.0062 | 0.00 | 0.00 | 0.06 | 2026-02-24 |
| boost fixed          | 100     | 0.0458 | 0.04 | 0.06 | 0.06 | 2026-03-03 |

**Key finding (2026-03-03):** The boost was broken — `score *= boost` on negative MaxSim
scores makes them more negative (worse rank). Fix: `score /= boost`. Added content-match
boost for NL queries. MRR improved 7.4x; first non-zero R@1 and R@5.

R@10 ceiling at 6% is a retrieval limit — the correct function is only in the omendb
top-10 candidates for ~6% of queries. The model (LateOn-Code-edge) is code-to-code
similarity optimized; NL→code retrieval quality is limited by the model.

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
