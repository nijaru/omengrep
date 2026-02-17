## Current State

| Metric    | Value                         | Updated    |
| --------- | ----------------------------- | ---------- |
| Package   | omengrep 0.0.1 (binary: og)   | 2026-02-16 |
| Models    | edge (48d), full (128d)       | 2026-02-16 |
| omendb    | 0.0.28 (multi-vector+compact) | 2026-02-16 |
| Toolchain | nightly-2025-12-04            | 2026-02-14 |
| Tests     | 26 (12 unit + 14 integration) | 2026-02-16 |

## Architecture

```
Build:  Scan -> Extract (tree-sitter) -> Split identifiers -> Embed (ort, LateOn-Code model) -> Store (omendb multi-vector compact) + index_text (BM25)
Search: Resolve model from manifest -> Embed query -> BM25 candidates + semantic candidates -> Merge by ID -> Code-aware boost -> Results
MCP:    og mcp (JSON-RPC/stdio) -> og_search, og_similar, og_status tools
```

## Recent Changes (2026-02-16)

Sprint 1-4 implemented in single session:

1. **Search quality**: MaxSim reranking in find_similar, merged BM25+semantic candidates, restored BM25 TF signal
2. **Token pooling**: MultiVectorConfig::compact() — 50% storage, 100.6% quality
3. **Multi-model**: ModelConfig struct, parameterized embedder, auto-detect from manifest, `--model` CLI flag
4. **MCP server**: `og mcp` (JSON-RPC/stdio), `og install-claude-code` helper

7 commits on main.

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

- Test `og build --model full` with larger model
- Test MCP server with Claude Code (`og install-claude-code`)
- Benchmark full vs edge model quality on real codebases

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
| `src/cli/search.rs`    | Search command + file ref parsing   |
| `src/cli/build.rs`     | Build/update index                  |
| `src/cli/mcp.rs`       | MCP server + install helper         |
| `src/embedder/mod.rs`  | ModelConfig, model registry         |
| `src/embedder/onnx.rs` | ORT inference (parameterized)       |
| `src/index/mod.rs`     | SemanticIndex (omendb multi-vector) |
| `src/tokenize.rs`      | BM25 identifier splitting           |
| `src/boost.rs`         | Code-aware ranking boosts           |
| `tests/cli.rs`         | Integration tests (assert_cmd)      |
