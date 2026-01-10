# BGE-base-en-v1.5 vs jina-embeddings-v2-base-code

Comparison for code search/embedding use cases.

## Summary

| Aspect             | BGE-base-en-v1.5               | jina-embeddings-v2-base-code |
| ------------------ | ------------------------------ | ---------------------------- |
| **Purpose**        | General English text embedding | Code + English embedding     |
| **Parameters**     | 109M                           | 161M                         |
| **Dimensions**     | 768                            | 768                          |
| **Context Length** | 512 tokens                     | 8192 tokens                  |
| **Architecture**   | BERT (standard)                | JinaBERT (ALiBi)             |
| **License**        | MIT                            | Apache 2.0                   |
| **Code Training**  | None (general text only)       | 150M+ code pairs             |

**Recommendation for code search:** jina-embeddings-v2-base-code is the clear winner for code-specific use cases due to code-specific training, 16x longer context, and strong CodeSearchNet performance.

## 1. MTEB Benchmark Scores

### BGE-base-en-v1.5 (General MTEB)

| Task                    | Score |
| ----------------------- | ----- |
| **Average (56 tasks)**  | 63.55 |
| Retrieval (15)          | 53.25 |
| Clustering (11)         | 45.77 |
| Pair Classification (3) | 86.55 |
| Reranking (4)           | 58.86 |
| STS (10)                | 82.40 |
| Summarization (1)       | 31.07 |
| Classification (12)     | 75.53 |

