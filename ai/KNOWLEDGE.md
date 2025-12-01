# Project Knowledge & Quirks

## Runtime Quirks

### Mojo & Python Version Mismatch
**Issue:** Mojo (v0.25.7) may link against a system Python (e.g. 3.14) that differs from the `.pixi` environment Python (e.g. 3.13).
**Symptom:** `Failed to initialize Smart Searcher: Python version mismatch`.
**Solution:** Always run the binary via `pixi run ./hygrep`. Pixi ensures the correct environment variables (`LD_LIBRARY_PATH` etc.) are set so Mojo finds the correct Python library.

## Architecture

### Stateless Hybrid
We intentionally avoid building an index. We rely on:
1.  **Recall:** Fast Regex scan (~20k files/sec). Pure Mojo + Parallelism.
2.  **Rerank:** Heavy semantic model (`mxbai-rerank`) on small candidate set (<100 files). Batched inference in Python.

### Smart Context
We use `tree-sitter` 0.24+ Python bindings.
*   **Code:** Extracts full functions/classes.
*   **Text/Fallback:** Sliding window (+/- 5 lines) around regex match.

### Known Limitations (Edge Cases)

1.  **Circular Symlinks:** The scanner currently follows all directories. Circular symlinks will cause an infinite loop/stack overflow.
2.  **Large Binary Files:** The scanner attempts to read files as text. We check extensions to skip binaries (`.exe`, `.png`), but a randomly named binary file might cause issues.
3.  **Hidden Files:** Files starting with `.` are skipped by default.
4.  **Memory Leak:** A small (128 byte) leak per run exists in the `Regex` struct due to Mojo `UnsafePointer` type inference limitations in v0.25.7 preventing clean destruction. This is negligible for a CLI tool.
