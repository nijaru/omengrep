"""MLX Embedder - Apple Silicon GPU acceleration via mlx-embeddings."""

import logging
import threading

import numpy as np

from .embedder import DIMENSIONS, MAX_LENGTH, MODEL_REPO

logger = logging.getLogger(__name__)

# MLX imports deferred for platform compatibility
try:
    import mlx.core as mx
    from mlx_embeddings import load

    MLX_AVAILABLE = True
except ImportError:
    MLX_AVAILABLE = False

# MLX batch size (larger than ONNX due to Metal efficiency)
BATCH_SIZE = 64


class MLXEmbedder:
    """Generate text embeddings using MLX on Apple Silicon."""

    def __init__(self):
        if not MLX_AVAILABLE:
            raise RuntimeError("MLX not available. Install with: pip install mlx mlx-embeddings")
        self._model = None
        self._tokenizer = None
        self._lock = threading.Lock()
        self._query_cache: dict[str, np.ndarray] = {}

    @property
    def provider(self) -> str:
        """Return the execution provider name."""
        return "MLXExecutionProvider"

    @property
    def batch_size(self) -> int:
        """Return the batch size."""
        return BATCH_SIZE

    def _ensure_loaded(self) -> None:
        """Lazy load model and tokenizer."""
        if self._model is not None:
            return

        with self._lock:
            if self._model is not None:
                return
            self._model, self._tokenizer_wrapper = load(MODEL_REPO)
            # Access underlying tokenizer - check for API changes
            if not hasattr(self._tokenizer_wrapper, "_tokenizer"):
                raise RuntimeError(
                    "mlx_embeddings API changed: _tokenizer attribute missing. "
                    "Please update hygrep or report this issue."
                )
            self._tokenizer = self._tokenizer_wrapper._tokenizer

    def _embed_one(self, text: str) -> np.ndarray:
        """Generate embedding for a single text."""
        # Tokenize
        encoded = self._tokenizer(
            [text],
            padding=True,
            truncation=True,
            max_length=MAX_LENGTH,
            return_tensors="np",
        )

        # Convert to MLX arrays
        input_ids = mx.array(encoded["input_ids"])
        attention_mask = mx.array(encoded["attention_mask"])

        # Run model
        outputs = self._model(input_ids, attention_mask=attention_mask)

        # Get sentence embedding and normalize
        embedding = np.array(outputs.text_embeds[0])
        norm = np.linalg.norm(embedding)
        if norm > 1e-9:
            embedding = embedding / norm

        return embedding.astype(np.float32)

    def _embed_batch_safe(self, texts: list[str]) -> np.ndarray:
        """Generate embeddings for texts of similar length."""
        # Tokenize using transformers tokenizer
        encoded = self._tokenizer(
            texts,
            padding=True,
            truncation=True,
            max_length=MAX_LENGTH,
            return_tensors="np",
        )

        # Convert to MLX arrays
        input_ids = mx.array(encoded["input_ids"])
        attention_mask = mx.array(encoded["attention_mask"])

        # Run model
        outputs = self._model(input_ids, attention_mask=attention_mask)

        # Get sentence embeddings
        embeddings = np.array(outputs.text_embeds)

        # L2 normalize
        norms = np.linalg.norm(embeddings, axis=1, keepdims=True)
        embeddings = embeddings / np.maximum(norms, 1e-9)

        return embeddings.astype(np.float32)

    def _embed_batch(self, texts: list[str]) -> np.ndarray:
        """Generate embeddings for a batch of texts.

        Groups texts by similar token length to avoid MLX batching bug
        that causes NaN when texts have very different lengths.
        """
        self._ensure_loaded()

        if len(texts) == 1:
            return np.array([self._embed_one(texts[0])])

        # Get approximate token counts (chars / 4 is rough estimate)
        lengths = [(i, len(t) // 4) for i, t in enumerate(texts)]

        # Group by similar lengths (buckets of ~50 tokens for less padding waste)
        bucket_size = 50
        buckets: dict[int, list[int]] = {}
        for idx, length in lengths:
            bucket = length // bucket_size
            buckets.setdefault(bucket, []).append(idx)

        # Process each bucket
        results = [None] * len(texts)
        for bucket_indices in buckets.values():
            bucket_texts = [texts[i] for i in bucket_indices]
            try:
                embeddings = self._embed_batch_safe(bucket_texts)
                # Check for NaN - fall back to individual if needed
                if np.isnan(embeddings).any():
                    logger.debug("NaN in batch embeddings, falling back to individual")
                    embeddings = np.array([self._embed_one(t) for t in bucket_texts])
            except Exception as e:
                logger.debug("Batch embedding failed (%s), falling back to individual", e)
                embeddings = np.array([self._embed_one(t) for t in bucket_texts])

            for j, idx in enumerate(bucket_indices):
                results[idx] = embeddings[j]

        return np.array(results)

    def embed(self, texts: list[str]) -> np.ndarray:
        """Generate embeddings for documents (for indexing).

        Args:
            texts: List of text strings to embed.

        Returns:
            numpy array of shape (len(texts), DIMENSIONS) with normalized embeddings.
        """
        if not texts:
            return np.array([], dtype=np.float32).reshape(0, DIMENSIONS)

        self._ensure_loaded()
        batch_size = self.batch_size

        # Process in batches
        all_embeddings = []
        for i in range(0, len(texts), batch_size):
            batch = texts[i : i + batch_size]
            all_embeddings.append(self._embed_batch(batch))

        return np.vstack(all_embeddings)

    def embed_one(self, text: str, use_cache: bool = True) -> np.ndarray:
        """Embed a single query string (for search).

        Args:
            text: Query text to embed.
            use_cache: Whether to use LRU cache for repeated queries (default True).

        Returns:
            Normalized embedding vector of shape (DIMENSIONS,).
        """
        if use_cache and text in self._query_cache:
            return self._query_cache[text]

        embedding = self._embed_batch([text])[0]

        if use_cache:
            if len(self._query_cache) >= 128:
                keys = list(self._query_cache.keys())[: len(self._query_cache) // 2]
                for k in keys:
                    del self._query_cache[k]
            self._query_cache[text] = embedding

        return embedding
