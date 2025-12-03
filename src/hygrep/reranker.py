"""Reranker - ONNX cross-encoder for semantic ranking."""

import os
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

from .extractor import ContextExtractor

# Suppress ONNX Runtime warnings (CoreML capability messages, etc.)
ort.set_default_logger_severity(3)  # ERROR level only

MODEL_REPO = "mixedbread-ai/mxbai-rerank-xsmall-v1"
MODEL_FILE = "onnx/model_quantized.onnx"
TOKENIZER_FILE = "tokenizer.json"


def get_cache_dir() -> Path | None:
    """Get custom cache directory if configured.

    Priority:
    1. HYGREP_CACHE_DIR env var
    2. cache_dir from config file
    3. None (use default HF cache)

    Returns:
        Custom cache path, or None to use shared HF cache
    """
    if env_dir := os.environ.get("HYGREP_CACHE_DIR"):
        return Path(env_dir).expanduser()

    # Check config file
    config_path = Path.home() / ".config" / "hygrep" / "config.toml"
    if config_path.exists():
        try:
            try:
                import tomllib
            except ImportError:
                import tomli as tomllib
            with open(config_path, "rb") as f:
                config = tomllib.load(f)
            if cache_dir := config.get("cache_dir"):
                return Path(cache_dir).expanduser()
        except Exception:
            pass

    return None  # Use shared HF cache


def _setup_hf_cache() -> Path:
    """Configure HuggingFace cache if custom dir specified.

    Returns:
        The effective cache directory
    """
    custom_dir = get_cache_dir()
    if custom_dir:
        os.environ["HF_HOME"] = str(custom_dir)
        return custom_dir

    # Return default HF cache location
    return Path(os.environ.get("HF_HOME", Path.home() / ".cache" / "huggingface"))


def get_model_paths() -> tuple[str, str]:
    """Get paths to model files, downloading on first use if needed.

    Uses local cache when available (fast, offline). Downloads automatically
    on first use only.

    Returns:
        Tuple of (model_path, tokenizer_path)
    """
    _setup_hf_cache()

    from huggingface_hub import hf_hub_download
    from huggingface_hub.utils import LocalEntryNotFoundError

    try:
        # Try local cache first (fast, no network)
        model_path = hf_hub_download(
            repo_id=MODEL_REPO,
            filename=MODEL_FILE,
            local_files_only=True,
        )
        tokenizer_path = hf_hub_download(
            repo_id=MODEL_REPO,
            filename=TOKENIZER_FILE,
            local_files_only=True,
        )
    except LocalEntryNotFoundError:
        # First use - download model
        import sys

        print(f"Downloading model ({MODEL_REPO})...", file=sys.stderr)
        model_path = hf_hub_download(
            repo_id=MODEL_REPO,
            filename=MODEL_FILE,
        )
        tokenizer_path = hf_hub_download(
            repo_id=MODEL_REPO,
            filename=TOKENIZER_FILE,
        )
        print("Model ready.", file=sys.stderr)

    return model_path, tokenizer_path


def download_model(force: bool = False, quiet: bool = False) -> tuple[str, str]:
    """Download model files from HuggingFace Hub.

    Args:
        force: Force re-download even if cached
        quiet: Suppress progress messages

    Returns:
        Tuple of (model_path, tokenizer_path)
    """
    _setup_hf_cache()

    from huggingface_hub import hf_hub_download

    if not quiet:
        print(f"Downloading model from {MODEL_REPO}...", file=sys.stderr)

    model_path = hf_hub_download(
        repo_id=MODEL_REPO,
        filename=MODEL_FILE,
        force_download=force,
    )
    tokenizer_path = hf_hub_download(
        repo_id=MODEL_REPO,
        filename=TOKENIZER_FILE,
        force_download=force,
    )

    if not quiet:
        size_mb = os.path.getsize(model_path) / 1024 / 1024
        print(f"Model ready ({size_mb:.0f}MB)", file=sys.stderr)

    return model_path, tokenizer_path


def get_model_info() -> dict:
    """Get information about the cached model.

    Returns:
        Dict with model info (installed, path, size, repo)
    """
    cache_dir = get_cache_dir()
    _setup_hf_cache()

    info = {
        "repo": MODEL_REPO,
        "cache_dir": str(cache_dir) if cache_dir else None,
        "installed": False,
        "model_path": None,
        "size_mb": None,
    }

    try:
        from huggingface_hub import try_to_load_from_cache

        model_path = try_to_load_from_cache(
            repo_id=MODEL_REPO,
            filename=MODEL_FILE,
        )
        tokenizer_path = try_to_load_from_cache(
            repo_id=MODEL_REPO,
            filename=TOKENIZER_FILE,
        )

        # Check both model and tokenizer are cached (isinstance guards against
        # _CACHED_NO_EXIST sentinel which is truthy but not a path)
        if (
            isinstance(model_path, str)
            and isinstance(tokenizer_path, str)
            and os.path.exists(model_path)
            and os.path.exists(tokenizer_path)
        ):
            info["installed"] = True
            info["model_path"] = model_path
            info["size_mb"] = round(os.path.getsize(model_path) / 1024 / 1024, 1)
    except Exception:
        pass

    return info


