"""Test reranker module."""

import os
import sys

sys.path.insert(0, os.path.join(os.getcwd(), "src"))

from hygrep.reranker import Reranker


def test_reranker():
    """Test reranker initialization and search.

    Model is auto-downloaded on first use if not cached.
    """
    print("Initializing reranker...")
    reranker = Reranker()

    # Test with dummy content
    file_contents = {
        "test_auth.py": (
            "def login():\n    # User login logic\n    pass\n\ndef logout():\n    pass\n"
        )
    }

    print("Running search...")
    results = reranker.search("user authentication", file_contents, top_k=5)

    print(f"Got {len(results)} results")
    for r in results:
        print(f" - {r['type']} {r['name']} (Score: {r['score']:.4f})")

    assert len(results) > 0
    # 'login' should score higher than 'logout' for 'authentication'
    first = results[0]
    assert first["name"] == "login", f"Expected 'login' first, got '{first['name']}'"


if __name__ == "__main__":
    test_reranker()
