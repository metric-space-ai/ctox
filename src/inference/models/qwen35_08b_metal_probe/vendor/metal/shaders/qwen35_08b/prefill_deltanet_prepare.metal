#include <metal_stdlib>
using namespace metal;

static inline float qwen35_08b_prepare_sigmoid(float x) {
    const float clamped = clamp(x, -20.0f, 20.0f);
    return 1.0f / (1.0f + exp(-clamped));
}

static inline float qwen35_08b_prepare_softplus(float x) {
    const float clamped = clamp(x, -20.0f, 20.0f);
    return select(log(1.0f + exp(clamped)), clamped, clamped > 20.0f);
}

static inline float qwen35_08b_prepare_silu(float x) {
    return x / (1.0f + exp(-clamp(x, -20.0f, 20.0f)));
}

static inline float qwen35_08b_prepare_conv1d_value(
    device const float* x_tokens,
    device const half* conv_state,
    device const half* weight,
    device const half* bias,
    uint tokens,
    uint token,
    uint channel
) {
    constexpr uint channels = 6144;
    constexpr uint kernel_width = 4;

    const uint w_base = channel * kernel_width;
    float acc = float(bias[channel]);
    for (uint tap = 0; tap < kernel_width; ++tap) {
        const int source = int(token) + int(tap) - 3;
        const float x = (source >= 0)
            ? x_tokens[uint(source) * channels + channel]
            : float(conv_state[uint(source + 3) * channels + channel]);
        acc += x * float(weight[w_base + tap]);
    }
    return qwen35_08b_prepare_silu(acc);
}

static inline float qwen35_08b_prepare_conv1d_value_qkvz(
    device const float* qkvz_tokens,
    device const half* conv_state,
    device const half* weight,
    device const half* bias,
    uint tokens,
    uint token,
    uint channel
) {
    constexpr uint channels = 6144;
    constexpr uint qkvz_width = 8192;
    constexpr uint kernel_width = 4;

    const uint w_base = channel * kernel_width;
    float acc = float(bias[channel]);
    for (uint tap = 0; tap < kernel_width; ++tap) {
        const int source = int(token) + int(tap) - 3;
        const float x = (source >= 0)
            ? qkvz_tokens[uint(source) * qkvz_width + channel]
            : float(conv_state[uint(source + 3) * channels + channel]);
        acc += x * float(weight[w_base + tap]);
    }
    return qwen35_08b_prepare_silu(acc);
}

