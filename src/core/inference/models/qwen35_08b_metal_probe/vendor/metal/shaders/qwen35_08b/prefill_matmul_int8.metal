#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_prefill_matmul_int8_row_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const uchar* w_quant [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& quant_group_size [[buffer(7)]],
    constant uint& n_col_tiles [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint tid [[thread_index_in_threadgroup]]
) {
    constexpr uint cols = 1024;
    constexpr uint max_row_tile = 16;
    threadgroup float partial[max_row_tile * 256];

    const uint row_tile_id = tg_pos.x;
    const uint token = tg_pos.y;
    if (token >= tokens || row_tile > max_row_tile || quant_group_size == 0u ||
        quant_group_size > col_tile || (col_tile % quant_group_size) != 0u) {
        return;
    }

    const uint groups_per_col_tile = col_tile / quant_group_size;
    const uint group_stride = 2u + quant_group_size;
    const uint row_stride = groups_per_col_tile * group_stride;
    const uint tile_stride = row_tile * row_stride;
    float acc[max_row_tile];
    for (uint r = 0; r < max_row_tile; ++r) {
        acc[r] = 0.0f;
    }

    for (uint tile = 0; tile < n_col_tiles; ++tile) {
        const uint col_base = tile * col_tile;
        const uint tile_base = (row_tile_id * n_col_tiles + tile) * tile_stride;

        for (uint local_col = tid; local_col < col_tile; local_col += 256u) {
            const uint col = col_base + local_col;
            const float xv = (col < cols) ? float(x_tokens[token * cols + col]) : 0.0f;
            const uint quant_group_id = local_col / quant_group_size;
            const uint col_in_group = local_col - quant_group_id * quant_group_size;
            for (uint r = 0; r < max_row_tile; ++r) {
                if (r < row_tile) {
                    const uint row = row_tile_id * row_tile + r;
                    if (row < rows) {
                        const uint group_base =
                            tile_base + r * row_stride + quant_group_id * group_stride;
                        const uint scale_bits =
                            uint(w_quant[group_base]) | (uint(w_quant[group_base + 1]) << 8);
                        const float scale = float(as_type<half>(ushort(scale_bits)));
                        const int q = int(as_type<char>(w_quant[group_base + 2u + col_in_group]));
                        acc[r] += xv * (float(q) * scale);
                    }
                }
            }
        }
    }

    for (uint r = 0; r < max_row_tile; ++r) {
        partial[r * 256u + tid] = acc[r];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            for (uint r = 0; r < max_row_tile; ++r) {
                partial[r * 256u + tid] += partial[r * 256u + tid + stride];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    if (tid == 0) {
        for (uint r = 0; r < max_row_tile; ++r) {
            if (r < row_tile) {
                const uint row = row_tile_id * row_tile + r;
                if (row < rows) {
                    y_tokens[token * rows + row] = partial[r * 256u];
                }
            }
        }
    }
}

kernel void qwen35_08b_prefill_matmul_int8_row_tiled_simd32_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const uchar* w_quant [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& quant_group_size [[buffer(7)]],
    constant uint& n_col_tiles [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint cols = 1024;
    constexpr uint max_row_tile = 16;

    const uint row_tile_id = tg_pos.x;
    const uint token = tg_pos.y;
    if (token >= tokens || row_tile == 0u || row_tile > max_row_tile ||
        quant_group_size == 0u || quant_group_size > col_tile ||
        (col_tile % quant_group_size) != 0u || simd_group >= row_tile) {
        return;
    }

    const uint row = row_tile_id * row_tile + simd_group;
    if (row >= rows) {
        return;
    }

    const uint groups_per_col_tile = col_tile / quant_group_size;
    const uint group_stride = 2u + quant_group_size;
    const uint row_stride = groups_per_col_tile * group_stride;
    const uint tile_stride = row_tile * row_stride;
    float acc = 0.0f;

    for (uint tile = 0; tile < n_col_tiles; ++tile) {
        const uint col_base = tile * col_tile;
        const uint tile_base = (row_tile_id * n_col_tiles + tile) * tile_stride;
        const uint local_row = simd_group;

        for (uint local_col = simd_lane; local_col < col_tile; local_col += 32u) {
            const uint col = col_base + local_col;
            const float xv = (col < cols) ? float(x_tokens[token * cols + col]) : 0.0f;
            const uint quant_group_id = local_col / quant_group_size;
            const uint col_in_group = local_col - quant_group_id * quant_group_size;
            const uint group_base =
                tile_base + local_row * row_stride + quant_group_id * group_stride;
            const uint scale_bits =
                uint(w_quant[group_base]) | (uint(w_quant[group_base + 1]) << 8);
            const float scale = float(as_type<half>(ushort(scale_bits)));
            const int q = int(as_type<char>(w_quant[group_base + 2u + col_in_group]));
            acc += xv * (float(q) * scale);
        }
    }

    const float sum = simd_sum(acc);
    if (simd_lane == 0u) {
        y_tokens[token * rows + row] = sum;
    }
}
