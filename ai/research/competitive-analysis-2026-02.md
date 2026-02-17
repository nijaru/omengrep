# Competitive Analysis: Semantic Code Search (Feb 2026)

## Executive Summary

omengrep's architecture (multi-vector ColBERT + BM25 hybrid, tree-sitter AST extraction) is validated by LightOn's ColGrep release (Feb 12, 2026), which uses the exact same model (LateOn-Code-edge) and a nearly identical architecture. ColGrep is the primary competitor. The key differentiator is omengrep's use of omendb (MuVERA FDE) vs ColGrep's use of NextPlaid (PLAID algorithm). Both are valid multi-vector approaches. omengrep's main risk is feature parity: ColGrep ships with agent integrations (Claude Code, OpenCode, Codex) and hybrid regex+semantic search out of the box.

**Bottom line:** omengrep's technical choices are sound and validated. The competitive threat is execution speed and distribution, not architecture.

---

## 1. Competitor Landscape

### 1.1 ColGrep (LightOn) -- PRIMARY COMPETITOR

Released: Feb 12, 2026 | Source: github.com/lightonai/next-plaid | License: Apache 2.0

| Aspect            | ColGrep                                 | omengrep                           |
| ----------------- | --------------------------------------- | ---------------------------------- |
| Model             | LateOn-Code-edge (17M, 48d/tok)         | LateOn-Code-edge (17M, 48d/tok)    |
| Vector DB         | NextPlaid (PLAID algorithm)             | omendb (MuVERA FDE + HNSW)         |
| Text search       | SQLite metadata filtering               | BM25 (tantivy)                     |
| Hybrid mode       | Regex + semantic (--regex + --semantic) | BM25 + MaxSim rerank               |
| Extraction        | Tree-sitter (23 langs)                  | Tree-sitter (25 langs)             |
| Agent integration | Claude Code, OpenCode, Codex            | None                               |
| Output formats    | Default, JSON, verbose                  | Default, JSON, compact, files-only |
| Auto-update       | Content hash, incremental               | Content hash, incremental          |
| Language          | Rust                                    | Rust                               |

**ColGrep technical details:**

- Uses NextPlaid: K-means clustering, IVF inverted index, product quantization (2-bit/4-bit), memory-mapped
- SQLite for metadata (file path, extension, language, regex pattern pre-filtering)
- Layered static analysis: AST layer (names, signatures, params, return types, doc comments) + call graph layer ("calls" and "called by" resolved globally)
- Search pipeline: SQL filter -> multi-vector candidate retrieval via IVF -> late-interaction MaxSim scoring -> line selection
- Evaluation: 70% win rate vs vanilla grep (judged by Claude Opus 4.5), 15.7% token savings, 56% fewer search operations

**Key differences from omengrep:**

1. **Call graph analysis** -- ColGrep extracts "calls" and "called by" fields; omengrep does not
2. **RawCode units** -- ColGrep indexes top-level statements, imports, initialization logic that fall outside AST queries
3. **Regex pre-filtering** -- ColGrep supports regex as a structural filter before semantic ranking
4. **Agent hooks** -- one-command install for Claude Code, OpenCode, Codex
5. **No BM25** -- ColGrep uses SQLite metadata filtering, not tantivy BM25

**omengrep advantages:**

