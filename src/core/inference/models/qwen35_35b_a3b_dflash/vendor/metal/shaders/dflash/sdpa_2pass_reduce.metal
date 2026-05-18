// ref: vendor/metal/dflash-mlx-ref/dflash_mlx/kernels.py:575-645
//
// Two-pass scaled-dot-product attention — PASS 2 (reduce).
// Combines the per-block `{partials, sums, maxs}` from
// `sdpa_2pass_partials.metal` into the final softmax-weighted output.
// Numerically-stable log-sum-exp merge.
//
// Build variant: INPUT_DTYPE_{BF16,F16} picks InT.

#include "common.h"

kernel void batched_sdpa_2pass_reduce(
    device const InT*   partials                            [[buffer(0)]],
    device const float* sums                                [[buffer(1)]],
    device const float* maxs                                [[buffer(2)]],
    constant  int&      blocks                              [[buffer(3)]],
    device       InT*   out                                 [[buffer(4)]],
    uint3  threadgroup_position_in_grid   [[threadgroup_position_in_grid]],
    uint   simdgroup_index_in_threadgroup [[simdgroup_index_in_threadgroup]],
    uint   thread_index_in_simdgroup      [[thread_index_in_simdgroup]]
) {
    constexpr int BN              = 32;
    constexpr int BD              = 32;
    const     int elem_per_thread = V / BD;

    auto head_idx  = threadgroup_position_in_grid.x;
    auto q_seq_idx = threadgroup_position_in_grid.y;
    auto simd_gid  = simdgroup_index_in_threadgroup;
    auto simd_lid  = thread_index_in_simdgroup;

    auto q_offset = head_idx * M_FIXED + q_seq_idx;
    partials += (q_offset * blocks + simd_gid) * V + simd_lid * elem_per_thread;
    sums     += q_offset * blocks;
    maxs     += q_offset * blocks;
    out      += q_offset * V + simd_gid * elem_per_thread;

    thread float o[8];   // bound: V/BD ≤ 8 for V ∈ {128, 256}
    threadgroup float outputs[BN * BD];

    for (int i = 0; i < elem_per_thread; ++i) {
        o[i] = 0.0f;
    }

    float sum_exp_score = 0.0f;
    float max_score     = Limits<float>::finite_min();

    for (int b = 0; b < blocks / BN; ++b) {
        max_score = metal::max(max_score, maxs[simd_lid + BN * b]);
    }
    max_score = simd_max(max_score);

    for (int b = 0; b < blocks / BN; ++b) {
        float factor = fast::exp(maxs[simd_lid + BN * b] - max_score);
        sum_exp_score += factor * sums[simd_lid + BN * b];
    }
    sum_exp_score = simd_sum(sum_exp_score);

    for (int b = 0; b < blocks / BN; ++b) {
        float factor = fast::exp(maxs[simd_gid] - max_score);
        for (int i = 0; i < elem_per_thread; ++i) {
            o[i] += factor * static_cast<float>(partials[i]);
        }
        maxs     += BN;
        partials += BN * V;
    }

    for (int i = 0; i < elem_per_thread; ++i) {
        outputs[simd_lid * BD + simd_gid] = o[i];
        threadgroup_barrier(mem_flags::mem_threadgroup);
        o[i] = simd_sum(outputs[simd_gid * BD + simd_lid]);
        o[i] = sum_exp_score == 0.0f ? o[i] : (o[i] / sum_exp_score);
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (simd_lid == 0) {
        for (int i = 0; i < elem_per_thread; ++i) {
            out[i] = static_cast<InT>(o[i]);
        }
    }
}
