#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok2_fp16_tiled_k1024_i3584(
    device const half* x_tokens [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* gate_tiled [[buffer(2)]],
    device const half* up_tiled [[buffer(3)]],
    device half* out_tokens [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& rows [[buffer(6)]],
    constant uint& row_tile [[buffer(7)]],
    constant uint& col_tile [[buffer(8)]],
    constant uint& n_col_tiles [[buffer(9)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint token_tile = 2;
    constexpr uint rows_per_tg = 4;
    constexpr uint cols = 1024;

    threadgroup float norm_partial[token_tile * 256];
    threadgroup float gate_partial[token_tile * rows_per_tg * 256];
    threadgroup float up_partial[token_tile * rows_per_tg * 256];

    const uint tid = tid_pos.x;
    const uint row_group = tg_pos.x;
    const uint token_base = tg_pos.y * token_tile;
    const uint row_base = row_group * rows_per_tg;

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

    float gate_acc[token_tile][rows_per_tg];
    float up_acc[token_tile][rows_per_tg];
    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            gate_acc[t][lane] = 0.0f;
            up_acc[t][lane] = 0.0f;
        }
    }

    for (uint col = tid; col < cols; col += 256) {
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;

        float gate_lane[rows_per_tg];
        float up_lane[rows_per_tg];
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            gate_lane[lane] = 0.0f;
            up_lane[lane] = 0.0f;
            const uint row = row_base + lane;
            if (row < rows) {
                const uint packed_row_tile_group = row / row_tile;
                const uint row_lane = row - packed_row_tile_group * row_tile;
                const uint idx =
                    ((packed_row_tile_group * n_col_tiles + col_tile_idx) * row_tile +
                        row_lane) *
                        col_tile +
                    col_lane;
                gate_lane[lane] = float(gate_tiled[idx]);
                up_lane[lane] = float(up_tiled[idx]);
            }
        }

        const float nw = float(norm_weight[col]);
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                const float xv = float(x_tokens[token * cols + col]) * inv_rms[t] * nw;
                for (uint lane = 0; lane < rows_per_tg; ++lane) {
                    gate_acc[t][lane] += gate_lane[lane] * xv;
                    up_acc[t][lane] += up_lane[lane] * xv;
                }
            }
        }
    }

    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            gate_partial[(t * rows_per_tg + lane) * 256 + tid] = gate_acc[t][lane];
            up_partial[(t * rows_per_tg + lane) * 256 + tid] = up_acc[t][lane];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            for (uint t = 0; t < token_tile; ++t) {
                for (uint lane = 0; lane < rows_per_tg; ++lane) {
                    gate_partial[(t * rows_per_tg + lane) * 256 + tid] +=
                        gate_partial[(t * rows_per_tg + lane) * 256 + tid + stride];
                    up_partial[(t * rows_per_tg + lane) * 256 + tid] +=
                        up_partial[(t * rows_per_tg + lane) * 256 + tid + stride];
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                for (uint lane = 0; lane < rows_per_tg; ++lane) {
                    const uint row = row_base + lane;
                    if (row < rows) {
                        const float g = gate_partial[(t * rows_per_tg + lane) * 256];
                        const float u = up_partial[(t * rows_per_tg + lane) * 256];
                        const float sig = 1.0f / (1.0f + exp(-g));
                        const float v = clamp(g * sig * u, -65504.0f, 65504.0f);
                        out_tokens[token * rows + row] = half(v);
                    }
                }
            }
        }
    }
}

