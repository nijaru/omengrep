# hygrep (hhg)

**Hybrid file search — semantic + keyword matching**

How it works:

1. **Index**: ModernBERT embeddings + text for hybrid search (requires `hhg build` first, or set `HHG_AUTO_BUILD=1`)
2. **Search**: Hybrid (semantic + BM25) via omendb (auto-updates stale files)

## Quick Reference

```bash
pixi run build-ext            # Build Mojo scanner extension
pixi run hhg build ./src      # Build index (required first)
pixi run hhg "query" ./src    # Semantic search (text query)
pixi run hhg file.py#func     # Find similar code (by name)
pixi run hhg file.py:42       # Find similar code (by line)
pixi run test                 # Run all tests
```

## Architecture

```
Hybrid Search:
Query → Embed → Hybrid search (semantic + BM25) → Results
                        ↓
             Requires 'hhg build' first (.hhg/)
             Auto-updates stale files on search

Index hierarchy:
- Building: checks parent (refuses if exists), merges subdirs (fast vector copy)
- Searching: walks up to find index, filters results to search scope
```

| Component  | Implementation                                                  |
| ---------- | --------------------------------------------------------------- |
| Scanner    | `src/scanner/_scanner.mojo` (Python extension) + `c_regex.mojo` |
| Extraction | `src/hygrep/extractor.py` (Tree-sitter AST)                     |
| Embeddings | `src/hygrep/embedder.py` (jina-code-int8 ONNX)                  |
| Vector DB  | `src/hygrep/semantic.py` (omendb wrapper)                       |

## Project Structure

```
src/
├── scanner/
│   ├── _scanner.mojo   # Python extension module (scan function)
│   └── c_regex.mojo    # POSIX regex FFI (libc)
├── hygrep/
│   ├── __init__.py     # Package version
│   ├── cli.py          # CLI entry point
│   ├── embedder.py     # jina-code-int8 ONNX embeddings
│   ├── semantic.py     # SemanticIndex (omendb wrapper)
│   ├── extractor.py    # Tree-sitter extraction
│   └── _scanner.so     # Built extension (gitignored)
tests/                  # Mojo + Python tests
pyproject.toml          # Python packaging
hatch_build.py          # Platform wheel hook
```

## Technology Stack

| Component    | Version               | Notes                      |
| ------------ | --------------------- | -------------------------- |
| Mojo         | 25.7.\*               | Via MAX package            |
| Python       | >=3.11, <3.14         | CLI + inference            |
| ONNX Runtime | >=1.16                | Model execution            |
| Tree-sitter  | >=0.24                | AST parsing (22 languages) |
| omendb       | >=0.0.14              | Hybrid vector + BM25 DB    |
| Embeddings   | jina-code-int8        | INT8, 768 dims, ~154MB     |

## Mojo Patterns

### Python Extension Modules

Build Mojo as native Python extension (no subprocess overhead):

```mojo
from python import Python, PythonObject
from python.bindings import PythonModuleBuilder

@export
fn PyInit__scanner() -> PythonObject:
    try:
        var b = PythonModuleBuilder("_scanner")
        b.def_function[scan]("scan", docstring="...")
        return b.finalize()
    except e:
        return abort[PythonObject](String("failed: ", e))

@export
fn scan(root: PythonObject, pattern: PythonObject) raises -> PythonObject:
    # Return Python dict of matches
    var result = Python.evaluate("{}")
    # ... scan logic ...
    return result
```

Build: `mojo build src/scanner/_scanner.mojo --emit shared-lib -o src/hygrep/_scanner.so`

### FFI for C Interop

```mojo
from sys.ffi import c_char, c_int, external_call

fn regcomp(
    preg: UnsafePointer[regex_t],
    pattern: UnsafePointer[c_char],
    cflags: c_int,
) -> c_int:
    return external_call["regcomp", c_int](preg, pattern, cflags)
```

### Memory Management

```mojo
var buffer = alloc[UInt8](size)
defer: buffer.free()
```

### Parallel Patterns

```mojo
@parameter
fn worker(i: Int):
    result[i] = process(items[i])

parallelize[worker](num_items)
```

## Code Standards

| Aspect     | Standard                                    |
| ---------- | ------------------------------------------- |
| Formatting | `mojo format` (automatic)                   |
| Imports    | stdlib → external → local                   |
| Functions  | Docstrings on public APIs                   |
| Memory     | Explicit cleanup, no leaks                  |
| Errors     | `raises` for recoverable, `abort` for fatal |

## Verification

| Check | Command                     | Pass Criteria         |
| ----- | --------------------------- | --------------------- |
| Build | `pixi run build-ext`        | Zero errors           |
| Test  | `pixi run test`             | All pass              |
| Smoke | `pixi run hhg "test" ./src` | Returns results       |
| Wheel | `uv build --wheel`          | Platform-tagged wheel |

## Release to PyPI

**DO NOT trigger release workflow unless user explicitly says "publish to PyPI".**

Prerequisites (one-time):

1. Configure PyPI trusted publishing at https://pypi.org/manage/account/publishing/
2. Add pending publisher: project=`hygrep`, owner=`nijaru`, repo=`hygrep`, workflow=`release.yml`

To release:

```bash
gh workflow run release.yml -f version=X.Y.Z
```

Or via GitHub UI: Actions → Release → Run workflow → Enter version

## AI Context

**Read order:** `ai/STATUS.md` → `ai/DECISIONS.md`

| File              | Purpose                 |
| ----------------- | ----------------------- |
| `ai/STATUS.md`    | Current state, blockers |
| `ai/DECISIONS.md` | Architectural decisions |

## External References

- Mojo stdlib patterns: `~/github/modular/modular/mojo/stdlib/`
- Python extensions: `~/github/modular/modular/mojo/integration-test/python-extension-modules/`
