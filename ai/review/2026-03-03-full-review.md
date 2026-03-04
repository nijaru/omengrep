# Code Review: omengrep (2026-03-03)

**Reviewer:** claude-sonnet-4-6 (fresh eyes)
**Scope:** All `src/` files
**State:** Build clean, 14/14 integration tests pass, 12/12 unit tests pass, no clippy warnings

---

## Correctness/Safety Issues

### [ERROR] src/index/mod.rs:292,374 — Scope filter uses raw prefix match, leaks sibling directories

`file.starts_with(scope.as_str())` without a `/` guard causes false positives.

When user runs `og "query" ./src/cli`, scope becomes `"src/cli"`. A file at
`src/cli_utils/helper.rs` passes the filter because `"src/cli_utils/helper.rs".starts_with("src/cli")` is `true`. Results from unintended directories are included.

**Reproduced with test:**

```
mkdir -p /tmp/test/src/cli_utils && mkdir -p /tmp/test/src/cli
# After indexing and searching ./src/cli:
# src/cli_utils/helper.rs appears in results
```

**Fix** (same pattern used correctly at line 624 in `remove_prefix`):

```rust
// Lines 292 and 374 — replace:
if !file.starts_with(scope.as_str()) {

// With:
if file != scope.as_str() && !file.starts_with(&format!("{scope}/")) {
```

### [WARN] src/extractor/text.rs:323 — Markdown multi-chunk sections: all but last chunk lost

`Block::make_id(file_path, section.start_line, &name)` generates the same ID for every
chunk produced from a single markdown section (same `start_line`, same header `name`).
When stored in omendb, duplicate IDs overwrite — only the last chunk survives.

A section longer than ~1600 characters (CHUNK_SIZE=400 tokens, estimate_tokens = len/4) is
split into multiple chunks via `split_text_recursive` + `add_overlap`. All chunks get the
same ID:

```
chunk 1: id = "README.md:10:Installation"  -> stored
chunk 2: id = "README.md:10:Installation"  -> overwrites chunk 1 (lost)
chunk 3: id = "README.md:10:Installation"  -> overwrites chunk 2 (lost)
```

Only chunk 3 is indexed. Long documentation sections lose their first portions.

The `extract_plain_text_blocks` function (line 355) is correct — it increments `line_num`
per chunk giving each a unique ID.

**Fix:** add a chunk index to the ID:

```rust
// In extract_markdown_blocks, for loop over chunks:
for (chunk_idx, chunk) in chunks.iter().enumerate() {
    // ...
    blocks.push(Block {
        id: Block::make_id(file_path, section.start_line + chunk_idx, &name),
        // ... rest unchanged
    });
}
```

### [WARN] src/cli/output.rs:73 — Negative score displayed as nonsensical negative percentage

`show_score=true` is passed only for similar-search results (run_similar_search, line 225).
The display computes `((r.score * 100.0) as i32)` without clamping or absolute value.

For negative MaxSim scores (which are the normal case for this model), the output is:

```
index/mod.rs:309 function find_similar (-40099% similar)
```

This is confusing and meaningless to users. The score is an internal distance metric,
not a percentage.

**Options:**

- Remove the percentage display, show raw score: `(score: {:.3})`
- Normalize to [0,100] range based on known score bounds
- Use `r.score.abs()` and note it's a distance (less = more similar)

---

## Quality/Refactoring Issues

### [NIT] src/cli/status.rs:20, src/cli/clean.rs:39 — Dead condition: `"different model"` never produced

The error message `"different model"` is checked in both files but never produced by any
`bail!()` or `anyhow!()` call in the codebase. All format-change errors from `manifest.rs`
say `"older version"`.

If omendb produces a "different model" error, this is correct to keep. If not, remove the
dead condition. Low priority.

### [NIT] src/cli/build.rs:83-85 — Double SemanticIndex construction on format change

When rebuilding after an older-version error:

```rust
let idx = SemanticIndex::new(&build_path, None)?;  // line 83
idx.clear()?;
build_index(&build_path, quiet)?;  // build_index creates another SemanticIndex internally
```

`build_index()` itself calls `SemanticIndex::new()` (line 131), so the embedder and model
are loaded twice. The `idx` at line 83 could be replaced with just:

```rust
std::fs::remove_dir_all(&build_path.join(crate::index::INDEX_DIR))?;
build_index(&build_path, quiet)?;
```

### [NIT] src/cli/search.rs:48 — Unnecessary clone

```rust
let search_path = path.clone();  // Before: path: PathBuf
```

`path` is not moved before `search_path` is used; could borrow instead:

```rust
let search_path = &path;
```

(Minor, may require lifetime adjustments with `index_root`.)

---

## Analysis Notes (No Action Required)

**Boost logic for negative scores (recently fixed):** Correct. `score /= boost` for
negative scores moves them toward zero (more similar). `remove_prefix` uses `"{prefix}/"` guard
(line 624) correctly but scope filter at lines 292/374 does not — inconsistency.

**Threshold filter for negative scores (line 123):** Correct. Default 0.0 disables the
filter (`threshold != 0.0` guard). For negative scores, user sets e.g. `--min-score -5.0`
meaning "keep results better than -5.0" — `score >= -5.0` is correct since -4.0 > -5.0
(less negative = more similar).

**Score sort order:** Correct. `b.score.partial_cmp(&a.score)` (descending) correctly
orders: -0.5 > -1.0 > -4.0 (less negative first = most similar first).

**Merge logic in search():** Correct. `r.distance > e.get().distance` keeps better
(less negative) score per ID across BM25 and semantic results.

**find_similar() early break:** Acceptable. omendb returns results sorted by distance, so
breaking at k is correct. search_k = k\*3 + blocks.len() provides overfetch for scope filtering.

**SCOPE_OVERFETCH=5:** Heuristic. May be insufficient for very narrow search scopes within
large indexes, but this is a known limitation.

**looks_like_code_query heuristic:** Known limitation. "what_is_this" triggers code query
path due to underscore, even as natural language. Acceptable tradeoff.

**Race condition in walker.scan():** mtime captured before file read. If file modified
between stat and read: stored mtime is old, next scan detects stale → re-indexed.
Self-correcting. Acceptable.

**OnnxEmbedder Mutex:** Correct for safety. Embedding is single-threaded in practice
(par_iter only covers extraction), so no contention.

**Markdown code block ID collisions:** Not possible in practice — code blocks cannot
share the same start_line (sequential sections).
