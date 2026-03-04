# Competitive Analysis: Local Semantic Code Search CLIs

**Researched:** 2026-03-03
**Scope:** Local/offline tools only. Cloud-SaaS tools included where they have a local mode or are commonly confused with local tools.

---

## Summary Table

| Tool                 | Lang       | Local?                        | Model                                   | Approach                                    | Stars  | Status                            |
| -------------------- | ---------- | ----------------------------- | --------------------------------------- | ------------------------------------------- | ------ | --------------------------------- |
| **ColGrep**          | Rust       | Yes (100%)                    | LateOn-Code-edge 17M                    | Multi-vector ColBERT + NextPlaid DB         | 173    | Active (v1.0.8, Feb 2026)         |
| **omengrep (og)**    | Rust       | Yes (100%)                    | LateOn-Code-edge 17M INT8               | Multi-vector ColBERT + omendb MuVERA + BM25 | —      | Active (v0.0.1, Feb 2026)         |
| **smgrep**           | Rust       | Yes (100%)                    | IBM Granite + answerai-colbert reranker | Dense embed + ColBERT rerank + LanceDB      | 56     | Early (v0.6.0, Nov 2025, stalled) |
| **grepai**           | Go         | Yes (Ollama) / Optional cloud | nomic-embed-text (Ollama) or OpenAI     | Single-vector + call graph                  | 1,300  | Active (v0.33.0, Feb 2026)        |
| **osgrep**           | TypeScript | Yes (100%)                    | onnxruntime-node + ColBERT rerank       | Dense embed + ColBERT rerank                | 988    | Stable (v0.5.16, Dec 2025)        |
| **mgrep**            | TypeScript | No (cloud-backed)             | Mixedbread Search (proprietary)         | Cloud sync + semantic retrieval + rerank    | 3,391  | Active (v0.1.10, Jan 2026)        |
| **vexor**            | Python     | Optional (local mode)         | Configurable (OpenAI/Gemini/local)      | Multi-mode indexing, configurable providers | 204    | Active (v0.23.1, Feb 2026)        |
| **ast-grep (sg)**    | Rust       | Yes (100%)                    | None (structural only)                  | AST pattern matching, no embeddings         | 12,710 | Active (v0.41.0, Feb 2026)        |
| **Augment Code CLI** | Node       | No (cloud)                    | Custom (proprietary)                    | Cloud context engine, Google Cloud index    | N/A    | Beta (commercial)                 |

---

## 1. ColGrep (LightOn)

**Repository:** https://github.com/lightonai/next-plaid
**Blog post:** https://www.lighton.ai/lighton-blogs/lateon-code-colgrep-lighton (Feb 12, 2026)
**Latest release:** v1.0.8 (Feb 25, 2026) | 19 releases since Jan 2026

### Overview

ColGrep is omengrep's closest direct competitor: same model (LateOn-Code-edge 17M, same authors), same language (Rust), same philosophy (local-first, grep-like CLI). Built on NextPlaid (PLAID-style multi-vector DB). Released publicly February 12, 2026 — about two weeks after omengrep's architecture was already set.

### Architecture

- **Vector DB:** NextPlaid — token centroid clustering + inverted index (PLAID algorithm)
- **Model:** LateOn-Code-edge (17M, INT8 quantization, ColBERT-style)
- **Index storage:** Memory-mapped, quantized (2-bit or 4-bit product quantization), SQLite for metadata
- **Parsing:** Tree-sitter, 23 programming languages + 9 document formats
- **Hybrid:** Regex pre-filter (ERE) + semantic re-ranking
- **Incremental:** Content-hash based, skips unchanged files

### Invocation

```bash
colgrep init                            # build index
colgrep init /path/to/project           # specific path
colgrep "database connection pooling"   # semantic search
colgrep -e "async.*await" "error handling"  # hybrid regex + semantic
colgrep -k 20 -n 10 "query"            # 20 results, 10 context lines
colgrep -l "query"                      # files-only output
colgrep -c "query"                      # full function content
colgrep --json "query"                  # JSON output
colgrep --code-only "query"             # skip docs/config
colgrep --include "*.rs" "query"        # glob filter
colgrep status                          # index status
colgrep clear                           # delete index
```

