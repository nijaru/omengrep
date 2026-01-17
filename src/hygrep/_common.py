"""Common constants and utilities for embedders."""

from collections.abc import Callable

import numpy as np

# snowflake-arctic-embed-s: 33M params, 384 dims, Apache 2.0
# BERT-based architecture (e5-small-unsupervised), fast inference
# ViDoRe V3: competitive with 100M+ models despite small size
MODEL_REPO = "Snowflake/snowflake-arctic-embed-s"
MODEL_FILE_FP16 = "onnx/model_fp16.onnx"  # ~67 MB - for GPU
MODEL_FILE_INT8 = "onnx/model_int8.onnx"  # ~34 MB - for CPU
TOKENIZER_FILE = "tokenizer.json"
DIMENSIONS = 384
MAX_LENGTH = 512  # snowflake supports 512 tokens
BATCH_SIZE = 64  # smaller model allows larger batches
MODEL_VERSION = "snowflake-arctic-embed-s-v1"  # For manifest migration tracking

# Query prefix for optimal retrieval (documents don't need prefix)
QUERY_PREFIX = "Represent this sentence for searching relevant passages: "

# Query cache configuration
QUERY_CACHE_MAX_SIZE = 128

# L2 normalization epsilon to avoid division by zero
NORM_EPSILON = 1e-9


def evict_cache(cache: dict[str, np.ndarray]) -> None:
    """Evict oldest half of cache entries."""
    keys = list(cache.keys())[: len(cache) // 2]
    for k in keys:
        del cache[k]


def l2_normalize(embeddings: np.ndarray) -> np.ndarray:
    """L2 normalize embeddings along the last axis.

    Works for both single vectors (1D) and batches (2D).
    """
    axis = -1 if embeddings.ndim == 1 else 1
    norms = np.linalg.norm(embeddings, axis=axis, keepdims=True)
    return embeddings / np.maximum(norms, NORM_EPSILON)


def batch_embed(
    texts: list[str],
    batch_size: int,
    embed_batch_fn: Callable[[list[str]], np.ndarray],
    ensure_loaded_fn: Callable[[], None],
) -> np.ndarray:
    """Generate embeddings in batches.

    Args:
        texts: List of texts to embed.
        batch_size: Number of texts per batch.
        embed_batch_fn: Function to embed a single batch.
        ensure_loaded_fn: Function to ensure model is loaded.

    Returns:
        numpy array of shape (len(texts), DIMENSIONS) with normalized embeddings.
    """
    if not texts:
        return np.array([], dtype=np.float32).reshape(0, DIMENSIONS)

    ensure_loaded_fn()

    all_embeddings = []
    for i in range(0, len(texts), batch_size):
        batch = texts[i : i + batch_size]
        all_embeddings.append(embed_batch_fn(batch))

    return np.vstack(all_embeddings)


def cached_embed_one(
    text: str,
    query_cache: dict[str, np.ndarray],
    embed_batch_fn: Callable[[list[str]], np.ndarray],
    use_cache: bool = True,
) -> np.ndarray:
    """Embed a single query with optional caching.

    Args:
        text: Query text to embed.
        query_cache: Cache dict for query embeddings.
        embed_batch_fn: Function to embed a batch of texts.
        use_cache: Whether to use cache.

    Returns:
        Normalized embedding vector of shape (DIMENSIONS,).
    """
    if use_cache and text in query_cache:
        return query_cache[text]

    prefixed_text = QUERY_PREFIX + text
    embedding = embed_batch_fn([prefixed_text])[0]

    if use_cache:
        if len(query_cache) >= QUERY_CACHE_MAX_SIZE:
            evict_cache(query_cache)
        query_cache[text] = embedding

    return embedding
