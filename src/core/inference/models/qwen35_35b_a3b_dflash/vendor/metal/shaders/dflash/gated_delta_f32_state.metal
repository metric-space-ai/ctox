// Normal Qwen3.5 GatedDeltaNet kernel for target prefill/decode.
//
// This is intentionally separate from gated_delta_tape.metal. The regular
// MLX target path keeps the recurrent SSM state in float32 while activations
// remain bf16/f16. The DFlash tape kernel records rollback deltas and follows
// the speculative-cache precision path; this one matches mlx_lm's
// gated_delta_update/gated_delta_kernel state contract.

#include "common.h"

#if defined(DFLASH_HAS_MASK)
  #define KERNEL_NAME_SUFFIX _mask
#else
  #define KERNEL_NAME_SUFFIX
#endif

#if defined(DFLASH_GATE_VEC)
  #define KERNEL_NAME_VECSUFFIX _vec
#else
  #define KERNEL_NAME_VECSUFFIX
#endif

#define _CONCAT3(a, b, c) a##b##c
#define CONCAT3(a, b, c)  _CONCAT3(a, b, c)
#define KERNEL_NAME       CONCAT3(gated_delta_f32_state, KERNEL_NAME_VECSUFFIX, KERNEL_NAME_SUFFIX)

kernel void KERNEL_NAME(
    device const InT*   q                                   [[buffer(0)]],
    device const InT*   k                                   [[buffer(1)]],
    device const InT*   v                                   [[buffer(2)]],
    device const InT*   g                                   [[buffer(3)]],
    device const InT*   beta                                [[buffer(4)]],
    device const float* state_in                            [[buffer(5)]],
    constant  int&      T                                   [[buffer(6)]],
#if defined(DFLASH_HAS_MASK)
    device const bool*  mask                                [[buffer(7)]],
#endif
    device       InT*   y                                   [[buffer(8)]],
    device       float* state_out                           [[buffer(9)]],
    uint3  thread_position_in_grid        [[thread_position_in_grid]],
    uint3  thread_position_in_threadgroup [[thread_position_in_threadgroup]],
    uint   thread_index_in_simdgroup      [[thread_index_in_simdgroup]]
) {
    auto n        = thread_position_in_grid.z;
    auto b_idx    = n / Hv;
    auto hv_idx   = n % Hv;
    auto hk_idx   = hv_idx / (Hv / Hk);
    constexpr int n_per_t = 8;

    auto q_ = q + b_idx * T * Hk * Dk + hk_idx * Dk;
    auto k_ = k + b_idx * T * Hk * Dk + hk_idx * Dk;

    auto v_ = v + b_idx * T * Hv * Dv + hv_idx * Dv;
    y      += b_idx * T * Hv * Dv + hv_idx * Dv;

    auto dk_idx = thread_position_in_threadgroup.x;
    auto dv_idx = thread_position_in_grid.y;

    auto i_state = state_in  + (n * Dv + dv_idx) * Dk;
    auto o_state = state_out + (n * Dv + dv_idx) * Dk;

    const int slots = Dk / 32;
    thread float state[n_per_t];
    for (int i = 0; i < slots; ++i) {
        auto s_idx = slots * dk_idx + i;
        state[i] = i_state[s_idx];
    }

#if defined(DFLASH_GATE_VEC)
    auto g_ = g + (b_idx * T * Hv + hv_idx) * Dk;
#else
    auto g_ = g + b_idx * T * Hv;
#endif
    auto beta_ = beta + b_idx * T * Hv;

    for (int t = 0; t < T; ++t) {
#if defined(DFLASH_HAS_MASK)
        const bool step_active = mask[b_idx * T + t];
#else
        const bool step_active = true;
#endif
        if (step_active) {
            float kv_mem = 0.0f;
            for (int i = 0; i < slots; ++i) {
                auto s_idx = slots * dk_idx + i;
#if defined(DFLASH_GATE_VEC)
                state[i] *= static_cast<float>(g_[s_idx]);
#else
                state[i] *= static_cast<float>(g_[hv_idx]);
#endif
                kv_mem += state[i] * static_cast<float>(k_[s_idx]);
            }
            kv_mem = simd_sum(kv_mem);

            const float delta =
                (static_cast<float>(v_[dv_idx]) - kv_mem) * static_cast<float>(beta_[hv_idx]);

            float out = 0.0f;
            for (int i = 0; i < slots; ++i) {
                auto s_idx = slots * dk_idx + i;
                state[i] += static_cast<float>(k_[s_idx]) * delta;
                out += state[i] * static_cast<float>(q_[s_idx]);
            }
            out = simd_sum(out);
            if (thread_index_in_simdgroup == 0) {
                y[dv_idx] = static_cast<InT>(out);
            }
        }
        q_ += Hk * Dk;
        k_ += Hk * Dk;
        v_ += Hv * Dv;
        y  += Hv * Dv;
#if defined(DFLASH_GATE_VEC)
        g_ += Hv * Dk;
#else
        g_ += Hv;
#endif
        beta_ += Hv;
    }

    for (int i = 0; i < slots; ++i) {
        auto s_idx = slots * dk_idx + i;
        o_state[s_idx] = state[i];
    }
}
