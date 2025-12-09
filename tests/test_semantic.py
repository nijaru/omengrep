"""Test semantic search module."""

import os
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, os.path.join(os.getcwd(), "src"))

from hygrep.semantic import INDEX_DIR, MANIFEST_FILE, SemanticIndex, find_index_root


def cleanup_tmpdir(tmpdir: Path):
    """Clean up temp directory, handling omendb file locks."""
    try:
        shutil.rmtree(tmpdir)
    except OSError:
        # omendb may hold file locks briefly, retry after small delay
        import time

        time.sleep(0.1)
        shutil.rmtree(tmpdir, ignore_errors=True)


def test_find_index_root_no_existing():
    """Test find_index_root when no index exists."""
    with tempfile.TemporaryDirectory() as tmpdir:
        search_path = Path(tmpdir) / "src"
        search_path.mkdir()

        root, existing = find_index_root(search_path)

        assert root == search_path.resolve()
        assert existing is None

    print("find_index_root (no existing): PASS")


def test_find_index_root_walk_up():
    """Test find_index_root walks up to find existing index."""
    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = Path(tmpdir).resolve()

        # Create index at root
        index_dir = tmpdir / INDEX_DIR
        index_dir.mkdir()
        (index_dir / MANIFEST_FILE).write_text('{"version": 3, "files": {}}')

        # Search from subdirectory
        subdir = tmpdir / "src" / "lib"
        subdir.mkdir(parents=True)

        root, existing = find_index_root(subdir)

        assert root == tmpdir
        assert existing == index_dir

    print("find_index_root (walk up): PASS")


def test_semantic_index_init():
    """Test SemanticIndex initialization."""
    with tempfile.TemporaryDirectory() as tmpdir:
        idx = SemanticIndex(tmpdir)

        assert idx.root == Path(tmpdir).resolve()
        assert idx.index_dir == Path(tmpdir).resolve() / INDEX_DIR
        assert idx._db is None  # Lazy load

    print("SemanticIndex init: PASS")


def test_semantic_index_scope():
    """Test SemanticIndex with search scope."""
    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = Path(tmpdir)
        subdir = tmpdir / "src" / "lib"
        subdir.mkdir(parents=True)

        idx = SemanticIndex(tmpdir, search_scope=subdir)

        assert idx.search_scope == "src/lib"

    print("SemanticIndex scope: PASS")


def test_semantic_index_roundtrip():
    """Test index creation, search, and retrieval."""
    tmpdir = Path(tempfile.mkdtemp())
    try:
        # Create test files
        (tmpdir / "auth.py").write_text(
            "def login(user, password):\n    # Authenticate user\n    return True\n"
        )
        (tmpdir / "math.py").write_text("def add(a, b):\n    return a + b\n")

        # Read files
        files = {}
        for f in tmpdir.glob("*.py"):
            files[str(f)] = f.read_text()

        # Create index
        idx = SemanticIndex(tmpdir)
        stats = idx.index(files)

        assert stats["files"] >= 2
        assert stats["blocks"] >= 2
        assert idx.is_indexed()
        assert idx.count() >= 2

        # Search
        results = idx.search("user authentication", k=5)

        assert len(results) >= 1
        # Auth-related function should rank higher
        top_result = results[0]
        assert "file" in top_result
        assert "name" in top_result
        assert "score" in top_result
        assert top_result["name"] == "login", f"Expected 'login' first, got '{top_result['name']}'"
    finally:
        cleanup_tmpdir(tmpdir)

    print("SemanticIndex roundtrip: PASS")


def test_semantic_index_incremental_update():
    """Test incremental update of index."""
    tmpdir = Path(tempfile.mkdtemp())
    try:
        # Initial file
        auth_file = tmpdir / "auth.py"
        auth_file.write_text("def login(): pass\n")

        files = {str(auth_file): auth_file.read_text()}
        idx = SemanticIndex(tmpdir)
        idx.index(files)

        initial_count = idx.count()

        # Modify file
        auth_file.write_text("def login(): return True\ndef logout(): pass\n")
        files = {str(auth_file): auth_file.read_text()}

        # Update should detect change
        needs = idx.needs_update(files)
        assert needs > 0, "Should detect changed file"

        # Perform update
        stats = idx.update(files)
        assert stats["files"] >= 1, "Should re-index changed file"

        # Verify updated
        new_count = idx.count()
        assert new_count >= initial_count, "Count should increase (2 functions now)"
    finally:
        cleanup_tmpdir(tmpdir)

    print("SemanticIndex incremental update: PASS")


