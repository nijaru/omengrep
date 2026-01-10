# ONNX Model Format Research: Cross-Platform Inference (CoreML, CUDA, CPU)

_Research date: 2026-01-09_

## Executive Summary

| Question             | Recommendation                                             |
| -------------------- | ---------------------------------------------------------- |
| Best single format?  | **FP16** - works everywhere, best speed/accuracy tradeoff  |
| CoreML preference?   | FP16 native (ANE runs FP16, casts FP32 down, limited INT8) |
| CUDA tensor cores?   | FP16 faster than INT8 on RTX 30xx/40xx for inference       |
| Ship one or many?    | **One FP16 model** - simplest, works cross-platform        |
| jina FP16 available? | Yes, official `model_fp16.onnx` on HuggingFace             |

**Recommended action for hygrep:** Switch from INT8 to FP16. Same file works on CoreML, CUDA, and CPU with better accuracy and comparable/better performance.

---

## 1. Model Size Comparison (jina-embeddings-v2-base-code)

Official ONNX models from HuggingFace (`jinaai/jina-embeddings-v2-base-code/onnx/`):

| Format | File                   | Size     | Notes                |
| ------ | ---------------------- | -------- | -------------------- |
| FP32   | `model.onnx`           | 611.8 MB | Original precision   |
| FP16   | `model_fp16.onnx`      | 306.2 MB | **Recommended**      |
| INT8   | `model_quantized.onnx` | 154.4 MB | Current hygrep model |

Size reduction: FP32 -> FP16 = 50%, FP32 -> INT8 = 75%

For a ~160M parameter model like jina-embeddings-v2-base-code:

- FP32: ~4 bytes/param = 640 MB
- FP16: ~2 bytes/param = 320 MB
- INT8: ~1 byte/param = 160 MB

---

## 2. CoreML / Apple Neural Engine Requirements

### Native Precision

- **ANE runs FP16 natively** (M1/M2/M3 Neural Engine is FP16-native)
- FP32 models are cast to FP16 by default when using CoreMLExecutionProvider
- BFloat16 not supported on ANE

### INT8 Support (Limited)

- **M1/M2/M3**: Weight-only INT8 quantization (weights compressed, compute in FP16)
- **A17 Pro / M4**: Native INT8-INT8 compute support (activation + weight quantization)
- Many INT8 operators NOT implemented in CoreML:
  - `ConvInteger` - not implemented
  - `MatMulInteger` - limited support
  - Dynamic INT8 operators cause fallback to CPU

### Critical Issue: Silent FP16 Conversion

When using CoreMLExecutionProvider with default settings, ONNX Runtime silently converts FP32 models to FP16 using the legacy "NeuralNetwork" format. This can cause prediction drift.

**Fix for FP32 precision on Mac GPU:**

```python
session = ort.InferenceSession(
    model_path,
    providers=[("CoreMLExecutionProvider", {"ModelFormat": "MLProgram"})]
)
```

### Recommendation for CoreML

