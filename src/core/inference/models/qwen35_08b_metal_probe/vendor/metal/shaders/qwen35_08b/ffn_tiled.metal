#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_swiglu_f32_to_fp16_i3584(
    device const float* gate [[buffer(0)]],
    device const float* up [[buffer(1)]],
    device half* out [[buffer(2)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 3584) {
        return;
    }

    const float g = gate[tid];
    const float u = up[tid];
    const float sig = 1.0f / (1.0f + exp(-g));
    const float v = clamp(g * sig * u, -65504.0f, 65504.0f);
    out[tid] = half(v);
}

kernel void qwen35_08b_mps_swiglu_gateup_fp16_i3584(
    device const half* gate_up [[buffer(0)]],
    device half* out [[buffer(1)]],
    constant uint& intermediate [[buffer(2)]],
    constant uint& gate_up_stride [[buffer(3)]],
    constant uint& out_stride [[buffer(4)]],
    uint2 gid [[thread_position_in_grid]]
) {
    const uint col = gid.x;
    const uint token = gid.y;
    if (col >= intermediate) {
        return;
    }
    const uint gate_up_base = token * gate_up_stride;
    const uint out_base = token * out_stride;
    const float gate = float(gate_up[gate_up_base + col]);
    const float up = float(gate_up[gate_up_base + intermediate + col]);
    const float sigmoid = 1.0f / (1.0f + exp(-gate));
    const float v = clamp(gate * sigmoid * up, -65504.0f, 65504.0f);
    out[out_base + col] = half(v);
}

kernel void qwen35_08b_ffn_gate_up_swiglu_rowtiles_f32_tiled_k1024_i3584(
    device const float* x [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* gate_tiled [[buffer(2)]],
    device const half* up_tiled [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& rows [[buffer(5)]],
    constant uint& row_tile [[buffer(6)]],
    constant uint& col_tile [[buffer(7)]],
    constant uint& n_col_tiles [[buffer(8)]],
    uint row_tile_group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float norm_partial[256];
    threadgroup float gate_partial[8 * 256];
    threadgroup float up_partial[8 * 256];
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
    float gate_acc[8];
    float up_acc[8];
    for (uint lane = 0; lane < 8; ++lane) {
        gate_acc[lane] = 0.0f;
        up_acc[lane] = 0.0f;
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
                const uint idx = packed_base + lane * col_tile;
                gate_acc[lane] += float(gate_tiled[idx]) * xv;
                up_acc[lane] += float(up_tiled[idx]) * xv;
            }
        }
    }

    for (uint lane = 0; lane < 8; ++lane) {
        gate_partial[lane * 256 + tid] = gate_acc[lane];
        up_partial[lane * 256 + tid] = up_acc[lane];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            for (uint lane = 0; lane < 8; ++lane) {
                gate_partial[lane * 256 + tid] += gate_partial[lane * 256 + tid + stride];
                up_partial[lane * 256 + tid] += up_partial[lane * 256 + tid + stride];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        for (uint lane = 0; lane < 8; ++lane) {
            const uint row = row_base + lane;
            if (row < rows) {
                const float g = gate_partial[lane * 256];
                const float u = up_partial[lane * 256];
                const float sig = 1.0f / (1.0f + exp(-g));
                const float v = clamp(g * sig * u, -65504.0f, 65504.0f);
                out[row] = half(v);
            }
        }
    }
}

kernel void qwen35_08b_matvec_rowtiles_fp16_tiled_k3584_f32(
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
    const uint cols = 3584;
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

kernel void qwen35_08b_matvec_residual_rowtiles_fp16_tiled_k3584_f32(
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
    const uint cols = 3584;
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
