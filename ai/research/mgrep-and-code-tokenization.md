# mgrep Architecture & BM25 Code Tokenization Research

**Date:** 2026-02-14

---

## 1. mgrep Architecture

### What it is

Cloud-backed semantic search CLI from Mixedbread. TypeScript (84%), Apache 2.0. ~3.2k stars. Designed as "semantic grep" for AI coding agents.

### Embedding model

**Proprietary / undisclosed.** mgrep uses Mixedbread's cloud inference -- files are uploaded to a "Mixedbread Store" and embedding + retrieval happens server-side. No local model, no disclosed model name or dimensions.

### Search architecture

| Component | Detail |
|-----------|--------|
| Indexing | Upload files to Mixedbread cloud via API |
| Retrieval | Cloud semantic search (model unknown) |
| Reranking | Mixedbread reranker (on by default, `--no-rerank` to disable) |
| BM25 | Not documented -- appears to be semantic-only with reranking |
| Hybrid | Implied but not confirmed |

### Code preprocessing

**None documented.** No AST extraction, no camelCase splitting, no language-specific tokenization. Files are:
- Discovered via `git ls-files` or recursive traversal
- Filtered through `.gitignore` + `.mgrepignore`
- Binary-filtered (text only)
- Hashed (SHA256 for delta sync)
- Chunked (line ranges in results suggest chunking, strategy unknown)

### What we can learn from mgrep

**Very little technically** -- it is a thin CLI wrapper around a cloud API. Its value is UX and agent integration, not novel search techniques. The proprietary backend prevents inspection.

However, the Mixedbread *research* team created **baguetter** and the **BMX algorithm** (paper: arXiv:2408.06643), which IS interesting -- see Section 3 below.

---

## 2. Semantic Code Search Landscape

### osgrep (open source mgrep alternative)

| Aspect | Detail |
|--------|--------|
| Model | Granite + ColBERT (embeddings) |
| Search | **Hybrid: Vector + BM25 + RRF fusion + ColBERT rerank** |
| Indexing | Local, transformers.js |
| Architecture | `Query -> [Vector Search] + [BM25] -> [RRF Fusion] -> [ColBERT Rerank] -> Results` |

Notable: osgrep independently arrived at the same hybrid architecture hygrep uses. The ColBERT reranking step on top of hybrid retrieval is a pattern worth noting.

### GitHub Blackbird (code search)

| Aspect | Detail |
|--------|--------|
| Engine | Custom Rust search engine |
| Tokenization | **Ngram-based** (character sequences, not words) |
| Stemming | **None** -- preserves exact character sequences |
| Stop words | **None removed** -- common code tokens kept |
| Punctuation | **Preserved** -- searchable (`.`, `()`, etc.) |
| Symbols | Extracted separately; content, symbols, paths are separate indices |
| Language | Linguist language IDs for filtering |

**Key insight:** GitHub deliberately avoids NLP-style preprocessing for code. No stemming, no stop words, no case folding. Code is not natural language and should not be treated as such for lexical search. Ngram indices handle substring matching naturally.

### sturdy-dev/semantic-code-search

| Aspect | Detail |
|--------|--------|
| Model | sentence-t5-base fine-tuned on CodeSearchNet |
| Extraction | Function/method extraction by language |
| Search | Pure vector similarity (no BM25) |
| Preprocessing | Identifier subtokenization (camelCase/snake_case split) |

### CodeSearchNet Challenge (academic baseline)

The foundational benchmark for semantic code search. Key finding on preprocessing:

| Preprocessing strategy | BM25 NDCG@10 improvement |
|------------------------|--------------------------|
| No code tokenization | Baseline (e.g., Python: 0.4317) |
| **camelCase/snake_case splitting** | **+38% avg** (Python: 0.5989) |
| + remove reserved tokens | +37% avg (marginal over splitting alone) |

**This is the single most impactful finding for hygrep's Priority 2 (BM25 Code Tokenization).**

---

## 3. BM25 Code Tokenization Best Practices

### The problem

Standard BM25 tokenizers (whitespace + punctuation split) treat `getUserName` as a single token. A query for "user name" will never match it. Same problem with `get_user_name` (matched as `get_user_name`, not `get`, `user`, `name`) and `config.database.host` (matched as `config`, `database`, `host` after dot-split, but only if dots are tokenizer delimiters).

