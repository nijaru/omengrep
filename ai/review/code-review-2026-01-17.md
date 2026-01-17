# Code Review: v0.0.29 to v0.0.32

**Reviewer:** Claude (Opus 4.5)
**Date:** 2026-01-17
**Scope:** Changes in \_common.py, embedder.py, mlx_embedder.py, cli.py, semantic.py

## Summary

The code changes implement a well-structured embedder refactoring with MLX support. The architecture follows good patterns: Protocol-based abstraction, thread-safe lazy loading, and clean separation of concerns. No critical issues found.

## File Size Analysis

| File              | Lines | Status                                 |
| ----------------- | ----- | -------------------------------------- |
| `_common.py`      | 28    | OK                                     |
| `embedder.py`     | 327   | OK                                     |
| `mlx_embedder.py` | 234   | OK                                     |
| `cli.py`          | 1258  | Large but justified (CLI + routing)    |
| `semantic.py`     | 913   | Large but justified (index management) |

## Test Results

All tests pass. Manual smoke test verified:

- Build index: OK (795 blocks, 12.3s)
- Semantic search: OK (returns relevant results)

## Findings

### No Critical Issues (ERROR)

None found.

### Important Issues (WARN)

#### [WARN] embedder.py:135 - Type annotation too narrow

```python
_global_embedder: "Embedder | None" = None
```

The global can hold either `Embedder` or `MLXEmbedder`, but the type annotation only mentions `Embedder`.

**Recommendation:** Change to `EmbedderProtocol | None` for correctness:

```python
_global_embedder: "EmbedderProtocol | None" = None
```

**Confidence:** 90%

#### [WARN] cli.py:548-830 - Large function (283 lines)

The `search()` callback handles both search logic and subcommand routing (workaround for Typer's positional arg handling). ~140 lines are subcommand routing, ~100 lines are actual search logic.

**Assessment:** This is a known Typer limitation workaround. Splitting would require significant restructuring with unclear benefit. The code is well-commented and organized into logical sections.

**Recommendation:** Accept as-is. Document the pattern in AGENTS.md if not already noted.

**Confidence:** 85% (not a bug, but large)

### Minor Issues (NIT)

#### [NIT] cli.py:322 - Lazy import inside loop-callable function

```python
def boost_results(results: list[dict], query: str) -> list[dict]:
    import re  # imported on every call
```

**Impact:** Negligible - Python caches imports, so repeated imports are fast.

**Confidence:** 80%

#### [NIT] cli.py:361-362 - Regex inside loop

```python
for r in results:
    ...
    name_expanded = re.sub(r"([a-z])([A-Z])", r"\1 \2", name)
    name_terms = set(re.split(r"[\s_\-./]+", name_expanded))
```

**Impact:** Minimal - result sets are typically 10-30 items, regex is compiled/cached by Python.

**Confidence:** 75%

## Positive Observations

1. **Thread safety:** MLX monkey-patching uses module-level lock (`_model_load_lock`)
2. **Double-checked locking:** `_ensure_loaded()` in `MLXEmbedder` properly uses lock
3. **Protocol pattern:** `EmbedderProtocol` with `@runtime_checkable` enables duck typing
4. **Error handling:** Fragile `_tokenizer` access has runtime check with helpful error message
5. **Shared constants:** `_common.py` properly centralizes model configuration
6. **Cache eviction:** Uses dict ordering (Python 3.7+) - correct for target versions
7. **No code smells:**
   - No `_v2`, `_new`, `_old` naming
   - No bare except clauses
   - No TODOs
   - No unused imports (MODEL_VERSION is re-exported)

## Verification

```bash
# All pass
pixi run test

# Smoke tests pass
pixi run hhg build .    # 795 blocks, 12.3s
pixi run hhg "semantic search" .  # Returns relevant results

# Protocol implementation verified
MLXEmbedder implements EmbedderProtocol: OK
Embedder implements EmbedderProtocol: OK
```

## Recommendation

**Ship it.** The only actionable item is the minor type annotation fix in embedder.py:135.