### Key Flags

| Flag                      | Purpose                    |
| ------------------------- | -------------------------- |
| `-e/--pattern`            | Regex pre-filter (ERE)     |
| `-k/--results`            | Result count (default: 15) |
| `-n/--lines`              | Context lines (default: 6) |
| `-l/--files-only`         | List files only            |
| `-c/--content`            | Full function body         |
| `--json`                  | JSON output                |
| `--code-only`             | Skip docs/config           |
| `--include/--exclude`     | Glob filtering             |
| `--model`                 | Override model             |
| `--no-pool/--pool-factor` | Embedding compression      |

### Agent Integration

`--install-claude-code`, `--install-opencode`, `--install-codex` flags install MCP/skill integrations.

### Performance / Benchmarks

**Model quality (MTEB Code v1):**

- LateOn-Code-edge (17M): **66.64** avg score — outperforms EmbeddingGemma-300M (68.76 at 300M params), granite-small (55.84 at 47M)
- LateOn-Code (130M): **74.12** — outperforms embeddinggemma-300M and CodeRankEmbed-137M

**Full MTEB Code v1 comparison:**
| Model | Params | Avg |
|-------|--------|-----|
| BM25 | — | 44.41 |
| granite-embedding-small-r2 | 47M | 55.84 |
| LateOn-Code-edge | 17M | **66.64** |
| granite-embedding-r2 | 149M | 57.22 |
| CodeRankEmbed | 137M | 60.47 |
| embeddinggemma-300M | 300M | 68.76 |
| LateOn-Code | 149M | **74.12** |
| C2LLM-0.5B | 500M | 75.46 |
| Qwen3-Embedding-0.6B | 600M | 75.42 |

**End-to-end agentic eval (135 questions, 7 repos, Claude Opus 4.5 judge):**

- Win rate vs. grep: **70%**
- Token savings: **15.7% overall**, up to 72% on complex queries
- Fewer search operations: **56% reduction**

**Weaknesses they disclosed:** TRL repo underperformed (descriptive function names make grep competitive). No indexing throughput, search latency p50/p99, or recall@k published.

### Differentiators vs omengrep

| Dimension          | ColGrep                           | omengrep                              |
| ------------------ | --------------------------------- | ------------------------------------- |
| Vector DB          | NextPlaid (PLAID token centroids) | omendb (MuVERA)                       |
| BM25 hybrid        | Regex pre-filter only             | Full BM25 hybrid merge                |
| Index hierarchy    | Single flat index                 | Parent-child hierarchy, merge subdirs |
| Auto-update        | Hash-based                        | mtime pre-check + hash                |
| File references    | Not documented                    | `file#func`, `file:42`                |
| Model size         | 17M (also 130M option)            | 17M INT8 only                         |
| ONNX quantization  | INT8                              | INT8                                  |
| Published recall@k | No                                | R@10=6%, MRR=0.046 (our own bench)    |

---

## 2. smgrep (can1357)

**Repository:** https://github.com/can1357/smgrep
**Latest release:** v0.6.0 (Nov 29, 2025) — appears stalled at 6 releases over 3 days

### Overview

Rust CLI, GPU-accelerated, local-first. Uses IBM Granite dense embeddings + ColBERT reranker (answerai-colbert-small-v1) + LanceDB. CUDA + Metal support. Similar tree-sitter chunking.

### Architecture

- **Dense embed:** `ibm-granite/granite-embedding-small-english-r2`
- **Reranker:** `answerdotai/answerai-colbert-small-v1` (ColBERT)
- **Vector DB:** LanceDB
- **GPU:** CUDA (candle) or Metal (Apple Silicon), CPU fallback
- **Parsing:** Tree-sitter WASM (downloaded on demand), 37 languages

### Invocation

```bash
smgrep setup                            # download models (~500MB)
smgrep "where do we handle auth?"       # search (auto-indexes on first run)
smgrep -m 20 "query"                    # max results
smgrep --per-file 2 "query"             # results per file
smgrep -c "query"                       # show full content
smgrep --json "query"                   # JSON output
smgrep --no-rerank "query"              # skip ColBERT rerank
smgrep -s "query"                       # force re-index
```

