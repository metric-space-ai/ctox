// ref: vendor/metal/dflash-mlx-ref/dflash_mlx/kernels.py:442-572
//
// Two-pass scaled-dot-product attention — PASS 1 (partials).
// Splits the KV axis into `blocks` chunks and returns per-chunk
// {partial output, sum of exp-scores, max score}, which the reduce
// kernel (`sdpa_2pass_reduce.metal`) combines into the final softmax-
// weighted output. Used on long-context verify where stock MLX SDPA
// starts diverging numerically.
//
// Build variants (select via `-D`):
//
//   DFLASH_HAS_MASK       accept an additive / finite-min mask buffer
//   INPUT_DTYPE_{BF16,F16}   pick InT in common.h

#include "common.h"

#if defined(DFLASH_HAS_MASK)
  #define KERNEL_NAME batched_sdpa_2pass_partials_mask
#else
  #define KERNEL_NAME batched_sdpa_2pass_partials
#endif

kernel void KERNEL_NAME(
    device const InT*   queries                             [[buffer(0)]],
    device const InT*   keys                                [[buffer(1)]],
    device const InT*   values                              [[buffer(2)]],
    constant  int&      gqa_factor                          [[buffer(3)]],
    constant  int&      N                                   [[buffer(4)]],
    constant  int&      k_head_stride                       [[buffer(5)]],
    constant  int&      k_seq_stride                        [[buffer(6)]],
    constant  int&      v_head_stride                       [[buffer(7)]],
    constant  int&      v_seq_stride                        [[buffer(8)]],
    constant  float&    scale                               [[buffer(9)]],
    constant  int&      blocks                              [[buffer(10)]],
#if defined(DFLASH_HAS_MASK)
    device const InT*   mask                                [[buffer(11)]],
#endif
    device       InT*   partials                            [[buffer(12)]],
    device       float* sums                                [[buffer(13)]],
    device       float* maxs                                [[buffer(14)]],
    uint3  threadgroup_position_in_grid   [[threadgroup_position_in_grid]],
    uint3  threadgroups_per_grid          [[threadgroups_per_grid]],
    uint3  thread_position_in_threadgroup [[thread_position_in_threadgroup]],
    uint   thread_index_in_simdgroup      [[thread_index_in_simdgroup]]
) {
    constexpr int BD            = 32;
    const     int qk_per_thread = D / BD;
    const     int v_per_thread  = V / BD;

    auto q_head_idx = threadgroup_position_in_grid.x;
    auto b_idx      = threadgroup_position_in_grid.y;
    auto block_idx  = threadgroup_position_in_grid.z;
    auto q_seq_idx  = thread_position_in_threadgroup.z;
    auto simd_lid   = thread_index_in_simdgroup;

    auto Hq                = threadgroups_per_grid.x;
    auto hk_idx            = q_head_idx / gqa_factor;
    auto q_batch_head_idx  = b_idx * Hq + q_head_idx;
    auto o_offset          = q_batch_head_idx * M_FIXED + q_seq_idx;

    auto q_ = queries + (o_offset * D)          + simd_lid * qk_per_thread;
    auto k_ = keys    + ((b_idx * Hk + hk_idx) * k_head_stride)
                      + block_idx * k_seq_stride
                      + simd_lid  * qk_per_thread;
    auto v_ = values  + ((b_idx * Hk + hk_idx) * v_head_stride)
                      + block_idx * v_seq_stride
                      + simd_lid  * v_per_thread;

    partials += (o_offset * blocks + block_idx) * V + simd_lid * v_per_thread;
    sums     += o_offset * blocks + block_idx;
    maxs     += o_offset * blocks + block_idx;

#if defined(DFLASH_HAS_MASK)
    auto mask_ = mask + (((b_idx * Hq + q_head_idx) * M_FIXED + q_seq_idx) * N + block_idx);
#endif

    // Per-thread register tiles. BD = simdgroup width = 32.
    // qk_per_thread / v_per_thread are bounded by D/V up to 8 in the
    // supported shapes (D,V in {128, 256}, BD=32 → 4 or 8).
    thread float q[8];
    thread float o[8];
    threadgroup InT tg_k[BD * 8];
    threadgroup InT tg_v[BD * 8];

    for (int i = 0; i < qk_per_thread; ++i) {
        q[i] = scale * static_cast<float>(q_[i]);
    }
    for (int i = 0; i < v_per_thread; ++i) {
        o[i] = 0.0f;
    }

    float max_score     = Limits<float>::finite_min();
    float sum_exp_score = 0.0f;

    for (int n = block_idx; n < N; n += blocks) {
        if (q_seq_idx == 0) {
            for (int i = 0; i < qk_per_thread; ++i) {
                tg_k[simd_lid * qk_per_thread + i] = k_[i];
            }
            for (int i = 0; i < v_per_thread; ++i) {
                tg_v[simd_lid * v_per_thread + i] = v_[i];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        bool use_key = (n <= (N - M_FIXED + int(q_seq_idx)));
#if defined(DFLASH_HAS_MASK)
        {
            float mask_value = static_cast<float>(mask_[0]);
            use_key = use_key && (mask_value >= float(Limits<InT>::finite_min()));
        }
#endif

        if (use_key) {
            float score = 0.0f;
            for (int i = 0; i < qk_per_thread; ++i) {
                score += q[i] * static_cast<float>(tg_k[simd_lid * qk_per_thread + i]);
            }
            score = simd_sum(score);

#if defined(DFLASH_HAS_MASK)
            score += static_cast<float>(mask_[0]);
#endif

            float new_max   = metal::max(max_score, score);
            float factor    = fast::exp(max_score - new_max);
            float exp_score = fast::exp(score   - new_max);

            max_score     = new_max;
            sum_exp_score = sum_exp_score * factor + exp_score;
            for (int i = 0; i < v_per_thread; ++i) {
                o[i] = o[i] * factor + exp_score * static_cast<float>(tg_v[simd_lid * v_per_thread + i]);
            }
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
        k_ += blocks * int(k_seq_stride);
        v_ += blocks * int(v_seq_stride);
#if defined(DFLASH_HAS_MASK)
        mask_ += blocks;
#endif
    }

    if (simd_lid == 0) {
        sums[0] = sum_exp_score;
        maxs[0] = max_score;
    }
    for (int i = 0; i < v_per_thread; ++i) {
        partials[i] = static_cast<InT>(o[i]);
    }
}
