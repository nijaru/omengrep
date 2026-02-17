## Current State

| Metric    | Value                          | Updated    |
| --------- | ------------------------------ | ---------- |
| Package   | omengrep 0.0.1 (binary: og)    | 2026-02-16 |
| Models    | LateOn-Code-edge (48d, single) | 2026-02-16 |
| omendb    | 0.0.28 (multi-vector+compact)  | 2026-02-16 |
| Toolchain | nightly-2025-12-04             | 2026-02-14 |
| Tests     | 26 (12 unit + 14 integration)  | 2026-02-16 |

## Architecture

```
Build:  Scan -> Extract (tree-sitter) -> Split identifiers -> Embed (ort, LateOn-Code-edge) -> Store (omendb multi-vector compact) + index_text (BM25)
Search: Embed query (query tokenizer, 256 max) -> BM25 candidates + semantic candidates -> Merge by ID -> Code-aware boost -> Results
MCP:    og mcp (JSON-RPC/stdio) -> og_search, og_similar, og_status tools
```

## Recent Changes (2026-02-16)

Sprint 1-4 complete. Post-sprint review + refactor:

- **Query tokenizer fix**: `embed_query` now uses query tokenizer (256 max) instead of document tokenizer (512). Affects search quality.
- **Simplified to single model**: Removed multi-model registry, single LateOn-Code-edge config
- **Centralized downloads**: Single `download_model_files()` with actionable error messages
- **SearchParams struct**: Replaced 12-parameter `search::run` with struct
- **Deduplication**: Shared `build_index`, `result_from_omendb` helper, `OutputFormat::from_flags`
- **Build fix**: Double subdir cleanup message, stale TODO removed

## Remaining Work

### Publish to crates.io (tk-4f2n)

- Blocked on omendb crates.io publish (currently path dependency)
- Release pipeline ready: `.github/workflows/release.yml` (6-stage)
- Homebrew formula ready: `nijaru/homebrew-tap/Formula/og.rb`
- Tag `v0.1.0` when unblocked

### Repo Rename

- Rename GitHub repo: hygrep -> omengrep (`gh repo rename omengrep`)
- Update git remote after rename

### Testing

- Test MCP server with Claude Code (`og install-claude-code`)
- Rebuild indexes after query tokenizer fix (search quality changed)

### Future: SPLADE Sparse Vectors

- Wait for omendb native sparse support
- Evaluate `ibm-granite/granite-embedding-30m-sparse` (30M, Apache 2.0, 50.8 nDCG)

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
