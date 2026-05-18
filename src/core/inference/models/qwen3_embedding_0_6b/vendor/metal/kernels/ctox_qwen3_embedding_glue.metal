// CTOX Qwen3.5-27B Metal glue kernels.
//
// These kernels are physically vendored with the 27B Metal implementation
// because they bridge the Rust-forward buffer layout to the byte-vendored
// MLX/DFlash kernels. They intentionally avoid external runtime libraries.

#include "common.h"

static inline float ctox_silu(float x) {
    return x / (1.0f + exp(-x));
}

static inline float ctox_softplus(float x) {
    return x > 20.0f ? x : log(1.0f + exp(x));
}

kernel void ctox_embedding_gather_mlx4_bf16(
    device const int*    ids     [[buffer(0)]],
    device const uint*   w_q     [[buffer(1)]],
    device const bfloat* scales  [[buffer(2)]],
    device const bfloat* biases  [[buffer(3)]],
    device       bfloat* out     [[buffer(4)]],
    constant int&        tokens  [[buffer(5)]],
    constant int&        hidden  [[buffer(6)]],
    uint                 gid     [[thread_position_in_grid]]
) {
    const uint total = uint(tokens * hidden);
    if (gid >= total) {
        return;
    }
    const int tok = int(gid / uint(hidden));
    const int col = int(gid - uint(tok * hidden));
    const int row = ids[tok];
    const int packed_cols = hidden / 8;
    const int groups = hidden / 64;
    const uint pack = w_q[row * packed_cols + col / 8];
    const uint q = (pack >> uint(4 * (col & 7))) & 0x0fu;
    const int g = row * groups + col / 64;
    out[gid] = bfloat(float(scales[g]) * float(q) + float(biases[g]));
}

kernel void ctox_silu_bf16(
    device const bfloat* x [[buffer(0)]],
    device       bfloat* y [[buffer(1)]],
    constant int&       n [[buffer(2)]],
    uint                gid [[thread_position_in_grid]]
) {
    if (gid < uint(n)) {
        y[gid] = bfloat(ctox_silu(float(x[gid])));
    }
}

kernel void ctox_sigmoid_bf16(
    device const bfloat* x [[buffer(0)]],
    device       bfloat* y [[buffer(1)]],
    constant int&       n [[buffer(2)]],
    uint                gid [[thread_position_in_grid]]
) {
    if (gid < uint(n)) {
        y[gid] = bfloat(1.0f / (1.0f + exp(-float(x[gid]))));
    }
}

kernel void ctox_softplus_bf16(
    device const bfloat* x [[buffer(0)]],
    device       bfloat* y [[buffer(1)]],
    constant int&       n [[buffer(2)]],
    uint                gid [[thread_position_in_grid]]
) {
    if (gid < uint(n)) {
        y[gid] = bfloat(ctox_softplus(float(x[gid])));
    }
}

kernel void ctox_add_bf16(
    device const bfloat* a [[buffer(0)]],
    device const bfloat* b [[buffer(1)]],
    device       bfloat* y [[buffer(2)]],
    constant int&       n [[buffer(3)]],
    uint                gid [[thread_position_in_grid]]
) {
    if (gid < uint(n)) {
        y[gid] = bfloat(float(a[gid]) + float(b[gid]));
    }
}

kernel void ctox_mul_bf16(
    device const bfloat* a [[buffer(0)]],
    device const bfloat* b [[buffer(1)]],
    device       bfloat* y [[buffer(2)]],
    constant int&       n [[buffer(3)]],
    uint                gid [[thread_position_in_grid]]
) {
    if (gid < uint(n)) {
        y[gid] = bfloat(float(a[gid]) * float(b[gid]));
    }
}

