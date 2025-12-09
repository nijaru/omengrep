## Current State

| Metric    | Value                             | Updated    |
| --------- | --------------------------------- | ---------- |
| Phase     | 14 (Hybrid Search)                | 2025-12-09 |
| Version   | 0.0.12 (PyPI)                     | 2025-12-05 |
| Branch    | main                              | 2025-12-09 |
| PyPI      | https://pypi.org/project/hygrep/  | 2025-12-05 |
| CLI       | `hhg` (primary), `hygrep` (alias) | 2025-12-05 |
| Languages | 22                                | 2025-12-03 |
| Perf      | ~20k files/sec (Mojo)             | 2025-12-02 |

## Architecture (Hybrid Search)

Hybrid search combining semantic (embeddings) + lexical (BM25) via omendb 0.0.7:

| Component              | Status | Notes                                       |
| ---------------------- | ------ | ------------------------------------------- |
| embedder.py            | Done   | ModernBERT-embed-base INT8 (256 dims)       |
| semantic.py            | Done   | Hybrid search via omendb search_hybrid()    |
| cli.py                 | Done   | 4 commands: build, search, status, clean    |
| Walk-up index          | Done   | Reuses parent index from subdirs            |
| Relative paths         | Done   | Manifest v4, portable indexes               |
| Search scope filtering | Done   | Filter results to search directory          |
| Auto-update stale      | Done   | Incremental updates (hash-based)            |
| Explicit build         | Done   | Requires `hhg build` before search          |
| Index hierarchy        | Done   | Parent check, subdir merge, walk-up         |
| **Hybrid search**      | Done   | BM25 + vector via omendb, alpha=0.5 balance |

**Manifest v4 changes:**

- Text content stored with vectors for BM25 search
- Falls back to vector-only for older indexes
- Rebuild with `hhg build --force` to enable hybrid on existing indexes

**Commands:**

```
hhg build ./src       # Build index (required first)
hhg "query" ./src     # Hybrid search (semantic + BM25)
hhg status ./src      # Show index status
hhg clean ./src       # Delete index
```

**Performance (M3 Max):**

| Phase       | Time   | Notes                             |
| ----------- | ------ | --------------------------------- |
| First index | ~34s   | 396 blocks, ModernBERT 512 tokens |
| Cold search | ~0.9s  | Model loading                     |
| Warm search | <1ms   | omendb hybrid search              |
| Auto-update | ~100ms | Per changed file                  |

## Blockers

None.

## Known Issues

- Mojo native scanner requires MAX/Mojo runtime (wheels use Python fallback)

## Next Steps

1. Better text chunking for prose (recursive splitting + header context)
2. Update README/docs for hybrid positioning
3. Tag new release (0.0.13)

## Branch Status

| Branch | Purpose          | Status |
| ------ | ---------------- | ------ |
| main   | v2 hybrid search | Active |
