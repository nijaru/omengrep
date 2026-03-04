## Current State

| Metric    | Value                          | Updated    |
| --------- | ------------------------------ | ---------- |
| Package   | omengrep 0.0.1 → 0.0.2 (wip)   | 2026-03-04 |
| Models    | LateOn-Code-edge (48d, single) | 2026-02-16 |
| omendb    | 0.0.30 (multi-vector+compact)  | 2026-02-23 |
| Manifest  | v10 (mtime field)              | 2026-02-23 |
| Toolchain | nightly-2025-12-04             | 2026-02-14 |
| Tests     | 17 integration, 12 unit        | 2026-03-04 |
| Boost     | Fixed (divide not multiply)    | 2026-03-03 |

## Architecture

```
Build:  Scan -> Extract (tree-sitter, nested dedup) -> Split identifiers (keyword filter) -> Embed (ort, LateOn-Code-edge) -> Store (omendb multi-vector compact) + index_text (BM25)
Search: scan_metadata (stat only) -> check_and_update (mtime pre-check, read only changed) -> Embed query -> BM25+semantic merge -> Code-aware boost -> Results
MCP:    og mcp (JSON-RPC/stdio) -> og_search, og_similar, og_status tools
```

## Active Work

v0.0.2 release ready (all commits on main, no tag yet). See `docs/plans/2026-03-03-v0.0.2-release.md`.

## Bugs Fixed This Session (2026-03-03/04)

| Bug                                                                    | Fix                                                          |
| ---------------------------------------------------------------------- | ------------------------------------------------------------ |
| Scope filter leaked sibling dirs (`src/cli` matched `src/cli_utils`)   | Exact match + trailing slash guard                           |
| Markdown chunk duplicate IDs (only last chunk survived in index)       | `chunk_idx` added to block ID                                |
| Similar search showed `-40099% similar` for negative MaxSim scores     | Show raw score with `score:` label                           |
| Double ONNX load on format-change rebuild                              | Replace `SemanticIndex::new().clear()` with `remove_dir_all` |
| Dead `"different model"` branch in status/clean                        | Removed; manifest only emits "older version"                 |
| Boost sign bug: `score *= boost` on negative scores (previous session) | `score /= boost` for negative scores                         |

All bugs have regression tests. 29/29 tests pass.

## Key Findings

**doc_max_length = 512 is omengrep's own limit** (src/embedder/mod.rs:25 `ModelConfig`), NOT omendb.
omendb stores whatever vectors the embedder produces — no internal token count constraint.
LateOn-Code-edge supports up to 2048 document tokens (from model's onnx_config.json).
Large functions silently lose their tail. Bumping doc_max_length is safe and high-impact.

**Boost fix impact:** MRR 0.0062 → 0.0458 (7.4x), first non-zero R@1=0.04, R@5=0.06.
R@10=6% ceiling is a retrieval limit (BM25 candidates), not a ranking limit.

**ColBERT prefix investigation (previous session):** `[Q]`/`[D]` prefixes tokenize correctly
(IDs 50368/50369) but hurt performance (R@10 2% vs 6%). Reverted. Do not retry.

## CLI Assessment (2026-03-04)

**Good:** Positional `og "query" ./path` is natural. Standard short flags. Subcommands clean.
File refs (`file#func`, `file:line`) unique and powerful. Auto-update on search. Gitignore-aware.

**Issues:**

- `--compact`/`-c` outputs compact JSON (no content), but name implies compact text.
  ColGrep uses `-c` for full content (opposite!). Rename to `--no-content`.
- Content preview hardcoded at 3 lines / 80 chars. ColGrep shows 6. Needs `--context N` flag.
- `--min-score` / `--threshold` naming: threshold is the alias, reads oddly in help.
- No regex pre-filter flag (`-e pattern`) — ColGrep parity gap.

## Competitive Position (2026-03-04)

See `ai/research/competitive-analysis-2026-03.md` for full analysis.

**Local tools:** ColGrep (Rust, PLAID, same model), grepai (Go, Ollama), osgrep (TS, daemon), smgrep (stalled).
**mgrep:** cloud (files leave machine) — not a local tool despite local CLI.

**Advantages:** File refs unique, true BM25 hybrid merge (ColGrep regex-only), index hierarchy, only tool with published recall@k.
**Gaps:** No 130M model option (+7.5 MTEB points), no call graph tracing, no published perf numbers.

## Roadmap

| Task                             | Priority | Notes                                |
| -------------------------------- | -------- | ------------------------------------ |
| v0.0.2 release (tk-jw5v)         | p2       | Tag, publish, GH release             |
| Bump doc_max_length (tk-n2yk)    | p2       | 512→1024/2048, easy win for quality  |
| CLI improvements (tk-h8pv)       | p3       | --no-content, --context, --regex     |
| Code synonym expansion (tk-bb7o) | p3       | BM25 recall, no model needed         |
| Publish benchmarks (tk-i4b4)     | p3       | Indexing throughput + search latency |
| 130M model support               | p3       | Needs doc_max_length bump first      |
| Call graph tracing               | p4       | grepai parity                        |

## Benchmarks

Performance bench: `benches/omendb.rs` (divan)

| Benchmark       | Baseline (0.0.30) | Current  | delta |
| --------------- | ----------------- | -------- | ----- |
| search_hybrid   | 392.3 us          | 404.5 us | +3%   |
| search_semantic | 422.0 us          | 539.4 us | +28%  |
| store_write     | 5.25 ms           | 6.169 ms | +18%  |

Quality bench: `bench/quality.py` (CodeSearchNet, 2000 corpus seed=42)

| Run                  | MRR@10 | R@1  | R@5  | R@10 | Date       |
| -------------------- | ------ | ---- | ---- | ---- | ---------- |
| baseline             | 0.0082 | 0.00 | 0.00 | 0.08 | 2026-02-22 |
| after a2a0a02 bundle | 0.0062 | 0.00 | 0.00 | 0.06 | 2026-02-24 |
| boost fixed          | 0.0458 | 0.04 | 0.06 | 0.06 | 2026-03-03 |

R@10 ceiling at 6% is BM25 retrieval limit on NL→code task. Model optimized for code-to-code.

## omendb Notes (user is maintainer)

1. **Custom tantivy tokenizer** — for camelCase/snake_case splitting in BM25
2. **Native sparse vector support** — for future SPLADE integration
3. **max_tokens in VectorStore** — exists in source but user confirms omendb does NOT constrain document token count; omengrep's ModelConfig is the only limit.

## Key Files

| File                    | Purpose                                        |
| ----------------------- | ---------------------------------------------- |
| `src/cli/mod.rs`        | CLI definition (clap), arg dispatch            |
| `src/cli/search.rs`     | SearchParams, search + file refs               |
| `src/cli/build.rs`      | Build/update index (shared helper)             |
| `src/cli/output.rs`     | Result formatting (default/json/compact/files) |
| `src/index/mod.rs`      | SemanticIndex (omendb multi-vector)            |
| `src/index/manifest.rs` | Manifest v10 (mtime field)                     |
| `src/embedder/mod.rs`   | ModelConfig (doc_max_length: 512)              |
| `src/boost.rs`          | Code-aware ranking boosts                      |
| `src/extractor/mod.rs`  | Extraction + nested block dedup                |
| `src/tokenize.rs`       | BM25 identifier splitting                      |
| `tests/cli.rs`          | Integration tests (assert_cmd)                 |
