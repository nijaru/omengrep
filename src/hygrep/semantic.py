"""Semantic search using embeddings + omendb vector database."""

import hashlib
import json
import multiprocessing
import os
from collections.abc import Callable
from pathlib import Path

import omendb

from .embedder import DIMENSIONS, get_embedder
from .extractor import ContextExtractor

INDEX_DIR = ".hhg"
VECTORS_DIR = "vectors"
MANIFEST_FILE = "manifest.json"
MANIFEST_VERSION = 6  # v6: gte-modernbert-base model (768 dims)

# Block types that are documentation, not code
DOC_BLOCK_TYPES = {"text", "section"}


class AmbiguousBlockError(Exception):
    """Raised when a block name matches multiple blocks."""

    def __init__(self, name: str, matches: list[dict]):
        self.name = name
        self.matches = matches
        super().__init__(f"Multiple blocks named '{name}' found")


class BlockNotFoundError(Exception):
    """Raised when a block cannot be found."""

    pass


class IndexNeedsRebuild(Exception):
    """Raised when the index needs to be rebuilt."""

    pass


def _extract_blocks_worker(file_path: str, content: str, rel_path: str) -> list[dict]:
    """Worker to extract blocks from a single file."""
    try:
        extractor = ContextExtractor()
        blocks = extractor.extract(file_path, query="", content=content)
        output_blocks = []
        for block in blocks:
            block_id = f"{rel_path}:{block['start_line']}:{block['name']}"
            output_blocks.append(
                {
                    "id": block_id,
                    "file": rel_path,
                    "block": block,
                    "text": f"{block['type']} {block['name']}\n{block['content']}",
                }
            )
        return output_blocks
    except Exception:
        return []  # Return empty list on error


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


def find_parent_index(path: Path) -> Path | None:
    """Find parent directory with existing index (not at path itself).

    Args:
        path: Directory to check from.

    Returns:
        Parent directory with index, or None if no parent has index.
    """
    path = Path(path).resolve()
    current = path.parent  # Start from parent, not self

    while current != current.parent:
        index_dir = current / INDEX_DIR
        if (index_dir / MANIFEST_FILE).exists():
            return current
        current = current.parent

    return None


def find_subdir_indexes(path: Path, include_root: bool = False) -> list[Path]:
    """Find all .hhg/ directories under path.

    Args:
        path: Root directory to search under.
        include_root: If True, include index at path itself (for hhg list).

    Returns:
        List of paths to .hhg/ directories found.
    """
    path = Path(path).resolve()
    indexes = []

    for root, dirs, _files in os.walk(path):
        root_path = Path(root)

        # Filter hidden directories early (before continue), but keep .hhg for detection
        dirs[:] = [d for d in dirs if not d.startswith(".") or d == INDEX_DIR]

        # Handle root path
        if root_path == path:
            if INDEX_DIR in dirs:
                dirs.remove(INDEX_DIR)  # Don't descend into .hhg
                if include_root:
                    index_path = root_path / INDEX_DIR
                    if (index_path / MANIFEST_FILE).exists():
                        indexes.append(index_path)
            continue

        # Found .hhg in a subdir
        if INDEX_DIR in dirs:
            index_path = root_path / INDEX_DIR
            if (index_path / MANIFEST_FILE).exists():
                indexes.append(index_path)
            dirs.remove(INDEX_DIR)  # Don't descend into .hhg

    return indexes


