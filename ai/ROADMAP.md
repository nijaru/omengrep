# Strategic Roadmap

**Goal:** Build `hygrep` - The high-performance Hybrid Search CLI.

## Completed Phases

### Phase 1-4: MVP (Completed)
- [x] Directory walker with parallel scanning (~20k files/sec)
- [x] POSIX regex via libc FFI
- [x] ONNX cross-encoder reranking (mxbai-rerank-xsmall-v1)
- [x] Tree-sitter extraction (Python, JS, TS, Go, Rust)
- [x] JSON output for agents
- [x] Auto model download on first run

### Phase 5: Distribution (Completed)
- [x] Mojo Python extension module (`_scanner.so`)
- [x] Python CLI entry point (`pip install hygrep`)
- [x] Platform-specific wheel tags
- [x] Removed legacy Mojo CLI

### Phase 6: Performance (Completed)
- [x] Thread optimization (4 threads, 2.5x speedup)
- [x] `--fast` mode (skip reranking, 10x faster)
- [x] `-t/--type` filter (file type filtering)
- [x] `--max-candidates` (cap inference work)
- [x] Graph optimization level (ORT_ENABLE_ALL)

## Current: Phase 7 - CLI Polish (v0.3.0)

**Goal:** Feature parity with modern CLI tools (ripgrep, fd, bat)

### P2: Essential Polish
| Feature | Description | Beads |
|---------|-------------|-------|
| Color output | Colored paths, types, scores | hgrep-zxs |
| Gitignore support | Parse .gitignore files | hgrep-qof |
| Exit codes | 0=match, 1=none, 2=error | hgrep-bu6 |
| Context lines | `-C/-A/-B` surrounding code | hgrep-rj4 |

### P3: Quality of Life
| Feature | Description | Beads |
|---------|-------------|-------|
| Stats flag | `--stats` timing breakdown | hgrep-5jf |
| Min score | `--min-score` threshold | hgrep-97l |
| Shell completions | bash, zsh, fish | hgrep-04d |
| Exclude patterns | `--exclude`, `--glob` | hgrep-3r9 |

### P4: Nice to Have
| Feature | Description | Beads |
|---------|-------------|-------|
| Config file | `~/.config/hygrep/config.toml` | hgrep-1dg |
| Hidden files | `--hidden` flag | hgrep-0gz |

## Phase 8: Distribution (v0.4.0)

**Goal:** Easy installation via PyPI

| Task | Description | Beads |
|------|-------------|-------|
| GitHub Actions | Build wheels (macOS-arm64, linux-x64) | hgrep-4n4 |
| PyPI publish | `pip install hygrep` | hgrep-4n4 |

## Phase 9: Hardware Acceleration (v0.5.0+)

**Goal:** Leverage GPU/NPU for inference

### macOS (Apple Silicon)
- CoreML requires custom onnxruntime build
- Alternative: MLX framework
- Expected: 3-5x speedup

### Linux/Windows
- CUDA via `onnxruntime-gpu`
- ROCm for AMD GPUs
- Expected: 5-10x (overhead for small batches)

### Model Options
| Model | Quality | Speed | Size |
|-------|---------|-------|------|
| mxbai-rerank-xsmall-v1 | Good | Fast | 40MB | **Current** |
| mxbai-rerank-base-v2 | Better | 2x slower | 110MB |
| jina-reranker-v1-tiny-en | OK | Fastest | 33MB |

## Non-Goals

- Indexing/persistence (stay stateless)
- Background daemon (keep CLI simple)
- Custom model training (use pretrained)
- Server mode (CLI-first design)