- **Use FP16 models directly** - matches ANE native precision
- Avoid INT8 models - limited operator support causes CPU fallback
- FP16 is ~3x faster than FP32 on Mac GPU (FP32 can't run on ANE)

---

## 3. CUDA Tensor Core Performance (RTX 4090)

### RTX 4090 Tensor Core Specs

- 4th generation Tensor Cores (Ada Lovelace)
- Supports: FP16, BF16, TF32, FP8, INT8
- **FP8 NOT available on consumer GPUs** (datacenter only: H100, etc.)

### FP16 vs INT8 on CUDA

| Precision | RTX 4090 Throughput | Notes                     |
| --------- | ------------------- | ------------------------- |
| TF32      | 1.0x baseline       | Auto-enabled for FP32 ops |
| FP16      | 1.6-1.8x vs TF32    | Best for inference        |
| INT8      | ~1.5-2x vs FP16     | Requires calibration      |

**However**, for transformer/embedding models:

- INT8 requires careful calibration for accuracy
- FP16 "just works" with minimal accuracy loss
- ONNX Runtime has excellent FP16 CUDA support

### Lambda Labs Benchmarks (RTX 4090 vs 3090)

```
Model             | RTX 3090 FP16 | RTX 4090 FP16 | Speedup
ResNet50          | 236           | 379           | 1.6x
BERT Base         | 172           | 297           | 1.7x
TransformerXL     | 22863         | 40427         | 1.8x
```

RTX 4090 FP16 is consistently 1.6-1.8x faster than RTX 3090 FP16.

### Recommendation for CUDA

- **FP16 preferred** - excellent tensor core utilization
- INT8 marginal gains, more complexity
- No FP8 on consumer GPUs

---

## 4. CPU Fallback Performance

| Precision | CPU Performance | Notes                      |
| --------- | --------------- | -------------------------- |
| FP32      | Baseline        | Full precision             |
| FP16      | ~Same or slower | No native FP16 SIMD on x86 |
| INT8      | 2-4x faster     | VNNI/AVX-512 acceleration  |

**Key insight**: CPU is the one place INT8 shines. But for a CLI tool with sub-second searches, CPU performance differences are negligible after model load.

For hygrep's use case (cold search ~0.9s, warm search <1ms), the embedding inference is ~100-200ms per query on CPU. FP16 vs INT8 difference is minimal.

---

## 5. Single Model vs Multiple Models

### Option A: Ship One FP16 Model (Recommended)

**Pros:**

- Simpler packaging and distribution
- Works on CoreML (ANE native), CUDA (tensor cores), CPU (acceptable)
- No runtime model selection logic
- Official model available from Jina AI

**Cons:**

- Slightly larger than INT8 (306 MB vs 154 MB)
- Not optimal for pure CPU inference

### Option B: Ship INT8 + FP16

**Pros:**

- Optimal for each platform
- Smaller INT8 for CPU-only users

**Cons:**

- More complex distribution
- Runtime detection needed
- INT8 doesn't work well on CoreML (limited ops)
- Two models to maintain/update

### Recommendation

**Ship FP16 only.** The complexity of multiple models isn't justified:

- CoreML strongly prefers FP16
- CUDA tensor cores are optimized for FP16
- CPU difference is negligible for interactive use
- 306 MB is acceptable for a development tool

---

## 6. jina-embeddings-v2-base-code FP16 Availability

**Official FP16 ONNX model exists:**

```
https://huggingface.co/jinaai/jina-embeddings-v2-base-code/blob/main/onnx/model_fp16.onnx
```

Size: 306.2 MB

The official repo provides:

- `model.onnx` - FP32 (611.8 MB)
- `model_fp16.onnx` - FP16 (306.2 MB)
- `model_quantized.onnx` - INT8 (154.4 MB)

No conversion needed - Jina AI provides all variants.

---

## 7. Implementation Plan for hygrep

### Current State

- Model: `nijaru/jina-code-int8` (INT8, 154 MB)
- Provider: CPUExecutionProvider only
- CoreML: Not supported (INT8 ops not implemented)
- CUDA: Not enabled

### Recommended Changes

1. **Switch to FP16 model**
   - Create `nijaru/jina-code-fp16` or use official `jinaai/jina-embeddings-v2-base-code` with FP16
   - Update `MODEL_FILE = "model_fp16.onnx"`

2. **Enable multi-provider support**

   ```python
   def get_providers() -> list[str]:
       available = set(ort.get_available_providers())
       preferred = [
           "CoreMLExecutionProvider",  # Now works with FP16
           "CUDAExecutionProvider",
           "CPUExecutionProvider",
       ]
       return [p for p in preferred if p in available]
   ```

3. **CoreML configuration**

   ```python
   providers = []
   if "CoreMLExecutionProvider" in available:
       providers.append(("CoreMLExecutionProvider", {
           "ModelFormat": "MLProgram",  # Preserve precision
       }))
   providers.append("CPUExecutionProvider")
   ```

4. **Model size impact**
   - Current INT8: ~154 MB
   - New FP16: ~306 MB
   - Tradeoff: +150 MB for CoreML/CUDA support + better accuracy

---

## 8. Accuracy Considerations

### FP16 vs INT8 Quality

For embedding models, accuracy matters:

- INT8 quantization can shift embedding values slightly
- For similarity search, small shifts accumulate across dimensions
- FP16 maintains better semantic precision

Research shows:

- FP16 embeddings typically within 0.1% of FP32 on retrieval tasks
- INT8 embeddings can show 1-3% degradation depending on calibration

For hygrep (code similarity search), FP16 provides better results.

---

## 9. Performance Summary Table

| Platform             | FP16                  | INT8                      | Notes                                     |
| -------------------- | --------------------- | ------------------------- | ----------------------------------------- |
| Apple ANE (M1-M4)    | Native, fast          | Limited ops, CPU fallback | FP16 wins                                 |
| CUDA (RTX 30xx/40xx) | Tensor core optimized | Marginal gain             | FP16 preferred                            |
| CPU (x86_64)         | Acceptable            | Faster                    | INT8 wins, but negligible for interactive |
| CPU (ARM64)          | Acceptable            | Faster                    | INT8 wins, but negligible for interactive |

**Overall winner: FP16** - consistent cross-platform performance without accuracy tradeoffs.

---

## Sources

- ONNX Runtime CoreML EP Docs: https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html
- CoreML Precision Guide: https://apple.github.io/coremltools/docs-guides/source/opt-quantization-perf.html
- FP16/CoreML Issue Analysis: https://ym2132.github.io/ONNX_MLProgram_NN_exploration
- Lambda Labs RTX 4090 Benchmarks: https://lambda.ai/blog/nvidia-rtx-4090-vs-rtx-3090-deep-learning-benchmark
- Jina ONNX Models: https://huggingface.co/jinaai/jina-embeddings-v2-base-code/tree/main/onnx
- Apple ANE Transformers: https://machinelearning.apple.com/research/neural-engine-transformers

---

_Last updated: 2026-01-09_