def clean_model_cache() -> bool:
    """Remove cached model files.

    Uses HuggingFace's cache management API to delete only the hygrep model,
    leaving other cached models untouched.

    Returns:
        True if cache was cleaned, False if nothing to clean
    """
    _setup_hf_cache()

    try:
        from huggingface_hub import scan_cache_dir

        cache_info = scan_cache_dir()
        deleted = False

        for repo in cache_info.repos:
            if repo.repo_id == MODEL_REPO:
                # Delete all revisions of our model
                for revision in repo.revisions:
                    strategy = cache_info.delete_revisions(revision.commit_hash)
                    strategy.execute()
                    deleted = True

        return deleted
    except Exception:
        # Don't fallback to rmtree - too dangerous for shared cache
        return False


def get_execution_providers() -> list:
    """Auto-detect best available execution provider."""
    available = ort.get_available_providers()

    # Prefer GPU providers in order
    # Note: CoreML skipped - causes "Context leak" spam on macOS and
    # CPU is fast enough for this model size (~2s/100 candidates)
    preferred = [
        'CUDAExecutionProvider',      # NVIDIA GPU (Linux/Windows)
        'DmlExecutionProvider',       # DirectML (Windows)
        'CPUExecutionProvider',       # Fallback (fast enough for small model)
    ]

    providers = []
    for p in preferred:
        if p in available:
            providers.append(p)

    return providers if providers else ['CPUExecutionProvider']


class Reranker:
    """Cross-encoder reranker using ONNX Runtime."""

    def __init__(self, num_threads: int = 4):
        model_path, tokenizer_path = get_model_paths()

        self.extractor = ContextExtractor()

        # Load tokenizer
        self.tokenizer = Tokenizer.from_file(tokenizer_path)
        self.tokenizer.enable_padding(length=512)
        self.tokenizer.enable_truncation(max_length=512)

        # Load model with optimized session options
        sess_options = ort.SessionOptions()
        sess_options.intra_op_num_threads = num_threads
        sess_options.inter_op_num_threads = 1
        sess_options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL

        # Auto-detect GPU providers with fallback
        providers = get_execution_providers()
        try:
            self.session = ort.InferenceSession(
                model_path, sess_options, providers=providers
            )
        except Exception:
            # GPU provider failed, fall back to CPU silently
            self.session = ort.InferenceSession(
                model_path, sess_options, providers=["CPUExecutionProvider"]
            )
        self.provider = self.session.get_providers()[0]

    def search(
        self, query: str, file_contents: dict, top_k: int = 10, max_candidates: int = 100
    ) -> list:
        """
        Full pipeline: Extract -> Rerank.

        Args:
            query: Search query
            file_contents: Dict mapping file paths to contents
            top_k: Number of results to return
            max_candidates: Max candidates to rerank (caps inference cost)

        Returns:
            List of ranked results
        """
        candidates = []

        # 1. Extraction Phase - parallel tree-sitter parsing
        def extract_file(item: tuple) -> list:
            path, content = item
            return [
                (path, block)
                for block in self.extractor.extract(path, query, content=content)
            ]

        items = list(file_contents.items())
        with ThreadPoolExecutor(max_workers=min(4, len(items))) as executor:
            all_blocks = list(executor.map(extract_file, items))

        # Flatten and build candidates
        for file_blocks in all_blocks:
            for path, block in file_blocks:
                text_to_score = f"{block['type']} {block['name']}: {block['content']}"
                candidates.append({
                    "file": path,
                    "type": block["type"],
                    "name": block["name"],
                    "start_line": block["start_line"],
                    "content": block["content"],
                    "score_text": text_to_score,
                })

        if not candidates:
            return []

        # Cap candidates to limit inference cost
        if len(candidates) > max_candidates:
            # Sort by content length (shorter = more focused) as heuristic
            candidates.sort(key=lambda x: len(x["score_text"]))
            candidates = candidates[:max_candidates]

        # 2. Reranking Phase (Batched)
        BATCH_SIZE = 32
        all_logits = []
        input_names = [x.name for x in self.session.get_inputs()]
        use_token_type_ids = len(input_names) > 2

        for i in range(0, len(candidates), BATCH_SIZE):
            batch = candidates[i : i + BATCH_SIZE]
            pairs = [(query, c["score_text"]) for c in batch]

            encodings = self.tokenizer.encode_batch(pairs)

            input_ids = np.array([e.ids for e in encodings], dtype=np.int64)
            attention_mask = np.array(
                [e.attention_mask for e in encodings], dtype=np.int64
            )

            inputs = {
                input_names[0]: input_ids,
                input_names[1]: attention_mask,
            }
            if use_token_type_ids:
                token_type_ids = np.array(
                    [e.type_ids for e in encodings], dtype=np.int64
                )
                inputs[input_names[2]] = token_type_ids

            res = self.session.run(None, inputs)
            all_logits.extend(res[0].flatten())

        # 3. Score and sort
        scored_results = []
        for i, score in enumerate(all_logits):
            cand = candidates[i]
            # Sigmoid normalization
            normalized_score = 1.0 / (1.0 + np.exp(-float(score)))
            cand["score"] = round(float(normalized_score), 4)
            del cand["score_text"]
            scored_results.append(cand)

        scored_results.sort(key=lambda x: x["score"], reverse=True)

        return scored_results[:top_k]
