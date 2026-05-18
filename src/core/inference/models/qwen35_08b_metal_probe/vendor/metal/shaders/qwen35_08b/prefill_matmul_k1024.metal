#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_prefill_matmul_rowtiles_tok4_simd_fp16_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint token_tile = 4;
    constexpr uint cols = 1024;
    constexpr uint simdgroups_per_tg = 8;

    threadgroup float partial[token_tile * 8 * simdgroups_per_tg];

    const uint tid = tid_pos.x;
    const uint row_tile_group = tg_pos.x;
    const uint token_base = tg_pos.y * token_tile;
    const uint row_base = row_tile_group * row_tile;

    float acc[token_tile][8];
    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < 8; ++lane) {
            acc[t][lane] = 0.0f;
        }
    }

    for (uint col = tid; col < cols; col += 256) {
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_tile_group * n_col_tiles + col_tile_idx) * row_tile) *
                col_tile +
            col_lane;

        float w_lane[8];
        for (uint lane = 0; lane < 8; ++lane) {
            w_lane[lane] = 0.0f;
            if (lane < row_tile && row_base + lane < rows) {
                w_lane[lane] = float(w_tiled[packed_base + lane * col_tile]);
            }
        }

        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                const float xv = float(x_tokens[token * cols + col]);
                for (uint lane = 0; lane < 8; ++lane) {
                    acc[t][lane] += w_lane[lane] * xv;
                }
            }
        }
    }

    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < 8; ++lane) {
            const float sum = simd_sum(acc[t][lane]);
            if (simd_lane == 0) {
                partial[(t * 8 + lane) * simdgroups_per_tg + simd_group] = sum;
            }
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (tid < simdgroups_per_tg) {
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                for (uint lane = 0; lane < 8; ++lane) {
                    const uint row = row_base + lane;
                    if (lane < row_tile && row < rows) {
                        float total = partial[(t * 8 + lane) * simdgroups_per_tg + tid];
                        total = simd_sum(total);
                        if (simd_lane == 0) {
                            y_tokens[token * rows + row] = total;
                        }
                    }
                }
            }
        }
    }
}
