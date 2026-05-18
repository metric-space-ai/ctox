#include <metal_stdlib>
#include <metal_simdgroup_matrix>
using namespace metal;

kernel void qwen35_08b_prefill_down_mma8x8_fp16_tiled_k3584_f32(
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
    constexpr uint cols = 3584;

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

kernel void qwen35_08b_prefill_down_mma16x8_fp16_tiled_k3584_f32(
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
    constexpr uint cols = 3584;

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

kernel void qwen35_08b_prefill_down_mma32x8_fp16_tiled_k3584_f32(
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
    constexpr uint cols = 3584;

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

kernel void qwen35_08b_prefill_down_mma32x8_residual_fp16_tiled_k3584_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device const half* residual [[buffer(2)]],
    device half* y_tokens [[buffer(3)]],
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
    constexpr uint cols = 3584;

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

    simdgroup_half8x8 r0;
    simdgroup_half8x8 r1;
    simdgroup_half8x8 r2;
    simdgroup_half8x8 r3;
    simdgroup_half8x8 out0;
    simdgroup_half8x8 out1;
    simdgroup_half8x8 out2;
    simdgroup_half8x8 out3;
    simdgroup_load(r0, residual + token_base * rows + row_base, rows);
    simdgroup_load(r1, residual + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_load(r2, residual + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_load(r3, residual + (token_base + sub_token_tile * 3) * rows + row_base, rows);

    for (uint i = 0; i < 2; ++i) {
        out0.thread_elements()[i] = half(clamp(c0.thread_elements()[i] + float(r0.thread_elements()[i]), -65504.0f, 65504.0f));
        out1.thread_elements()[i] = half(clamp(c1.thread_elements()[i] + float(r1.thread_elements()[i]), -65504.0f, 65504.0f));
        out2.thread_elements()[i] = half(clamp(c2.thread_elements()[i] + float(r2.thread_elements()[i]), -65504.0f, 65504.0f));
        out3.thread_elements()[i] = half(clamp(c3.thread_elements()[i] + float(r3.thread_elements()[i]), -65504.0f, 65504.0f));
    }

    simdgroup_store(out0, y_tokens + token_base * rows + row_base, rows);
    simdgroup_store(out1, y_tokens + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_store(out2, y_tokens + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_store(out3, y_tokens + (token_base + sub_token_tile * 3) * rows + row_base, rows);
}

kernel void qwen35_08b_prefill_down_mma64x8_fp16_tiled_k3584_f32(
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
    constexpr uint cols = 3584;

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

kernel void qwen35_08b_prefill_down_mma64x8_residual_fp16_tiled_k3584_f32(
    device const half* x_tokens [[buffer(0)]],
    device const half* w_tiled [[buffer(1)]],
    device const half* residual [[buffer(2)]],
    device half* y_tokens [[buffer(3)]],
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
    constexpr uint cols = 3584;

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

    simdgroup_half8x8 r0;
    simdgroup_half8x8 r1;
    simdgroup_half8x8 r2;
    simdgroup_half8x8 r3;
    simdgroup_half8x8 r4;
    simdgroup_half8x8 r5;
    simdgroup_half8x8 r6;
    simdgroup_half8x8 r7;
    simdgroup_half8x8 out0;
    simdgroup_half8x8 out1;
    simdgroup_half8x8 out2;
    simdgroup_half8x8 out3;
    simdgroup_half8x8 out4;
    simdgroup_half8x8 out5;
    simdgroup_half8x8 out6;
    simdgroup_half8x8 out7;
    simdgroup_load(r0, residual + token_base * rows + row_base, rows);
    simdgroup_load(r1, residual + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_load(r2, residual + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_load(r3, residual + (token_base + sub_token_tile * 3) * rows + row_base, rows);
    simdgroup_load(r4, residual + (token_base + sub_token_tile * 4) * rows + row_base, rows);
    simdgroup_load(r5, residual + (token_base + sub_token_tile * 5) * rows + row_base, rows);
    simdgroup_load(r6, residual + (token_base + sub_token_tile * 6) * rows + row_base, rows);
    simdgroup_load(r7, residual + (token_base + sub_token_tile * 7) * rows + row_base, rows);

    for (uint i = 0; i < 2; ++i) {
        out0.thread_elements()[i] = half(clamp(c0.thread_elements()[i] + float(r0.thread_elements()[i]), -65504.0f, 65504.0f));
        out1.thread_elements()[i] = half(clamp(c1.thread_elements()[i] + float(r1.thread_elements()[i]), -65504.0f, 65504.0f));
        out2.thread_elements()[i] = half(clamp(c2.thread_elements()[i] + float(r2.thread_elements()[i]), -65504.0f, 65504.0f));
        out3.thread_elements()[i] = half(clamp(c3.thread_elements()[i] + float(r3.thread_elements()[i]), -65504.0f, 65504.0f));
        out4.thread_elements()[i] = half(clamp(c4.thread_elements()[i] + float(r4.thread_elements()[i]), -65504.0f, 65504.0f));
        out5.thread_elements()[i] = half(clamp(c5.thread_elements()[i] + float(r5.thread_elements()[i]), -65504.0f, 65504.0f));
        out6.thread_elements()[i] = half(clamp(c6.thread_elements()[i] + float(r6.thread_elements()[i]), -65504.0f, 65504.0f));
        out7.thread_elements()[i] = half(clamp(c7.thread_elements()[i] + float(r7.thread_elements()[i]), -65504.0f, 65504.0f));
    }

    simdgroup_store(out0, y_tokens + token_base * rows + row_base, rows);
    simdgroup_store(out1, y_tokens + (token_base + sub_token_tile) * rows + row_base, rows);
    simdgroup_store(out2, y_tokens + (token_base + sub_token_tile * 2) * rows + row_base, rows);
    simdgroup_store(out3, y_tokens + (token_base + sub_token_tile * 3) * rows + row_base, rows);
    simdgroup_store(out4, y_tokens + (token_base + sub_token_tile * 4) * rows + row_base, rows);
    simdgroup_store(out5, y_tokens + (token_base + sub_token_tile * 5) * rows + row_base, rows);
    simdgroup_store(out6, y_tokens + (token_base + sub_token_tile * 6) * rows + row_base, rows);
    simdgroup_store(out7, y_tokens + (token_base + sub_token_tile * 7) * rows + row_base, rows);
}
