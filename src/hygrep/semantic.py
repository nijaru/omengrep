"""Semantic search using embeddings + omendb vector database."""

import hashlib
import json
from pathlib import Path

from .embedder import DIMENSIONS, Embedder
from .extractor import ContextExtractor

# Try to import omendb
try:
    import omendb

    HAS_OMENDB = True
except ImportError:
    HAS_OMENDB = False


INDEX_DIR = ".hhg"
VECTORS_DIR = "vectors"
MANIFEST_FILE = "manifest.json"


class SemanticIndex:
    """Manages semantic search index using omendb."""

    def __init__(self, root: Path, cache_dir: str | None = None):
        if not HAS_OMENDB:
            raise ImportError(
                "omendb is required for semantic search. Install with: pip install omendb"
            )

        self.root = Path(root).resolve()
        self.index_dir = self.root / INDEX_DIR
        self.vectors_path = str(self.index_dir / VECTORS_DIR)
        self.manifest_path = self.index_dir / MANIFEST_FILE

        self.embedder = Embedder(cache_dir=cache_dir)
        self.extractor = ContextExtractor()

        self._db: "omendb.Database | None" = None

    def _ensure_db(self) -> "omendb.Database":
        """Open or create the vector database."""
        if self._db is None:
            self.index_dir.mkdir(parents=True, exist_ok=True)
            self._db = omendb.open(self.vectors_path, dimensions=DIMENSIONS)
        return self._db

    def _file_hash(self, path: Path) -> str:
        """Get hash of file content for change detection."""
        content = path.read_bytes()
        return hashlib.sha256(content).hexdigest()[:16]

    def _load_manifest(self) -> dict:
        """Load manifest of indexed files."""
        if self.manifest_path.exists():
            return json.loads(self.manifest_path.read_text())
        return {"files": {}}

    def _save_manifest(self, manifest: dict) -> None:
        """Save manifest."""
        self.manifest_path.write_text(json.dumps(manifest, indent=2))

    def index(
        self,
        files: dict[str, str],
        batch_size: int = 128,
        on_progress: callable = None,
    ) -> dict:
        """Index code files for semantic search.

        Args:
            files: Dict mapping file paths to content.
            batch_size: Number of code blocks to embed at once.
            on_progress: Callback(current, total, message) for progress updates.

        Returns:
            Stats dict with counts.
        """
        db = self._ensure_db()
        manifest = self._load_manifest()

        stats = {"files": 0, "blocks": 0, "skipped": 0, "errors": 0}

        # Collect all code blocks
        all_blocks = []
        for file_path, content in files.items():
            file_hash = hashlib.sha256(content.encode()).hexdigest()[:16]

            # Skip unchanged files
            if manifest["files"].get(file_path) == file_hash:
                stats["skipped"] += 1
                continue

            # Extract code blocks
            try:
                blocks = self.extractor.extract(file_path, query="", content=content)
                for block in blocks:
                    block_id = f"{file_path}:{block['start_line']}:{block['name']}"
                    all_blocks.append(
                        {
                            "id": block_id,
                            "file": file_path,
                            "file_hash": file_hash,
                            "block": block,
                            "text": f"{block['type']} {block['name']}\n{block['content']}",
                        }
                    )
                stats["files"] += 1
            except Exception:
                stats["errors"] += 1
                continue

        if not all_blocks:
            return stats

        # Batch embed and store
        total = len(all_blocks)
        for i in range(0, total, batch_size):
            batch = all_blocks[i : i + batch_size]
            texts = [b["text"] for b in batch]

            if on_progress:
                on_progress(i, total, f"Embedding {len(batch)} blocks...")

            # Generate embeddings
            embeddings = self.embedder.embed(texts)

            # Store in omendb
            items = []
            for j, block_info in enumerate(batch):
                items.append(
                    {
                        "id": block_info["id"],
                        "embedding": embeddings[j].tolist(),
                        "metadata": {
                            "file": block_info["file"],
                            "type": block_info["block"]["type"],
                            "name": block_info["block"]["name"],
                            "start_line": block_info["block"]["start_line"],
                            "end_line": block_info["block"]["end_line"],
                            "content": block_info["block"]["content"],
                        },
                    }
                )

            db.set(items)
            stats["blocks"] += len(batch)

            # Update manifest for these files
            for block_info in batch:
                manifest["files"][block_info["file"]] = block_info["file_hash"]

        if on_progress:
            on_progress(total, total, "Done")

        self._save_manifest(manifest)
        return stats

    def search(self, query: str, k: int = 10) -> list[dict]:
        """Search for code blocks similar to query.

        Args:
            query: Natural language query.
            k: Number of results to return.

        Returns:
            List of results with file, type, name, content, score.
        """
        db = self._ensure_db()

        # Embed query
        query_embedding = self.embedder.embed_one(query)

        # Search
        results = db.search(query_embedding.tolist(), k=k)

        # Format results
        output = []
        for r in results:
            meta = r.get("metadata", {})
            output.append(
                {
                    "file": meta.get("file", ""),
                    "type": meta.get("type", ""),
                    "name": meta.get("name", ""),
                    "start_line": meta.get("start_line", 0),
                    "end_line": meta.get("end_line", 0),
                    "content": meta.get("content", ""),
                    "score": 1.0 - r.get("distance", 0),  # Convert distance to similarity
                }
            )

        return output

    def is_indexed(self) -> bool:
        """Check if index exists."""
        return self.manifest_path.exists()

    def count(self) -> int:
        """Count indexed vectors."""
        if not self.is_indexed():
            return 0
        db = self._ensure_db()
        return db.count()

    def clear(self) -> None:
        """Delete the index."""
        import shutil

        if self.index_dir.exists():
            shutil.rmtree(self.index_dir)
