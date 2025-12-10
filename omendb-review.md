# OmenDB Review - Issues Found and Fixed

## Summary

During hygrep integration, several issues were discovered in OmenDB v0.0.9. Most have been fixed and will ship in v0.0.10.

## Issues Fixed

### 1. `__version__` Returns Wrong Value

**Status:** Fixed (will ship in v0.0.10)

```python
>>> import omendb
>>> omendb.__version__
'0.0.1a1'  # Should be '0.0.9'
```

**Root cause:** `python/omendb/__init__.py` had hardcoded version that wasn't synced.

**Fix:**

- Updated `__version__` to `0.0.9`
- Added `python/omendb/__init__.py` as 8th location in `sync-version.sh`
- Added `src/ffi.rs` as 9th location (was stuck at `0.0.5`)
- Added verification to `release.yml`

### 2. README Incorrectly Claims Memory Reduction

**Status:** Fixed (will ship in v0.0.10)

**Before:** "Compression - RaBitQ quantization for 4-8x memory reduction"

**After:** "RaBitQ quantization - Two-phase search for faster candidate filtering"

**Added note:** `quantization` enables two-phase search but does not reduce disk or memory usage.

## Issues Identified (Not Yet Fixed)

### 3. Quantization Doesn't Reduce Storage

**Current behavior:**

- Stores BOTH original vectors AND quantized vectors to disk
- Both are kept in memory during runtime
- Disk size WITH quantization is actually larger

**Test results (5K vectors, 256D):**

```
No quant: 5443KB
Quant=4:  5443KB (same - stores both)
```

**What quantization actually does:**

- Two-phase search: fast candidate filtering with quantized, rerank with originals
- Improves search SPEED at scale
- Does NOT reduce memory or disk usage

**Potential future options:**

1. `store_only_quantized=True` - Lossy mode for actual disk savings
2. Memory-mapped quantized-only search mode
3. Separate `disk_quantization` config

### 4. Version Locations (Now 9 files)

The sync-version.sh script now handles all locations:

| #   | File                      | Description          |
| --- | ------------------------- | -------------------- |
| 1   | VERSION                   | Source of truth      |
| 2   | Cargo.toml                | Main Rust crate      |
| 3   | omendb-core/Cargo.toml    | Core algorithms      |
| 4   | python/Cargo.toml         | Python bindings      |
| 5   | python/omendb/**init**.py | Python `__version__` |
| 6   | src/ffi.rs                | C FFI version        |
| 7   | node/Cargo.toml           | Node bindings        |
| 8   | node/package.json         | npm package          |
| 9   | node/wrapper/package.json | npm wrapper          |

## Recommendation for hygrep

Use OmenDB with defaults (no quantization) until storage-efficient quantization is implemented:

```python
self._db = omendb.open(self.vectors_path, dimensions=DIMENSIONS)
```

If quantization is needed for search speed at scale (>100K vectors), use:

```python
self._db = omendb.open(self.vectors_path, dimensions=DIMENSIONS, quantization=4)
```

Note: This will INCREASE disk usage but may improve search performance.

## Benchmark Results (Reference)

M3 Max, 128GB:

| Scale       | Insert Time | Search Time | Index Size |
| ----------- | ----------- | ----------- | ---------- |
| 1K vectors  | 0.6s        | <0.1ms      | 1MB        |
| 10K vectors | 13s         | 0.1ms       | 11MB       |
| 50K vectors | 100s        | 0.2ms       | 53MB       |

Search performance is excellent. Insert is slow at scale (~2K vectors/sec).

## Files Changed in OmenDB

- `python/omendb/__init__.py` - Fixed `__version__`
- `src/ffi.rs` - Fixed FFI version string
- `scripts/sync-version.sh` - Added 9 locations
- `.github/workflows/release.yml` - Added version verification
- `README.md` - Fixed quantization claims
