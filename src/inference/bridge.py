import os
import json
import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer
from concurrent.futures import ThreadPoolExecutor
from src.inference.context import ContextExtractor

class SmartSearcher:
    def __init__(self, model_path: str, tokenizer_path: str):
        self.extractor = ContextExtractor()
        
        # Load Tokenizer
        try:
            self.tokenizer = Tokenizer.from_file(tokenizer_path)
            self.tokenizer.enable_padding(length=512)
            self.tokenizer.enable_truncation(max_length=512)
        except Exception as e:
            print(f"Error loading tokenizer: {e}")
            raise e

        # Load Model
        try:
            self.session = ort.InferenceSession(model_path)
        except Exception as e:
            print(f"Error loading model: {e}")
            raise e

    def search(self, query: str, file_contents: dict[str, str], top_k: int = 10) -> str:
        """
        Full pipeline: Extract -> Rerank.
        Accepts pre-read file contents to avoid double-reads.
        Returns a JSON string of results.
        """
        candidates = []

        # 1. Extraction Phase - parallel tree-sitter parsing
        def extract_file(item: tuple[str, str]) -> list[dict]:
            path, content = item
            return [(path, block) for block in self.extractor.extract(path, query, content=content)]

        # Use ThreadPoolExecutor for parallel extraction
        items = list(file_contents.items())
        with ThreadPoolExecutor(max_workers=min(4, len(items))) as executor:
            all_blocks = list(executor.map(extract_file, items))

        # Flatten results and build candidates
        for file_blocks in all_blocks:
            for path, block in file_blocks:
                text_to_score = f"{block['type']} {block['name']}: {block['content']}"
                candidates.append({
                    "file": path,
                    "type": block['type'],
                    "name": block['name'],
                    "start_line": block['start_line'],
                    "content": block['content'],
                    "score_text": text_to_score
                })

        if not candidates:
            return json.dumps([])

        # 2. Reranking Phase (Batched)
        BATCH_SIZE = 32
        all_logits = []
        
        for i in range(0, len(candidates), BATCH_SIZE):
            batch = candidates[i:i+BATCH_SIZE]
            pairs = [(query, c["score_text"]) for c in batch]
            
            encodings = self.tokenizer.encode_batch(pairs)
            
            input_ids = np.array([e.ids for e in encodings], dtype=np.int64)
            attention_mask = np.array([e.attention_mask for e in encodings], dtype=np.int64)
            token_type_ids = np.array([e.type_ids for e in encodings], dtype=np.int64)
            
            input_names = [x.name for x in self.session.get_inputs()]
            inputs = {
                input_names[0]: input_ids,
                input_names[1]: attention_mask
            }
            if len(input_names) > 2:
                inputs[input_names[2]] = token_type_ids
            
            res = self.session.run(None, inputs)
            all_logits.extend(res[0].flatten())
        
        # 3. Merge Scores (normalize with sigmoid for 0-1 range)
        scored_results = []
        for i, score in enumerate(all_logits):
            cand = candidates[i]
            # Sigmoid normalization: 1 / (1 + exp(-x))
            normalized_score = 1.0 / (1.0 + np.exp(-float(score)))
            cand["score"] = round(float(normalized_score), 4)
            del cand["score_text"]  # Remove internal field
            scored_results.append(cand)

        # Sort by score desc
        scored_results.sort(key=lambda x: x["score"], reverse=True)

        return json.dumps(scored_results[:top_k])

# Global instance for easy interop (optional, or instantiate in Mojo)
_searcher = None

def ensure_models(model_path: str, tokenizer_path: str):
    if os.path.exists(model_path) and os.path.exists(tokenizer_path):
        return

    print("First run detected: Downloading AI models (40MB)...")
    try:
        from huggingface_hub import hf_hub_download
        import shutil
        
        MODEL_REPO = "mixedbread-ai/mxbai-rerank-xsmall-v1"
        MODEL_FILE = "onnx/model_quantized.onnx" 
        TOKENIZER_FILE = "tokenizer.json"
        
        # Ensure dir exists
        model_dir = os.path.dirname(model_path)
        if model_dir and not os.path.exists(model_dir):
            os.makedirs(model_dir)
            
        # Download
        print(f"Fetching {MODEL_REPO}...")
        m_path = hf_hub_download(repo_id=MODEL_REPO, filename=MODEL_FILE)
        t_path = hf_hub_download(repo_id=MODEL_REPO, filename=TOKENIZER_FILE)
        
        shutil.copy(m_path, model_path)
        shutil.copy(t_path, tokenizer_path)
        print("Download complete.")
    except Exception as e:
        print(f"Failed to download models: {e}")

def init_searcher(model_path: str, tokenizer_path: str):
    ensure_models(model_path, tokenizer_path)
    global _searcher
    _searcher = SmartSearcher(model_path, tokenizer_path)

def run_search(query: str, file_contents: dict[str, str], top_k: int = 10) -> str:
    if _searcher is None:
        return json.dumps({"error": "Searcher not initialized"})
    return _searcher.search(query, file_contents, top_k)