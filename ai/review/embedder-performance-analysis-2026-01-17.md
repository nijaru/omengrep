# Embedder Performance Analysis (2026-01-17)

## Performance Analysis

**Target:** `src/hygrep/embedder.py`, `src/hygrep/mlx_embedder.py`, `src/hygrep/_common.py`

**Baseline:**
| Metric | Value |
|--------|-------|
| Build time (src/) | 7.48s (127 blocks) |
| MLX throughput | 97.6 texts/sec |
| ONNX throughput | 21.3 texts/sec |
| Query latency (cold) | 3-14ms |
| Query latency (cached) | 0.001ms |

## Findings

### 1. Redundant Internal Bucketing in MLX - MAJOR

**Location:** `src/hygrep/mlx_embedder.py:152-190` (`_embed_batch`)

**Issue:** The MLX embedder internally buckets texts by length to avoid a NaN bug. However:

1. `semantic.py` already sorts texts by length before calling `embed()` (line 385)
2. The NaN bug does not occur with snowflake-arctic-embed-s (tested with extreme length variation)
3. The bucketing creates many small model calls instead of full batches

**Measurement:**

```
Current (sorted + internal bucket): 1.097s (101 texts)
Direct batching (sorted, no bucket): 0.626s (101 texts)
Speedup: 43%
```

The internal bucketing splits 101 texts into 25 separate buckets, causing 25+ model invocations instead of 2 (ceil(101/64)).

**Impact:** HIGH - 43% of embedding time wasted on redundant bucketing

### 2. Tokenizer Parallel Warning Spam - MINOR

**Location:** Multiprocessing in `semantic.py:357-361`

**Issue:** The tokenizers library warns about parallelism after fork. This is cosmetic but noisy.

**Evidence:**

```
huggingface/tokenizers: The current process just got forked, after parallelism has already been used.
(repeated 30+ times per build)
```

**Impact:** LOW - Cosmetic issue, no performance impact

### 3. Model Forward Pass Dominates - EXPECTED

**Location:** `src/hygrep/mlx_embedder.py:141`

**Measurement (64 texts batch):**

```
Tokenization:        31.5ms (14%)
np->mlx conversion:   0.1ms (<1%)
Model forward pass: 221.6ms (85%)  <- GPU-bound
mlx->np conversion:   0.3ms (<1%)
L2 normalization:     0.1ms (<1%)
```

The model forward pass is the true bottleneck. This is expected and cannot be optimized without model changes.

**Impact:** N/A - Working as intended

### 4. Query Cache Effective - NO ACTION

**Location:** `src/hygrep/mlx_embedder.py:213-232`

**Measurement:**

```
Cache miss: 3-14ms
Cache hit:  0.001ms
Speedup:    3000-14000x
```

**Impact:** N/A - Working as intended

### 5. First Query Warm-up - EXPECTED

**Measurement:**

```
First query:  13.8ms
Second query:  3.2ms
```

The first query is 4x slower due to Metal shader compilation. Subsequent queries are fast.

**Impact:** N/A - Hardware/driver behavior, cannot optimize

## Recommendations

| Fix                                                                | Expected Impact          | Effort  | Risk                                               |
| ------------------------------------------------------------------ | ------------------------ | ------- | -------------------------------------------------- |
| Remove internal bucketing in MLX when texts are pre-sorted         | 30-40% embedding speedup | Low     | Low (NaN tested, doesn't occur with current model) |
| Add TOKENIZERS_PARALLELISM env var in semantic.py                  | Cleaner output           | Trivial | None                                               |
| Document that semantic.py sorting is required for optimal MLX perf | Maintenance              | Trivial | None                                               |

### Option A: Simple Fix (Recommended)

Modify `MLXEmbedder._embed_batch()` to skip bucketing when texts appear sorted:

```python
def _embed_batch(self, texts: list[str]) -> np.ndarray:
    self._ensure_loaded()

    if len(texts) == 1:
        return np.array([self._embed_one(texts[0])])

    # Skip bucketing if texts appear sorted (semantic.py sorts before calling)
    # Check first/last length as heuristic
    if len(texts[0]) <= len(texts[-1]):
        return self._embed_batch_safe(texts)

    # Existing bucketing logic for unsorted input...
```

### Option B: API Contract

Add `sorted: bool = False` parameter to `embed()`:

```python
def embed(self, texts: list[str], sorted: bool = False) -> np.ndarray:
    if sorted:
        # Direct batching, no bucketing
        return self._embed_batch_safe_all(texts)
    # Existing bucketing logic
```

### Option C: Remove Bucketing Entirely

Since NaN doesn't occur with snowflake-arctic-embed-s, remove the bucketing code entirely and rely on `semantic.py` sorting.

**Recommended:** Option C (simplest, tested, no user-facing change)

## Complexity Analysis

### `_embed_batch` (current)

```
Time: O(n log n) for sorting + O(b) model calls where b = number of buckets
Space: O(n) for bucket indices
```

With bucket_size=50 and diverse text lengths, b approaches n/bucket_size = n/50, making it O(n) model calls instead of O(n/batch_size).

### `_embed_batch_safe` (direct)

```
Time: O(1) model calls per batch of 64
Space: O(batch_size) intermediate arrays
```

## Memory Allocation Patterns

### Current Hot Path

```
1. _embed_batch: Creates bucket dict + indices lists (O(n))
2. Per bucket: Calls _embed_batch_safe
3. _embed_batch_safe: tokenizer output (numpy) -> mlx arrays -> model -> numpy output
4. Results list: Stores partial results before np.array() combines them
```

### Improved Hot Path

```
1. embed: Iterates in batch_size chunks
2. _embed_batch_safe: tokenizer -> mlx -> model -> numpy
3. all_embeddings list: One append per batch
4. np.vstack: Single allocation at end
```

The improved path has fewer intermediate allocations and better memory locality.

## I/O Patterns

No file I/O in embedding path. All I/O is in:

- Model loading (one-time, lazy)
- HuggingFace Hub cache check (one-time)

## Concurrency Analysis

### Thread Safety

- `_model_load_lock` protects monkey-patching during model load
- `_ensure_loaded` uses double-checked locking pattern
- Query cache uses dict (not thread-safe for concurrent writes)

**Recommendation:** Query cache is fine for CLI usage (single-threaded). For library usage with concurrent queries, consider `threading.Lock` or `functools.lru_cache`.

## Next Steps

1. Remove internal bucketing from MLXEmbedder (Option C) - measure before/after
2. Add `TOKENIZERS_PARALLELISM=false` to semantic.py multiprocessing section
3. Document the semantic.py -> embedder contract (texts must be sorted for optimal perf)
4. Consider adding a benchmark suite for regression testing

## Appendix: Test Data

```
Source: src/hygrep/*.py
Files: 8
Blocks: 101
Total chars: 177,408
Length distribution: min=106, max=28577, median=802

Bucket analysis (current bucket_size=50):
Number of buckets: 25
Most buckets contain 1-20 texts
```
