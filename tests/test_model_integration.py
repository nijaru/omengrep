"""Integration tests for model management commands.

These tests hit the network and modify the cache.
Run manually: python tests/test_model_integration.py
Not included in default test suite.
"""
import io
import os
import sys
from contextlib import redirect_stdout, redirect_stderr

sys.path.insert(0, os.path.join(os.getcwd(), "src"))

from hygrep import cli
from hygrep.reranker import get_model_info, clean_model_cache


def test_model_clean_and_install():
    """Test full cycle: clean -> verify gone -> install -> verify installed."""
    print("=== Testing model clean/install cycle ===\n")

    # 1. Clean any existing model
    print("1. Cleaning model cache...")
    cleaned = clean_model_cache()
    print(f"   Cleaned: {cleaned}")

    # 2. Verify model is gone
    print("2. Verifying model removed...")
    info = get_model_info()
    assert not info["installed"], f"Model should be uninstalled, got: {info}"
    print("   Model not installed (correct)")

    # 3. Test 'hygrep model' shows not installed
    print("3. Testing 'hygrep model' output...")
    sys.argv = ["hygrep", "model"]
    stdout = io.StringIO()
    with redirect_stdout(stdout):
        try:
            cli.main()
        except SystemExit as e:
            assert e.code == 1, f"Expected exit 1 (not installed), got {e.code}"
    out = stdout.getvalue()
    assert "Not installed" in out, f"Expected 'Not installed' in output: {out}"
    print("   Shows 'Not installed' (correct)")

    # 4. Install model
    print("4. Installing model (this downloads ~83MB)...")
    sys.argv = ["hygrep", "model", "install"]
    stderr = io.StringIO()
    with redirect_stderr(stderr):
        try:
            cli.main()
        except SystemExit as e:
            assert e.code == 0, f"Expected exit 0 on install, got {e.code}"
    err = stderr.getvalue()
    print(f"   {err.strip()}")

    # 5. Verify model is installed
    print("5. Verifying model installed...")
    info = get_model_info()
    assert info["installed"], f"Model should be installed, got: {info}"
    print(f"   Installed: {info['size_mb']}MB at {info['model_path'][:50]}...")

    # 6. Test 'hygrep model' shows installed
    print("6. Testing 'hygrep model' output...")
    sys.argv = ["hygrep", "model"]
    stdout = io.StringIO()
    with redirect_stdout(stdout):
        try:
            cli.main()
        except SystemExit as e:
            assert e.code == 0, f"Expected exit 0 (installed), got {e.code}"
    out = stdout.getvalue()
    assert "Installed" in out, f"Expected 'Installed' in output: {out}"
    print("   Shows 'Installed' (correct)")

    # 7. Test search works
    print("7. Testing search with fresh model...")
    import tempfile
    with tempfile.TemporaryDirectory() as tmpdir:
        test_file = os.path.join(tmpdir, "auth.py")
        with open(test_file, "w") as f:
            f.write("def login(): pass\n")

        # Use "login" as query to match file content
        sys.argv = ["hygrep", "login", tmpdir, "--json", "-q"]
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            try:
                cli.main()
            except SystemExit:
                pass

        import json
        results = json.loads(stdout.getvalue())
        assert len(results) >= 1, f"Expected results, got: {results}"
        print(f"   Search returned {len(results)} result(s)")

    print("\n=== All integration tests passed! ===")


def test_model_install_force():
    """Test --force flag re-downloads model."""
    print("\n=== Testing model install --force ===\n")

    # Get current model info
    info_before = get_model_info()
    if not info_before["installed"]:
        print("Model not installed, installing first...")
        sys.argv = ["hygrep", "model", "install"]
        try:
            cli.main()
        except SystemExit:
            pass

    # Force reinstall
    print("Running 'hygrep model install --force'...")
    sys.argv = ["hygrep", "model", "install", "--force"]
    stderr = io.StringIO()
    with redirect_stderr(stderr):
        try:
            cli.main()
        except SystemExit as e:
            assert e.code == 0, f"Expected exit 0, got {e.code}"

    err = stderr.getvalue()
    assert "Downloading" in err, f"Expected download message: {err}"
    print(f"   {err.strip()}")

    # Verify still installed
    info_after = get_model_info()
    assert info_after["installed"], "Model should still be installed"
    print("   Model still installed after --force")

    print("\n=== Force install test passed! ===")


if __name__ == "__main__":
    test_model_clean_and_install()
    test_model_install_force()
