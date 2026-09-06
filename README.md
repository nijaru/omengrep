# omengrep (og)

Local semantic code search: hybrid BM25 + static embeddings, single binary, no daemon.

```bash
cargo install --path crates/og
og build ./src
og "authentication flow" ./src
```

## What it does

omengrep extracts functions, classes, and methods from source files using tree-sitter, then indexes each block with static embeddings plus BM25 keywords. Queries match against both indexes, so searching "error handling" finds `errorHandler()` and `AppError` — not just comments containing those words.

```bash
$ og build ./src
Indexed 418 blocks from 62 files

$ og "error handling" ./src
crates/og-core/src/search.rs:63 function run_similar
  pub fn run_similar(ref_path: &str, line: Option<usize>, name: Option<&str>, k: usize) -> Result<Vec<SearchResult>> {

crates/og-core/src/boost.rs:1 module boost
  //! Code-aware ranking boosts: exact-name match, type match, path match.

2 results (0.27s)
```

| Query            | grep finds                | omengrep finds                |
| ---------------- | ------------------------- | ----------------------------- |
| "error handling" | Comments mentioning it    | `errorHandler()`, `AppError`  |
| "authentication" | Strings containing "auth" | `login()`, `verify_token()`   |
| "database"       | Config files, comments    | `Connection`, `query()`, `Db` |

Use grep/ripgrep for exact strings. Use omengrep when you want implementations, not mentions.

## Install

Requires the pinned Rust nightly toolchain (`rust-toolchain.toml`).

```bash
git clone https://github.com/nijaru/omengrep && cd omengrep
cargo install --path crates/og        # binary: og
```

From crates.io (once published): `cargo install omengrep` — same `og` binary.

The embedding model (~33 MB) downloads automatically on first use — announced before it starts — and is cached for offline use. The cache honors `HF_HOME`; `og model status` is read-only, `og model install` prefetches.

## Usage

```bash
og build [path]                # Build index (required first)
og "query" [path]              # Search
og file.rs#func_name           # Find code similar to a named block
og file.rs:42                  # Find code similar to a specific line
og outline [path]              # Show indexed block structure
og context [path]              # Show ranked file/symbol context
og status [path]               # Index health: freshness, size, corruption
og clean [path]                # Delete index
og model status                # Show embedding model status
og model install               # Download the embedding model

# Options
og -n 5 "error handling" .     # Limit to 5 results
og --json "auth" .             # JSON output
og --no-content "auth" .       # JSON output without source content
og --highlight "auth" .        # Highlight query-related tokens in previews
og -l "config" .               # List matching files only
og -t py,js "api" .            # Filter by file type
og --exclude "tests/*" "fn" .  # Exclude patterns
og --code-only "handler" .     # Skip docs (md, txt, rst)
og --no-semantic "handler" .   # BM25 + trigram only, no vectors
og outline --skeleton .        # Full signatures without bodies
og context -n 12 --symbols 5 . # Ranked files/symbols, token-budgeted
```

Set `OG_AUTO_BUILD=1` to build the index automatically on first search. Searching a changed tree auto-updates the index first.

Exit codes: 0 = match found, 1 = no match, 2 = error.

### Shell completions

`og` emits a [usage](https://usage.build) KDL spec consumable by
[usage-cli](https://usage.build) shell completion:

```bash
og __usage_spec__ > /tmp/og.kdl
usage g completion -s zsh /tmp/og.kdl   # also: bash, fish
```

### `og status`

```
Index root:    /path/to/repo
Generation:   g-a992432d728b (2.8 MB)
Status:       up to date        # or: stale / corrupt (with recovery hint)
Files:         141
Blocks:        941
Model:         minishlab/potion-code-16M-v2@b06ea69c8c55@1879a46ba038
Dimensions:    256
```

## How it works

Tree-sitter parses source files into AST blocks (functions, classes, methods). Each block is indexed three ways:

1. **Embeddings** — static Model2Vec vectors ([potion-code-16M-v2](https://huggingface.co/minishlab/potion-code-16M-v2), 256-d), stored as fp16 rows in a memory-mapped sidecar and searched with an exact parallel scan.
2. **BM25** — keyword search over identifier-split terms (SQLite FTS5), OR-matched so partial terms rank instead of filtering to zero.
3. **Trigram** — substring identifier matching (FTS5 trigram index over block names).

At search time the channels fuse with RRF, then code-aware boosts (exact name, path, type) reorder. Everything runs locally on CPU; no server, no daemon.

## Index format (`.og/`)

```text
.og/
├── CURRENT                  # pointer file: published generation name
└── generations/
    └── g-<hash>/            # content + model + schema hash
        ├── catalog.sqlite   # files, blocks, block_fts, block_trigram
        ├── vectors-000.bin  # fp16 rows at blocks.id positions
        └── manifest.json    # model identity, dims, schema version
```

Builds publish atomically: a staging generation is completed, then `CURRENT` flips to it. Readers never see a partial index, and a killed build leaves the previous generation readable. Fingerprint diffing re-embeds only changed files on rebuild.

**Upgrading from 0.0.3 or earlier:** the index format changed completely (previously OmenDB-backed). Delete old `.og` directories or just run `og build` — the new core builds a fresh generation and leaves legacy files untouched.

## Benchmarks

Measured numbers live in [`bench/results/`](bench/results/) with methodology, environment, and raw-data notes (og-only, per project policy). Current gates: p99 575ms at 500k blocks, search RSS under 500 MB, identifier Recall@10 1.000 on the repo qrels.

## Supported languages

**Code** (25 languages): Bash, C, C++, C#, CSS, Elixir, Go, HCL, HTML, Java, JavaScript, JSON, Kotlin, Lua, PHP, Python, Ruby, Rust, Swift, TOML, TypeScript, YAML, Zig

**Text**: Markdown, plain text (chunked by headers)

## License

MIT