1. BM25 hybrid ranking (tantivy) is more sophisticated than SQLite metadata filtering for text matching
2. Code-aware boost heuristics (name match 2.5x, term overlap, type-aware)
3. File reference search (file#name, file:line) -- unique feature
4. 25 vs 23 languages
5. omendb ecosystem (shared with other tools)

### 1.2 GitHub Code Search (Blackbird)

Architecture: Custom Rust search engine, trigram-based (NOT neural/semantic)

| Aspect        | Detail                                                     |
| ------------- | ---------------------------------------------------------- |
| Approach      | Trigram indexing with dynamic ngram sizes ("sparse grams") |
| Scale         | 45M repos, 115TB code, 15.5B documents                     |
| Index size    | 25TB (from 115TB original, 28TB unique)                    |
| Ingest        | 120K docs/sec, full re-index in ~18 hours                  |
| Latency       | P99 ~100ms per shard, 640 QPS per 64-core host             |
| Sharding      | By Git blob object ID (content-addressable)                |
| ML/Embeddings | None -- purely lexical/structural                          |

**Note:** GitHub Copilot added "instant semantic code search indexing" (Mar 2025) for codebase-aware AI assistance, but details of that embedding system are not public. Blackbird itself remains pure trigram search.

**Relevance to omengrep:** Different category entirely. Blackbird solves exact substring/regex search at massive scale. omengrep solves semantic intent-based search at local/repo scale. Not direct competitors.

### 1.3 Sourcegraph / Zoekt

Architecture: Trigram indexing in Go, structural search via ctags symbols

| Aspect            | Detail                                                        |
| ----------------- | ------------------------------------------------------------- |
| Approach          | Trigram index + boolean query language + ctags symbol ranking |
| Scale             | Designed for multi-repo enterprise search                     |
| Embedding support | Had codesearch.ai prototype (archived Aug 2024), abandoned    |
| Current focus     | Large-scale lexical/structural search, NOT semantic           |

**Key insight:** Sourcegraph explicitly argues that workspace-scoped semantic search (what omengrep does) is complementary to, not competitive with, cross-org lexical search. They positioned omengrep-style tools as filling a different need.

### 1.4 bloop

**Status: Archived (Jan 2, 2025).** Repository is read-only.

Architecture (when active): Qdrant vector DB + GPT-4 for query reformulation + semantic search. Used single-vector embeddings (not multi-vector). Required OpenAI API key.

**Relevance:** Dead product. Their approach (cloud-dependent, single-vector, GPT-4 query reformulation) was the opposite of omengrep's fully-local multi-vector approach. Validates the local-first direction.

### 1.5 Greptile

Architecture: Cloud API, RAG-based code understanding

**Key findings from their engineering blog:**

- Found that raw code embeds poorly -- converting code to natural language descriptions before embedding improved similarity by ~12% (0.7280 -> 0.8152)
- File-level chunking fails; function-level chunking is essential
- No public details on which embedding model they use
- Cloud-only, no local option

**Relevance:** Validates omengrep's AST-level extraction approach. Their "translate code to NL before embedding" insight is worth investigating -- could improve omengrep's recall for conceptual queries.

### 1.6 ast-grep

Different category: structural search/rewrite tool, not semantic search.

| Aspect   | Detail                                                 |
| -------- | ------------------------------------------------------ |
| Approach | Tree-sitter pattern matching (syntax-aware grep/sed)   |
| Strength | Exact structural patterns, refactoring, linting        |
| Weakness | No semantic understanding -- cannot find "retry logic" |
| Stars    | 12.5K GitHub stars                                     |

**Relationship to omengrep:** Complementary, not competitive. ast-grep finds code by structure ("$A && $A()"), omengrep finds code by intent ("retry with exponential backoff"). The semly blog post articulates this well: ripgrep for exact text, ast-grep for declarations/patterns, semantic search for intent-level queries.

---

## 2. Embedding Model Landscape

### 2.1 Multi-Vector (ColBERT-style) Models for Code

| Model                     | Params | Dim/tok | MTEB Code Avg | License    | Notes                             |
| ------------------------- | ------ | ------- | ------------- | ---------- | --------------------------------- |
| **LateOn-Code-edge**      | 17M    | 48      | **66.64**     | Apache 2.0 | Current omengrep model            |
| **LateOn-Code**           | 149M   | 128\*   | **74.12**     | Apache 2.0 | Higher quality, 8.8x larger       |
| answerai-colbert-small-v1 | ~33M   | 96      | N/A (code)    | MIT        | General-purpose, not code-trained |
| Jina-ColBERT-v2           | 137M   | 128     | N/A (code)    | Apache 2.0 | General-purpose ColBERT           |

\*LateOn-Code dimension inferred from architecture (ModernBERT-base -> 128d projection typical for ColBERT).

**Key finding:** There are NO other multi-vector models specifically trained for code besides LateOn-Code variants. omengrep is already using the best (and only) small multi-vector code model available.

### 2.2 Single-Vector Code Embedding Models

| Model                            | Params  | Dim               | MTEB Code Avg | License        | Local-Friendly       |
| -------------------------------- | ------- | ----------------- | ------------- | -------------- | -------------------- |
| jina-code-embeddings-0.5b        | 494M    | 896 (flex 64-896) | **78.72**     | Apache 2.0     | Marginal (needs GPU) |
| jina-code-embeddings-1.5b        | 1.54B   | 896               | **78.94**     | Apache 2.0     | No (GPU required)    |
| C2LLM-0.5B                       | 500M    | --                | **75.46**     | --             | No                   |
| Qwen3-Embedding-0.6B             | 600M    | 4096 (flex)       | **75.42**     | Apache 2.0     | No                   |
| LateOn-Code (multi-vec)          | 149M    | 48\*              | **74.12**     | Apache 2.0     | Yes                  |
| GTE-ModernBERT                   | 149M    | 768               | 71.66         | Apache 2.0     | Yes                  |
| EmbeddingGemma-300M              | 300M    | --                | 68.76         | --             | Marginal             |
| **LateOn-Code-edge (multi-vec)** | **17M** | **48**            | **66.64**     | **Apache 2.0** | **Yes**              |
| granite-embedding-small-r2       | 47M     | --                | 55.84         | Apache 2.0     | Yes                  |
| CodeRankEmbed                    | 137M    | --                | 60.47         | Apache 2.0     | Yes                  |
| Nomic-embed-code                 | 7B      | --                | ~81.7 (CSNet) | Apache 2.0     | No (7B)              |
| CodeSage-Large-v2                | 1.3B    | flex (Matryoshka) | --            | Apache 2.0     | No                   |
| CodeSage-Small-v2                | 130M    | flex              | --            | Apache 2.0     | Yes                  |
| VoyageCode3                      | --      | 2048 (flex)       | --            | Proprietary    | No (API)             |

### 2.3 Model Evaluation for omengrep

**Question: What's the best small (<100MB) code embedding model?**

Answer: **LateOn-Code-edge (17M, ~17MB) is the best small model for omengrep's architecture.** No other multi-vector code model exists at this size. In the single-vector category, CodeRankEmbed (137M) and granite-embedding-small-r2 (47M) exist but score lower on MTEB Code and would require architectural changes (single-vector search instead of multi-vector).

**Question: Are there models with higher dimensions that would improve recall?**

Answer: **LateOn-Code (149M, ~500MB estimated) would improve MTEB Code from 66.64 to 74.12 (+11.2%).** This is the single most impactful upgrade available. It uses the same architecture (ColBERT/MaxSim), so omendb integration would be straightforward -- just change the dimension parameter. The 8.8x parameter increase means ~3-5x slower embedding, but search latency should be similar with MuVERA compression.

For single-vector, jina-code-embeddings-0.5b (78.72 MTEB Code) would require rearchitecting away from multi-vector, which is not recommended -- multi-vector's token-level matching is omengrep's core advantage.

**Question: What do competitors use for ranking/scoring?**

| Tool             | Ranking Approach                                   |
| ---------------- | -------------------------------------------------- |
| ColGrep          | MaxSim (late interaction) on NextPlaid IVF         |
| GitHub Blackbird | Trigram + code signals (symbol match, file type)   |
| Zoekt            | Trigram + ctags symbol ranking                     |
| Greptile         | Single-vector cosine similarity + LLM reranking    |
| bloop (dead)     | Single-vector Qdrant + GPT-4 reranking             |
| omengrep         | BM25 candidates + MuVERA MaxSim + code-aware boost |

omengrep's three-stage ranking (BM25 -> MaxSim -> code-aware boost) is the most sophisticated local approach.

---

## 3. Feature Gap Analysis

### Features competitors have that omengrep lacks:

| Feature                                       | Who Has It       | Priority | Effort                |
| --------------------------------------------- | ---------------- | -------- | --------------------- |
| Agent integration (Claude Code, Codex)        | ColGrep          | **High** | Low (MCP tool config) |
| Regex pre-filter before semantic search       | ColGrep          | Medium   | Medium                |
| Call graph extraction ("calls"/"called by")   | ColGrep          | Low      | High                  |
| NL description of code blocks for indexing    | Greptile         | Medium   | Medium                |
| Larger model option (LateOn-Code 149M)        | ColGrep          | Medium   | Low (model swap)      |
| RawCode units (imports, top-level statements) | ColGrep          | Low      | Low                   |
| Online/cloud hosted search                    | Greptile, GitHub | N/A      | N/A (not in scope)    |

### Features omengrep has that competitors lack:

| Feature                                      | Notes                                     |
| -------------------------------------------- | ----------------------------------------- |
| BM25 hybrid search (tantivy)                 | ColGrep uses SQLite, not full-text search |
| Code-aware boost heuristics                  | Unique to omengrep                        |
| File reference search (file#name, file:line) | Unique navigation paradigm                |
| find_similar (code-to-code by reference)     | No equivalent in ColGrep                  |
| Auto-build on search (OG_AUTO_BUILD)         | ColGrep requires explicit init            |

---

## 4. Recommendations

### 4.1 Immediate (before v0.1.0)

1. **Agent integration** -- Add Claude Code / Codex integration. ColGrep's `--install-claude-code` shows this is a one-command setup that writes an MCP tool definition. This is the highest-leverage feature for adoption.

2. **Support LateOn-Code (149M) as optional model** -- Allow users to select between edge (17M, fast) and standard (149M, accurate). MTEB Code jumps from 66.64 to 74.12. The model architecture is identical (ColBERT/MaxSim), only dimensions change. Implementation: add `--model` flag or config option.

### 4.2 Near-term (v0.2.0)

3. **Regex pre-filter** -- Add `--regex` flag that filters candidates before semantic ranking. ColGrep demonstrated this is valuable for hybrid workflows (e.g., `og --regex "async.*await" "error handling"`).

4. **Index top-level code** -- Extract imports, module-level statements, and other code that falls outside AST function/class/struct queries. ColGrep calls these "RawCode units."

5. **NL enrichment for indexed blocks** -- Following Greptile's finding that code-to-NL translation improves retrieval by ~12%, consider generating a brief NL summary of each code block (from AST metadata: name, params, return type, doc comment) and embedding it alongside the raw code. This could be done without an LLM by concatenating structured fields.

### 4.3 Research / Long-term

6. **Benchmark against ColGrep** -- Run both tools on identical codebases and compare precision@K, recall@K, and latency. Quantify the actual impact of BM25+boost vs SQLite+MaxSim.

7. **Evaluate LateOn-Code (149M)** -- Measure the quality/speed tradeoff. If build time stays under 10s for a medium project, this is worth offering as default with edge as the "fast" option.

8. **Call graph extraction** -- Lower priority but ColGrep's "calls" and "called by" metadata could improve ranking for dependency-style queries.

9. **SPLADE sparse vectors** -- Still waiting on omendb support. The granite-embedding-30m-sparse model (Decision #7) remains the best candidate. No code-specific SPLADE models have appeared.

---

## 5. Strategic Assessment

### Positioning

omengrep and ColGrep are converging on the same architecture. The key strategic questions:

1. **Differentiation via omendb ecosystem** -- If omendb adds features (store_with_text, custom tokenizers, sparse vectors), omengrep benefits automatically. ColGrep's NextPlaid is purpose-built for ColBERT only.

2. **BM25 advantage is real** -- ColGrep's SQLite filtering is weaker than tantivy BM25 for text matching. For queries like "get user profile settings," BM25 with camelCase splitting will find `getUserProfile` and `loadSettings` as candidates; SQLite metadata filtering cannot do this.

3. **Code-aware boost is unique** -- No competitor has post-search heuristic boosts. This is a genuine quality advantage for identifier-style queries.

4. **Distribution matters** -- ColGrep's one-command agent installs are a significant adoption advantage. Matching this is the highest priority.

### Market Reality

The semantic code search space in Feb 2026:

- **Cloud/enterprise**: Sourcegraph (lexical), Greptile (semantic API), GitHub (trigram + Copilot)
- **Local/developer**: ColGrep (new), omengrep, semly (Swift), various MCP code search servers
- **Dead**: bloop, codesearch.ai (Sourcegraph)

Multi-vector ColBERT-style search is emerging as the winning approach for local semantic code search. Both ColGrep and omengrep validate this. The BM25 baseline (44.41 MTEB Code) vs LateOn-Code-edge (66.64) shows a 50% improvement -- semantic search provides genuine value over lexical methods for code.

---

## Sources

- LightOn blog: https://huggingface.co/blog/lightonai/colgrep-lateon-code (Feb 12, 2026)
- LightOn NextPlaid: https://www.lighton.ai/lighton-blogs/introducing-lighton-nextplaid (Feb 11, 2026)
- NextPlaid/ColGrep repo: https://github.com/lightonai/next-plaid
- LateOn-Code-edge model: https://huggingface.co/lightonai/LateOn-Code-edge
- LateOn-Code model: https://huggingface.co/lightonai/LateOn-Code
- GitHub Blackbird: https://github.blog/2023-02-06-the-technology-behind-githubs-new-code-search/
- GitHub Copilot semantic indexing: https://github.blog/changelog/2025-03-12-instant-semantic-code-search-indexing/
- Zoekt: https://sourcegraph.com/github.com/sourcegraph/zoekt
- Greptile semantic search: https://www.greptile.com/blog/semantic-codebase-search
- Jina code embeddings paper: https://arxiv.org/abs/2508.21290
- Jina code embeddings 0.5b: https://huggingface.co/jinaai/jina-code-embeddings-0.5b
- CodeSage v2: https://code-representation-learning.github.io/
- Nomic embed code: https://huggingface.co/nomic-ai/nomic-embed-code
- Modal code embedding comparison: https://modal.com/blog/6-best-code-embedding-models-compared
- ColBERT-serve (ECIR 2025): https://arxiv.org/abs/2504.14903
- ColBERT in Practice: https://sease.io/2025/11/colbert-in-practice-bridging-research-and-industry.html
- Awesome multi-vector: https://github.com/DeployQL/awesome-multi-vector
- bloop (archived): https://github.com/BloopAI/bloop
