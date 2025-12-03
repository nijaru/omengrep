# hygrep

**Hyper hybrid grep: fast scanning + neural reranking**

```bash
pip install hygrep
hygrep "auth logic" ./src   # or: hhg "auth logic" ./src
```

## What it does

- **Semantic search:** "auth" finds "login", "session", "token"
- **Smart context:** Returns full functions/classes, not just lines
- **Fast:** Parallel regex recall (~20k files/sec), then neural reranking
- **Zero indexing:** Works instantly on any codebase

## Install

```bash
pip install hygrep
# or
uv tool install hygrep
# or
pipx install hygrep
```

First search downloads the model (~83MB, cached in `~/.cache/huggingface/`).

## Usage

```bash
hygrep "query" [path]           # Search (default: current dir)
hygrep "error handling" . -n 5  # Limit to 5 results
hygrep "auth" . --fast          # Skip reranking (instant grep)
hygrep "test" . -t py,js        # Filter by file type
hygrep "config" . --json        # JSON output for agents/scripts
hygrep info                     # Check installation status
hygrep model                    # Show model info
hygrep model install            # Pre-download model (for CI/offline)
hygrep model clean              # Remove cached model
```

Run `hygrep --help` for all options.

## Output

```
src/auth.py:42 [function] login (0.89)
src/session.py:15 [function] validate_token (0.76)
```

With `--json`:
```json
[{"file": "src/auth.py", "type": "function", "name": "login", "start_line": 42, "score": 0.89, "content": "def login(user): ..."}]
```

## Config

Optional `~/.config/hygrep/config.toml`:
```toml
n = 10
color = "always"
exclude = ["*.test.js", "tests/*"]
cache_dir = "~/.cache/hygrep"  # Custom model cache (default: shared HF cache)
```

## Supported Languages

Python, JavaScript, TypeScript, Go, Rust, C, C++, Java, Ruby, C#, Mojo

## How it works

```
Query → Parallel regex scan → Tree-sitter extraction → ONNX reranking → Results
```

| Stage | What |
|-------|------|
| Recall | Mojo/Python parallel scanner (~20k files/sec) |
| Extract | Tree-sitter AST (functions, classes) |
| Rerank | ONNX cross-encoder (mxbai-rerank-xsmall-v1) |

## Development

```bash
git clone https://github.com/nijaru/hygrep && cd hygrep
pixi install && pixi run build-ext && pixi run test
```

## License

MIT
