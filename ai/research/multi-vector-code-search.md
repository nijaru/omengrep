# Multi-Vector / ColBERT-Style Embedding Models for Code Search

**Purpose:** Comprehensive comparison of multi-vector (late interaction) models for hygrep's semantic code search, evaluating feasibility of upgrading from single-vector snowflake-arctic-embed-s.

**Date:** 2026-02-14

---

## Executive Summary

Multi-vector (ColBERT-style) models offer a meaningful quality improvement over single-vector embeddings for code search. The gap is approximately **+2.5 to +11 NDCG@10 points** depending on model pairing and benchmark. The standout model for hygrep is **LateOn-Code-edge (17M params, 48d/token, Apache 2.0)** -- it has ONNX INT8 available, outperforms single-vector models 3-10x its size, and is purpose-built for code search. The main blocker is omendb's multi-vector support: MuVERA is the right approach but requires implementation work.

**Recommendation:** Adopt LateOn-Code-edge as the multi-vector model. Use MuVERA encoding to flatten multi-vector embeddings into fixed-dimensional single vectors for omendb storage. This gives multi-vector quality with single-vector infrastructure.

---

## 1. ColBERT-Based Models

### 1.1 ColBERTv2 (Baseline Reference)

| Property       | Value                   |
| -------------- | ----------------------- |
| Parameters     | 110M                    |
| Embedding dims | 128/token               |
| Architecture   | BERT-base + compression |
| License        | MIT                     |
| ONNX           | Not official            |
| Context        | 512 tokens              |
| BEIR avg       | 0.488 NDCG@10           |
| Code-specific  | No                      |

The original late-interaction benchmark. Not code-trained. Superseded by newer ModernBERT-based models on all metrics.

### 1.2 mxbai-edge-colbert-v0-17m (Mixedbread)

| Property        | Value                                             |
| --------------- | ------------------------------------------------- |
| Parameters      | 17M (Ettin-17M backbone)                          |
| Embedding dims  | 48/token                                          |
| Architecture    | ModernBERT (Ettin-17M) + 2-layer dense projection |
| License         | Apache 2.0                                        |
| HuggingFace     | `mixedbread-ai/mxbai-edge-colbert-v0-17m`         |
| ONNX            | Available (via sentence-transformers export)      |
| Context         | Up to 32,000 tokens                               |
| BEIR avg        | 0.490 (outperforms ColBERTv2 at 0.488)            |
| LongEmbed       | 0.847 at 32k tokens                               |
| Code-specific   | No (general-purpose)                              |
| Token output    | Variable length (1 vector per token)              |
| CPU time (BEIR) | 487 seconds                                       |
| Memory          | 275 MB                                            |

The foundation that LateOn-Code-edge was built on. General-purpose, not code-trained. Excellent baseline. Paper: arXiv:2510.14880.

### 1.3 mxbai-edge-colbert-v0-32m (Mixedbread)

| Property        | Value                                     |
| --------------- | ----------------------------------------- |
| Parameters      | 32M                                       |
| Embedding dims  | 64/token                                  |
| Architecture    | ModernBERT (Ettin-32M) + dense projection |
| License         | Apache 2.0                                |
| BEIR avg        | 0.521                                     |
| LongEmbed       | 0.849 at 32k                              |
| CPU time (BEIR) | 589 seconds                               |
| Memory          | 366 MB                                    |

Larger variant with marginal improvements. Not worth the 2x size for our use case.

### 1.4 GTE-ModernColBERT-v1 (LightOn)

| Property       | Value                                             |
| -------------- | ------------------------------------------------- |
| Parameters     | ~150M (ModernBERT-base)                           |
| Embedding dims | 128/token                                         |
| Architecture   | Alibaba GTE-ModernBERT-base + dense projection    |
| License        | Apache 2.0                                        |
| HuggingFace    | `lightonai/GTE-ModernColBERT-v1`                  |
| ONNX           | Available                                         |
| Context        | Configurable up to 32,768 tokens (trained on 300) |
| BEIR avg       | 0.547 (first ColBERT to beat ColBERT-small)       |
| LongEmbed      | 0.884 (10 points above prior SOTA)                |
| Code-specific  | No (general-purpose, trained on MS MARCO)         |

General-purpose. Good quality but 10x the size of edge models. Not code-optimized.

