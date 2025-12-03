"""Pure Python fallback scanner for when Mojo extension is not available."""

import os
import re
from pathlib import Path
from typing import Dict

# Directories to always skip
IGNORED_DIRS = frozenset({
    "node_modules", "target", "build", "dist", "venv", "env",
    ".git", ".pixi", ".vscode", ".idea", "__pycache__",
})

# Binary file extensions to skip
BINARY_EXTENSIONS = frozenset({
    # Compiled/object files
    ".pyc", ".pyo", ".o", ".so", ".dylib", ".dll", ".bin", ".exe", ".a", ".lib",
    # Archives
    ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar", ".jar", ".war", ".whl",
    # Documents/media
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    # Images
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".svg", ".webp", ".bmp", ".tiff",
    # Audio/video
    ".mp3", ".mp4", ".wav", ".avi", ".mov", ".mkv",
    # Data files
    ".db", ".sqlite", ".sqlite3", ".pickle", ".pkl", ".npy", ".npz",
    ".onnx", ".pt", ".pth", ".safetensors",
    # Lock files
    ".lock",
})

MAX_FILE_SIZE = 1_000_000  # 1MB


def _is_binary_content(content: bytes, check_size: int = 8192) -> bool:
    """Check if content appears to be binary (contains null bytes)."""
    return b"\x00" in content[:check_size]


def scan(root: str, pattern: str, include_hidden: bool = False) -> Dict[str, str]:
    """
    Scan directory tree for files matching regex pattern.

    Args:
        root: Root directory path
        pattern: Regex pattern to match
        include_hidden: Whether to include hidden files (default False)

    Returns:
        Dict mapping file paths to their contents
    """
    root_path = Path(root)
    if not root_path.exists():
        raise ValueError(f"Path does not exist: {root}")
    if not root_path.is_dir():
        raise ValueError(f"Path is not a directory: {root}")

    try:
        regex = re.compile(pattern, re.IGNORECASE)
    except re.error as e:
        raise ValueError(f"Invalid regex pattern: {e}")

    results = {}
    visited = set()

    for dirpath, dirnames, filenames in os.walk(root_path, followlinks=False):
        # Resolve to catch symlink loops
        try:
            real_path = os.path.realpath(dirpath)
            if real_path in visited:
                continue
            visited.add(real_path)
        except OSError:
            continue

        # Filter directories in-place
        dirnames[:] = [
            d for d in dirnames
            if d not in IGNORED_DIRS
            and (include_hidden or not d.startswith("."))
        ]

        for filename in filenames:
            # Skip hidden files unless flag set
            if not include_hidden and filename.startswith("."):
                continue

            # Skip binary extensions
            _, ext = os.path.splitext(filename)
            if ext.lower() in BINARY_EXTENSIONS:
                continue

            # Skip lock files by pattern (e.g., package-lock.json)
            if filename.endswith("-lock.json"):
                continue

            filepath = os.path.join(dirpath, filename)

            try:
                # Skip large files
                stat = os.stat(filepath)
                if stat.st_size > MAX_FILE_SIZE:
                    continue

                # Read and check content
                with open(filepath, "rb") as f:
                    content_bytes = f.read()

                # Skip binary files
                if _is_binary_content(content_bytes):
                    continue

                # Decode as UTF-8
                try:
                    content = content_bytes.decode("utf-8")
                except UnicodeDecodeError:
                    continue

                # Match pattern
                if regex.search(content):
                    results[filepath] = content

            except (OSError, PermissionError):
                continue

    return results
