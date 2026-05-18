#include <metal_stdlib>
#include <metal_simdgroup_matrix>
using namespace metal;

kernel void qwen35_08b_prefill_ffn_gate_up_mma8x8_normed_fp16_tiled_k1024_i3584(
    device const half* x_normed [[buffer(0)]],
    device const half* gate_tiled [[buffer(1)]],
    device const half* up_tiled [[buffer(2)]],
    device half* out_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    constant uint& rows [[buffer(5)]],
    constant uint& row_tile [[buffer(6)]],
    constant uint& col_tile [[buffer(7)]],
    constant uint& n_col_tiles [[buffer(8)]],
    uint2 tg_pos [[threadgroup_position_in_grid]]
) {
    constexpr uint token_tile = 8;
    constexpr uint row_tile_expected = 8;
    constexpr uint cols = 1024;

    if (row_tile != row_tile_expected) {
        return;
    }

    const uint row_group = tg_pos.x;
    const uint token_group = tg_pos.y;
    const uint row_base = row_group * row_tile_expected;
    const uint token_base = token_group * token_tile;

    simdgroup_half8x8 a;
    simdgroup_half8x8 gate_w;
    simdgroup_half8x8 up_w;
    simdgroup_float8x8 gate_acc(0.0f);
    simdgroup_float8x8 up_acc(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a, x_normed + token_base * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(gate_w, gate_tiled + packed_base, col_tile, ulong2(0, 0), true);
        simdgroup_load(up_w, up_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(gate_acc, a, gate_w, gate_acc);
        simdgroup_multiply_accumulate(up_acc, a, up_w, up_acc);
    }

    simdgroup_half8x8 out_mat;
    for (uint i = 0; i < 2; ++i) {
        const float g = gate_acc.thread_elements()[i];
        const float u = up_acc.thread_elements()[i];
        const float sig = 1.0f / (1.0f + exp(-g));
        out_mat.thread_elements()[i] = half(clamp(g * sig * u, -65504.0f, 65504.0f));
    }

    simdgroup_store(out_mat, out_tokens + token_base * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_ffn_gate_up_mma16x8_normed_fp16_tiled_k1024_i3584(
    device const half* x_normed [[buffer(0)]],
    device const half* gate_tiled [[buffer(1)]],
    device const half* up_tiled [[buffer(2)]],
    device half* out_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    constant uint& rows [[buffer(5)]],
    constant uint& row_tile [[buffer(6)]],
    constant uint& col_tile [[buffer(7)]],
    constant uint& n_col_tiles [[buffer(8)]],
    uint2 tg_pos [[threadgroup_position_in_grid]]
) {
    constexpr uint token_tile = 16;
    constexpr uint half_token_tile = 8;
    constexpr uint row_tile_expected = 8;
    constexpr uint cols = 1024;

    if (row_tile != row_tile_expected) {
        return;
    }

    const uint row_group = tg_pos.x;
    const uint token_group = tg_pos.y;
    const uint row_base = row_group * row_tile_expected;
    const uint token_base = token_group * token_tile;

    simdgroup_half8x8 a0;
    simdgroup_half8x8 a1;
    simdgroup_half8x8 gate_w;
    simdgroup_half8x8 up_w;
    simdgroup_float8x8 gate_acc0(0.0f);
    simdgroup_float8x8 up_acc0(0.0f);
    simdgroup_float8x8 gate_acc1(0.0f);
    simdgroup_float8x8 up_acc1(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a0, x_normed + token_base * cols + k, cols);
        simdgroup_load(a1, x_normed + (token_base + half_token_tile) * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(gate_w, gate_tiled + packed_base, col_tile, ulong2(0, 0), true);
        simdgroup_load(up_w, up_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(gate_acc0, a0, gate_w, gate_acc0);
        simdgroup_multiply_accumulate(up_acc0, a0, up_w, up_acc0);
        simdgroup_multiply_accumulate(gate_acc1, a1, gate_w, gate_acc1);
        simdgroup_multiply_accumulate(up_acc1, a1, up_w, up_acc1);
    }

    simdgroup_half8x8 out_mat0;
    simdgroup_half8x8 out_mat1;
    for (uint i = 0; i < 2; ++i) {
        const float g0 = gate_acc0.thread_elements()[i];
        const float u0 = up_acc0.thread_elements()[i];
        const float sig0 = 1.0f / (1.0f + exp(-g0));
        out_mat0.thread_elements()[i] = half(clamp(g0 * sig0 * u0, -65504.0f, 65504.0f));

        const float g1 = gate_acc1.thread_elements()[i];
        const float u1 = up_acc1.thread_elements()[i];
        const float sig1 = 1.0f / (1.0f + exp(-g1));
        out_mat1.thread_elements()[i] = half(clamp(g1 * sig1 * u1, -65504.0f, 65504.0f));
    }

    simdgroup_store(out_mat0, out_tokens + token_base * rows + row_base, rows);
    simdgroup_store(out_mat1, out_tokens + (token_base + half_token_tile) * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_ffn_gate_up_mma32x8_normed_fp16_tiled_k1024_i3584(
    device const half* x_normed [[buffer(0)]],
    device const half* gate_tiled [[buffer(1)]],
    device const half* up_tiled [[buffer(2)]],
    device half* out_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    constant uint& rows [[buffer(5)]],
    constant uint& row_tile [[buffer(6)]],
    constant uint& col_tile [[buffer(7)]],
    constant uint& n_col_tiles [[buffer(8)]],
    uint2 tg_pos [[threadgroup_position_in_grid]]
) {
    constexpr uint token_tile = 32;
    constexpr uint sub_token_tile = 8;
    constexpr uint row_tile_expected = 8;
    constexpr uint cols = 1024;

    if (row_tile != row_tile_expected) {
        return;
    }

    const uint row_group = tg_pos.x;
    const uint token_group = tg_pos.y;
    const uint row_base = row_group * row_tile_expected;
    const uint token_base = token_group * token_tile;

    simdgroup_half8x8 a0;
    simdgroup_half8x8 a1;
    simdgroup_half8x8 a2;
    simdgroup_half8x8 a3;
    simdgroup_half8x8 gate_w;
    simdgroup_half8x8 up_w;
    simdgroup_float8x8 gate_acc0(0.0f);
    simdgroup_float8x8 up_acc0(0.0f);
    simdgroup_float8x8 gate_acc1(0.0f);
    simdgroup_float8x8 up_acc1(0.0f);
    simdgroup_float8x8 gate_acc2(0.0f);
    simdgroup_float8x8 up_acc2(0.0f);
    simdgroup_float8x8 gate_acc3(0.0f);
    simdgroup_float8x8 up_acc3(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a0, x_normed + token_base * cols + k, cols);
        simdgroup_load(a1, x_normed + (token_base + sub_token_tile) * cols + k, cols);
        simdgroup_load(a2, x_normed + (token_base + sub_token_tile * 2) * cols + k, cols);
        simdgroup_load(a3, x_normed + (token_base + sub_token_tile * 3) * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(gate_w, gate_tiled + packed_base, col_tile, ulong2(0, 0), true);
        simdgroup_load(up_w, up_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(gate_acc0, a0, gate_w, gate_acc0);
        simdgroup_multiply_accumulate(up_acc0, a0, up_w, up_acc0);
        simdgroup_multiply_accumulate(gate_acc1, a1, gate_w, gate_acc1);
        simdgroup_multiply_accumulate(up_acc1, a1, up_w, up_acc1);
        simdgroup_multiply_accumulate(gate_acc2, a2, gate_w, gate_acc2);
        simdgroup_multiply_accumulate(up_acc2, a2, up_w, up_acc2);
        simdgroup_multiply_accumulate(gate_acc3, a3, gate_w, gate_acc3);
        simdgroup_multiply_accumulate(up_acc3, a3, up_w, up_acc3);
    }

    simdgroup_half8x8 out_mat0;
    simdgroup_half8x8 out_mat1;
    simdgroup_half8x8 out_mat2;
    simdgroup_half8x8 out_mat3;
    for (uint i = 0; i < 2; ++i) {
        const float g0 = gate_acc0.thread_elements()[i];
        const float u0 = up_acc0.thread_elements()[i];
        const float sig0 = 1.0f / (1.0f + exp(-g0));
        out_mat0.thread_elements()[i] = half(clamp(g0 * sig0 * u0, -65504.0f, 65504.0f));

        const float g1 = gate_acc1.thread_elements()[i];
        const float u1 = up_acc1.thread_elements()[i];
        const float sig1 = 1.0f / (1.0f + exp(-g1));
        out_mat1.thread_elements()[i] = half(clamp(g1 * sig1 * u1, -65504.0f, 65504.0f));

        const float g2 = gate_acc2.thread_elements()[i];
        const float u2 = up_acc2.thread_elements()[i];
        const float sig2 = 1.0f / (1.0f + exp(-g2));
        out_mat2.thread_elements()[i] = half(clamp(g2 * sig2 * u2, -65504.0f, 65504.0f));

        const float g3 = gate_acc3.thread_elements()[i];
        const float u3 = up_acc3.thread_elements()[i];
        const float sig3 = 1.0f / (1.0f + exp(-g3));
        out_mat3.thread_elements()[i] = half(clamp(g3 * sig3 * u3, -65504.0f, 65504.0f));
    }

    simdgroup_store(out_mat0, out_tokens + token_base * rows + row_base, rows);
    simdgroup_store(out_mat1, out_tokens + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_store(out_mat2, out_tokens + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_store(out_mat3, out_tokens + (token_base + sub_token_tile * 3) * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_ffn_gate_up_mma64x8_normed_fp16_tiled_k1024_i3584(
    device const half* x_normed [[buffer(0)]],
    device const half* gate_tiled [[buffer(1)]],
    device const half* up_tiled [[buffer(2)]],
    device half* out_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    constant uint& rows [[buffer(5)]],
    constant uint& row_tile [[buffer(6)]],
    constant uint& col_tile [[buffer(7)]],
    constant uint& n_col_tiles [[buffer(8)]],
    uint2 tg_pos [[threadgroup_position_in_grid]]
) {
    constexpr uint token_tile = 64;
    constexpr uint sub_token_tile = 8;
    constexpr uint row_tile_expected = 8;
    constexpr uint cols = 1024;

    if (row_tile != row_tile_expected) {
        return;
    }

    const uint row_group = tg_pos.x;
    const uint token_group = tg_pos.y;
    const uint row_base = row_group * row_tile_expected;
    const uint token_base = token_group * token_tile;

    simdgroup_half8x8 a0;
    simdgroup_half8x8 a1;
    simdgroup_half8x8 a2;
    simdgroup_half8x8 a3;
    simdgroup_half8x8 a4;
    simdgroup_half8x8 a5;
    simdgroup_half8x8 a6;
    simdgroup_half8x8 a7;
    simdgroup_half8x8 gate_w;
    simdgroup_half8x8 up_w;
    simdgroup_float8x8 gate_acc0(0.0f);
    simdgroup_float8x8 up_acc0(0.0f);
    simdgroup_float8x8 gate_acc1(0.0f);
    simdgroup_float8x8 up_acc1(0.0f);
    simdgroup_float8x8 gate_acc2(0.0f);
    simdgroup_float8x8 up_acc2(0.0f);
    simdgroup_float8x8 gate_acc3(0.0f);
    simdgroup_float8x8 up_acc3(0.0f);
    simdgroup_float8x8 gate_acc4(0.0f);
    simdgroup_float8x8 up_acc4(0.0f);
    simdgroup_float8x8 gate_acc5(0.0f);
    simdgroup_float8x8 up_acc5(0.0f);
    simdgroup_float8x8 gate_acc6(0.0f);
    simdgroup_float8x8 up_acc6(0.0f);
    simdgroup_float8x8 gate_acc7(0.0f);
    simdgroup_float8x8 up_acc7(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a0, x_normed + token_base * cols + k, cols);
        simdgroup_load(a1, x_normed + (token_base + sub_token_tile) * cols + k, cols);
        simdgroup_load(a2, x_normed + (token_base + sub_token_tile * 2) * cols + k, cols);
        simdgroup_load(a3, x_normed + (token_base + sub_token_tile * 3) * cols + k, cols);
        simdgroup_load(a4, x_normed + (token_base + sub_token_tile * 4) * cols + k, cols);
        simdgroup_load(a5, x_normed + (token_base + sub_token_tile * 5) * cols + k, cols);
        simdgroup_load(a6, x_normed + (token_base + sub_token_tile * 6) * cols + k, cols);
        simdgroup_load(a7, x_normed + (token_base + sub_token_tile * 7) * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(gate_w, gate_tiled + packed_base, col_tile, ulong2(0, 0), true);
        simdgroup_load(up_w, up_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(gate_acc0, a0, gate_w, gate_acc0);
        simdgroup_multiply_accumulate(up_acc0, a0, up_w, up_acc0);
        simdgroup_multiply_accumulate(gate_acc1, a1, gate_w, gate_acc1);
        simdgroup_multiply_accumulate(up_acc1, a1, up_w, up_acc1);
        simdgroup_multiply_accumulate(gate_acc2, a2, gate_w, gate_acc2);
        simdgroup_multiply_accumulate(up_acc2, a2, up_w, up_acc2);
        simdgroup_multiply_accumulate(gate_acc3, a3, gate_w, gate_acc3);
        simdgroup_multiply_accumulate(up_acc3, a3, up_w, up_acc3);
        simdgroup_multiply_accumulate(gate_acc4, a4, gate_w, gate_acc4);
        simdgroup_multiply_accumulate(up_acc4, a4, up_w, up_acc4);
        simdgroup_multiply_accumulate(gate_acc5, a5, gate_w, gate_acc5);
        simdgroup_multiply_accumulate(up_acc5, a5, up_w, up_acc5);
        simdgroup_multiply_accumulate(gate_acc6, a6, gate_w, gate_acc6);
        simdgroup_multiply_accumulate(up_acc6, a6, up_w, up_acc6);
        simdgroup_multiply_accumulate(gate_acc7, a7, gate_w, gate_acc7);
        simdgroup_multiply_accumulate(up_acc7, a7, up_w, up_acc7);
    }

    simdgroup_half8x8 out_mat0;
    simdgroup_half8x8 out_mat1;
    simdgroup_half8x8 out_mat2;
    simdgroup_half8x8 out_mat3;
    simdgroup_half8x8 out_mat4;
    simdgroup_half8x8 out_mat5;
    simdgroup_half8x8 out_mat6;
    simdgroup_half8x8 out_mat7;
    for (uint i = 0; i < 2; ++i) {
        const float g0 = gate_acc0.thread_elements()[i];
        const float u0 = up_acc0.thread_elements()[i];
        const float sig0 = 1.0f / (1.0f + exp(-g0));
        out_mat0.thread_elements()[i] = half(clamp(g0 * sig0 * u0, -65504.0f, 65504.0f));

        const float g1 = gate_acc1.thread_elements()[i];
        const float u1 = up_acc1.thread_elements()[i];
        const float sig1 = 1.0f / (1.0f + exp(-g1));
        out_mat1.thread_elements()[i] = half(clamp(g1 * sig1 * u1, -65504.0f, 65504.0f));

        const float g2 = gate_acc2.thread_elements()[i];
        const float u2 = up_acc2.thread_elements()[i];
        const float sig2 = 1.0f / (1.0f + exp(-g2));
        out_mat2.thread_elements()[i] = half(clamp(g2 * sig2 * u2, -65504.0f, 65504.0f));

        const float g3 = gate_acc3.thread_elements()[i];
        const float u3 = up_acc3.thread_elements()[i];
        const float sig3 = 1.0f / (1.0f + exp(-g3));
        out_mat3.thread_elements()[i] = half(clamp(g3 * sig3 * u3, -65504.0f, 65504.0f));

        const float g4 = gate_acc4.thread_elements()[i];
        const float u4 = up_acc4.thread_elements()[i];
        const float sig4 = 1.0f / (1.0f + exp(-g4));
        out_mat4.thread_elements()[i] = half(clamp(g4 * sig4 * u4, -65504.0f, 65504.0f));

        const float g5 = gate_acc5.thread_elements()[i];
        const float u5 = up_acc5.thread_elements()[i];
        const float sig5 = 1.0f / (1.0f + exp(-g5));
        out_mat5.thread_elements()[i] = half(clamp(g5 * sig5 * u5, -65504.0f, 65504.0f));

        const float g6 = gate_acc6.thread_elements()[i];
        const float u6 = up_acc6.thread_elements()[i];
        const float sig6 = 1.0f / (1.0f + exp(-g6));
        out_mat6.thread_elements()[i] = half(clamp(g6 * sig6 * u6, -65504.0f, 65504.0f));

        const float g7 = gate_acc7.thread_elements()[i];
        const float u7 = up_acc7.thread_elements()[i];
        const float sig7 = 1.0f / (1.0f + exp(-g7));
        out_mat7.thread_elements()[i] = half(clamp(g7 * sig7 * u7, -65504.0f, 65504.0f));
    }

    simdgroup_store(out_mat0, out_tokens + token_base * rows + row_base, rows);
    simdgroup_store(out_mat1, out_tokens + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_store(out_mat2, out_tokens + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_store(out_mat3, out_tokens + (token_base + sub_token_tile * 3) * rows + row_base, rows);
    simdgroup_store(out_mat4, out_tokens + (token_base + sub_token_tile * 4) * rows + row_base, rows);
    simdgroup_store(out_mat5, out_tokens + (token_base + sub_token_tile * 5) * rows + row_base, rows);
    simdgroup_store(out_mat6, out_tokens + (token_base + sub_token_tile * 6) * rows + row_base, rows);
    simdgroup_store(out_mat7, out_tokens + (token_base + sub_token_tile * 7) * rows + row_base, rows);
}
