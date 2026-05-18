#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_embedding_gather_fp16_k1024(
    device const uint* token_in [[buffer(0)]],
    device const half* embedding [[buffer(1)]],
    device half* hidden [[buffer(2)]],
    constant uint& rows [[buffer(3)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 1024) {
        return;
    }
    uint token = token_in[0];
    if (token >= rows) {
        token = 0;
    }
    hidden[tid] = embedding[token * 1024 + tid];
}

kernel void qwen35_08b_embedding_gather_fp16_tiled_k1024(
    device const uint* token_in [[buffer(0)]],
    device const half* embedding_tiled [[buffer(1)]],
    device half* hidden [[buffer(2)]],
    constant uint& rows [[buffer(3)]],
    constant uint& row_tile [[buffer(4)]],
    constant uint& col_tile [[buffer(5)]],
    constant uint& n_col_tiles [[buffer(6)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 1024) {
        return;
    }

    uint token = token_in[0];
    if (token >= rows) {
        token = 0;
    }

    const uint row_tile_idx = token / row_tile;
    const uint row_lane = token - row_tile_idx * row_tile;
    const uint col_tile_idx = tid / col_tile;
    const uint col_lane = tid - col_tile_idx * col_tile;
    const uint packed_idx =
        ((row_tile_idx * n_col_tiles + col_tile_idx) * row_tile + row_lane) *
            col_tile +
        col_lane;

    hidden[tid] = embedding_tiled[packed_idx];
}

kernel void qwen35_08b_embedding_gather_fp16_tiled_k1024_f32(
    device const uint* token_in [[buffer(0)]],
    device const half* embedding_tiled [[buffer(1)]],
    device float* hidden [[buffer(2)]],
    constant uint& rows [[buffer(3)]],
    constant uint& row_tile [[buffer(4)]],
    constant uint& col_tile [[buffer(5)]],
    constant uint& n_col_tiles [[buffer(6)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 1024) {
        return;
    }

    uint token = token_in[0];
    if (token >= rows) {
        token = 0;
    }

    const uint row_tile_idx = token / row_tile;
    const uint row_lane = token - row_tile_idx * row_tile;
    const uint col_tile_idx = tid / col_tile;
    const uint col_lane = tid - col_tile_idx * col_tile;
    const uint packed_idx =
        ((row_tile_idx * n_col_tiles + col_tile_idx) * row_tile + row_lane) *
            col_tile +
        col_lane;

    hidden[tid] = float(embedding_tiled[packed_idx]);
}