### 1.5 Reason-ModernColBERT (LightOn)

| Property       | Value                         |
| -------------- | ----------------------------- |
| Parameters     | ~150M                         |
| Embedding dims | 128/token                     |
| License        | CC-BY-NC-4.0 (non-commercial) |
| Specialty      | Reasoning-intensive retrieval |

Outperforms models up to 7B on BRIGHT benchmark. Non-commercial license eliminates it for hygrep.

---

## 2. LateOn-Code Models (LightOn -- colgrep's models)

### 2.1 LateOn-Code-edge (17M) -- PRIMARY CANDIDATE

| Property       | Value                                                  |
| -------------- | ------------------------------------------------------ |
| Parameters     | 17M (16.8M actual)                                     |
| Embedding dims | 48/token                                               |
| Architecture   | ModernBERT (Ettin-17M) + 2x Dense(256->512->48)        |
| Base model     | `mixedbread-ai/mxbai-edge-colbert-v0-17m` (fine-tuned) |
| License        | **Apache 2.0**                                         |
| HuggingFace    | `lightonai/LateOn-Code-edge`                           |
| ONNX           | **Yes: model.onnx + model_int8.onnx** (on HF repo)     |
| Context        | 2048 tokens (doc), 256 tokens (query)                  |
| Similarity     | MaxSim                                                 |
| Token output   | Variable (1 vector per token, 48 dims each)            |
| Release date   | 2026-02-06                                             |

**MTEB Code v1 Benchmarks (NDCG@10):**

| Dataset                  | Score     |
| ------------------------ | --------- |
| Average                  | **66.64** |
| CodeSearchNet Python     | 92.44     |
| CodeSearchNet Go         | 96.07     |
| CodeSearchNet Java       | 86.55     |
| CodeSearchNet JavaScript | 79.37     |
| CodeSearchNet PHP        | 88.24     |
| CodeSearchNet Ruby       | 83.57     |
| AppsRetrieval            | 26.22     |
| StackOverflow QA         | ~79       |

**Comparison with single-vector models at similar or larger sizes:**

| Model                                  | Params  | Type             | MTEB Code Avg |
| -------------------------------------- | ------- | ---------------- | ------------- |
| **LateOn-Code-edge**                   | **17M** | **Multi-vector** | **66.64**     |
| snowflake-arctic-embed-s (current hhg) | 33M     | Single-vector    | ~52\*         |
| granite-embedding-small-r2             | 47M     | Single-vector    | 55.84         |
| EmbeddingGemma-300M                    | 300M    | Single-vector    | 68.76         |
| BM25 (lexical)                         | --      | Lexical          | 44.41         |

\*snowflake-arctic-embed-s is not on MTEB Code v1 but scores ~52 on general MTEB retrieval; code-specific would likely be lower since it is not code-trained.

**Key advantage:** 17M params, half the size of current snowflake model (33M), yet +11 NDCG@10 points on code benchmarks. ONNX INT8 already on HuggingFace.

**Training data:** ~2.1M samples from CodeSearchNet (6 langs), CodeFeedback, CodeTrans, APPS, CosQA, StackOverflow QA, Text2SQL. Trained with CoRNStack methodology + nv-retriever hard negative mining.

### 2.2 LateOn-Code (149M) -- QUALITY OPTION

| Property       | Value                                             |
| -------------- | ------------------------------------------------- |
| Parameters     | 149M                                              |
| Embedding dims | 128/token                                         |
| Architecture   | ModernBERT-base + Dense(768->128)                 |
| License        | **Apache 2.0**                                    |
| HuggingFace    | `lightonai/LateOn-Code`                           |
| ONNX           | Available (ONNX tag on HF, safetensors confirmed) |
| Context        | 2048 tokens (doc), 256 tokens (query)             |
| MTEB Code Avg  | **74.12**                                         |

**Comparison at 149M scale:**

| Model                       | Params | Type          | MTEB Code Avg |
| --------------------------- | ------ | ------------- | ------------- |
| **LateOn-Code**             | 149M   | Multi-vector  | **74.12**     |
| GTE-ModernBERT (single-vec) | 149M   | Single-vector | 71.66         |
| C2LLM-0.5B                  | 500M   | Single-vector | 75.46         |
| Qwen3-Embedding-0.6B        | 600M   | Single-vector | 75.42         |

