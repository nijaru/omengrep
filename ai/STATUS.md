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

### Semantic Search v2 Design (experiment/semantic-search branch)

Reimagining hhg as semantic-first code search:

| Component            | Status  | Notes                            |
| -------------------- | ------- | -------------------------------- |
| embedder.py          | ✅ Done | ONNX all-MiniLM-L6-v2 (384 dims) |
| semantic.py          | ✅ Done | SemanticIndex using omendb       |
| CLI index commands   | ✅ Done | build, status, clear, search     |
| DESIGN-v2.md         | ✅ Done | New architecture spec            |
| Auto-index on query  | ❌ TODO | Core v2 feature                  |
| Auto-update stale    | ❌ TODO | Incremental updates              |
| -e/-r escape hatches | ❌ TODO | Exact/regex fallback             |

**Key decisions:**

- Semantic search is the default (not opt-in)
- Auto-index on first query (no `index build` command)
- Auto-update when stale (incremental)
- `-e` (exact) and `-r` (regex) for escape hatches
- Drop: --fast, --hybrid, cross-encoder reranking

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

1. Implement v2 design (auto-index, auto-update, clean UX)
2. Drop cross-encoder reranking (embeddings sufficient)
3. Add -e/-r escape hatches for exact/regex search

## Branch Status

| Branch                     | Purpose            | Status |
| -------------------------- | ------------------ | ------ |
| main                       | v0.0.6 release     | Stable |
| experiment/semantic-search | v2 semantic design | Active |
