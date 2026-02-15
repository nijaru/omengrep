## Current State

| Metric    | Value                      | Updated    |
| --------- | -------------------------- | ---------- |
| Version   | 0.1.0 (Rust)               | 2026-02-14 |
| Model     | LateOn-Code-edge (48d/tok) | 2026-02-14 |
| omendb    | 0.0.27 (multi-vector)      | 2026-02-14 |
| Toolchain | nightly-2025-12-04         | 2026-02-14 |
| Tests     | 26 (12 unit + 14 integration) | 2026-02-14 |

## Architecture

```
Build:  Scan (ignore crate) -> Extract (tree-sitter, 25 langs) -> Split identifiers -> Embed (ort, LateOn-Code-edge INT8) -> Store (omendb multi-vector) + index_text (BM25 with split terms)
Search: Split query identifiers -> Embed query -> search_multi_with_text (BM25 candidates + MuVERA MaxSim rerank) -> Code-aware boost -> Results
```

## Benchmark (hygrep src, M3 Max, release)

| Metric       | Value                          |
| ------------ | ------------------------------ |
| Build        | 2.1s (23 files, 118 blocks)    |
| Search       | 110ms                          |
| Throughput   | ~56 blocks/s                   |

## Remaining Work

### Rename (tk-uwun)

- Evaluate rename to omgrep/omg — ties branding to omendb

### Distribution (tk-4f2n)

- Blocked on omendb crates.io publish (currently path dependency)
- crates.io + cargo-dist for binary releases

### Future: SPLADE Sparse Vectors

- Wait for omendb native sparse support
- Evaluate `ibm-granite/granite-embedding-30m-sparse` (30M, Apache 2.0, 50.8 nDCG)

## Completed

- BM25 code-aware tokenization (camelCase/snake_case splitting, +35% NDCG expected)
- Fixed double-lock bug in incremental update
- Fixed invalid lookaround regex in text.rs
- Fixed boost.rs camelCase splitting (was lowercasing before split — no-op)
- Correctness verification (all CLI features tested)
- Integration tests with assert_cmd (14 tests)
- CI workflow (build/test/clippy)
- Build profiling (no bottlenecks at current scale)

## omendb Requests

1. **`store_with_text()`** for multi-vector stores — filed in `omendb/cloud/multi-vector-text-indexing-bug.md`
2. **Custom tantivy tokenizer** in `TextSearchConfig` — for camelCase/snake_case splitting
3. **Native sparse vector support** — for future SPLADE integration

## Key Files

| File                   | Purpose                             |
| ---------------------- | ----------------------------------- |
| `src/cli/search.rs`    | Search command + file ref parsing   |
| `src/cli/build.rs`     | Build/update index                  |
| `src/embedder/onnx.rs` | ORT inference (LateOn-Code-edge)    |
| `src/extractor/mod.rs` | Tree-sitter extraction coordinator  |
| `src/index/mod.rs`     | SemanticIndex (omendb multi-vector) |
| `src/index/walker.rs`  | File walker (ignore crate)          |
| `src/tokenize.rs`      | BM25 identifier splitting           |
| `src/boost.rs`         | Code-aware ranking boosts           |
| `src/types.rs`         | Block, SearchResult, FileRef        |
| `tests/cli.rs`         | Integration tests (assert_cmd)      |