The 149M multi-vector model matches 500-600M single-vector models. However, 149M is too large for hygrep's CLI indexing use case where build speed matters.

### 2.3 Availability & Public Access

Both models are fully public on HuggingFace under Apache 2.0. The `lightonai/next-plaid` repo (GitHub) contains ColGrep (Rust binary) and the PLAID index implementation. PyLate library provides the Python training/inference framework.

Files on `lightonai/LateOn-Code-edge` HF repo:

- `model.safetensors` (68 MB)
- `model.onnx` (67.2 MB)
- `model_int8.onnx` (17.2 MB) -- this is what we want
- `tokenizer.json` (1.52 kB)
- `config.json`, `modules.json`, etc.

---

## 3. Snowflake Arctic Embed Multi-Vector

**No multi-vector variant exists.** Snowflake Arctic Embed (all versions: xs/s/m/l, 1.0 and 2.0) are single-vector only. Arctic Embed 2.0 adds multilingual support and Matryoshka dimensions but remains single-vector. No ColBERT variant has been announced.

Current hygrep model specs for reference:

| Property       | snowflake-arctic-embed-s     |
| -------------- | ---------------------------- |
| Parameters     | 33M                          |
| Dimensions     | 384 (single vector)          |
| Context        | 512 tokens                   |
| Architecture   | BERT (e5-small-unsupervised) |
| ONNX           | INT8 (34 MB), FP16 (67 MB)   |
| License        | Apache 2.0                   |
| MTEB Retrieval | ~52 NDCG@10                  |

---

## 4. ModernBERT-Based ColBERT Models

ModernBERT is the dominant backbone for current ColBERT models due to:

- Flash Attention (faster inference)
- Rotary Position Embeddings (long context)
- Unpadding (efficient variable-length batches)
- 8192 token native context

All top ColBERT models in 2025-2026 use ModernBERT:

| Model                     | Base                   | Params | Dims | Code-Trained |
| ------------------------- | ---------------------- | ------ | ---- | ------------ |
| mxbai-edge-colbert-v0-17m | Ettin-17M (ModernBERT) | 17M    | 48   | No           |
| mxbai-edge-colbert-v0-32m | Ettin-32M (ModernBERT) | 32M    | 64   | No           |
| GTE-ModernColBERT-v1      | GTE-ModernBERT-base    | 150M   | 128  | No           |
| LateOn-Code-edge          | Ettin-17M (ModernBERT) | 17M    | 48   | **Yes**      |
| LateOn-Code               | ModernBERT-base        | 149M   | 128  | **Yes**      |

LateOn-Code models are the only code-trained ColBERT models currently available.

---

## 5. Code-Specific Single-Vector Models

### Do any support multi-vector?

| Model                          | Multi-vector? | Notes                                                  |
| ------------------------------ | ------------- | ------------------------------------------------------ |
| CodeSage (130M/356M/1.3B)      | **No**        | Single-vector only, BERT encoder                       |
| CodeSage V2                    | **No**        | Improved quality, flexible dims, still single-vector   |
| StarEncoder                    | **No**        | Single-vector encoder                                  |
| UniXcoder                      | **No**        | Single-vector, supports code understanding tasks       |
| jina-embeddings-v2-base-code   | **No**        | Single-vector, 161M, Apache 2.0                        |
| jina-code-embeddings-0.5b/1.5b | **No**        | Single-vector, CC-BY-NC-4.0                            |
| CodeRankEmbed-137M             | **No**        | Single-vector, MIT license                             |
| jina-embeddings-v4             | **Hybrid**    | 3.8B, supports both single+multi-vector, but too large |

**None of the practical code-specific embedding models support multi-vector output**, except the LateOn-Code family and the massive jina-embeddings-v4 (3.8B, impractical for CLI).

---

## 6. MuVERA vs PLAID vs Raw MaxSim

### 6.1 Raw MaxSim

The native ColBERT scoring: for each query token, find its max similarity to any document token, then sum.

| Aspect   | Raw MaxSim                              |
| -------- | --------------------------------------- |
| Quality  | Perfect (ground truth)                  |
| Latency  | O(Q \* D) per query-doc pair, very slow |
| Storage  | All token vectors stored                |
| Index    | None -- brute force                     |
| Use case | Reranking small candidate sets only     |

