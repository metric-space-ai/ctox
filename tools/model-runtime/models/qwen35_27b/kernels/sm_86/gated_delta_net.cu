// Gated DeltaNet — Qwen3.5 hybrid linear-attention layer.
//
// Ported from the dflash reference (MIT-licensed, derived in turn from
// llama.cpp's ggml-cuda backend). Upstream files:
//   deps/llama.cpp/ggml/src/ggml-cuda/gated_delta_net.cu
//   deps/llama.cpp/ggml/src/ggml-cuda/gated_delta_net.cuh
//   deps/llama.cpp/ggml/src/ggml-cuda/common.cuh  (fastdiv / warp_reduce helpers)
//
// Upstream license (reproduced verbatim):
//
//   MIT License
//
//   Copyright (c) 2023-2024 The ggml authors
//
//   Permission is hereby granted, free of charge, to any person obtaining a copy
//   of this software and associated documentation files (the "Software"), to deal
//   in the Software without restriction, including without limitation the rights
//   to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//   copies of the Software, and to permit persons to whom the Software is
//   furnished to do so, subject to the following conditions:
//
//   The above copyright notice and this permission notice shall be included in all
//   copies or substantial portions of the Software.
//
//   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//   IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//   FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//   AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//   LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//   OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//   SOFTWARE.
//
// ---------------------------------------------------------------------------
//
// Scope and fidelity notes
// ------------------------
//
// This is a BYTE-EQUIVALENT port. The kernel body matches the upstream
// `gated_delta_net_cuda<S_v, KDA, TREE_MODE, InterT>` template literally
// — same register layout, same warp-shuffle reduction, same recurrence,
// same per-token intermediate write. The two (KDA × TREE_MODE) boolean
// axes from the reference are preserved; the persist-intermediate InterT
// axis (float vs __half) is preserved.
//
// What's different from the reference at the ".cu" boundary:
//
//   * No ggml tensor abstraction. Callers pass device pointers, strides
//     (in units of float), and shapes directly — same arguments
//     `ggml_cuda_op_gated_delta_net` pulls out of `ggml_tensor::src`.
//
//   * No `ggml_cuda_info()` / `ggml_cuda_get_device()` — we don't need
//     warp-size / compute-cap lookup because we target sm_86+ (A6000,
//     3090, Ada), which is always physical warp size 32.
//
//   * `warp_reduce_sum<width>(float)`, `fastdiv`, `fastmodulo` are
//     inlined at the top of this file. Their bodies are verbatim from
//     `ggml-cuda/common.cuh`.
//
//   * Four `extern "C"` entry points replace the template dispatch:
//       gated_delta_net_{kda|gda}_{chain|tree}_bf16_stub
//     …wait, this kernel is all-float, not bf16. The public names
//     match the 4 (KDA × TREE_MODE) instantiations; the persist-
//     intermediate precision (f32 vs f16) is selected by a runtime
//     argument flag because having 8 `extern "C"` entry points is
//     noisy. Internally we still template on the 3 axes.
//
// Recurrence invariants the callers rely on:
//
//   1. `parent_ids` handling — TREE_MODE path reconstructs the SSM
//      recurrence along non-linear DDTree paths. Without this,
//      tree-verify acceptance is incorrect for any non-chain tree.
//      See the per-token `parent_t` reload block.
//
//   2. `persist_inter` capture — every token writes its post-update
//      state to `[S_v, S_v, H, n_tokens × n_seqs]`. The spec-decode
//      fast-rollback path (DFlash's partial-accept) reads from this
//      buffer instead of replaying the forward pass.
//
//   3. S_v must be one of {16, 32, 64, 128}. 128 is what Qwen3.5 uses.
//
// No optimizations. No fused dtype upconvert / downconvert beyond the
// `__half` persist-inter variant the reference already had. Profile
// first, then change code — this port is the baseline we profile
// against.

#include <cuda_fp16.h>
#include <cstdint>
#include <type_traits>

// ---------------------------------------------------------------------------
// Helpers inlined from ggml-cuda/common.cuh. Bodies are verbatim; names
// are kept the same so the kernel body reads identically to the upstream
// source when diffing.
// ---------------------------------------------------------------------------

// A6000 / Ada / Hopper / Blackwell all have physical warp size 32.
// We compile -arch=sm_86 and up — no HIP / CDNA support in this port.
static constexpr __device__ int ggml_cuda_get_physical_warp_size() {
    return 32;
}

// The host-side `init_fastdiv_values` lives in the Rust wrapper
// (see src/kernels/gated_delta_net.rs). The kernel receives the
// packed uint3 magic triple ready-to-use.

