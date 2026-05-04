#include <metal_stdlib>
#include <metal_simdgroup_matrix>
using namespace metal;

kernel void qwen35_08b_prefill_matmul_mma8x8_fp16_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
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
    simdgroup_half8x8 b;
    simdgroup_float8x8 c(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a, x_tokens + token_base * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(b, w_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(c, a, b, c);
    }

    simdgroup_store(c, y_tokens + token_base * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_matmul_mma16x8_fp16_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
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
    simdgroup_half8x8 b;
    simdgroup_float8x8 c0(0.0f);
    simdgroup_float8x8 c1(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a0, x_tokens + token_base * cols + k, cols);
        simdgroup_load(a1, x_tokens + (token_base + half_token_tile) * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(b, w_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(c0, a0, b, c0);
        simdgroup_multiply_accumulate(c1, a1, b, c1);
    }

    simdgroup_store(c0, y_tokens + token_base * rows + row_base, rows);
    simdgroup_store(c1, y_tokens + (token_base + half_token_tile) * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_matmul_mma32x8_fp16_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
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
    simdgroup_half8x8 b;
    simdgroup_float8x8 c0(0.0f);
    simdgroup_float8x8 c1(0.0f);
    simdgroup_float8x8 c2(0.0f);
    simdgroup_float8x8 c3(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a0, x_tokens + token_base * cols + k, cols);
        simdgroup_load(a1, x_tokens + (token_base + sub_token_tile) * cols + k, cols);
        simdgroup_load(a2, x_tokens + (token_base + sub_token_tile * 2) * cols + k, cols);
        simdgroup_load(a3, x_tokens + (token_base + sub_token_tile * 3) * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(b, w_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(c0, a0, b, c0);
        simdgroup_multiply_accumulate(c1, a1, b, c1);
        simdgroup_multiply_accumulate(c2, a2, b, c2);
        simdgroup_multiply_accumulate(c3, a3, b, c3);
    }

    simdgroup_store(c0, y_tokens + token_base * rows + row_base, rows);
    simdgroup_store(c1, y_tokens + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_store(c2, y_tokens + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_store(c3, y_tokens + (token_base + sub_token_tile * 3) * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_matmul_mma64x8_fp16_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
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
    simdgroup_half8x8 b;
    simdgroup_float8x8 c0(0.0f);
    simdgroup_float8x8 c1(0.0f);
    simdgroup_float8x8 c2(0.0f);
    simdgroup_float8x8 c3(0.0f);
    simdgroup_float8x8 c4(0.0f);
    simdgroup_float8x8 c5(0.0f);
    simdgroup_float8x8 c6(0.0f);
    simdgroup_float8x8 c7(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a0, x_tokens + token_base * cols + k, cols);
        simdgroup_load(a1, x_tokens + (token_base + sub_token_tile) * cols + k, cols);
        simdgroup_load(a2, x_tokens + (token_base + sub_token_tile * 2) * cols + k, cols);
        simdgroup_load(a3, x_tokens + (token_base + sub_token_tile * 3) * cols + k, cols);
        simdgroup_load(a4, x_tokens + (token_base + sub_token_tile * 4) * cols + k, cols);
        simdgroup_load(a5, x_tokens + (token_base + sub_token_tile * 5) * cols + k, cols);
        simdgroup_load(a6, x_tokens + (token_base + sub_token_tile * 6) * cols + k, cols);
        simdgroup_load(a7, x_tokens + (token_base + sub_token_tile * 7) * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(b, w_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(c0, a0, b, c0);
        simdgroup_multiply_accumulate(c1, a1, b, c1);
        simdgroup_multiply_accumulate(c2, a2, b, c2);
        simdgroup_multiply_accumulate(c3, a3, b, c3);
        simdgroup_multiply_accumulate(c4, a4, b, c4);
        simdgroup_multiply_accumulate(c5, a5, b, c5);
        simdgroup_multiply_accumulate(c6, a6, b, c6);
        simdgroup_multiply_accumulate(c7, a7, b, c7);
    }

    simdgroup_store(c0, y_tokens + token_base * rows + row_base, rows);
    simdgroup_store(c1, y_tokens + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_store(c2, y_tokens + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_store(c3, y_tokens + (token_base + sub_token_tile * 3) * rows + row_base, rows);
    simdgroup_store(c4, y_tokens + (token_base + sub_token_tile * 4) * rows + row_base, rows);
    simdgroup_store(c5, y_tokens + (token_base + sub_token_tile * 5) * rows + row_base, rows);
    simdgroup_store(c6, y_tokens + (token_base + sub_token_tile * 6) * rows + row_base, rows);
    simdgroup_store(c7, y_tokens + (token_base + sub_token_tile * 7) * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_matmul_mma128x8_fp16_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint2 tg_pos [[threadgroup_position_in_grid]]
) {
    constexpr uint token_tile = 128;
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
    simdgroup_half8x8 a8;
    simdgroup_half8x8 a9;
    simdgroup_half8x8 a10;
    simdgroup_half8x8 a11;
    simdgroup_half8x8 a12;
    simdgroup_half8x8 a13;
    simdgroup_half8x8 a14;
    simdgroup_half8x8 a15;
    simdgroup_half8x8 b;
    simdgroup_float8x8 c0(0.0f);
    simdgroup_float8x8 c1(0.0f);
    simdgroup_float8x8 c2(0.0f);
    simdgroup_float8x8 c3(0.0f);
    simdgroup_float8x8 c4(0.0f);
    simdgroup_float8x8 c5(0.0f);
    simdgroup_float8x8 c6(0.0f);
    simdgroup_float8x8 c7(0.0f);
    simdgroup_float8x8 c8(0.0f);
    simdgroup_float8x8 c9(0.0f);
    simdgroup_float8x8 c10(0.0f);
    simdgroup_float8x8 c11(0.0f);
    simdgroup_float8x8 c12(0.0f);
    simdgroup_float8x8 c13(0.0f);
    simdgroup_float8x8 c14(0.0f);
    simdgroup_float8x8 c15(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        simdgroup_load(a0, x_tokens + token_base * cols + k, cols);
        simdgroup_load(a1, x_tokens + (token_base + sub_token_tile) * cols + k, cols);
        simdgroup_load(a2, x_tokens + (token_base + sub_token_tile * 2) * cols + k, cols);
        simdgroup_load(a3, x_tokens + (token_base + sub_token_tile * 3) * cols + k, cols);
        simdgroup_load(a4, x_tokens + (token_base + sub_token_tile * 4) * cols + k, cols);
        simdgroup_load(a5, x_tokens + (token_base + sub_token_tile * 5) * cols + k, cols);
        simdgroup_load(a6, x_tokens + (token_base + sub_token_tile * 6) * cols + k, cols);
        simdgroup_load(a7, x_tokens + (token_base + sub_token_tile * 7) * cols + k, cols);
        simdgroup_load(a8, x_tokens + (token_base + sub_token_tile * 8) * cols + k, cols);
        simdgroup_load(a9, x_tokens + (token_base + sub_token_tile * 9) * cols + k, cols);
        simdgroup_load(a10, x_tokens + (token_base + sub_token_tile * 10) * cols + k, cols);
        simdgroup_load(a11, x_tokens + (token_base + sub_token_tile * 11) * cols + k, cols);
        simdgroup_load(a12, x_tokens + (token_base + sub_token_tile * 12) * cols + k, cols);
        simdgroup_load(a13, x_tokens + (token_base + sub_token_tile * 13) * cols + k, cols);
        simdgroup_load(a14, x_tokens + (token_base + sub_token_tile * 14) * cols + k, cols);
        simdgroup_load(a15, x_tokens + (token_base + sub_token_tile * 15) * cols + k, cols);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(b, w_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(c0, a0, b, c0);
        simdgroup_multiply_accumulate(c1, a1, b, c1);
        simdgroup_multiply_accumulate(c2, a2, b, c2);
        simdgroup_multiply_accumulate(c3, a3, b, c3);
        simdgroup_multiply_accumulate(c4, a4, b, c4);
        simdgroup_multiply_accumulate(c5, a5, b, c5);
        simdgroup_multiply_accumulate(c6, a6, b, c6);
        simdgroup_multiply_accumulate(c7, a7, b, c7);
        simdgroup_multiply_accumulate(c8, a8, b, c8);
        simdgroup_multiply_accumulate(c9, a9, b, c9);
        simdgroup_multiply_accumulate(c10, a10, b, c10);
        simdgroup_multiply_accumulate(c11, a11, b, c11);
        simdgroup_multiply_accumulate(c12, a12, b, c12);
        simdgroup_multiply_accumulate(c13, a13, b, c13);
        simdgroup_multiply_accumulate(c14, a14, b, c14);
        simdgroup_multiply_accumulate(c15, a15, b, c15);
    }

    simdgroup_store(c0, y_tokens + token_base * rows + row_base, rows);
    simdgroup_store(c1, y_tokens + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_store(c2, y_tokens + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_store(c3, y_tokens + (token_base + sub_token_tile * 3) * rows + row_base, rows);
    simdgroup_store(c4, y_tokens + (token_base + sub_token_tile * 4) * rows + row_base, rows);
    simdgroup_store(c5, y_tokens + (token_base + sub_token_tile * 5) * rows + row_base, rows);
    simdgroup_store(c6, y_tokens + (token_base + sub_token_tile * 6) * rows + row_base, rows);
    simdgroup_store(c7, y_tokens + (token_base + sub_token_tile * 7) * rows + row_base, rows);
    simdgroup_store(c8, y_tokens + (token_base + sub_token_tile * 8) * rows + row_base, rows);
    simdgroup_store(c9, y_tokens + (token_base + sub_token_tile * 9) * rows + row_base, rows);
    simdgroup_store(c10, y_tokens + (token_base + sub_token_tile * 10) * rows + row_base, rows);
    simdgroup_store(c11, y_tokens + (token_base + sub_token_tile * 11) * rows + row_base, rows);
    simdgroup_store(c12, y_tokens + (token_base + sub_token_tile * 12) * rows + row_base, rows);
    simdgroup_store(c13, y_tokens + (token_base + sub_token_tile * 13) * rows + row_base, rows);
    simdgroup_store(c14, y_tokens + (token_base + sub_token_tile * 14) * rows + row_base, rows);
    simdgroup_store(c15, y_tokens + (token_base + sub_token_tile * 15) * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_matmul_mma128x8_rg4_ashared_fp16_tiled_k1024_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device float* y_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    constant uint& row_tile [[buffer(5)]],
    constant uint& col_tile [[buffer(6)]],
    constant uint& n_col_tiles [[buffer(7)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    uint2 tid_pos [[thread_position_in_threadgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint token_tile = 128;
    constexpr uint sub_token_tile = 8;
    constexpr uint row_tile_expected = 8;
    constexpr uint row_groups_per_tg = 4;
    constexpr uint cols = 1024;

    if (row_tile != row_tile_expected || simd_group >= row_groups_per_tg) {
        return;
    }

    const uint tid = tid_pos.x;
    const uint row_group = tg_pos.x * row_groups_per_tg + simd_group;
    const uint token_group = tg_pos.y;
    const uint row_base = row_group * row_tile_expected;
    const uint token_base = token_group * token_tile;
    if (row_base >= rows || token_base >= tokens) {
        return;
    }

    threadgroup half a_tile[token_tile * 8];

    simdgroup_half8x8 a0;
    simdgroup_half8x8 a1;
    simdgroup_half8x8 a2;
    simdgroup_half8x8 a3;
    simdgroup_half8x8 a4;
    simdgroup_half8x8 a5;
    simdgroup_half8x8 a6;
    simdgroup_half8x8 a7;
    simdgroup_half8x8 a8;
    simdgroup_half8x8 a9;
    simdgroup_half8x8 a10;
    simdgroup_half8x8 a11;
    simdgroup_half8x8 a12;
    simdgroup_half8x8 a13;
    simdgroup_half8x8 a14;
    simdgroup_half8x8 a15;
    simdgroup_half8x8 b;
    simdgroup_float8x8 c0(0.0f);
    simdgroup_float8x8 c1(0.0f);
    simdgroup_float8x8 c2(0.0f);
    simdgroup_float8x8 c3(0.0f);
    simdgroup_float8x8 c4(0.0f);
    simdgroup_float8x8 c5(0.0f);
    simdgroup_float8x8 c6(0.0f);
    simdgroup_float8x8 c7(0.0f);
    simdgroup_float8x8 c8(0.0f);
    simdgroup_float8x8 c9(0.0f);
    simdgroup_float8x8 c10(0.0f);
    simdgroup_float8x8 c11(0.0f);
    simdgroup_float8x8 c12(0.0f);
    simdgroup_float8x8 c13(0.0f);
    simdgroup_float8x8 c14(0.0f);
    simdgroup_float8x8 c15(0.0f);

    for (uint k = 0; k < cols; k += 8) {
        for (uint i = tid; i < token_tile * 8; i += 128) {
            const uint token_lane = i / 8;
            const uint col_lane_for_a = i - token_lane * 8;
            a_tile[i] = x_tokens[(token_base + token_lane) * cols + k + col_lane_for_a];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        simdgroup_load(a0, a_tile + sub_token_tile * 0 * 8, 8);
        simdgroup_load(a1, a_tile + sub_token_tile * 1 * 8, 8);
        simdgroup_load(a2, a_tile + sub_token_tile * 2 * 8, 8);
        simdgroup_load(a3, a_tile + sub_token_tile * 3 * 8, 8);
        simdgroup_load(a4, a_tile + sub_token_tile * 4 * 8, 8);
        simdgroup_load(a5, a_tile + sub_token_tile * 5 * 8, 8);
        simdgroup_load(a6, a_tile + sub_token_tile * 6 * 8, 8);
        simdgroup_load(a7, a_tile + sub_token_tile * 7 * 8, 8);
        simdgroup_load(a8, a_tile + sub_token_tile * 8 * 8, 8);
        simdgroup_load(a9, a_tile + sub_token_tile * 9 * 8, 8);
        simdgroup_load(a10, a_tile + sub_token_tile * 10 * 8, 8);
        simdgroup_load(a11, a_tile + sub_token_tile * 11 * 8, 8);
        simdgroup_load(a12, a_tile + sub_token_tile * 12 * 8, 8);
        simdgroup_load(a13, a_tile + sub_token_tile * 13 * 8, 8);
        simdgroup_load(a14, a_tile + sub_token_tile * 14 * 8, 8);
        simdgroup_load(a15, a_tile + sub_token_tile * 15 * 8, 8);

        const uint col_tile_idx = k / col_tile;
        const uint col_lane = k - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_group * n_col_tiles + col_tile_idx) * row_tile_expected) *
                col_tile +
            col_lane;
        simdgroup_load(b, w_tiled + packed_base, col_tile, ulong2(0, 0), true);

        simdgroup_multiply_accumulate(c0, a0, b, c0);
        simdgroup_multiply_accumulate(c1, a1, b, c1);
        simdgroup_multiply_accumulate(c2, a2, b, c2);
        simdgroup_multiply_accumulate(c3, a3, b, c3);
        simdgroup_multiply_accumulate(c4, a4, b, c4);
        simdgroup_multiply_accumulate(c5, a5, b, c5);
        simdgroup_multiply_accumulate(c6, a6, b, c6);
        simdgroup_multiply_accumulate(c7, a7, b, c7);
        simdgroup_multiply_accumulate(c8, a8, b, c8);
        simdgroup_multiply_accumulate(c9, a9, b, c9);
        simdgroup_multiply_accumulate(c10, a10, b, c10);
        simdgroup_multiply_accumulate(c11, a11, b, c11);
        simdgroup_multiply_accumulate(c12, a12, b, c12);
        simdgroup_multiply_accumulate(c13, a13, b, c13);
        simdgroup_multiply_accumulate(c14, a14, b, c14);
        simdgroup_multiply_accumulate(c15, a15, b, c15);
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    simdgroup_store(c0, y_tokens + token_base * rows + row_base, rows);
    simdgroup_store(c1, y_tokens + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_store(c2, y_tokens + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_store(c3, y_tokens + (token_base + sub_token_tile * 3) * rows + row_base, rows);
    simdgroup_store(c4, y_tokens + (token_base + sub_token_tile * 4) * rows + row_base, rows);
    simdgroup_store(c5, y_tokens + (token_base + sub_token_tile * 5) * rows + row_base, rows);
    simdgroup_store(c6, y_tokens + (token_base + sub_token_tile * 6) * rows + row_base, rows);
    simdgroup_store(c7, y_tokens + (token_base + sub_token_tile * 7) * rows + row_base, rows);
    simdgroup_store(c8, y_tokens + (token_base + sub_token_tile * 8) * rows + row_base, rows);
    simdgroup_store(c9, y_tokens + (token_base + sub_token_tile * 9) * rows + row_base, rows);
    simdgroup_store(c10, y_tokens + (token_base + sub_token_tile * 10) * rows + row_base, rows);
    simdgroup_store(c11, y_tokens + (token_base + sub_token_tile * 11) * rows + row_base, rows);
    simdgroup_store(c12, y_tokens + (token_base + sub_token_tile * 12) * rows + row_base, rows);
    simdgroup_store(c13, y_tokens + (token_base + sub_token_tile * 13) * rows + row_base, rows);
    simdgroup_store(c14, y_tokens + (token_base + sub_token_tile * 14) * rows + row_base, rows);
    simdgroup_store(c15, y_tokens + (token_base + sub_token_tile * 15) * rows + row_base, rows);
}
