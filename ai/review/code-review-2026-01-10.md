# Code Review: hygrep 0.0.29

**Date:** 2026-01-10
**Reviewer:** Claude
**Scope:** Exception handling, error paths, edge cases
**Test Results:** All tests pass (4 Mojo tests, 19 Python tests)

## Summary

Overall the codebase is well-structured with good exception handling in most places. Found 2 bugs and 1 minor issue.

## Critical Issues (Must Fix)

### 1. [ERROR] `list_indexes()` does not catch `IndexNeedsRebuild`

**File:** `/Users/nick/github/nijaru/hygrep/src/hygrep/cli.py`
**Line:** 1071-1073

```python
# Get block count from manifest
index = SemanticIndex(idx_root)
block_count = index.count()  # Can raise IndexNeedsRebuild
```

**Problem:** `index.count()` calls `_load_manifest()` which can raise `IndexNeedsRebuild` if the index was created with an older model version. This exception is not caught, causing an unhandled exception traceback.

**Impact:** When running `hhg list` on a directory containing old v5 indexes (jina-code model), the command crashes instead of showing a helpful message.

**Fix:**

```python
for idx_path in indexes:
    idx_root = idx_path.parent
    try:
        rel_path = idx_root.relative_to(path)
        display_path = f"./{rel_path}" if str(rel_path) != "." else "."
    except ValueError:
        display_path = str(idx_root)

    # Get block count from manifest
    index = SemanticIndex(idx_root)
    try:
        block_count = index.count()
        console.print(f"  {display_path}/.hhg/ [dim]({block_count} blocks)[/]")
    except IndexNeedsRebuild:
        console.print(f"  {display_path}/.hhg/ [yellow](needs rebuild)[/]")
```

### 2. [ERROR] `clean` command partial clean does not catch `IndexNeedsRebuild`

**File:** `/Users/nick/github/nijaru/hygrep/src/hygrep/cli.py`
**Lines:** 1110-1111

```python
index = SemanticIndex(parent)
stats = index.remove_prefix(rel_prefix)  # Can raise IndexNeedsRebuild
```

**Problem:** `remove_prefix()` calls `_load_manifest()` which can raise `IndexNeedsRebuild`. When running `hhg clean ./subdir` where the parent index is v5 format, this crashes.

**Impact:** Partial clean of subdirectories from parent index fails on old indexes.

**Fix:**

```python
try:
    index = SemanticIndex(parent)
    stats = index.remove_prefix(rel_prefix)
    if stats["blocks"] > 0:
        console.print(
            f"[green]ok[/] Removed {stats['blocks']} blocks "
            f"({stats['files']} files) from parent index"
        )
        deleted_count += 1
    else:
        err_console.print(
            f"[dim]No blocks found for {rel_prefix} in parent index[/]"
        )
except IndexNeedsRebuild:
    err_console.print(
        f"[yellow]![/] Parent index needs rebuild. Run: hhg build --force {parent}"
    )
except ValueError:
    pass  # path not under parent, shouldn't happen
```

## Important Issues (Should Fix)

### 3. [WARN] Missing `from .semantic import IndexNeedsRebuild` in `clean` command

**File:** `/Users/nick/github/nijaru/hygrep/src/hygrep/cli.py`
**Line:** 1087

The `clean` command imports `SemanticIndex, find_parent_index, find_subdir_indexes` but does not import `IndexNeedsRebuild`. This is needed for the fix above.

**Current:**

```python
from .semantic import SemanticIndex, find_parent_index, find_subdir_indexes
```

**Fix:**

```python
from .semantic import IndexNeedsRebuild, SemanticIndex, find_parent_index, find_subdir_indexes
```

## Verified Working

The following exception handling patterns were verified as correct:

| Location                         | Exception           | Handling                      |
| -------------------------------- | ------------------- | ----------------------------- |
| `status` command (line 873)      | `IndexNeedsRebuild` | Caught, shows rebuild message |
| `build` command (line 949)       | `IndexNeedsRebuild` | Caught, triggers rebuild      |
| `search` callback (line 799)     | `IndexNeedsRebuild` | Caught, prompts user          |
| `_run_similar_search` (line 513) | `IndexNeedsRebuild` | Caught, prompts user          |
| `build_index` (line 214)         | `RuntimeError`      | Caught, shows error           |
| Model loading errors             | `RuntimeError`      | Caught at multiple levels     |

## Edge Cases Verified

1. **Empty query**: Handled by showing help (line 742)
2. **Non-existent path**: Handled with error message (line 766-767)
3. **File reference parsing**: Proper fallback when file does not exist (line 93-94)
4. **Keyboard interrupt during build**: Graceful exit with message (line 210-213)
5. **Permission errors**: Caught and reported (line 218-221)
6. **Disk space errors**: Caught and reported (line 222-227)

## Test Coverage

Current tests cover:

- Exit codes (0, 1, 2)
- JSON output format
- Exclude patterns
- Type filtering
- Files-only mode
- Compact output
- Subcommands (build, status, clean, list, model)
- Recursive clean
- Semantic search roundtrip
- Incremental updates
- Stale detection
- Scope filtering

**Gap:** No test for `IndexNeedsRebuild` scenario. Consider adding a test that:

1. Creates a v5 manifest manually
2. Verifies commands handle it gracefully

## Recommendations

1. Fix the two bugs above
2. Add test for v5 index handling
3. Consider adding `--force` to list command to show blocks even for old indexes
