# hygrep (hhg)

**Hybrid semantic code search — multi-vector embeddings + BM25**

## Quick Reference

```bash
cargo build --release                 # Build
cargo install --path .                # Install binary
hhg build ./src                       # Build index (required first)
hhg "query" ./src                     # Semantic search (text query)
hhg file.rs#func                      # Find similar code (by name)
hhg file.rs:42                        # Find similar code (by line)
cargo test                            # Run tests
```

## Architecture

```
Build:  Scan (ignore crate) -> Extract (tree-sitter, 25 langs) -> Embed (ort, LateOn-Code-edge INT8) -> Store (omendb multi-vector) + index_text (BM25)
Search: Embed query -> search_multi_with_text (BM25 candidates + MuVERA MaxSim rerank) -> Code-aware boost -> Results

Index hierarchy:
- Building: checks parent (refuses if exists), merges subdirs (fast vector copy)
- Searching: walks up to find index, filters results to search scope
```

| Component  | Implementation                                        |
| ---------- | ----------------------------------------------------- |
| Scanner    | `ignore` crate (gitignore-aware, binary detection)    |
| Extraction | Tree-sitter AST (25 languages)                        |
| Embeddings | `ort` (LateOn-Code-edge INT8 ONNX, 48d/token)        |
| Vector DB  | omendb (MuVERA multi-vector + BM25 hybrid)            |
| Boosting   | Code-aware heuristics (name match, type, path)        |

## Project Structure

```
src/
├── main.rs                 # Entry point
├── lib.rs                  # Re-exports
├── types.rs                # Block, SearchResult, FileRef
├── boost.rs                # Code-aware ranking boosts
├── cli/
│   ├── mod.rs              # Command dispatch (clap)
│   ├── search.rs           # Search command + file ref parsing
│   ├── build.rs            # Build/update index
│   ├── status.rs           # Index status
│   ├── clean.rs            # Delete index
│   ├── list.rs             # List indexes
│   ├── model.rs            # Model management
│   └── output.rs           # Result formatting (default, json, compact, files-only)
├── embedder/
│   ├── mod.rs              # Embedder trait + factory
│   ├── onnx.rs             # ORT ONNX inference (LateOn-Code-edge)
│   └── tokenizer.rs        # HuggingFace tokenizer wrapper
├── extractor/
│   ├── mod.rs              # Tree-sitter extraction coordinator
│   ├── languages.rs        # Language registry (extension -> parser + query)
│   ├── queries.rs          # Tree-sitter query definitions per language
│   └── text.rs             # Markdown/prose chunking
└── index/
    ├── mod.rs              # SemanticIndex (omendb multi-vector)
    ├── manifest.rs         # Manifest v8 (JSON, tracks files/hashes/blocks)
    └── walker.rs           # File walker (ignore crate, gitignore-aware)
Cargo.toml
```

## Technology Stack

| Component    | Version                  | Notes                                 |
| ------------ | ------------------------ | ------------------------------------- |
| Rust         | nightly-2025-12-04       | Required for omendb (portable_simd)   |
| ort          | 2.0.0-rc.11              | ONNX Runtime inference                |
| omendb       | 0.0.27 (path dep)        | Multi-vector + BM25 hybrid search     |
| tree-sitter  | 0.25                     | AST parsing (25 languages)            |
| Embeddings   | LateOn-Code-edge INT8    | 17M params, 48d/token, ~17MB          |

## Code Standards

| Aspect     | Standard                                           |
| ---------- | -------------------------------------------------- |
| Edition    | 2021 (moving to 2024)                              |
| Errors     | `anyhow` (app), `thiserror` (lib boundaries)       |
| Imports    | `crate::` over `super::`, stdlib -> external -> local |
| Parallelism| `rayon` for CPU-bound extraction                   |
| Strings    | `&str` > `String`, `&[T]` > `Vec<T>` where possible |

## Verification

| Check  | Command                          | Pass Criteria   |
| ------ | -------------------------------- | --------------- |
| Build  | `cargo build --release`          | Zero errors     |
| Test   | `cargo test`                     | All pass        |
| Smoke  | `hhg "test" ./src`               | Returns results |
| Lint   | `cargo clippy`                   | No warnings     |

## Key Behaviors

- `HHG_AUTO_BUILD=1` — auto-build index on search if missing
- Auto-update: search detects stale files and re-indexes before searching
- Exit codes: 0 = match found, 1 = no match, 2 = error
- File refs: `file#name` (by block name), `file:line` (by line number)
- Output formats: default (colored), `--json`, `--json --compact`, `-l` (files only)

## AI Context

**Read order:** `ai/STATUS.md` -> `ai/DECISIONS.md`

| File              | Purpose                          |
| ----------------- | -------------------------------- |
| `ai/STATUS.md`    | Current state, blockers, roadmap |
| `ai/DECISIONS.md` | Architectural decisions          |
