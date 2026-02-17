# omengrep (og)

Local semantic code search using embeddings and BM25.

```bash
cargo install --path .
og build ./src
og "authentication flow" ./src
```

## What it does

omengrep extracts functions, classes, and methods from source files using tree-sitter, then indexes each block with both embeddings and BM25 keywords. Queries match against both indexes, so searching "error handling" finds `errorHandler()` and `AppError` — not just comments containing those words.

```bash
$ og build ./src
Found 69 files (0.0s)
Indexed 801 blocks from 69 files (10.8s)

$ og "error handling" ./src
src/cli/search.rs:42 function handle_search
  pub fn handle_search(args: &SearchArgs) -> Result<()> {

src/types.rs:15 enum SearchError
  pub enum SearchError {
      IndexNotFound,

2 results (0.27s)
```

| Query            | grep finds                | omengrep finds                |
| ---------------- | ------------------------- | ----------------------------- |
| "error handling" | Comments mentioning it    | `errorHandler()`, `AppError`  |
| "authentication" | Strings containing "auth" | `login()`, `verify_token()`   |
| "database"       | Config files, comments    | `Connection`, `query()`, `Db` |

Use grep/ripgrep for exact strings. Use omengrep when you want implementations, not mentions.

## Install

Requires Rust nightly toolchain.

```bash
git clone https://github.com/nijaru/omengrep && cd omengrep
cargo install --path .
```

The embedding model (~17MB) downloads automatically on first use.

## Usage

```bash
og build [path]                # Build index (required first)
og "query" [path]              # Search
og file.rs#func_name           # Find code similar to a named block
og file.rs:42                  # Find code similar to a specific line
og status [path]               # Show index info
og list [path]                 # List all indexes under path
og clean [path]                # Delete index
og mcp                         # Start MCP server (JSON-RPC over stdio)

# Options
og -n 5 "error handling" .     # Limit to 5 results
og --json "auth" .             # JSON output
og -l "config" .               # List matching files only
og -t py,js "api" .            # Filter by file type
og --exclude "tests/*" "fn" .  # Exclude patterns
og --code-only "handler" .     # Skip docs (md, txt, rst)
```

Set `OG_AUTO_BUILD=1` to build the index automatically on first search.

## How it works

omengrep uses tree-sitter to parse source files into AST blocks (functions, classes, methods), then builds two indexes per block:

1. **Embedding index** — per-token embeddings from a ColBERT-style model ([LateOn-Code-edge](https://huggingface.co/answerdotai/LateOn-Code-edge), 17M params, INT8 ONNX). Stored as [MuVERA](https://arxiv.org/abs/2405.19504) compressed multi-vectors, searched with MaxSim reranking.
2. **BM25 index** — keyword search with identifier-aware tokenization.

At search time, both indexes run in parallel and results merge by ID, keeping the higher score.

Runs locally on CPU with no external dependencies. Search latency is 270-440ms.

Built on [omendb](https://github.com/nijaru/omendb).

## Supported languages

**Code** (25 languages): Bash, C, C++, C#, CSS, Elixir, Go, HCL, HTML, Java, JavaScript, JSON, Kotlin, Lua, PHP, Python, Ruby, Rust, Swift, TOML, TypeScript, YAML, Zig

**Text**: Markdown, plain text (chunked by headers)

## License

MIT
