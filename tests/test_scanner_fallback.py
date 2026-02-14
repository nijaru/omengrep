"""Tests for Python fallback scanner and edge cases."""

import os
import sys
import tempfile

sys.path.insert(0, os.path.join(os.getcwd(), "src"))

from hygrep.scanner import scan


class TestScannerBasics:
    """Basic scanner functionality."""

    def test_simple_match(self):
        """Find files matching pattern."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, "test.py"), "w") as f:
                f.write("def hello(): pass\n")

            results = scan(tmpdir, "hello")
            assert len(results) == 1
            assert "hello" in next(iter(results.values()))

    def test_no_match(self):
        """Return empty dict when no matches."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, "test.py"), "w") as f:
                f.write("def goodbye(): pass\n")

            results = scan(tmpdir, "hello")
            assert len(results) == 0

    def test_case_insensitive(self):
        """Search is case insensitive."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, "test.py"), "w") as f:
                f.write("def HELLO(): pass\n")

            results = scan(tmpdir, "hello")
            assert len(results) == 1

    def test_regex_pattern(self):
        """Support regex patterns."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, "test.py"), "w") as f:
                f.write("def hello_world(): pass\n")

            results = scan(tmpdir, "hello.*world")
            assert len(results) == 1

    def test_invalid_regex(self):
        """Raise on invalid regex."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, "test.py"), "w") as f:
                f.write("test\n")

            try:
                scan(tmpdir, "[invalid")
                raise AssertionError("Should raise ValueError")
            except ValueError as e:
                assert "Invalid regex" in str(e)


class TestIgnoredDirs:
    """Directory filtering."""

    def test_ignored_dirs_skipped(self):
        """Skip directories in IGNORED_DIRS."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create node_modules with a matching file
            nm_dir = os.path.join(tmpdir, "node_modules")
            os.makedirs(nm_dir)
            with open(os.path.join(nm_dir, "test.js"), "w") as f:
                f.write("function hello() {}\n")

            # Create a matching file in root
            with open(os.path.join(tmpdir, "main.py"), "w") as f:
                f.write("def hello(): pass\n")

            results = scan(tmpdir, "hello")
            # Should only find main.py, not node_modules/test.js
            assert len(results) == 1
            assert any("main.py" in k for k in results)

    def test_all_ignored_dirs(self):
        """All IGNORED_DIRS are skipped."""
        for ignored_dir in ["node_modules", ".git", "__pycache__", "venv"]:
            with tempfile.TemporaryDirectory() as tmpdir:
                subdir = os.path.join(tmpdir, ignored_dir)
                os.makedirs(subdir)
                with open(os.path.join(subdir, "test.py"), "w") as f:
                    f.write("match_me\n")

                results = scan(tmpdir, "match_me")
                assert len(results) == 0, f"{ignored_dir} should be ignored"


class TestHiddenFiles:
    """Hidden file handling."""

    def test_hidden_files_excluded_by_default(self):
        """Hidden files excluded by default."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, ".hidden"), "w") as f:
                f.write("secret\n")
            with open(os.path.join(tmpdir, "visible.txt"), "w") as f:
                f.write("secret\n")

            results = scan(tmpdir, "secret")
            assert len(results) == 1
            assert ".hidden" not in next(iter(results.keys()))

    def test_hidden_files_included_with_flag(self):
        """Hidden files included when flag set."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, ".hidden"), "w") as f:
                f.write("secret\n")

            results = scan(tmpdir, "secret", include_hidden=True)
            assert len(results) == 1


