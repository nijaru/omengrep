# Optimization Sprint Review (2026-02-23)

Scope: 10 Rust files, ~590 lines. Commits d999b97..fe2dc38.
Build: clean. Tests: 26/26 pass. Clippy: 1 pre-existing warning (type_complexity).

## Correctness Issues

### [ERROR] src/extractor/queries.rs:40 + src/extractor/mod.rs:115 -- Python decorated_definition captured then discarded

The Python query now captures both `(function_definition) @function` and `(decorated_definition) @function`. A decorated function produces two overlapping blocks:

- `decorated_definition`: starts at decorator line, ends at function end
- `function_definition`: starts at `def` line, ends at function end

The `remove_nested_blocks()` function (mod.rs:124) sees the `decorated_definition` as a parent containing the `function_definition` child, and removes the parent. Result: the decorator lines are lost -- exactly what the query addition was meant to capture.

Fix: Either (a) remove `(decorated_definition)` from the Python query since `remove_nested_blocks` makes it pointless, or (b) change the Python query to only capture `(decorated_definition)` when present and `(function_definition)` when NOT decorated, or (c) make `remove_nested_blocks` prefer the larger block when both share the same end line. Option (b) is cleanest:

```
(function_definition) @function
(class_definition) @class
(decorated_definition
  definition: (function_definition)) @function
(decorated_definition
  definition: (class_definition)) @class
```

Then in `remove_nested_blocks`, the inner `function_definition` and the outer `decorated_definition` overlap. The decorated_definition (parent) would be removed. That still doesn't work.

The real fix is option (c): change `remove_nested_blocks` to keep the LARGER block when parent and child share the same end_line, or option (a): just remove `(decorated_definition)` from the query and accept that decorators are excluded from the block content. Since `remove_nested_blocks` is a general mechanism, the simplest correct fix is to only emit the `decorated_definition` and let tree-sitter NOT emit the inner `function_definition`:

```python
"python" => {
    r#"
    (function_definition) @function
    (class_definition) @class
    (decorated_definition) @function
    "#
}
```

...and then in `remove_nested_blocks`, prefer the parent (larger span) over children when the parent fully covers all its children. Currently it always drops the parent. The algorithm should drop the parent ONLY when children collectively cover its content (e.g., a class with methods). For a decorated function, the decorator lines are NOT covered by the child `function_definition`.

Cleanest fix: change `remove_nested_blocks` to only drop parents whose children collectively cover the parent's line range (within some tolerance), OR check if parent content minus children content is trivially small (like just a decorator or class signature).

### [WARN] src/index/mod.rs:527 -- Double manifest load in check_and_update

`check_and_update()` loads the manifest on line 464, then loads it again on line 527 inside the delete block. The second load is unnecessary since nothing writes to disk between the two loads.

```rust
// Line 464
let manifest = Manifest::load(&self.index_dir)?;
// ... mtime checks using manifest ...
// Line 527
let mut manifest = Manifest::load(&self.index_dir)?; // redundant
```

Fix: pass the existing `manifest` into the delete block as mutable, or restructure to avoid the second load.

### [WARN] src/index/mod.rs:460-547 -- check_and_update duplicates logic from get_stale_files_fast

The mtime pre-check logic in `check_and_update()` (lines 466-485) is a copy-paste of `get_stale_files_fast()` (lines 435-455). Both iterate metadata, check mtime against manifest, and collect changed/deleted paths.

Fix: call `get_stale_files_fast` from `check_and_update`, or extract the shared logic. This removes ~20 lines of duplication and prevents the two from diverging.

### [WARN] src/tokenize.rs:47-140 -- KEYWORD_STOP_LIST has mixed case entries

The stop list contains `"None"`, `"True"`, `"False"` (Python builtins, PascalCase). The whole-word filter (`KEYWORD_STOP_LIST.contains(&word)`) is case-sensitive. After `split_word`, parts are lowercased. So:

- `None` as a 4-char identifier hits the whole-word filter (correct)
- But if `None` appears as a split part from e.g. `isNone`, `split_word` returns `["is", "none"]`, and `"none"` does NOT match `"None"` in the stop list

Fix: either lowercase the stop list entries (`"none"`, `"true"`, `"false"`) or compare case-insensitively. Since `split_word` always lowercases, the stop list should use lowercase.

## Quality Issues

### [WARN] benches/omendb.rs:42 -- Clippy warning: `&PathBuf` should be `&Path`

```rust
// Before:
fn make_store(dir: &PathBuf) -> VectorStore {
// After:
fn make_store(dir: &Path) -> VectorStore {
```

### [NIT] src/index/mod.rs:208-222 -- mtime recorded after embedding, not at scan time

In `index()`, mtime is read from disk AFTER all blocks have been extracted and embedded (line 211). If the file is modified during the build, the recorded mtime will be newer than the content that was actually indexed. On next search, the mtime check would say "up to date" even though the indexed content is stale.

The mtime should ideally be captured at scan time (before content is read) or at the same time as content. The current `to_process` tuple doesn't include mtime, but `scan()` doesn't collect it either.

This is a minor race window -- files rarely change during a build -- but worth noting for correctness.

### [NIT] src/extractor/mod.rs:124-166 -- remove_nested_blocks is O(n^2)

The algorithm checks all pairs (i, j) where j > i. For files with many blocks (e.g., a large Java file with 100+ methods in a class), this is O(n^2). Since blocks are sorted by start_line, the inner loop could break early when `blocks[j].start_line > blocks[i].end_line`.

```rust
// Before (inner loop):
for j in (i + 1)..blocks.len() {
    // ...check containment...
}

// After:
for j in (i + 1)..blocks.len() {
    if blocks[j].start_line > blocks[i].end_line {
        break; // sorted by start_line, no more children possible
    }
    // ...check containment...
}
```

## Summary

| Severity | Count | Description                                                      |
| -------- | ----- | ---------------------------------------------------------------- |
| ERROR    | 1     | decorated_definition captured then removed by nested dedup       |
| WARN     | 4     | Double manifest load, duplicated logic, case-sensitivity, clippy |
| NIT      | 2     | mtime race window, O(n^2) nested dedup                           |
