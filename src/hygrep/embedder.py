"""Embedder - ONNX text embeddings for semantic search."""

import logging
import os
import threading
from typing import Protocol, runtime_checkable

import numpy as np
import onnxruntime as ort
from huggingface_hub import hf_hub_download
from tokenizers import Tokenizer

from ._common import (
    BATCH_SIZE,
    DIMENSIONS,
    MAX_LENGTH,
    MODEL_FILE_FP16,
    MODEL_FILE_INT8,
    MODEL_REPO,
    MODEL_VERSION,
    QUERY_CACHE_MAX_SIZE,
    TOKENIZER_FILE,
    batch_embed,
    cached_embed_one,
    l2_normalize,
)

logger = logging.getLogger(__name__)

# Suppress ONNX Runtime warnings
ort.set_default_logger_severity(3)

# Re-export for backward compatibility
__all__ = [
    "BATCH_SIZE",
    "DIMENSIONS",
    "Embedder",
    "EmbedderProtocol",
    "MAX_LENGTH",
    "MODEL_REPO",
    "MODEL_VERSION",
    "QUERY_CACHE_MAX_SIZE",
    "QUERY_PREFIX",
    "get_embedder",
]


@runtime_checkable
class EmbedderProtocol(Protocol):
    """Protocol for text embedding backends."""

    @property
    def provider(self) -> str:
        """Return the execution provider name."""
        ...

    @property
    def batch_size(self) -> int:
        """Return the batch size for processing."""
        ...

    def embed(self, texts: list[str]) -> np.ndarray:
        """Generate embeddings for documents (for indexing).

        Args:
            texts: List of text strings to embed.

        Returns:
            numpy array of shape (len(texts), DIMENSIONS) with normalized embeddings.
        """
        ...

    def embed_one(self, text: str, use_cache: bool = True) -> np.ndarray:
        """Embed a single query string (for search).

        Args:
            text: Query text to embed.
            use_cache: Whether to use LRU cache for repeated queries.

        Returns:
            Normalized embedding vector of shape (DIMENSIONS,).
        """
        ...


# Provider priority: (primary, fallbacks, model_file)
_PROVIDER_CONFIGS = [
    (
        "TensorrtExecutionProvider",
        ["CUDAExecutionProvider", "CPUExecutionProvider"],
        MODEL_FILE_FP16,
    ),
    ("CUDAExecutionProvider", ["CPUExecutionProvider"], MODEL_FILE_FP16),
    ("MIGraphXExecutionProvider", ["CPUExecutionProvider"], MODEL_FILE_FP16),
]


def _get_best_provider_and_model() -> tuple[list[str], str]:
    """Detect best available provider and matching model file.

    Returns:
        Tuple of (providers, model_file).
    """
    available = set(ort.get_available_providers())

    for primary, fallbacks, model_file in _PROVIDER_CONFIGS:
        if primary in available:
            return [primary, *fallbacks], model_file

    # CPU fallback - use INT8 for smallest/fastest
    return ["CPUExecutionProvider"], MODEL_FILE_INT8


# MLX backend for Apple Silicon (uses Metal GPU)
# Requires: pip install mlx mlx-embeddings
_MLX_AVAILABLE = False
try:
    import platform

    if platform.system() == "Darwin" and platform.machine() == "arm64":
        from .mlx_embedder import MLX_AVAILABLE, MLXEmbedder

        _MLX_AVAILABLE = MLX_AVAILABLE
except ImportError as e:
    logger.debug("MLX backend unavailable: %s", e)

# Global embedder instance for caching across calls (useful for library usage)
_global_embedder: "EmbedderProtocol | None" = None
_global_lock = threading.Lock()


def get_embedder(cache_dir: str | None = None) -> EmbedderProtocol:
    """Get or create the global embedder instance.

    Auto-detects best backend:
    - macOS with MLX: MLXEmbedder (Metal GPU, ~1500 texts/sec)
    - Otherwise: ONNX Embedder (CPU INT8, ~330 texts/sec)

    Args:
        cache_dir: Cache directory for ONNX model files. Ignored when MLX
            backend is used (MLX uses HuggingFace Hub's default cache).

    Using a global instance enables query embedding caching across calls.
    Useful when hygrep is used as a library with multiple searches.
    """
    global _global_embedder
    with _global_lock:
        if _global_embedder is None:
            if _MLX_AVAILABLE:
                _global_embedder = MLXEmbedder()
            else:
                _global_embedder = Embedder(cache_dir=cache_dir)
        return _global_embedder


