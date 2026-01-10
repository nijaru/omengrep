## Current State

| Metric    | Value                         | Updated    |
| --------- | ----------------------------- | ---------- |
| Phase     | 26 (multi-provider)           | 2026-01-10 |
| Version   | 0.0.28                        | 2026-01-10 |
| Package   | `hhg` (renamed from `hygrep`) | 2025-12-16 |
| Branch    | main                          | 2025-12-16 |
| PyPI      | https://pypi.org/project/hhg/ | 2025-12-16 |
| CLI       | `hhg`                         | 2025-12-16 |
| Languages | 28 + prose (md, txt, rst)     | 2025-12-16 |
| Model     | jina-code (INT8/FP16)         | 2026-01-10 |
| omendb    | >=0.0.23                      | 2026-01-10 |

## v0.0.28 Changes

- **Upgrade omendb** to 0.0.23 (bug fixes from 0.0.21-0.0.22)
- **Progress bar** for large builds (50+ files), spinner for small
- **Partial clean** - `hhg clean ./subdir` removes only that subdir from parent index
- **Multi-provider support** - auto-detect CUDA/CoreML/CPU
  - CUDA: Uses FP16 from `jinaai/jina-embeddings-v2-base-code`
  - CoreML: Would use FP16 (not available in pip packages yet)
  - CPU: Uses INT8 from `nijaru/jina-code-int8`
- **Optional CUDA dependency** - `pip install hhg[cuda]` for GPU acceleration

## Provider Status

| Platform     | Provider              | Model | Status                    |
| ------------ | --------------------- | ----- | ------------------------- |
| Linux + CUDA | CUDAExecutionProvider | FP16  | Works with `hhg[cuda]`    |
| macOS        | CPUExecutionProvider  | INT8  | Works (CoreML not in pip) |
| Linux CPU    | CPUExecutionProvider  | INT8  | Works                     |

## Uncommitted Work

Multi-provider embedder changes ready to commit:

- `embedder.py`: Auto-detect provider, select optimal model
- `cli.py`: Updated `hhg model` to show provider/model info
- `pyproject.toml`: Added `[cuda]` optional dependency

## Previous Versions

<details>
<summary>v0.0.25 - Similar search UX</summary>

- `hhg file.py#function_name` - find similar by block name
- `hhg file.py:42` - find similar by line number
- Exclude text/doc blocks from similar results
- Show similarity percentage
</details>

<details>
<summary>v0.0.17-0.0.24</summary>

- Package renamed `hygrep` → `hhg`
- jina-code-int8 model (768 dims)
- Parallel extraction, hybrid search
- Code-aware ranking boosts
</details>

## Architecture

```
Build:  Scan → Extract (parallel) → Embed (batched) → Store in omendb
Search: Embed query → Hybrid search (semantic + BM25) → Results

Provider selection:
  CUDA available? → FP16 from Jina + CUDAExecutionProvider
  CoreML available? → FP16 from Jina + CoreMLExecutionProvider
  Otherwise → INT8 from nijaru + CPUExecutionProvider
```

## Key Files

| File                        | Purpose                               |
| --------------------------- | ------------------------------------- |
| `src/hygrep/cli.py`         | CLI, subcommand handling              |
| `src/hygrep/embedder.py`    | ONNX embeddings, provider detection   |
| `src/hygrep/semantic.py`    | Index management, parallel extraction |
| `src/hygrep/extractor.py`   | Tree-sitter code extraction           |
| `src/scanner/_scanner.mojo` | Fast file scanning (Mojo)             |

## Open Issues

- CoreML not available via pip (third-party packages outdated)
- Mac builds CPU-only for now (~10 texts/sec)
