# Strategic Roadmap

**Goal:** Build `hygrep` - The high-performance Hybrid Search CLI.

## Phase 1: Prototype (Completed)
**Goal:** functional End-to-End pipeline (Scanner -> Reranker).
- [x] Basic Directory Walker (Sequential, Mojo).
- [x] Regex Matching (Python `re` wrapper).
- [x] Reranker Integration (Python `onnxruntime` wrapper).
- [x] Single-command CLI wiring.

## Phase 2: Optimization (Current Focus)
**Goal:** Replace Python components in the "Hot Loop" (Scanner) with Native Mojo/C.

### 2a. FFI Foundation (The Blocker)
- [ ] **Research:** Create minimal reproduction of `libc` regex binding using modern Mojo `UnsafePointer`.
- [ ] **Implement:** `src/scanner/c_regex.mojo` (Native `libc` binding).
- [ ] **Verify:** Ensure 0 allocations/Python calls per match.

### 2b. Parallelism
- [ ] **Research:** Mojo `parallelize` vs Work-Stealing Queue patterns.
- [ ] **Implement:** `src/scanner/walker.mojo` using parallel execution.
- [ ] **Benchmark:** Measure files/sec against `ripgrep`.

## Phase 3: Polish
**Goal:** Professional CLI Experience.
- [ ] Implement `--help` and `--version` flags.
- [ ] Implement `--limit` flag.
- [ ] Improve Error Handling (e.g. "Model not found - run download script").
- [ ] JSON Output mode for Agents.