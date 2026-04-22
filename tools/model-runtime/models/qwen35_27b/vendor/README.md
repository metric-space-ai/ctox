# Vendored third-party kernels for Qwen3.5-27B

This directory holds **verbatim copies** of CUDA source files from
upstream projects used by the Qwen3.5-27B forward. No modifications
beyond the strict minimum needed to compile standalone (header
paths, `extern "C"` entry-point shims). Each file carries the
original upstream license header.

## Upstream sources

### llama.cpp (ggml-cuda) — MIT

Commit/tag pinned in `vendor/llama-cpp.version`. Files:

| File in kernels/sm_XX/ | Upstream path (llama.cpp tree) |
|-----------------------|-------------------------------|
| `fattn_mma_f16.cu`    | `ggml/src/ggml-cuda/fattn-mma-f16.cu` |
| `fattn_common.cuh`    | `ggml/src/ggml-cuda/fattn-common.cuh` |
| `mmvq.cu`             | `ggml/src/ggml-cuda/mmvq.cu` |
| `vecdotq.cuh`         | `ggml/src/ggml-cuda/vecdotq.cuh` |
| `quantize_q8_1.cu`    | `ggml/src/ggml-cuda/quantize.cu` (q8_1 entry only) |
| `rmsnorm.cu`          | `ggml/src/ggml-cuda/norm.cu` (rms path) |
| `softmax.cu`          | `ggml/src/ggml-cuda/softmax.cu` |
| `silu_mul.cu`         | `ggml/src/ggml-cuda/unary.cu` (silu path) |
| `rope.cu`             | `ggml/src/ggml-cuda/rope.cu` (mrope path) |

### dflash (z-lab / lucebox-hub) — MIT

Commit/tag pinned in `vendor/dflash.version`. Files:

| File in kernels/sm_XX/ | Upstream path (dflash tree) |
|-----------------------|----------------------------|
| `gated_delta_net.cu`  | `src/gated_delta_net_kernel.cu` |
| `ssm_conv1d.cu`       | `src/ssm_conv1d_kernel.cu` |
| `l2_norm.cu`          | `src/l2_norm_kernel.cu` |

## Update protocol

Updating an upstream file is a three-step commit chain so the diff
stays reviewable:

1. Bump the `vendor/*.version` pin.
2. `cp <upstream>/<file> kernels/sm_XX/<file>` verbatim — no edits.
3. If the upstream file requires new sibling headers or changed
   include paths, fix those in a **separate** commit.

Rule of thumb: if you find yourself editing vendored `.cu` code for
anything other than include-path tweaks or `extern "C"` shims, stop
and ask — that's drift, and drift is how we got here in the first
place.
