## Current State

| Metric    | Value                         | Updated    |
| --------- | ----------------------------- | ---------- |
| Phase     | 29 (snowflake model upgrade)  | 2026-01-17 |
| Version   | 0.0.29-dev                    | 2026-01-17 |
| Package   | `hhg` (renamed from `hygrep`) | 2025-12-16 |
| Branch    | main                          | 2025-12-16 |
| PyPI      | https://pypi.org/project/hhg/ | 2025-12-16 |
| CLI       | `hhg`                         | 2025-12-16 |
| Languages | 28 + prose (md, txt, rst)     | 2025-12-16 |
| Model     | snowflake-arctic-embed-s      | 2026-01-17 |
| omendb    | >=0.0.23                      | 2026-01-10 |

## Completed: Snowflake Model Upgrade

Switched from gte-modernbert-base (150M, 768 dims) to snowflake-arctic-embed-s (33M, 384 dims) based on ViDoRe V3 benchmark analysis.

### Performance Improvements

| Metric            | Before (gte-modernbert) | After (snowflake) | Change       |
| ----------------- | ----------------------- | ----------------- | ------------ |
| Model size (INT8) | 150MB                   | 34MB              | 4.4x smaller |
| Parameters        | 150M                    | 33M               | 4.5x fewer   |
| Vector dims       | 768                     | 384               | 2x smaller   |
| Batch size        | 32                      | 64                | 2x larger    |

### Key Changes

1. **Model**: snowflake-arctic-embed-s (competitive with 100M+ models on ViDoRe V3)
2. **Pooling**: CLS token (was mean pooling)
3. **Query prefix**: Added for optimal retrieval
4. **Manifest**: Version 7, requires rebuild from v6
5. **MLX**: Temporarily disabled (pooling incompatibility)

### Breaking Change

Model switch requires index rebuild:

- Old indexes (v6) prompt: "Rebuild with: hhg build --force"
- CLI handles gracefully with "Rebuild now? [Y/n]" prompt

## Pending

- Re-enable MLX with CLS pooling implementation (tk-7wfk)
- Benchmark actual build time improvement on real codebase

## Architecture

```
Build:  Scan → Extract (parallel) → Embed (batched) → Store in omendb
Search: Embed query (with prefix) → Hybrid search (semantic + BM25) → Results

Backend selection (current):
  ONNX INT8 (CPU) - all platforms
  MLX disabled pending CLS pooling fix
```

## Key Files

| File                         | Purpose                               |
| ---------------------------- | ------------------------------------- |
| `src/hygrep/cli.py`          | CLI, subcommand handling              |
| `src/hygrep/embedder.py`     | ONNX embeddings, CLS pooling          |
| `src/hygrep/mlx_embedder.py` | MLX embeddings (disabled)             |
| `src/hygrep/semantic.py`     | Index management, parallel extraction |
| `src/hygrep/extractor.py`    | Tree-sitter code extraction           |
| `src/scanner/_scanner.mojo`  | Fast file scanning (Mojo)             |

## Research

- `ai/research/vidore-v3-embedding-analysis.md` - Model comparison and ColBERT analysis
