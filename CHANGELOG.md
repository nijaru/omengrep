# Changelog

## [0.1.0] - 2026-09-03

Complete Rust rewrite on owned storage (no OmenDB, no ONNX runtime). The
index format changed: delete old `.og` directories or run `og build`,
which builds a fresh generation alongside legacy files without touching
them. Old `.og` indexes are not readable by 0.1.0 and vice versa.

### Added

- `og outline` / `og context` ported onto the new catalog, with
  token-budgeted packing (`--max-tokens`, default 8000) for agent use.
- `og model status` / `og model install` for the default embedding model.
- `og build --force` for explicit full rebuilds; `--deterministic` test
  embedder hatch (no model download, no semantic signal).
- Incremental builds: fingerprint diffing re-embeds only changed files;
  generations publish atomically via CURRENT; stale auto-update before
  search; `OG_AUTO_BUILD=1` auto-indexing.
- `bench/`: deterministic corpus generator, scale/parity/eval harnesses,
  repo qrels fixture, and measured gate results in `bench/results/`.
- Measured gates (M3 Max, 16 cores): query p99 575ms at 500k blocks,
  search RSS under 500 MB (flat exact scan, no ANN), identifier
  Recall@10 1.000 on the repo qrels.

### Changed

- Default model is now `minishlab/potion-code-16M-v2` (static Model2Vec,
  256-d, MIT), loaded natively — no ONNX runtime, sub-100ms startup,
  offline after first download.
- Vector sidecar rows are fp16 (halved scan RSS); BM25 runs on SQLite
  FTS5 with identifier-split terms (OR-matched) plus a trigram channel;
  channels fuse with RRF (k=60) then code-aware boosts.
- Install path is now `cargo install --path crates/og` (workspace).

### Removed

- OmenDB storage backend and the ONNX embedding runtime (+ `ort`).
- MCP server (`og mcp`), `og list`, and `og install-claude-code`.
  CLI + `--json` is the agent interface.

### Fixed

- Lexical search OR-matches and BM25-ranks instead of AND-filtering to
  empty on partial term matches.
- Generation names mix content + model + schema, so upgrades publish a
  new directory instead of stranding an old row format behind a new
  reader.
- Cached models resolve without network revalidation (was ~2.4s per
  invocation).

## [0.0.3] - 2026-04-26

### Added

- `og context <file|dir>` — show ranked file and symbol context from the existing index. Supports `--json`, `--skeleton`, `-n`, and `--symbols`.
- `--highlight` — highlight query-related tokens in default terminal preview output without changing JSON/files-only output.
- Migration to Rust Edition 2024.
- High-performance zero-copy indexing using `into_par_iter` to reduce string allocations.
- Optimized Tree-sitter extraction with direct UTF-8 text access.
- Improved fallback indexing for large/unparseable files using byte-indexed line limits.
- **Performance:** Parallelized `ignore::WalkBuilder` during file scanning.
- **Performance:** Parallelized hybrid search (BM25 and MaxSim run concurrently via `rayon::join`) on multi-core CPUs.
- **UX:** Integrated `indicatif` spinner for clean, glitch-free progress reporting during `og build`.

### Changed

- Updated `omendb` to `0.0.37` (registry).
- Updated direct dependencies for the release baseline, including `hf-hub`, `indicatif`, `ort`, `rand`, and Tree-sitter crates.
- Clean builds now stream file contents through indexing instead of retaining all file bodies up front.
- Reduced embed batch size to bound ONNX runtime memory during indexing.
- `og context` now caps high-fanout reference scoring and filters generic high-frequency definitions from ranked context.
- Benchmarks modernized to match latest `omendb` API and traits.

### Fixed

- Avoided a build deadlock caused by nested Rayon use between extraction workers and tokenizer padding.
- Fixed noisy context output where generic symbol names could dominate file rankings.
- Fixed extraction of decorated Python symbols so decorators do not replace function/class names or types.

### Removed

- Removed the experimental MCP server and install command. The supported agent interface is the CLI with JSON/no-content/files-only output.
- Removed the `repomap` alias. Use `og context` for ranked file and symbol context.

## [0.0.2] - 2026-03-04

### Added

- `og outline <file|dir>` — show block structure (name, type, line) for indexed files without content. Reads manifest metadata directly, no embedder load. Supports `--json` output.
- `--context/-C N` — configurable content preview lines (default 5). No width truncation; terminal wraps naturally.
- `--regex/-e PATTERN` — post-search filter applied to content and block name.
- BM25 synonym expansion (`crates/og-core/src/synonyms.rs`) — ~120-entry vocabulary table expands query terms at search time (e.g., `auth` → `authenticate login session token`). No model or index rebuild required.

### Changed

- `--no-content` replaces `--compact/-c`.
- `--threshold` is now the primary flag; `--min-score` alias removed (v0.0.x, no compatibility obligation).
- `doc_max_length` bumped from 512 to 1024 tokens — large functions no longer truncated. Model supports up to 2048.

### Fixed

- Scope filter sibling directory leak (`starts_with` → exact match + slash guard).
- Markdown chunk IDs — only the last chunk survived before; `chunk_idx` now part of block ID.
- Score display — showed "N% similar" instead of raw score.
- Double ONNX model load on `og clean` and `og status`.
- Dead code branches in status/clean model version check.

## [0.0.1] - 2026-02-23

Initial release.

- Semantic code search with multi-vector embeddings + BM25 hybrid
- Tree-sitter extraction (25 languages)
- LateOn-Code-edge INT8 model (17M params, 48d/token)
- omendb multi-vector store with MuVERA MaxSim reranking
- File references: `file#name`, `file:line`
- Index hierarchy: build checks parent, merges subdirs
- Auto-update: mtime pre-check before each search
- Code-aware ranking boosts
