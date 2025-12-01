# Architectural Decisions

## 1. Language & Runtime
**Decision:** Mojo + MAX Engine
**Why:**
- **Single Binary:** Mojo compiles systems code (grep) and AI graph (rerank) into one static binary.
- **Performance:** Python is too slow for directory walking; Rust is difficult to ship with AI dependencies.
- **Native AI:** MAX Engine allows running quantized models (INT8/FP16) efficiently on CPU/GPU.

## 2. Core Architecture: "Hyper Hybrid"
**Decision:** Two-Stage Pipeline: Recall (Keyword/Regex) -> Rerank (Cross-Encoder).
**Rationale:**
- **Recall:** "Hyper Scanner" finds candidates efficiently using parallel regex matching.
    - *Note:* Can be enhanced with **Query Expansion** (LLM generates synonyms) to improve recall coverage.
- **Rerank:** "The Brain" scores candidates on-the-fly using a Cross-Encoder model.
- **UX:** Single entry point; the tool manages the complexity.
- **Simplicity:** Stateless design with no persistent index to maintain.

## 3. Interface (UX)
**Decision:** Single "Magic" Command
- **Command:** `hygrep "query"`
- **Behavior:** The tool automatically performs Recall -> Rerank.
- **Flags:** Optional flags for specific overrides.

## 4. Model Selection
**Decision:** Tiered Strategy
- **Default:** `mixedbread-ai/mxbai-rerank-xsmall-v1`.
- **Format:** ONNX (Quantized).

## 5. Protocol
**Decision:** MCP Native
**Why:** Allows AI agents to integrate `hygrep` as a structured tool for code exploration.
