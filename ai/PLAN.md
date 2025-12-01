# Strategic Roadmap

**Goal:** Build `hygrep` - The high-performance Hybrid Search CLI.

## Phase 1: Prototype (Completed)
**Goal:** Prove the concept (Recall -> Rerank) works end-to-end.
- [x] Basic Directory Walker (Sequential).
- [x] Regex Matching (Python `re`).
- [x] Reranker Integration (Python `onnxruntime`).
- [x] CLI Wiring.

## Phase 2: Optimization (Current Focus)
**Goal:** Achieve C++ level performance.
- [ ] **Parallel Scanner:** Implement parallel directory walking (Mojo `parallelize`).
- [ ] **Native Regex:** Bind `libc` or `pcre` to remove Python overhead per file.
- [ ] **Benchmark:** Compare against `ripgrep` (target <2x slower).

## Phase 3: Polish
**Goal:** Ship a professional tool.
- [ ] `--help` / `--version`.
- [ ] `--json` output.
- [ ] Robust error handling (Model download prompts).
