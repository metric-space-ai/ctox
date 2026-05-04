#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_matvec_rowtiles_fp16_tiled_k1024_f32(
    device const half* x [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y [[buffer(2)]],
    constant uint& rows [[buffer(3)]],
    constant uint& row_tile [[buffer(4)]],
    constant uint& col_tile [[buffer(5)]],
    constant uint& n_col_tiles [[buffer(6)]],
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
                y[row] = partial[lane * 256];
            }
        }
    }
}

kernel void qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32(
    device const half* x [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* w_tiled [[buffer(2)]],
    device float* y [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float norm_partial[256];
    threadgroup float partial[8 * 256];
    const uint cols = 1024;
    const uint row_base = row_tile_group * row_tile;

    float ss = 0.0f;
    for (uint col = tid; col < cols; col += 256) {
        const float v = float(x[col]);
        ss += v * v;
    }
    norm_partial[tid] = ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            norm_partial[tid] += norm_partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(norm_partial[0] / 1024.0f + 1.0e-6f);
    float acc[8];
    for (uint lane = 0; lane < 8; ++lane) {
        acc[lane] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        const float xv = float(x[col]) * inv_rms * float(norm_weight[col]);
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
                y[row] = partial[lane * 256];
            }
        }
    }
}

kernel void qwen35_08b_rms_matvec_rowtiles_f32_tiled_k1024_f32(
    device const float* x [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* w_tiled [[buffer(2)]],
    device float* y [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float norm_partial[256];
    threadgroup float partial[8 * 256];
    const uint cols = 1024;
    const uint row_base = row_tile_group * row_tile;

    float ss = 0.0f;
    for (uint col = tid; col < cols; col += 256) {
        const float v = x[col];
        ss += v * v;
    }
    norm_partial[tid] = ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            norm_partial[tid] += norm_partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(norm_partial[0] / 1024.0f + 1.0e-6f);
    float acc[8];
    for (uint lane = 0; lane < 8; ++lane) {
        acc[lane] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        const float xv = x[col] * inv_rms * float(norm_weight[col]);
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
                y[row] = partial[lane * 256];
            }
        }
    }
}

kernel void qwen35_08b_deltanet_qkv_z_b_a_rms_project_f32_tiled_k1024(
    device const float* x [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* qkv_tiled [[buffer(2)]],
    device const half* z_tiled [[buffer(3)]],
    device const half* b_tiled [[buffer(4)]],
    device const half* a_tiled [[buffer(5)]],
    device float* qkv_out [[buffer(6)]],
    device float* z_out [[buffer(7)]],
    device float* b_out [[buffer(8)]],
    device float* a_out [[buffer(9)]],
    constant uint& qkv_rows [[buffer(10)]],
    constant uint& z_rows [[buffer(11)]],
    constant uint& b_rows [[buffer(12)]],
    constant uint& a_rows [[buffer(13)]],
    constant uint& row_tile [[buffer(14)]],
    constant uint& col_tile [[buffer(15)]],
    constant uint& n_col_tiles [[buffer(16)]],
    uint global_row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float norm_partial[256];
    threadgroup float partial[8 * 256];
    const uint cols = 1024;

    const uint qkv_groups = (qkv_rows + row_tile - 1) / row_tile;
    const uint z_groups = (z_rows + row_tile - 1) / row_tile;
    const uint b_groups = (b_rows + row_tile - 1) / row_tile;

    uint local_group = global_row_tile_group;
    uint rows = qkv_rows;
    device const half* w_tiled = qkv_tiled;
    device float* y = qkv_out;
    if (local_group >= qkv_groups) {
        local_group -= qkv_groups;
        rows = z_rows;
        w_tiled = z_tiled;
        y = z_out;
        if (local_group >= z_groups) {
            local_group -= z_groups;
            rows = b_rows;
            w_tiled = b_tiled;
            y = b_out;
            if (local_group >= b_groups) {
                local_group -= b_groups;
                rows = a_rows;
                w_tiled = a_tiled;
                y = a_out;
            }
        }
    }

    const uint row_base = local_group * row_tile;

    float ss = 0.0f;
    for (uint col = tid; col < cols; col += 256) {
        const float v = x[col];
        ss += v * v;
    }
    norm_partial[tid] = ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            norm_partial[tid] += norm_partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(norm_partial[0] / 1024.0f + 1.0e-6f);
    float acc[8];
    for (uint lane = 0; lane < 8; ++lane) {
        acc[lane] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        const float xv = x[col] * inv_rms * float(norm_weight[col]);
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_base =
            ((local_group * n_col_tiles + col_tile_idx) * row_tile) *
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
                y[row] = partial[lane * 256];
            }
        }
    }
}

kernel void qwen35_08b_attention_q_k_v_rms_project_f32_tiled_k1024(
    device const float* x [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* q_tiled [[buffer(2)]],
    device const half* k_tiled [[buffer(3)]],
    device const half* v_tiled [[buffer(4)]],
    device float* q_out [[buffer(5)]],
    device float* k_out [[buffer(6)]],
    device float* v_out [[buffer(7)]],
    constant uint& q_rows [[buffer(8)]],
    constant uint& k_rows [[buffer(9)]],
    constant uint& v_rows [[buffer(10)]],
    constant uint& row_tile [[buffer(11)]],
    constant uint& col_tile [[buffer(12)]],
    constant uint& n_col_tiles [[buffer(13)]],
    uint global_row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float norm_partial[256];
    threadgroup float partial[8 * 256];
    const uint cols = 1024;

    const uint q_groups = (q_rows + row_tile - 1) / row_tile;
    const uint k_groups = (k_rows + row_tile - 1) / row_tile;

    uint local_group = global_row_tile_group;
    uint rows = q_rows;
    device const half* w_tiled = q_tiled;
    device float* y = q_out;
    if (local_group >= q_groups) {
        local_group -= q_groups;
        rows = k_rows;
        w_tiled = k_tiled;
        y = k_out;
        if (local_group >= k_groups) {
            local_group -= k_groups;
            rows = v_rows;
            w_tiled = v_tiled;
            y = v_out;
        }
    }

    const uint row_base = local_group * row_tile;

    float ss = 0.0f;
    for (uint col = tid; col < cols; col += 256) {
        const float val = x[col];
        ss += val * val;
    }
    norm_partial[tid] = ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            norm_partial[tid] += norm_partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(norm_partial[0] / 1024.0f + 1.0e-6f);
    float acc[8];
    for (uint lane = 0; lane < 8; ++lane) {
        acc[lane] = 0.0f;
    }

    for (uint col = tid; col < cols; col += 256) {
        const float xv = x[col] * inv_rms * float(norm_weight[col]);
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_base =
            ((local_group * n_col_tiles + col_tile_idx) * row_tile) *
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
                y[row] = partial[lane * 256];
            }
        }
    }
}

kernel void qwen35_08b_matvec_rowtiles_fp16_tiled_k2048_f32(
    device const half* x [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y [[buffer(2)]],
    constant uint& rows [[buffer(3)]],
    constant uint& row_tile [[buffer(4)]],
    constant uint& col_tile [[buffer(5)]],
    constant uint& n_col_tiles [[buffer(6)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[8 * 256];
    const uint cols = 2048;
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
                y[row] = partial[lane * 256];
            }
        }
    }
}

kernel void qwen35_08b_matvec_residual_rowtiles_fp16_tiled_k2048_f32(
    device const half* x [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device const float* residual [[buffer(2)]],
    device float* y [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[8 * 256];
    const uint cols = 2048;
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
                y[row] = residual[row] + partial[lane * 256];
            }
        }
    }
}
