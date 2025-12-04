# Semantic Search System Design

## Problem Statement

Current hhg architecture: `Grep → Tree-sitter extraction → ONNX reranking`

This works well for **known unknowns** ("find functions with 'auth' in them") but struggles with **unknown unknowns** ("how does authentication work?"). Semantic search using embeddings can bridge this gap.

**Key tension**: Stateless instant search vs indexed semantic understanding.

## Research Findings

### Industry Best Practice: Hybrid Search

From [Elastic](https://www.elastic.co/search-labs/blog/search-relevance-tuning-in-semantic-search) and [Zilliz](https://zilliz.com/blog/semantic-search-vs-lexical-search-vs-full-text-search):

| Approach              | Strengths                          | Weaknesses                                  |
| --------------------- | ---------------------------------- | ------------------------------------------- |
| Lexical (grep)        | Exact matches, variable names, IDs | No understanding of intent                  |
| Semantic (embeddings) | Intent, synonyms, natural language | Struggles with exact keywords, highlighting |
| **Hybrid**            | Best of both                       | Complexity                                  |

> "Code data contains a lot of definitions, keywords, and other information where text-based search can be particularly effective. Meanwhile, dense embedding models can capture higher-level semantic information."

### Reciprocal Rank Fusion (RRF)

From [OpenSearch](https://opensearch.org/blog/introducing-reciprocal-rank-fusion-hybrid-search/) and [Azure AI Search](https://learn.microsoft.com/en-us/azure/search/hybrid-search-ranking):

RRF combines ranked results without needing comparable scores:

```
score(doc) = Σ 1/(k + rank_i)  where k=60
```

**Advantages**:

- No tuning required
- Works with incompatible score ranges
- Simple, robust, outperforms complex methods
- Prioritizes documents that rank highly across multiple sources

### CLI UX Principles

From [clig.dev](https://clig.dev/) and [Lucas F Costa](https://lucasfcosta.com/2022/06/01/ux-patterns-cli-tools.html):

1. **Explicit over implicit** - minimize hidden state
2. **Allow override** - flags for everything
3. **Progressive disclosure** - simple default, power features available
4. **Automation-friendly** - `--quiet`, `--json`, `--no-input`

## Design Principles

1. **Zero config for basic usage** - `hhg "query" .` always works (stateless)
2. **Explicit semantic opt-in** - user chooses when to use index
3. **No magic** - index lifecycle is visible and controllable
4. **Graceful degradation** - stale index warns but doesn't block
5. **Progressive enhancement** - fast → rerank → semantic → hybrid

## Architecture

### Search Modes

```
┌─────────────────────────────────────────────────────────────────┐
│                          User Query                              │
└─────────────────────────────────────────────────────────────────┘
                                │
                    ┌───────────┴───────────┐
                    ▼                       ▼
            ┌──────────────┐       ┌──────────────┐
            │  --fast      │       │  Default     │
            │  Grep only   │       │  Grep+Rerank │
            │  <100ms      │       │  ~1-2s       │
            └──────────────┘       └──────────────┘
                                          │
                                ┌─────────┴─────────┐
                                ▼                   ▼
                        ┌──────────────┐   ┌──────────────┐
                        │  --semantic  │   │  --hybrid    │
                        │  Embeddings  │   │  RRF Fusion  │
                        │  <500ms*     │   │  ~1.5s*      │
                        └──────────────┘   └──────────────┘
                                          (* requires index)
```

### Mode Selection

| Flag         | Behavior                      | Index Required | Latency |
| ------------ | ----------------------------- | -------------- | ------- |
| `--fast`     | Grep only, no reranking       | No             | <100ms  |
| (default)    | Grep + cross-encoder rerank   | No             | ~1-2s   |
| `--semantic` | Embedding similarity only     | Yes            | <500ms  |
| `--hybrid`   | RRF fusion of grep + semantic | Yes            | ~1.5s   |

### Index Lifecycle

```bash
hhg index build [path]        # Build/rebuild full index
hhg index update [path]       # Incremental update (changed files only)
hhg index status [path]       # Show index health + staleness
hhg index clear [path]        # Delete index
```

### Hybrid Search Algorithm

```python
def hybrid_search(query: str, path: Path, k: int = 10) -> list[Result]:
    # 1. Run both searches in parallel
    grep_results = grep_rerank(query, path, k=k*2)  # Over-fetch
    semantic_results = semantic_search(query, path, k=k*2)

    # 2. Apply Reciprocal Rank Fusion
    scores = defaultdict(float)
    K = 60  # Smoothing constant (standard value)

    for rank, result in enumerate(grep_results):
        scores[result.id] += 1.0 / (K + rank + 1)

    for rank, result in enumerate(semantic_results):
        scores[result.id] += 1.0 / (K + rank + 1)

    # 3. Sort by combined score, return top k
    combined = sorted(scores.items(), key=lambda x: -x[1])
    return [get_result(id) for id, score in combined[:k]]
```

### Index Freshness

```
┌─────────────────────────────────────────────────────────────────┐
│ On --semantic or --hybrid search:                               │
│                                                                 │
│ 1. Check manifest.last_update timestamp                         │
│ 2. If git repo: count files changed since last update           │
│ 3. If >50 files changed: show warning                           │
│ 4. Continue search (don't block)                                │
│ 5. Suggest: "Run `hhg index update` to refresh"                 │
└─────────────────────────────────────────────────────────────────┘
```

**Staleness detection**:

```python
def check_freshness(index_path: Path, root: Path) -> IndexStatus:
    manifest = load_manifest(index_path)

    if is_git_repo(root):
        # Fast: use git to find changed files
        changed = git_diff_files(since=manifest.last_commit)
        if len(changed) > STALE_THRESHOLD:
            return IndexStatus.STALE
        if changed:
            return IndexStatus.SLIGHTLY_STALE
    else:
        # Fallback: check newest file mtime
        newest = max(root.rglob("*"), key=lambda p: p.stat().st_mtime)
        if newest.stat().st_mtime > manifest.last_update:
            return IndexStatus.POSSIBLY_STALE

    return IndexStatus.FRESH
```

## Data Model

```
.hhg/                          # Per-project, gitignore-able
├── config.toml                # Optional user config
├── manifest.json              # Index metadata
│   ├── version: "1"
│   ├── last_update: 1701234567
│   ├── last_commit: "abc123"  # For git repos
│   ├── file_hashes: {
│   │     "src/main.py": "a1b2c3...",
│   │     ...
│   │   }
│   └── stats: {
│         files: 847,
│         blocks: 3241,
│         errors: 2
│       }
└── vectors/                   # omendb storage
    ├── *.sst
    └── *.vlog
```

## UX Flows

### First-time User

```
$ hhg "authentication" ./src
Found 15 results (grep + rerank)

src/auth/login.py:23 [function] authenticate_user
src/auth/session.py:45 [class] SessionManager
...

Tip: Run `hhg index build` for semantic search
```

### Power User with Index

```
$ hhg index build ./src
Indexing 847 files...
✓ Indexed 3,241 code blocks (42s)

$ hhg "how does user login work" ./src --semantic
src/auth/login.py:23 [function] authenticate_user (0.94)
src/auth/session.py:45 [class] SessionManager (0.89)
src/middleware/auth.py:12 [function] verify_token (0.85)
...

$ hhg "login" ./src --hybrid
# Combined grep + semantic results via RRF
```

### Stale Index Warning

```
$ hhg "query" ./src --semantic
⚠ Index is stale (127 files changed since last build)
  Run `hhg index update` to refresh

src/auth/login.py:23 [function] authenticate_user (0.91)
...
```

### Automation/CI

```bash
# Deterministic, no prompts
hhg "TODO" ./src --fast --json --quiet

# With semantic (explicit build step)
hhg index build ./src --quiet
hhg "security vulnerability" ./src --semantic --json
```

## Configuration

```toml
# pyproject.toml
[tool.hhg]
n = 10
fast = false
color = "auto"

[tool.hhg.semantic]
enabled = true
model = "all-MiniLM-L6-v2"
stale_threshold = 50  # Files changed before warning
```

## Performance Targets

| Operation        | Target         | Notes                           |
| ---------------- | -------------- | ------------------------------- |
| `--fast`         | <100ms         | Grep only                       |
| Default (rerank) | <2s            | Cross-encoder inference         |
| `--semantic`     | <500ms         | Vector similarity (indexed)     |
| `--hybrid`       | <2s            | Parallel grep + semantic + RRF  |
| `index build`    | <60s/10k files | Initial embedding generation    |
| `index update`   | <5s/100 files  | Incremental, only changed files |

## Implementation Phases

### Phase 1: Polish Experiment (Current)

- [x] Basic semantic search with omendb
- [ ] Fix score display (similarity 0-1, not distance)
- [ ] Add staleness detection
- [ ] Improve CLI help/docs

### Phase 2: Incremental Updates

- [ ] Track file hashes in manifest
- [ ] Implement `hhg index update` (delta only)
- [ ] Git integration for change detection

### Phase 3: Hybrid Mode

- [ ] Implement RRF algorithm
- [ ] Add `--hybrid` flag
- [ ] Parallel execution (grep + semantic)

### Phase 4: Polish

- [ ] Configuration file support
- [ ] Better progress indicators
- [ ] Index compression/optimization

## Key Decisions

| Decision            | Choice                     | Rationale                   |
| ------------------- | -------------------------- | --------------------------- |
| Default mode        | grep+rerank (stateless)    | Always works, no setup      |
| Semantic trigger    | Explicit `--semantic` flag | Predictable, no magic       |
| Index location      | Per-project `.hhg/`        | Simple, gitignore-able      |
| Staleness handling  | Warn but continue          | Don't block user            |
| Fusion algorithm    | RRF (k=60)                 | Simple, no tuning, proven   |
| Background indexing | Not in v1                  | Complexity not worth it yet |

## Open Questions

1. **Should `--hybrid` be the default when index exists?**
   - Leaning no: explicit is better for CLI tools

2. **Watch mode for auto-updating index?**
   - Defer to v2: adds complexity, resource usage

3. **Support for custom embedding models?**
   - Defer: all-MiniLM-L6-v2 is a good default

## References

- [Elastic: Search Relevance Tuning](https://www.elastic.co/search-labs/blog/search-relevance-tuning-in-semantic-search)
- [OpenSearch: Reciprocal Rank Fusion](https://opensearch.org/blog/introducing-reciprocal-rank-fusion-hybrid-search/)
- [Azure AI: Hybrid Search Scoring](https://learn.microsoft.com/en-us/azure/search/hybrid-search-ranking)
- [CLI Guidelines](https://clig.dev/)
- [Sourcegraph Architecture](https://sourcegraph.com/code-search)
