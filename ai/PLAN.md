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

### 2a. FFI Foundation (Completed)
- [x] **Research:** Proven "Int-cast" pattern for FFI in v0.25.7.
- [x] **Implement:** `src/scanner/c_regex.mojo` (Native `libc` binding).
- [x] **Verify:** Verified regex logic via tests.

### 2b. Parallelism (Completed)
- [x] **Implement:** `src/scanner/walker.mojo` using `algorithm.parallelize` + `UnsafePointer` Mask.
- [x] **Verify:** Verified finding matches across directory structure.

### 2c. Benchmarking & Tuning (Next)
- [ ] **Benchmark:** Measure files/sec against `ripgrep` (need large dataset).
- [ ] **Tune:** Optimize chunk size or batching if overhead is high.

## Phase 3: Polish
**Goal:** Professional CLI Experience.
- [ ] Implement `--help` and `--version` flags.
- [ ] Implement `--limit` flag.
- [ ] Improve Error Handling (e.g. "Model not found - run download script").
- [ ] JSON Output mode for Agents.