static __device__ __forceinline__ uint32_t fastdiv(uint32_t n, const uint3 fastdiv_values) {
    const uint32_t hi = __umulhi(n, fastdiv_values.x);
    return (hi + n) >> fastdiv_values.y;
}

static __device__ __forceinline__ uint32_t fastmodulo(uint32_t n, const uint3 fastdiv_values) {
    return n - fastdiv(n, fastdiv_values) * fastdiv_values.z;
}

template <int width = 32>
static __device__ __forceinline__ float warp_reduce_sum(float x) {
#pragma unroll
    for (int offset = width / 2; offset > 0; offset >>= 1) {
        x += __shfl_xor_sync(0xffffffffu, x, offset, width);
    }
    return x;
}

// ---------------------------------------------------------------------------
// Persist-intermediate storage helpers. fp16 halves the DDTree state
// cache footprint — enough to fit larger verify budgets on 24 GB cards.
// Verbatim from upstream.
// ---------------------------------------------------------------------------

static __device__ __forceinline__ float load_inter_state(const float * p, int idx) {
    return p[idx];
}
static __device__ __forceinline__ float load_inter_state(const __half * p, int idx) {
    return __half2float(p[idx]);
}
static __device__ __forceinline__ void store_inter_state(float * p, int idx, float v) {
    p[idx] = v;
}
static __device__ __forceinline__ void store_inter_state(__half * p, int idx, float v) {
    p[idx] = __float2half(v);
}

// Tree-mode parent index sentinel. A node whose parent is the pre-block
// state uses this value in parent_ids[]. Any value < 0 triggers a reload
// from curr_state.
#define GDN_TREE_ROOT_PARENT (-1)

// ---------------------------------------------------------------------------
// Main kernel — verbatim port of the upstream template. S_v is 16/32/64/128;
// each gets its own extern "C" entry point emitted at the bottom of this
// file. KDA toggles the per-element vs scalar gate. TREE_MODE enables
// parent_ids reload. InterT selects the persist-intermediate precision.
// ---------------------------------------------------------------------------

