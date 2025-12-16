## Current State

| Metric    | Value                         | Updated    |
| --------- | ----------------------------- | ---------- |
| Phase     | 17 (Post-rename)              | 2025-12-16 |
| Version   | 0.0.17 (PyPI)                 | 2025-12-16 |
| Package   | `hhg` (renamed from `hygrep`) | 2025-12-16 |
| Branch    | main                          | 2025-12-16 |
| PyPI      | https://pypi.org/project/hhg/ | 2025-12-16 |
| CLI       | `hhg`                         | 2025-12-16 |
| Languages | 22 + prose (md, txt, rst)     | 2025-12-09 |
| Model     | jina-code-int8 (768 dims)     | 2025-12-16 |

## v0.0.17 Changes

- **Package renamed** `hygrep` → `hhg` (clearer: `install hhg → hhg`)
- **Embedding model** switched to jina-code-int8 (768 dims, ~154MB, better quality)
- **GPU auto-detection** - CUDA/CoreML/CPU with tuned batch sizes (256/128/32)
- **Parallel extraction** - multiprocessing for ~3x faster builds (~31s → ~10s potential)
- **`hhg doctor`** - setup diagnostics, GPU suggestions
- **`hhg model`** - shows active provider and batch size
- **Manifest v5** - requires rebuild from v4 (dimension change)

## Uncommitted Changes

- Remove `hygrep` CLI entry point (only `hhg` needed)

## Open Issues

### High Priority

1. **CLI arg ordering** - `--exclude` must come before positional args
   - `hhg "query" . --exclude "*.md"` fails
   - `hhg --exclude "*.md" "query" .` works
   - Root cause: typer positional args + subcommand pattern

2. **`--code-only` flag** - filter out docs/markdown automatically

### Medium Priority

3. **More tree-sitter grammars** - evaluate Scala, Haskell, OCaml, R, Julia, etc.

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

## GPU Acceleration

| Platform      | Package                 | Batch Size |
| ------------- | ----------------------- | ---------- |
| NVIDIA        | `onnxruntime-gpu`       | 256        |
| Apple Silicon | `onnxruntime-silicon`   | 128        |
| CPU           | `onnxruntime` (default) | 32         |

Run `hhg doctor` for suggestions.

## Performance (M3 Max, CPU)

| Phase       | Time  | Notes                 |
| ----------- | ----- | --------------------- |
| First index | ~31s  | 530 blocks, jina 768d |
| Cold search | ~0.9s | Model loading         |
| Warm search | <1ms  | omendb hybrid search  |
