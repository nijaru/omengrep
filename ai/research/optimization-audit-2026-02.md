# Optimization Audit: omengrep Performance & omendb Feature Gaps

**Date:** 2026-02-16
**Baseline:** M3 Max, release build, 23 files / 118 blocks
**Build:** 2.1s | **Search:** 110ms | **Model:** LateOn-Code-edge INT8 48d/tok
**omendb version:** 0.0.28

---

## Summary

Five high-impact findings, ordered by expected impact:

| #   | Finding                                                                  | Impact                                        | Effort | Type        |
| --- | ------------------------------------------------------------------------ | --------------------------------------------- | ------ | ----------- |
| 1   | `find_similar` uses FDE-only search, skips MaxSim reranking              | Quality: major                                | Low    | Bug         |
| 2   | `search()` bypasses `query_with_options`, misses brute-force MaxSim path | Quality: significant                          | Low    | Feature gap |
| 3   | `open_or_create_store` does not use `multi_vector_with` config on create | Quality: configurable                         | Low    | Feature gap |
| 4   | Embedder creates new session per `SemanticIndex::new` call               | Perf: 50-100ms wasted on search               | Medium | Perf        |
| 5   | Token pooling (pool_factor=2) not enabled                                | Quality: slight (+0.6%), storage: -50% tokens | Low    | Feature gap |

---

## 1. `find_similar` Uses FDE-Only Search (Quality Bug)

**File:** `/Users/nick/github/nijaru/hygrep/src/index/mod.rs:316-320`

**Problem:** `find_similar` retrieves the FDE vector via `store.get(&block_id)` and then calls `store.search(&query_vec, ...)`. For a multi-vector store, `store.get()` returns the FDE (Fixed Dimensional Encoding) vector, and `store.search()` does HNSW inner-product search on FDEs without MaxSim reranking. This is the approximate retrieval path only -- it never reranks with the original token embeddings.

By contrast, the `search()` method (text query search) uses `store.search_multi_with_text()` which does BM25 candidate generation followed by MaxSim reranking on original tokens. The quality gap between FDE-only and MaxSim-reranked results is significant, especially for small indexes where omendb would normally use brute-force MaxSim (the `BRUTE_FORCE_MAXSIM_THRESHOLD` of 5000 documents).

**Current code:**

```rust
// src/index/mod.rs:316-320
let (query_vec, _meta) = store
    .get(&block_id)
    .with_context(|| "Could not retrieve block embedding")?;

let results = store.search(&query_vec, k * 3 + entry.blocks.len(), None)?;
```

**Fix:** Use `store.get_tokens()` to retrieve original token embeddings, then `store.query_with_options()` (multi-vector path) for MaxSim-reranked search.

```rust
let (tokens, _meta) = store
    .get_tokens(&block_id)
    .with_context(|| "Could not retrieve block tokens")?;

let token_refs: Vec<&[f32]> = tokens.iter().map(|t| t.as_slice()).collect();
let results = store.query_with_options(
    &token_refs,
    k * 3 + entry.blocks.len(),
    &SearchOptions::default(),
)?;
```