kernel void ctox_split_q_gate_bf16(
    device const bfloat* raw        [[buffer(0)]],
    device       bfloat* q          [[buffer(1)]],
    device       bfloat* gate       [[buffer(2)]],
    constant int&        n_tokens   [[buffer(3)]],
    constant int&        q_features [[buffer(4)]],
    uint                 gid        [[thread_position_in_grid]]
) {
    const int total = n_tokens * q_features;
    if (gid >= uint(total)) {
        return;
    }
    const int tok = int(gid / uint(q_features));
    const int col = int(gid - uint(tok * q_features));
    const int raw_base = tok * q_features * 2;
    q[gid] = raw[raw_base + col];
    gate[gid] = raw[raw_base + q_features + col];
}

kernel void ctox_apply_attention_gate_bf16(
    device       bfloat* attn [[buffer(0)]],
    device const bfloat* gate [[buffer(1)]],
    constant int&        n    [[buffer(2)]],
    uint                 gid  [[thread_position_in_grid]]
) {
    if (gid < uint(n)) {
        const float g = 1.0f / (1.0f + exp(-float(gate[gid])));
        attn[gid] = bfloat(float(attn[gid]) * g);
    }
}

kernel void ctox_add_bias_bf16(
    device const bfloat* a    [[buffer(0)]],
    device const bfloat* bias [[buffer(1)]],
    device       bfloat* y    [[buffer(2)]],
    constant int&       rows [[buffer(3)]],
    constant int&       cols [[buffer(4)]],
    uint                gid  [[thread_position_in_grid]]
) {
    const uint total = uint(rows * cols);
    if (gid < total) {
        const int col = int(gid % uint(cols));
        y[gid] = bfloat(float(a[gid]) + float(bias[col]));
    }
}

kernel void ctox_neg_exp_mul_bf16(
    device const bfloat* x     [[buffer(0)]],
    device const bfloat* a_log [[buffer(1)]],
    device       bfloat* y     [[buffer(2)]],
    constant int&       rows  [[buffer(3)]],
    constant int&       cols  [[buffer(4)]],
    uint                gid   [[thread_position_in_grid]]
) {
    const uint total = uint(rows * cols);
    if (gid < total) {
        const int col = int(gid % uint(cols));
        y[gid] = bfloat(float(x[gid]) * -exp(float(a_log[col])));
    }
}

kernel void ctox_copy_bf16(
    device const bfloat* src [[buffer(0)]],
    device       bfloat* dst [[buffer(1)]],
    constant int&       n   [[buffer(2)]],
    uint                gid [[thread_position_in_grid]]
) {
    if (gid < uint(n)) {
        dst[gid] = src[gid];
    }
}

kernel void ctox_repeat_hidden5_bf16(
    device const bfloat* src    [[buffer(0)]],
    device       bfloat* dst    [[buffer(1)]],
    constant int&       rows   [[buffer(2)]],
    constant int&       hidden [[buffer(3)]],
    uint                gid    [[thread_position_in_grid]]
) {
    const uint total = uint(rows * hidden * 5);
    if (gid >= total) {
        return;
    }
    const int row = int(gid / uint(hidden * 5));
    const int col = int(gid - uint(row * hidden * 5));
    dst[gid] = src[row * hidden + (col % hidden)];
}

kernel void ctox_copy_hidden_slot_bf16(
    device const bfloat* src     [[buffer(0)]],
    device       bfloat* dst     [[buffer(1)]],
    constant int&       src_row [[buffer(2)]],
    constant int&       hidden  [[buffer(3)]],
    constant int&       slot    [[buffer(4)]],
    uint                gid     [[thread_position_in_grid]]
) {
    if (gid >= uint(hidden)) {
        return;
    }
    dst[slot * hidden + int(gid)] = src[src_row * hidden + int(gid)];
}

kernel void ctox_dense_matmul_bf16(
    device const bfloat* x        [[buffer(0)]],
    device const bfloat* w        [[buffer(1)]],
    device const bfloat* bias     [[buffer(2)]],
    device       bfloat* y        [[buffer(3)]],
    constant int&        m        [[buffer(4)]],
    constant int&        k        [[buffer(5)]],
    constant int&        n        [[buffer(6)]],
    constant int&        has_bias [[buffer(7)]],
    uint                 gid      [[thread_position_in_grid]]
) {
    const int total = m * n;
    if (gid >= uint(total)) {
        return;
    }
    const int row = int(gid / uint(n));
    const int col = int(gid - uint(row * n));
    float acc = has_bias != 0 ? float(bias[col]) : 0.0f;
    for (int i = 0; i < k; ++i) {
        acc += float(x[row * k + i]) * float(w[col * k + i]);
    }
    y[gid] = bfloat(acc);
}

