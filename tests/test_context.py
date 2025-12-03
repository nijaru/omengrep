"""Test extractor module."""

import os
import sys

sys.path.insert(0, os.path.join(os.getcwd(), "src"))

from hygrep.extractor import ContextExtractor


def test_extraction():
    extractor = ContextExtractor()

    # Create dummy file
    dummy_path = "tests/dummy.py"
    code = "def hello():\n    print('Hello')\n\nclass World:\n    def greet(self):\n        pass\n"

    with open(dummy_path, "w") as f:
        f.write(code)

    try:
        blocks = extractor.extract(dummy_path, "hello")
        print(f"Found {len(blocks)} blocks:")
        for b in blocks:
            print(f" - {b['type']}: {b['name']} (Lines {b['start_line']}-{b['end_line']})")

        assert len(blocks) >= 1

    finally:
        if os.path.exists(dummy_path):
            os.remove(dummy_path)


if __name__ == "__main__":
    test_extraction()
