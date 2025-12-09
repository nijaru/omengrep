# hygrep (hhg)

**Hybrid file search — semantic + keyword matching**

```bash
pip install hygrep
hhg build ./src
hhg "authentication flow" ./src
```

## What it does

Search code and text using natural language. Combines semantic understanding with keyword matching (BM25) for accurate results:

```bash
$ hhg build ./src                    # Build index first
Found 40 files (0.0s)
✓ Indexed 646 blocks from 40 files (34.2s)

$ hhg "error handling" ./src         # Then search
api_handlers.ts:127 function errorHandler
  function errorHandler(err: Error, req: Request, res: Response, next: NextFunc...

errors.rs:7 class AppError
  pub enum AppError {
      Database(DatabaseError),

2 results (0.52s)
```

## Why hhg over grep?

grep finds exact text. hhg understands what you're looking for.

| Query            | grep finds                | hhg finds                     |
| ---------------- | ------------------------- | ----------------------------- |
| "error handling" | Comments mentioning it    | `errorHandler()`, `AppError`  |
| "authentication" | Strings containing "auth" | `login()`, `verify_token()`   |
| "database"       | Config files, comments    | `Connection`, `query()`, `Db` |

**Hybrid search** combines semantic understanding (finds related concepts) with BM25 keyword matching (finds exact terms). Best of both worlds.

Use grep/ripgrep for exact strings (`TODO`, `FIXME`, import statements).
Use hhg when you want implementations, not mentions.

## Install

Requires Python 3.11-3.13 (onnxruntime lacks 3.14 support).

```bash
pip install hygrep
# or
uv tool install hygrep --python 3.13
# or
pipx install hygrep
```

Models are downloaded from HuggingFace on first use (~40MB).

## Usage

```bash
hhg build [path]                # Build/update index (required first)
hhg "query" [path]              # Semantic search
hhg status [path]               # Check index status
hhg clean [path]                # Delete index

# Options
hhg -n 5 "error handling" .     # Limit results
hhg --json "auth" .             # JSON output for scripts/agents
hhg -l "config" .               # List matching files only
hhg -t py,js "api" .            # Filter by file type
hhg --exclude "tests/*" "fn" .  # Exclude patterns

# Model management
hhg model                       # Check model status
hhg model install               # Download/reinstall models
```

**Note:** Options must come before positional arguments.

## Output

Default:

```
src/auth.py:42 function login
  def login(user, password):
      """Authenticate user and create session."""
      ...
```

JSON (`--json`):

```json
[
  {
    "file": "src/auth.py",
    "type": "function",
    "name": "login",
    "line": 42,
    "end_line": 58,
    "content": "def login(user, password): ...",
    "score": 0.87
  }
]
```

Compact JSON (`--json --compact`): Same fields without `content`.

## How it Works

```
Query → Embed → Hybrid search (semantic + BM25) → Results
                        ↓
             Requires 'hhg build' first (.hhg/)
             Auto-updates stale files on search
```

## Supported Files

**Code** (22 languages): Bash, C, C++, C#, Elixir, Go, Java, JavaScript, JSON, Kotlin, Lua, Mojo, PHP, Python, Ruby, Rust, Svelte, Swift, TOML, TypeScript, YAML, Zig

**Text**: Markdown, plain text, RST — smart chunking with header context for docs, blog posts, research papers

## Development

```bash
git clone https://github.com/nijaru/hygrep && cd hygrep
pixi install && pixi run build-ext && pixi run test
```

## License

MIT
