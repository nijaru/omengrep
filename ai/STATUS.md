## Current State

| Metric | Value | Updated |
|--------|-------|---------|
| Phase | 8 (Hardening) | 2025-12-02 |
| Version | 0.0.1 | 2025-12-02 |
| Perf | ~20k files/sec (Mojo) | 2025-12-02 |
| Inference | ~2s/100 candidates (CPU) | 2025-12-02 |
| Mojo | v25.7 | 2025-12-01 |
| Wheels | 6 (py3.11-3.13 × linux/macos) | 2025-12-02 |

## Active Work

Ready for PyPI release. Wheels build and work (Python fallback for users without Mojo runtime).

## Completed (Recent)

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
- Branding: "Hyper + Hybrid grep"
- Added C, C++, Java, Ruby, C# language support (11 languages total)

### Phase 7: CLI Polish (2025-12-02)
- Color output with --color flag
- Gitignore filtering with pathspec
- Grep-compatible exit codes
- Context lines with -C flag
- Stats with --stats
- Config file support
- Shell completions
- Exclude patterns
- Min-score filter
- Hidden files option

### Phase 6: Performance
- Thread optimization: 4-thread ONNX inference
- `--fast` mode: Skip neural reranking
- `-t/--type` filter: Filter by file type
- `--max-candidates`: Cap inference work
- Tree-sitter query caching (15% improvement)

## Blockers

None.

## Known Issues

- 128-byte regex memory leak (Mojo v25.7 limitation)
- GPU providers not widely available in conda-forge
- Mojo native scanner requires MAX/Mojo runtime (wheels use Python fallback)

## Next Steps

1. Distribution: PyPI wheels + CI/CD (hgrep-4n4)
2. Consider daemon mode for warm model (future)

See `bd list --status=open` for all open issues.
