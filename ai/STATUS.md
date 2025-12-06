## Current State

| Metric    | Value                             | Updated    |
| --------- | --------------------------------- | ---------- |
| Phase     | 12 (v2 Semantic stable)           | 2025-12-05 |
| Version   | 0.0.12 (PyPI)                     | 2025-12-05 |
| Branch    | main                              | 2025-12-05 |
| PyPI      | https://pypi.org/project/hygrep/  | 2025-12-05 |
| CLI       | `hhg` (primary), `hygrep` (alias) | 2025-12-05 |
| Languages | 22                                | 2025-12-03 |
| Perf      | ~20k files/sec (Mojo)             | 2025-12-02 |
| Inference | ~2s/100 candidates (CPU)          | 2025-12-02 |

## Semantic Search (on main)

Semantic-first code search with ModernBERT embeddings:

| Component              | Status | Notes                                 |
| ---------------------- | ------ | ------------------------------------- |
| embedder.py            | Done   | ModernBERT-embed-base INT8 (256 dims) |
| semantic.py            | Done   | SemanticIndex with walk-up discovery  |
| cli.py                 | Done   | Rich spinners, progress bars          |
| Walk-up index          | Done   | Reuses parent index from subdirs      |
| Relative paths         | Done   | Manifest v3, portable indexes         |
| Search scope filtering | Done   | Filter results to search directory    |
| Auto-update stale      | Done   | Incremental updates (hash-based)      |
| Explicit build         | Done   | Requires `hhg build` before search    |

**Index UX:**

- Requires explicit `hhg build` before semantic search
- Set `HHG_AUTO_BUILD=1` to auto-build on first search
- Walk-up discovery: `hhg "query" ./src/foo` finds index at `./` or `./src/`
- Stale auto-update: Changed files updated automatically during search
- Progress: Spinner for scanning, progress bar for embedding
- Tip: Use `-f` for fast mode without building index

**Performance (M3 Max):**

| Phase       | Time   | Notes                             |
| ----------- | ------ | --------------------------------- |
| First index | ~34s   | 396 blocks, ModernBERT 512 tokens |
| Cold search | ~0.9s  | Model loading                     |
| Warm search | <1ms   | omendb vector search              |
| Auto-update | ~100ms | Per changed file                  |

**Key UX:**

- `hhg build ./src` - build index (required first)
- `hhg "query" ./src` - semantic search (requires index)
- `hhg "query" ./src/subdir` - uses parent index, filters results
- `hhg -f "query" ./src` - grep + rerank (no index needed)
- `hhg -e "pattern" ./src` - exact match (grep)
- `hhg -r "regex" ./src` - regex match

## Blockers

None.

## Known Issues

- Mojo native scanner requires MAX/Mojo runtime (wheels use Python fallback)

## Next Steps

1. Wait for seerdb/omendb releases, update deps
2. Tag new release after dep updates
3. Test on larger codebases, gather feedback

## Branch Status

| Branch | Purpose            | Status |
| ------ | ------------------ | ------ |
| main   | v2 semantic stable | Active |
