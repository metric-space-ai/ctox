#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_lm_head_score_pairs_fp16_tiled_k1024(
    device const half* x [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* scores [[buffer(2)]],
    device uint* ids [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint row [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    if (row >= rows) {
        return;
    }

    threadgroup float partial[256];
    const uint cols = 1024;
    const uint row_tile_idx = row / row_tile;
    const uint row_lane = row - row_tile_idx * row_tile;
    float acc = 0.0f;

    for (uint col = tid; col < cols; col += 256) {
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_idx =
            ((row_tile_idx * n_col_tiles + col_tile_idx) * row_tile + row_lane) *
                col_tile +
            col_lane;
        acc += float(w_tiled[packed_idx]) * float(x[col]);
    }

    partial[tid] = acc;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        scores[row] = partial[0];
        ids[row] = row;
    }
}

kernel void qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024(
    device const half* x [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* scores [[buffer(2)]],
    device uint* ids [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[8 * 256];
    const uint cols = 1024;
    const uint row_base = row_tile_group * row_tile;
    float acc[8];

    for (uint lane = 0; lane < 8; ++lane) {
        acc[lane] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        const float xv = float(x[col]);
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_tile_group * n_col_tiles + col_tile_idx) * row_tile) *
                col_tile +
            col_lane;

        for (uint lane = 0; lane < 8; ++lane) {
            if (row_base + lane < rows) {
                acc[lane] += float(w_tiled[packed_base + lane * col_tile]) * xv;
            }
        }
    }

    for (uint lane = 0; lane < 8; ++lane) {
        partial[lane * 256 + tid] = acc[lane];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            for (uint lane = 0; lane < 8; ++lane) {
                partial[lane * 256 + tid] += partial[lane * 256 + tid + stride];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        for (uint lane = 0; lane < 8; ++lane) {
            const uint row = row_base + lane;
            if (row < rows) {
                scores[row] = partial[lane * 256];
                ids[row] = row;
            }
        }
    }
}

kernel void qwen35_08b_lm_head_score_rowtiles_f32_tiled_k1024(
    device const float* x [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* scores [[buffer(2)]],
    device uint* ids [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[8 * 256];
    const uint cols = 1024;
    const uint row_base = row_tile_group * row_tile;
    float acc[8];

    for (uint lane = 0; lane < 8; ++lane) {
        acc[lane] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        const float xv = x[col];
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_tile_group * n_col_tiles + col_tile_idx) * row_tile) *
                col_tile +
            col_lane;

        for (uint lane = 0; lane < 8; ++lane) {
            if (row_base + lane < rows) {
                acc[lane] += float(w_tiled[packed_base + lane * col_tile]) * xv;
            }
        }
    }

    for (uint lane = 0; lane < 8; ++lane) {
        partial[lane * 256 + tid] = acc[lane];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            for (uint lane = 0; lane < 8; ++lane) {
                partial[lane * 256 + tid] += partial[lane * 256 + tid + stride];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        for (uint lane = 0; lane < 8; ++lane) {
            const uint row = row_base + lane;
            if (row < rows) {
                scores[row] = partial[lane * 256];
                ids[row] = row;
            }
        }
    }
}

kernel void qwen35_08b_lm_head_argmax_rowtiles_f32_tiled_k1024(
    device const float* x [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* scores [[buffer(2)]],
    device uint* ids [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[8 * 256];
    const uint cols = 1024;
    const uint row_base = row_tile_group * row_tile;
    float acc[8];

    for (uint lane = 0; lane < 8; ++lane) {
        acc[lane] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        const float xv = x[col];
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_tile_group * n_col_tiles + col_tile_idx) * row_tile) *
                col_tile +
            col_lane;

        for (uint lane = 0; lane < 8; ++lane) {
            if (row_base + lane < rows) {
                acc[lane] += float(w_tiled[packed_base + lane * col_tile]) * xv;
            }
        }
    }

    for (uint lane = 0; lane < 8; ++lane) {
        partial[lane * 256 + tid] = acc[lane];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            for (uint lane = 0; lane < 8; ++lane) {
                partial[lane * 256 + tid] += partial[lane * 256 + tid + stride];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        float best_score = -3.402823466e+38f;
        uint best_id = 0;
        for (uint lane = 0; lane < 8; ++lane) {
            const uint row = row_base + lane;
            if (row < rows) {
                const float score = partial[lane * 256];
                const bool take = (score > best_score) ||
                    (score == best_score && row < best_id);
                if (take) {
                    best_score = score;
                    best_id = row;
                }
            }
        }
        scores[row_tile_group] = best_score;
        ids[row_tile_group] = best_id;
    }
}

kernel void qwen35_08b_lm_head_argmax_rowtiles_simd32_f32_tiled_k1024(
    device const float* x [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* scores [[buffer(2)]],
    device uint* ids [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint simdgroups_per_tg = 8;
    threadgroup float partial[8 * simdgroups_per_tg];
    const uint cols = 1024;
    const uint row_base = row_tile_group * row_tile;
    float acc[8];

    for (uint lane = 0; lane < 8; ++lane) {
        acc[lane] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        const float xv = x[col];
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_tile_group * n_col_tiles + col_tile_idx) * row_tile) *
                col_tile +
            col_lane;

        for (uint lane = 0; lane < 8; ++lane) {
            if (row_base + lane < rows) {
                acc[lane] += float(w_tiled[packed_base + lane * col_tile]) * xv;
            }
        }
    }

    for (uint lane = 0; lane < 8; ++lane) {
        const float sum = simd_sum(acc[lane]);
        if (simd_lane == 0) {
            partial[lane * simdgroups_per_tg + simd_group] = sum;
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (tid == 0) {
        float best_score = -3.402823466e+38f;
        uint best_id = 0;
        for (uint lane = 0; lane < 8; ++lane) {
            const uint row = row_base + lane;
            if (row < rows) {
                float score = 0.0f;
                for (uint group = 0; group < simdgroups_per_tg; ++group) {
                    score += partial[lane * simdgroups_per_tg + group];
                }
                const bool take = (score > best_score) ||
                    (score == best_score && row < best_id);
                if (take) {
                    best_score = score;
                    best_id = row;
                }
            }
        }
        scores[row_tile_group] = best_score;
        ids[row_tile_group] = best_id;
    }
}
