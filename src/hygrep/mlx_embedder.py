"""MLX Embedder - Apple Silicon GPU acceleration via mlx-embeddings."""

import logging
import threading

import numpy as np

from ._common import (
    BATCH_SIZE,
    MAX_LENGTH,
    MODEL_REPO,
    batch_embed,
    cached_embed_one,
    l2_normalize,
)

logger = logging.getLogger(__name__)

# MLX imports deferred for platform compatibility
try:
    import mlx.core as mx
    import mlx.nn as nn
    from mlx_embeddings.tokenizer_utils import load_tokenizer
    from mlx_embeddings.utils import get_model_path, load_model

    MLX_AVAILABLE = True
except ImportError:
    MLX_AVAILABLE = False

# Module-level lock for thread-safe model loading with monkey-patching
_model_load_lock = threading.Lock()


def _load_model_relaxed(path_or_hf_repo: str):
    """Load model with strict=False to handle models without pooler layer.

    snowflake-arctic-embed-s doesn't include pooler weights (trained with
    add_pooling_layer=False), but the BERT model class expects them.
    Using strict=False allows loading without the pooler weights.

    Thread-safe: Uses module-level lock to prevent race conditions when
    patching nn.Module.load_weights.
    """
    model_path = get_model_path(path_or_hf_repo)

    with _model_load_lock:
        original_load_weights = nn.Module.load_weights

        def load_weights_relaxed(self, file_or_weights, strict=True):
            return original_load_weights(self, file_or_weights, strict=False)

        nn.Module.load_weights = load_weights_relaxed
        try:
            model = load_model(model_path, lazy=False, path_to_repo=path_or_hf_repo)
        finally:
            nn.Module.load_weights = original_load_weights

    tokenizer = load_tokenizer(model_path)
    return model, tokenizer


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
            self._model, self._tokenizer_wrapper = _load_model_relaxed(MODEL_REPO)
            # Access underlying tokenizer - fragile private API, but necessary
            # See: https://github.com/Blaizzy/mlx-embeddings for updates
            if not hasattr(self._tokenizer_wrapper, "_tokenizer"):
                raise RuntimeError(
                    "mlx_embeddings API changed: _tokenizer attribute missing. "
                    "Please update hygrep or report this issue."
                )
            self._tokenizer = self._tokenizer_wrapper._tokenizer

    def _embed_one(self, text: str) -> np.ndarray:
        """Generate embedding for a single text using CLS pooling."""
        encoded = self._tokenizer(
            [text],
            padding=True,
            truncation=True,
            max_length=MAX_LENGTH,
            return_tensors="np",
        )

        input_ids = mx.array(encoded["input_ids"])
        attention_mask = mx.array(encoded["attention_mask"])

        outputs = self._model(input_ids, attention_mask=attention_mask)

        # CLS pooling - take first token (snowflake-arctic-embed uses CLS)
        embedding = np.array(outputs.last_hidden_state[0, 0, :])

        return l2_normalize(embedding).astype(np.float32)

    def _embed_batch_safe(self, texts: list[str]) -> np.ndarray:
        """Generate embeddings for texts of similar length using CLS pooling."""
        encoded = self._tokenizer(
            texts,
            padding=True,
            truncation=True,
            max_length=MAX_LENGTH,
            return_tensors="np",
        )

        input_ids = mx.array(encoded["input_ids"])
        attention_mask = mx.array(encoded["attention_mask"])

        outputs = self._model(input_ids, attention_mask=attention_mask)

        # CLS pooling - take first token (snowflake-arctic-embed uses CLS)
        embeddings = np.array(outputs.last_hidden_state[:, 0, :])

        return l2_normalize(embeddings).astype(np.float32)

    def _embed_batch(self, texts: list[str]) -> np.ndarray:
        """Generate embeddings for a batch of texts.

        Note: semantic.py pre-sorts texts by length before calling embed(),
        so internal bucketing is unnecessary with snowflake-arctic-embed-s.
        """
        self._ensure_loaded()

        if len(texts) == 1:
            return np.array([self._embed_one(texts[0])])

        try:
            embeddings = self._embed_batch_safe(texts)
            # Safety check for NaN (shouldn't occur with snowflake model)
            if np.isnan(embeddings).any():
                logger.debug("NaN in batch embeddings, falling back to individual")
                return np.array([self._embed_one(t) for t in texts])
            return embeddings
        except Exception as e:
            logger.debug("Batch embedding failed (%s), falling back to individual", e)
            return np.array([self._embed_one(t) for t in texts])

    def embed(self, texts: list[str]) -> np.ndarray:
        """Generate embeddings for documents (for indexing).

        Args:
            texts: List of text strings to embed.

        Returns:
            numpy array of shape (len(texts), DIMENSIONS) with normalized embeddings.
        """
        return batch_embed(texts, self.batch_size, self._embed_batch, self._ensure_loaded)

    def embed_one(self, text: str, use_cache: bool = True) -> np.ndarray:
        """Embed a single query string (for search).

        Args:
            text: Query text to embed.
            use_cache: Whether to use LRU cache for repeated queries (default True).

        Returns:
            Normalized embedding vector of shape (DIMENSIONS,).
        """
        return cached_embed_one(text, self._query_cache, self._embed_batch, use_cache)
