## Current State

| Metric    | Value                          | Updated    |
| --------- | ------------------------------ | ---------- |
| Package   | omengrep 0.0.1 (binary: og)    | 2026-02-16 |
| Models    | LateOn-Code-edge (48d, single) | 2026-02-16 |
| omendb    | 0.0.29 (multi-vector+compact)  | 2026-02-21 |
| Toolchain | nightly-2025-12-04             | 2026-02-14 |
| Tests     | 26 (12 unit + 14 integration)  | 2026-02-16 |

## Architecture

```
Build:  Scan -> Extract (tree-sitter) -> Split identifiers -> Embed (ort, LateOn-Code-edge) -> Store (omendb multi-vector compact) + index_text (BM25)
Search: Embed query (query tokenizer, 256 max) -> BM25 candidates + semantic candidates -> Merge by ID -> Code-aware boost -> Results
MCP:    og mcp (JSON-RPC/stdio) -> og_search, og_similar, og_status tools
```

## Remaining Work

### Publish to crates.io (tk-4f2n)

- Blocked on omendb crates.io publish (currently path dependency)
- Release pipeline ready: `.github/workflows/release.yml` (6-stage)
- Homebrew formula ready: `nijaru/homebrew-tap/Formula/og.rb`
- Tag `v0.1.0` when unblocked

### Testing

- Test MCP server with Claude Code (`og install-claude-code`)
- Rebuild indexes after query tokenizer fix (search quality changed)

## Future Ideas

- **Regex pre-filter** — `--regex` flag to filter before semantic ranking (ColGrep has this)
- **LateOn-Code (149M)** — 11% higher MTEB Code, same architecture, just swap model
- **SPLADE sparse vectors** — wait for omendb native support, evaluate `granite-embedding-30m-sparse`
- **NL enrichment** — embed NL description alongside raw code (Greptile found ~12% retrieval improvement)

## Competitive Context

Primary competitor: **ColGrep** (LightOn, Feb 2026). Same model (LateOn-Code-edge), same architecture (ColBERT + tree-sitter). Uses NextPlaid (PLAID algo) vs omendb (MuVERA). SQLite metadata filtering vs our BM25 hybrid. Ships with agent integrations (Claude Code, OpenCode, Codex).

omengrep advantages: BM25 hybrid ranking, code-aware boost heuristics, file reference search (`file#name`, `file:line`), find_similar.

## omendb Requests

1. **Custom tantivy tokenizer** in `TextSearchConfig` — for camelCase/snake_case splitting
2. **Native sparse vector support** — for future SPLADE integration
3. **tantivy lru vulnerability** — filed in `omendb/tantivy-lru-vulnerability.md`

## Key Files

| File                   | Purpose                             |
| ---------------------- | ----------------------------------- |
| `src/cli/search.rs`    | SearchParams, search + file refs    |
| `src/cli/build.rs`     | Build/update index (shared helper)  |
| `src/cli/mcp.rs`       | MCP server + install helper         |
| `src/embedder/mod.rs`  | MODEL config, embedder factory      |
| `src/embedder/onnx.rs` | ORT inference (query vs doc paths)  |
| `src/index/mod.rs`     | SemanticIndex (omendb multi-vector) |
| `src/tokenize.rs`      | BM25 identifier splitting           |
| `src/boost.rs`         | Code-aware ranking boosts           |
| `tests/cli.rs`         | Integration tests (assert_cmd)      |