kernel void ctox_kv_cache_append_bf16(
    device const bfloat* src          [[buffer(0)]],
    device       bfloat* cache        [[buffer(1)]],
    constant int&        n_tokens     [[buffer(2)]],
    constant int&        n_kv_heads   [[buffer(3)]],
    constant int&        head_dim     [[buffer(4)]],
    constant int&        max_ctx      [[buffer(5)]],
    constant int&        write_offset [[buffer(6)]],
    uint                 gid          [[thread_position_in_grid]]
) {
    const int per_tok = n_kv_heads * head_dim;
    const uint total = uint(n_tokens * per_tok);
    if (gid >= total) {
        return;
    }
    const int tok = int(gid / uint(per_tok));
    const int rest = int(gid - uint(tok * per_tok));
    const int h = rest / head_dim;
    const int d = rest - h * head_dim;
    cache[(h * max_ctx + write_offset + tok) * head_dim + d] = src[gid];
}

kernel void ctox_rope_bf16(
    device const bfloat* x          [[buffer(0)]],
    device const int*    positions  [[buffer(1)]],
    device       bfloat* y          [[buffer(2)]],
    constant int&        head_dim   [[buffer(3)]],
    constant int&        rope_dim   [[buffer(4)]],
    constant int&        n_heads    [[buffer(5)]],
    constant float&      base       [[buffer(6)]],
    constant int&        n_tokens   [[buffer(7)]],
    uint                 gid        [[thread_position_in_grid]]
) {
    const int total = n_tokens * n_heads * head_dim;
    if (gid >= uint(total)) {
        return;
    }
    const int d = int(gid % uint(head_dim));
    const int h = int((gid / uint(head_dim)) % uint(n_heads));
    const int t = int(gid / uint(n_heads * head_dim));
    const int row_base = (t * n_heads + h) * head_dim;
    const int rope_half = rope_dim / 2;
    if (d >= rope_dim) {
        y[gid] = x[gid];
        return;
    }
    const int pair = d < rope_half ? d : d - rope_half;
    const float inv_freq = pow(base, -float(2 * pair) / float(rope_dim));
    const float theta = float(positions[t]) * inv_freq;
    const float c = cos(theta);
    const float s = sin(theta);
    const float x0 = float(x[row_base + pair]);
    const float x1 = float(x[row_base + rope_half + pair]);
    if (d < rope_half) {
        y[gid] = bfloat(x0 * c - x1 * s);
    } else {
        y[gid] = bfloat(x1 * c + x0 * s);
    }
}

kernel void ctox_rms_norm_bf16(
    device const bfloat* x      [[buffer(0)]],
    device const bfloat* weight [[buffer(1)]],
    device       bfloat* y      [[buffer(2)]],
    constant int&        d      [[buffer(3)]],
    constant float&      eps    [[buffer(4)]],
    constant int&        rows   [[buffer(5)]],
    uint                 row    [[threadgroup_position_in_grid]],
    uint                 tid    [[thread_index_in_threadgroup]]
) {
    threadgroup float partial[256];
    float sum = 0.0f;
    for (int i = int(tid); i < d; i += 256) {
        const float v = float(x[int(row) * d + i]);
        sum += v * v;
    }
    partial[tid] = sum;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float inv = rsqrt(partial[0] / float(d) + eps);
    for (int i = int(tid); i < d; i += 256) {
        const int idx = int(row) * d + i;
        y[idx] = bfloat(float(x[idx]) * inv * float(weight[i]));
    }
}

