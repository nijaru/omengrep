## Current State

| Metric    | Value                             | Updated    |
| --------- | --------------------------------- | ---------- |
| Phase     | 10 (Semantic Experiment)          | 2025-12-04 |
| Version   | 0.0.6 (PyPI)                      | 2025-12-04 |
| Branch    | experiment/semantic-search        | 2025-12-04 |
| PyPI      | https://pypi.org/project/hygrep/  | 2025-12-04 |
| CLI       | `hhg` (primary), `hygrep` (alias) | 2025-12-03 |
| Languages | 22                                | 2025-12-03 |
| Perf      | ~20k files/sec (Mojo)             | 2025-12-02 |
| Inference | ~2s/100 candidates (CPU)          | 2025-12-02 |

## Active Work

### v2 MVP Complete (experiment/semantic-search branch)

Semantic-first code search with auto-index/update:

| Component            | Status  | Notes                            |
| -------------------- | ------- | -------------------------------- |
| cli_v2.py            | ✅ Done | Semantic-first CLI               |
| Auto-index on query  | ✅ Done | Builds index on first search     |
| Auto-update stale    | ✅ Done | Incremental updates (hash-based) |
| -e/-r escape hatches | ✅ Done | Exact/regex fallback to grep     |
| embedder.py          | ✅ Done | ONNX all-MiniLM-L6-v2 (384 dims) |
| semantic.py          | ✅ Done | SemanticIndex using omendb       |

**Performance:**

| Phase       | Time        | Notes                     |
| ----------- | ----------- | ------------------------- |
| First index | ~5-6s       | 8k blocks, batch_size=128 |
| Cold search | ~5.8s       | Model loading             |
| Warm search | <1ms        | omendb vector search      |
| Auto-update | ~100ms/file | Incremental               |

**Key UX:**

- `hhg "query" ./src` - semantic search (default)
- `hhg -e "pattern" ./src` - exact match (grep)
- `hhg -r "regex" ./src` - regex match
- Auto-builds index on first query
- Auto-updates when files change

See `ai/DESIGN-v2.md` for full design.

## Completed (Recent)

### Semantic Search Experiment (2025-12-04)

- Created embedder.py: ONNX text embeddings using all-MiniLM-L6-v2
- Created semantic.py: SemanticIndex class wrapping omendb vector DB
- Added CLI: `hhg index build/status/clear/search`
- Fixed extractor.extract() argument order bug
- Fixed sys import shadowing in cli.py
- Added SIMD literal fast path for non-regex patterns (~12% faster)
- Clarified docs: hhg is grep+rerank, not semantic search (v1)
- Research: hybrid search, RRF algorithm, CLI UX best practices

### v0.0.6 Release (2025-12-04)

- Add `end_line` to JSON output for editor integration
- Add `-l`/`--files-only` option (list unique file paths)
- Add `--compact` option (JSON without content)
- Add syntax highlighting for code context (40+ languages)
- Modernize CLI with Typer + Rich (visible subcommands, examples panel)

## Blockers

None.

## Known Issues

- Mojo native scanner requires MAX/Mojo runtime (wheels use Python fallback)
- omendb is optional dependency (not in main pyproject.toml yet)

## Next Steps

1. Test v2 on real codebase, gather feedback
2. Merge experiment/semantic-search to main
3. Update entry point to use cli_v2 (or promote hhg v2)

## Branch Status

| Branch                     | Purpose            | Status |
| -------------------------- | ------------------ | ------ |
| main                       | v0.0.6 release     | Stable |
| experiment/semantic-search | v2 semantic design | Active |