kernel void qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok4_simd_fp16_tiled_k1024_i3584(
    device const half* x_tokens [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* gate_tiled [[buffer(2)]],
    device const half* up_tiled [[buffer(3)]],
    device half* out_tokens [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& rows [[buffer(6)]],
    constant uint& row_tile [[buffer(7)]],
    constant uint& col_tile [[buffer(8)]],
    constant uint& n_col_tiles [[buffer(9)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint token_tile = 4;
    constexpr uint rows_per_tg = 4;
    constexpr uint cols = 1024;
    constexpr uint simdgroups_per_tg = 8;

    threadgroup float norm_partial[token_tile * simdgroups_per_tg];
    threadgroup float inv_rms_shared[token_tile];
    threadgroup float gate_partial[token_tile * rows_per_tg * simdgroups_per_tg];
    threadgroup float up_partial[token_tile * rows_per_tg * simdgroups_per_tg];

    const uint tid = tid_pos.x;
    const uint row_group = tg_pos.x;
    const uint token_base = tg_pos.y * token_tile;
    const uint row_base = row_group * rows_per_tg;

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
        const float sum = simd_sum(ss[t]);
        if (simd_lane == 0) {
            norm_partial[t * simdgroups_per_tg + simd_group] = sum;
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (tid == 0) {
        for (uint t = 0; t < token_tile; ++t) {
            float total = 0.0f;
            for (uint group = 0; group < simdgroups_per_tg; ++group) {
                total += norm_partial[t * simdgroups_per_tg + group];
            }
            inv_rms_shared[t] = rsqrt(total / 1024.0f + 1.0e-6f);
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float gate_acc[token_tile][rows_per_tg];
    float up_acc[token_tile][rows_per_tg];
    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            gate_acc[t][lane] = 0.0f;
            up_acc[t][lane] = 0.0f;
        }
    }

    for (uint col = tid; col < cols; col += 256) {
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;

        float gate_lane[rows_per_tg];
        float up_lane[rows_per_tg];
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            gate_lane[lane] = 0.0f;
            up_lane[lane] = 0.0f;
            const uint row = row_base + lane;
            if (row < rows) {
                const uint packed_row_tile_group = row / row_tile;
                const uint row_lane = row - packed_row_tile_group * row_tile;
                const uint idx =
                    ((packed_row_tile_group * n_col_tiles + col_tile_idx) * row_tile +
                        row_lane) *
                        col_tile +
                    col_lane;
                gate_lane[lane] = float(gate_tiled[idx]);
                up_lane[lane] = float(up_tiled[idx]);
            }
        }

        const float nw = float(norm_weight[col]);
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                const float xv = float(x_tokens[token * cols + col]) * inv_rms_shared[t] * nw;
                for (uint lane = 0; lane < rows_per_tg; ++lane) {
                    gate_acc[t][lane] += gate_lane[lane] * xv;
                    up_acc[t][lane] += up_lane[lane] * xv;
                }
            }
        }
    }

    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            const float gate_sum = simd_sum(gate_acc[t][lane]);
            const float up_sum = simd_sum(up_acc[t][lane]);
            if (simd_lane == 0) {
                gate_partial[(t * rows_per_tg + lane) * simdgroups_per_tg + simd_group] =
                    gate_sum;
                up_partial[(t * rows_per_tg + lane) * simdgroups_per_tg + simd_group] = up_sum;
            }
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (tid < simdgroups_per_tg) {
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                for (uint lane = 0; lane < rows_per_tg; ++lane) {
                    const uint row = row_base + lane;
                    if (row < rows) {
                        float g = gate_partial[(t * rows_per_tg + lane) * simdgroups_per_tg + tid];
                        float u = up_partial[(t * rows_per_tg + lane) * simdgroups_per_tg + tid];
                        g = simd_sum(g);
                        u = simd_sum(u);
                        if (simd_lane == 0) {
                            const float sig = 1.0f / (1.0f + exp(-g));
                            const float v = clamp(g * sig * u, -65504.0f, 65504.0f);
                            out_tokens[token * rows + row] = half(v);
                        }
                    }
                }
            }
        }
    }
}