kernel void ctox_l2_norm_bf16(
    device const bfloat* x    [[buffer(0)]],
    device       bfloat* y    [[buffer(1)]],
    constant int&        d    [[buffer(2)]],
    constant float&      eps  [[buffer(3)]],
    constant int&        rows [[buffer(4)]],
    uint                 row  [[threadgroup_position_in_grid]],
    uint                 tid  [[thread_index_in_threadgroup]]
) {
    threadgroup float partial[256];
    float sum = 0.0f;
    for (int i = int(tid); i < d; i += 256) {
        const float v = float(x[int(row) * d + i]);
        sum += v * v;
    }
    partial[tid] = sum;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float inv = rsqrt(partial[0] + eps);
    for (int i = int(tid); i < d; i += 256) {
        const int idx = int(row) * d + i;
        y[idx] = bfloat(float(x[idx]) * inv);
    }
}

kernel void ctox_conv_concat_bf16(
    device const bfloat* conv_state [[buffer(0)]],
    device const bfloat* qkv_new    [[buffer(1)]],
    device       bfloat* out        [[buffer(2)]],
    constant int&        kernel_m1  [[buffer(3)]],
    constant int&        n_tokens   [[buffer(4)]],
    constant int&        channels   [[buffer(5)]],
    uint                 gid        [[thread_position_in_grid]]
) {
    const int rows = kernel_m1 + n_tokens;
    const uint total = uint(rows * channels);
    if (gid >= total) {
        return;
    }
    const int r = int(gid / uint(channels));
    const int c = int(gid - uint(r * channels));
    out[gid] = r < kernel_m1
        ? conv_state[r * channels + c]
        : qkv_new[(r - kernel_m1) * channels + c];
}

kernel void ctox_ssm_conv1d_bf16(
    device const bfloat* conv_state [[buffer(0)]],
    device const bfloat* x_new      [[buffer(1)]],
    device const bfloat* weight     [[buffer(2)]],
    device const bfloat* bias       [[buffer(3)]],
    device       bfloat* y          [[buffer(4)]],
    constant int&        n_tokens   [[buffer(5)]],
    constant int&        channels   [[buffer(6)]],
    constant int&        kernel_sz  [[buffer(7)]],
    constant int&        has_bias   [[buffer(8)]],
    uint                 gid        [[thread_position_in_grid]]
) {
    const uint total = uint(n_tokens * channels);
    if (gid >= total) {
        return;
    }
    const int t = int(gid / uint(channels));
    const int c = int(gid - uint(t * channels));
    float acc = has_bias != 0 ? float(bias[c]) : 0.0f;
    const int km1 = kernel_sz - 1;
    for (int j = 0; j < kernel_sz; ++j) {
        const int row = t + j;
        const float xv = row < km1
            ? float(conv_state[row * channels + c])
            : float(x_new[(row - km1) * channels + c]);
        acc += xv * float(weight[(c * kernel_sz) + j]);
    }
    y[gid] = bfloat(acc);
}

kernel void ctox_ssm_conv_state_update_bf16(
    device const bfloat* conv_state_in  [[buffer(0)]],
    device const bfloat* x_new          [[buffer(1)]],
    device       bfloat* conv_state_out [[buffer(2)]],
    constant int&        n_tokens       [[buffer(3)]],
    constant int&        channels       [[buffer(4)]],
    constant int&        kernel_sz      [[buffer(5)]],
    uint                 c_gid          [[thread_position_in_grid]]
) {
    if (c_gid >= uint(channels)) {
        return;
    }
    const int c = int(c_gid);
    const int km1 = kernel_sz - 1;
    for (int r = 0; r < km1; ++r) {
        const int concat_row = n_tokens + r;
        conv_state_out[r * channels + c] = concat_row < km1
            ? conv_state_in[concat_row * channels + c]
            : x_new[(concat_row - km1) * channels + c];
    }
}

