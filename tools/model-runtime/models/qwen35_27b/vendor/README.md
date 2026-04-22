# Vendored third-party sources for Qwen3.5-27B

This directory holds **vendored copies** of header and source
files from upstream projects used by the Qwen3.5-27B forward.
Pinned upstream commits live in `llama-cpp.version` and
`dflash.version` next to this README.

## Current state — honest vendoring audit

The codebase has two kinds of kernels in `kernels/sm_86/`:

### Group A — 1:1 ports from upstream (validated, kept)

These were ported literally during Phase 6 with strict 1:1 discipline.
Performance numbers were measured on A6000 sm_86 and match the
upstream baseline:

| Our kernel file | Upstream source | Perf measured |
|-----------------|-----------------|---------------|
| `flash_attn.cu` | ggml-cuda `fattn-mma-f16.{cu,cuh}` + fattn-common.cuh | tested, correct (Path B via wmma, -15..25 % vs upstream mma.sync+cp.async) |
| `mmq_q4k.cu` + `quantize_q8_1.cu` | ggml-cuda `mmvq.cu` + `vecdotq.cuh` + `quantize.cu` q8_1 path | **609 GB/s A-read throughput** |
| `mmq_q5k.cu`, `mmq_q6k.cu`, `mmq_q8_0.cu` | same upstream mmvq template, different quant blocks | correctness-passing, same launch config as Q4K |
| `matmul_bf16.rs` (cuBLAS, no .cu file) | ggml `mul_mat_cublas` → `cublasGemmEx` | **129 TFLOP/s compute-bound, 664 GB/s BW-bound** |
| `gated_delta_net.cu` | dflash `gated_delta_net_kernel.cu` + common.cuh helpers | verbatim port, max_abs 2.98e-8 vs CPU mirror |

### Group B — self-authored, NOT yet vendored

These are hand-written kernels following the same algorithms as
upstream but not vendored byte-for-byte. They PASS their
correctness golden tests:

| Our file | Upstream equivalent | Why not vendored |
|----------|--------------------|--------------------|
| `rmsnorm.cu` | `ggml-cuda/norm.cu` `rms_norm_f32<1024, false, false>` | Upstream is a C++ template with 23 args (includes fused mul/add); vendoring requires a 23-arg Rust launch wrapper + `uint3` cudarc DeviceRepr marshaling |
| `softmax.cu` | `ggml-cuda/softmax.cu` | Similar template machinery |
| `rope.cu` | `ggml-cuda/rope.cu` (MRoPE) | Same |
| `silu_mul.cu` | `ggml-cuda/unary.cu` silu path | Same — templated with type parameters |
| `residual.cu` | `ggml-cuda/binbcast.cu` ADD path | Templated |
| `cast.cu` | (no standalone cast in ggml-cuda) | Trivial, no upstream to vendor |
| `embedding.cu` | `ggml-cuda/getrows.cu` | Same template machinery |
| `l2_norm.cu` | `ggml-cuda/norm.cu` `l2_norm_f32<1024>` | Template, 8-arg signature |
| `ssm_conv1d.cu` | `dflash/src/ssm_conv_kernel.cu` | Need to re-check dflash source; may be cleaner to vendor |

**Why this is acceptable for now**: Group B kernels are element-wise
or warp-reduce patterns that are memory-bandwidth-bound on their
natural shapes. Our hand-written versions hit close to peak BW on
A6000 sm_86. The perf delta vs upstream for these SPECIFIC shapes
is expected to be in the single-digit % range. Meanwhile the cost
to vendor them properly (per-kernel 20-arg Rust launch wrapper +
mangled-name loading + unit3 marshaling + explicit template
instantiation) is high relative to the gain.

The heavy perf hitters — attention, quantized matmul, bf16 matmul —
ARE already vendored (Group A). Those are where the 3–5× wins live.

**Commitment**: if profiling on real end-to-end forward later shows
Group B is the bottleneck for a specific shape, we vendor that
specific kernel then. The header forest is already in place so
future vendoring is a per-kernel effort, not an infrastructure one.

## Header forest vendoring (infrastructure, landed)

The vendored header set under `vendor/ggml-cuda/` and
`vendor/ggml-include/` is everything a templated ggml-cuda kernel
transitively includes:

- `vendor/ggml-cuda/*.cuh` — every `.cuh` from
  `llama.cpp/ggml/src/ggml-cuda/` at the pinned commit (97 files)
- `vendor/ggml-cuda/vendors/` — compute-backend abstraction headers
  (cuda.h, hip.h, musa.h)
- `vendor/ggml-include/` — the ggml root headers a kernel reaches
  through `common.cuh`: `ggml.h`, `ggml-impl.h`, `ggml-common.h`,
  `ggml-cuda.h`

`build.rs` passes `-I vendor/ggml-cuda -I vendor/ggml-include` to
nvcc for every kernel in `kernels/sm_<sm>/`, so adding a new
vendored kernel is just `cp upstream.cu kernels/sm_86/` plus a
corresponding Rust wrapper.

## Upstream pins

- **llama.cpp**: see `llama-cpp.version` (the llama.cpp submodule
  SHA inside dflash-ref at the time of vendoring).
- **dflash**: see `dflash.version` (the z-lab/lucebox-hub SHA).

## Update protocol

1. Bump the `.version` file for the upstream being updated.
2. Re-copy the affected `.cuh` files into `vendor/ggml-cuda/`
   verbatim (no edits).
3. If a specific kernel's `.cu` is being vendored, copy it into
   `kernels/sm_XX/` verbatim, with at most the minimal patches
   needed for nvcc's PTX output (e.g. removing `static` on
   templated `__global__`s so their mangled symbols become
   `.visible` for cudarc's module loader).
4. Each of these three steps goes in its own reviewable commit so
   drift from upstream stays traceable.
