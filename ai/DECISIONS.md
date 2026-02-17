# Architectural Decisions

## 1. Rust Rewrite (2026-02-14)

**Decision:** Full rewrite from Python/Mojo to Rust.

**Context:** Python startup (~150ms), GIL-limited parallelism, serialized embed/insert loops. omendb is a Rust crate — Python bindings add overhead. Multi-vector (MuVERA) support requires tight omendb integration.

**Choice:** Single crate (lib + bin), Rust nightly (portable_simd for omendb).

**Trade-offs:**

- Lose PyPI distribution — gain cargo install, native binary
- Lose Mojo scanner — gain ignore crate (same underlying behavior)
- No index migration — manifest v8 is a clean break

## 2. Multi-Vector Embeddings via MuVERA (2026-02-14)

**Decision:** ColBERT-style token-level embeddings with MuVERA compression, replacing single-vector CLS pooling.

**Context:** Single-vector (CLS pool) loses structural patterns — function signatures, type annotations, parameter names all collapse into one vector. Token-level matching preserves per-token semantics.

**Choice:** LateOn-Code-edge (17M params, 48d/token, Apache 2.0, ONNX INT8). omendb's MuVERA compresses variable-length token sequences into fixed-dimensional encodings for HNSW, then MaxSim reranks candidates.

**Why this model:**

- Code-specific training data
- 48d/token is small enough for local CPU inference
- Apache 2.0 license
- INT8 quantization available

**Alternatives rejected:**

- answerai-colbert-small-v1 (96d — 2x memory per token)
- snowflake-arctic-embed-s (single-vector, no token-level matching)
- granite-embedding-30m (single-vector, larger but no quality gain over multi-vector)

## 3. BM25 + MuVERA Hybrid Search (2026-02-14)

**Decision:** Two-stage retrieval: BM25 candidate generation, then MuVERA MaxSim reranking.

**Context:** Brute-force MaxSim over all documents is O(n \* seq_len). BM25 narrows candidates cheaply. omendb's `search_multi_with_text()` implements this as a single API call.

**Architecture:**

```
Query -> BM25 (tantivy) candidates -> MuVERA MaxSim rerank -> Code-aware boost -> Results
```

**Performance:** Search latency 270-440ms on real codebases (M3 Max). BM25 pre-filtering is the key — eliminates brute-force token comparison.

## 4. Code-Aware AST Extraction (2025-12-04, carried forward)

**Decision:** Tree-sitter AST extraction into semantic blocks, not whole-file indexing.

**Context:** Indexing entire files gives coarse results — a match on line 5 of a 500-line file isn't useful. Extracting functions, classes, methods, structs gives precise, actionable results.

**Implementation:** 25 language parsers compiled in. Each language has tree-sitter queries for extractable node types (functions, classes, methods, structs, interfaces, enums, traits, impls).

**Trade-offs:**

- More blocks per file = more embeddings = slower build
- But: more precise search results, faster search (smaller docs = faster MaxSim)

## 5. Code-Aware Ranking Boosts (2025-12-18, carried forward)

**Decision:** Post-search heuristic boosts for code-specific ranking.

**Implementation:**

- CamelCase/snake_case splitting for term matching
- Exact name match: 2.5x boost
- Term overlap: +30% per matching term
- Type-aware: 1.5x if query mentions "class"/"function"
- File path relevance: 1.15x
- Boost cap at 4x

**Rationale:** Zero latency overhead. Handles identifier-style searches that semantic models alone miss.

## 6. BM25 Tokenization Strategy (2026-02-14)

**Decision:** Pre-process text with camelCase/snake_case splitting before indexing in BM25.

**Context:** tantivy's default tokenizer treats `getUserProfile` as one token. A query for "get user profile" won't match. Two approaches:

| Approach                             | Pros               | Cons                        |
| ------------------------------------ | ------------------ | --------------------------- |
| Custom tantivy tokenizer in omendb   | Clean, proper      | Requires omendb API changes |
| Pre-split text before `index_text()` | Works now, no deps | Duplicates terms in index   |

**Choice:** Pre-split in omengrep as immediate fix. Request custom tokenizer support in omendb for clean solution.

**Implementation:** Transform `getUserProfile` -> `getUserProfile get User Profile` before calling `index_text()`. Preserves original for exact match, adds split terms for partial match.

