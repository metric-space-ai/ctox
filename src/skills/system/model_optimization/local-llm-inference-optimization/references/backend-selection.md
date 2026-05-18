# Backend Selection

Use this reference when choosing CPU, GPU, NPU, framework, custom kernel,
quantization, or sparse strategies for a local LLM inference engine.

## Hardware-First Rule

Do not optimize for an imagined platform. Capture local facts:

```text
CPU:
  ISA extensions, vector width, matrix extensions, thread count, memory sharing

GPU:
  generation, SIMDgroup size, matrix/tensor support, storage modes, counters

NPU/ANE/TPU:
  accessible API, graph restrictions, dtype support, placement visibility

Memory:
  capacity, sustained bandwidth, unified/discrete behavior, cache constraints

Runtime:
  Metal/MPS/MPSGraph/Core ML, CUDA/cuBLAS/TensorRT, Vulkan, ROCm, Accelerate,
  oneDNN, vendor graph compiler, framework scheduling behavior
```

Verify feature use with probes, disassembly, traces, or placement reports.
Feature availability alone is not speed evidence.

## Backend Matrix

Use this selection pattern:

```text
large dense projections / FFN / LM head:
  try platform matrix/tensor backend first
  compare to custom kernel with real model shapes

attention prefill:
  exact tiled QK-softmax-V, FlashAttention-style schedule, or mature backend
  avoid materializing full dense scores at long contexts unless it is proven OK

decode attention:
  Flash-Decoding/Split-K only when output length/context thresholds justify
  scratch and dispatch overhead

recurrent/stateful layers:
  custom kernels or specialized recurrence schedule
  keep state GPU-local
  avoid per-token CPU intervention

normalization/reductions:
  SIMDgroup reductions first, but validate numerical order and wall time

sampling/top-k/argmax:
  keep on accelerator; CPU reads only the next token

NPU/ANE:
  coarse graph, prefill, vision/audio encoder, or full graph baseline
  avoid per-layer ping-pong with GPU custom kernels
```

## Native Runtime Boundary

The optimized engine should be owned by CTOX:

```text
host runtime:
  Rust by default

kernel code:
  MSL, CUDA, Vulkan/GLSL/SPIR-V, HIP/ROCm, CPU C/ASM, or platform-specific
  kernel language

framework backend:
  allowed only as an integrated sidecar/backend component with measured
  overhead and correctness gates
```

Do not treat a Python/MLX/PyTorch script as the final engine. It can be:

```text
reference implementation
correctness oracle
artifact converter
backend feasibility probe
```

The final path should expose native build/test/bench commands and a reusable
model-specific runtime API.

## Matrix Backend Rule

Before hand-optimizing large dense matmul:

```text
pack weights into backend-native layout
run matrix backend probe with real dimensions
run integrated sidecar benchmark
compare against MSL/custom kernel
check correctness and dtype behavior
```

The Qwen3.5 lesson: MPS sidecars for FFN, Delta project, Attention O, and
DeltaOut were more valuable than continuing to tune every dense projection in
handwritten MSL.

## Custom Kernel Rule

Write custom kernels when:

```text
the operator is stateful or recurrent
online softmax or streaming state must remain on-chip
the framework backend cannot express the layout
the kernel fuses memory-bound glue around a matrix backend
sampling or reduction needs only compact output
quantized/sparse traversal is model-specific
```

Custom kernels must be shape-specialized to the model where possible.
Generic kernels are baseline tools, not the final optimization target.

## SIMD Rule

SIMD/SIMT is a design primitive, not a late cleanup:

```text
use SIMDgroup ownership for row/head reductions
use lane-local vectors for head_dim chunks
use simd_sum/max where reduction order is acceptable
use threadgroup memory only when cross-SIMD sharing pays for barriers
```

But SIMD does not guarantee speed. It can lose due to:

```text
changed numerical reduction order
register pressure
lower occupancy
extra shuffles
bad memory reuse
tail underfill
```

Measure every SIMD rewrite against the integrated path.

## Quantization Rule

Quantization is a hardware/backend decision:

```text
choose formats the platform can consume efficiently
quantize offline or at load time
do not materialize full dequant tensors in the hot path
avoid f32 -> f16 -> f32 ping-pong
keep scales/zero-points adjacent to the packed groups
measure group size and layout empirically
```

Useful candidates:

```text
weight-only int8/int4 with in-dot dequant
KV-cache quantization for long-context decode
activation quantization only with a quality gate
CPU SME/I8MM only when both operands and layout fit the instruction path
GPU tensor/matrix int8 only when the runtime exposes a real fast path
```

Reject quantized paths that save bytes but lose to scalar unpack/dequant cost.

## Sparse And Approximate Rule

Separate exact and approximate rows:

```text
exact:
  FlashAttention-style tiling, paged memory, better scheduling, no semantic loss

approximate:
  sparse/windowed attention, KV pruning, lower precision, selected heads/pages,
  recurrent approximation
```

Approximate paths may be product-valid, but only with explicit quality gates
and clear user/model constraints.

## CPU Orchestration Rule

CPU optimization matters when it removes overhead, not when it recreates the
model hot path:

```text
precompile pipelines
cache descriptors by hash/map, not linear scans
preallocate buffers
avoid per-layer decisions
use one sync per generated token
do not read hidden states or full logits
```

If CPU SIMD/SME is considered, run it as a backend column with its own roofline,
not as an assumption.