class TestBinaryFiles:
    """Binary file handling."""

    def test_binary_extensions_skipped(self):
        """Skip files with binary extensions."""
        with tempfile.TemporaryDirectory() as tmpdir:
            for ext in [".pyc", ".so", ".png", ".zip"]:
                with open(os.path.join(tmpdir, f"test{ext}"), "w") as f:
                    f.write("match_me\n")

            with open(os.path.join(tmpdir, "test.py"), "w") as f:
                f.write("match_me\n")

            results = scan(tmpdir, "match_me")
            assert len(results) == 1
            assert "test.py" in next(iter(results.keys()))

    def test_binary_content_detected(self):
        """Detect binary content by null bytes."""
        assert b"\x00" in b"hello\x00world"
        assert b"\x00" not in b"hello world"

    def test_binary_content_skipped(self):
        """Skip files with binary content even if extension is text."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, "binary.txt"), "wb") as f:
                f.write(b"hello\x00world")

            results = scan(tmpdir, "hello")
            assert len(results) == 0


class TestLargeFiles:
    """Large file handling."""

    def test_large_files_skipped(self):
        """Skip files larger than MAX_FILE_SIZE."""
        with tempfile.TemporaryDirectory() as tmpdir:
            large_file = os.path.join(tmpdir, "large.txt")
            # Create file larger than 1MB
            with open(large_file, "w") as f:
                f.write("match_me\n" * 200000)  # ~1.8MB

            small_file = os.path.join(tmpdir, "small.txt")
            with open(small_file, "w") as f:
                f.write("match_me\n")

            results = scan(tmpdir, "match_me")
            assert len(results) == 1
            assert "small.txt" in next(iter(results.keys()))


class TestSymlinks:
    """Symlink handling."""

    def test_symlink_loop_handled(self):
        """Don't infinite loop on symlink cycles."""
        with tempfile.TemporaryDirectory() as tmpdir:
            subdir = os.path.join(tmpdir, "subdir")
            os.makedirs(subdir)

            # Create symlink pointing back to parent
            link = os.path.join(subdir, "loop")
            try:
                os.symlink(tmpdir, link)
            except OSError:
                # Skip on systems that don't support symlinks
                return

            with open(os.path.join(tmpdir, "test.py"), "w") as f:
                f.write("match_me\n")

            # Should complete without infinite loop
            results = scan(tmpdir, "match_me")
            # Should find the file exactly once
            assert len(results) == 1


class TestPathValidation:
    """Path validation."""

    def test_nonexistent_path(self):
        """Raise on nonexistent path."""
        try:
            scan("/nonexistent/path", "test")
            raise AssertionError("Should raise ValueError")
        except ValueError as e:
            assert "does not exist" in str(e)

    def test_file_not_directory(self):
        """Raise when path is file not directory."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = os.path.join(tmpdir, "test.py")
            with open(filepath, "w") as f:
                f.write("test\n")

            try:
                scan(filepath, "test")
                raise AssertionError("Should raise ValueError")
            except ValueError as e:
                assert "not a directory" in str(e)


class TestUnicode:
    """Unicode handling."""

    def test_utf8_content(self):
        """Handle UTF-8 content."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, "test.py"), "w", encoding="utf-8") as f:
                f.write("# Comment: 你好世界\ndef hello(): pass\n")

            results = scan(tmpdir, "hello")
            assert len(results) == 1

    def test_invalid_utf8_skipped(self):
        """Skip files with invalid UTF-8."""
        with tempfile.TemporaryDirectory() as tmpdir:
            with open(os.path.join(tmpdir, "test.py"), "wb") as f:
                f.write(b"hello \xff\xfe world\n")

            results = scan(tmpdir, "hello")
            assert len(results) == 0


def run_tests():
    """Run all tests."""
    import traceback

    test_classes = [
        TestScannerBasics,
        TestIgnoredDirs,
        TestHiddenFiles,
        TestBinaryFiles,
        TestLargeFiles,
        TestSymlinks,
        TestPathValidation,
        TestUnicode,
    ]

    passed = 0
    failed = 0

    for cls in test_classes:
        print(f"\n=== {cls.__name__} ===")
        instance = cls()
        for name in dir(instance):
            if not name.startswith("test_"):
                continue
            method = getattr(instance, name)
            try:
                method()
                print(f"  PASS: {name}")
                passed += 1
            except Exception as e:
                print(f"  FAIL: {name}")
                print(f"        {e}")
                traceback.print_exc()
                failed += 1

    print(f"\n{'=' * 50}")
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(run_tests())