## 7. Sparse Vectors / SPLADE (2026-02-14, future)

**Decision:** Wait for omendb native sparse support, then evaluate.

**Context:** SPLADE is SOTA for learned sparse retrieval (+10 nDCG over BM25). But: no code-specific models exist, BERT's WordPiece tokenizer mangles code identifiers, and NAVER's SPLADE models are non-commercial licensed.

**Plan:**

1. Near-term: improve BM25 tokenization (Decision #6) — zero inference cost
2. When omendb ships sparse vectors: evaluate `ibm-granite/granite-embedding-30m-sparse` (30M, Apache 2.0, 50.8 nDCG)
3. Long-term: fine-tune sparse model on code data

**Key insight from research:** MuVERA reranking already handles semantic understanding. SPLADE's value is better candidate recall for conceptual queries. Current code-aware boost heuristics partially address this gap.

## 8. Naming: omengrep / og (2026-02-14)

**Decision:** Package name `omengrep`, binary name `og`, index directory `.og/`.

**Context:** Evaluating names for the crate and CLI binary after the Rust rewrite. Needed a name that works on crates.io, is memorable as a CLI command, and connects to the omendb ecosystem.

**Choice:** `omengrep` (crate) + `og` (binary). Ties to the omendb brand while keeping the CLI invocation short. No namespace conflicts on crates.io. Environment variable prefix `OG_` (e.g., `OG_AUTO_BUILD`).

**Alternatives rejected:**

- `omgrep` / `omg` — too close to "oh my god", unprofessional
- `hygrep` / `hhg` — original name, no brand connection to omendb
- `omengrep` as binary — too long for frequent CLI use

## 9. omendb Integration Points (2026-02-16)

**Current API usage:**

- `VectorStore::multi_vector_with(dim, MultiVectorConfig::compact())` — create store with token pooling
- `store.enable_text_search()` — enable BM25
- `store.store_with_text(id, tokens, text, metadata)` — store multi-vector + BM25 text
- `store.search_multi_with_text(query, tokens, k, filter)` — BM25 candidates + MaxSim rerank
- `store.query_with_options(tokens, k, options)` — pure semantic MaxSim search
- `store.get_tokens(id)` — retrieve token embeddings + metadata
- `store.get_metadata_by_id(id)` — metadata only
- `store.delete(id)` / `store.flush()` — remove + persist

**Requested omendb changes:**

1. Custom tantivy tokenizer config in `TextSearchConfig` — code-aware BM25
2. Native sparse vector support — for future SPLADE integration

## 10. Merged BM25 + Semantic Candidates (2026-02-16)

**Decision:** Run both `search_multi_with_text()` (BM25 candidates) and `query_with_options()` (semantic candidates) in parallel, merge by ID keeping higher score.

**Context:** BM25-first-only search misses conceptual queries ("error handling patterns") where BM25 recall is poor. For <5K blocks, `query_with_options()` does brute-force MaxSim with 100% recall.

**Implementation:** HashMap merge with Entry API — O(n) dedup. Overfetch 5x when scope filtering active, 1x without.

## 11. Single Model (2026-02-16)

**Decision:** Single model (LateOn-Code-edge) with `ModelConfig` struct for future extensibility.

**Context:** Initially built multi-model registry with `--model` flag. Simplified to single model — LateOn-Code (149M) is the obvious upgrade path but not worth the complexity until users ask for it. The `ModelConfig` struct remains for when that happens.

## 12. Token Pooling via compact() (2026-02-16)

**Decision:** Enable `MultiVectorConfig::compact()` (pool_factor=2) for 50% token storage reduction at 100.6% quality.

**Context:** omendb supports token pooling that averages adjacent token pairs, halving storage while maintaining search quality per MTEB benchmarks. Required manifest version bump (8->9) and `og build --force` to rebuild.

## 13. MCP Server for Agent Integration (2026-02-16)

**Decision:** Manual JSON-RPC over stdio implementing MCP protocol, no external crate.

**Context:** MCP protocol is simple JSON-RPC with `initialize`, `tools/list`, `tools/call` methods. Manual implementation is ~300 lines vs adding a dependency for essentially the same code. Three tools exposed: `og_search`, `og_similar`, `og_status`.

**Trade-offs:**

- No streaming support (not needed for search results)
- No prompts/resources (tools-only server)
- Simple, zero-dependency implementation
