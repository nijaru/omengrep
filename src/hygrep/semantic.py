"""Semantic search using embeddings + omendb vector database."""

import hashlib
import json
import os
from collections.abc import Callable
from pathlib import Path

from .embedder import DIMENSIONS, get_embedder
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

# Common code abbreviations and synonyms for query expansion
CODE_SYNONYMS: dict[str, list[str]] = {
    "auth": ["authentication", "authorize", "authorization"],
    "authn": ["authentication"],
    "authz": ["authorization"],
    "config": ["configuration", "settings", "options"],
    "cfg": ["config", "configuration"],
    "db": ["database"],
    "err": ["error", "exception"],
    "exc": ["exception", "error"],
    "fn": ["function"],
    "func": ["function"],
    "impl": ["implementation", "implement"],
    "init": ["initialize", "initialization"],
    "msg": ["message"],
    "param": ["parameter"],
    "params": ["parameters"],
    "req": ["request"],
    "res": ["response"],
    "resp": ["response"],
    "ret": ["return"],
    "srv": ["server", "service"],
    "svc": ["service"],
    "util": ["utility", "utilities"],
    "utils": ["utilities", "utility"],
    "val": ["value", "validate", "validation"],
}


def expand_query_terms(query: str) -> set[str]:
    """Expand query with common code synonyms.

    Returns set of all terms to look for (original + expansions).
    """
    terms = set()
    for word in query.lower().split():
        terms.add(word)
        if word in CODE_SYNONYMS:
            terms.update(CODE_SYNONYMS[word])
    return terms


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


def find_subdir_indexes(path: Path) -> list[Path]:
    """Find all .hhg/ directories in subdirectories.

    Args:
        path: Root directory to search under.

    Returns:
        List of paths to .hhg/ directories found in subdirs.
    """
    path = Path(path).resolve()
    indexes = []

    for root, dirs, _files in os.walk(path):
        root_path = Path(root)
        # Skip the path itself
        if root_path == path:
            # Check subdirs for .hhg
            if INDEX_DIR in dirs:
                # Don't descend into .hhg
                dirs.remove(INDEX_DIR)
            continue

        # Found .hhg in a subdir
        if INDEX_DIR in dirs:
            index_path = root_path / INDEX_DIR
            if (index_path / MANIFEST_FILE).exists():
                indexes.append(index_path)
            # Don't descend into .hhg
            dirs.remove(INDEX_DIR)

        # Skip hidden directories
        dirs[:] = [d for d in dirs if not d.startswith(".")]

    return indexes


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

        # Use global embedder for caching benefits
        self.embedder = get_embedder(cache_dir=cache_dir)
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
        on_progress: Callable[[int, int, str], None] | None = None,
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

        Uses hybrid approach: semantic similarity + keyword boost.
        Query terms are expanded with common code synonyms.

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

        # Expand query terms for keyword matching
        query_terms = expand_query_terms(query)

        # Request more results for hybrid re-ranking
        search_k = k * 3
        results = db.search(query_embedding.tolist(), k=search_k)

        # Format results with hybrid scoring
        output = []
        for r in results:
            meta = r.get("metadata", {})
            rel_file = meta.get("file", "")

            # Filter by search scope if set
            if self.search_scope and not rel_file.startswith(self.search_scope):
                continue

            # Convert to absolute path for display
            abs_file = self._to_absolute(rel_file)

            # Base score from semantic similarity
            semantic_score = (2.0 - r.get("distance", 0)) / 2.0

            # Hybrid boost: check for literal query term matches
            content = (meta.get("content") or "").lower()
            name = (meta.get("name") or "").lower()
            text_to_check = f"{name} {content}"

            # Count matching terms (including expanded synonyms)
            matches = sum(1 for term in query_terms if term in text_to_check)
            if matches > 0:
                # Boost 10% per matching term, max 50% boost
                boost = min(1.5, 1.0 + (0.1 * matches))
                final_score = min(1.0, semantic_score * boost)
            else:
                final_score = semantic_score

            output.append(
                {
                    "file": abs_file,
                    "type": meta.get("type", ""),
                    "name": meta.get("name", ""),
                    "line": meta.get("start_line", 0),
                    "end_line": meta.get("end_line", 0),
                    "content": meta.get("content", ""),
                    "score": final_score,
                }
            )

        # Re-sort by hybrid score and return top k
        output.sort(key=lambda x: -x["score"])
        return output[:k]

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
        on_progress: Callable[[int, int, str], None] | None = None,
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

        # Open subdir database
        subdir_vectors_path = str(subdir_index_path / VECTORS_DIR)
        try:
            subdir_db = omendb.open(subdir_vectors_path, dimensions=DIMENSIONS)
        except Exception as e:
            return {"merged": 0, "error": f"cannot open subdir db: {e}"}

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
                        "vector": item["embedding"],
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

        self._save_manifest(manifest)
        return stats
