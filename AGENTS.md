# HyperGrep (hygrep)

**Agent-Native search tool — fast enough for humans, structured for agents.**

## Project Overview

**hygrep** is a high-performance CLI tool that combines instant directory scanning (like `ripgrep`) with semantic reranking (using local LLMs via ONNX Runtime).

## Core Architecture: "Hyper Hybrid"

1.  **Hyper Scanner (Mojo)**: Parallel directory walk + Fuzzy/Regex matching.
    *   **Current:** Sequential + Python `re` (Functional Prototype).
    *   **Goal:** Parallel + `libc`/`pcre` (High Performance).
2.  **Semantic Reranker (Mojo/Python)**: Cross-encoder model scores candidates.
    *   **Current:** ONNX Runtime (via Python Interop).
    *   **Model:** `mixedbread-ai/mxbai-rerank-xsmall-v1`.
3.  **Agent Formatter**: JSON output.

## Project Structure

| Directory | Purpose |
|-----------|---------|
| `src/` | Mojo source code |
| `models/` | ONNX models (downloaded) |
| `ai/` | **AI session context** |

## Technology Stack

-   **Language**: Mojo (Nightly/Stable)
-   **Inference**: ONNX Runtime (via Python Interop)
-   **Regex**: Python `re` (Prototype) -> `libc` (Goal)
-   **Models**: `mxbai-rerank-xsmall-v1` (INT8 ONNX)

## Commands

```bash
# Build
pixi run build

# Run search (Auto-Hybrid)
./hygrep "query" ./src
```

## Development Phases

1.  **Prototype**: Functional End-to-End - **Done**
2.  **Optimization**: Make it Fast (Parallel Walk, Native Regex) - **Pending**
3.  **Polish**: Professional CLI - **Pending**
