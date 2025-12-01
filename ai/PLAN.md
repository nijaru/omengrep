# Strategic Roadmap

**Goal:** Build `hgrep` - the standard "Agent-Native" search tool.

## Phase 1: The "MojoGrep" (Week 1-2)
**Goal:** Recreate `ripgrep` functionality in Mojo.
- [ ] Implement parallel directory walker (Mojo `List[Path]`)
- [ ] Bind C regex library (`libc`/`pcre2`)
- [ ] **Milestone:** `hygrep pattern ./src` matches `grep` speed (<50ms).

## Phase 2: The Brain (Week 3-4)
**Goal:** Integrate MAX Engine.
- [ ] Convert `Qwen3-Reranker` to ONNX
- [ ] Implement model loading in Mojo
- [ ] Implement `rerank(query, candidates)`
- [ ] **Milestone:** `hygrep "login bug" ./src` (Default Mode) returns semantic matches.

## Phase 3: The Agent Interface (Week 5)
**Goal:** JSON & Token Counting.
- [ ] Implement fast BPE Tokenizer in Mojo
- [ ] Add `--json` flag
- [ ] Implement basic MCP Server loop (stdio)
- [ ] **Milestone:** Connect `hygrep` to an MCP Client.