Source: [BAAI/bge-base-en-v1.5 HuggingFace](https://huggingface.co/BAAI/bge-base-en-v1.5)

### jina-embeddings-v2-base-code (General MTEB)

| Task                | Score              |
| ------------------- | ------------------ |
| **Average**         | ~60-62 (estimated) |
| Retrieval (nDCG@10) | 47.87              |
| Classification      | 73.36              |
| STS                 | 80.70              |
| Reranking           | 56.98              |

Note: Jina v2 code model optimizes for code, not general text. General MTEB scores are slightly lower than BGE, but this is expected given the code-focused training objective.

Source: [arXiv:2310.19923](https://arxiv.org/abs/2310.19923)

### Code-Specific Benchmarks

**No direct code-related tasks in standard MTEB.** However, Jina claims leadership on 9/15 CodeSearchNet benchmarks.

## 2. Training Data

### BGE-base-en-v1.5

- **Pre-training:** RetroMAE (masked language modeling with reconstruction)
- **Fine-tuning:** C-MTP (Massive Text Pair) dataset
  - Labeled pairs from public datasets (NLI, QA, retrieval)
  - Unlabeled pairs mined from web corpora
  - English dataset ~2x larger than Chinese counterpart
- **Training Method:** Contrastive learning with hard negatives
- **No code-specific data**

Source: [C-Pack: arXiv:2309.07597](https://arxiv.org/abs/2309.07597)

### jina-embeddings-v2-base-code

- **Backbone Pre-training:** JinaBERT on [github-code dataset](https://huggingface.co/datasets/codeparrot/github-code)
- **Fine-tuning Data:**
  - 150M+ coding Q&A pairs
  - Docstring-source code pairs
  - Multi-domain, carefully cleaned
- **Languages:** English + 30 programming languages
- **Training Method:** Contrastive learning on text pairs, then hard negatives

Source: [HuggingFace model card](https://huggingface.co/jinaai/jina-embeddings-v2-base-code)

## 3. Architecture Differences

### BGE-base-en-v1.5

- **Base:** Standard BERT architecture
- **Positional Encoding:** Absolute positional embeddings (learned)
- **Context Limit:** 512 tokens (hard limit from position embeddings)
- **Pooling:** CLS token with normalization
- **Query Instruction:** Recommended prefix for short queries

### jina-embeddings-v2-base-code

- **Base:** JinaBERT (modified BERT)
- **Positional Encoding:** Bidirectional ALiBi (Attention with Linear Biases)
  - No learned position embeddings
  - Enables extrapolation beyond training length
- **Context Limit:** 8192 tokens (extrapolates from 512 training length)
- **Pooling:** Mean pooling
- **No query prefix needed**

**Key Difference:** ALiBi enables 16x longer context without retraining, critical for code files.

## 4. Context Length Support

| Model             | Training Length | Inference Length | Notes                       |
| ----------------- | --------------- | ---------------- | --------------------------- |
| BGE-base-en-v1.5  | 512             | 512 (hard limit) | Cannot process longer input |
| jina-v2-base-code | 512             | 8192+            | ALiBi extrapolation         |

**Impact for Code Search:**

- Average function: 50-200 tokens
- Average file: 500-2000 tokens
- Large files: 5000+ tokens

BGE truncates most files; Jina can embed entire functions/classes.

## 5. Code-Specific Benchmarks

### CodeSearchNet Performance

jina-embeddings-v2-base-code claims **9/15 top positions** on CodeSearchNet benchmarks, outperforming Microsoft CodeBERT and Salesforce CodeGen.

| Language   | Jina Code v2 | Notes                  |
| ---------- | ------------ | ---------------------- |
| Python     | Strong       | Core training language |
| JavaScript | Strong       | Core training language |
| Java       | Strong       | Core training language |
| Go         | Strong       | Core training language |
| Ruby       | Strong       | Core training language |
| PHP        | Strong       | Core training language |

Source: [Jina AI announcement](https://jina.ai/news/elevate-your-code-search-with-new-jina-code-embeddings/)

### BGE on Code Tasks

BGE-base-en-v1.5 has **no code-specific training or benchmarks**. When tested on code:

- Treats code as natural language
- Loses syntactic/structural understanding
- Context truncation loses function bodies

## 6. Community Usage for Code Search

### jina-embeddings-v2-base-code

- **Ollama:** 82.9K downloads (unclemusclez/jina-embeddings-v2-base-code)
- **HuggingFace:** 74K+ monthly downloads
- **AWS SageMaker:** Official marketplace listing
- **Azure:** Available via Jina AI
- **Use Cases:**
  - Code search tools (Cursor, Windsurf-style)
  - Documentation assistants
  - RAG for codebases
  - Code review automation

### BGE-base-en-v1.5

- **HuggingFace:** 2.8M+ monthly downloads (but general purpose)
- **Code-specific usage:** Minimal
- **Common Use:** RAG, semantic search, document retrieval
- **Not recommended for code by practitioners**

### Community Recommendations

Modal's [6 Best Code Embedding Models](https://modal.com/blog/6-best-code-embedding-models-compared) lists:

1. VoyageCode3
2. OpenAI text-embedding-3-large
3. **Jina Code Embeddings V2** (recommended for open-source)
4. Nomic Embed Code
5. CodeSage Large V2
6. CodeRankEmbed

BGE is **not listed** as a code embedding option.

## 7. Direct Comparisons

### No Published Direct Comparison

No academic papers or official benchmarks directly compare BGE-base-en-v1.5 vs jina-embeddings-v2-base-code.

### Indirect Evidence

| Factor                 | Winner                      |
| ---------------------- | --------------------------- |
| Code understanding     | Jina (trained on code)      |
| Long code files        | Jina (8K vs 512 context)    |
| General text retrieval | BGE (MTEB optimized)        |
| Docstring-to-code      | Jina (trained on pairs)     |
| Model size             | BGE (smaller: 109M vs 161M) |
| Inference speed        | BGE (shorter context)       |

### LoCo (Long Context) Benchmark

From Jina paper (Table 11):

| Model                                 | Context | avg. nDCG@10        |
| ------------------------------------- | ------- | ------------------- |
| jina-base-v2                          | 8192    | 85.4                |
| bge-base-en-v1.5                      | 512     | 73.4 (no fine-tune) |
| bge-base-en-v1.5 (fine-tuned on LoCo) | 512     | 83.0                |

Jina outperforms even fine-tuned BGE on long-context retrieval.

## Newer Alternatives (2025+)

Both models have successors worth considering:

### BGE Family

- **BGE-M3:** Multi-functionality (dense + sparse + multi-vector), 8K context
- **BGE-en-icl:** Instruction-tuned for English

### Jina Family

- **jina-code-embeddings-0.5b/1.5b (Sept 2025):** New SOTA
  - 78-79% on 25 code retrieval benchmarks
  - Built on Qwen2.5-Coder backbone
  - 32K context, Matryoshka dimensions

## Recommendations

### Use jina-embeddings-v2-base-code when:

- Building code search tools
- Indexing codebases for RAG
- Need docstring-to-code matching
- Processing long code files
- Supporting multiple programming languages

### Use BGE-base-en-v1.5 when:

- General text retrieval (documents, articles)
- Shorter text inputs (<512 tokens)
- Inference speed is critical
- No code understanding needed
- Memory constrained environments

### For hhg (this project):

Current choice (jina-embeddings-v2-base-code) is correct:

- Code search is the primary use case
- Tree-sitter extracts code blocks (variable length)
- 8K context handles large functions/classes
- Code+English mixed queries supported

---

## Sources

- [BAAI/bge-base-en-v1.5](https://huggingface.co/BAAI/bge-base-en-v1.5)
- [jinaai/jina-embeddings-v2-base-code](https://huggingface.co/jinaai/jina-embeddings-v2-base-code)
- [C-Pack Paper (BGE training)](https://arxiv.org/abs/2309.07597)
- [Jina Embeddings 2 Paper](https://arxiv.org/abs/2310.19923)
- [Modal: 6 Best Code Embedding Models](https://modal.com/blog/6-best-code-embedding-models-compared)
- [MTEB Leaderboard](https://huggingface.co/spaces/mteb/leaderboard)
- [Jina Code Embeddings Announcement](https://jina.ai/news/jina-code-embeddings-sota-code-retrieval-at-0-5b-and-1-5b/)

---

_Research date: 2026-01-10_