### 6.2 PLAID (ColGrep's approach)

Optimized ColBERT index: clusters token vectors into centroids, builds inverted index, uses residual quantization.

| Aspect         | PLAID                                                 |
| -------------- | ----------------------------------------------------- |
| Quality        | Near-exact MaxSim (centroid pruning)                  |
| Latency        | Fast retrieval via inverted index + quantized scoring |
| Storage        | Compressed: centroids + residuals + inverted lists    |
| Index          | Custom PLAID index format (memory-mapped files)       |
| Implementation | `lightonai/next-plaid` (Rust), PyLate (Python)        |
| Complexity     | High -- custom index format, Rust dependency          |

ColGrep uses NextPlaid (Rust reimplementation):

- Clusters token vectors into centroids
- Stores residuals with quantization
- Memory-mapped files (no preloading)
- SQLite metadata for hybrid regex+semantic
- Apple Accelerate + CoreML on macOS

### 6.3 MuVERA (Google Research -- omendb's approach)

Converts multi-vector sets into Fixed Dimensional Encodings (FDEs) that approximate MaxSim via single-vector MIPS.

| Aspect         | MuVERA                                                                   |
| -------------- | ------------------------------------------------------------------------ |
| Quality        | ~90% of exact MaxSim (with rerank: ~99%)                                 |
| Latency        | **90% lower than PLAID** on BEIR                                         |
| Recall         | **10% higher than PLAID** at same latency                                |
| Storage        | Single FDE vector per doc + original multi-vecs for rerank               |
| Index          | Standard HNSW/IVF (any MIPS index works)                                 |
| Memory         | 32x compression with product quantization                                |
| Implementation | Google `graph-mining/sketching`, `sionic-ai/muvera-py`, Qdrant FastEmbed |
| Complexity     | Low -- FDE is a preprocessing step, rest is standard vector search       |

**MuVERA workflow:**

1. Generate multi-vector embeddings (e.g., LateOn-Code-edge produces N x 48 vectors per doc)
2. Convert to FDE: partition embedding space, sum/average vectors per partition -> fixed-length single vector
3. Index FDE in standard MIPS index (HNSW, IVF, etc.)
4. Search: convert query multi-vectors to FDE, do standard MIPS search
5. Rerank: use original multi-vectors + MaxSim on top-k candidates

**Key advantage for hygrep:** MuVERA works with omendb's existing single-vector HNSW. No custom index format needed. The FDE generation is a preprocessing step during indexing.

### 6.4 Comparison Table

| Approach   | Quality                  | Latency  | Storage       | Implementation Complexity | Works with omendb |
| ---------- | ------------------------ | -------- | ------------- | ------------------------- | ----------------- |
| Raw MaxSim | Best                     | Worst    | High          | Low                       | No                |
| PLAID      | Near-best                | Good     | Medium        | High (custom index)       | No                |
| MuVERA     | Good (rerank: near-best) | **Best** | Medium-High\* | **Low**                   | **Yes**           |
| ConstBERT  | Good                     | Good     | Low           | Medium                    | Partial           |

\*MuVERA needs both FDE vectors (for search) and original multi-vectors (for rerank).

### 6.5 ConstBERT (Pinecone)

Fixed number of vectors per document (e.g., 32 or 64), regardless of document length.

| Aspect       | ConstBERT                                             |
| ------------ | ----------------------------------------------------- |
| Quality      | 73.1 vs ColBERT 74.6 on TREC DL19 (as reranker: 74.4) |
| Storage      | 50%+ reduction vs ColBERT                             |
| Use case     | Best as reranker in cascading pipeline                |
| Availability | Open source                                           |

Less relevant for hygrep since we need first-stage retrieval, not just reranking.

---

## 7. Quality Gap: Single-Vector vs Multi-Vector for Code

### Direct Comparisons (MTEB Code v1, same backbone)

| Comparison             | Single-Vector           | Multi-Vector            | Gap        |
| ---------------------- | ----------------------- | ----------------------- | ---------- |
| ModernBERT-base (149M) | GTE-ModernBERT: 71.66   | LateOn-Code: 74.12      | **+2.46**  |
| 17M vs 47M             | granite-small-r2: 55.84 | LateOn-Code-edge: 66.64 | **+10.80** |
| 17M vs 300M            | EmbeddingGemma: 68.76   | LateOn-Code-edge: 66.64 | -2.12      |
| 149M vs 600M           | Qwen3-Embed-0.6B: 75.42 | LateOn-Code: 74.12      | -1.30      |