class SemanticIndex:
    """Manages semantic search index using omendb.

    Can be used as a context manager for automatic cleanup:
        with SemanticIndex(path) as index:
            index.search(query)
    """

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

        # Use global embedder for caching benefits
        self.embedder = get_embedder(cache_dir=cache_dir)
        self._extractor: ContextExtractor | None = None

        self._db: "omendb.VectorDatabase | None" = None

    @property
    def extractor(self) -> ContextExtractor:
        """Lazy-load extractor (only needed for build, not search)."""
        if self._extractor is None:
            self._extractor = ContextExtractor()
        return self._extractor

    def __enter__(self) -> "SemanticIndex":
        """Context manager entry."""
        return self

    def __exit__(self, _exc_type, _exc_val, _exc_tb) -> None:
        """Context manager exit - ensures db is closed."""
        self.close()

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

    def _ensure_db(self) -> "omendb.VectorDatabase":
        """Open or create the vector database."""
        if self._db is None:
            self.index_dir.mkdir(parents=True, exist_ok=True)
            self._db = omendb.open(self.vectors_path, dimensions=DIMENSIONS)
        return self._db

    def close(self) -> None:
        """Close database handle and release lock."""
        self._db = None

    def _file_hash(self, path: Path) -> str:
        """Get hash of file content for change detection."""
        content = path.read_bytes()
        return hashlib.sha256(content).hexdigest()[:16]

    def _load_manifest(self) -> dict:
        """Load manifest of indexed files.

        Manifest format v6:
            {"version": 6, "model": "gte-modernbert-base-v1",
             "files": {"rel/path": {"hash": "abc123", "blocks": ["id1", "id2"]}}}

        Migrates from older formats on load.
        """
        from .embedder import MODEL_VERSION

        if self.manifest_path.exists():
            content = self.manifest_path.read_text().strip()
            if not content:
                return {"version": MANIFEST_VERSION, "model": MODEL_VERSION, "files": {}}
            data = json.loads(content)
            version = data.get("version", 1)
            files = data.get("files", {})

            # v5 -> v6: model changed from jina-code to gte-modernbert
            # v4 -> v5: embedding model changed (256 -> 768 dims)
            # Requires full rebuild - embeddings are incompatible
            if version < 6 and files:
                raise IndexNeedsRebuild()

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
                data["version"] = 3

            # v3 -> v4: hybrid search (text field added)
            # No manifest migration needed - search() falls back to vector-only
            # for indexes without text. Rebuild with `hhg build --force` to enable hybrid.

            # Validate model version matches - different models produce incompatible embeddings
            stored_model = data.get("model")
            if stored_model and stored_model != MODEL_VERSION and files:
                raise IndexNeedsRebuild()

            # Ensure model version is set for v6+
            if "model" not in data:
                data["model"] = MODEL_VERSION

            return data
        return {"version": MANIFEST_VERSION, "model": MODEL_VERSION, "files": {}}

    def _save_manifest(self, manifest: dict) -> None:
        """Save manifest with current model version."""
        from .embedder import MODEL_VERSION

        manifest["version"] = MANIFEST_VERSION
        manifest["model"] = MODEL_VERSION
        self.manifest_path.write_text(json.dumps(manifest, indent=2))

    def index(
        self,
        files: dict[str, str],
        batch_size: int = 128,
        workers: int = 0,
        on_progress: Callable[[int, int, str], None] | None = None,
    ) -> dict:
        """Index code files for semantic search using a parallel extractor."""
        db = self._ensure_db()
        manifest = self._load_manifest()
        stats = {"files": 0, "blocks": 0, "skipped": 0, "errors": 0, "deleted": 0}

        # 1. Identify files that need processing
        files_to_process = []
        for file_path, content in files.items():
            rel_path = self._to_relative(file_path)
            file_hash = hashlib.sha256(content.encode()).hexdigest()[:16]

            file_entry = manifest["files"].get(rel_path, {})
            if isinstance(file_entry, dict) and file_entry.get("hash") == file_hash:
                stats["skipped"] += 1
                continue

            # Delete old vectors for this file before re-indexing
            old_blocks = file_entry.get("blocks", []) if isinstance(file_entry, dict) else []
            if old_blocks:
                db.delete(old_blocks)
                stats["deleted"] += len(old_blocks)

            files_to_process.append(
                {
                    "path": file_path,
                    "content": content,
                    "rel_path": rel_path,
                    "hash": file_hash,
                }
            )

        if not files_to_process:
            if stats["deleted"] > 0:
                db.flush()
            return stats
        db.flush()  # Commit deletions

        # 2. Extract blocks in parallel
        num_workers = workers if workers > 0 else (os.cpu_count() or 1)
        all_blocks = []

        with multiprocessing.Pool(num_workers) as pool:
            results = pool.starmap(
                _extract_blocks_worker,
                [(f["path"], f["content"], f["rel_path"]) for f in files_to_process],
            )

            for i, file_info in enumerate(files_to_process):
                extracted_blocks = results[i]
                if extracted_blocks:
                    # Associate the file hash with each block for manifest update
                    for block in extracted_blocks:
                        block["file_hash"] = file_info["hash"]
                    all_blocks.extend(extracted_blocks)
                    stats["files"] += 1
                else:
                    # Check if the file had content, implying an extraction error
                    if file_info["content"]:
                        stats["errors"] += 1

        if not all_blocks:
            # remove deleted files from manifest
            for f in files_to_process:
                manifest["files"].pop(f["rel_path"], None)
            self._save_manifest(manifest)  # Save to record deletions
            return stats

        # 3. Embed and store in batches (sequential)
        # Sort by text length for better MLX batching (more homogeneous -> less padding)
        all_blocks.sort(key=lambda b: len(b["text"]))
        total = len(all_blocks)
        for i in range(0, total, batch_size):
            batch = all_blocks[i : i + batch_size]
            texts = [b["text"] for b in batch]

            if on_progress:
                on_progress(i, total, f"Embedding {len(batch)} blocks...")

            embeddings = self.embedder.embed(texts)

            items = [
                {
                    "id": block_info["id"],
                    "vector": embeddings[j].tolist(),
                    "text": block_info["text"],
                    "metadata": {
                        "file": block_info["file"],
                        "type": block_info["block"]["type"],
                        "name": block_info["block"]["name"],
                        "start_line": block_info["block"]["start_line"],
                        "end_line": block_info["block"]["end_line"],
                        "content": block_info["block"]["content"],
                    },
                }
                for j, block_info in enumerate(batch)
            ]
            db.set(items)
            stats["blocks"] += len(batch)
        db.flush()

        # 4. Update manifest
        blocks_by_file = {}
        for block in all_blocks:
            blocks_by_file.setdefault(block["file"], []).append(block["id"])

        for rel_path, block_ids in blocks_by_file.items():
            file_hash = next((b["file_hash"] for b in all_blocks if b["file"] == rel_path), None)
            if file_hash:
                manifest["files"][rel_path] = {"hash": file_hash, "blocks": block_ids}

        # Remove files that now have no blocks
        for f in files_to_process:
            if f["rel_path"] not in blocks_by_file:
                manifest["files"].pop(f["rel_path"], None)

        self._save_manifest(manifest)

        if on_progress:
            on_progress(total, total, "Done")

        return stats

    def search(self, query: str, k: int = 10) -> list[dict]:
        """Search for code blocks similar to query.

        Uses hybrid search combining semantic similarity (embeddings) and
        keyword matching (BM25) via omendb's search_hybrid.

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

        # Use hybrid search if text index is available, otherwise fall back to vector-only
        search_k = k * 3  # Request more for scope filtering
        if db.has_text_search():
            results = db.search_hybrid(
                query_embedding.tolist(),
                query,
                k=search_k,
                alpha=0.5,  # Balance between vector and text
            )
        else:
            # Fall back to vector-only search for older indexes
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

            # Score from hybrid search (RRF) or vector search (distance)
            if "score" in r:
                # Hybrid search returns score directly
                score = r["score"]
            else:
                # Vector-only search returns distance, convert to score
                score = (2.0 - r.get("distance", 0)) / 2.0

            output.append(
                {
                    "file": abs_file,
                    "type": meta.get("type", ""),
                    "name": meta.get("name", ""),
                    "line": meta.get("start_line", 0),
                    "end_line": meta.get("end_line", 0),
                    "content": meta.get("content", ""),
                    "score": score,
                }
            )

        # Sort by score (hybrid results may need re-sorting after scope filtering)
        output.sort(key=lambda x: -x["score"])
        return output[:k]

    def find_similar(
        self,
        file_path: str,
        line: int | None = None,
        name: str | None = None,
        k: int = 10,
        include_docs: bool = False,
    ) -> list[dict]:
        """Find code blocks similar to a given file/block.

        Uses pure vector similarity (no BM25) to find semantically similar code.

        Args:
            file_path: Path to the source file (absolute or relative).
            line: Line number to find block (mutually exclusive with name).
            name: Block name to find (mutually exclusive with line).
            k: Number of similar results to return.
            include_docs: Include text/doc blocks in results (default False).

        Returns:
            List of similar blocks with file, type, name, content, score.
            Excludes the query block itself and blocks from the same file.

        Raises:
            BlockNotFoundError: If file or block not found in index.
            AmbiguousBlockError: If name matches multiple blocks.
        """
        if line is not None and name is not None:
            raise ValueError("Cannot specify both line and name")

        db = self._ensure_db()
        manifest = self._load_manifest()

        # Convert to relative path for lookup
        rel_path = self._to_relative(file_path)

        # Find the block(s) for this file
        file_entry = manifest.get("files", {}).get(rel_path)
        if not file_entry or not isinstance(file_entry, dict):
            raise BlockNotFoundError(f"File not in index: {rel_path}")

        block_ids = file_entry.get("blocks", [])
        if not block_ids:
            raise BlockNotFoundError(f"No blocks found in {rel_path}")

        # Find the query block
        query_block_id = None

        if name is not None:
            # Look up by name
            matching = []
            for block_id in block_ids:
                item = db.get(block_id)
                if item:
                    meta = item.get("metadata", {})
                    block_name = meta.get("name", "")
                    # Support Class.method syntax
                    if block_name == name or block_name.endswith(f".{name}"):
                        matching.append(
                            {
                                "id": block_id,
                                "name": block_name,
                                "line": meta.get("start_line", 0),
                                "type": meta.get("type", ""),
                            }
                        )

            if not matching:
                raise BlockNotFoundError(f"No block named '{name}' in {rel_path}")
            elif len(matching) > 1:
                raise AmbiguousBlockError(name, matching)
            else:
                query_block_id = matching[0]["id"]

        elif line is not None:
            # Look up by line
            for block_id in block_ids:
                item = db.get(block_id)
                if item:
                    meta = item.get("metadata", {})
                    start = meta.get("start_line", 0)
                    end = meta.get("end_line", 0)
                    if start <= line <= end:
                        query_block_id = block_id
                        break
            if not query_block_id:
                # Line not in any block, use first block
                query_block_id = block_ids[0]
        else:
            # No line or name, use first block
            query_block_id = block_ids[0]

        # Get the embedding for the query block
        query_item = db.get(query_block_id)
        if not query_item:
            raise BlockNotFoundError("Could not retrieve block embedding")

        query_vector = query_item.get("vector")
        if not query_vector:
            raise BlockNotFoundError("Block has no embedding")

        # Search by vector similarity (request more to filter out self and docs)
        results = db.search(query_vector, k=k * 3 + len(block_ids))

        # Format results, filtering as needed
        output = []
        for r in results:
            r_id = r.get("id", "")
            # Skip blocks from the same file
            if r_id in block_ids:
                continue

            meta = r.get("metadata", {})
            block_type = meta.get("type", "")

            # Filter out doc blocks unless requested
            if not include_docs and block_type in DOC_BLOCK_TYPES:
                continue

            rel_file = meta.get("file", "")

            # Filter by search scope if set
            if self.search_scope and not rel_file.startswith(self.search_scope):
                continue

            # Convert distance to score (smaller distance = higher score)
            distance = r.get("distance", 0)
            score = (2.0 - distance) / 2.0

            output.append(
                {
                    "file": self._to_absolute(rel_file),
                    "type": block_type,
                    "name": meta.get("name", ""),
                    "line": meta.get("start_line", 0),
                    "end_line": meta.get("end_line", 0),
                    "content": meta.get("content", ""),
                    "score": score,
                }
            )

            if len(output) >= k:
                break

        return output

    def is_indexed(self) -> bool:
        """Check if index exists."""
        return self.manifest_path.exists()

    def count(self) -> int:
        """Count indexed vectors from manifest."""
        if not self.is_indexed():
            return 0
        manifest = self._load_manifest()
        total = 0
        for file_info in manifest.get("files", {}).values():
            if isinstance(file_info, dict):
                total += len(file_info.get("blocks", []))
        return total

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
        batch_size: int = 128,
        workers: int = 0,
        on_progress: Callable[[int, int, str], None] | None = None,
    ) -> dict:
        """Incremental update - only reindex changed files.

        Args:
            files: Dict mapping file paths to content (all files).
            batch_size: Number of code blocks to embed at once.
            workers: Number of parallel workers for extraction.
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
            db.flush()
            self._save_manifest(manifest)

        # Re-index changed files (index() handles deleting old vectors)
        changed_files = {f: files[f] for f in changed if f in files}
        stats = self.index(
            changed_files,
            batch_size=batch_size,
            workers=workers,
            on_progress=on_progress,
        )
        stats["deleted"] = stats.get("deleted", 0) + deleted_count

        return stats

    def clear(self) -> None:
        """Delete the index."""
        import shutil
        import time

        # Close db handle if open
        self._db = None

        if self.index_dir.exists():
            # omendb may briefly hold file locks, retry if needed
            for _ in range(3):
                try:
                    shutil.rmtree(self.index_dir)
                    break
                except OSError:
                    time.sleep(0.1)
            else:
                # Final attempt with ignore_errors
                shutil.rmtree(self.index_dir, ignore_errors=True)

    def remove_prefix(self, prefix: str) -> dict:
        """Remove all files/blocks matching a path prefix.

        Args:
            prefix: Relative path prefix (e.g., "src/subdir").

        Returns:
            Stats dict with files and blocks removed.
        """
        # Guard against empty/root prefix which would delete everything
        prefix = prefix.rstrip("/")
        if not prefix or prefix == ".":
            return {"files": 0, "blocks": 0}

        db = self._ensure_db()
        manifest = self._load_manifest()
        files = manifest.get("files", {})

        # Find matching files
        to_remove = []
        for rel_path in files:
            if rel_path == prefix or rel_path.startswith(f"{prefix}/"):
                to_remove.append(rel_path)

        if not to_remove:
            return {"files": 0, "blocks": 0}

        # Delete blocks
        blocks_removed = 0
        for rel_path in to_remove:
            file_entry = files.get(rel_path, {})
            block_ids = file_entry.get("blocks", []) if isinstance(file_entry, dict) else []
            if block_ids:
                db.delete(block_ids)
                blocks_removed += len(block_ids)
            manifest["files"].pop(rel_path, None)

        db.flush()
        self._save_manifest(manifest)

        return {"files": len(to_remove), "blocks": blocks_removed}

    def merge_from_subdir(self, subdir_index_path: Path) -> dict:
        """Merge vectors from a subdirectory index into this one.

        Translates paths from subdir-relative to parent-relative.
        Much faster than re-embedding since vectors are just copied.

        Args:
            subdir_index_path: Path to subdir's .hhg/ directory.

        Returns:
            Stats dict with counts.
        """
        db = self._ensure_db()
        manifest = self._load_manifest()

        # Calculate subdir prefix (path from self.root to subdir)
        subdir_root = subdir_index_path.parent
        try:
            prefix = str(subdir_root.relative_to(self.root))
        except ValueError:
            return {"merged": 0, "error": "subdir not under root"}

        # Load subdir manifest
        subdir_manifest_path = subdir_index_path / MANIFEST_FILE
        if not subdir_manifest_path.exists():
            return {"merged": 0, "error": "no manifest"}

        subdir_manifest = json.loads(subdir_manifest_path.read_text())
        subdir_files = subdir_manifest.get("files", {})

        # Open subdir database with context manager for proper cleanup
        subdir_vectors_path = str(subdir_index_path / VECTORS_DIR)
        try:
            with omendb.open(subdir_vectors_path, dimensions=DIMENSIONS) as subdir_db:
                stats = {"merged": 0, "files": 0, "skipped": 0}

                # Process each file in subdir manifest
                for rel_path, file_info in subdir_files.items():
                    if not isinstance(file_info, dict):
                        continue

                    # Translate path: subdir-relative → parent-relative
                    parent_rel_path = f"{prefix}/{rel_path}"

                    # Skip if already in parent manifest
                    if parent_rel_path in manifest.get("files", {}):
                        stats["skipped"] += 1
                        continue

                    block_ids = file_info.get("blocks", [])
                    new_block_ids = []
                    items_to_insert = []

                    for block_id in block_ids:
                        # Get vector from subdir db
                        item = subdir_db.get(block_id)
                        if item is None:
                            continue

                        # Translate block ID and metadata
                        new_id = f"{prefix}/{block_id}"
                        metadata = item.get("metadata", {})
                        if "file" in metadata:
                            metadata["file"] = f"{prefix}/{metadata['file']}"

                        items_to_insert.append(
                            {
                                "id": new_id,
                                "vector": item["vector"],
                                "metadata": metadata,
                            }
                        )
                        new_block_ids.append(new_id)

                    # Batch insert all blocks for this file
                    if items_to_insert:
                        db.set(items_to_insert)
                        stats["merged"] += len(items_to_insert)

                    # Update parent manifest
                    if new_block_ids:
                        manifest["files"][parent_rel_path] = {
                            "hash": file_info.get("hash", ""),
                            "blocks": new_block_ids,
                        }
                        stats["files"] += 1

                db.flush()
                self._save_manifest(manifest)
                return stats
        except Exception as e:
            return {"merged": 0, "error": f"cannot open subdir db: {e}"}