kernel void qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok8_simd_fp16_tiled_k1024_i3584(
    device const half* x_tokens [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* gate_tiled [[buffer(2)]],
    device const half* up_tiled [[buffer(3)]],
    device half* out_tokens [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& rows [[buffer(6)]],
    constant uint& row_tile [[buffer(7)]],
    constant uint& col_tile [[buffer(8)]],
    constant uint& n_col_tiles [[buffer(9)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint token_tile = 8;
    constexpr uint rows_per_tg = 4;
    constexpr uint cols = 1024;
    constexpr uint simdgroups_per_tg = 8;

    threadgroup float norm_partial[token_tile * simdgroups_per_tg];
    threadgroup float inv_rms_shared[token_tile];
    threadgroup float gate_partial[token_tile * rows_per_tg * simdgroups_per_tg];
    threadgroup float up_partial[token_tile * rows_per_tg * simdgroups_per_tg];

    const uint tid = tid_pos.x;
    const uint row_group = tg_pos.x;
    const uint token_base = tg_pos.y * token_tile;
    const uint row_base = row_group * rows_per_tg;

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
        const float sum = simd_sum(ss[t]);
        if (simd_lane == 0) {
            norm_partial[t * simdgroups_per_tg + simd_group] = sum;
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (tid == 0) {
        for (uint t = 0; t < token_tile; ++t) {
            float total = 0.0f;
            for (uint group = 0; group < simdgroups_per_tg; ++group) {
                total += norm_partial[t * simdgroups_per_tg + group];
            }
            inv_rms_shared[t] = rsqrt(total / 1024.0f + 1.0e-6f);
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float gate_acc[token_tile][rows_per_tg];
    float up_acc[token_tile][rows_per_tg];
    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            gate_acc[t][lane] = 0.0f;
            up_acc[t][lane] = 0.0f;
        }
    }

    for (uint col = tid; col < cols; col += 256) {
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;

        float gate_lane[rows_per_tg];
        float up_lane[rows_per_tg];
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            gate_lane[lane] = 0.0f;
            up_lane[lane] = 0.0f;
            const uint row = row_base + lane;
            if (row < rows) {
                const uint packed_row_tile_group = row / row_tile;
                const uint row_lane = row - packed_row_tile_group * row_tile;
                const uint idx =
                    ((packed_row_tile_group * n_col_tiles + col_tile_idx) * row_tile +
                        row_lane) *
                        col_tile +
                    col_lane;
                gate_lane[lane] = float(gate_tiled[idx]);
                up_lane[lane] = float(up_tiled[idx]);
            }
        }

        const float nw = float(norm_weight[col]);
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                const float xv = float(x_tokens[token * cols + col]) * inv_rms_shared[t] * nw;
                for (uint lane = 0; lane < rows_per_tg; ++lane) {
                    gate_acc[t][lane] += gate_lane[lane] * xv;
                    up_acc[t][lane] += up_lane[lane] * xv;
                }
            }
        }
    }

    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < rows_per_tg; ++lane) {
            const float gate_sum = simd_sum(gate_acc[t][lane]);
            const float up_sum = simd_sum(up_acc[t][lane]);
            if (simd_lane == 0) {
                gate_partial[(t * rows_per_tg + lane) * simdgroups_per_tg + simd_group] =
                    gate_sum;
                up_partial[(t * rows_per_tg + lane) * simdgroups_per_tg + simd_group] = up_sum;
            }
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (tid < simdgroups_per_tg) {
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                for (uint lane = 0; lane < rows_per_tg; ++lane) {
                    const uint row = row_base + lane;
                    if (row < rows) {
                        float g = gate_partial[(t * rows_per_tg + lane) * simdgroups_per_tg + tid];
                        float u = up_partial[(t * rows_per_tg + lane) * simdgroups_per_tg + tid];
                        g = simd_sum(g);
                        u = simd_sum(u);
                        if (simd_lane == 0) {
                            const float sig = 1.0f / (1.0f + exp(-g));
                            const float v = clamp(g * sig * u, -65504.0f, 65504.0f);
                            out_tokens[token * rows + row] = half(v);
                        }
                    }
                }
            }
        }
    }
}