**Expected impact:** Major quality improvement for `find_similar` (file#name, file:line searches). At 118 blocks (well under 5000), omendb will use brute-force MaxSim for 100% recall instead of FDE approximation.

**Effort:** Low -- straightforward API change.

**Score normalization:** The current code at line 353 does `(2.0 - r.distance) / 2.0` which is wrong for MaxSim scores. MaxSim returns similarity (higher = better), not L2 distance. With the query_with_options path, `r.distance` is already a MaxSim score. Just use it directly (or invert to keep the same convention).

---

## 2. `search()` Bypasses `query_with_options` Multi-Vector Path

**File:** `/Users/nick/github/nijaru/hygrep/src/index/mod.rs:211-224`

**Problem:** The main `search()` method manually encodes query tokens to `Vec<Vec<f32>>`, then passes them to `store.search_multi_with_text()`. This works but bypasses omendb's `query_with_options()` which has important internal logic:

1. **Brute-force MaxSim for small collections** -- at <5000 docs, omendb skips FDE entirely and does exact MaxSim over all documents for 100% recall.
2. **Configurable reranking factor** -- `SearchOptions::rerank` controls candidate generation depth.

The current `search_multi_with_text()` path is text-first (BM25 candidates, then MaxSim rerank). This is good when the text query has meaningful terms, but for conceptual/semantic queries where BM25 recall is poor (e.g., "error handling patterns"), the text-first path may miss relevant results that FDE+MaxSim would catch.

**Recommendation:** For small indexes (<5000 blocks), use `query_with_options()` as primary search and BM25 as a secondary signal. For larger indexes, `search_multi_with_text()` remains the right choice.

Alternatively, combine both candidate sets: get BM25 candidates AND FDE candidates, then MaxSim rerank the union. This is what `search_hybrid()` does for single-vector stores.

**Expected impact:** Better recall for conceptual queries on small-to-medium codebases. Quantify by comparing MRR on a benchmark set.

**Effort:** Low-Medium. The API exists; the question is how to combine text and vector candidates.

---

## 3. Store Creation Does Not Pass `MultiVectorConfig`

**File:** `/Users/nick/github/nijaru/hygrep/src/index/mod.rs:559`

**Problem:** `open_or_create_store` creates new stores with `VectorStore::multi_vector(TOKEN_DIM)` which uses `MultiVectorConfig::default()`. The default config is:

- repetitions: 8, partition_bits: 4, d_proj: 16 -> FDE dim = 2048
- pool_factor: None (no token pooling)

This is reasonable for general use. However, for code search with 48d tokens (very small), the d_proj=16 projection loses only 3x information. Two improvements to evaluate:

**A. Enable pool_factor=2** -- omendb docs claim 100.6% quality (slight improvement) with 50% storage reduction. For 118 blocks with ~5-50 tokens each, the MaxSim rerank time would halve.

```rust
VectorStore::multi_vector_with(TOKEN_DIM, MultiVectorConfig::compact())
```

**B. Consider quality preset** -- for 48d tokens, `d_proj=32` is larger than the token dim, so it would be a no-op. `d_proj=16` (default) is fine. But `repetitions=10` might give better FDE approximation:

```rust
MultiVectorConfig {
    repetitions: 10,
    pool_factor: Some(2),
    ..Default::default()
}
```

**Expected impact:** pool_factor=2 is the main win: 50% less token storage, faster MaxSim, claimed +0.6% quality. The FDE dimension would stay at 2048 (or 2560 with rep=10).

**Effort:** Low -- one line change. Would require `og build --force` to rebuild existing indexes.

**Risk:** Changing config breaks existing indexes. Must bump manifest version or detect config mismatch.

---

## 4. Embedder Initialization per SemanticIndex::new()

**File:** `/Users/nick/github/nijaru/hygrep/src/index/mod.rs:49`, `/Users/nick/github/nijaru/hygrep/src/cli/search.rs:77-98`

**Problem:** `SemanticIndex::new()` calls `embedder::create_embedder()` which calls `OnnxEmbedder::new()` which:

1. Downloads/locates model from HuggingFace Hub cache (`hf_hub::api::sync::Api`)
2. Creates ORT Session (loads ONNX model, compiles graph)
3. Loads tokenizer from HF cache

In the search path (`cli/search.rs:77-98`), two `SemanticIndex::new()` calls happen:

- Line 77: `SemanticIndex::new(&index_root, None)` for stale file check / auto-update
- Line 98: `SemanticIndex::new(&index_root, Some(&search_path))` for actual search

Each creates a separate ORT session. ORT session creation involves graph optimization (Level3) and thread pool setup, which takes 30-50ms even with cached model files. The HF Hub API also has filesystem overhead checking cache validity.

**Fix:** Create the embedder once and share it, or make SemanticIndex take an optional pre-built embedder. Alternatively, combine the two SemanticIndex instances into one (the only difference is `search_scope`).

```rust
// Create index once, reuse for both update check and search
let index = SemanticIndex::new(&index_root, Some(&search_path))?;
// Use index for stale check (needs None scope, but stale check doesn't use scope)
let stale_count = index.needs_update(&files)?;
// ... update if needed ...
let results = index.search(query, num_results)?;
```

The stale check (`needs_update`) only reads the manifest and hashes files -- it does not use the embedder or search scope. So a single SemanticIndex with search_scope set works for both.

Actually, looking more carefully, the update path at line 84 calls `index.update()` which does need the embedder but does NOT need search_scope (it re-indexes the full index root). The search at line 99 needs search_scope. Since update operates on the index root and search operates on a filtered view, one instance with scope set should work because update() calls self.index() internally which does not filter by scope.

**Expected impact:** ~30-50ms saved per search invocation. On a 110ms baseline, that is 27-45% improvement.

**Effort:** Medium -- requires refactoring the search command to reuse a single SemanticIndex.

---

## 5. ORT Execution Provider and Batch Size

**File:** `/Users/nick/github/nijaru/hygrep/src/embedder/onnx.rs:19-23`, `/Users/nick/github/nijaru/hygrep/src/embedder/mod.rs:15`

### 5a. Execution Provider

The ORT session uses the default CPU execution provider. On M3 Max, the CoreML execution provider could accelerate inference for INT8 models. However, the `ort` crate's CoreML support requires the `coreml` feature and may have compatibility issues with INT8 ONNX models. The ANE (Apple Neural Engine) path in CoreML is particularly fast for quantized models.

**Recommendation:** Benchmark with CoreML EP enabled. Add `ort = { ..., features = ["coreml"] }` and `Session::builder()?.with_execution_providers([CoreMLExecutionProvider::default()])`. If it does not improve or breaks INT8, revert.

**Expected impact:** Potentially 2-5x faster inference on M3 Max, but uncertain. CoreML INT8 support varies.

**Effort:** Low to try, may require debugging if CoreML rejects the INT8 graph.

### 5b. Batch Size

`BATCH_SIZE = 64` is used for document embedding during build. For 118 blocks, this means 2 batches (64 + 54). The batch size is fine for build throughput.

For query embedding, only 1 document is embedded (the query), so batch size is irrelevant.

The real bottleneck during build is sequential embedding -- blocks are extracted in parallel (rayon) but then embedded serially in `BATCH_SIZE` chunks. For 118 blocks, this is only 2 ONNX inference calls. The 2.1s build time is likely dominated by:

- ORT session init: ~50ms
- Tokenization + embedding: ~1.5s (2 batches of ~750ms each at Level3 optimization)
- Extraction + file I/O: ~200ms
- omendb insert + flush: ~200ms

### 5c. Intra-op threads

`num_cpus()` returns all available cores (M3 Max = 16). ORT's intra-op thread count controls parallelism within a single inference call. For a batch of 64 with a 17M parameter model, 16 threads is appropriate. No change needed.

---

## 6. Over-Fetching Ratios

**File:** `/Users/nick/github/nijaru/hygrep/src/index/mod.rs:220,320`

### Text search path (line 220)

`search_k = k * 3` is the over-fetch factor for search scope filtering. The BM25 candidate count is controlled by omendb internally (default 10x k in `search_multi_with_text`). So the actual candidate pipeline is:

- BM25 fetches `10 * search_k` = `30k` candidates
- MaxSim reranks those `30k` to `search_k`
- Scope filter reduces to `k`

For k=10 (default), this is 300 BM25 candidates -> 30 MaxSim -> 10 scope-filtered. This is reasonable. If search scope rarely filters, 3x is wasteful. If scope filters aggressively (e.g., searching `src/cli/` in a large index), 3x may not be enough.

**Recommendation:** Make scope-aware: if search_scope is set, use higher over-fetch (5x). If no scope, use 1x (no filtering needed).

### find_similar path (line 320)

`k * 3 + entry.blocks.len()` over-fetches to skip self-file blocks. This is correct -- if the file has N blocks, you need to skip up to N results plus fetch k extra.

---

## 7. Build Pipeline: Extraction/Embedding Overlap

**File:** `/Users/nick/github/nijaru/hygrep/src/index/mod.rs:106-184`

**Current flow:**

1. Extract all blocks in parallel (rayon) -> `all_blocks` (line 106-113)
2. Prepare embedding text + sort by length (line 116-143)
3. Embed in sequential batches (line 148-183)

Steps 1 and 3 are strictly sequential. For 118 blocks, step 1 takes ~200ms and step 3 takes ~1.5s. Overlapping them would save ~200ms (extract next batch while current batch is embedding).

**However:** The batch is only 118 blocks (2 ONNX calls). The overhead of a pipeline abstraction would be minimal gain. For larger indexes (1000+ blocks), this matters more.

**Recommendation:** Not worth the complexity at current scale. Revisit if targeting larger codebases (500+ files).

---

## 8. BM25 Tokenization Quality

**File:** `/Users/nick/github/nijaru/hygrep/src/tokenize.rs`

The current approach appends split identifier terms to the original text before indexing in BM25. This works but has edge cases:

**A. Short identifier skipping (line 56-58):** Words < 4 chars are skipped. This means identifiers like `Vec`, `Map`, `Arc`, `Box` are never split. But they are also never _matched_ -- a query for "Vec" won't match a document containing "Vec" through the split terms path, only through the original text path (which treats "Vec" as a single token in tantivy).

This is actually fine -- tantivy's default tokenizer handles short words. The `split_identifiers` function only needs to handle compound identifiers.

**B. Deduplication strips frequency signal:** The `sort_unstable + dedup` at line 66-68 removes duplicate terms. In BM25, term frequency matters. A function that uses "user" 5 times should rank higher for "user" than one that uses it once. By deduplicating, we flatten the TF signal for split terms.

**Recommendation:** Remove dedup for BM25 text. Keep it for extract_terms (used in boost scoring where frequency does not matter).

**Expected impact:** Minor quality improvement for queries matching repeated identifier subterms.

**Effort:** Low -- remove 2 lines in `split_identifiers`.

---

## 9. Boost Scoring Calibration

**File:** `/Users/nick/github/nijaru/hygrep/src/boost.rs`

The boost multipliers (2.5x name match, 1.5x type match, 1.3x class, 1.2x function) are applied multiplicatively and capped at 4x. Key observations:

**A. Boost applied after search, not during candidate generation.** This means if the best match by score is rank 15 but gets a 2.5x name boost, it jumps to rank 1. This is correct behavior -- boosts should rerank, not filter.

**B. The 4x cap may be too low for exact name matches.** If a user searches for "handleSearch" and there is a function literally named `handleSearch`, the 2.5x name match + 1.2x function type = 3.0x combined boost. This is under the 4x cap but may not be enough to override a semantically closer but differently-named result.

**C. File path boost (1.15x) fires on any 3+ character term match.** For a query "search results", the term "search" would match file paths like `search.rs`, `cli/search.rs`, etc. This is helpful but the 3-char minimum means short but distinctive path components like "db" or "io" are missed (they are in SHORT_WHITELIST for term extraction but not for path matching since `t.len() >= 3` filters them).

**Recommendation:** No changes needed. The calibration is reasonable for code search. Monitor MRR on real queries before tuning.

---

## 10. Unused omendb Features

### A. `compact()` and `optimize()`

**Not called anywhere in omengrep.** After incremental updates with many deletes, tombstoned records accumulate. For small indexes (118 blocks), this is negligible. For larger indexes with frequent file changes, periodic compaction would reclaim space and improve search performance.

**Recommendation:** Call `store.compact()` when `store.deleted_count()` exceeds a threshold (e.g., 10% of total). Add to the `og build` command. Low priority.

### B. `SQ8 quantization`

Not relevant for current FDE dimensions (2048). SQ8 gives 4x compression with ~99% recall. At 118 vectors of 2048 dimensions, total storage is ~1MB. Not worth quantizing.

### C. `search_multi_with_sparse` (SPLADE)

Future feature, already documented in `ai/DECISIONS.md`. Requires a sparse embedding model. Not actionable now.

### D. `VectorStoreOptions` builder for store creation

Not used. Currently creates stores with `VectorStore::multi_vector(TOKEN_DIM).persist(path)`. Could use the builder to set HNSW params:

```rust
VectorStoreOptions::default()
    .dimensions(fde_dim)
    .ef_search(200)  // higher for better recall at small scale
    .text_search(true)
    .open(path)?
```

But this does not work for multi-vector stores -- `VectorStoreOptions` does not have a `multi_vector()` method. The builder creates single-vector stores only. This is an omendb API gap.

### E. `HybridParams` for tuning BM25/vector fusion

The `search_multi_with_text()` method in omendb does not use `HybridParams` -- it is a text-first pipeline, not RRF fusion. The BM25 candidate count is the only tunable (currently `None` = 10x k default).

**Recommendation:** Expose `num_candidates` as a CLI flag for power users. Default 10x is fine.

### F. `TextSearchConfig::writer_buffer_mb`

Default is 50MB. For a code search tool indexing <1000 blocks, this is generous. Could reduce to 15MB to lower memory footprint during build. Negligible performance difference at this scale.

---

## Priority Implementation Order

1. **Fix `find_similar` to use `query_with_options` with multi-vector tokens** -- quality bug, low effort
2. **Eliminate duplicate SemanticIndex creation in search path** -- 30-50ms saved, medium effort
3. **Enable `MultiVectorConfig::compact()` (pool_factor=2)** -- storage + quality improvement, low effort
4. **Adjust over-fetch ratio based on search scope** -- quality edge case, low effort
5. **Remove dedup from split_identifiers BM25 text** -- minor quality, trivial effort

Items 1 and 2 are the highest ROI. Item 3 requires a manifest version bump or config check.

---

## Measurement Plan

Before implementing, establish baselines:

```bash
# Build benchmark (5 runs, median)
hyperfine --warmup 1 'og build --force ./src'

# Search benchmark (5 runs, median)
hyperfine --warmup 1 'og "error handling" ./src'

# Search quality (manual inspection)
og "error handling" ./src          # conceptual query
og "embed_query" ./src             # exact name query
og src/boost.rs#boost_results      # find_similar by name
og src/index/mod.rs:211            # find_similar by line
```

After each change, re-run and compare. Record in this file.
