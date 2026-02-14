"""File scanner — walks directory tree and reads matching files."""

import os
import re
from pathlib import Path

# Directories to always skip
IGNORED_DIRS = frozenset(
    {
        "node_modules",
        "target",
        "build",
        "dist",
        "venv",
        "env",
        ".git",
        ".pixi",
        ".vscode",
        ".idea",
        "__pycache__",
    },
)

# Binary file extensions to skip
BINARY_EXTENSIONS = frozenset(
    {
        # Compiled/object files
        ".pyc",
        ".pyo",
        ".o",
        ".so",
        ".dylib",
        ".dll",
        ".bin",
        ".exe",
        ".a",
        ".lib",
        # Archives
        ".zip",
        ".tar",
        ".gz",
        ".bz2",
        ".xz",
        ".7z",
        ".rar",
        ".jar",
        ".war",
        ".whl",
        # Documents/media
        ".pdf",
        ".doc",
        ".docx",
        ".xls",
        ".xlsx",
        ".ppt",
        ".pptx",
        # Images
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".ico",
        ".svg",
        ".webp",
        ".bmp",
        ".tiff",
        # Audio/video
        ".mp3",
        ".mp4",
        ".wav",
        ".avi",
        ".mov",
        ".mkv",
        # Data files
        ".db",
        ".sqlite",
        ".sqlite3",
        ".pickle",
        ".pkl",
        ".npy",
        ".npz",
        ".onnx",
        ".pt",
        ".pth",
        ".safetensors",
        # Lock files
        ".lock",
    },
)

MAX_FILE_SIZE = 1_000_000  # 1MB


def scan(root: str | Path, pattern: str, include_hidden: bool = False) -> dict[str, str]:
    """Scan directory tree for files matching regex pattern.

    Args:
        root: Root directory path.
        pattern: Regex pattern to match file contents.
        include_hidden: Whether to include hidden files (default False).

    Returns:
        Dict mapping file paths to their contents.
    """
    root_path = Path(root) if isinstance(root, str) else root
    if not root_path.exists():
        raise ValueError(f"Path does not exist: {root}")
    if not root_path.is_dir():
        raise ValueError(f"Path is not a directory: {root}")

    # Fast path: "." matches everything, skip regex
    match_all = pattern == "."
    if not match_all:
        try:
            regex = re.compile(pattern, re.IGNORECASE)
        except re.error as e:
            raise ValueError(f"Invalid regex pattern: {e}") from e

    results: dict[str, str] = {}
    visited: set[str] = set()

    for dirpath, dirnames, filenames in os.walk(str(root_path), followlinks=False):
        try:
            real_path = os.path.realpath(dirpath)
            if real_path in visited:
                dirnames.clear()
                continue
            visited.add(real_path)
        except OSError:
            dirnames.clear()
            continue

        dirnames[:] = [
            d
            for d in dirnames
            if d not in IGNORED_DIRS and (include_hidden or not d.startswith("."))
        ]

        for filename in filenames:
            if not include_hidden and filename.startswith("."):
                continue

            _, ext = os.path.splitext(filename)
            if ext.lower() in BINARY_EXTENSIONS:
                continue

            if filename.endswith("-lock.json"):
                continue

            filepath = os.path.join(dirpath, filename)

            try:
                size = os.path.getsize(filepath)
                if size > MAX_FILE_SIZE:
                    continue

                with open(filepath, "rb") as f:
                    raw = f.read()

                if b"\x00" in raw[:8192]:
                    continue

                try:
                    content = raw.decode("utf-8")
                except UnicodeDecodeError:
                    continue

                if match_all or regex.search(content):
                    results[filepath] = content

            except (OSError, PermissionError):
                continue

    return results
