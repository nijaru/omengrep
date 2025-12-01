# HyperGrep (hygrep)

**Agent-Native search tool — fast enough for humans, structured for agents.**

## Project Overview

**hgrep** is a high-performance CLI tool that combines instant directory scanning (like `ripgrep`) with semantic reranking (using local LLMs via MAX Engine). It is designed specifically to be the "eyes" for AI agents, outputting structured, token-aware JSON or serving directly via MCP.

## Core Architecture: "Hyper Hybrid"

1.  **Hyper Scanner (Mojo)**: Parallel directory walk + Fuzzy/Regex matching. (Goal: <50ms)
2.  **Semantic Reranker (MAX)**: Cross-encoder model scores candidates. (Goal: ~200ms)
3.  **Agent Formatter**: JSON output with token counting and truncation.

## Project Structure

| Directory | Purpose |
|-----------|---------|
| `src/` | Mojo source code |
| `models/` | ONNX models (downloaded/exported) |
| `ai/` | **AI session context** - workspace for tracking state across sessions |

### AI Context Organization

**Purpose:** AI maintains project context between sessions using `ai/`.

**Session files** (read every session):
- `ai/STATUS.md` — Current state, metrics, blockers (read FIRST)
- `ai/TODO.md` — Active tasks and backlog
- `ai/DECISIONS.md` — Architectural decisions
- `ai/PLAN.md` — Strategic roadmap
- `ai/RESEARCH.md` — Research index

**Reference files** (loaded on demand):
- `ai/research/` — Detailed research
- `ai/design/` — Design specifications
- `ai/decisions/` — Archived decisions

## Technology Stack

-   **Language**: Mojo (Nightly/Stable)
-   **Inference**: ONNX Runtime (via Python Interop)
-   **Regex**: `libc` (FFI)
-   **CLI Parsing**: Manual (Phase 3)
-   **Models**: `mxbai-rerank-xsmall-v1` (INT8 ONNX)
-   **Protocol**: Standard CLI / JSON

## Commands

```bash
# Build
pixi run build

# Run search (Auto-Hybrid)
./hygrep "query" ./src
```

## Development Phases

1.  **Hyper Scanner**: Recreate `ripgrep` functionality (Scanner) - **Done**
2.  **The Brain**: Integrate ONNX Reranker - **Done**
3.  **CLI Polish**: Professional CLI experience - **In Progress**

See `ai/PLAN.md` for detailed roadmap.

`~/github/modular/modular` for latest modular docs, src, stdlib for mojo and max
