## Current State

| Metric    | Value                             | Updated    |
| --------- | --------------------------------- | ---------- |
| Phase     | 11 (v2 Semantic on main)          | 2025-12-04 |
| Version   | 0.0.6 (PyPI)                      | 2025-12-04 |
| Branch    | main                              | 2025-12-04 |
| PyPI      | https://pypi.org/project/hygrep/  | 2025-12-04 |
| CLI       | `hhg` (primary), `hygrep` (alias) | 2025-12-04 |
| Languages | 22                                | 2025-12-03 |
| Perf      | ~20k files/sec (Mojo)             | 2025-12-02 |
| Inference | ~2s/100 candidates (CPU)          | 2025-12-02 |

## v2 Semantic Search (now on main)

Semantic-first code search with ModernBERT embeddings:

| Component              | Status | Notes                                 |
| ---------------------- | ------ | ------------------------------------- |
| embedder.py            | Done   | ModernBERT-embed-base INT8 (256 dims) |
| semantic.py            | Done   | SemanticIndex with walk-up discovery  |
| cli.py                 | Done   | Rich progress bars, spinner           |
| Walk-up index          | Done   | Reuses parent index from subdirs      |
| Relative paths         | Done   | Manifest v3, portable indexes         |
| Search scope filtering | Done   | Filter results to search directory    |
| Auto-index on query    | Done   | Builds index on first search          |
| Auto-update stale      | Done   | Incremental updates (hash-based)      |

**Index UX:**

- Walk-up discovery: `hhg "query" ./src/foo` finds index at `./` or `./src/`
- Relative paths in manifest: Index portable across machines
- Scope filtering: Results limited to search directory
- Progress: Spinner for scanning, progress bar for embedding

**Performance (M3 Max):**

| Phase       | Time   | Notes                             |
| ----------- | ------ | --------------------------------- |
| First index | ~34s   | 396 blocks, ModernBERT 512 tokens |
| Cold search | ~0.9s  | Model loading                     |
| Warm search | <1ms   | omendb vector search              |
| Auto-update | ~100ms | Per changed file                  |

**Key UX:**

- `hhg "query" ./src` - semantic search (default)
- `hhg "query" ./src/subdir` - uses parent index, filters results
- `hhg -f "query" ./src` - grep + rerank (no index)
- `hhg -e "pattern" ./src` - exact match (grep)
- `hhg -r "regex" ./src` - regex match

## Completed (Recent)

### CLI Consolidation (2025-12-04)

- Merged cli_v2 into cli.py (single clean module)
- Removed backwards compat code paths
- Simplified entry point

### Index UX Improvements (2025-12-04)

- Walk-up index discovery (git-style)
- Relative paths in manifest (v3 format)
- Search scope filtering for subdirectory searches
- Rich progress bar for embedding phase
- Spinner for scanning phase
- ModernBERT upgrade with performance fixes (MAX_LENGTH=512, BATCH_SIZE=16)

### Semantic Search Experiment (2025-12-04)

- Created embedder.py: ONNX text embeddings
- Created semantic.py: SemanticIndex class wrapping omendb
- Added CLI: `hhg status/rebuild/clean`
- Fixed extractor.extract() argument order bug
- Added SIMD literal fast path for non-regex patterns (~12% faster)

## Blockers

None.

## Known Issues

- Mojo native scanner requires MAX/Mojo runtime (wheels use Python fallback)
- omendb is optional dependency (not in main pyproject.toml yet)

## Next Steps

1. Test v2 on larger codebases, gather feedback
2. Add `hhg init-ci` command for CI workflow generation
3. Release v0.0.7 with semantic search

## Branch Status

| Branch | Purpose                  | Status |
| ------ | ------------------------ | ------ |
| main   | v2 semantic (unreleased) | Active |
