// Vendored from llama.cpp ggml-cuda/ssm-conv.cu.
// Modification (in the vendor file): `static` stripped from
// `ssm_conv_f32` and `ssm_conv_long_token_f32` so the template
// specializations instantiated by the vendor's `ssm_conv_f32_cuda`
// host helper get externally-linkable PTX symbols. We don't actually
// load those specializations from Rust today — our hot path is the
// BF16 GDN pre-conv wrapped below — but including the vendor file
// keeps the upstream f32 kernels around as a ready fallback and, more
// importantly, brings in `ggml_cuda_op_silu_single` (from unary.cuh)
// which the BF16 wrapper uses.
//
// BF16 wrapper kernels (extern "C" entry points the Rust side loads):
//   * ssm_conv1d_bf16           — conv + fused SiLU, bf16 in/out.
//   * ssm_conv1d_state_update_bf16 — state ring rotation, bf16 in/out.
//
// These kernels match the signatures consumed by
// src/kernels/ssm_conv1d.rs; the public API (`launch_ssm_conv1d_bf16`)
// is unchanged.

// Note: we skip ssm-conv.cuh explicitly — it declares
// `ggml_cuda_op_ssm_conv(..., ggml_tensor * silu_dst = nullptr)` with a
// default argument, and the definition in ssm-conv.cu redeclares the
// same default, which nvcc rejects ("redefinition of default argument").
// Pulling in common.cuh + unary.cuh gives us everything the shim needs
// (the silu device helper and ggml base headers), and the included
// ssm-conv.cu translation unit provides its own forward declarations.
#include "../../vendor/ggml-cuda/common.cuh"
#include "../../vendor/ggml-cuda/unary.cuh"
#include "../../vendor/ggml-cuda/ssm-conv.cu"

#include <cuda_bf16.h>

// ---------------------------------------------------------------------------
// Conv + fused SiLU (bf16).
//
// Reuses the vendor's `ggml_cuda_op_silu_single(float)` device helper
// (from unary.cuh) for the SiLU activation. The rest of the kernel
// mirrors the BF16-specific logic the qwen35 GDN block needs — vendor
// only ships f32 kernels and our embedding/KV tensors are bf16, so we
// keep a thin bf16 kernel here rather than burn a cast bounce on the
// hot path.
// ---------------------------------------------------------------------------

extern "C" __global__ void ssm_conv1d_bf16(
    const __nv_bfloat16 * __restrict__ x,      // [n_tokens, n_channels]
    const __nv_bfloat16 * __restrict__ state,  // [K-1,       n_channels]
    const __nv_bfloat16 * __restrict__ w,      // [K,         n_channels]
    __nv_bfloat16 * __restrict__ y,            // [n_tokens,  n_channels]
    int n_tokens,
    int n_channels,
    int kernel_size                            // K (Qwen3.5 = 4)
) {
    const int c = blockIdx.x * blockDim.x + threadIdx.x;
    const int t = blockIdx.y;
    if (c >= n_channels || t >= n_tokens) {
        return;
    }

    const int K     = kernel_size;
    const int K_m1  = K - 1;

    float acc = 0.0f;
    #pragma unroll 4
    for (int k = 0; k < K; ++k) {
        const int src_idx = t + K_m1 - k;
        float xv;
        if (src_idx < K_m1) {
            xv = __bfloat162float(state[(size_t)src_idx * n_channels + c]);
        } else {
            const int xi = src_idx - K_m1;
            xv = __bfloat162float(x[(size_t)xi * n_channels + c]);
        }
        const float wv = __bfloat162float(w[(size_t)k * n_channels + c]);
        acc += wv * xv;
    }

    // Vendor-supplied SiLU. `ggml_cuda_op_silu_single` is defined
    // __device__ __forceinline__ in vendor/ggml-cuda/unary.cuh and
    // matches SGLang's SiLU reference (x / (1 + exp(-x))).
    const float silu = ggml_cuda_op_silu_single(acc);
    y[(size_t)t * n_channels + c] = __float2bfloat16(silu);
}

// ---------------------------------------------------------------------------
// State update: state_out[i, c] ← concat(state, x)[n_tokens + i, c].
// Run in a separate kernel on the same stream AFTER the conv kernel
// so `state_out` aliasing `state` stays safe (all reads of `state`
// inside the conv kernel complete before we overwrite state_out).
// ---------------------------------------------------------------------------

extern "C" __global__ void ssm_conv1d_state_update_bf16(
    const __nv_bfloat16 * __restrict__ x,
    const __nv_bfloat16 * __restrict__ state,
    __nv_bfloat16 * __restrict__ state_out,
    int n_tokens,
    int n_channels,
    int kernel_size
) {
    const int c = blockIdx.x * blockDim.x + threadIdx.x;
    const int i = blockIdx.y;
    const int K_m1 = kernel_size - 1;
    if (c >= n_channels || i >= K_m1) {
        return;
    }

    const int src_idx = n_tokens + i;
    __nv_bfloat16 v;
    if (src_idx < K_m1) {
        v = state[(size_t)src_idx * n_channels + c];
    } else {
        const int xi = src_idx - K_m1;
        v = x[(size_t)xi * n_channels + c];
    }
    state_out[(size_t)i * n_channels + c] = v;
}
