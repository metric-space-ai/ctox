#include <metal_stdlib>
using namespace metal;

static inline float qwen35_08b_prefill_attn_rope_freq(uint d) {
    constexpr float theta = 10000000.0f;
    constexpr float rope_dim = 64.0f;
    const float pair = float(d % 32u);
    return pow(theta, -(2.0f * pair) / rope_dim);
}

static inline float qwen35_08b_prefill_attn_rope_pair(
    float a,
    float b,
    uint d,
    uint position
) {
    constexpr uint half_rope_dim = 32;
    const float angle = float(position) * qwen35_08b_prefill_attn_rope_freq(d);
    const float c = cos(angle);
    const float s = sin(angle);
    if (d < half_rope_dim) {
        return a * c - b * s;
    }
    return b * c + a * s;
}

kernel void qwen35_08b_prefill_attention_prepare_qk_rope_v_gqa8_kv2_d256(
    device const float* q_tokens [[buffer(0)]],
    device const float* k_tokens [[buffer(1)]],
    device const float* v_tokens [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* q_cache [[buffer(5)]],
    device half* k_cache [[buffer(6)]],
    device half* v_cache [[buffer(7)]],
    constant uint& tokens [[buffer(8)]],
    constant uint& q_rows [[buffer(9)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint rope_dim = 64;
    constexpr uint half_rope_dim = rope_dim / 2;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[256];

    const uint q_base =
        token * q_rows + ((q_rows >= q_width * 2) ? head * head_dim * 2 : head * head_dim);
    float q_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float qv = q_tokens[q_base + d];
        q_ss += qv * qv;
    }
    partial[tid] = q_ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float q_inv_rms = rsqrt(partial[0] / float(head_dim) + eps);

    float q_out;
    if (tid < rope_dim) {
        const uint lo = tid % half_rope_dim;
        const uint hi = lo + half_rope_dim;
        const float a = q_tokens[q_base + lo] * q_inv_rms * float(q_norm_weight[lo]);
        const float b = q_tokens[q_base + hi] * q_inv_rms * float(q_norm_weight[hi]);
        q_out = qwen35_08b_prefill_attn_rope_pair(a, b, tid, token);
    } else {
        q_out = q_tokens[q_base + tid] * q_inv_rms * float(q_norm_weight[tid]);
    }
    q_cache[token * q_width + head * head_dim + tid] = half(clamp(q_out, -65504.0f, 65504.0f));

    if (head >= kv_heads) {
        return;
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);
    const uint kv_base = token * kv_width + head * head_dim;
    float k_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float kv = k_tokens[kv_base + d];
        k_ss += kv * kv;
    }
    partial[tid] = k_ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float k_inv_rms = rsqrt(partial[0] / float(head_dim) + eps);

    float k_out;
    if (tid < rope_dim) {
        const uint lo = tid % half_rope_dim;
        const uint hi = lo + half_rope_dim;
        const float a = k_tokens[kv_base + lo] * k_inv_rms * float(k_norm_weight[lo]);
        const float b = k_tokens[kv_base + hi] * k_inv_rms * float(k_norm_weight[hi]);
        k_out = qwen35_08b_prefill_attn_rope_pair(a, b, tid, token);
    } else {
        k_out = k_tokens[kv_base + tid] * k_inv_rms * float(k_norm_weight[tid]);
    }
    k_cache[token * kv_width + head * head_dim + tid] = half(clamp(k_out, -65504.0f, 65504.0f));
    v_cache[token * kv_width + head * head_dim + tid] =
        half(clamp(v_tokens[kv_base + tid], -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_attention_prepare_qk_rope_v_interleaved_gqa8_kv2_d256(
    device const float* q_tokens [[buffer(0)]],
    device const float* k_tokens [[buffer(1)]],
    device const float* v_tokens [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* q_cache [[buffer(5)]],
    device half* kv_cache [[buffer(6)]],
    device half* unused_v_cache [[buffer(7)]],
    constant uint& tokens [[buffer(8)]],
    constant uint& q_rows [[buffer(9)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint rope_dim = 64;
    constexpr uint half_rope_dim = rope_dim / 2;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[256];

    const uint q_base =
        token * q_rows + ((q_rows >= q_width * 2) ? head * head_dim * 2 : head * head_dim);
    float q_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float qv = q_tokens[q_base + d];
        q_ss += qv * qv;
    }
    partial[tid] = q_ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float q_inv_rms = rsqrt(partial[0] / float(head_dim) + eps);

    float q_out;
    if (tid < rope_dim) {
        const uint lo = tid % half_rope_dim;
        const uint hi = lo + half_rope_dim;
        const float a = q_tokens[q_base + lo] * q_inv_rms * float(q_norm_weight[lo]);
        const float b = q_tokens[q_base + hi] * q_inv_rms * float(q_norm_weight[hi]);
        q_out = qwen35_08b_prefill_attn_rope_pair(a, b, tid, token);
    } else {
        q_out = q_tokens[q_base + tid] * q_inv_rms * float(q_norm_weight[tid]);
    }
    q_cache[token * q_width + head * head_dim + tid] = half(clamp(q_out, -65504.0f, 65504.0f));

    if (head >= kv_heads) {
        return;
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);
    const uint kv_base = token * kv_width + head * head_dim;
    float k_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float kv = k_tokens[kv_base + d];
        k_ss += kv * kv;
    }
    partial[tid] = k_ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float k_inv_rms = rsqrt(partial[0] / float(head_dim) + eps);

    float k_out;
    if (tid < rope_dim) {
        const uint lo = tid % half_rope_dim;
        const uint hi = lo + half_rope_dim;
        const float a = k_tokens[kv_base + lo] * k_inv_rms * float(k_norm_weight[lo]);
        const float b = k_tokens[kv_base + hi] * k_inv_rms * float(k_norm_weight[hi]);
        k_out = qwen35_08b_prefill_attn_rope_pair(a, b, tid, token);
    } else {
        k_out = k_tokens[kv_base + tid] * k_inv_rms * float(k_norm_weight[tid]);
    }
    const uint packed_base = (token * kv_width + head * head_dim + tid) * 2;
    kv_cache[packed_base] = half(clamp(k_out, -65504.0f, 65504.0f));
    kv_cache[packed_base + 1] = half(clamp(v_tokens[kv_base + tid], -65504.0f, 65504.0f));
    (void)unused_v_cache;
}

kernel void qwen35_08b_prefill_attention_prepare_qk_rope_v_int8_gqa8_kv2_d256(
    device const float* q_tokens [[buffer(0)]],
    device const float* k_tokens [[buffer(1)]],
    device const float* v_tokens [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* q_cache [[buffer(5)]],
    device char* k_cache_i8 [[buffer(6)]],
    device char* v_cache_i8 [[buffer(7)]],
    constant uint& tokens [[buffer(8)]],
    constant uint& q_rows [[buffer(9)]],
    device half* kv_scale [[buffer(10)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint rope_dim = 64;
    constexpr uint half_rope_dim = rope_dim / 2;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[256];
    threadgroup float k_tmp[256];
    threadgroup float v_tmp[256];

    const uint q_base =
        token * q_rows + ((q_rows >= q_width * 2) ? head * head_dim * 2 : head * head_dim);
    float q_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float qv = q_tokens[q_base + d];
        q_ss += qv * qv;
    }
    partial[tid] = q_ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float q_inv_rms = rsqrt(partial[0] / float(head_dim) + eps);

    float q_out;
    if (tid < rope_dim) {
        const uint lo = tid % half_rope_dim;
        const uint hi = lo + half_rope_dim;
        const float a = q_tokens[q_base + lo] * q_inv_rms * float(q_norm_weight[lo]);
        const float b = q_tokens[q_base + hi] * q_inv_rms * float(q_norm_weight[hi]);
        q_out = qwen35_08b_prefill_attn_rope_pair(a, b, tid, token);
    } else {
        q_out = q_tokens[q_base + tid] * q_inv_rms * float(q_norm_weight[tid]);
    }
    q_cache[token * q_width + head * head_dim + tid] = half(clamp(q_out, -65504.0f, 65504.0f));

    if (head >= kv_heads) {
        return;
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);
    const uint kv_base = token * kv_width + head * head_dim;
    float k_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float kv = k_tokens[kv_base + d];
        k_ss += kv * kv;
    }
    partial[tid] = k_ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float k_inv_rms = rsqrt(partial[0] / float(head_dim) + eps);

    float k_out;
    if (tid < rope_dim) {
        const uint lo = tid % half_rope_dim;
        const uint hi = lo + half_rope_dim;
        const float a = k_tokens[kv_base + lo] * k_inv_rms * float(k_norm_weight[lo]);
        const float b = k_tokens[kv_base + hi] * k_inv_rms * float(k_norm_weight[hi]);
        k_out = qwen35_08b_prefill_attn_rope_pair(a, b, tid, token);
    } else {
        k_out = k_tokens[kv_base + tid] * k_inv_rms * float(k_norm_weight[tid]);
    }
    const float v_out = v_tokens[kv_base + tid];
    k_tmp[tid] = k_out;
    v_tmp[tid] = v_out;
    partial[tid] = max(abs(k_out), abs(v_out));
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] = max(partial[tid], partial[tid + stride]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float scale = max(partial[0] / 127.0f, 1.0e-8f);
    if (tid == 0) {
        kv_scale[token * kv_heads + head] = half(scale);
    }
    const uint out_idx = token * kv_width + head * head_dim + tid;
    const float inv_scale = 1.0f / scale;
    k_cache_i8[out_idx] = char(clamp(rint(k_tmp[tid] * inv_scale), -127.0f, 127.0f));
    v_cache_i8[out_idx] = char(clamp(rint(v_tmp[tid] * inv_scale), -127.0f, 127.0f));
}

kernel void qwen35_08b_prefill_attention_prepare_qk_rope_v_int8_v_gqa8_kv2_d256(
    device const float* q_tokens [[buffer(0)]],
    device const float* k_tokens [[buffer(1)]],
    device const float* v_tokens [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* q_cache [[buffer(5)]],
    device half* k_cache [[buffer(6)]],
    device char* v_cache_i8 [[buffer(7)]],
    constant uint& tokens [[buffer(8)]],
    constant uint& q_rows [[buffer(9)]],
    device half* v_scale [[buffer(10)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint rope_dim = 64;
    constexpr uint half_rope_dim = rope_dim / 2;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[256];
    threadgroup float v_tmp[256];

    const uint q_base =
        token * q_rows + ((q_rows >= q_width * 2) ? head * head_dim * 2 : head * head_dim);
    float q_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float qv = q_tokens[q_base + d];
        q_ss += qv * qv;
    }
    partial[tid] = q_ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float q_inv_rms = rsqrt(partial[0] / float(head_dim) + eps);

    float q_out;
    if (tid < rope_dim) {
        const uint lo = tid % half_rope_dim;
        const uint hi = lo + half_rope_dim;
        const float a = q_tokens[q_base + lo] * q_inv_rms * float(q_norm_weight[lo]);
        const float b = q_tokens[q_base + hi] * q_inv_rms * float(q_norm_weight[hi]);
        q_out = qwen35_08b_prefill_attn_rope_pair(a, b, tid, token);
    } else {
        q_out = q_tokens[q_base + tid] * q_inv_rms * float(q_norm_weight[tid]);
    }
    q_cache[token * q_width + head * head_dim + tid] = half(clamp(q_out, -65504.0f, 65504.0f));

    if (head >= kv_heads) {
        return;
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);
    const uint kv_base = token * kv_width + head * head_dim;
    float k_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float kv = k_tokens[kv_base + d];
        k_ss += kv * kv;
    }
    partial[tid] = k_ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float k_inv_rms = rsqrt(partial[0] / float(head_dim) + eps);

    float k_out;
    if (tid < rope_dim) {
        const uint lo = tid % half_rope_dim;
        const uint hi = lo + half_rope_dim;
        const float a = k_tokens[kv_base + lo] * k_inv_rms * float(k_norm_weight[lo]);
        const float b = k_tokens[kv_base + hi] * k_inv_rms * float(k_norm_weight[hi]);
        k_out = qwen35_08b_prefill_attn_rope_pair(a, b, tid, token);
    } else {
        k_out = k_tokens[kv_base + tid] * k_inv_rms * float(k_norm_weight[tid]);
    }
    k_cache[token * kv_width + head * head_dim + tid] = half(clamp(k_out, -65504.0f, 65504.0f));

    const float v_out = v_tokens[kv_base + tid];
    v_tmp[tid] = v_out;
    partial[tid] = abs(v_out);
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] = max(partial[tid], partial[tid + stride]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    const float scale = max(partial[0] / 127.0f, 1.0e-8f);
    if (tid == 0) {
        v_scale[token * kv_heads + head] = half(scale);
    }
    const float inv_scale = 1.0f / scale;
    v_cache_i8[token * kv_width + head * head_dim + tid] =
        char(clamp(rint(v_tmp[tid] * inv_scale), -127.0f, 127.0f));
}

kernel void qwen35_08b_prefill_attention_causal_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint token = tg_pos.x;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || q_head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[256];

    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint q_base = token * q_rows +
        ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
    const uint q_cache_base = token * q_width + q_head * head_dim;
    const uint out_base = token * q_width + q_head * head_dim;
    const float q_value = float(q_cache[q_cache_base + tid]);

    float gate = 1.0f;
    if (q_rows >= q_width * 2) {
        gate = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
    }

    float m = -3.402823466e38f;
    float l = 0.0f;
    float acc = 0.0f;

    for (uint t = 0; t <= token; ++t) {
        const uint kv_base = t * kv_width + kv_head * head_dim;
        partial[tid] = q_value * float(k_cache[kv_base + tid]);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint stride = 128; stride > 0; stride >>= 1) {
            if (tid < stride) {
                partial[tid] += partial[tid + stride];
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }

        const float score = partial[0] * inv_sqrt_head_dim;
        const float m_next = max(m, score);
        const float alpha = exp(m - m_next);
        const float beta = exp(score - m_next);
        acc = acc * alpha + float(v_cache[kv_base + tid]) * beta;
        l = l * alpha + beta;
        m = m_next;
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float normalized = (l > 0.0f) ? (acc / l) : 0.0f;
    out[out_base + tid] = half(clamp(normalized * gate, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_attention_causal_simdreduce_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint token = tg_pos.x;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || q_head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[simdgroups_per_tg];

    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint q_base = token * q_rows +
        ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
    const uint q_cache_base = token * q_width + q_head * head_dim;
    const uint out_base = token * q_width + q_head * head_dim;
    const float q_value = float(q_cache[q_cache_base + tid]);

    float gate = 1.0f;
    if (q_rows >= q_width * 2) {
        gate = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
    }

    float m = -3.402823466e38f;
    float l = 0.0f;
    float acc = 0.0f;

    for (uint t = 0; t <= token; ++t) {
        const uint kv_base = t * kv_width + kv_head * head_dim;
        float score_part = q_value * float(k_cache[kv_base + tid]);
        score_part = simd_sum(score_part);
        if (simd_lane == 0) {
            partial[simd_group] = score_part;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        float score = 0.0f;
        if (simd_group == 0) {
            score = (tid < simdgroups_per_tg) ? partial[tid] : 0.0f;
            score = simd_sum(score);
            if (simd_lane == 0) {
                partial[0] = score;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        score = partial[0] * inv_sqrt_head_dim;
        const float m_next = max(m, score);
        const float alpha = exp(m - m_next);
        const float beta = exp(score - m_next);
        acc = acc * alpha + float(v_cache[kv_base + tid]) * beta;
        l = l * alpha + beta;
        m = m_next;
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float normalized = (l > 0.0f) ? (acc / l) : 0.0f;
    out[out_base + tid] = half(clamp(normalized * gate, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_attention_causal_qblk4_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 4;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (query_start >= tokens || q_head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[query_block][head_dim];

    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[query_block];
    float gate[query_block];
    float m[query_block];
    float l[query_block];
    float acc[query_block];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        const uint query = query_start + qi;
        active[qi] = query < tokens;
        m[qi] = -3.402823466e38f;
        l[qi] = 0.0f;
        acc[qi] = 0.0f;
        q_value[qi] = 0.0f;
        gate[qi] = 1.0f;
        if (active[qi]) {
            const uint q_cache_base = query * q_width + q_head * head_dim;
            const uint q_base = query * q_rows +
                ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
            q_value[qi] = float(q_cache[q_cache_base + tid]);
            if (q_rows >= q_width * 2) {
                gate[qi] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        for (uint qi = 0; qi < query_block; ++qi) {
            const uint query = query_start + qi;
            if (active[qi] && key <= query) {
                partial[qi][tid] = q_value[qi] * k_value;
            } else {
                partial[qi][tid] = 0.0f;
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);

            for (uint stride = 128; stride > 0; stride >>= 1) {
                if (tid < stride) {
                    partial[qi][tid] += partial[qi][tid + stride];
                }
                threadgroup_barrier(mem_flags::mem_threadgroup);
            }

            if (active[qi] && key <= query) {
                const float score = partial[qi][0] * inv_sqrt_head_dim;
                const float m_next = max(m[qi], score);
                const float alpha = exp(m[qi] - m_next);
                const float beta = exp(score - m_next);
                acc[qi] = acc[qi] * alpha + v_value * beta;
                l[qi] = l[qi] * alpha + beta;
                m[qi] = m_next;
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }
    }

    for (uint qi = 0; qi < query_block; ++qi) {
        if (active[qi]) {
            const uint query = query_start + qi;
            const uint out_base = query * q_width + q_head * head_dim;
            const float normalized = (l[qi] > 0.0f) ? (acc[qi] / l[qi]) : 0.0f;
            out[out_base + tid] = half(clamp(normalized * gate[qi], -65504.0f, 65504.0f));
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qblk2_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 2;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (query_start >= tokens || q_head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[query_block][head_dim];

    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[query_block];
    float gate[query_block];
    float m[query_block];
    float l[query_block];
    float acc[query_block];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        const uint query = query_start + qi;
        active[qi] = query < tokens;
        m[qi] = -3.402823466e38f;
        l[qi] = 0.0f;
        acc[qi] = 0.0f;
        q_value[qi] = 0.0f;
        gate[qi] = 1.0f;
        if (active[qi]) {
            const uint q_cache_base = query * q_width + q_head * head_dim;
            const uint q_base = query * q_rows +
                ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
            q_value[qi] = float(q_cache[q_cache_base + tid]);
            if (q_rows >= q_width * 2) {
                gate[qi] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        for (uint qi = 0; qi < query_block; ++qi) {
            const uint query = query_start + qi;
            if (active[qi] && key <= query) {
                partial[qi][tid] = q_value[qi] * k_value;
            } else {
                partial[qi][tid] = 0.0f;
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);

            for (uint stride = 128; stride > 0; stride >>= 1) {
                if (tid < stride) {
                    partial[qi][tid] += partial[qi][tid + stride];
                }
                threadgroup_barrier(mem_flags::mem_threadgroup);
            }

            if (active[qi] && key <= query) {
                const float score = partial[qi][0] * inv_sqrt_head_dim;
                const float m_next = max(m[qi], score);
                const float alpha = exp(m[qi] - m_next);
                const float beta = exp(score - m_next);
                acc[qi] = acc[qi] * alpha + v_value * beta;
                l[qi] = l[qi] * alpha + beta;
                m[qi] = m_next;
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }
    }

    for (uint qi = 0; qi < query_block; ++qi) {
        if (active[qi]) {
            const uint query = query_start + qi;
            const uint out_base = query * q_width + q_head * head_dim;
            const float normalized = (l[qi] > 0.0f) ? (acc[qi] / l[qi]) : 0.0f;
            out[out_base + tid] = half(clamp(normalized * gate[qi], -65504.0f, 65504.0f));
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qblk2_simdreduce_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 2;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (query_start >= tokens || q_head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[query_block][simdgroups_per_tg];

    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[query_block];
    float gate[query_block];
    float m[query_block];
    float l[query_block];
    float acc[query_block];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        const uint query = query_start + qi;
        active[qi] = query < tokens;
        m[qi] = -3.402823466e38f;
        l[qi] = 0.0f;
        acc[qi] = 0.0f;
        q_value[qi] = 0.0f;
        gate[qi] = 1.0f;
        if (active[qi]) {
            const uint q_cache_base = query * q_width + q_head * head_dim;
            const uint q_base = query * q_rows +
                ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
            q_value[qi] = float(q_cache[q_cache_base + tid]);
            if (q_rows >= q_width * 2) {
                gate[qi] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        for (uint qi = 0; qi < query_block; ++qi) {
            const uint query = query_start + qi;
            float score_part = 0.0f;
            if (active[qi] && key <= query) {
                score_part = q_value[qi] * k_value;
            }
            score_part = simd_sum(score_part);
            if (simd_lane == 0) {
                partial[qi][simd_group] = score_part;
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);

            float score = 0.0f;
            if (simd_group == 0) {
                score = (tid < simdgroups_per_tg) ? partial[qi][tid] : 0.0f;
                score = simd_sum(score);
                if (simd_lane == 0) {
                    partial[qi][0] = score;
                }
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);

            if (active[qi] && key <= query) {
                score = partial[qi][0] * inv_sqrt_head_dim;
                const float m_next = max(m[qi], score);
                const float alpha = exp(m[qi] - m_next);
                const float beta = exp(score - m_next);
                acc[qi] = acc[qi] * alpha + v_value * beta;
                l[qi] = l[qi] * alpha + beta;
                m[qi] = m_next;
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }
    }

    for (uint qi = 0; qi < query_block; ++qi) {
        if (active[qi]) {
            const uint query = query_start + qi;
            const uint out_base = query * q_width + q_head * head_dim;
            const float normalized = (l[qi] > 0.0f) ? (acc[qi] / l[qi]) : 0.0f;
            out[out_base + tid] = half(clamp(normalized * gate[qi], -65504.0f, 65504.0f));
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qblk4_simdreduce_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 4;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (query_start >= tokens || q_head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[query_block][simdgroups_per_tg];

    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[query_block];
    float gate[query_block];
    float m[query_block];
    float l[query_block];
    float acc[query_block];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        const uint query = query_start + qi;
        active[qi] = query < tokens;
        m[qi] = -3.402823466e38f;
        l[qi] = 0.0f;
        acc[qi] = 0.0f;
        q_value[qi] = 0.0f;
        gate[qi] = 1.0f;
        if (active[qi]) {
            const uint q_cache_base = query * q_width + q_head * head_dim;
            const uint q_base = query * q_rows +
                ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
            q_value[qi] = float(q_cache[q_cache_base + tid]);
            if (q_rows >= q_width * 2) {
                gate[qi] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        for (uint qi = 0; qi < query_block; ++qi) {
            const uint query = query_start + qi;
            float score_part = 0.0f;
            if (active[qi] && key <= query) {
                score_part = q_value[qi] * k_value;
            }
            score_part = simd_sum(score_part);
            if (simd_lane == 0) {
                partial[qi][simd_group] = score_part;
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);

            float score = 0.0f;
            if (simd_group == 0) {
                score = (tid < simdgroups_per_tg) ? partial[qi][tid] : 0.0f;
                score = simd_sum(score);
                if (simd_lane == 0) {
                    partial[qi][0] = score;
                }
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);

            if (active[qi] && key <= query) {
                score = partial[qi][0] * inv_sqrt_head_dim;
                const float m_next = max(m[qi], score);
                const float alpha = exp(m[qi] - m_next);
                const float beta = exp(score - m_next);
                acc[qi] = acc[qi] * alpha + v_value * beta;
                l[qi] = l[qi] * alpha + beta;
                m[qi] = m_next;
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }
    }

    for (uint qi = 0; qi < query_block; ++qi) {
        if (active[qi]) {
            const uint query = query_start + qi;
            const uint out_base = query * q_width + q_head * head_dim;
            const float normalized = (l[qi] > 0.0f) ? (acc[qi] / l[qi]) : 0.0f;
            out[out_base + tid] = half(clamp(normalized * gate[qi], -65504.0f, 65504.0f));
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qblk4_simdreduce_batch_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 4;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (query_start >= tokens || q_head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[query_block][simdgroups_per_tg];

    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[query_block];
    float gate[query_block];
    float m[query_block];
    float l[query_block];
    float acc[query_block];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        const uint query = query_start + qi;
        active[qi] = query < tokens;
        m[qi] = -3.402823466e38f;
        l[qi] = 0.0f;
        acc[qi] = 0.0f;
        q_value[qi] = 0.0f;
        gate[qi] = 1.0f;
        if (active[qi]) {
            const uint q_cache_base = query * q_width + q_head * head_dim;
            const uint q_base = query * q_rows +
                ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
            q_value[qi] = float(q_cache[q_cache_base + tid]);
            if (q_rows >= q_width * 2) {
                gate[qi] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        float score_part[query_block];
        for (uint qi = 0; qi < query_block; ++qi) {
            const uint query = query_start + qi;
            score_part[qi] = 0.0f;
            if (active[qi] && key <= query) {
                score_part[qi] = q_value[qi] * k_value;
            }
            score_part[qi] = simd_sum(score_part[qi]);
            if (simd_lane == 0) {
                partial[qi][simd_group] = score_part[qi];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        float score[query_block];
        for (uint qi = 0; qi < query_block; ++qi) {
            score[qi] = 0.0f;
            if (simd_group == 0) {
                score[qi] = (tid < simdgroups_per_tg) ? partial[qi][tid] : 0.0f;
                score[qi] = simd_sum(score[qi]);
                if (simd_lane == 0) {
                    partial[qi][0] = score[qi];
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint qi = 0; qi < query_block; ++qi) {
            const uint query = query_start + qi;
            if (active[qi] && key <= query) {
                score[qi] = partial[qi][0] * inv_sqrt_head_dim;
                const float m_next = max(m[qi], score[qi]);
                const float alpha = exp(m[qi] - m_next);
                const float beta = exp(score[qi] - m_next);
                acc[qi] = acc[qi] * alpha + v_value * beta;
                l[qi] = l[qi] * alpha + beta;
                m[qi] = m_next;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint qi = 0; qi < query_block; ++qi) {
        if (active[qi]) {
            const uint query = query_start + qi;
            const uint out_base = query * q_width + q_head * head_dim;
            const float normalized = (l[qi] > 0.0f) ? (acc[qi] / l[qi]) : 0.0f;
            out[out_base + tid] = half(clamp(normalized * gate[qi], -65504.0f, 65504.0f));
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qblk8_simdreduce_batch_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 8;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (query_start >= tokens || q_head >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[query_block][simdgroups_per_tg];

    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[query_block];
    float gate[query_block];
    float m[query_block];
    float l[query_block];
    float acc[query_block];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        const uint query = query_start + qi;
        active[qi] = query < tokens;
        m[qi] = -3.402823466e38f;
        l[qi] = 0.0f;
        acc[qi] = 0.0f;
        q_value[qi] = 0.0f;
        gate[qi] = 1.0f;
        if (active[qi]) {
            const uint q_cache_base = query * q_width + q_head * head_dim;
            const uint q_base = query * q_rows +
                ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
            q_value[qi] = float(q_cache[q_cache_base + tid]);
            if (q_rows >= q_width * 2) {
                gate[qi] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        float score_part[query_block];
        for (uint qi = 0; qi < query_block; ++qi) {
            const uint query = query_start + qi;
            score_part[qi] = 0.0f;
            if (active[qi] && key <= query) {
                score_part[qi] = q_value[qi] * k_value;
            }
            score_part[qi] = simd_sum(score_part[qi]);
            if (simd_lane == 0) {
                partial[qi][simd_group] = score_part[qi];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        float score[query_block];
        for (uint qi = 0; qi < query_block; ++qi) {
            score[qi] = 0.0f;
            if (simd_group == 0) {
                score[qi] = (tid < simdgroups_per_tg) ? partial[qi][tid] : 0.0f;
                score[qi] = simd_sum(score[qi]);
                if (simd_lane == 0) {
                    partial[qi][0] = score[qi];
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint qi = 0; qi < query_block; ++qi) {
            const uint query = query_start + qi;
            if (active[qi] && key <= query) {
                score[qi] = partial[qi][0] * inv_sqrt_head_dim;
                const float m_next = max(m[qi], score[qi]);
                const float alpha = exp(m[qi] - m_next);
                const float beta = exp(score[qi] - m_next);
                acc[qi] = acc[qi] * alpha + v_value * beta;
                l[qi] = l[qi] * alpha + beta;
                m[qi] = m_next;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint qi = 0; qi < query_block; ++qi) {
        if (active[qi]) {
            const uint query = query_start + qi;
            const uint out_base = query * q_width + q_head * head_dim;
            const float normalized = (l[qi] > 0.0f) ? (acc[qi] / l[qi]) : 0.0f;
            out[out_base + tid] = half(clamp(normalized * gate[qi], -65504.0f, 65504.0f));
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh2_qblk4_simdreduce_batch_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 4;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint tid = tid_pos.x;
    if (query_start >= tokens || q_head_start >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[heads_per_group][query_block][simdgroups_per_tg];

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[heads_per_group][query_block];
    float gate[heads_per_group][query_block];
    float m[heads_per_group][query_block];
    float l[heads_per_group][query_block];
    float acc[heads_per_group][query_block];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        const uint query = query_start + qi;
        active[qi] = query < tokens;
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        for (uint qi = 0; qi < query_block; ++qi) {
            m[hp][qi] = -3.402823466e38f;
            l[hp][qi] = 0.0f;
            acc[hp][qi] = 0.0f;
            q_value[hp][qi] = 0.0f;
            gate[hp][qi] = 1.0f;
            if (active[qi] && q_head < q_heads) {
                const uint query = query_start + qi;
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][qi] = float(q_cache[q_cache_base + tid]);
                if (q_rows >= q_width * 2) {
                    gate[hp][qi] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
                }
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        float score_part[heads_per_group][query_block];
        for (uint hp = 0; hp < heads_per_group; ++hp) {
            for (uint qi = 0; qi < query_block; ++qi) {
                const uint query = query_start + qi;
                score_part[hp][qi] = 0.0f;
                if (active[qi] && key <= query) {
                    score_part[hp][qi] = q_value[hp][qi] * k_value;
                }
                score_part[hp][qi] = simd_sum(score_part[hp][qi]);
                if (simd_lane == 0) {
                    partial[hp][qi][simd_group] = score_part[hp][qi];
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        float score[heads_per_group][query_block];
        for (uint hp = 0; hp < heads_per_group; ++hp) {
            for (uint qi = 0; qi < query_block; ++qi) {
                score[hp][qi] = 0.0f;
                if (simd_group == 0) {
                    score[hp][qi] = (tid < simdgroups_per_tg) ? partial[hp][qi][tid] : 0.0f;
                    score[hp][qi] = simd_sum(score[hp][qi]);
                    if (simd_lane == 0) {
                        partial[hp][qi][0] = score[hp][qi];
                    }
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            for (uint qi = 0; qi < query_block; ++qi) {
                const uint query = query_start + qi;
                if (active[qi] && key <= query) {
                    score[hp][qi] = partial[hp][qi][0] * inv_sqrt_head_dim;
                    const float m_next = max(m[hp][qi], score[hp][qi]);
                    const float alpha = exp(m[hp][qi] - m_next);
                    const float beta = exp(score[hp][qi] - m_next);
                    acc[hp][qi] = acc[hp][qi] * alpha + v_value * beta;
                    l[hp][qi] = l[hp][qi] * alpha + beta;
                    m[hp][qi] = m_next;
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            for (uint qi = 0; qi < query_block; ++qi) {
                if (active[qi]) {
                    const uint query = query_start + qi;
                    const uint out_base = query * q_width + q_head * head_dim;
                    const float normalized = (l[hp][qi] > 0.0f) ? (acc[hp][qi] / l[hp][qi]) : 0.0f;
                    out[out_base + tid] = half(clamp(normalized * gate[hp][qi], -65504.0f, 65504.0f));
                }
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk2_simdreduce_batch_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 2;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint tid = tid_pos.x;
    if (query_start >= tokens || q_head_start >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[heads_per_group][query_block][simdgroups_per_tg];

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[heads_per_group][query_block];
    float gate[heads_per_group][query_block];
    float m[heads_per_group][query_block];
    float l[heads_per_group][query_block];
    float acc[heads_per_group][query_block];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        const uint query = query_start + qi;
        active[qi] = query < tokens;
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        for (uint qi = 0; qi < query_block; ++qi) {
            m[hp][qi] = -3.402823466e38f;
            l[hp][qi] = 0.0f;
            acc[hp][qi] = 0.0f;
            q_value[hp][qi] = 0.0f;
            gate[hp][qi] = 1.0f;
            if (active[qi] && q_head < q_heads) {
                const uint query = query_start + qi;
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][qi] = float(q_cache[q_cache_base + tid]);
                if (q_rows >= q_width * 2) {
                    gate[hp][qi] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
                }
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        float score_part[heads_per_group][query_block];
        for (uint hp = 0; hp < heads_per_group; ++hp) {
            for (uint qi = 0; qi < query_block; ++qi) {
                const uint query = query_start + qi;
                score_part[hp][qi] = 0.0f;
                if (active[qi] && key <= query) {
                    score_part[hp][qi] = q_value[hp][qi] * k_value;
                }
                score_part[hp][qi] = simd_sum(score_part[hp][qi]);
                if (simd_lane == 0) {
                    partial[hp][qi][simd_group] = score_part[hp][qi];
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        float score[heads_per_group][query_block];
        for (uint hp = 0; hp < heads_per_group; ++hp) {
            for (uint qi = 0; qi < query_block; ++qi) {
                score[hp][qi] = 0.0f;
                if (simd_group == 0) {
                    score[hp][qi] = (tid < simdgroups_per_tg) ? partial[hp][qi][tid] : 0.0f;
                    score[hp][qi] = simd_sum(score[hp][qi]);
                    if (simd_lane == 0) {
                        partial[hp][qi][0] = score[hp][qi];
                    }
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            for (uint qi = 0; qi < query_block; ++qi) {
                const uint query = query_start + qi;
                if (active[qi] && key <= query) {
                    score[hp][qi] = partial[hp][qi][0] * inv_sqrt_head_dim;
                    const float m_next = max(m[hp][qi], score[hp][qi]);
                    const float alpha = exp(m[hp][qi] - m_next);
                    const float beta = exp(score[hp][qi] - m_next);
                    acc[hp][qi] = acc[hp][qi] * alpha + v_value * beta;
                    l[hp][qi] = l[hp][qi] * alpha + beta;
                    m[hp][qi] = m_next;
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            for (uint qi = 0; qi < query_block; ++qi) {
                if (active[qi]) {
                    const uint query = query_start + qi;
                    const uint out_base = query * q_width + q_head * head_dim;
                    const float normalized = (l[hp][qi] > 0.0f) ? (acc[hp][qi] / l[hp][qi]) : 0.0f;
                    out[out_base + tid] = half(clamp(normalized * gate[hp][qi], -65504.0f, 65504.0f));
                }
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simdreduce_batch_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint tid = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[heads_per_group][simdgroups_per_tg];

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    float q_value[heads_per_group];
    float gate[heads_per_group];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        acc[hp] = 0.0f;
        q_value[hp] = 0.0f;
        gate[hp] = 1.0f;
        if (q_head < q_heads) {
            const uint q_cache_base = query * q_width + q_head * head_dim;
            const uint q_base = query * q_rows +
                ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
            q_value[hp] = float(q_cache[q_cache_base + tid]);
            if (q_rows >= q_width * 2) {
                gate[hp] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + tid]));
            }
        }
    }

    for (uint key = 0; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float k_value = float(k_cache[kv_base + tid]);
        const float v_value = float(v_cache[kv_base + tid]);

        float score_part[heads_per_group];
        for (uint hp = 0; hp < heads_per_group; ++hp) {
            score_part[hp] = q_value[hp] * k_value;
            score_part[hp] = simd_sum(score_part[hp]);
            if (simd_lane == 0) {
                partial[hp][simd_group] = score_part[hp];
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        float score[heads_per_group];
        for (uint hp = 0; hp < heads_per_group; ++hp) {
            score[hp] = 0.0f;
            if (simd_group == 0) {
                score[hp] = (tid < simdgroups_per_tg) ? partial[hp][tid] : 0.0f;
                score[hp] = simd_sum(score[hp]);
                if (simd_lane == 0) {
                    partial[hp][0] = score[hp];
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            score[hp] = partial[hp][0] * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score[hp]);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score[hp] - m_next);
            acc[hp] = acc[hp] * alpha + v_value * beta;
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            const float normalized = (l[hp] > 0.0f) ? (acc[hp] / l[hp]) : 0.0f;
            out[out_base + tid] = half(clamp(normalized * gate[hp], -65504.0f, 65504.0f));
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    float q_value[heads_per_group][lane_dims];
    float gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            gate[hp][c] = 1.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim]));
                }
            }
        }
    }

    for (uint key = 0; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache[kv_base + dim]);
            v_value[c] = float(v_cache[kv_base + dim]);
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (acc[hp][c] / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * gate[hp][c], -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_interleaved_kv_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* kv_cache [[buffer(1)]],
    device const half* unused_v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    float q_value[heads_per_group][lane_dims];
    float gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            gate[hp][c] = 1.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim]));
                }
            }
        }
    }

    for (uint key = 0; key <= query; ++key) {
        const uint kv_base = (key * kv_width + kv_head * head_dim) * 2;
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            const uint packed_idx = kv_base + dim * 2;
            k_value[c] = float(kv_cache[packed_idx]);
            v_value[c] = float(kv_cache[packed_idx + 1]);
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (acc[hp][c] / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * gate[hp][c], -65504.0f, 65504.0f));
            }
        }
    }
    (void)unused_v_cache;
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_kv_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const char* k_cache_i8 [[buffer(1)]],
    device const char* v_cache_i8 [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    device const half* kv_scale [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    float q_value[heads_per_group][lane_dims];
    float gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            gate[hp][c] = 1.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim]));
                }
            }
        }
    }

    for (uint key = 0; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float scale = float(kv_scale[key * kv_heads + kv_head]);
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache_i8[kv_base + dim]) * scale;
            v_value[c] = float(v_cache_i8[kv_base + dim]) * scale;
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (acc[hp][c] / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * gate[hp][c], -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_v_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const char* v_cache_i8 [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    device const half* v_scale [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    float q_value[heads_per_group][lane_dims];
    float gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            gate[hp][c] = 1.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim]));
                }
            }
        }
    }

    for (uint key = 0; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float scale = float(v_scale[key * kv_heads + kv_head]);
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache[kv_base + dim]);
            v_value[c] = float(v_cache_i8[kv_base + dim]) * scale;
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (acc[hp][c] / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * gate[hp][c], -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_v_pack4_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const uint* v_cache_i8x4 [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    device const half* v_scale [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);
    const uint lane_source = lane & ~3u;
    const uint byte_shift = (lane & 3u) * 8u;

    float q_value[heads_per_group][lane_dims];
    float gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            gate[hp][c] = 1.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim]));
                }
            }
        }
    }

    for (uint key = 0; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        const float scale = float(v_scale[key * kv_heads + kv_head]);
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache[kv_base + dim]);
            uint packed = 0u;
            if ((lane & 3u) == 0u) {
                packed = v_cache_i8x4[(kv_base + dim) >> 2];
            }
            packed = simd_broadcast(packed, lane_source);
            int raw = int((packed >> byte_shift) & 0xffu);
            if (raw >= 128) {
                raw -= 256;
            }
            v_value[c] = float(raw) * scale;
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (acc[hp][c] / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * gate[hp][c], -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_halfacc_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    half q_value[heads_per_group][lane_dims];
    half gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    half acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = half(0.0);
            gate[hp][c] = half(1.0);
            acc[hp][c] = half(0.0);
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = q_cache[q_cache_base + dim];
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = half(1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim])));
                }
            }
        }
    }

    for (uint key = 0; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        half k_value[lane_dims];
        half v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = k_cache[kv_base + dim];
            v_value[c] = v_cache[kv_base + dim];
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += float(q_value[hp][c]) * float(k_value[c]);
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                const float next_acc = float(acc[hp][c]) * alpha + float(v_value[c]) * beta;
                acc[hp][c] = half(clamp(next_acc, -65504.0f, 65504.0f));
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (float(acc[hp][c]) / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * float(gate[hp][c]), -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_halfdot_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    half q_value[heads_per_group][lane_dims];
    half gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    half acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = half(0.0);
            gate[hp][c] = half(1.0);
            acc[hp][c] = half(0.0);
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = q_cache[q_cache_base + dim];
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = half(1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim])));
                }
            }
        }
    }

    for (uint key = 0; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        half k_value[lane_dims];
        half v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = k_cache[kv_base + dim];
            v_value[c] = v_cache[kv_base + dim];
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            half score_part = half(0.0);
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = float(simd_sum(score_part)) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                const float next_acc = float(acc[hp][c]) * alpha + float(v_value[c]) * beta;
                acc[hp][c] = half(clamp(next_acc, -65504.0f, 65504.0f));
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (float(acc[hp][c]) / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * float(gate[hp][c]), -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk2_simd32_vec8_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint query_block = 2;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query_start >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value[heads_per_group][query_block][lane_dims];
    float gate[heads_per_group][query_block][lane_dims];
    float m[heads_per_group][query_block];
    float l[heads_per_group][query_block];
    float acc[heads_per_group][query_block][lane_dims];
    bool active[query_block];

    for (uint qi = 0; qi < query_block; ++qi) {
        active[qi] = (query_start + qi) < tokens;
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        for (uint qi = 0; qi < query_block; ++qi) {
            m[hp][qi] = -3.402823466e38f;
            l[hp][qi] = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                q_value[hp][qi][c] = 0.0f;
                gate[hp][qi][c] = 1.0f;
                acc[hp][qi][c] = 0.0f;
                if (active[qi] && q_head < q_heads) {
                    const uint query = query_start + qi;
                    const uint q_cache_base = query * q_width + q_head * head_dim;
                    const uint q_base = query * q_rows +
                        ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                    q_value[hp][qi][c] = float(q_cache[q_cache_base + dim]);
                    if (q_rows >= q_width * 2) {
                        gate[hp][qi][c] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim]));
                    }
                }
            }
        }
    }

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache[kv_base + dim]);
            v_value[c] = float(v_cache[kv_base + dim]);
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            for (uint qi = 0; qi < query_block; ++qi) {
                const uint query = query_start + qi;
                if (active[qi] && key <= query) {
                    float score_part = 0.0f;
                    for (uint c = 0; c < lane_dims; ++c) {
                        score_part += q_value[hp][qi][c] * k_value[c];
                    }
                    const float score = simd_sum(score_part) * inv_sqrt_head_dim;
                    const float m_next = max(m[hp][qi], score);
                    const float alpha = exp(m[hp][qi] - m_next);
                    const float beta = exp(score - m_next);
                    for (uint c = 0; c < lane_dims; ++c) {
                        acc[hp][qi][c] = acc[hp][qi][c] * alpha + v_value[c] * beta;
                    }
                    l[hp][qi] = l[hp][qi] * alpha + beta;
                    m[hp][qi] = m_next;
                }
            }
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            for (uint qi = 0; qi < query_block; ++qi) {
                if (active[qi]) {
                    const uint query = query_start + qi;
                    const uint out_base = query * q_width + q_head * head_dim;
                    for (uint c = 0; c < lane_dims; ++c) {
                        const uint dim = lane + c * 32;
                        const float normalized = (l[hp][qi] > 0.0f) ? (acc[hp][qi][c] / l[hp][qi]) : 0.0f;
                        out[out_base + dim] = half(clamp(normalized * gate[hp][qi][c], -65504.0f, 65504.0f));
                    }
                }
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_win4096_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint window = 4096;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);
    const uint key_start = (query + 1 > window) ? (query + 1 - window) : 0;

    float q_value[heads_per_group][lane_dims];
    float gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            gate[hp][c] = 1.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim]));
                }
            }
        }
    }

    for (uint key = key_start; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache[kv_base + dim]);
            v_value[c] = float(v_cache[kv_base + dim]);
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (acc[hp][c] / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * gate[hp][c], -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_window_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    constant uint& window [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32 || window == 0) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);
    const uint key_start = (query + 1 > window) ? (query + 1 - window) : 0;

    float q_value[heads_per_group][lane_dims];
    float gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            gate[hp][c] = 1.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim]));
                }
            }
        }
    }

    for (uint key = key_start; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache[kv_base + dim]);
            v_value[c] = float(v_cache[kv_base + dim]);
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (acc[hp][c] / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * gate[hp][c], -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_window_halfdot_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    constant uint& window [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32 || window == 0) {
        return;
    }

    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);
    const uint key_start = (query + 1 > window) ? (query + 1 - window) : 0;

    half q_value[heads_per_group][lane_dims];
    half gate[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    half acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = half(0.0);
            gate[hp][c] = half(1.0);
            acc[hp][c] = half(0.0);
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                const uint q_base = query * q_rows +
                    ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
                q_value[hp][c] = q_cache[q_cache_base + dim];
                if (q_rows >= q_width * 2) {
                    gate[hp][c] = half(1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + dim])));
                }
            }
        }
    }

    for (uint key = key_start; key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        half k_value[lane_dims];
        half v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = k_cache[kv_base + dim];
            v_value[c] = v_cache[kv_base + dim];
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            half score_part = half(0.0);
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = float(simd_sum(score_part)) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                const float next_acc = float(acc[hp][c]) * alpha + float(v_value[c]) * beta;
                acc[hp][c] = half(clamp(next_acc, -65504.0f, 65504.0f));
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint out_base = query * q_width + q_head * head_dim;
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                const float normalized = (l[hp] > 0.0f) ? (float(acc[hp][c]) / l[hp]) : 0.0f;
                out[out_base + dim] = half(clamp(normalized * float(gate[hp][c]), -65504.0f, 65504.0f));
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_causal_qblk2x512_gqa8_kv2_d256_to_fp16(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 2;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head = tg_pos.y;
    const uint tid = tid_pos.x;
    const uint qi = tid / head_dim;
    const uint lane = tid - qi * head_dim;
    if (query_start >= tokens || q_head >= q_heads) {
        return;
    }

    threadgroup float partial0[head_dim];
    threadgroup float partial1[head_dim];
    threadgroup float k_vec[head_dim];
    threadgroup float v_vec[head_dim];

    const uint query = query_start + qi;
    const bool active = qi < query_block && query < tokens;
    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint last_query = min(query_start + query_block - 1, tokens - 1);

    float q_value = 0.0f;
    float gate = 1.0f;
    if (active) {
        const uint q_cache_base = query * q_width + q_head * head_dim;
        const uint q_base = query * q_rows +
            ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
        q_value = float(q_cache[q_cache_base + lane]);
        if (q_rows >= q_width * 2) {
            gate = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + lane]));
        }
    }

    float m = -3.402823466e38f;
    float l = 0.0f;
    float acc = 0.0f;

    for (uint key = 0; key <= last_query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        if (qi == 0) {
            k_vec[lane] = float(k_cache[kv_base + lane]);
            v_vec[lane] = float(v_cache[kv_base + lane]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        if (active && key <= query) {
            if (qi == 0) {
                partial0[lane] = q_value * k_vec[lane];
            } else {
                partial1[lane] = q_value * k_vec[lane];
            }
        } else {
            if (qi == 0) {
                partial0[lane] = 0.0f;
            } else {
                partial1[lane] = 0.0f;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint stride = 128; stride > 0; stride >>= 1) {
            if (lane < stride) {
                if (qi == 0) {
                    partial0[lane] += partial0[lane + stride];
                } else {
                    partial1[lane] += partial1[lane + stride];
                }
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }

        if (active && key <= query) {
            const float dot = (qi == 0) ? partial0[0] : partial1[0];
            const float score = dot * inv_sqrt_head_dim;
            const float m_next = max(m, score);
            const float alpha = exp(m - m_next);
            const float beta = exp(score - m_next);
            acc = acc * alpha + v_vec[lane] * beta;
            l = l * alpha + beta;
            m = m_next;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (active) {
        const uint out_base = query * q_width + q_head * head_dim;
        const float normalized = (l > 0.0f) ? (acc / l) : 0.0f;
        out[out_base + lane] = half(clamp(normalized * gate, -65504.0f, 65504.0f));
    }
}

kernel void qwen35_08b_prefill_attention_partial_qblk2_kblk64_gqa8_kv2_d256(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device float* partial_m [[buffer(3)]],
    device float* partial_l [[buffer(4)]],
    device float* partial_acc [[buffer(5)]],
    constant uint& tokens [[buffer(6)]],
    constant uint& n_key_blocks [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr uint query_block = 2;
    constexpr uint key_block = 64;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query_start = tg_pos.x * query_block;
    const uint q_head = tg_pos.y;
    const uint key_block_id = tg_pos.z;
    const uint tid = tid_pos.x;
    const uint qi = tid / head_dim;
    const uint lane = tid - qi * head_dim;
    if (query_start >= tokens || q_head >= q_heads || qi >= query_block) {
        return;
    }

    const uint last_query = min(query_start + query_block - 1, tokens - 1);
    const uint key_start = key_block_id * key_block;
    if (key_start > last_query) {
        return;
    }

    threadgroup float partial0[head_dim];
    threadgroup float partial1[head_dim];
    threadgroup float k_vec[head_dim];
    threadgroup float v_vec[head_dim];

    const uint query = query_start + qi;
    const bool active = query < tokens;
    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint key_end = min(key_start + key_block, tokens);

    float q_value = 0.0f;
    if (active) {
        const uint q_cache_base = query * q_width + q_head * head_dim;
        q_value = float(q_cache[q_cache_base + lane]);
    }

    float m = -3.402823466e38f;
    float l = 0.0f;
    float acc = 0.0f;

    for (uint key = key_start; key < key_end; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        if (qi == 0) {
            k_vec[lane] = float(k_cache[kv_base + lane]);
            v_vec[lane] = float(v_cache[kv_base + lane]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        if (active && key <= query) {
            if (qi == 0) {
                partial0[lane] = q_value * k_vec[lane];
            } else {
                partial1[lane] = q_value * k_vec[lane];
            }
        } else {
            if (qi == 0) {
                partial0[lane] = 0.0f;
            } else {
                partial1[lane] = 0.0f;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint stride = 128; stride > 0; stride >>= 1) {
            if (lane < stride) {
                if (qi == 0) {
                    partial0[lane] += partial0[lane + stride];
                } else {
                    partial1[lane] += partial1[lane + stride];
                }
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }

        if (active && key <= query) {
            const float dot = (qi == 0) ? partial0[0] : partial1[0];
            const float score = dot * inv_sqrt_head_dim;
            const float m_next = max(m, score);
            const float alpha = exp(m - m_next);
            const float beta = exp(score - m_next);
            acc = acc * alpha + v_vec[lane] * beta;
            l = l * alpha + beta;
            m = m_next;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (active) {
        const uint scalar_idx = (query * q_heads + q_head) * n_key_blocks + key_block_id;
        const uint acc_idx = scalar_idx * head_dim + lane;
        partial_acc[acc_idx] = acc;
        if (lane == 0) {
            partial_m[scalar_idx] = m;
            partial_l[scalar_idx] = l;
        }
    }
}

kernel void qwen35_08b_prefill_attention_partial_combine_kblk64_gqa8_d256_to_fp16(
    device const float* partial_m [[buffer(0)]],
    device const float* partial_l [[buffer(1)]],
    device const float* partial_acc [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    constant uint& n_key_blocks [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint key_block = 64;

    const uint query = tg_pos.x;
    const uint q_head = tg_pos.y;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head >= q_heads || lane >= head_dim) {
        return;
    }

    float m = -3.402823466e38f;
    float l = 0.0f;
    float acc = 0.0f;
    const uint max_key_block = min(query / key_block + 1, n_key_blocks);

    for (uint kb = 0; kb < max_key_block; ++kb) {
        const uint scalar_idx = (query * q_heads + q_head) * n_key_blocks + kb;
        const float bm = partial_m[scalar_idx];
        const float bl = partial_l[scalar_idx];
        if (bl <= 0.0f) {
            continue;
        }
        const float m_next = max(m, bm);
        const float alpha = exp(m - m_next);
        const float beta = exp(bm - m_next);
        acc = acc * alpha + partial_acc[scalar_idx * head_dim + lane] * beta;
        l = l * alpha + bl * beta;
        m = m_next;
    }

    const uint q_base = query * q_rows +
        ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
    float gate = 1.0f;
    if (q_rows >= q_width * 2) {
        gate = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + lane]));
    }

    const uint out_base = query * q_width + q_head * head_dim;
    const float normalized = (l > 0.0f) ? (acc / l) : 0.0f;
    out[out_base + lane] = half(clamp(normalized * gate, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_attention_qh4_splitk64_gqa8_kv2_d256(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device float* partial_m [[buffer(3)]],
    device float* partial_l [[buffer(4)]],
    device float* partial_acc [[buffer(5)]],
    constant uint& tokens [[buffer(6)]],
    constant uint& n_key_blocks [[buffer(7)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint key_block = 64;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint key_block_id = tg_pos.z;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32) {
        return;
    }

    const uint key_start = key_block_id * key_block;
    if (key_start > query) {
        return;
    }
    const uint key_end = min(key_start + key_block, tokens);
    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    float q_value[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
            }
        }
    }

    for (uint key = key_start; key < key_end && key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache[kv_base + dim]);
            v_value[c] = float(v_cache[kv_base + dim]);
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            const uint q_head = q_head_start + hp;
            if (q_head >= q_heads) {
                continue;
            }
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint scalar_idx = (query * q_heads + q_head) * n_key_blocks + key_block_id;
            if (lane == 0) {
                partial_m[scalar_idx] = m[hp];
                partial_l[scalar_idx] = l[hp];
            }
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                partial_acc[scalar_idx * head_dim + dim] = acc[hp][c];
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_qh4_splitk_gqa8_kv2_d256(
    device const half* q_cache [[buffer(0)]],
    device const half* k_cache [[buffer(1)]],
    device const half* v_cache [[buffer(2)]],
    device float* partial_m [[buffer(3)]],
    device float* partial_l [[buffer(4)]],
    device float* partial_acc [[buffer(5)]],
    constant uint& tokens [[buffer(6)]],
    constant uint& n_key_blocks [[buffer(7)]],
    constant uint& key_block [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint lane_dims = 8;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint kv_width = kv_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;

    const uint query = tg_pos.x;
    const uint q_head_start = tg_pos.y * heads_per_group;
    const uint key_block_id = tg_pos.z;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head_start >= q_heads || lane >= 32 || key_block == 0) {
        return;
    }

    const uint key_start = key_block_id * key_block;
    if (key_start > query) {
        return;
    }
    const uint key_end = min(key_start + key_block, tokens);
    const uint kv_head = min(q_head_start / (q_heads / kv_heads), kv_heads - 1);

    float q_value[heads_per_group][lane_dims];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group][lane_dims];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            q_value[hp][c] = 0.0f;
            acc[hp][c] = 0.0f;
            if (q_head < q_heads) {
                const uint q_cache_base = query * q_width + q_head * head_dim;
                q_value[hp][c] = float(q_cache[q_cache_base + dim]);
            }
        }
    }

    for (uint key = key_start; key < key_end && key <= query; ++key) {
        const uint kv_base = key * kv_width + kv_head * head_dim;
        float k_value[lane_dims];
        float v_value[lane_dims];
        for (uint c = 0; c < lane_dims; ++c) {
            const uint dim = lane + c * 32;
            k_value[c] = float(k_cache[kv_base + dim]);
            v_value[c] = float(v_cache[kv_base + dim]);
        }

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            const uint q_head = q_head_start + hp;
            if (q_head >= q_heads) {
                continue;
            }
            float score_part = 0.0f;
            for (uint c = 0; c < lane_dims; ++c) {
                score_part += q_value[hp][c] * k_value[c];
            }
            const float score = simd_sum(score_part) * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            for (uint c = 0; c < lane_dims; ++c) {
                acc[hp][c] = acc[hp][c] * alpha + v_value[c] * beta;
            }
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        if (q_head < q_heads) {
            const uint scalar_idx = (query * q_heads + q_head) * n_key_blocks + key_block_id;
            if (lane == 0) {
                partial_m[scalar_idx] = m[hp];
                partial_l[scalar_idx] = l[hp];
            }
            for (uint c = 0; c < lane_dims; ++c) {
                const uint dim = lane + c * 32;
                partial_acc[scalar_idx * head_dim + dim] = acc[hp][c];
            }
        }
    }
}

kernel void qwen35_08b_prefill_attention_partial_combine_splitk_gqa8_d256_to_fp16(
    device const float* partial_m [[buffer(0)]],
    device const float* partial_l [[buffer(1)]],
    device const float* partial_acc [[buffer(2)]],
    device const float* q_tokens [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    constant uint& n_key_blocks [[buffer(7)]],
    constant uint& key_block [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;

    const uint query = tg_pos.x;
    const uint q_head = tg_pos.y;
    const uint lane = tid_pos.x;
    if (query >= tokens || q_head >= q_heads || lane >= head_dim || key_block == 0) {
        return;
    }

    float m = -3.402823466e38f;
    float l = 0.0f;
    float acc = 0.0f;
    const uint max_key_block = min(query / key_block + 1, n_key_blocks);

    for (uint kb = 0; kb < max_key_block; ++kb) {
        const uint scalar_idx = (query * q_heads + q_head) * n_key_blocks + kb;
        const float bm = partial_m[scalar_idx];
        const float bl = partial_l[scalar_idx];
        if (bl <= 0.0f) {
            continue;
        }
        const float m_next = max(m, bm);
        const float alpha = exp(m - m_next);
        const float beta = exp(bm - m_next);
        acc = acc * alpha + partial_acc[scalar_idx * head_dim + lane] * beta;
        l = l * alpha + bl * beta;
        m = m_next;
    }

    const uint q_base = query * q_rows +
        ((q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim);
    float gate = 1.0f;
    if (q_rows >= q_width * 2) {
        gate = 1.0f / (1.0f + exp(-q_tokens[q_base + head_dim + lane]));
    }

    const uint out_base = query * q_width + q_head * head_dim;
    const float normalized = (l > 0.0f) ? (acc / l) : 0.0f;
    out[out_base + lane] = half(clamp(normalized * gate, -65504.0f, 65504.0f));
}