### Evidence-based solutions

#### A. Identifier splitting (highest impact, lowest cost)

From CodeSearchNet (2019) and "Bag-of-Words Baselines for Semantic Code Search" (2021):

**Split camelCase and snake_case into constituent tokens:**
- `getUserName` -> `get`, `user`, `name`, `getUserName` (keep original too)
- `get_user_name` -> `get`, `user`, `name`, `get_user_name`
- `HTTPSConnection` -> `HTTPS`, `Connection`, `HTTPSConnection`

**Results:**
- BM25 NDCG@10 jumps from 0.43 to 0.58 average across 6 languages (+35%)
- Ruby improves most: 0.4484 -> 0.5789 (+29%)
- Go improves least: already 0.6979 -> 0.7289 (+4%, Go uses short names)
- **This single change makes BM25 competitive with early neural models (NBoW, SelfAtt)**

#### B. Reserved token removal (marginal)

Removing language reserved words (`if`, `for`, `return`, `class`, etc.) has marginal or slightly negative effect on BM25. These tokens have high IDF naturally since they appear in most documents, so BM25 already downweights them.

#### C. Stemming (mixed for code)

Porter stemming helps natural language queries but can hurt code tokens:
- `sorted` -> `sort` (good: matches `sort`, `sorting`)
- `getter` -> `get` (good)
- `class` -> `class` (no change, already a root)
- `indexes` -> `index` (good)
- `axes` -> `ax` (bad: not meaningful for code)

**Recommendation:** Apply stemming to the query but NOT to code tokens. Or use a code-aware stemmer that only stems English words, not identifiers.

#### D. Ngram indexing (GitHub's approach)

Character ngrams (trigrams, etc.) handle substring matching without explicit splitting:
- `getUserName` with trigrams: `get`, `etU`, `tUs`, `Use`, `ser`, `erN`, `rNa`, `Nam`, `ame`
- Query "user" matches via shared trigrams

**Pros:** Language-agnostic, handles any naming convention, no splitting heuristics.
**Cons:** Larger index, more false positives, slower than inverted index.

#### E. Custom tokenizer pipeline (recommended for hygrep)

Best practice combining the above:

```
Input:  "pub fn get_user_name(config: &AppConfig) -> String"

Step 1 - Standard tokenization:
  ["pub", "fn", "get_user_name", "config", "AppConfig", "String"]

Step 2 - Identifier splitting (keep originals):
  ["pub", "fn", "get_user_name", "get", "user", "name",
   "config", "AppConfig", "App", "Config", "String"]

Step 3 - Lowercase normalization:
  ["pub", "fn", "get_user_name", "get", "user", "name",
   "config", "appconfig", "app", "config", "string"]

Step 4 - Optional: remove language keywords:
  ["get_user_name", "get", "user", "name",
   "appconfig", "app", "config", "string"]
```

### F. JetBrains buckwheat tokenizer

Purpose-built multi-language identifier tokenizer. Pipeline:
1. Language detection (via enry)
2. Tree-sitter parsing (16 languages) to extract identifiers
3. **Subtokenization:** split by camelCase + snake_case, merge short subtokens with adjacent, then stem

This is essentially the academic best practice implemented as a library. Supports: C, C#, C++, Go, Haskell, Java, JavaScript, Kotlin, PHP, Python, Ruby, Rust, Scala, Shell, Swift, TypeScript.

### G. Mixedbread BMX (BM25 extension)

From arXiv:2408.06643. Extends BM25 with:
1. **Entropy-weighted similarity:** Uses token entropy to weight query-document similarity
2. **Semantic enhancement:** Embeds tokens and uses semantic similarity alongside lexical matching
3. **Score normalization:** Normalizes BM25 scores for better fusion with other signals

This is interesting but complex. The simple identifier splitting gives most of the gains.

---

## 4. Recommendations for hygrep

### Priority 2 (BM25 Code Tokenization) -- immediate actions

**Action 1: Pre-split identifiers before `index_text()`**

