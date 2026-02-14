## Current State

| Metric    | Value                      | Updated    |
| --------- | -------------------------- | ---------- |
| Version   | 0.1.0 (Rust)               | 2026-02-14 |
| Model     | LateOn-Code-edge (48d/tok) | 2026-02-14 |
| omendb    | 0.0.27 (multi-vector)      | 2026-02-14 |
| Toolchain | nightly-2025-12-04         | 2026-02-14 |

## Rust Rewrite

**Status:** Phases 1-6 complete. Compiles, CLI works, all commands implemented.

**Branch:** Merged to `main`. 3 commits, 25 files, ~7600 lines.

**Architecture:**

```
Build:  Scan (ignore crate) -> Extract (tree-sitter, 25 langs) -> Embed (ort, LateOn-Code-edge INT8) -> Store (omendb multi-vector)
Search: Embed query -> search_multi_with_text (BM25 + MuVERA MaxSim) -> Code-aware boost -> Results
```

**Key decisions:**

- Single crate (lib + bin), not workspace
- Multi-vector embeddings (48d/token, all tokens kept)
- Manifest v8 (clean break from Python v1-v7)
- omendb `search_multi_with_text` for hybrid search
- ort 2.0.0-rc.11 with `Mutex<Session>` for `&self` compatibility

## Remaining Work

### Phase 7: Polish & Parity (tk-we4e)

- Verify all CLI flags/output match Python
- Performance benchmark vs Python
- Test on real codebases
- Integration tests with assert_cmd

### Phase 8: Distribution (tk-8yhl)

- `cargo install` from git
- GitHub Actions CI (macOS-arm64, linux-x64)
- cargo-dist for binary releases
- Update README/CLAUDE.md

## Key Files (Rust)

| File                   | Purpose                             |
| ---------------------- | ----------------------------------- |
| `src/cli/search.rs`    | Search command + file ref parsing   |
| `src/cli/build.rs`     | Build/update index                  |
| `src/embedder/onnx.rs` | ORT inference (LateOn-Code-edge)    |
| `src/extractor/mod.rs` | Tree-sitter extraction coordinator  |
| `src/index/mod.rs`     | SemanticIndex (omendb multi-vector) |
| `src/index/walker.rs`  | File walker (ignore crate)          |
| `src/types.rs`         | Block, SearchResult, FileRef        |

## Research

| File                                   | Topic                |
| -------------------------------------- | -------------------- |
| `research/multi-vector-code-search.md` | ColBERT/multi-vector |
