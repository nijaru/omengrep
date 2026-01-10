# ONNX Runtime Execution Providers Research (2025/2026)

## Summary

| Question                          | Answer                                                    |
| --------------------------------- | --------------------------------------------------------- |
| CoreML in standard `onnxruntime`? | **No** - requires `onnxruntime-coreml` or custom build    |
| CUDA package?                     | `onnxruntime-gpu` (CUDA 12.x default since v1.19)         |
| Auto-fallback with provider list? | **Yes** - pass list, ORT uses first available             |
| Single cross-platform package?    | **No** - need platform-specific deps or runtime detection |
| INT8 on CoreML?                   | **Limited** - ConvInteger and some ops not supported      |
| INT8 on CUDA?                     | **Works** - full support on modern GPUs                   |

---

## 1. CoreML on macOS

### Standard Package Status

The standard `onnxruntime` pip package does **NOT** include CoreMLExecutionProvider. CoreML EP is only available via:

1. **`onnxruntime-coreml`** - Third-party build (last version 1.13.1, March 2023 - outdated)
2. **`onnxruntime-silicon`** - Third-party build for Apple Silicon (last version 1.16.3, Jan 2024 - also outdated)
3. **Custom build** - Build from source with `--use_coreml` flag
4. **CocoaPods** - Pre-built for iOS/macOS native apps (`onnxruntime-c`, `onnxruntime-objc`)

### Requirements

- macOS 10.15+ or iOS 13+
- Apple Neural Engine (ANE) recommended for best performance
- MLProgram format requires macOS 12+ / iOS 15+

### Current State (2025)

The third-party packages (`onnxruntime-silicon`, `onnxruntime-coreml`) are significantly outdated. For Python on macOS, the practical options are:

- **Use CPU provider** (default, works everywhere)
- **Build from source** (complex, not suitable for distribution)

---

## 2. CUDA on Linux

### Package

```bash
pip install onnxruntime-gpu
```

### Version Compatibility (as of v1.20+)

| ONNX Runtime | CUDA | cuDNN | Notes                                             |
| ------------ | ---- | ----- | ------------------------------------------------- |
| 1.20.x+      | 12.x | 9.x   | Default in PyPI, compatible with PyTorch >= 2.4.0 |
| 1.20.x+      | 11.8 | 8.x   | Not in PyPI, use Azure DevOps feed                |
| 1.18.x       | 11.8 | 8.x   | Last version in PyPI with CUDA 11                 |

### Key Points

- **CUDA 12.x is default** since v1.19.0
- Minor version compatibility: ORT built with CUDA 12.x works with any CUDA 12.x
- cuDNN 8.x and 9.x are NOT compatible with each other
- Can use PyTorch's bundled CUDA/cuDNN (no separate install needed)

### PyTorch Compatibility Trick

```python
# Option 1: Import PyTorch first to preload CUDA DLLs
import torch
import onnxruntime as ort

# Option 2: Use preload_dlls (v1.21.0+)
import onnxruntime as ort
ort.preload_dlls(cuda=True, cudnn=True)
```

---

## 3. Automatic Provider Selection

**Yes, this works.** Pass a list of providers and ORT tries them in order:

```python
import onnxruntime as ort

# ORT will use first available provider
session = ort.InferenceSession(
    "model.onnx",
    providers=[
        "CoreMLExecutionProvider",   # macOS with CoreML build
        "CUDAExecutionProvider",      # Linux with GPU
        "CPUExecutionProvider",       # Fallback (always available)
    ]
)

# Check which providers are actually available
print(ort.get_available_providers())
# e.g., ['CUDAExecutionProvider', 'CPUExecutionProvider']

# Check which providers the session is using
print(session.get_providers())
```

### Behavior

- Unknown providers are silently skipped (with a warning)
- First available provider that can handle operations is used
- CPUExecutionProvider always available as fallback
- Operators not supported by an EP fall back to CPU automatically

### Gotcha: Fallback Bug

There's a known issue (GitHub #25145) where fallback logic can sometimes lose GPU acceleration unexpectedly. Best practice is to check `session.get_providers()` after creation.

---

## 4. Package Compatibility and Platform-Specific Dependencies

### The Problem

