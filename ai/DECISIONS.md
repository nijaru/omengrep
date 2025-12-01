# Architectural Decisions

## 1. Language & Runtime
**Decision:** Mojo + ONNX Runtime
**Why:**
- **Mojo:** Native performance for systems code (Scanner).
- **ONNX Runtime:** Industry standard for inference. Using Python Interop for now (stability), targeting pure C-API binding later if needed.

## 2. Core Architecture: "Hyper Hybrid"
**Decision:** Two-Stage Pipeline: Recall (Keyword) -> Rerank (Semantic).
**Rationale:**
- **Recall:** Fast regex filtering finds candidates.
- **Reranker:** Cross-encoder scores candidates.
- **Benefit:** Zero indexing time. Always fresh.

## 3. Model Selection
**Decision:** `mixedbread-ai/mxbai-rerank-xsmall-v1`
**Why:**
- **Size:** ~40MB (Quantized).
- **Speed:** Extremely low latency.
- **Performance:** Competitive with larger models for code retrieval.

## 4. Optimization Strategy
**Decision:** Parallelize IO, Native Regex
**Why:** Python overhead is acceptable for the *Reranker* (run once on 50 items), but unacceptable for the *Scanner* (run on 50,000 items).
- **Scanner:** Must be pure Mojo/C (Parallel).
- **Reranker:** Python Interop is fine (Vectorized).

## 5. Parallel Implementation
**Decision:** `algorithm.parallelize` with `UnsafePointer` Mask.
**Why:**
- Mojo's `List` is not thread-safe for concurrent writes.
- Allocating a boolean mask (thread-safe writing by index) prevents locks/contention.
- Single-threaded gather pass is negligible compared to scanning 10k+ files.