### Analysis

1. **At same parameter count (149M):** Multi-vector wins by 2.5 points. This is the fair comparison.
2. **At extreme size efficiency (17M):** Multi-vector at 17M matches single-vector at ~200-250M. The 17M LateOn-Code-edge is only 2 points behind the 300M EmbeddingGemma.
3. **Very large single-vector (500M+) can match medium multi-vector (149M).** But these are impractical for CLI.
4. **Fine-tuning matters enormously.** LateOn-Code-edge gains +9.14 from code-specific fine-tuning. Pre-trained multi-vector (57.50) actually trails pre-trained single-vector GTE-ModernBERT (71.66).

### Why Multi-Vector Helps Code Search Specifically

- **Per-token matching:** Code has many tokens with precise meaning (function names, types, operators). Single-vector compression loses these distinctions.
- **Cross-modal bridging:** Natural language query tokens match individual code tokens (e.g., "sort" matches `sorted()`, `Arrays.sort()`, `.sort_by()`).
- **Long-context stability:** Code blocks vary widely in length. Multi-vector handles this naturally.
- **Out-of-domain robustness:** Code spans 22+ languages with different syntax. Token-level matching generalizes better.

### Verdict

The quality gap is real but moderate (~2.5 points at same params). The efficiency advantage is dramatic: 17M multi-vector matches 200M+ single-vector. For a CLI tool where model size determines indexing speed, multi-vector at 17M is the sweet spot.

---

## 8. State of the Art for Code Search (2025-2026)

### Current MTEB Code v1 Leaderboard (top models)

| Rank | Model                | Type             | Params   | Avg       |
| ---- | -------------------- | ---------------- | -------- | --------- |
| 1    | C2LLM-0.5B           | Single-vector    | 500M     | 75.46     |
| 2    | Qwen3-Embedding-0.6B | Single-vector    | 600M     | 75.42     |
| 3    | **LateOn-Code**      | **Multi-vector** | **149M** | **74.12** |
| 4    | GTE-ModernBERT       | Single-vector    | 149M     | 71.66     |
| 5    | EmbeddingGemma-300M  | Single-vector    | 300M     | 68.76     |
| 6    | **LateOn-Code-edge** | **Multi-vector** | **17M**  | **66.64** |
| 7    | granite-small-r2     | Single-vector    | 47M      | 55.84     |
| --   | BM25                 | Lexical          | --       | 44.41     |

### Key Trends

1. **Multi-vector models are now code-specific.** LateOn-Code is the first code-trained ColBERT family.
2. **Small models dominate local/CLI use.** The 17M edge model is purpose-built for terminal tools.
3. **Decoder-based single-vector models lead at scale** (Qwen3, C2LLM) but require 500M+ params.
4. **Hybrid search (semantic + BM25) remains important.** BM25 handles exact keyword matching that neural models miss.
5. **ONNX availability varies.** Most small encoder models (ModernBERT-based) export cleanly. Decoder-based models often lack ONNX.

### Best Approach for Code Search

The optimal approach is **multi-vector (ColBERT) with hybrid BM25**, which is exactly what hygrep already does (semantic + BM25) and what colgrep does. The models now exist; the infrastructure needs to catch up.

---

## 9. Implementation Strategy for hygrep

### Option A: MuVERA Flattening (Recommended)

Use LateOn-Code-edge to generate multi-vector embeddings, then flatten via MuVERA FDE into fixed-dimensional single vectors for omendb.

**Workflow:**

```
Index:
  text -> LateOn-Code-edge (ONNX INT8) -> N x 48 token vectors
       -> MuVERA FDE -> single 4096-dim vector (configurable)
       -> store in omendb (existing HNSW)
       -> also store raw token vectors (for reranking)

Search:
  query -> LateOn-Code-edge -> M x 48 token vectors
        -> MuVERA FDE -> single 4096-dim vector
        -> omendb HNSW search -> top-50 candidates
        -> MaxSim rerank with raw token vectors -> top-10 results
```

