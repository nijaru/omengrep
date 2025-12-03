"""Test CLI module."""

import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.join(os.getcwd(), "src"))

from hygrep import cli


def test_exit_codes():
    """Test grep-compatible exit codes."""
    with tempfile.TemporaryDirectory() as tmpdir:
        test_file = os.path.join(tmpdir, "test.py")
        with open(test_file, "w") as f:
            f.write("def hello(): pass\n")

        # Test match (exit 0)
        sys.argv = ["hygrep", "hello", tmpdir, "-q", "--fast"]
        try:
            cli.main()
            assert False, "Should have called sys.exit"
        except SystemExit as e:
            assert e.code == 0, f"Expected exit 0 on match, got {e.code}"

        # Test no match (exit 1)
        sys.argv = ["hygrep", "nonexistent_xyz", tmpdir, "-q", "--fast"]
        try:
            cli.main()
            assert False, "Should have called sys.exit"
        except SystemExit as e:
            assert e.code == 1, f"Expected exit 1 on no match, got {e.code}"

        # Test error (exit 2)
        sys.argv = ["hygrep", "test", "/nonexistent/path", "-q"]
        try:
            cli.main()
            assert False, "Should have called sys.exit"
        except SystemExit as e:
            assert e.code == 2, f"Expected exit 2 on error, got {e.code}"

    print("Exit codes: PASS")


def test_json_output(capsys=None):
    """Test JSON output format."""
    import io
    from contextlib import redirect_stdout

    with tempfile.TemporaryDirectory() as tmpdir:
        test_file = os.path.join(tmpdir, "auth.py")
        with open(test_file, "w") as f:
            f.write("def login(): pass\n")

        sys.argv = ["hygrep", "login", tmpdir, "--json", "--fast", "-q"]
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            try:
                cli.main()
            except SystemExit:
                pass

        out = stdout.getvalue()
        results = json.loads(out)
        assert isinstance(results, list)
        assert len(results) > 0
        assert "file" in results[0]
        assert "type" in results[0]
        assert "name" in results[0]

    print("JSON output: PASS")


def test_exclude_patterns():
    """Test --exclude pattern filtering."""
    import io
    from contextlib import redirect_stdout

    with tempfile.TemporaryDirectory() as tmpdir:
        with open(os.path.join(tmpdir, "main.py"), "w") as f:
            f.write("def main(): pass\n")
        with open(os.path.join(tmpdir, "test_main.py"), "w") as f:
            f.write("def test_main(): pass\n")

        # Without exclude
        sys.argv = ["hygrep", "main", tmpdir, "--json", "--fast", "-q"]
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            try:
                cli.main()
            except SystemExit:
                pass
        results = json.loads(stdout.getvalue())
        assert len(results) >= 2, f"Expected >= 2 results, got {len(results)}"

        # With exclude
        sys.argv = ["hygrep", "main", tmpdir, "--json", "--fast", "-q", "--exclude", "test_*"]
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            try:
                cli.main()
            except SystemExit:
                pass
        results = json.loads(stdout.getvalue())
        # Should have fewer results after exclusion
        for r in results:
            assert "test_main" not in r["file"], f"test_main should be excluded: {r['file']}"

    print("Exclude patterns: PASS")


def test_type_filter():
    """Test -t/--type file type filtering."""
    import io
    from contextlib import redirect_stdout

    with tempfile.TemporaryDirectory() as tmpdir:
        with open(os.path.join(tmpdir, "code.py"), "w") as f:
            f.write("def hello(): pass\n")
        with open(os.path.join(tmpdir, "code.js"), "w") as f:
            f.write("function hello() {}\n")

        sys.argv = ["hygrep", "hello", tmpdir, "--json", "--fast", "-q", "-t", "py"]
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            try:
                cli.main()
            except SystemExit:
                pass

        results = json.loads(stdout.getvalue())
        assert len(results) >= 1, f"Expected >= 1 Python result, got {len(results)}"
        for r in results:
            assert r["file"].endswith(".py"), f"Expected .py file, got {r['file']}"

    print("Type filter: PASS")


def test_help():
    """Test --help flag."""
    import io
    from contextlib import redirect_stdout

    sys.argv = ["hygrep", "--help"]
    stdout = io.StringIO()
    with redirect_stdout(stdout):
        try:
            cli.main()
        except SystemExit as e:
            assert e.code == 0

    out = stdout.getvalue()
    assert "hygrep" in out.lower()
    print("Help flag: PASS")


def test_info_command():
    """Test 'hygrep info' command."""
    import io
    from contextlib import redirect_stdout

    sys.argv = ["hygrep", "info"]
    stdout = io.StringIO()
    with redirect_stdout(stdout):
        try:
            cli.main()
        except SystemExit as e:
            assert e.code == 0, f"Expected exit 0, got {e.code}"

    out = stdout.getvalue()
    assert "hygrep" in out
    assert "Model:" in out or "Model" in out
    assert "Scanner:" in out or "scanner" in out.lower()

    print("Info command: PASS")


def test_model_command():
    """Test 'hygrep model' command."""
    import io
    from contextlib import redirect_stdout

    sys.argv = ["hygrep", "model"]
    stdout = io.StringIO()
    with redirect_stdout(stdout):
        try:
            cli.main()
        except SystemExit as e:
            # Exit 0 if installed, 1 if not
            assert e.code in (0, 1), f"Expected exit 0 or 1, got {e.code}"

    out = stdout.getvalue()
    assert "mixedbread-ai" in out or "Model:" in out

    print("Model command: PASS")


def test_fast_mode():
    """Test --fast mode (skip reranking)."""
    import io
    from contextlib import redirect_stdout

    with tempfile.TemporaryDirectory() as tmpdir:
        test_file = os.path.join(tmpdir, "test.py")
        with open(test_file, "w") as f:
            f.write("def hello(): pass\n")

        sys.argv = ["hygrep", "hello", tmpdir, "--fast", "--json", "-q"]
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            try:
                cli.main()
            except SystemExit:
                pass

        results = json.loads(stdout.getvalue())
        assert len(results) >= 1
        # Fast mode should have score 0.0
        assert results[0]["score"] == 0.0

    print("Fast mode: PASS")


if __name__ == "__main__":
    print("Running CLI tests...\n")
    test_exit_codes()
    test_json_output()
    test_exclude_patterns()
    test_type_filter()
    test_help()
    test_info_command()
    test_model_command()
    test_fast_mode()
    print("\nAll CLI tests passed!")
