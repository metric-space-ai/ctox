#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_prefill_rms_matmul_rowtiles_tok2_fp16_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* w_tiled [[buffer(2)]],
    device float* y_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    constant uint& rows [[buffer(5)]],
    constant uint& row_tile [[buffer(6)]],
    constant uint& col_tile [[buffer(7)]],
    constant uint& n_col_tiles [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint token_tile = 2;
    constexpr uint cols = 1024;

    threadgroup float norm_partial[token_tile * 256];
    threadgroup float partial[token_tile * 8 * 256];

    const uint tid = tid_pos.x;
    const uint row_tile_group = tg_pos.x;
    const uint token_base = tg_pos.y * token_tile;
    const uint row_base = row_tile_group * row_tile;

    float ss[token_tile];
    for (uint t = 0; t < token_tile; ++t) {
        ss[t] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                const float v = float(x_tokens[token * cols + col]);
                ss[t] += v * v;
            }
        }
    }
    for (uint t = 0; t < token_tile; ++t) {
        norm_partial[t * 256 + tid] = ss[t];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            for (uint t = 0; t < token_tile; ++t) {
                norm_partial[t * 256 + tid] += norm_partial[t * 256 + tid + stride];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    float inv_rms[token_tile];
    for (uint t = 0; t < token_tile; ++t) {
        inv_rms[t] = rsqrt(norm_partial[t * 256] / 1024.0f + 1.0e-6f);
    }

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
            if (row_base + lane < rows) {
                w_lane[lane] = float(w_tiled[packed_base + lane * col_tile]);
            }
        }

        const float nw = float(norm_weight[col]);
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                const float xv = float(x_tokens[token * cols + col]) * inv_rms[t] * nw;
                for (uint lane = 0; lane < 8; ++lane) {
                    acc[t][lane] += w_lane[lane] * xv;
                }
            }
        }
    }

    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < 8; ++lane) {
            partial[(t * 8 + lane) * 256 + tid] = acc[t][lane];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            for (uint t = 0; t < token_tile; ++t) {
                for (uint lane = 0; lane < 8; ++lane) {
                    partial[(t * 8 + lane) * 256 + tid] +=
                        partial[(t * 8 + lane) * 256 + tid + stride];
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                for (uint lane = 0; lane < 8; ++lane) {
                    const uint row = row_base + lane;
                    if (row < rows) {
                        y_tokens[token * rows + row] = partial[(t * 8 + lane) * 256];
                    }
                }
            }
        }
    }
}
