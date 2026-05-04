#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_tiled_attention_pack_qwen_qkv_group(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device half* q_mps [[buffer(3)]],
    device half* k_mps [[buffer(4)]],
    device half* v_mps [[buffer(5)]],
    constant uint& tokens [[buffer(6)]],
    constant uint& kv_group [[buffer(7)]],
    constant uint& q_row_stride [[buffer(8)]],
    constant uint& k_row_stride [[buffer(9)]],
    constant uint& v_row_stride [[buffer(10)]],
    uint gid [[thread_position_in_grid]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = q_heads / kv_heads;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;

    const uint q_total = tokens * heads_per_group * head_dim;
    if (gid < q_total) {
        const uint token = gid / (heads_per_group * head_dim);
        const uint rem = gid - token * heads_per_group * head_dim;
        const uint hp = rem / head_dim;
        const uint dim = rem - hp * head_dim;
        const uint q_head = kv_group * heads_per_group + hp;
        q_mps[(token * heads_per_group + hp) * q_row_stride + dim] =
            q_cache[token * q_width + q_head * head_dim + dim];
    }

    const uint kv_total = tokens * head_dim;
    if (gid < kv_total) {
        const uint token = gid / head_dim;
        const uint dim = gid - token * head_dim;
        const uint src = token * kv_width + kv_group * head_dim + dim;
        k_mps[dim * k_row_stride + token] = k_cache[src];
        v_mps[token * v_row_stride + dim] = v_cache[src];
    }
}

kernel void qwen35_08b_tiled_attention_init_rows(
    device float* m_state [[buffer(0)]],
    device float* l_state [[buffer(1)]],
    device half* out [[buffer(2)]],
    constant uint& q_rows [[buffer(3)]],
    constant uint& head_dim [[buffer(4)]],
    uint gid [[thread_position_in_grid]]
) {
    const uint total = q_rows * head_dim;
    if (gid < q_rows) {
        m_state[gid] = -INFINITY;
        l_state[gid] = 0.0f;
    }
    if (gid < total) {
        out[gid] = half(0.0f);
    }
}

kernel void qwen35_08b_tiled_attention_softmax_update_simd32(
    device half* score [[buffer(0)]],
    device half* prob [[buffer(1)]],
    device float* m_state [[buffer(2)]],
    device float* l_state [[buffer(3)]],
    device float* old_scale [[buffer(4)]],
    device float* inv_l [[buffer(5)]],
    device float* pv_scale [[buffer(6)]],
    constant uint& q_rows [[buffer(7)]],
    constant uint& k_tile [[buffer(8)]],
    constant uint& score_row_stride [[buffer(9)]],
    constant uint& q_block [[buffer(10)]],
    constant uint& k_block [[buffer(11)]],
    constant uint& q_tile [[buffer(12)]],
    uint gid [[thread_position_in_grid]]
) {
    const uint lane = gid & 31u;
    const uint row = gid >> 5u;
    if (row >= q_rows) {
        return;
    }

    constexpr uint heads_per_group = 4;
    const uint query_row = row / heads_per_group;
    const uint q_abs = q_block * q_tile + query_row;
    float local_m = -INFINITY;
    for (uint col = lane; col < k_tile; col += 32u) {
        const uint k_abs = k_block * k_tile + col;
        const float s = (k_abs <= q_abs) ? float(score[row * score_row_stride + col]) : -INFINITY;
        local_m = max(local_m, s);
    }
    const float tile_m = simd_max(local_m);

    float local_l = 0.0f;
    for (uint col = lane; col < k_tile; col += 32u) {
        const uint k_abs = k_block * k_tile + col;
        float p = 0.0f;
        if (k_abs <= q_abs && isfinite(tile_m)) {
            p = exp(float(score[row * score_row_stride + col]) - tile_m);
        }
        local_l += p;
        prob[row * score_row_stride + col] = half(clamp(p, 0.0f, 65504.0f));
    }
    const float tile_l = simd_sum(local_l);

    if (lane == 0u) {
        const float prev_m = m_state[row];
        const float prev_l = l_state[row];
        const float next_m = max(prev_m, tile_m);
        const float prev_scale = isfinite(prev_m) ? prev_l * exp(prev_m - next_m) : 0.0f;
        const float tile_scale = isfinite(tile_m) ? tile_l * exp(tile_m - next_m) : 0.0f;
        const float next_l = prev_scale + tile_scale;

        old_scale[row] = prev_scale;
        pv_scale[row] = isfinite(tile_m) ? exp(tile_m - next_m) : 0.0f;
        inv_l[row] = (next_l > 0.0f) ? (1.0f / next_l) : 0.0f;
        m_state[row] = next_m;
        l_state[row] = next_l;
    }
}

