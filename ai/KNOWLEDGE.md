# Knowledge Base

## Mojo Stable (v0.25.7)

### UnsafePointer & Allocation
**Syntax:** `UnsafePointer.alloc` exists but `alloc` from `memory` is preferred/future-proof.
**Quirk:** `UnsafePointer[T]` type inference for `mut` and `origin` parameters can be brittle when used in struct fields or aliases.
**Workaround:** For FFI, casting pointers to `Int` (Address) is a reliable way to store and pass pointers without fighting the type checker.

```mojo
from memory import alloc

# Alloc returns UnsafePointer[T]
var ptr = alloc[UInt8](10)
var addr = Int(ptr) 

# Pass 'addr' to C functions taking void*
external_call["c_func", NoneType](addr)
```

### Libc Binding
Use `Int` as the type alias for `void*` to avoid `UnsafePointer[NoneType]` inference issues.