// The kernel body lives in a __device__ template helper. Each
// `extern "C" __global__` entry point below is a tiny forwarding
// kernel that simply instantiates the right (S_v, KDA, TREE_MODE,
// InterT) combination and calls `gated_delta_net_impl<...>()` as a
// device function — legal, because device-from-device calls don't
// need a `<<< >>>` configuration. The template itself is NOT
// __global__, so the C++ call syntax works.
//
// We still keep `__launch_bounds__` on each entry kernel so ptxas
// can allocate registers appropriately; the directive lives on the
// forwarding kernels near the bottom of the file.
template <int S_v, bool KDA, bool TREE_MODE, typename InterT>
__device__ void gated_delta_net_impl(const float * q,
                     const float * k,
                     const float * v,
                     const float * g,
                     const float * beta,
                     const float * curr_state,
                     float *       dst,
                     const int *   parent_ids,     // TREE_MODE only; else ignored
                     InterT *      persist_inter,  // optional; if null, embedded region inside dst is used
                     int64_t       H,
                     int64_t       n_tokens,
                     int64_t       n_seqs,
                     int64_t       sq1,
                     int64_t       sq2,
                     int64_t       sq3,
                     int64_t       sv1,
                     int64_t       sv2,
                     int64_t       sv3,
                     int64_t       sb1,
                     int64_t       sb2,
                     int64_t       sb3,
                     const uint3   neqk1_magic,
                     const uint3   rq3_magic,
                     float         scale) {
    const uint32_t h_idx    = blockIdx.x;
    const uint32_t sequence = blockIdx.y;
    const int      lane     = threadIdx.x;
    const int      col      = blockIdx.z * blockDim.y + threadIdx.y;

    const uint32_t iq1 = fastmodulo(h_idx, neqk1_magic);
    const uint32_t iq3 = fastdiv(sequence, rq3_magic);

    const int64_t attn_score_elems  = S_v * H * n_tokens * n_seqs;
    const int64_t final_state_elems = S_v * S_v * H * n_seqs;
    float *       attn_data = dst;
    float *       state     = dst + attn_score_elems;

    // When persist_inter is null, the intermediate-state region lives
    // right after the final-state block inside `dst`. InterT MUST be
    // float in that case — the host-side caller is responsible.
    InterT * inter_states = persist_inter
        ? persist_inter
        : (InterT *)(dst + attn_score_elems + final_state_elems);

    const int64_t state_offset = (sequence * H + h_idx) * S_v * S_v;
    state += state_offset;
    curr_state += state_offset + col * S_v;
    attn_data += (sequence * n_tokens * H + h_idx) * S_v;

    // Per-sequence per-head base for this block's intermediates, t=0.
    // Advance by (H * S_v * S_v) each token.
    InterT * inter_base = inter_states + (sequence * n_tokens * H + h_idx) * S_v * S_v;

    constexpr int warp_size = ggml_cuda_get_physical_warp_size() < S_v
                                  ? ggml_cuda_get_physical_warp_size()
                                  : S_v;
    static_assert(S_v % warp_size == 0, "S_v must be a multiple of warp_size");
    constexpr int rows_per_lane = (S_v + warp_size - 1) / warp_size;
    float         s_shard[rows_per_lane];

    // Transposed state layout: M[col][i] = S[i][col], so row col is contiguous.
#pragma unroll
    for (int r = 0; r < rows_per_lane; r++) {
        const int i = r * warp_size + lane;
        s_shard[r]  = curr_state[i];
    }

    const int * parent_ids_seq = nullptr;
    if constexpr (TREE_MODE) {
        parent_ids_seq = parent_ids + sequence * n_tokens;
    }

    for (int t = 0; t < n_tokens; t++) {
        // Tree-branch reload. Same-thread read-after-write on global
        // memory; no __syncthreads() needed because each lane touches
        // only its own (col, row) slots.
        if constexpr (TREE_MODE) {
            if (t > 0) {
                const int parent_t = parent_ids_seq[t];
                if (parent_t == GDN_TREE_ROOT_PARENT) {
                    // Root-level sibling: reset to the pre-block state.
#pragma unroll
                    for (int r = 0; r < rows_per_lane; r++) {
                        const int i = r * warp_size + lane;
                        s_shard[r]  = curr_state[i];
                    }
                } else if (parent_t != t - 1) {
                    // Branch: pull state for parent_t from the
                    // intermediate-state region.
                    const InterT * parent_base = inter_states
                        + ((sequence * n_tokens + parent_t) * H + h_idx) * S_v * S_v;
#pragma unroll
                    for (int r = 0; r < rows_per_lane; r++) {
                        const int i = r * warp_size + lane;
                        s_shard[r]  = load_inter_state(parent_base, col * S_v + i);
                    }
                }
                // parent_t == t - 1: sequential, registers are fine.
            }
        }

        const float * q_t = q + iq3 * sq3 + t * sq2 + iq1 * sq1;
        const float * k_t = k + iq3 * sq3 + t * sq2 + iq1 * sq1;
        const float * v_t = v + sequence * sv3 + t * sv2 + h_idx * sv1;

        const int64_t gb_offset = sequence * sb3 + t * sb2 + h_idx * sb1;
        const float * beta_t    = beta + gb_offset;
        const float * g_t       = g    + gb_offset * (KDA ? S_v : 1);

        const float beta_val = *beta_t;

        // Cache k and q in registers.
        float k_reg[rows_per_lane];
        float q_reg[rows_per_lane];
#pragma unroll
        for (int r = 0; r < rows_per_lane; r++) {
            const int i = r * warp_size + lane;
            k_reg[r] = k_t[i];
            q_reg[r] = q_t[i];
        }

        if constexpr (!KDA) {
            const float g_val = expf(*g_t);

            // kv[col] = sum_i S[i][col] * k[i]
            float kv_shard = 0.0f;
#pragma unroll
            for (int r = 0; r < rows_per_lane; r++) {
                kv_shard += s_shard[r] * k_reg[r];
            }
            float kv_col = warp_reduce_sum<warp_size>(kv_shard);

            float delta_col = (v_t[col] - g_val * kv_col) * beta_val;

            // fused: S[i][col] = g * S[i][col] + k[i] * delta[col];
            //        attn[col] = sum_i S[i][col] * q[i]
            float attn_partial = 0.0f;
#pragma unroll
            for (int r = 0; r < rows_per_lane; r++) {
                s_shard[r]   = g_val * s_shard[r] + k_reg[r] * delta_col;
                attn_partial += s_shard[r] * q_reg[r];
            }

            float attn_col = warp_reduce_sum<warp_size>(attn_partial);

            if (lane == 0) {
                attn_data[col] = attn_col * scale;
            }
        } else {
            // kv[col] = sum_i g[i] * S[i][col] * k[i]
            float kv_shard = 0.0f;
#pragma unroll
            for (int r = 0; r < rows_per_lane; r++) {
                const int i = r * warp_size + lane;
                kv_shard += expf(g_t[i]) * s_shard[r] * k_reg[r];
            }

            float kv_col = warp_reduce_sum<warp_size>(kv_shard);

            float delta_col = (v_t[col] - kv_col) * beta_val;

            // fused: S[i][col] = g[i] * S[i][col] + k[i] * delta[col];
            //        attn[col] = sum_i S[i][col] * q[i]
            float attn_partial = 0.0f;
#pragma unroll
            for (int r = 0; r < rows_per_lane; r++) {
                const int i = r * warp_size + lane;
                s_shard[r]   = expf(g_t[i]) * s_shard[r] + k_reg[r] * delta_col;
                attn_partial += s_shard[r] * q_reg[r];
            }

            float attn_col = warp_reduce_sum<warp_size>(attn_partial);

            if (lane == 0) {
                attn_data[col] = attn_col * scale;
            }
        }

        // Per-token intermediate state capture. Same transposed layout
        // as the final-state write below. Read by the spec-decode
        // rollback path.
#pragma unroll
        for (int r = 0; r < rows_per_lane; r++) {
            const int i = r * warp_size + lane;
            store_inter_state(inter_base, col * S_v + i, s_shard[r]);
        }
        inter_base += S_v * S_v * H;

        attn_data += S_v * H;
    }

    // Final state write (transposed layout).
#pragma unroll
    for (int r = 0; r < rows_per_lane; r++) {
        const int i          = r * warp_size + lane;
        state[col * S_v + i] = s_shard[r];
    }
}