kernel void ctox_split_qkv_conv_bf16(
    device const bfloat* conv_out      [[buffer(0)]],
    device       bfloat* q             [[buffer(1)]],
    device       bfloat* k             [[buffer(2)]],
    device       bfloat* v             [[buffer(3)]],
    constant int&        n_tokens      [[buffer(4)]],
    constant int&        q_size        [[buffer(5)]],
    constant int&        v_size        [[buffer(6)]],
    constant int&        conv_channels [[buffer(7)]],
    uint                 gid           [[thread_position_in_grid]]
) {
    const uint q_total = uint(n_tokens * q_size);
    const uint v_total = uint(n_tokens * v_size);
    const uint all = q_total * 2u + v_total;
    if (gid >= all) {
        return;
    }
    if (gid < q_total) {
        const int t = int(gid / uint(q_size));
        const int c = int(gid - uint(t * q_size));
        q[gid] = conv_out[t * conv_channels + c];
    } else if (gid < q_total * 2u) {
        const uint off = gid - q_total;
        const int t = int(off / uint(q_size));
        const int c = int(off - uint(t * q_size));
        k[off] = conv_out[t * conv_channels + q_size + c];
    } else {
        const uint off = gid - q_total * 2u;
        const int t = int(off / uint(v_size));
        const int c = int(off - uint(t * v_size));
        v[off] = conv_out[t * conv_channels + 2 * q_size + c];
    }
}

kernel void ctox_argmax_bf16(
    device const bfloat* x      [[buffer(0)]],
    device       int*    out    [[buffer(1)]],
    constant int&        vocab  [[buffer(2)]],
    constant int&        rows   [[buffer(3)]],
    uint                 row    [[thread_position_in_grid]]
) {
    if (row >= uint(rows)) {
        return;
    }
    const int base = int(row) * vocab;
    int best = 0;
    float best_v = float(x[base]);
    for (int i = 1; i < vocab; ++i) {
        const float v = float(x[base + i]);
        if (v > best_v) {
            best_v = v;
            best = i;
        }
    }
    out[row] = best;
}

kernel void ctox_sdpa_naive_bf16(
    device const bfloat* q            [[buffer(0)]],
    device const bfloat* k_cache      [[buffer(1)]],
    device const bfloat* v_cache      [[buffer(2)]],
    device       bfloat* out          [[buffer(3)]],
    constant int&        n_heads      [[buffer(4)]],
    constant int&        n_kv_heads   [[buffer(5)]],
    constant int&        q_len        [[buffer(6)]],
    constant int&        kv_len       [[buffer(7)]],
    constant int&        head_dim     [[buffer(8)]],
    constant float&      scale        [[buffer(9)]],
    constant int&        causal       [[buffer(10)]],
    uint                 gid          [[thread_position_in_grid]]
) {
    const int total = q_len * n_heads * head_dim;
    if (gid >= uint(total)) {
        return;
    }
    const int d = int(gid % uint(head_dim));
    const int h = int((gid / uint(head_dim)) % uint(n_heads));
    const int t = int(gid / uint(n_heads * head_dim));
    const int gqa = n_heads / n_kv_heads;
    const int hk = h / gqa;

    float max_score = -FLT_MAX;
    for (int s = 0; s < kv_len; ++s) {
        if (causal != 0 && s > (kv_len - q_len + t)) {
            continue;
        }
        float score = 0.0f;
        for (int j = 0; j < head_dim; ++j) {
            const float qv = float(q[(t * n_heads + h) * head_dim + j]);
            const float kv = float(k_cache[(hk * kv_len + s) * head_dim + j]);
            score += qv * kv;
        }
        max_score = max(max_score, score * scale);
    }

    float denom = 0.0f;
    float acc = 0.0f;
    for (int s = 0; s < kv_len; ++s) {
        if (causal != 0 && s > (kv_len - q_len + t)) {
            continue;
        }
        float score = 0.0f;
        for (int j = 0; j < head_dim; ++j) {
            const float qv = float(q[(t * n_heads + h) * head_dim + j]);
            const float kv = float(k_cache[(hk * kv_len + s) * head_dim + j]);
            score += qv * kv;
        }
        const float p = exp(score * scale - max_score);
        denom += p;
        acc += p * float(v_cache[(hk * kv_len + s) * head_dim + d]);
    }
    out[(t * n_heads + h) * head_dim + d] = bfloat(acc / denom);
}