class Embedder:
    """Generate text embeddings using ONNX model."""

    def __init__(self, cache_dir: str | None = None):
        self.cache_dir = cache_dir
        self._session: ort.InferenceSession | None = None
        self._tokenizer: Tokenizer | None = None
        self._query_cache: dict[str, np.ndarray] = {}
        self._providers: list[str] = []
        self._model_file: str = MODEL_FILE_INT8
        self._input_names: list[str] = []
        self._output_names: list[str] = []

    @property
    def provider(self) -> str:
        """Return the active execution provider."""
        if self._session is not None:
            providers = self._session.get_providers()
            return providers[0] if providers else "CPUExecutionProvider"
        return "CPUExecutionProvider"

    @property
    def batch_size(self) -> int:
        """Return the batch size."""
        return BATCH_SIZE

    def _ensure_loaded(self) -> None:
        """Lazy load model and tokenizer."""
        if self._session is not None:
            return

        self._providers, self._model_file = _get_best_provider_and_model()
        model_path, tokenizer_path = self._download_model_files()
        self._load_tokenizer(tokenizer_path)
        self._load_session(model_path)

    def _download_model_files(self) -> tuple[str, str]:
        """Download model and tokenizer files from HuggingFace Hub."""
        try:
            model_path = hf_hub_download(
                repo_id=MODEL_REPO,
                filename=self._model_file,
                cache_dir=self.cache_dir,
            )
            tokenizer_path = hf_hub_download(
                repo_id=MODEL_REPO,
                filename=TOKENIZER_FILE,
                cache_dir=self.cache_dir,
            )
            return model_path, tokenizer_path
        except Exception as e:
            raise RuntimeError(
                f"Failed to download model: {e}\n"
                "Check your network connection and try: hhg model install"
            ) from e

    def _load_tokenizer(self, tokenizer_path: str) -> None:
        """Load and configure the tokenizer."""
        try:
            self._tokenizer = Tokenizer.from_file(tokenizer_path)
            self._tokenizer.enable_truncation(max_length=MAX_LENGTH)
            self._tokenizer.enable_padding(pad_id=0, pad_token="[PAD]")
        except Exception as e:
            raise RuntimeError(
                f"Failed to load tokenizer (may be corrupted): {e}\n"
                "Try reinstalling: hhg model install"
            ) from e

    def _load_session(self, model_path: str) -> None:
        """Load the ONNX inference session."""
        try:
            opts = ort.SessionOptions()
            opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
            opts.intra_op_num_threads = os.cpu_count() or 4

            self._session = ort.InferenceSession(
                model_path,
                sess_options=opts,
                providers=self._providers,
            )

            self._input_names = [i.name for i in self._session.get_inputs()]
            self._output_names = [o.name for o in self._session.get_outputs()]
        except Exception as e:
            raise RuntimeError(
                f"Failed to load model (may be corrupted): {e}\nTry reinstalling: hhg model install"
            ) from e

    def _embed_batch(self, texts: list[str]) -> np.ndarray:
        """Generate embeddings for a batch of texts (internal)."""
        self._ensure_loaded()
        assert self._tokenizer is not None
        assert self._session is not None

        # Tokenize
        encoded = self._tokenizer.encode_batch(texts)
        input_ids = np.array([e.ids for e in encoded], dtype=np.int64)
        attention_mask = np.array([e.attention_mask for e in encoded], dtype=np.int64)

        # Build inputs dict based on what model expects
        inputs = {"input_ids": input_ids, "attention_mask": attention_mask}
        if "token_type_ids" in self._input_names:
            inputs["token_type_ids"] = np.zeros_like(input_ids)

        # Run inference
        outputs = self._session.run(None, inputs)

        # Handle different output formats
        if "sentence_embedding" in self._output_names:
            idx = self._output_names.index("sentence_embedding")
            embeddings = outputs[idx]
        else:
            # CLS pooling - take first token (snowflake-arctic-embed uses CLS)
            token_embeddings = outputs[0]  # (batch, seq_len, hidden_size)
            embeddings = token_embeddings[:, 0, :]  # CLS token at position 0

        return l2_normalize(embeddings).astype(np.float32)

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
