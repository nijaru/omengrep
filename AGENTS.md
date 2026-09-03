# omengrep (og)

**Semantic code search — static embeddings + BM25, owned SQLite storage**

## Quick Reference

```bash
cargo build --release                 # Build
cargo install --path crates/og       # Install binary (crates.io name: omengrep)
og build ./src                        # Build index (required first)
og "query" ./src                      # Semantic search (text query)
og file.rs#func                       # Find similar code (by name)
og file.rs:42                         # Find similar code (by line)
og outline ./src --skeleton           # Structural signatures
og context ./src                      # Ranked file/symbol context
cargo test                            # Run tests
```

## Architecture

```
Build:  Scan (ignore crate) -> Extract (tree-sitter, 25 langs) -> Embed (potion static Model2Vec, native loader) -> Catalog (SQLite FTS5) + Vectors (mmap fp16 sidecar)
Search: Embed query -> BM25 + trigram + exact vector scan -> RRF fuse -> Code-aware boost -> Results

Index generations (.og/):
- Build stages a generation dir, writes the manifest inside it, then flips CURRENT atomically
- Generation name mixes content + model identity + schema version (any change publishes a new dir)
- Incremental: fingerprint diff re-embeds changed files only; vector rows are position-addressed at SQLite-assigned rowids (SQLite reuses freed max rowids — never append by arithmetic)
- Searching: walks up to find index, filters results to search scope, stale-check auto-updates before search
```

| Component  | Implementation                                        |
| ---------- | ----------------------------------------------------- |
| Scanner    | `ignore` crate (gitignore-aware, binary detection)    |
| Extraction | Tree-sitter AST (25 languages)                        |
| Embeddings | Native static Model2Vec (`minishlab/potion-code-16M-v2`, 256-d) |
| Vector store | mmap fp16 sidecar + rayon exact scan (no ANN)       |
| Lexical    | SQLite FTS5 (unicode61 identifier-split terms + trigram names) |
| Fusion     | RRF (k=60) + code-aware boosts (name match, type, path) |

## Project Structure

```
crates/og/          # CLI binary (clap): search, build, status, clean, outline, context, model
crates/og-core/     # All product logic
├── lib.rs          # Re-exports
├── types.rs        # Block, SearchResult, FileRef
├── index.rs        # Generations, incremental builds, atomic CURRENT publish, open
├── catalog.rs      # SQLite: files, blocks, block_fts, block_trigram (SCHEMA_VERSION)
├── vectors.rs      # mmap fp16 sidecar, rayon exact scan, position-addressed writer
├── retrieve.rs     # BM25 + trigram + vector channels, RRF fusion
├── search.rs       # Query surface incl. file#name / file:line refs
├── context/        # outline + ranked context packing (ported policy)
├── model/          # Embedder trait, native potion loader, deterministic test embedder
├── extract/        # Tree-sitter extraction coordinator (25 languages)
├── scan.rs         # File walker (ignore crate, gitignore-aware)
├── manifest.rs     # Generation manifest (model identity, dims, schema)
├── boost.rs        # Code-aware ranking boosts
├── output.rs       # Result formatting (default, json, no-content, files-only)
├── tokenize.rs     # BM25 identifier splitting
└── synonyms.rs     # Query synonym expansion
bench/              # Corpus generator, scale/parity/eval harnesses, qrels, results
```

## Technology Stack

| Component    | Version                  | Notes                                 |
| ------------ | ------------------------ | ------------------------------------- |
| Rust         | nightly-2025-12-04       | Pinned in rust-toolchain.toml         |
| rusqlite     | bundled                  | SQLite catalog + FTS5 (no system dep) |
| memmap2      | —                        | Vector sidecar mmap                   |
| tokenizers / safetensors / half / hf-hub | —       | Native static-model loading (no ort)  |
| tree-sitter  | 0.25–0.26                | AST parsing (25 languages)            |
| Embeddings   | potion-code-16M-v2       | Static Model2Vec, 256-d, MIT          |

## Code Standards

| Aspect     | Standard                                           |
| ---------- | -------------------------------------------------- |
| Edition    | 2024                                               |
| Errors     | `anyhow` (app), `thiserror` (lib boundaries)       |
| Imports    | `crate::` over `super::`, stdlib -> external -> local |
| Parallelism| `rayon` for CPU-bound extraction                   |
| Strings    | `&str` > `String`, `&[T]` > `Vec<T>` where possible |

## Verification

| Check  | Command                          | Pass Criteria   |
| ------ | -------------------------------- | --------------- |
| Build  | `cargo build --release`          | Zero errors     |
| Test   | `cargo test`                     | All pass        |
| Smoke  | `og "test" ./src`                | Returns results |
| Lint   | `cargo clippy --all-targets -- -D warnings` | No warnings (CI lints tests too; plain `clippy` does not) |

## Public Docs and Benchmarks

- Public docs may compare `og` to grep/ripgrep as exact-string baselines.
- Do not publicly compare, rank, or name other semantic/code search tools.
- Public benchmarks must report `og` only: dataset, environment, methodology, raw results, index size, throughput, and latency.
- Local comparisons against other public tools are allowed for private product judgment, but do not publish those results or tool names.

## Key Behaviors

- `OG_AUTO_BUILD=1` — auto-build index on search if missing
- Auto-update: search detects stale files and re-indexes before searching
- Generations: builds publish atomically via CURRENT; incremental rebuilds re-embed changed files only; `og build --force` for full rebuilds
- Model identity pins the vector space: query embedding always uses the manifest's model; model/schema change forces full rebuild
- No MCP, no daemon; CLI + `--json` is the agent interface
- Exit codes: 0 = match found, 1 = no match, 2 = error
- File refs: `file#name` (by block name), `file:line` (by line number)
- Output formats: default (colored), `--json`, `--no-content`, `-l` (files only)
- `--highlight` colors query-related tokens in terminal previews only; JSON and files-only output stay unstyled.

## AI Context

**Read order:** `ai/brief.md` -> `ai/decisions.md` -> `ai/architecture.md`

| File              | Purpose                          |
| ----------------- | -------------------------------- |
| `ai/brief.md`     | Active task, direction, next step |
| `ai/decisions.md` | Architectural decisions          |
| `ai/architecture.md` | System design and components  |
