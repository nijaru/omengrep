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
MANIFEST_VERSION = 3  # v3: relative paths


def find_index_root(search_path: Path) -> tuple[Path, Path | None]:
    """Walk up directory tree to find existing index.

    Args:
        search_path: The directory being searched.

    Returns:
        Tuple of (index_root, existing_index_dir or None).
        - index_root: Where index should be (search_path if no existing found)
        - existing_index_dir: Path to existing .hhg/ if found, else None
    """
    search_path = Path(search_path).resolve()

    # Walk up looking for existing .hhg/
    current = search_path
    while current != current.parent:  # Stop at filesystem root
        index_dir = current / INDEX_DIR
        if (index_dir / MANIFEST_FILE).exists():
            return (current, index_dir)
        current = current.parent

    # No existing index found, will create at search root
    return (search_path, None)


class SemanticIndex:
    """Manages semantic search index using omendb."""

    def __init__(
        self,
        root: Path,
        search_scope: Path | None = None,
        cache_dir: str | None = None,
    ):
        """Initialize semantic index.

        Args:
            root: Index root directory (where .hhg/ lives).
            search_scope: Optional subdirectory to filter results to.
                         If None, returns all results.
            cache_dir: Optional cache directory for embeddings model.
        """
        if not HAS_OMENDB:
            raise ImportError(
                "omendb is required for semantic search. Install with: pip install omendb"
            )

        self.root = Path(root).resolve()
        self.index_dir = self.root / INDEX_DIR
        self.vectors_path = str(self.index_dir / VECTORS_DIR)
        self.manifest_path = self.index_dir / MANIFEST_FILE

        # Search scope for filtering results (relative to root)
        self.search_scope: str | None = None
        if search_scope:
            scope_path = Path(search_scope).resolve()
            if scope_path != self.root:
                try:
                    self.search_scope = str(scope_path.relative_to(self.root))
                except ValueError:
                    pass  # search_scope not under root, ignore

        self.embedder = Embedder(cache_dir=cache_dir)
        self.extractor = ContextExtractor()

        self._db: "omendb.Database | None" = None

    def _to_relative(self, abs_path: str) -> str:
        """Convert absolute path to relative (for storage)."""
        try:
            return str(Path(abs_path).relative_to(self.root))
        except ValueError:
            return abs_path  # Already relative or not under root

    def _to_absolute(self, rel_path: str) -> str:
        """Convert relative path to absolute (for display)."""
        if Path(rel_path).is_absolute():
            return rel_path
        return str(self.root / rel_path)

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
        """Load manifest of indexed files.

        Manifest format v3:
            {"version": 3, "files": {"rel/path": {"hash": "abc123", "blocks": ["id1", "id2"]}}}

        Migrates from older formats on load.
        """
        if self.manifest_path.exists():
            data = json.loads(self.manifest_path.read_text())
            version = data.get("version", 1)
            files = data.get("files", {})

            # Migrate v1 -> v2: hash string -> dict
            if version < 2:
                for path, value in list(files.items()):
                    if isinstance(value, str):
                        files[path] = {"hash": value, "blocks": []}

            # Migrate v2 -> v3: absolute paths -> relative paths
            if version < 3:
                new_files = {}
                for path, value in files.items():
                    rel_path = self._to_relative(path)
                    new_files[rel_path] = value
                    # Also update block IDs to use relative paths
                    if isinstance(value, dict) and "blocks" in value:
                        value["blocks"] = [
                            b.replace(path, rel_path) if path in b else b for b in value["blocks"]
                        ]
                files = new_files
                data["files"] = files
                data["version"] = MANIFEST_VERSION

            return data
        return {"version": MANIFEST_VERSION, "files": {}}

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

        stats = {"files": 0, "blocks": 0, "skipped": 0, "errors": 0, "deleted": 0}

        # Collect all code blocks
        all_blocks = []
        files_to_update = {}  # Track which files we're updating

        for file_path, content in files.items():
            # Convert to relative path for storage
            rel_path = self._to_relative(file_path)
            file_hash = hashlib.sha256(content.encode()).hexdigest()[:16]

            # Skip unchanged files (check by relative path)
            file_entry = manifest["files"].get(rel_path, {})
            if isinstance(file_entry, dict) and file_entry.get("hash") == file_hash:
                stats["skipped"] += 1
                continue

            # Delete old vectors for this file before re-indexing
            old_blocks = file_entry.get("blocks", []) if isinstance(file_entry, dict) else []
            if old_blocks:
                db.delete(old_blocks)
                stats["deleted"] += len(old_blocks)

            # Extract code blocks (use original path for extraction)
            try:
                blocks = self.extractor.extract(file_path, query="", content=content)
                new_block_ids = []
                for block in blocks:
                    # Use relative path in block ID for portability
                    block_id = f"{rel_path}:{block['start_line']}:{block['name']}"
                    new_block_ids.append(block_id)
                    all_blocks.append(
                        {
                            "id": block_id,
                            "file": rel_path,  # Store relative path
                            "file_hash": file_hash,
                            "block": block,
                            "text": f"{block['type']} {block['name']}\n{block['content']}",
                        }
                    )
                files_to_update[rel_path] = {"hash": file_hash, "blocks": new_block_ids}
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
                        "vector": embeddings[j].tolist(),
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

        # Update manifest with new file entries
        for file_path, file_info in files_to_update.items():
            manifest["files"][file_path] = file_info

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
            File paths are absolute.
        """
        db = self._ensure_db()

        # Embed query
        query_embedding = self.embedder.embed_one(query)

        # If filtering by scope, request more results to ensure we get k after filtering
        search_k = k * 3 if self.search_scope else k
        results = db.search(query_embedding.tolist(), k=search_k)

        # Format results
        output = []
        for r in results:
            meta = r.get("metadata", {})
            rel_file = meta.get("file", "")

            # Filter by search scope if set
            if self.search_scope and not rel_file.startswith(self.search_scope):
                continue

            # Convert to absolute path for display
            abs_file = self._to_absolute(rel_file)

            output.append(
                {
                    "file": abs_file,
                    "type": meta.get("type", ""),
                    "name": meta.get("name", ""),
                    "line": meta.get("start_line", 0),
                    "end_line": meta.get("end_line", 0),
                    "content": meta.get("content", ""),
                    "score": (2.0 - r.get("distance", 0))
                    / 2.0,  # Cosine distance 0-2 → similarity 0-1
                }
            )

            # Stop once we have enough results
            if len(output) >= k:
                break

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

    def get_stale_files(self, files: dict[str, str]) -> tuple[list[str], list[str]]:
        """Find files that need reindexing.

        Args:
            files: Dict mapping file paths (absolute) to content.

        Returns:
            Tuple of (changed_files, deleted_files) - original paths from input.
        """
        manifest = self._load_manifest()
        indexed_files = manifest.get("files", {})

        # Build mapping of relative -> original path
        rel_to_orig = {self._to_relative(p): p for p in files.keys()}

        changed = []
        for file_path, content in files.items():
            rel_path = self._to_relative(file_path)
            file_hash = hashlib.sha256(content.encode()).hexdigest()[:16]
            file_entry = indexed_files.get(rel_path, {})
            stored_hash = file_entry.get("hash") if isinstance(file_entry, dict) else file_entry
            if stored_hash != file_hash:
                changed.append(file_path)  # Return original path

        # Files in manifest but not in current scan = deleted
        current_rel_files = set(rel_to_orig.keys())
        deleted = [f for f in indexed_files if f not in current_rel_files]

        return changed, deleted

    def needs_update(self, files: dict[str, str]) -> int:
        """Quick check: how many files need updating?"""
        changed, deleted = self.get_stale_files(files)
        return len(changed) + len(deleted)

    def update(
        self,
        files: dict[str, str],
        on_progress: callable = None,
    ) -> dict:
        """Incremental update - only reindex changed files.

        Args:
            files: Dict mapping file paths to content (all files).
            on_progress: Callback for progress updates.

        Returns:
            Stats dict with counts.
        """
        changed, deleted = self.get_stale_files(files)

        if not changed and not deleted:
            return {"files": 0, "blocks": 0, "deleted": 0, "skipped": len(files)}

        db = self._ensure_db()
        manifest = self._load_manifest()

        # Delete vectors for deleted files
        deleted_count = 0
        if deleted:
            for f in deleted:
                file_entry = manifest["files"].get(f, {})
                old_blocks = file_entry.get("blocks", []) if isinstance(file_entry, dict) else []
                if old_blocks:
                    db.delete(old_blocks)
                    deleted_count += len(old_blocks)
                manifest["files"].pop(f, None)
            self._save_manifest(manifest)

        # Re-index changed files (index() handles deleting old vectors)
        changed_files = {f: files[f] for f in changed if f in files}
        stats = self.index(changed_files, on_progress=on_progress)
        stats["deleted"] = stats.get("deleted", 0) + deleted_count

        return stats

    def clear(self) -> None:
        """Delete the index."""
        import shutil

        if self.index_dir.exists():
            shutil.rmtree(self.index_dir)