### Notes

- Model download is ~500MB upfront (significantly heavier than omengrep's ~17MB)
- Batch size 48 (configurable to 96, auto-adapts on OOM)
- Native MCP server + Claude Code integration
- Last activity: November 2025. Only 56 stars, appears effectively unmaintained

---

## 3. grepai (yoanbernabeu)

**Repository:** https://github.com/yoanbernabeu/grepai
**Docs:** https://yoanbernabeu.github.io/grepai/
**Latest release:** v0.33.0 (Feb 22, 2026) | 1,300 stars

### Overview

Go CLI, 100% local with Ollama. Uses nomic-embed-text (or OpenAI as cloud fallback). Key differentiator: **call graph tracing** (`grepai trace callers/callees`). File watcher daemon for real-time indexing. MCP server.

### Architecture

- **Model:** `nomic-embed-text` via Ollama (local) or `text-embedding-3-small` (OpenAI)
- **Approach:** Single-vector embedding (not multi-vector/ColBERT)
- **DB:** Not specified (likely SQLite + FAISS or similar)
- **Parsing:** Tree-sitter (call graph extraction for 12+ languages)
- **Daemon:** Background watcher for real-time index updates

### Invocation

```bash
grepai init                             # initialize project
grepai watch                            # start background daemon
grepai search "user authentication flow"   # semantic search
grepai trace callers "handleLogin"      # who calls this?
grepai trace callees "handleLogin"      # what does this call?
grepai status                           # index status
grepai mcp-serve                        # start MCP server
grepai agent-setup                      # configure Claude Code/Cursor/Windsurf
```

### Performance Claims

- Indexes ~1,247 files in 3.2 seconds
- Search results in milliseconds
- Re-index single file: ~0.1s

### Language Support

Call graph tracing: Go, TypeScript, JavaScript, Python, PHP, Java, C, C#, F#, C++, Rust, Zig

### Competitive Notes

- Call graph tracing is a unique differentiator not present in ColGrep or omengrep
- Requires Ollama daemon running locally; heavier operational overhead
- Single-vector approach — lower retrieval quality ceiling than multi-vector ColBERT
- 27% token savings claimed by external blogger (vs. omengrep/ColGrep's 15-50% range)

---

## 4. osgrep (Ryandonofrio3)

**Repository:** https://github.com/Ryandonofrio3/osgrep
**Latest release:** v0.5.16 (Dec 9, 2025) | 988 stars, 48 releases

### Overview

TypeScript CLI using `onnxruntime-node` for local embeddings + ColBERT reranking. Interesting because it separates "Code" and "Docs" indices and has `skeleton` command for code structure preview. Last release Dec 2025 — activity may have slowed.

### Architecture

- **Embed:** onnxruntime-node (local ONNX, model not specified)
- **Reranker:** ColBERT (local)
- **Parsing:** TreeSitter grammars, 14+ languages
- **DB:** Resident in-memory (daemon, <50ms responses claimed)

### Invocation

```bash
osgrep serve                            # start background daemon
osgrep search "query" -m 10 --scores   # search with scores
osgrep trace "functionName"             # call graph
osgrep skeleton                         # code structure preview
osgrep index --reset                    # manual reindex
```

### Performance Claims

- <50ms responses via daemon
- ~20% LLM token savings, 30% speedup (cited from public benchmarks)
- Bounded concurrency: 1-4 thread pools

### Notes

- Separate indices for Code and Docs (novel approach)
- "skeleton" command shows function signatures without bodies (useful for AI agent orientation)
- Last release Dec 2025, possible slowdown

---

## 5. mgrep (Mixedbread)

**Repository:** https://github.com/mixedbread-ai/mgrep
**Website:** https://mgrep.dev/
**Latest release:** v0.1.10 (Jan 23, 2026) | 3,391 stars

### Overview

**NOT local-only.** mgrep syncs files to Mixedbread's cloud "Stores." Code leaves your machine. This is a cloud-backed semantic search tool with a CLI frontend. Included here because it's commonly encountered in the same discussions.

### Architecture

- **Backend:** Mixedbread cloud infrastructure (proprietary)
- **Model:** Mixedbread Search (proprietary, state-of-the-art retrieval + reranking)
- **Sync:** Background watcher, respects .gitignore + .mgrepignore
- **Reranking:** Default on (disable with `--no-rerank`)
- **Multi-modal:** Images, PDFs (code, audio/video coming)

### Invocation

```bash
npm i -g @mixedbread/mgrep              # install (TypeScript, not Rust)
mgrep "database connection"             # search
mgrep "auth middleware" --web           # also search the web
mgrep "complex query" --agentic         # multi-query refinement
mgrep --answer "query"                  # summarized answer
mgrep -m 5 "query"                      # limit results
```

### Performance Claims

- 50% fewer tokens vs grep in 50-task benchmark with Claude Code
- "Cloud-backed" = indexing/search latency dependent on network

### Disqualifier for omengrep comparison

Files leave the machine. Not air-gap compatible. Requires API key. Data privacy concerns for proprietary code. Mentioned for completeness because it dominates HN/X mentions.

---

## 6. ast-grep (sg)

**Repository:** https://github.com/ast-grep/ast-grep
**Latest release:** v0.41.0 (Feb 22, 2026) | 12,710 stars

### Overview

**Structural search, not semantic.** No embeddings, no natural language queries. Pattern-based AST matching — find code by syntactic structure, not meaning. Completely orthogonal to omengrep's use case. Included because it's in CLAUDE.md as a standard tool and often mentioned alongside semantic search.

### What it does

Pattern: `$A && $A()` matches any `expr && expr()` regardless of variable names. Think grep for code structure, not text or meaning.

### Invocation

```bash
sg -p '$A && $A()' src/               # structural search
sg -p '$A && $A()' -r '$A?.()' src/   # search + rewrite
sg scan                               # lint with rules
```

### Comparison

- Local: Yes, fully offline
- Semantic: No — structural only
- Languages: 20+ (tree-sitter based)
- Stars: 12,710 — most established tool in this space
- Use case: Refactoring, linting, codemods — not "find where auth happens"

---

## 7. Augment Code (auggie CLI)

**Docs:** https://docs.augmentcode.com/cli

### Overview

**Cloud-only.** Auggie CLI is a terminal agent powered by Augment's cloud "Context Engine." Not a local semantic search tool — it's a full coding agent that requires an Augment account.

### Technical claims

From their engineering blog:

- Indexes "thousands of files/second" using Google Cloud
- Custom AI models (not generic embeddings)
- Real-time index updates (within seconds of changes)
- 8x memory reduction via quantized vector search: 2GB → 250MB for 100M LOC
- Search latency: 2+ seconds → under 200ms (40% faster)
- 99.9% accuracy maintained after quantization

### Install

```bash
npm install -g @augmentcode/auggie
auggie login                            # requires account
```

### Disqualifier

Cloud. Requires login. Code indexed on Google Cloud. No offline mode.

---

## 8. vexor (scarletkc)

**Repository:** https://github.com/scarletkc/vexor
**Latest release:** v0.23.1rc1 (Feb 24, 2026) | 204 stars

### Overview

Python CLI with configurable embedding/reranking providers. Supports local mode with `pip install "vexor[local]"` or CUDA variant. Also has desktop GUI (Electron). Multiple index modes: `name`, `head`, `brief`, `full`, `code` (AST-aware for Python/JS), `outline`.

### Architecture

- **Embed:** Configurable — OpenAI, Gemini, custom, or local ONNX
- **Rerank:** BM25, FlashRank, or remote endpoint
- **Index modes:** name / head / brief / full / code / outline / auto
- **Parsing:** AST-aware for Python and JavaScript only

### Invocation

```bash
vexor init                              # guided setup wizard
vexor "api client config"               # search current dir
vexor search "query" --path ~/repo --top 5
vexor search "query" --mode code        # AST-aware mode
vexor search "query" --no-cache         # in-memory only
vexor search "query" --format porcelain # TSV output for scripts
vexor search "query" --ext .py,.md      # filter extensions
```

### Notes

- Most configurable provider setup in the landscape
- Local mode available but not default
- AST support limited to Python + JS (vs. omengrep's 25 languages)
- Still in RC releases

---

## 9. Additional Minor Tools

| Tool              | Lang   | Stars | Notes                                        |
| ----------------- | ------ | ----- | -------------------------------------------- |
| **odino**         | Python | 14    | Local ONNX, EmbeddingGemma-300M, minimal     |
| **ogrep**         | Python | —     | OpenAI/Voyage AI required, not local         |
| **codebased**     | Python | 21    | Archived Sep 2024, OpenAI required           |
| **reflex-search** | Rust   | 10    | Trigram + tree-sitter symbols, no embeddings |
| **codeqai**       | Python | —     | Local-first, LLM chat + search               |

---

## Key Differentiators for omengrep

### Where omengrep is distinct from all competitors

1. **MuVERA vs. PLAID:** omendb uses MuVERA (dimension-reduced MaxSim approximation) vs. ColGrep's NextPlaid (token centroid clustering). Different quality/speed tradeoffs — MuVERA is less studied at the tool level but theoretically superior for recall on tail queries.

2. **True BM25 hybrid merge:** omengrep merges BM25 candidates with vector candidates before reranking. ColGrep uses regex as a pre-filter only — different semantics. omengrep's BM25 uses a custom camelCase/snake_case tokenizer (from omendb) which is significant for code identifier retrieval.

3. **Index hierarchy:** Parent directory refuses to index if child already indexed; sub-directory indexes are merged via fast vector copy. No other tool has this.

4. **File references:** `og file.rs#func` and `og file.rs:42` find code similar to a named block or line — unique capability.

5. **Published recall numbers:** Only tool in this landscape with published MRR/recall@k (even if modest: MRR=0.046, R@10=6% on CodeSearchNet). ColGrep only published win-rate.

### Where omengrep lags

1. **Call graph tracing:** grepai and osgrep have this; omengrep does not.
2. **Real-time daemon:** grepai, osgrep, and smgrep have background file watchers with sub-second update latency. omengrep auto-updates on search via mtime check but has no daemon.
3. **Stars/visibility:** ColGrep (173), grepai (1,300), osgrep (988), mgrep (3,391) — omengrep is effectively unknown.
4. **MRR quality:** MRR=0.046 on CodeSearchNet NL queries reveals the LateOn-Code-edge model is primarily optimized for code-to-code retrieval. NL→code quality ceiling is a model limitation shared with ColGrep.
5. **Language count:** smgrep (37), grepai (12+ call graph), vs omengrep (25). Close.

---

## Benchmark Gap Analysis

No tool in this landscape has published:

- Indexing throughput (files/sec or blocks/sec) on a standardized corpus
- Search latency p50/p99 on a standardized query set
- Recall@k against CoIR or CodeSearchNet ground truth
- Memory usage profile at scale

This is a gap omengrep could fill. The ripgrep methodology (pinned Linux kernel corpus, 10 timed runs, warmup) is the right template.

ColGrep's end-to-end QA evaluation is currently the best published benchmark but it measures agent task quality, not retrieval precision, and is confounded by the LLM judge.

---

## References

- ColGrep / LateOn-Code: https://github.com/lightonai/next-plaid, https://www.lighton.ai/lighton-blogs/lateon-code-colgrep-lighton
- smgrep: https://github.com/can1357/smgrep
- grepai: https://github.com/yoanbernabeu/grepai
- osgrep: https://github.com/Ryandonofrio3/osgrep
- mgrep: https://github.com/mixedbread-ai/mgrep
- vexor: https://github.com/scarletkc/vexor
- ast-grep: https://github.com/ast-grep/ast-grep
- Augment Code CLI: https://docs.augmentcode.com/cli
- Augment quantized search blog: https://augmentcode.com/blog/repo-scale-100M-line-codebase-quantized-vector-search
- MTEB Code v1: https://huggingface.co/blog/lightonai/colgrep-lateon-code
- LateOn-Code HF model card: https://huggingface.co/lightonai/LateOn-Code
