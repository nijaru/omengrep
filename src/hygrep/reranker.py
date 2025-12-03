"""Reranker - ONNX cross-encoder for semantic ranking."""

import json
import os
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer
from concurrent.futures import ThreadPoolExecutor

from .extractor import ContextExtractor


MODEL_REPO = "mixedbread-ai/mxbai-rerank-xsmall-v1"
MODEL_FILE = "onnx/model_quantized.onnx"
TOKENIZER_FILE = "tokenizer.json"
MODEL_MIN_SIZE = 80_000_000  # ~83MB, sanity check


def ensure_models(model_path: str, tokenizer_path: str):
    """Download models if not present or corrupted."""
    # Check if files exist and are valid
    if _models_valid(model_path, tokenizer_path):
        return

    print("Downloading AI models (~83MB)...", file=sys.stderr)
    try:
        from huggingface_hub import hf_hub_download
        import shutil

        model_dir = os.path.dirname(model_path)
        if model_dir and not os.path.exists(model_dir):
            os.makedirs(model_dir)

        print(f"Fetching {MODEL_REPO}...", file=sys.stderr)
        m_path = hf_hub_download(repo_id=MODEL_REPO, filename=MODEL_FILE)
        t_path = hf_hub_download(repo_id=MODEL_REPO, filename=TOKENIZER_FILE)

        shutil.copy(m_path, model_path)
        shutil.copy(t_path, tokenizer_path)

        # Verify download
        if not _models_valid(model_path, tokenizer_path):
            raise RuntimeError("Downloaded files appear corrupted")

        print("Download complete.", file=sys.stderr)
    except Exception as e:
        # Clean up partial downloads
        for f in [model_path, tokenizer_path]:
            if os.path.exists(f):
                try:
                    os.remove(f)
                except OSError:
                    pass
        raise RuntimeError(f"Failed to download models: {e}") from e


def _models_valid(model_path: str, tokenizer_path: str) -> bool:
    """Check if model files exist and appear valid."""
    if not os.path.exists(model_path) or not os.path.exists(tokenizer_path):
        return False
    # Sanity check: model should be ~83MB
    if os.path.getsize(model_path) < MODEL_MIN_SIZE:
        return False
    # Tokenizer should be valid JSON
    try:
        with open(tokenizer_path, 'r') as f:
            json.load(f)
    except (json.JSONDecodeError, OSError):
        return False
    return True


def get_execution_providers() -> list:
    """Auto-detect best available execution provider."""
    available = ort.get_available_providers()

    # Prefer GPU providers in order
    preferred = [
        'CUDAExecutionProvider',      # NVIDIA GPU (Linux/Windows)
        'CoreMLExecutionProvider',    # Apple Neural Engine / GPU
        'DmlExecutionProvider',       # DirectML (Windows)
        'CPUExecutionProvider',       # Fallback
    ]

    providers = []
    for p in preferred:
        if p in available:
            providers.append(p)

    return providers if providers else ['CPUExecutionProvider']


class Reranker:
    """Cross-encoder reranker using ONNX Runtime."""

    def __init__(self, model_dir: str = "models", num_threads: int = 4):
        model_path = os.path.join(model_dir, "reranker.onnx")
        tokenizer_path = os.path.join(model_dir, "tokenizer.json")

        ensure_models(model_path, tokenizer_path)

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
            self.session = ort.InferenceSession(model_path, sess_options, providers=providers)
        except Exception:
            # GPU provider failed, fall back to CPU silently
            self.session = ort.InferenceSession(model_path, sess_options, providers=['CPUExecutionProvider'])
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
            return [(path, block) for block in self.extractor.extract(path, query, content=content)]

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

        for i in range(0, len(candidates), BATCH_SIZE):
            batch = candidates[i : i + BATCH_SIZE]
            pairs = [(query, c["score_text"]) for c in batch]

            encodings = self.tokenizer.encode_batch(pairs)

            input_ids = np.array([e.ids for e in encodings], dtype=np.int64)
            attention_mask = np.array([e.attention_mask for e in encodings], dtype=np.int64)
            token_type_ids = np.array([e.type_ids for e in encodings], dtype=np.int64)

            input_names = [x.name for x in self.session.get_inputs()]
            inputs = {
                input_names[0]: input_ids,
                input_names[1]: attention_mask,
            }
            if len(input_names) > 2:
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
