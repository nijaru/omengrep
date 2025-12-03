# Architectural Decisions

## 1. Language & Runtime
**Decision:** Mojo + ONNX Runtime
**Why:**
- **Mojo:** Native performance for systems code (Scanner).
- **ONNX Runtime:** Industry standard for inference. Using Python Interop for now (stability).

## 2. Core Architecture: "Hyper Hybrid" (Stateless)
**Decision:** Two-Stage Pipeline: Recall (Regex) -> Rerank (Semantic).
**Rationale:**
- **Statelessness:** No background daemons, no index maintenance. This is our key differentiator against `mgrep` and `morph`.
- **Recall:** Fast "dumb" regex scanning (Mojo) finds candidates (~100 files).
- **Rerank:** "Smart" Cross-Encoder (`mxbai`) scores them.

## 3. Context Strategy (Smart Context)
**Decision:** Tiered Extraction Strategy (Tree-sitter -> Fallback).
**Why:** Agents need logical blocks (functions), not just lines.
- **Tier 1 (Code):** Use Tree-sitter (in Python stage) to extract full functions/classes for candidates.
- **Tier 2 (Docs):** Sliding window (+/- 5 lines) for unsupported files.

## 4. Optimization Strategy
**Decision:** Parallelize IO, Native Regex
**Why:** Python overhead is acceptable for the *Reranker* (run on <100 items), but unacceptable for the *Scanner* (run on 50,000 items).
- **Scanner:** Pure Mojo/C (Parallel).
- **Reranker:** Python Interop (Vectorized) + Tree-sitter.

## 5. Parallel Implementation
**Decision:** `algorithm.parallelize` with `UnsafePointer` Mask.
**Why:**
- Mojo's `List` is not thread-safe for concurrent writes.
- Allocating a boolean mask (thread-safe writing by index) prevents locks/contention.

## 6. Distribution Strategy (2025-12-01, Updated)
**Decision:** Mojo as Python Extension Module
**Choice:** Python CLI + Mojo native extension → PyPI wheel

**Discovery:** Mojo supports `PythonModuleBuilder` for building native Python extensions:
```bash
mojo build scanner.mojo --emit shared-lib -o _scanner.so
python -c "from _scanner import scan; scan(...)"  # Native speed!
```

**Architecture:**
```
hygrep (Python CLI) → _scanner.so (Mojo extension) → reranker (Python/ONNX)
```

**Why this works:**
- Native call overhead (~0.01ms vs ~6ms subprocess)
- `pip install hygrep` / `uv tool install hygrep` just works
- Keep Mojo scanner code (no Rust rewrite)
- Python handles deps (onnxruntime, tree-sitter, etc.)

**Implementation:**
1. Refactor `walker.mojo` → `_scanner.mojo` (Python extension API)
2. Python CLI entry point imports `_scanner` directly
3. GitHub Actions builds platform wheels (macOS-arm64, linux-x64)
4. Publish to PyPI as `hygrep`

**Trade-offs:**
- Need Mojo SDK in CI (vs maturin for Rust)
- Manual wheel packaging (no maturin equivalent yet)
- Python startup overhead (~50ms) - acceptable for CLI

**Long-term:** When Mojo ecosystem matures, can go pure Mojo binary.

## 7. Performance Profiling (2025-12-02)

**Findings:**
| Phase | Time | Notes |
|-------|------|-------|
| Scan | ~2ms | Mojo parallel regex - blazing fast |
| Filter | ~0ms | Gitignore + exclude patterns |
| Rerank | ~2200ms | Tree-sitter extraction + ONNX inference |

**Bottleneck:** Rerank phase (98% of total time)
- Model loading: ~500ms first call (cached after)
- Tree-sitter extraction: ~200ms (100 files)
- ONNX inference: ~1500ms (100 candidates @ batch=32)

**Optimizations Applied:**
1. **Query caching** - Pre-compile tree-sitter queries per language (15% improvement)
2. **Parallel extraction** - ThreadPoolExecutor for tree-sitter parsing
3. **Batched inference** - BATCH_SIZE=32, ORT_ENABLE_ALL optimization
4. **max_candidates cap** - Default 100, prevents unbounded inference cost

**Future Options (not implemented):**
- GPU acceleration (onnxruntime-gpu) - 5-10x faster inference
- Daemon mode with warm model - eliminate load time
- Smaller model - quality tradeoff

## 8. GPU Acceleration Investigation (2025-12-02)

**Findings:**

| Option | Status | Notes |
|--------|--------|-------|
| MAX Engine | ❌ Not viable | Only supports specific GenAI architectures (LLMs), not arbitrary ONNX cross-encoders |
| ONNX Runtime + CoreML (macOS) | ⚠️ Complex | Not in conda-forge, requires custom build or pypi `onnxruntime-silicon` |
| ONNX Runtime + CUDA (Linux) | ⚠️ Complex | SELinux issues on Fedora, Python 3.13 compatibility gaps |
| PyTorch + transformers | ⚠️ Heavy | +2GB dependencies, different inference path |
| CPU baseline | ✅ Working | 1.7s/batch@32, ~19 pairs/sec - acceptable for 100 candidates |

**Decision:** Stay with CPU ONNX Runtime, auto-detect GPU when available

**Rationale:**
1. Current performance is acceptable (2-3s total for typical search)
2. GPU providers are fragmented and platform-specific
3. Auto-detection code already in place (`get_execution_providers()`)
4. When `onnxruntime-gpu` becomes more available, it will "just work"

**Code pattern (already implemented in reranker.py):**
```python
def get_execution_providers() -> list:
    available = ort.get_available_providers()
    preferred = ['CUDAExecutionProvider', 'CoreMLExecutionProvider',
                 'DmlExecutionProvider', 'CPUExecutionProvider']
    return [p for p in preferred if p in available] or ['CPUExecutionProvider']
```

**Why MAX Engine doesn't work:**
- MAX Engine is optimized for LLM inference (transformer decoder architectures)
- Our model is a cross-encoder (BERT-like) which MAX doesn't recognize
- Error: "cannot compile input with format input has unknown contents"
- Even non-quantized ONNX fails - it's an architecture mismatch, not format issue
