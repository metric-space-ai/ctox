// ref: vendor/metal/dflash-mlx-ref/dflash_mlx/kernels.py:227-311
//
// Tape-replay kernel — given an innovation tape recorded by the
// `gated_delta_tape` forward, walk `T` steps forward from `state_in`
// and write the committed `state_out`. Skips the score/exp/sum path
// because `delta` is already known per step, so this is a cheap
// rollback re-forward.
//
// Build variants (select via `-D`):
//
//   DFLASH_HAS_MASK
//   DFLASH_GATE_VEC
//   INPUT_DTYPE_{BF16,F16}

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
#define KERNEL_NAME       CONCAT3(tape_replay, KERNEL_NAME_VECSUFFIX, KERNEL_NAME_SUFFIX)

kernel void KERNEL_NAME(
    device const float* tape                                [[buffer(0)]],
    device const InT*   k                                   [[buffer(1)]],
    device const InT*   g                                   [[buffer(2)]],
    device const InT*   state_in                            [[buffer(3)]],
    constant  int&      T                                   [[buffer(4)]],
#if defined(DFLASH_HAS_MASK)
    device const bool*  mask                                [[buffer(5)]],
#endif
    device       InT*   state_out                           [[buffer(6)]],
    uint3  thread_position_in_grid        [[thread_position_in_grid]],
    uint3  thread_position_in_threadgroup [[thread_position_in_threadgroup]],
    uint   thread_index_in_simdgroup      [[thread_index_in_simdgroup]]
) {
    auto n      = thread_position_in_grid.z;
    auto b_idx  = n / Hv;
    auto hv_idx = n % Hv;
    auto hk_idx = hv_idx / (Hv / Hk);
    constexpr int n_per_t = 8;  // upper bound: Dk=256 → 8

    // tape: [B, T, Hv, Dv]
    auto tape_ = tape + b_idx * T * Hv * Dv + hv_idx * Dv;
    // k:    [B, T, Hk, Dk]
    auto k_    = k    + b_idx * T * Hk * Dk + hk_idx * Dk;

    auto dk_idx = thread_position_in_threadgroup.x;
    auto dv_idx = thread_position_in_grid.y;

    auto i_state = state_in  + (n * Dv + dv_idx) * Dk;
    auto o_state = state_out + (n * Dv + dv_idx) * Dk;

    const int slots = Dk / 32;
    thread float state[n_per_t];
    for (int i = 0; i < slots; ++i) {
        auto s_idx = (Dk / 32) * dk_idx + i;
        state[i]   = static_cast<float>(i_state[s_idx]);
    }

#if defined(DFLASH_GATE_VEC)
    auto g_ = g + (b_idx * T * Hv + hv_idx) * Dk;
#else
    auto g_ = g + b_idx * T * Hv;
#endif

    for (int t = 0; t < T; ++t) {
#if defined(DFLASH_HAS_MASK)
        const bool step_active = mask[b_idx * T + t];
#else
        const bool step_active = true;
#endif
        if (step_active) {
            float delta = static_cast<float>(tape_[dv_idx]);
            for (int i = 0; i < slots; ++i) {
                auto s_idx = (Dk / 32) * dk_idx + i;
#if defined(DFLASH_GATE_VEC)
                state[i] = state[i] * static_cast<float>(g_[s_idx]);
#else
                state[i] = state[i] * static_cast<float>(g_[hv_idx]);
#endif
                state[i] = state[i] + static_cast<float>(k_[s_idx]) * delta;
            }
            // Round-trip precision (preserved from the reference).
            for (int i = 0; i < slots; ++i) {
                state[i] = static_cast<float>(static_cast<InT>(state[i]));
            }
        }
        tape_ += Hv * Dv;
        k_    += Hk * Dk;
#if defined(DFLASH_GATE_VEC)
        g_    += Hv * Dk;
#else
        g_    += Hv;
#endif
    }

    for (int i = 0; i < slots; ++i) {
        auto s_idx = (Dk / 32) * dk_idx + i;
        o_state[s_idx] = static_cast<InT>(state[i]);
    }
}