The Rust implementation should split camelCase/snake_case identifiers in the `embedding_text` before passing to omendb's BM25 indexer. This is the highest-impact, lowest-cost improvement.

```
// Regex approach:
// camelCase: insert space before uppercase following lowercase
// snake_case: replace _ with space
// dot.notation: replace . with space
// Keep original identifier as well

"getUserName" -> "getUserName get User Name"
"get_user_name" -> "get_user_name get user name"
"config.database" -> "config.database config database"
```

Expected impact: **+30-38% improvement in BM25 keyword recall** based on CodeSearchNet results.

**Action 2: Apply same splitting to search queries**

When a user queries "getUserName" or "get_user_name", split the query terms the same way so BM25 can match the split tokens in the index.

**Action 3: Consider requesting tantivy custom tokenizer in omendb**

The clean solution is a tantivy tokenizer that handles code identifiers natively. This avoids doubling the text size by pre-splitting. Already noted in STATUS.md as an omendb request.

### Not recommended

- **Ngram indexing:** Overkill for hygrep's scale; identifier splitting covers the main cases
- **Reserved token removal:** Marginal benefit, BM25 IDF already handles this
- **BMX algorithm:** Interesting but complex; hybrid search (vector + BM25) already bridges the semantic gap
- **Code-specific stemming:** Small benefit, risk of harming code token matching

---

## 5. Tool Comparison Matrix

| Tool | Model | Search Type | Code Preprocessing | Local/Cloud | BM25 Tokenization |
|------|-------|-------------|-------------------|-------------|-------------------|
| **hygrep** | LateOn-Code-edge (17M) | Multi-vector + BM25 hybrid | Tree-sitter AST extraction | Local | **Needs improvement** |
| mgrep | Proprietary (Mixedbread) | Semantic + reranking | None documented | Cloud | N/A (cloud) |
| osgrep | Granite + ColBERT | Vector + BM25 + RRF + ColBERT rerank | None documented | Local | Unknown |
| colgrep | LateOn-Code-edge | PLAID multi-vector | None | Local | Regex (no BM25) |
| GitHub Blackbird | N/A (lexical) | Ngram + symbol extraction | Symbol extraction | Cloud | Ngram (no splitting needed) |
| Elasticsearch | Configurable | BM25 + kNN | None by default | Self-hosted | Standard tokenizer |

### hygrep's advantages

1. **Tree-sitter AST extraction** -- most tools just chunk by lines or paragraphs. Extracting functions/classes gives semantically coherent blocks.
2. **LateOn-Code-edge** -- the best small code-specific multi-vector model available.
3. **Hybrid search** -- BM25 + multi-vector covers both exact and semantic matching.
4. **Local-first** -- no API keys, no cloud dependency.

### hygrep's gaps (vs competition)

1. **BM25 tokenization** -- standard tokenizer misses camelCase/snake_case. This is Priority 2.
2. **No reranking step** -- osgrep adds ColBERT reranking after RRF fusion. hygrep uses MuVERA MaxSim rerank in the Rust version but the Python version does not.
3. **No query expansion** -- GNO (another tool) expands queries into lexical and semantic variants via LLM. Interesting but heavy.

---

## Sources

- [mgrep GitHub](https://github.com/mixedbread-ai/mgrep)
- [mgrep DeepWiki](https://deepwiki.com/mixedbread-ai/mgrep)
- [mgrep.dev](https://mgrep.dev/)
- [GitHub Blog: Technology behind code search](https://github.blog/2023-02-06-the-technology-behind-githubs-new-code-search/)
- [JetBrains buckwheat tokenizer](https://github.com/JetBrains-Research/buckwheat)
- [Bag-of-Words Baselines for Semantic Code Search (ACL 2021)](https://aclanthology.org/2021.nlp4prog-1.10.pdf)
- [CodeSearchNet Challenge (arXiv:1909.09436)](https://arxiv.org/abs/1909.09436)
- [Mixedbread baguetter / BMX (arXiv:2408.06643)](https://github.com/mixedbread-ai/baguetter)
- [osgrep](https://github.com/Ryandonofrio3/osgrep)
- [GNO search architecture](https://www.gno.sh/docs/HOW-SEARCH-WORKS)

_Research date: 2026-02-14_
