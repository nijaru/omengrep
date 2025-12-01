# HyperGrep (`hygrep`)

> **The Hybrid Search CLI: `grep` speed, LLM intelligence.**

`hygrep` is a next-generation command-line search tool designed for developers and AI agents alike. It combines the instant performance of regex-based directory scanning with the semantic understanding of local Large Language Models (LLMs).

**Zero Setup. Zero Indexing. Local Inference.**

## Features

*   **Smart Context:** Extracts full functions and classes (Python, JS, TS, Go, Rust) instead of just matching lines.
*   **Semantic Reranking:** Understands "auth" matches "login".
*   **Agent Ready:** Outputs structured JSON with `--json`.
*   **Fast:** Scans 10,000+ files/sec (Mojo Native Scanner).
*   **Robust:** Ignores `node_modules`, binaries, and hidden files by default.

## Installation

**Prerequisites:**
- [Pixi](https://pixi.sh/) (Package Manager)

```bash
git clone https://github.com/nijaru/hypergrep
cd hypergrep
pixi install
pixi run build
```

## Usage

Run inside the Pixi environment:

```bash
pixi run ./hygrep "login logic" ./src
```

### Agent Search (JSON)
Get structured output for tool use.

```bash
pixi run ./hygrep "login logic" ./src --json
```

**Output:**
```json
[
  {
    "file": "src/auth.py",
    "type": "function",
    "name": "login",
    "score": 0.89,
    "content": "def login(user): ..."
  }
]
```

## Architecture

- **Scanner (Recall):** Pure Mojo + libc Regex. Parallel directory walker.
- **Bridge:** Python bridge to ONNX Runtime & Tree-sitter.
- **Model:** `mxbai-rerank-xsmall-v1` (Quantized).

## License

[MIT](LICENSE)