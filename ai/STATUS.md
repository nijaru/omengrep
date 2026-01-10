## Current State

| Metric    | Value                         | Updated    |
| --------- | ----------------------------- | ---------- |
| Phase     | 27 (MLX acceleration)         | 2026-01-10 |
| Version   | 0.0.28                        | 2026-01-10 |
| Package   | `hhg` (renamed from `hygrep`) | 2025-12-16 |
| Branch    | main                          | 2025-12-16 |
| PyPI      | https://pypi.org/project/hhg/ | 2025-12-16 |
| CLI       | `hhg`                         | 2025-12-16 |
| Languages | 28 + prose (md, txt, rst)     | 2025-12-16 |
| Model     | jina-code (MLX/FP16/INT8)     | 2026-01-10 |
| omendb    | >=0.0.23                      | 2026-01-10 |

## Current Work: MLX Acceleration

Adding MLX backend for Apple Silicon to get ~50x speedup over CPU.

### Target Architecture

| Platform      | Backend               | Model Source                          | Expected Speed   |
| ------------- | --------------------- | ------------------------------------- | ---------------- |
| Apple Silicon | MLX (custom JinaBERT) | jina-code safetensors                 | ~2000+ texts/sec |
| Linux + CUDA  | ONNX FP16             | `jinaai/jina-embeddings-v2-base-code` | ~500+ texts/sec  |
| Linux + ROCm  | ONNX FP16             | Same as CUDA                          | TBD              |
| CPU fallback  | ONNX INT8             | `nijaru/jina-code-int8`               | ~230 texts/sec   |

### Why Custom JinaBERT?

jina-code uses JinaBERT architecture (not standard BERT):

- ALiBi positional encoding (enables 8K context)
- Gated MLPs (GLU-style)
- LayerNorm on Q and K in attention
- Pre-norm architecture

Standard MLX BERT implementations don't support this. Need ~200 lines of custom code.

### Research Completed

- [BGE vs jina-code comparison](research/bge-vs-jina-code.md) - jina-code wins for code search
- [ONNX model format research](research/onnx-model-format-crossplatform.md) - FP16 for GPU, INT8 for CPU

## Tasks

See `tk ls` for current implementation tasks.

## Previous Versions

<details>
<summary>v0.0.28 - Multi-provider support</summary>

- Upgrade omendb to 0.0.23
- Progress bar for large builds (50+ files)
- Partial clean - `hhg clean ./subdir`
- Multi-provider ONNX detection (CUDA/CPU)
- Optional CUDA dependency - `pip install hhg[cuda]`
</details>

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

Backend selection (auto-detect):
  macOS + MLX available? → MLX with JinaBERT (Metal GPU)
  CUDAExecutionProvider? → ONNX FP16 (tensor cores)
  ROCmExecutionProvider? → ONNX FP16 (AMD GPU)
  Otherwise → ONNX INT8 (CPU optimized)
```

## Key Files

| File                         | Purpose                                   |
| ---------------------------- | ----------------------------------------- |
| `src/hygrep/cli.py`          | CLI, subcommand handling                  |
| `src/hygrep/embedder.py`     | ONNX embeddings, provider detection       |
| `src/hygrep/mlx_embedder.py` | MLX embeddings (to be created)            |
| `src/hygrep/mlx_jinabert.py` | JinaBERT MLX architecture (to be created) |
| `src/hygrep/semantic.py`     | Index management, parallel extraction     |
| `src/hygrep/extractor.py`    | Tree-sitter code extraction               |
| `src/scanner/_scanner.mojo`  | Fast file scanning (Mojo)                 |

## Open Issues

- AMD ROCm needs testing (should work with existing ONNX FP16)
