## Current State

| Metric    | Value                             | Updated    |
| --------- | --------------------------------- | ---------- |
| Phase     | 15 (Release 0.0.13)               | 2025-12-09 |
| Version   | 0.0.13 (pending release)          | 2025-12-09 |
| Branch    | main                              | 2025-12-09 |
| PyPI      | https://pypi.org/project/hygrep/  | 2025-12-05 |
| CLI       | `hhg` (primary), `hygrep` (alias) | 2025-12-05 |
| Languages | 22 + prose (md, txt, rst)         | 2025-12-09 |
| Perf      | ~20k files/sec (Mojo)             | 2025-12-02 |

## Architecture (Hybrid Search)

Hybrid search combining semantic (embeddings) + lexical (BM25) via omendb 0.0.8:

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
| **Prose chunking**     | Done   | Recursive splitting + header context        |

**Manifest v4 changes:**

- Text content stored with vectors for BM25 search
- Falls back to vector-only for older indexes
- Rebuild with `hhg build --force` to enable hybrid on existing indexes

**Prose chunking (markdown, txt, rst):**

- Recursive splitting: paragraph → line → sentence → word
- ~400 token chunks with ~50 token overlap (industry baseline)
- Character-based token estimation (~4 chars/token)
- Regex sentence detection (handles `.`, `!`, `?`)
- Header context injection: `Section > Subsection | content`
- Markdown code blocks extracted separately with language tag
- Works well for blog posts, research papers, documentation

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

1. Trigger release workflow for 0.0.13

## Branch Status

| Branch | Purpose          | Status |
| ------ | ---------------- | ------ |
| main   | v2 hybrid search | Active |
