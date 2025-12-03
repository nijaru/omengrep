## Current State

| Metric | Value | Updated |
|--------|-------|---------|
| Phase | 8 (Hardening) | 2025-12-02 |
| Version | 0.0.2 | 2025-12-02 |
| Perf | ~20k files/sec (Recall) | 2025-12-02 |
| Inference | ~2s/100 candidates (CPU) | 2025-12-02 |
| Mojo | v25.7 | 2025-12-01 |

## Active Work

Phase 8 hardening complete. Ready for distribution (hgrep-4n4).

## Completed (Recent)

### Phase 8: Hardening (2025-12-02)
- GPU auto-detection with silent fallback
- Model download validation (size + JSON integrity)
- Partial download cleanup on failure
- Tree-sitter deprecation fix (`Query()` constructor)
- `hygrep info` command for installation verification
- Mojo tree-sitter support added
- Branding: "Hyper + Hybrid grep"

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

## Next Steps

1. Distribution: PyPI wheels + CI/CD (hgrep-4n4)
2. Consider daemon mode for warm model (future)

See `bd list --status=open` for all open issues.
