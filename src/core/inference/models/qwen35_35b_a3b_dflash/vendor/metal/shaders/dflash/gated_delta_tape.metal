// ref: vendor/metal/dflash-mlx-ref/dflash_mlx/kernels.py:12-122
//
// Gated-DeltaNet "with tape" kernel — forward pass that also records the
// per-step innovation delta into `innovation_tape` so a later
// `tape_replay` call can restore state without a full re-forward.
//
// Four build variants, selected by `-D` flags at `xcrun metal -c` time:
//
//   DFLASH_HAS_MASK       enable the per-batch mask gate
//   DFLASH_GATE_VEC       vector gating (g is [B,T,Hv,Dk]); else scalar
//                         gating (g is [B,T,Hv])
//   INPUT_DTYPE_{BF16,F16}   pick the InT type in common.h
//
// The Rust side builds four pipelines from this single source by
// varying DFLASH_HAS_MASK / DFLASH_GATE_VEC. Dk/Dv/Hk/Hv are
// function-constants (id 0..3) specialised at pipeline-create time.

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

// Metal token-pasting to build the final kernel name.
#define _CONCAT3(a, b, c) a##b##c
#define CONCAT3(a, b, c)  _CONCAT3(a, b, c)
#define KERNEL_NAME       CONCAT3(gated_delta_tape, KERNEL_NAME_VECSUFFIX, KERNEL_NAME_SUFFIX)

kernel void KERNEL_NAME(
    device const InT*   q                                   [[buffer(0)]],
    device const InT*   k                                   [[buffer(1)]],
    device const InT*   v                                   [[buffer(2)]],
    device const InT*   g                                   [[buffer(3)]],
    device const InT*   beta                                [[buffer(4)]],
    device const InT*   state_in                            [[buffer(5)]],
    constant  int&      T                                   [[buffer(6)]],
#if defined(DFLASH_HAS_MASK)
    device const bool*  mask                                [[buffer(7)]],
#endif
    device       InT*   y                                   [[buffer(8)]],
    device       InT*   state_out                           [[buffer(9)]],
    device       float* innovation_tape                     [[buffer(10)]],
    uint3  thread_position_in_grid        [[thread_position_in_grid]],
    uint3  thread_position_in_threadgroup [[thread_position_in_threadgroup]],
    uint   thread_index_in_simdgroup      [[thread_index_in_simdgroup]]
) {
    auto n        = thread_position_in_grid.z;
    auto b_idx    = n / Hv;
    auto hv_idx   = n % Hv;
    auto hk_idx   = hv_idx / (Hv / Hk);
    constexpr int n_per_t = 8;  // upper bound: Dk=256 → 8; Dk=128 → 4; actual use = Dk/32

    // q, k: [B, T, Hk, Dk]
    auto q_ = q + b_idx * T * Hk * Dk + hk_idx * Dk;
    auto k_ = k + b_idx * T * Hk * Dk + hk_idx * Dk;

    // v, y, tape: [B, T, Hv, Dv]
    auto v_    = v + b_idx * T * Hv * Dv + hv_idx * Dv;
    y         += b_idx * T * Hv * Dv + hv_idx * Dv;
    auto tape_ = innovation_tape + b_idx * T * Hv * Dv + hv_idx * Dv;

    auto dk_idx = thread_position_in_threadgroup.x;
    auto dv_idx = thread_position_in_grid.y;

    // state_in, state_out: [B, Hv, Dv, Dk]
    auto i_state = state_in  + (n * Dv + dv_idx) * Dk;
    auto o_state = state_out + (n * Dv + dv_idx) * Dk;

    // Only the first Dk/32 slots are used; the rest stay unused.
    const int slots = Dk / 32;
    thread float state[n_per_t];
    for (int i = 0; i < slots; ++i) {
        auto s_idx = (Dk / 32) * dk_idx + i;
        state[i]   = static_cast<float>(i_state[s_idx]);
    }

#if defined(DFLASH_GATE_VEC)
    // g: [B, T, Hv, Dk]
    auto g_ = g + (b_idx * T * Hv + hv_idx) * Dk;
#else
    // g: [B, T, Hv]
    auto g_ = g + b_idx * T * Hv;
#endif
    auto beta_ = beta + b_idx * T * Hv;

    for (int t = 0; t < T; ++t) {
        float delta = 0.0f;
#if defined(DFLASH_HAS_MASK)
        const bool step_active = mask[b_idx * T + t];
#else
        const bool step_active = true;
#endif
        if (step_active) {
            float kv_mem = 0.0f;
            for (int i = 0; i < slots; ++i) {
                auto s_idx = (Dk / 32) * dk_idx + i;
#if defined(DFLASH_GATE_VEC)
                state[i] = state[i] * static_cast<float>(g_[s_idx]);
#else
                state[i] = state[i] * static_cast<float>(g_[hv_idx]);
#endif
                kv_mem += state[i] * static_cast<float>(k_[s_idx]);
            }
            kv_mem = simd_sum(kv_mem);

            delta = (static_cast<float>(v_[dv_idx]) - kv_mem) * static_cast<float>(beta_[hv_idx]);

            float out = 0.0f;
            for (int i = 0; i < slots; ++i) {
                auto s_idx = (Dk / 32) * dk_idx + i;
                state[i] = state[i] + static_cast<float>(k_[s_idx]) * delta;
                out += state[i] * static_cast<float>(q_[s_idx]);
            }
            out = simd_sum(out);
            if (thread_index_in_simdgroup == 0) {
                y[dv_idx] = static_cast<InT>(out);
            }
        }
        if (thread_index_in_simdgroup == 0) {
            tape_[dv_idx] = delta;
        }
        // Round-trip through InT to match the reference's precision loss.
        for (int i = 0; i < slots; ++i) {
            state[i] = static_cast<float>(static_cast<InT>(state[i]));
        }
        q_    += Hk * Dk;
        k_    += Hk * Dk;
        v_    += Hv * Dv;
        y     += Hv * Dv;
        tape_ += Hv * Dv;
#if defined(DFLASH_GATE_VEC)
        g_    += Hv * Dk;
#else
        g_    += Hv;
#endif
        beta_ += Hv;
    }

    for (int i = 0; i < slots; ++i) {
        auto s_idx = (Dk / 32) * dk_idx + i;
        o_state[s_idx] = static_cast<InT>(state[i]);
    }
}