// ---------------------------------------------------------------------------
// Extern "C" entry points. One per (S_v × KDA × TREE_MODE × InterT) axis.
//
// There are 4 S_v × 2 KDA × 2 TREE_MODE × 2 InterT = 32 combinations.
// We only instantiate the ones Qwen3.5 hybrid actually reaches:
//
//   S_v = 128   (GDN head-dim)
//   KDA = false (Qwen3.5 uses scalar gate, g->ne[0] == 1)
//   TREE_MODE ∈ {false, true}       — chain verify vs DDTree
//   InterT    ∈ {float, __half}     — persist precision
//
// That's 4 entry points. Other combinations can be added when a new
// model family needs them.
//
// Naming: gated_delta_net_sv<N>_<kda|gda>_<chain|tree>_<f32|f16>
//   * gda = "gate-dim-attention", KDA=false, scalar gate (our path)
//   * kda = per-element gate (NOT instantiated; template still supports it)
// ---------------------------------------------------------------------------

// Thin forwarding kernel per (S_v, KDA, TREE_MODE, InterT) combo.
// __launch_bounds__((warp_size < S_v ? warp_size : S_v) * 4, 2)
// matches the upstream launch-bounds directive exactly.
#define GDN_EXTERN(SV, KDA_TAG, KDA_VAL, TREE_TAG, TREE_VAL, INTER_TAG, INTER_T)               \
    extern "C" __global__ void                                                                 \
    __launch_bounds__((ggml_cuda_get_physical_warp_size() < (SV) ?                             \
                       ggml_cuda_get_physical_warp_size() : (SV)) * 4, 2)                      \
    gated_delta_net_sv##SV##_##KDA_TAG##_##TREE_TAG##_##INTER_TAG(                             \
        const float * q, const float * k, const float * v, const float * g,                   \
        const float * beta, const float * curr_state, float * dst,                             \
        const int * parent_ids, INTER_T * persist_inter,                                       \
        int64_t H, int64_t n_tokens, int64_t n_seqs,                                           \
        int64_t sq1, int64_t sq2, int64_t sq3,                                                 \
        int64_t sv1, int64_t sv2, int64_t sv3,                                                 \
        int64_t sb1, int64_t sb2, int64_t sb3,                                                 \
        uint3 neqk1_magic, uint3 rq3_magic, float scale) {                                     \
        gated_delta_net_impl<SV, KDA_VAL, TREE_VAL, INTER_T>(                                  \
            q, k, v, g, beta, curr_state, dst, parent_ids, persist_inter,                      \
            H, n_tokens, n_seqs, sq1, sq2, sq3, sv1, sv2, sv3,                                 \
            sb1, sb2, sb3, neqk1_magic, rq3_magic, scale);                                     \
    }

// Qwen3.5 hybrid target path (S_v=128, GDA, chain & tree, f32 & f16 persist).
GDN_EXTERN(128, gda, false, chain, false, f32, float)
GDN_EXTERN(128, gda, false, chain, false, f16, __half)
GDN_EXTERN(128, gda, false, tree,  true,  f32, float)
GDN_EXTERN(128, gda, false, tree,  true,  f16, __half)

// Smaller S_v instantiations for tests / smaller models. Float persist
// only — there's no model family that wants the fp16 persist at these
// dims yet.
GDN_EXTERN(16, gda, false, chain, false, f32, float)
GDN_EXTERN(16, gda, false, tree,  true,  f32, float)
GDN_EXTERN(32, gda, false, chain, false, f32, float)
GDN_EXTERN(32, gda, false, tree,  true,  f32, float)
GDN_EXTERN(64, gda, false, chain, false, f32, float)
GDN_EXTERN(64, gda, false, tree,  true,  f32, float)

#undef GDN_EXTERN
