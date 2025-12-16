## Current State

| Metric    | Value                         | Updated    |
| --------- | ----------------------------- | ---------- |
| Phase     | 18 (CPU-only)                 | 2025-12-16 |
| Version   | 0.0.18 (PyPI)                 | 2025-12-16 |
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

## Unreleased

- Clean up build output: remove scan time, reorder Found → Merged → Skipped → Cleaned up → Indexed

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
