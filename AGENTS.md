# HyperGrep (hygrep)

**Agent-Native search tool — fast enough for humans, structured for agents.**

## Project Overview

**hygrep** is a high-performance CLI tool designed to replace `grep` for the AI era. It combines the raw speed of regex-based directory scanning (Recall) with the semantic understanding of local Large Language Models (Rerank).

## Core Architecture: "Hyper Hybrid"

The architecture follows a strict **Recall -> Rerank** pipeline.

1.  **Hyper Scanner (Recall)**
    *   **Role:** Find ~100 candidate files from 10,000+ files.
    *   **Constraint:** **Must be Pure Mojo/C.** No Python Runtime overhead allowed per file.
    *   **Mechanism:** Parallel Directory Walk + `libc` Regex (POSIX).
2.  **Inference Engine (Rerank)**
    *   **Role:** Score candidates by semantic relevance to the query.
    *   **Constraint:** Latency < 200ms total. Python Interop is acceptable here (batch operation).
    *   **Mechanism:** ONNX Runtime (via Python) + `mxbai-rerank-xsmall-v1` (Cross-Encoder).
3.  **Agent Formatter**
    *   **Role:** Output structured JSON for tool use.

## Project Structure

| Directory | Purpose |
|-----------|---------|
| `src/` | Mojo source code root |
| `src/scanner/` | **Hyper Scanner** (Pure Mojo/FFI + Parallel) |
| `src/inference/` | **Inference Engine** (Python Interop/ONNX) |
| `models/` | Downloaded ONNX models (gitignored) |
| `ai/` | **AI Context** (See below) |

## AI Context & Workflow

Agents **MUST** follow this workflow:

1.  **Read State:** Check `ai/STATUS.md` for current focus and blockers.
2.  **Check Plan:** Refer to `ai/PLAN.md` for the active phase and next steps.
3.  **Update Context:** After completing a task, update `ai/STATUS.md`.

### Reference Files
- `ai/DECISIONS.md`: Architectural Constraints & Decisions (Read before refactoring).
- `ai/RESEARCH.md`: Links to research (Models, FFI patterns).

## Technology Stack

| Component | Technology | Note |
|-----------|------------|------|
| **Language** | Mojo (Stable v0.25.7) | Primary systems language |
| **Inference** | ONNX Runtime | via Python Interop |
| **Regex** | `libc` (POSIX) | FFI + Parallel |
| **Model** | `mxbai-rerank-xsmall-v1` | Quantized INT8 (~40MB) |
| **Package Mgr** | `pixi` | Handles Python/Mojo deps |

## Commands

```bash
# Build
pixi run build

# Test
pixi run test

# Run Search (Auto-Hybrid)
./hygrep "query" ./src
```
