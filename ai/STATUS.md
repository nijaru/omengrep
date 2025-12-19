## Current State

| Metric    | Value                         | Updated    |
| --------- | ----------------------------- | ---------- |
| Phase     | 21 (omendb update)            | 2025-12-19 |
| Version   | 0.0.21 (PyPI)                 | 2025-12-19 |
| Package   | `hhg` (renamed from `hygrep`) | 2025-12-16 |
| Branch    | main                          | 2025-12-16 |
| PyPI      | https://pypi.org/project/hhg/ | 2025-12-16 |
| CLI       | `hhg`                         | 2025-12-16 |
| Languages | 28 + prose (md, txt, rst)     | 2025-12-16 |
| Model     | jina-code-int8 (768 dims)     | 2025-12-16 |

## v0.0.17 Changes

- **Package renamed** `hygrep` → `hhg` (clearer: `install hhg → hhg`)
- **Embedding model** switched to jina-code-int8 (768 dims, ~154MB, better quality)
- **Parallel extraction** - multiprocessing for faster builds
- **`hhg model`** - shows model and provider status
- **Manifest v5** - requires rebuild from v4 (dimension change)

## v0.0.18 Changes

- Fix CLI arg ordering (`--exclude` now works after positional args)
- Add `--code-only` flag to exclude docs (md, txt, rst, adoc)
- Add 6 new tree-sitter grammars: HTML, CSS, SQL, Julia, HCL/Terraform (28 total)
- Remove `doctor` command (redundant with `hhg model`)
- Simplify embedder to CPU-only (GPU providers not stable)

## v0.0.19 Changes

- Clean up build output: reorder Found → Merged → Skipped → Cleaned up → Indexed
- Fix singular grammar ("1 result" not "1 results")
- Quiet mode (`-q`) suppresses "Running:" prefix
- Fix Mojo and Python warnings (utcnow deprecation, type hints)

## v0.0.21 Changes

- Update omendb to 0.0.12 (bug fixes, subscores API)
- Fix type annotations to match omendb type stubs
- Remove unneeded ty ignore rule (omendb now has stubs)

## v0.0.20 Changes

- Add code-aware ranking boosts:
  - CamelCase/snake_case aware term matching
  - Exact name match: 2.5x, term overlap: +30% per term
  - Context-aware type boost: 1.5x if query mentions "class"/"function"
  - File path relevance: 1.15x
  - Boost cap at 4x to prevent over-boosting

## Planned

- **Optional cross-encoder reranking** (`--rerank` flag)
  - Model: jinaai/jina-reranker-v1-tiny-en (33MB)
  - Rerank top 20-30 results
  - ~50-80ms overhead
  - Off by default

- **RRF tuning** (omendb 0.0.12 exposes subscores - can use for custom weighting)

## Open Issues

None currently tracked.

## Architecture

```
Build:  Scan → Extract (parallel) → Embed (batched) → Store in omendb
Search: Embed query → Hybrid search (semantic + BM25) → Results
```

## Key Files

| File                        | Purpose                               |
| --------------------------- | ------------------------------------- |
| `src/hygrep/cli.py`         | CLI, subcommand handling              |
| `src/hygrep/embedder.py`    | ONNX embeddings, provider detection   |
| `src/hygrep/semantic.py`    | Index management, parallel extraction |
| `src/hygrep/extractor.py`   | Tree-sitter code extraction           |
| `src/scanner/_scanner.mojo` | Fast file scanning (Mojo)             |

## Performance (M3 Max, CPU)

| Phase       | Time  | Notes                 |
| ----------- | ----- | --------------------- |
| First index | ~31s  | 530 blocks, jina 768d |
| Cold search | ~0.9s | Model loading         |
| Warm search | <1ms  | omendb hybrid search  |
