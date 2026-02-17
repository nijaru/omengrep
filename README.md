# omengrep (og)

**Code search that understands what you're looking for**

```bash
cargo install --path .
og build ./src
og "authentication flow" ./src
```

## What it does

Search code using natural language. Instead of matching exact strings like grep, og understands the meaning of your query and finds relevant code — even when the words don't match.

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

## Why og over grep?

grep finds exact text. og understands what you're looking for.

| Query            | grep finds                | og finds                      |
| ---------------- | ------------------------- | ----------------------------- |
| "error handling" | Comments mentioning it    | `errorHandler()`, `AppError`  |
| "authentication" | Strings containing "auth" | `login()`, `verify_token()`   |
| "database"       | Config files, comments    | `Connection`, `query()`, `Db` |

Use grep/ripgrep for exact strings (`TODO`, `FIXME`, import statements).
Use og when you want implementations, not mentions.

## Install

Requires Rust nightly toolchain.

```bash
git clone https://github.com/nijaru/omengrep && cd omengrep
cargo install --path .
```

The embedding model (~17MB) downloads automatically on first use.

## Usage

```bash
og build [path]                # Build index (required before searching)
og "query" [path]              # Search with natural language
og file.rs#func_name           # Find code similar to a named block
og file.rs:42                  # Find code similar to a specific line
og status [path]               # Show index info
og list [path]                 # List all indexes under path
og clean [path]                # Delete index
og mcp                         # Start MCP server (for AI agents)

# Options
og -n 5 "error handling" .     # Limit to 5 results
og --json "auth" .             # JSON output (for scripts/agents)
og -l "config" .               # List matching files only
og -t py,js "api" .            # Filter by file type
og --exclude "tests/*" "fn" .  # Exclude patterns
```

## How it Works

og combines meaning-based search with keyword matching.

**Indexing:** og parses your code into logical blocks (functions, classes, methods) using tree-sitter, then builds two indexes:

1. **Meaning-based** — a small AI model creates a representation of each code block that captures what the code _does_, so searching "authentication flow" finds `login()` and `verify_token()`.
2. **Keyword-based** — traditional text matching, so searching "getUserProfile" still finds that exact function name.

**Searching:** og runs both searches in parallel and merges the results, keeping the best matches. It also boosts results where identifier names (like `camelCase` or `snake_case` tokens) match your query. Typical search latency is 270-440ms.

Everything runs locally on CPU with a small model (~17MB). No GPU, no API keys, no cloud.

Built on [omendb](https://github.com/nijaru/omendb).

## Supported Files

**Code** (25 languages): Bash, C, C++, C#, CSS, Elixir, Go, HCL, HTML, Java, JavaScript, JSON, Kotlin, Lua, PHP, Python, Ruby, Rust, Swift, TOML, TypeScript, YAML, Zig

**Text**: Markdown, plain text — smart chunking with header context

## Technical Details

For those interested in the internals:

- **Multi-vector embeddings:** Each code block gets per-token embeddings (ColBERT-style via [MuVERA](https://arxiv.org/abs/2405.19504)), not a single vector. Token-level matching captures structural patterns that pooled single vectors lose.
- **Hybrid retrieval:** BM25 (tantivy) generates keyword candidates, MuVERA MaxSim reranks them using token-level similarity.
- **AST extraction:** tree-sitter parses code into semantic blocks (functions, classes, methods), giving precise results instead of whole-file matches.
- **Code-aware boost:** Post-search heuristics for camelCase/snake_case identifier matching, type-aware ranking, and file path relevance.

```
Build:  Scan (gitignore-aware) -> Extract (tree-sitter AST) -> Embed (ONNX INT8, 48d/token) -> Store (omendb multi-vector + BM25)
Search: Embed query -> BM25 candidates + semantic candidates -> Merge by ID -> Code-aware boost -> Results
```

## License

MIT
