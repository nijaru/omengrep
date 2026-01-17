## Current State

| Metric    | Value                         | Updated    |
| --------- | ----------------------------- | ---------- |
| Phase     | 30 (code quality refactor)    | 2026-01-17 |
| Version   | 0.0.29-dev                    | 2026-01-17 |
| Package   | `hhg` (renamed from `hygrep`) | 2025-12-16 |
| Branch    | main                          | 2025-12-16 |
| PyPI      | https://pypi.org/project/hhg/ | 2025-12-16 |
| CLI       | `hhg`                         | 2025-12-16 |
| Languages | 28 + prose (md, txt, rst)     | 2025-12-16 |
| Model     | snowflake-arctic-embed-s      | 2026-01-17 |
| omendb    | >=0.0.23                      | 2026-01-10 |

## Completed: Code Quality Refactor

Addressed all findings from parallel review/refactor/profile analysis.

### Changes

1. **Thread safety**: Module-level lock for MLX monkey-patching
2. **Type safety**: Added `EmbedderProtocol` with `@runtime_checkable`
3. **Circular import fix**: Extracted `_common.py` with shared constants
4. **Debug logging**: MLX import failures now logged (was silent)
5. **Constants**: Extracted `QUERY_CACHE_MAX_SIZE = 128`
6. **Method decomposition**: Split `_ensure_loaded()` into focused methods
7. **Normalization**: Fixed inconsistency in `MLXEmbedder._embed_one()`

### Benchmark Results

| Model                     | Backend  | Build Time | Relative     |
| ------------------------- | -------- | ---------- | ------------ |
| gte-modernbert-base (old) | ONNX CPU | 10.66s     | 1.43x slower |
| snowflake-arctic-embed-s  | MLX GPU  | 7.48s      | baseline     |

MLX is 2.57x faster than ONNX (667 vs 259 texts/sec).

## Architecture

```
Build:  Scan → Extract (parallel) → Embed (batched) → Store in omendb
Search: Embed query (with prefix) → Hybrid search (semantic + BM25) → Results

Backend selection (auto-detected):
  MLX (Metal GPU) - macOS Apple Silicon
  ONNX INT8 (CPU) - all other platforms

Module structure:
  _common.py      - Shared constants, evict_cache()
  embedder.py     - ONNX backend, EmbedderProtocol, get_embedder()
  mlx_embedder.py - MLX backend with CLS pooling
```

## Key Files

| File                         | Purpose                               |
| ---------------------------- | ------------------------------------- |
| `src/hygrep/_common.py`      | Shared constants and utilities        |
| `src/hygrep/embedder.py`     | ONNX embeddings, protocol, factory    |
| `src/hygrep/mlx_embedder.py` | MLX embeddings (Apple Silicon)        |
| `src/hygrep/cli.py`          | CLI, subcommand handling              |
| `src/hygrep/semantic.py`     | Index management, parallel extraction |

## Research & Reviews

- `ai/research/vidore-v3-embedding-analysis.md` - Model comparison
- `ai/review/mlx-embedder-profile-2026-01-17.md` - Performance analysis