def test_semantic_index_stale_detection():
    """Test stale file detection."""
    tmpdir = Path(tempfile.mkdtemp())
    try:
        # Create and index file
        test_file = tmpdir / "test.py"
        test_file.write_text("def foo(): pass\n")

        files = {str(test_file): test_file.read_text()}
        idx = SemanticIndex(tmpdir)
        idx.index(files)

        # No changes yet
        changed, deleted = idx.get_stale_files(files)
        assert len(changed) == 0, "No changes expected"
        assert len(deleted) == 0, "No deletions expected"

        # Modify file
        new_content = "def foo(): return 1\n"
        files = {str(test_file): new_content}
        changed, deleted = idx.get_stale_files(files)
        assert len(changed) == 1, "Should detect 1 changed file"

        # Simulate file deletion (not in files dict)
        changed, deleted = idx.get_stale_files({})
        assert len(deleted) == 1, "Should detect 1 deleted file"
    finally:
        cleanup_tmpdir(tmpdir)

    print("SemanticIndex stale detection: PASS")


def test_semantic_index_clear():
    """Test index clearing."""
    tmpdir = Path(tempfile.mkdtemp())
    try:
        # Create and index file
        (tmpdir / "test.py").write_text("def foo(): pass\n")
        files = {str(tmpdir / "test.py"): "def foo(): pass\n"}

        idx = SemanticIndex(tmpdir)
        idx.index(files)

        assert idx.is_indexed()

        # Clear
        idx.clear()

        assert not idx.is_indexed()
        assert not (tmpdir / INDEX_DIR).exists()
    finally:
        cleanup_tmpdir(tmpdir)

    print("SemanticIndex clear: PASS")


def test_semantic_index_scope_filtering():
    """Test that search scope filters results."""
    tmpdir = Path(tempfile.mkdtemp())
    try:
        # Create files in different directories
        src_dir = tmpdir / "src"
        test_dir = tmpdir / "tests"
        src_dir.mkdir()
        test_dir.mkdir()

        (src_dir / "auth.py").write_text("def login(): pass\n")
        (test_dir / "test_auth.py").write_text("def test_login(): pass\n")

        # Index all files
        files = {}
        for f in tmpdir.rglob("*.py"):
            files[str(f)] = f.read_text()

        idx = SemanticIndex(tmpdir)
        idx.index(files)

        # Search without scope - should find both
        results_all = idx.search("login", k=10)
        assert len(results_all) >= 2, "Should find files in both directories"

        # Search with scope - create new index with scope
        # (reuse same db handle by setting scope after)
        idx._db = None  # Release lock
        idx_scoped = SemanticIndex(tmpdir, search_scope=src_dir)

        results = idx_scoped.search("login", k=10)

        # All results should be from src/
        for r in results:
            assert "src" in r["file"], f"Result should be from src/: {r['file']}"
            assert "tests" not in r["file"], f"Result should not be from tests/: {r['file']}"
    finally:
        cleanup_tmpdir(tmpdir)

    print("SemanticIndex scope filtering: PASS")


def test_semantic_index_relative_paths():
    """Test that manifest uses relative paths (portable)."""
    tmpdir = Path(tempfile.mkdtemp()).resolve()  # Resolve symlinks (macOS /var -> /private/var)
    try:
        # Create file
        (tmpdir / "code.py").write_text("def hello(): pass\n")
        files = {str(tmpdir / "code.py"): "def hello(): pass\n"}

        idx = SemanticIndex(tmpdir)
        idx.index(files)

        # Check manifest has relative paths
        import json

        manifest = json.loads((tmpdir / INDEX_DIR / MANIFEST_FILE).read_text())
        for path in manifest.get("files", {}).keys():
            assert not Path(path).is_absolute(), f"Manifest path should be relative: {path}"
            assert path == "code.py", f"Expected 'code.py', got '{path}'"
    finally:
        cleanup_tmpdir(tmpdir)

    print("SemanticIndex relative paths: PASS")


if __name__ == "__main__":
    print("Running semantic tests...\n")
    test_find_index_root_no_existing()
    test_find_index_root_walk_up()
    test_semantic_index_init()
    test_semantic_index_scope()
    test_semantic_index_roundtrip()
    test_semantic_index_incremental_update()
    test_semantic_index_stale_detection()
    test_semantic_index_clear()
    test_semantic_index_scope_filtering()
    test_semantic_index_relative_paths()
    print("\nAll semantic tests passed!")
