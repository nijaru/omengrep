# Mojo & MAX Project Reference

**Based on:** `modular/modular` repository analysis (Stable v0.25.7).

## 1. Project Setup (Pixi)

The standard way to set up a Mojo project with MAX dependencies is using `pixi`.

```toml
[workspace]
name = "hgrep"
version = "0.1.0"
description = "Agent-Native search tool"
channels = ["conda-forge", "https://conda.modular.com/max/", "pytorch"]
platforms = ["osx-arm64", "linux-64"]

[dependencies]
max = ">=25.7"
python = ">=3.11,<3.14"
onnxruntime = ">=1.16.0,<2"

[tasks]
build = "mojo build cli.mojo -o hygrep"
run = "mojo cli.mojo"
test = "mojo -I . tests/test_regex_smoke.mojo"
```

## 2. MAX Engine Architecture

The MAX Engine uses a **Graph-based architecture**.

### Key Components:
1.  **Mojo Graph Operations (`max.kernels`):** Low-level kernels registered via `@compiler.register`.
2.  **Python Graph API (`max.graph`):** Constructs the graph. currently NO public high-level Mojo Graph Construction API.
3.  **Execution:** Models are typically loaded/run via `max.engine.InferenceSession` (Python).

### "Single Static Binary" Reality
Since the high-level Graph API is Python-centric, the "standard" path for MAX + Mojo is **Hybrid**:
1.  **Python Interop:** Use `from python import Python` to import `max.engine` (or `onnxruntime`).
2.  **Execution:** Load and run the model from Mojo, driving the Python object.

**Decision for `hgrep`:**
We use **Python Interop**.
*   **Primary:** `onnxruntime` (via Python) for maximum compatibility and ease of shipping (Phase 2).
*   **Secondary:** `max.engine` (via Python) for performance on supported hardware.

## 3. Directory Structure
Recommended structure for `hgrep`:

```
hgrep/
├── pixi.toml             # Dependencies
├── src/
│   ├── cli.mojo          # Entry point
│   ├── scanner/          # "Hyper Scanner" (Mojo + Libc)
│   └── inference/        # AI Integration (Python Interop)
└── models/               # ONNX models
```

## 4. Mojo Standard Library (v0.25.7 Stable)

### Allocation
Use `alloc` from `memory` for best practice, though `UnsafePointer.alloc` exists.

```mojo
from memory import alloc

fn example():
    var p = alloc[Int](10)
    p[0] = 1
    p.free()
```

### FFI & C-Bindings (The "Int" Pattern)
To bind C functions taking `void*` or `char*` without fighting type inference:
1.  Allocate using `alloc`.
2.  Cast to `Int` (Address) for storage/passing.
3.  Pass `Int` to `external_call`.

```mojo
from sys import external_call
from memory import alloc

alias VoidPtr = Int

fn call_c_func(ptr: VoidPtr):
    external_call["c_func", NoneType](ptr)

fn main():
    var p = alloc[UInt8](10)
    call_c_func(Int(p))
    p.free()
```

### Testing
The `mojo test` command is **removed**. Use `TestSuite`.

```mojo
from testing import TestSuite, assert_true

fn test_something():
    assert_true(True)

fn main():
    TestSuite.discover_tests[__functions_in_module()]().run()
```

### Lifecycle Methods (New Syntax)
Use `deinit` keyword instead of `owned` for `__del__` and `__moveinit__` arguments to avoid warnings.

```mojo
struct MyStruct:
    fn __moveinit__(out self, deinit existing: Self):
        pass

    fn __del__(deinit self):
        pass
```

## 5. Code Examples

### Python Interop (The "Standard" Way)
```mojo
from python import Python

fn rerank(query: String, candidates: List[String]) raises:
    # Import Engine via Python
    var ort = Python.import_module("onnxruntime")
    var session = ort.InferenceSession("models/reranker.onnx")
    
    # Execute
    var inputs = ... 
    var outputs = session.run(None, inputs)
```