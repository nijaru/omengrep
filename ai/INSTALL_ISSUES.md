# Installation Issues (Dec 2024)

## Summary

Two issues prevent clean `uv tool install hygrep`:

| Issue                | Cause                             | Fix                         |
| -------------------- | --------------------------------- | --------------------------- |
| Falls back to 0.0.7  | omendb is pre-release (`0.0.1a3`) | Release omendb 0.0.2 stable |
| Fails on Python 3.14 | onnxruntime lacks 3.14 wheels     | Wait for onnxruntime update |

## Issue 1: Pre-release Dependency

### Symptom

```bash
$ uv tool install hygrep --python 3.13
# Installs hygrep==0.0.7 instead of 0.0.9
```

### Root Cause

- hygrep 0.0.9 depends on `omendb>=0.0.1a1`
- The `a1` suffix marks it as a pre-release (alpha)
- uv/pip don't install pre-releases by default
- Resolver falls back to hygrep 0.0.7 (which had omendb as optional)

### Workaround

```bash
uv tool install hygrep --python 3.13 --prerelease=allow
```

### Permanent Fix

1. Release `omendb==0.0.2` (stable, no alpha suffix)
2. Update hygrep dependency to `omendb>=0.0.2`
3. Release hygrep 0.0.10

## Issue 2: Python 3.14 Incompatibility

### Symptom

```bash
$ uv tool install hygrep  # on system with Python 3.14 default
error: No solution found when resolving dependencies
# onnxruntime has no cp314 wheels
```

### Root Cause

- onnxruntime doesn't have Python 3.14 wheels yet
- hygrep depends on onnxruntime for ONNX model inference
- `requires-python = ">=3.11,<3.14"` is set but uv picks Python before checking constraints

### Workaround

```bash
uv tool install hygrep --python 3.13
```

### Permanent Fix

Wait for onnxruntime to release Python 3.14 wheels, then:

1. Add Python 3.14 to release workflow
2. Update `requires-python = ">=3.11,<3.15"`
3. Release new version

## uv Behavior Notes

### `requires-python` Limitation

uv selects Python version _before_ checking package constraints ([uv#16333](https://github.com/astral-sh/uv/issues/16333)). This means:

- `requires-python = ">=3.11,<3.14"` doesn't make uv auto-select 3.13
- Users must explicitly specify `--python 3.13`

### Pre-release Resolution

When a dependency requires a pre-release version:

- uv won't consider it unless `--prerelease=allow` is passed
- Falls back to older versions that don't have the dependency
- No warning is shown about the fallback

## Action Items

- [ ] Release omendb 0.0.2 (stable)
- [ ] Update hygrep to depend on `omendb>=0.0.2`
- [ ] Release hygrep 0.0.10
- [ ] Monitor onnxruntime for Python 3.14 support
