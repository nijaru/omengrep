## Current State

| Metric    | Value                             | Updated    |
| --------- | --------------------------------- | ---------- |
| Phase     | 9 (Released)                      | 2025-12-03 |
| Version   | 0.0.5 (PyPI)                      | 2025-12-03 |
| PyPI      | https://pypi.org/project/hygrep/  | 2025-12-03 |
| CLI       | `hhg` (primary), `hygrep` (alias) | 2025-12-03 |
| Languages | 22                                | 2025-12-03 |
| Perf      | ~20k files/sec (Mojo)             | 2025-12-02 |
| Inference | ~2s/100 candidates (CPU)          | 2025-12-02 |
| Mojo      | v25.7                             | 2025-12-01 |
| Wheels    | 6 (py3.11-3.13 × linux/macos)     | 2025-12-03 |

## Active Work

None.

## Completed (Recent)

### CLI Improvements (2025-12-03)

- Add progress bar for reranking batches (Rich Progress)
- Add syntax highlighting for code context (Rich Syntax, 40+ extensions)
- Modernize CLI with Typer + Rich (visible subcommands, examples panel)

### v0.0.5 Release (2025-12-03)

- Add 11 new language grammars (22 total): Bash, PHP, Kotlin, Lua, Swift, Elixir, Zig, Svelte, YAML, TOML, JSON
- Fix 128-byte regex memory leak in Mojo scanner
- Fix regex escaping in query expansion
- Fix config load errors reporting to stderr
- Fix hatch_build.py to error on unsupported platforms
- Code quality: loop variable shadowing (PLW2901), list comprehension (PERF401)
- Add Python fallback scanner tests (18 tests)

### v0.0.4 Release (2025-12-03)

- Fix name extraction for Go methods and Rust traits/structs/enums
- Add golden dataset integration tests (21 tests)

### v0.0.3 Release (2025-12-03)

- Suppress ONNX Runtime warnings on macOS (CoreML capability + Context leak)
- Skip CoreML provider (CPU fast enough for model size)

### v0.0.2 Release (2025-12-03)

- `hygrep model [install|clean]` commands
- HuggingFace cache integration (shared `~/.cache/huggingface/`)
- Offline-first: auto-download on first use, then cached
- `cache_dir` config option for custom cache location
- Integration tests for model install/clean cycle
- Shell completions updated for new commands

### v0.0.1 Release (2025-12-03)

- Published to PyPI via trusted publishing
- GitHub release created
- Tested on macOS (arm64) and Fedora (x86_64)
- Linting fixed (ruff, vulture, ty)

### Wheel Distribution Fix (2025-12-02)

- Added Python fallback scanner for wheels (Mojo runtime not bundled)
- Dropped Python 3.14 (onnxruntime not available yet)
- CI builds 6 wheels: 3.11, 3.12, 3.13 × linux-64, macos-arm64

### Phase 8: Hardening (2025-12-02)

- Model download validation (size + JSON integrity)
- Partial download cleanup on failure
- Tree-sitter deprecation fix (`Query()` constructor)
- `hygrep info` command for installation verification
- Mojo tree-sitter support added
- Added C, C++, Java, Ruby, C# language support (11 languages total)

## Blockers

None.

## Known Issues

- Mojo native scanner requires MAX/Mojo runtime (wheels use Python fallback)

## GPU Support

**Status:** CPU-only until GPU support is ready.

| Provider | Status       |
| -------- | ------------ |
| CPU      | ✅ Current   |
| CUDA     | ❌ Not ready |
| CoreML   | ❌ Not ready |

See `ai/DECISIONS.md` section 8 for details.

## Next Steps

1. Consider daemon mode for warm model (future)
2. GPU support when onnxruntime packages are ready

See `bd list --status=open` for all open issues.
