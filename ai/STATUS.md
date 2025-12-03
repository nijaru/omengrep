## Current State

| Metric | Value | Updated |
|--------|-------|---------|
| Phase | 9 (Released) | 2025-12-03 |
| Version | 0.0.2 (PyPI) | 2025-12-03 |
| PyPI | https://pypi.org/project/hygrep/ | 2025-12-03 |
| Perf | ~20k files/sec (Mojo) | 2025-12-02 |
| Inference | ~2s/100 candidates (CPU) | 2025-12-02 |
| Mojo | v25.7 | 2025-12-01 |
| Wheels | 6 (py3.11-3.13 × linux/macos) | 2025-12-03 |

## Active Work

None. v0.0.2 released.

## Completed (Recent)

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
- GPU auto-detection with silent fallback
- Model download validation (size + JSON integrity)
- Partial download cleanup on failure
- Tree-sitter deprecation fix (`Query()` constructor)
- `hygrep info` command for installation verification
- Mojo tree-sitter support added
- Added C, C++, Java, Ruby, C# language support (11 languages total)

## Blockers

None.

## Known Issues

- 128-byte regex memory leak (Mojo v25.7 limitation)
- GPU providers not widely available in conda-forge
- Mojo native scanner requires MAX/Mojo runtime (wheels use Python fallback)

## Next Steps

1. Improve test coverage (see beads)
2. Consider daemon mode for warm model (future)

See `bd list --status=open` for all open issues.