kernel void qwen35_08b_tiled_attention_combine(
    device half* out [[buffer(0)]],
    device half* pv [[buffer(1)]],
    device float* old_scale [[buffer(2)]],
    device float* inv_l [[buffer(3)]],
    device float* pv_scale [[buffer(4)]],
    constant uint& q_rows [[buffer(5)]],
    constant uint& head_dim [[buffer(6)]],
    constant uint& out_row_stride [[buffer(7)]],
    uint gid [[thread_position_in_grid]]
) {
    const uint total = q_rows * head_dim;
    if (gid >= total) {
        return;
    }
    const uint row = gid / head_dim;
    const uint col = gid - row * head_dim;
    const uint offset = row * out_row_stride + col;
    const float value =
        (float(out[offset]) * old_scale[row] + float(pv[offset]) * pv_scale[row]) * inv_l[row];
    out[offset] = half(clamp(value, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_tiled_attention_store_global(
    device const half* out_tile [[buffer(0)]],
    device half* global_out [[buffer(1)]],
    constant uint& q_rows [[buffer(2)]],
    constant uint& head_dim [[buffer(3)]],
    constant uint& out_row_stride [[buffer(4)]],
    constant uint& global_row_stride [[buffer(5)]],
    constant uint& q_block [[buffer(6)]],
    constant uint& q_tile [[buffer(7)]],
    constant uint& tokens [[buffer(8)]],
    uint gid [[thread_position_in_grid]]
) {
    const uint total = q_rows * head_dim;
    if (gid >= total) {
        return;
    }
    const uint row = gid / head_dim;
    const uint col = gid - row * head_dim;
    constexpr uint heads_per_group = 4;
    const uint query_row = row / heads_per_group;
    const uint q_abs = q_block * q_tile + query_row;
    if (q_abs >= tokens) {
        return;
    }
    const uint global_row = q_block * q_rows + row;
    global_out[global_row * global_row_stride + col] = out_tile[row * out_row_stride + col];
}

kernel void qwen35_08b_tiled_attention_store_qwen_attn_with_gate(
    device const half* out_tile [[buffer(0)]],
    device const float* q_tokens [[buffer(1)]],
    device half* attn [[buffer(2)]],
    constant uint& q_project_rows [[buffer(3)]],
    constant uint& out_row_stride [[buffer(4)]],
    constant uint& q_block [[buffer(5)]],
    constant uint& q_tile [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    constant uint& kv_group [[buffer(8)]],
    uint gid [[thread_position_in_grid]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = q_heads / kv_heads;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;

    const uint q_rows = q_tile * heads_per_group;
    const uint total = q_rows * head_dim;
    if (gid >= total) {
        return;
    }

    const uint row = gid / head_dim;
    const uint col = gid - row * head_dim;
    const uint local_token = row / heads_per_group;
    const uint hp = row - local_token * heads_per_group;
    const uint token = q_block * q_tile + local_token;
    if (token >= tokens) {
        return;
    }

    const uint q_head = kv_group * heads_per_group + hp;
    const uint q_base = token * q_project_rows +
        ((q_project_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
    const float gate = (q_project_rows >= q_width * 2)
        ? (1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + col])))
        : 1.0f;
    const float value = float(out_tile[row * out_row_stride + col]) * gate;
    attn[token * q_width + q_head * head_dim + col] = half(clamp(value, -65504.0f, 65504.0f));
}