- `onnxruntime` (CPU) - works everywhere
- `onnxruntime-gpu` - Linux only, requires CUDA
- `onnxruntime-coreml` - macOS only, outdated
- These packages conflict (can't install together)

### Solutions

#### Option A: Runtime Detection (Recommended for hygrep)

Use base `onnxruntime` package, detect available providers at runtime:

```python
import onnxruntime as ort

def get_best_providers() -> list[str]:
    """Return providers in preference order, filtered to available ones."""
    available = set(ort.get_available_providers())

    # Preference order
    preferred = [
        "CUDAExecutionProvider",
        "CoreMLExecutionProvider",
        "CPUExecutionProvider",
    ]

    return [p for p in preferred if p in available]
```

**Pros:** Simple, single package, works everywhere
**Cons:** GPU users must install `onnxruntime-gpu` separately

#### Option B: Optional Dependencies

```toml
# pyproject.toml
[project]
dependencies = [
    "onnxruntime>=1.16",  # Base CPU package
]

[project.optional-dependencies]
cuda = [
    "onnxruntime-gpu>=1.16; sys_platform == 'linux'",
]
```

**Gotcha:** Can't have both `onnxruntime` and `onnxruntime-gpu` installed - they conflict.

#### Option C: Dependency Groups (uv/pdm)

```toml
# Using uv's dependency groups
[dependency-groups]
cuda = ["onnxruntime-gpu>=1.16"]
cpu = ["onnxruntime>=1.16"]
```

Then install with `uv sync --group cuda` or `uv sync --group cpu`.

#### Option D: Platform Markers

```toml
[project]
dependencies = [
    # Can't actually do this - packages conflict
    # "onnxruntime-gpu>=1.16; sys_platform == 'linux'",
    # "onnxruntime>=1.16; sys_platform == 'darwin'",
]
```

This **doesn't work** because `onnxruntime-gpu` and `onnxruntime` are separate packages that both provide the `onnxruntime` module.

---

## 5. Gotchas

### INT8 Quantized Models

#### CoreML

**Limited support.** CoreML EP does not implement several INT8 operators:

- `ConvInteger` - not implemented
- `MatMulInteger` - limited support
- `QLinearConv` - may work in MLProgram format

Example error:

```
[ONNXRuntimeError] : 9 : NOT_IMPLEMENTED : Could not find an implementation
for ConvInteger(10) node with name '/model/Conv_quant'
```

**Recommendation:** Use FP32 or FP16 models with CoreML, not INT8.

#### CUDA

**Full support** on compute capability 7.0+ GPUs (V100, RTX 20xx+).

- INT8 operations use Tensor Cores when available
- Older GPUs (V100) may be slower with INT8 than FP16

### CoreML Caching

CoreML compiles models at load time, which can take minutes for complex models. Use caching:

```python
session = ort.InferenceSession(
    "model.onnx",
    providers=[
        ("CoreMLExecutionProvider", {
            "ModelCacheDirectory": "/path/to/cache"
        }),
        "CPUExecutionProvider",
    ]
)
```

### CUDA Memory

```python
session = ort.InferenceSession(
    "model.onnx",
    providers=[
        ("CUDAExecutionProvider", {
            "device_id": 0,
            "gpu_mem_limit": 4 * 1024 * 1024 * 1024,  # 4GB limit
            "arena_extend_strategy": "kSameAsRequested",
        }),
    ]
)
```

---

## 6. Recommended Implementation for hygrep

Given that hygrep uses an INT8 quantized model (`jina-code-int8`), the practical approach is:

### pyproject.toml

```toml
[project]
dependencies = [
    "onnxruntime>=1.16",  # Base package, CPU always works
    # ... other deps
]

[project.optional-dependencies]
cuda = [
    "onnxruntime-gpu>=1.19; sys_platform == 'linux'",
]
```

### embedder.py - Provider Selection

```python
import onnxruntime as ort

def get_providers() -> list[str]:
    """Get execution providers in preference order."""
    available = set(ort.get_available_providers())

    # INT8 models: skip CoreML (limited INT8 support)
    # For FP32/FP16 models, could add CoreMLExecutionProvider
    preferred = [
        "CUDAExecutionProvider",
        "CPUExecutionProvider",
    ]

    return [p for p in preferred if p in available]

# In Embedder._ensure_loaded():
self._session = ort.InferenceSession(
    model_path,
    sess_options=opts,
    providers=get_providers(),
)

# Log which provider is actually being used
actual = self._session.get_providers()[0]
if actual != "CPUExecutionProvider":
    logger.info(f"Using {actual} for inference")
```

### Installation Instructions for GPU Users

```bash
# Linux with CUDA 12.x
pip uninstall onnxruntime  # Remove CPU version first
pip install onnxruntime-gpu

# Verify
python -c "import onnxruntime as ort; print(ort.get_available_providers())"
# Should show: ['CUDAExecutionProvider', 'CPUExecutionProvider']
```

---

## Sources

- ONNX Runtime Install Docs: https://onnxruntime.ai/docs/install/
- CoreML EP Docs: https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html
- CUDA EP Docs: https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html
- GitHub Issue on INT8/CoreML: https://huggingface.co/onnx-community/dfine_n_coco-ONNX/discussions/1
- Provider Fallback Bug: https://github.com/microsoft/onnxruntime/issues/25145

---

_Last updated: 2026-01-09_
