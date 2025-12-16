"""Embedder - ONNX text embeddings for semantic search."""

import os
import threading

import numpy as np
import onnxruntime as ort
from huggingface_hub import hf_hub_download
from tokenizers import Tokenizer

# Suppress ONNX Runtime warnings
ort.set_default_logger_severity(3)

# jina-embeddings-v2-base-code: trained on 31 programming languages
# INT8 quantized (~154MB)
MODEL_REPO = "nijaru/jina-code-int8"
MODEL_FILE = "model_int8.onnx"
TOKENIZER_FILE = "tokenizer.json"
DIMENSIONS = 768
MAX_LENGTH = 512  # jina supports 8K but 512 is enough for code blocks
BATCH_SIZE = 32


# Global embedder instance for caching across calls (useful for library usage)
_global_embedder: "Embedder | None" = None
_global_lock = threading.Lock()


def get_embedder(cache_dir: str | None = None) -> "Embedder":
    """Get or create the global embedder instance.

    Using a global instance enables query embedding caching across calls.
    Useful when hygrep is used as a library with multiple searches.
    """
    global _global_embedder
    with _global_lock:
        if _global_embedder is None:
            _global_embedder = Embedder(cache_dir=cache_dir)
        return _global_embedder


class Embedder:
    """Generate text embeddings using ONNX model."""

    def __init__(self, cache_dir: str | None = None):
        self.cache_dir = cache_dir
        self._session: ort.InferenceSession | None = None
        self._tokenizer: Tokenizer | None = None
        self._query_cache: dict[str, np.ndarray] = {}  # LRU cache for query embeddings

    @property
    def provider(self) -> str:
        """Return the execution provider (CPU only for now)."""
        return "CPUExecutionProvider"

    @property
    def batch_size(self) -> int:
        """Return the batch size."""
        return BATCH_SIZE

    def _ensure_loaded(self) -> None:
        """Lazy load model and tokenizer."""
        if self._session is not None:
            return

        try:
            # Download model files
            model_path = hf_hub_download(
                repo_id=MODEL_REPO,
                filename=MODEL_FILE,
                cache_dir=self.cache_dir,
            )
            tokenizer_path = hf_hub_download(
                repo_id=MODEL_REPO,
                filename=TOKENIZER_FILE,
                cache_dir=self.cache_dir,
            )
        except Exception as e:
            raise RuntimeError(
                f"Failed to download model: {e}\n"
                "Check your network connection and try: hhg model install"
            ) from e

        try:
            # Load tokenizer with truncation for efficiency
            self._tokenizer = Tokenizer.from_file(tokenizer_path)
            self._tokenizer.enable_truncation(max_length=MAX_LENGTH)
            self._tokenizer.enable_padding(pad_id=0, pad_token="[PAD]")
        except Exception as e:
            raise RuntimeError(
                f"Failed to load tokenizer (may be corrupted): {e}\n"
                "Try reinstalling: hhg model install"
            ) from e

        try:
            # Load ONNX model with CPU provider
            opts = ort.SessionOptions()
            opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
            opts.intra_op_num_threads = os.cpu_count() or 4

            self._session = ort.InferenceSession(
                model_path,
                sess_options=opts,
                providers=["CPUExecutionProvider"],
            )

            # Cache input/output names
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
            # Direct sentence embedding output
            idx = self._output_names.index("sentence_embedding")
            embeddings = outputs[idx]
        else:
            # Mean pooling over token embeddings
            token_embeddings = outputs[0]  # (batch, seq_len, hidden_size)
            mask_expanded = attention_mask[:, :, np.newaxis].astype(np.float32)
            sum_embeddings = np.sum(token_embeddings * mask_expanded, axis=1)
            sum_mask = np.sum(mask_expanded, axis=1)
            embeddings = sum_embeddings / np.maximum(sum_mask, 1e-9)

        # L2 normalize
        norms = np.linalg.norm(embeddings, axis=1, keepdims=True)
        embeddings = embeddings / np.maximum(norms, 1e-9)

        return embeddings.astype(np.float32)

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

        # Process in batches to avoid memory issues and reduce padding waste
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
        # Check cache first
        if use_cache and text in self._query_cache:
            return self._query_cache[text]

        embedding = self._embed_batch([text])[0]

        # Cache result (limit cache size to avoid memory bloat)
        if use_cache:
            if len(self._query_cache) >= 128:
                # Simple eviction: clear oldest half
                keys = list(self._query_cache.keys())[: len(self._query_cache) // 2]
                for k in keys:
                    del self._query_cache[k]
            self._query_cache[text] = embedding

        return embedding