**Pros:**

- Works with current omendb infrastructure
- MuVERA Python implementation exists (`sionic-ai/muvera-py`, Qdrant FastEmbed)
- 90% lower latency than PLAID at 10% higher recall
- Single-vector HNSW is well-optimized everywhere

**Cons:**

- Increased storage (FDE + raw vectors)
- FDE dimension is a tuning parameter (affects quality/speed tradeoff)
- Reranking step adds complexity

**Implementation effort:** Medium. Need to:

1. Add LateOn-Code-edge model loading (ONNX, similar to current snowflake)
2. Implement MuVERA FDE generation (port from `muvera-py`, ~200 lines)
3. Store raw token vectors alongside FDE (omendb metadata or separate store)
4. Add MaxSim reranking on top-k candidates

### Option B: Direct PLAID (Like ColGrep)

Use PyLate's PLAID index directly instead of omendb.

**Pros:**

- Battle-tested in ColGrep
- Best retrieval quality

**Cons:**

- Replaces omendb entirely (loses BM25 hybrid)
- PyLate dependency (heavy: torch, sentence-transformers)
- Or Rust NextPlaid dependency
- Major architectural change

### Option C: Wait for omendb Multi-Vector Support

Stay on snowflake-arctic-embed-s until omendb natively supports multi-vector.

**Pros:**

- Zero work now
- Current system works well

**Cons:**

- Unknown timeline
- Competitive disadvantage vs ColGrep
- Leaving quality on the table

### Recommended Path

**Option A (MuVERA)** is the right approach. It gives multi-vector quality while preserving the existing omendb + BM25 hybrid architecture. The implementation is tractable and the key components (ONNX model, MuVERA algorithm) are available.

---

## 10. Model Comparison Matrix (All Candidates)

| Model                     | Params  | Dims       | Type      | License        | ONNX       | Code-Trained | MTEB Code | Speed\*  | Recommendation     |
| ------------------------- | ------- | ---------- | --------- | -------------- | ---------- | ------------ | --------- | -------- | ------------------ |
| snowflake-arctic-embed-s  | 33M     | 384        | Single    | Apache 2.0     | INT8       | No           | ~52       | Fast     | Current model      |
| LateOn-Code-edge          | **17M** | **48/tok** | **Multi** | **Apache 2.0** | **INT8**   | **Yes**      | **66.64** | **Fast** | **Top pick**       |
| LateOn-Code               | 149M    | 128/tok    | Multi     | Apache 2.0     | Yes        | Yes          | 74.12     | Medium   | Quality option     |
| mxbai-edge-colbert-v0-17m | 17M     | 48/tok     | Multi     | Apache 2.0     | Via export | No           | ~55\*\*   | Fast     | Untrained base     |
| mxbai-edge-colbert-v0-32m | 32M     | 64/tok     | Multi     | Apache 2.0     | Via export | No           | ~57\*\*   | Fast     | Slightly better    |
| GTE-ModernColBERT-v1      | 150M    | 128/tok    | Multi     | Apache 2.0     | Yes        | No           | ~55\*\*   | Medium   | General purpose    |
| ColBERTv2                 | 110M    | 128/tok    | Multi     | MIT            | No         | No           | ~49\*\*   | Medium   | Outdated           |
| granite-small-r2          | 47M     | 384        | Single    | Apache 2.0     | Yes        | Partial      | 55.84     | Fast     | Marginal upgrade   |
| jina-code-v2              | 161M    | 768        | Single    | Apache 2.0     | INT8       | Yes          | ~55       | Medium   | Previous hhg model |
| CodeRankEmbed             | 137M    | 768        | Single    | MIT            | No         | Yes          | ~60       | Medium   | No ONNX            |
| jina-code-0.5b            | 494M    | 896        | Single    | CC-BY-NC       | GGUF       | Yes          | 78.4      | Slow     | Non-commercial     |

\*Speed: relative for CPU ONNX INT8 inference
\*\*Estimated on code tasks based on general BEIR performance

---

## 11. ONNX Export Feasibility

