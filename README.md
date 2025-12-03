# hygrep

> **Hyper + Hybrid grep: fast scanning + neural reranking**

`hygrep` combines the instant performance of parallel regex scanning (Mojo) with semantic understanding via neural reranking (ONNX).

**Zero indexing. Local inference. ~20k files/sec.**

## Features

- **Smart Context:** Extracts full functions/classes (Python, JS, TS, Go, Rust, Mojo)
- **Semantic Reranking:** "auth" matches "login"
- **Agent Ready:** JSON output with `--json`
- **Fast:** Parallel Mojo scanner, ~20k files/sec
- **Robust:** Ignores node_modules, binaries, hidden files, .gitignore
- **Color Output:** Colored paths, types, and scores (respects NO_COLOR)

## Installation

### From Source (with Pixi)

```bash
git clone https://github.com/nijaru/hygrep
cd hygrep
pixi install
pixi run build-ext   # Build Mojo scanner extension
pixi run hygrep "query" ./src
```

### From PyPI (coming soon)

```bash
pip install hygrep
# or
uv tool install hygrep
```

## Usage

```bash
# Basic search
hygrep "login logic" ./src

# Limit results
hygrep "auth" . -n 5

# Fast mode (skip neural reranking, instant grep)
hygrep "error" ./src --fast

# Filter by file type
hygrep "function" . -t py,js

# JSON output for agents
hygrep "error handling" ./src --json

# Quiet mode (no progress)
hygrep "test" . -q

# Limit candidates for faster reranking
hygrep "query" ./large-codebase --max-candidates 50

# Show code context (5 lines)
hygrep "auth" . -C 5

# Include gitignored files
hygrep "config" . --no-ignore

# Disable color output
hygrep "error" . --color=never

# Show timing statistics
hygrep "test" . --stats

# Filter by minimum score
hygrep "auth" . --min-score 0.5

# Exclude patterns
hygrep "test" . --exclude "*.test.js" --exclude "tests/*"

# Generate shell completions
hygrep --completions bash >> ~/.bashrc
hygrep --completions zsh >> ~/.zshrc
hygrep --completions fish > ~/.config/fish/completions/hygrep.fish

# Include hidden files
hygrep "TODO" . --hidden

# Check installation status
hygrep info
```

### Config File

Create `~/.config/hygrep/config.toml` for persistent defaults:

```toml
# Default number of results
n = 10

# Always use color
color = "always"

# Default exclude patterns
exclude = ["*.test.js", "node_modules/*"]

# Other options
# fast = true
# quiet = true
# hidden = true
# no_ignore = true
# max_candidates = 50
# min_score = 0.3
```

### Output

```
src/auth.py:42 [function] login (0.89)
src/session.py:15 [function] validate_token (0.76)
```

### JSON Output

```json
[
  {
    "file": "src/auth.py",
    "type": "function",
    "name": "login",
    "start_line": 42,
    "score": 0.89,
    "content": "def login(user): ..."
  }
]
```

## Architecture

```
Query → [Mojo Scanner] → candidates → [ONNX Reranker] → results
              ↓                              ↓
        Parallel regex              Tree-sitter extraction
        ~20k files/sec              Cross-encoder scoring
```

| Component | Implementation |
|-----------|----------------|
| Scanner | Mojo Python extension (`_scanner.so`) |
| Extraction | Tree-sitter (Python, JS, TS, Go, Rust, Mojo) |
| Reranking | ONNX Runtime (`mxbai-rerank-xsmall-v1`) + GPU auto-detect |

## Development

```bash
pixi run build-ext   # Build Mojo extension
pixi run hygrep      # Run CLI
pixi run test        # Run tests
```

## License

[MIT](LICENSE)