kernel void qwen35_08b_prefill_deltanet_split_qkvz_project_f32(
    device const float* qkvz_tokens [[buffer(0)]],
    device float* qkv_tokens [[buffer(1)]],
    device float* z_tokens [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    uint tid [[thread_position_in_grid]]
) {
    constexpr uint qkv_width = 6144;
    constexpr uint z_width = 2048;
    constexpr uint qkvz_width = qkv_width + z_width;
    const uint total = tokens * qkvz_width;
    if (tid >= total) {
        return;
    }

    const uint token = tid / qkvz_width;
    const uint col = tid - token * qkvz_width;
    const float value = qkvz_tokens[tid];
    if (col < qkv_width) {
        qkv_tokens[token * qkv_width + col] = value;
    } else {
        z_tokens[token * z_width + (col - qkv_width)] = value;
    }
}

kernel void qwen35_08b_prefill_deltanet_split_qkv_norm_tok_f32_to_fp16_h16d128(
    device const float* qkv_tokens [[buffer(0)]],
    device half* q_tokens [[buffer(1)]],
    device half* k_tokens [[buffer(2)]],
    device half* v_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint qkv_width = width * 3;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || head >= heads || tid >= head_dim) {
        return;
    }

    threadgroup float q_partial[128];
    threadgroup float k_partial[128];

    const uint token_qkv_base = token * qkv_width;
    const uint vec_base = head * head_dim;
    const uint idx = vec_base + tid;
    const float qv = float(half(clamp(qkv_tokens[token_qkv_base + idx], -65504.0f, 65504.0f)));
    const float kv =
        float(half(clamp(qkv_tokens[token_qkv_base + width + idx], -65504.0f, 65504.0f)));
    const float vv =
        float(half(clamp(qkv_tokens[token_qkv_base + 2 * width + idx], -65504.0f, 65504.0f)));

    q_partial[tid] = qv * qv;
    k_partial[tid] = kv * kv;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 64; stride > 0; stride >>= 1) {
        if (tid < stride) {
            q_partial[tid] += q_partial[tid + stride];
            k_partial[tid] += k_partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float q_l2 = rsqrt(q_partial[0] + float(head_dim) * eps);
    const float k_l2 = rsqrt(k_partial[0] + float(head_dim) * eps);
    const float q_scale = rsqrt(float(head_dim));
    const uint out_base = token * width + idx;
    q_tokens[out_base] = half(clamp(qv * q_l2 * q_scale, -65504.0f, 65504.0f));
    k_tokens[out_base] = half(clamp(kv * k_l2, -65504.0f, 65504.0f));
    v_tokens[out_base] = half(clamp(vv, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_deltanet_conv_split_qkv_norm_tok_f32_to_fp16_h16d128(
    device const float* qkv_raw_tokens [[buffer(0)]],
    device const half* conv_state [[buffer(1)]],
    device const half* conv_weight [[buffer(2)]],
    device const half* conv_bias [[buffer(3)]],
    device half* q_tokens [[buffer(4)]],
    device half* k_tokens [[buffer(5)]],
    device half* v_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || head >= heads || tid >= head_dim) {
        return;
    }

    threadgroup float q_partial[128];
    threadgroup float k_partial[128];

    const uint vec_base = head * head_dim;
    const uint idx = vec_base + tid;
    const float qv = float(half(clamp(
        qwen35_08b_prepare_conv1d_value(
            qkv_raw_tokens, conv_state, conv_weight, conv_bias, tokens, token, idx),
        -65504.0f, 65504.0f)));
    const float kv = float(half(clamp(
        qwen35_08b_prepare_conv1d_value(
            qkv_raw_tokens, conv_state, conv_weight, conv_bias, tokens, token, width + idx),
        -65504.0f, 65504.0f)));
    const float vv = float(half(clamp(
        qwen35_08b_prepare_conv1d_value(
            qkv_raw_tokens, conv_state, conv_weight, conv_bias, tokens, token, 2 * width + idx),
        -65504.0f, 65504.0f)));

    q_partial[tid] = qv * qv;
    k_partial[tid] = kv * kv;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 64; stride > 0; stride >>= 1) {
        if (tid < stride) {
            q_partial[tid] += q_partial[tid + stride];
            k_partial[tid] += k_partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float q_l2 = rsqrt(q_partial[0] + float(head_dim) * eps);
    const float k_l2 = rsqrt(k_partial[0] + float(head_dim) * eps);
    const float q_scale = rsqrt(float(head_dim));
    const uint out_base = token * width + idx;
    q_tokens[out_base] = half(clamp(qv * q_l2 * q_scale, -65504.0f, 65504.0f));
    k_tokens[out_base] = half(clamp(kv * k_l2, -65504.0f, 65504.0f));
    v_tokens[out_base] = half(clamp(vv, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_deltanet_conv_split_qkvz_norm_tok_f32_to_fp16_h16d128(
    device const float* qkvz_raw_tokens [[buffer(0)]],
    device const half* conv_state [[buffer(1)]],
    device const half* conv_weight [[buffer(2)]],
    device const half* conv_bias [[buffer(3)]],
    device half* q_tokens [[buffer(4)]],
    device half* k_tokens [[buffer(5)]],
    device half* v_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || head >= heads || tid >= head_dim) {
        return;
    }

    threadgroup float q_partial[128];
    threadgroup float k_partial[128];

    const uint vec_base = head * head_dim;
    const uint idx = vec_base + tid;
    const float qv = float(half(clamp(
        qwen35_08b_prepare_conv1d_value_qkvz(
            qkvz_raw_tokens, conv_state, conv_weight, conv_bias, tokens, token, idx),
        -65504.0f, 65504.0f)));
    const float kv = float(half(clamp(
        qwen35_08b_prepare_conv1d_value_qkvz(
            qkvz_raw_tokens, conv_state, conv_weight, conv_bias, tokens, token, width + idx),
        -65504.0f, 65504.0f)));
    const float vv = float(half(clamp(
        qwen35_08b_prepare_conv1d_value_qkvz(
            qkvz_raw_tokens, conv_state, conv_weight, conv_bias, tokens, token, 2 * width + idx),
        -65504.0f, 65504.0f)));

    q_partial[tid] = qv * qv;
    k_partial[tid] = kv * kv;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 64; stride > 0; stride >>= 1) {
        if (tid < stride) {
            q_partial[tid] += q_partial[tid + stride];
            k_partial[tid] += k_partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float q_l2 = rsqrt(q_partial[0] + float(head_dim) * eps);
    const float k_l2 = rsqrt(k_partial[0] + float(head_dim) * eps);
    const float q_scale = rsqrt(float(head_dim));
    const uint out_base = token * width + idx;
    q_tokens[out_base] = half(clamp(qv * q_l2 * q_scale, -65504.0f, 65504.0f));
    k_tokens[out_base] = half(clamp(kv * k_l2, -65504.0f, 65504.0f));
    v_tokens[out_base] = half(clamp(vv, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_deltanet_conv_split_qkv_norm_tok4_f32_to_fp16_h16d128(
    device const float* qkv_raw_tokens [[buffer(0)]],
    device const half* conv_state [[buffer(1)]],
    device const half* conv_weight [[buffer(2)]],
    device const half* conv_bias [[buffer(3)]],
    device half* q_tokens [[buffer(4)]],
    device half* k_tokens [[buffer(5)]],
    device half* v_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint qkv_width = width * 3;
    constexpr uint token_block = 4;
    constexpr float eps = 1.0e-6f;

    const uint token_base = tg_pos.x * token_block;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token_base >= tokens || head >= heads || tid >= head_dim) {
        return;
    }

    threadgroup float q_partial[128];
    threadgroup float k_partial[128];

    const uint vec_base = head * head_dim;
    const uint idx = vec_base + tid;
    const uint q_channel = idx;
    const uint k_channel = width + idx;
    const uint v_channel = 2 * width + idx;

    const uint q_w_base = q_channel * 4;
    const uint k_w_base = k_channel * 4;
    const uint v_w_base = v_channel * 4;
    const float q_w0 = float(conv_weight[q_w_base]);
    const float q_w1 = float(conv_weight[q_w_base + 1]);
    const float q_w2 = float(conv_weight[q_w_base + 2]);
    const float q_w3 = float(conv_weight[q_w_base + 3]);
    const float k_w0 = float(conv_weight[k_w_base]);
    const float k_w1 = float(conv_weight[k_w_base + 1]);
    const float k_w2 = float(conv_weight[k_w_base + 2]);
    const float k_w3 = float(conv_weight[k_w_base + 3]);
    const float v_w0 = float(conv_weight[v_w_base]);
    const float v_w1 = float(conv_weight[v_w_base + 1]);
    const float v_w2 = float(conv_weight[v_w_base + 2]);
    const float v_w3 = float(conv_weight[v_w_base + 3]);
    const float q_b = float(conv_bias[q_channel]);
    const float k_b = float(conv_bias[k_channel]);
    const float v_b = float(conv_bias[v_channel]);

    float q_s0;
    float q_s1;
    float q_s2;
    float k_s0;
    float k_s1;
    float k_s2;
    float v_s0;
    float v_s1;
    float v_s2;
    if (token_base == 0) {
        q_s0 = float(conv_state[q_channel]);
        q_s1 = float(conv_state[qkv_width + q_channel]);
        q_s2 = float(conv_state[2 * qkv_width + q_channel]);
        k_s0 = float(conv_state[k_channel]);
        k_s1 = float(conv_state[qkv_width + k_channel]);
        k_s2 = float(conv_state[2 * qkv_width + k_channel]);
        v_s0 = float(conv_state[v_channel]);
        v_s1 = float(conv_state[qkv_width + v_channel]);
        v_s2 = float(conv_state[2 * qkv_width + v_channel]);
    } else if (token_base == 1) {
        q_s0 = float(conv_state[qkv_width + q_channel]);
        q_s1 = float(conv_state[2 * qkv_width + q_channel]);
        q_s2 = qkv_raw_tokens[q_channel];
        k_s0 = float(conv_state[qkv_width + k_channel]);
        k_s1 = float(conv_state[2 * qkv_width + k_channel]);
        k_s2 = qkv_raw_tokens[k_channel];
        v_s0 = float(conv_state[qkv_width + v_channel]);
        v_s1 = float(conv_state[2 * qkv_width + v_channel]);
        v_s2 = qkv_raw_tokens[v_channel];
    } else if (token_base == 2) {
        q_s0 = float(conv_state[2 * qkv_width + q_channel]);
        q_s1 = qkv_raw_tokens[q_channel];
        q_s2 = qkv_raw_tokens[qkv_width + q_channel];
        k_s0 = float(conv_state[2 * qkv_width + k_channel]);
        k_s1 = qkv_raw_tokens[k_channel];
        k_s2 = qkv_raw_tokens[qkv_width + k_channel];
        v_s0 = float(conv_state[2 * qkv_width + v_channel]);
        v_s1 = qkv_raw_tokens[v_channel];
        v_s2 = qkv_raw_tokens[qkv_width + v_channel];
    } else {
        const uint hist_base = (token_base - 3) * qkv_width;
        q_s0 = qkv_raw_tokens[hist_base + q_channel];
        q_s1 = qkv_raw_tokens[hist_base + qkv_width + q_channel];
        q_s2 = qkv_raw_tokens[hist_base + 2 * qkv_width + q_channel];
        k_s0 = qkv_raw_tokens[hist_base + k_channel];
        k_s1 = qkv_raw_tokens[hist_base + qkv_width + k_channel];
        k_s2 = qkv_raw_tokens[hist_base + 2 * qkv_width + k_channel];
        v_s0 = qkv_raw_tokens[hist_base + v_channel];
        v_s1 = qkv_raw_tokens[hist_base + qkv_width + v_channel];
        v_s2 = qkv_raw_tokens[hist_base + 2 * qkv_width + v_channel];
    }

    for (uint offset = 0; offset < token_block; ++offset) {
        const uint token = token_base + offset;
        if (token >= tokens) {
            break;
        }
        const uint raw_base = token * qkv_width;
        const float q_new = qkv_raw_tokens[raw_base + q_channel];
        const float k_new = qkv_raw_tokens[raw_base + k_channel];
        const float v_new = qkv_raw_tokens[raw_base + v_channel];

        const float qv = float(half(clamp(
            qwen35_08b_prepare_silu(q_b + q_s0 * q_w0 + q_s1 * q_w1 + q_s2 * q_w2 + q_new * q_w3),
            -65504.0f, 65504.0f)));
        const float kv = float(half(clamp(
            qwen35_08b_prepare_silu(k_b + k_s0 * k_w0 + k_s1 * k_w1 + k_s2 * k_w2 + k_new * k_w3),
            -65504.0f, 65504.0f)));
        const float vv = float(half(clamp(
            qwen35_08b_prepare_silu(v_b + v_s0 * v_w0 + v_s1 * v_w1 + v_s2 * v_w2 + v_new * v_w3),
            -65504.0f, 65504.0f)));

        q_s0 = q_s1;
        q_s1 = q_s2;
        q_s2 = q_new;
        k_s0 = k_s1;
        k_s1 = k_s2;
        k_s2 = k_new;
        v_s0 = v_s1;
        v_s1 = v_s2;
        v_s2 = v_new;

        q_partial[tid] = qv * qv;
        k_partial[tid] = kv * kv;
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint stride = 64; stride > 0; stride >>= 1) {
            if (tid < stride) {
                q_partial[tid] += q_partial[tid + stride];
                k_partial[tid] += k_partial[tid + stride];
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }

        const float q_l2 = rsqrt(q_partial[0] + float(head_dim) * eps);
        const float k_l2 = rsqrt(k_partial[0] + float(head_dim) * eps);
        const float q_scale = rsqrt(float(head_dim));
        const uint out_base = token * width + idx;
        q_tokens[out_base] = half(clamp(qv * q_l2 * q_scale, -65504.0f, 65504.0f));
        k_tokens[out_base] = half(clamp(kv * k_l2, -65504.0f, 65504.0f));
        v_tokens[out_base] = half(clamp(vv, -65504.0f, 65504.0f));
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
}

kernel void qwen35_08b_prefill_deltanet_conv_state_update_c6144_k4(
    device const float* qkv_raw_tokens [[buffer(0)]],
    device half* conv_state [[buffer(1)]],
    constant uint& tokens [[buffer(2)]],
    uint channel [[thread_position_in_grid]]
) {
    constexpr uint channels = 6144;
    if (channel >= channels || tokens == 0) {
        return;
    }

    const float s0 = float(conv_state[channel]);
    const float s1 = float(conv_state[channels + channel]);
    const float s2 = float(conv_state[2 * channels + channel]);
    float final_state[3] = {s0, s1, s2};
    for (uint slot = 0; slot < 3; ++slot) {
        const int source = int(tokens) + int(slot) - 3;
        final_state[slot] = (source >= 0)
            ? qkv_raw_tokens[uint(source) * channels + channel]
            : final_state[uint(source + 3)];
    }

    conv_state[channel] = half(clamp(final_state[0], -65504.0f, 65504.0f));
    conv_state[channels + channel] = half(clamp(final_state[1], -65504.0f, 65504.0f));
    conv_state[2 * channels + channel] = half(clamp(final_state[2], -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_deltanet_conv_state_update_qkvz_c6144_k4(
    device const float* qkvz_raw_tokens [[buffer(0)]],
    device half* conv_state [[buffer(1)]],
    constant uint& tokens [[buffer(2)]],
    uint channel [[thread_position_in_grid]]
) {
    constexpr uint channels = 6144;
    constexpr uint qkvz_width = 8192;
    if (channel >= channels || tokens == 0) {
        return;
    }

    const float s0 = float(conv_state[channel]);
    const float s1 = float(conv_state[channels + channel]);
    const float s2 = float(conv_state[2 * channels + channel]);
    float final_state[3] = {s0, s1, s2};
    for (uint slot = 0; slot < 3; ++slot) {
        const int source = int(tokens) + int(slot) - 3;
        final_state[slot] = (source >= 0)
            ? qkvz_raw_tokens[uint(source) * qkvz_width + channel]
            : final_state[uint(source + 3)];
    }

    conv_state[channel] = half(clamp(final_state[0], -65504.0f, 65504.0f));
    conv_state[channels + channel] = half(clamp(final_state[1], -65504.0f, 65504.0f));
    conv_state[2 * channels + channel] = half(clamp(final_state[2], -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_deltanet_activate_beta_decay_tok_h16(
    device const float* beta_raw_tokens [[buffer(0)]],
    device const float* alpha_raw_tokens [[buffer(1)]],
    device const float* a_log [[buffer(2)]],
    device const float* dt_bias [[buffer(3)]],
    device float* beta_tokens [[buffer(4)]],
    device float* decay_tokens [[buffer(5)]],
    constant uint& tokens [[buffer(6)]],
    uint tid [[thread_position_in_grid]]
) {
    constexpr uint heads = 16;
    const uint total = tokens * heads;
    if (tid >= total) {
        return;
    }

    const uint head = tid % heads;
    beta_tokens[tid] = qwen35_08b_prepare_sigmoid(beta_raw_tokens[tid]);
    const float alpha = qwen35_08b_prepare_softplus(alpha_raw_tokens[tid] + dt_bias[head]);
    const float a = exp(clamp(a_log[head], -20.0f, 20.0f));
    decay_tokens[tid] = exp(-a * alpha);
}

kernel void qwen35_08b_prefill_deltanet_ba_project_activate_tok4_h16_k1024(
    device const half* x_tokens [[buffer(0)]],
    device const half* b_w_tiled [[buffer(1)]],
    device const half* a_w_tiled [[buffer(2)]],
    device const float* a_log [[buffer(3)]],
    device const float* dt_bias [[buffer(4)]],
    device float* beta_tokens [[buffer(5)]],
    device float* decay_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    constant uint& row_tile [[buffer(8)]],
    constant uint& col_tile [[buffer(9)]],
    constant uint& n_col_tiles [[buffer(10)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint token_tile = 4;
    constexpr uint heads = 16;
    constexpr uint cols = 1024;
    constexpr uint simdgroups_per_tg = 8;

    threadgroup float b_partial[token_tile * 8 * simdgroups_per_tg];
    threadgroup float a_partial[token_tile * 8 * simdgroups_per_tg];

    const uint tid = tid_pos.x;
    const uint row_tile_group = tg_pos.x;
    const uint token_base = tg_pos.y * token_tile;
    const uint row_base = row_tile_group * row_tile;

    float b_acc[token_tile][8];
    float a_acc[token_tile][8];
    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < 8; ++lane) {
            b_acc[t][lane] = 0.0f;
            a_acc[t][lane] = 0.0f;
        }
    }

    for (uint col = tid; col < cols; col += 256) {
        const uint col_tile_idx = col / col_tile;
        const uint col_lane = col - col_tile_idx * col_tile;
        const uint packed_base =
            ((row_tile_group * n_col_tiles + col_tile_idx) * row_tile) *
                col_tile +
            col_lane;

        float b_w_lane[8];
        float a_w_lane[8];
        for (uint lane = 0; lane < 8; ++lane) {
            b_w_lane[lane] = 0.0f;
            a_w_lane[lane] = 0.0f;
            if (lane < row_tile && row_base + lane < heads) {
                const uint offset = packed_base + lane * col_tile;
                b_w_lane[lane] = float(b_w_tiled[offset]);
                a_w_lane[lane] = float(a_w_tiled[offset]);
            }
        }

        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                const float xv = float(x_tokens[token * cols + col]);
                for (uint lane = 0; lane < 8; ++lane) {
                    b_acc[t][lane] += b_w_lane[lane] * xv;
                    a_acc[t][lane] += a_w_lane[lane] * xv;
                }
            }
        }
    }

    for (uint t = 0; t < token_tile; ++t) {
        for (uint lane = 0; lane < 8; ++lane) {
            const float b_sum = simd_sum(b_acc[t][lane]);
            const float a_sum = simd_sum(a_acc[t][lane]);
            if (simd_lane == 0) {
                b_partial[(t * 8 + lane) * simdgroups_per_tg + simd_group] = b_sum;
                a_partial[(t * 8 + lane) * simdgroups_per_tg + simd_group] = a_sum;
            }
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (tid < simdgroups_per_tg) {
        for (uint t = 0; t < token_tile; ++t) {
            const uint token = token_base + t;
            if (token < tokens) {
                for (uint lane = 0; lane < 8; ++lane) {
                    const uint head = row_base + lane;
                    if (lane < row_tile && head < heads) {
                        float b_total =
                            b_partial[(t * 8 + lane) * simdgroups_per_tg + tid];
                        float a_total =
                            a_partial[(t * 8 + lane) * simdgroups_per_tg + tid];
                        b_total = simd_sum(b_total);
                        a_total = simd_sum(a_total);
                        if (simd_lane == 0) {
                            const uint out_idx = token * heads + head;
                            beta_tokens[out_idx] = qwen35_08b_prepare_sigmoid(b_total);
                            const float alpha =
                                qwen35_08b_prepare_softplus(a_total + dt_bias[head]);
                            const float a = exp(clamp(a_log[head], -20.0f, 20.0f));
                            decay_tokens[out_idx] = exp(-a * alpha);
                        }
                    }
                }
            }
        }
    }
}