| Model                     | ONNX Status | Notes                                          |
| ------------------------- | ----------- | ---------------------------------------------- |
| LateOn-Code-edge          | **Ready**   | `model_int8.onnx` (17.2 MB) on HF repo         |
| LateOn-Code               | Available   | `model.onnx` on HF, may need INT8 quantization |
| mxbai-edge-colbert-v0-17m | Exportable  | ModernBERT exports cleanly via optimum         |
| GTE-ModernColBERT-v1      | Available   | ONNX tag on HF                                 |
| snowflake-arctic-embed-s  | **Ready**   | Current hhg model                              |

**Key difference from single-vector:** ColBERT ONNX models output `(batch, seq_len, dim)` instead of `(batch, dim)`. Need to keep all token embeddings, not just CLS. The attention mask indicates which tokens are real (for variable-length output).

---

## 12. MLX Compatibility

LateOn-Code-edge is ModernBERT-based. Current hhg MLX embedder already supports ModernBERT (snowflake-arctic-embed-s uses a different architecture, but the MLX path uses `mlx-embeddings` which has ModernBERT support).

| Model                    | MLX Feasibility | Notes                                         |
| ------------------------ | --------------- | --------------------------------------------- |
| LateOn-Code-edge         | **Good**        | ModernBERT arch, `mlx-embeddings` supports it |
| LateOn-Code              | Good            | Same ModernBERT-base arch                     |
| snowflake-arctic-embed-s | Working         | Current hhg MLX backend                       |

The main change for MLX would be outputting per-token vectors instead of pooled single vectors. This requires modifying the MLX embedder to return the full sequence output.

---

## 13. Storage Impact Estimate

For a 10,000-block codebase (typical medium project):

| Approach                                | Storage Per Block       | Total (10k blocks) |
| --------------------------------------- | ----------------------- | ------------------ |
| Current (384d single-vec)               | 384 \* 4B = 1.5 KB      | ~15 MB             |
| LateOn-Code-edge (48d, ~100 tokens avg) | 100 _ 48 _ 4B = 19.2 KB | ~192 MB            |
| MuVERA FDE (4096d)                      | 4096 \* 4B = 16 KB      | ~160 MB            |
| MuVERA FDE + raw vectors                | 16 + 19.2 KB = 35.2 KB  | ~352 MB            |

Multi-vector approaches use ~10-25x more storage than single-vector. MuVERA without raw vector storage for reranking is comparable to raw multi-vector. For CLI tools indexing local codebases, 200-350 MB is acceptable.

---

## Sources

- [LateOn-Code model card](https://huggingface.co/lightonai/LateOn-Code)
- [LateOn-Code-edge model card](https://huggingface.co/lightonai/LateOn-Code-edge)
- [LateOn-Code collection](https://huggingface.co/collections/lightonai/lateon-code)
- [ColGrep / NextPlaid (GitHub)](https://github.com/lightonai/next-plaid)
- [LightOn blog: LateOn-Code & ColGrep](https://huggingface.co/blog/lightonai/colgrep-lateon-code)
- [mxbai-edge-colbert-v0 (Mixedbread)](https://huggingface.co/mixedbread-ai/mxbai-edge-colbert-v0-17m)
- [mxbai-edge-colbert blog](https://www.mixedbread.com/blog/edge-v0)
- [GTE-ModernColBERT-v1](https://huggingface.co/lightonai/GTE-ModernColBERT-v1)
- [MUVERA (Google Research)](https://research.google/blog/muvera-making-multi-vector-retrieval-as-fast-as-single-vector-search/)
- [MUVERA paper (NeurIPS 2024)](https://arxiv.org/abs/2405.19504)
- [muvera-py (Python implementation)](https://github.com/sionic-ai/muvera-py)
- [ConstBERT (Pinecone)](https://www.pinecone.io/blog/cascading-retrieval-with-multi-vector-representations/)
- [LEMUR multi-vector retrieval](https://arxiv.org/html/2601.21853v1)
- [CoRNStack paper](https://arxiv.org/abs/2412.01007)
- [PyLate library](https://github.com/lightonai/pylate)
- [Qdrant FastEmbed MuVERA](https://qdrant.tech/documentation/fastembed/fastembed-postprocessing/)
- [Jina Code Embeddings 0.5b/1.5b](https://jina.ai/news/jina-code-embeddings-sota-code-retrieval-at-0-5b-and-1-5b/)
- [CodeSage](https://code-representation-learning.github.io/)

---

_Research date: 2026-02-14_
