#include <metal_stdlib>
using namespace metal;

static inline float qwen35_08b_rope_freq(uint d) {
    constexpr float theta = 10000000.0f;
    constexpr float rope_dim = 64.0f;
    const float pair = float(d % 32u);
    return pow(theta, -(2.0f * pair) / rope_dim);
}

static inline float qwen35_08b_rope_component(
    device const float* x,
    uint base,
    uint d,
    uint position
) {
    constexpr uint rope_dim = 64;
    if (d >= rope_dim) {
        return x[base + d];
    }
    constexpr uint half_rope_dim = rope_dim / 2;
    const uint pair_d = (d < half_rope_dim) ? d + half_rope_dim : d - half_rope_dim;
    const float a = x[base + (d % half_rope_dim)];
    const float b = x[base + ((d % half_rope_dim) + half_rope_dim)];
    const float angle = float(position) * qwen35_08b_rope_freq(d);
    const float c = cos(angle);
    const float s = sin(angle);
    if (d < half_rope_dim) {
        (void)pair_d;
        return a * c - b * s;
    }
    (void)pair_d;
    return b * c + a * s;
}

static inline float qwen35_08b_rope_component_normed(
    device const float* x,
    device const half* norm_weight,
    uint base,
    uint d,
    uint position,
    float inv_rms
) {
    constexpr uint rope_dim = 64;
    if (d >= rope_dim) {
        return x[base + d] * inv_rms * float(norm_weight[d]);
    }
    constexpr uint half_rope_dim = rope_dim / 2;
    const uint lo = d % half_rope_dim;
    const uint hi = lo + half_rope_dim;
    const float a = x[base + lo] * inv_rms * float(norm_weight[lo]);
    const float b = x[base + hi] * inv_rms * float(norm_weight[hi]);
    const float angle = float(position) * qwen35_08b_rope_freq(d);
    const float c = cos(angle);
    const float s = sin(angle);
    if (d < half_rope_dim) {
        return a * c - b * s;
    }
    return b * c + a * s;
}

kernel void qwen35_08b_attention_single_token_qkv1024_to_fp16(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device half* out [[buffer(3)]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[256];

    float acc = 0.0f;
    for (uint col = tid; col < 1024; col += 256) {
        acc += q[col] * k[col];
    }
    partial[tid] = acc;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float gate = 1.0f / (1.0f + exp(-(partial[0] * rsqrt(1024.0f))));
    for (uint col = tid; col < 1024; col += 256) {
        out[col] = half(clamp(v[col] * gate, -65504.0f, 65504.0f));
    }
}

kernel void qwen35_08b_attention_single_token_gqa8_kv2_d256_to_fp16(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device half* out [[buffer(3)]],
    constant uint& q_rows [[buffer(4)]],
    uint q_head [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[256];

    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
    const uint q_out_base = q_head * head_dim;
    const uint kv_base = kv_head * head_dim;

    float acc = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        acc += q[q_base + d] * k[kv_base + d];
    }
    partial[tid] = acc;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    // Single-token decode has one visible KV item, so softmax is 1.0. The dot
    // product is still evaluated above to keep the q/k dataflow present while
    // this shape-correct placeholder grows into the real KV-cache attention.
    (void)partial[0];
    for (uint d = tid; d < head_dim; d += 256) {
        float gate = 1.0f;
        if (q_rows >= q_width * 2) {
            gate = 1.0f / (1.0f + exp(-q[q_base + head_dim + d]));
        } else if (q_rows >= q_width + q_heads) {
            gate = 1.0f / (1.0f + exp(-q[q_width + q_head]));
        }
        out[q_out_base + d] = half(clamp(v[kv_base + d] * gate, -65504.0f, 65504.0f));
    }
}

kernel void qwen35_08b_attention_qk_rmsnorm_f32_h8_kv2_d256(
    device float* q [[buffer(0)]],
    device float* k [[buffer(1)]],
    device const half* q_norm_weight [[buffer(2)]],
    device const half* k_norm_weight [[buffer(3)]],
    constant uint& q_rows [[buffer(4)]],
    uint head [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr float eps = 1.0e-6f;
    threadgroup float partial[256];

    if (head >= q_heads) {
        return;
    }

    const uint q_base = (q_rows >= q_width * 2) ? head * head_dim * 2 : head * head_dim;
    float q_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float v = q[q_base + d];
        q_ss += v * v;
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
    for (uint d = tid; d < head_dim; d += 256) {
        q[q_base + d] = q[q_base + d] * q_inv_rms * float(q_norm_weight[d]);
    }

    if (head >= kv_heads) {
        return;
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);
    const uint k_base = head * head_dim;
    float k_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float v = k[k_base + d];
        k_ss += v * v;
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
    for (uint d = tid; d < head_dim; d += 256) {
        k[k_base + d] = k[k_base + d] * k_inv_rms * float(k_norm_weight[d]);
    }
}

kernel void qwen35_08b_attention_single_token_gqa8_kv2_d256_rope_cache_to_fp16(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device half* k_cache [[buffer(3)]],
    device half* v_cache [[buffer(4)]],
    device half* out [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    constant uint& position [[buffer(7)]],
    constant uint& max_context [[buffer(8)]],
    uint q_head [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[256];

    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr float inv_sqrt_head_dim = 0.0625f;
    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
    const uint q_out_base = q_head * head_dim;
    const uint kv_base = kv_head * head_dim;
    const uint cache_pos = min(position, max_context - 1);
    const uint cache_base = cache_pos * (kv_heads * head_dim) + kv_base;

    const float current_k = qwen35_08b_rope_component(k, kv_base, tid, position);
    k_cache[cache_base + tid] = half(clamp(current_k, -65504.0f, 65504.0f));
    v_cache[cache_base + tid] = half(clamp(v[kv_base + tid], -65504.0f, 65504.0f));
    threadgroup_barrier(mem_flags::mem_device);

    const float q_value = qwen35_08b_rope_component(q, q_base, tid, position);
    float gate = 1.0f;
    if (q_rows >= q_width * 2) {
        gate = 1.0f / (1.0f + exp(-q[q_base + head_dim + tid]));
    } else if (q_rows >= q_width + q_heads) {
        gate = 1.0f / (1.0f + exp(-q[q_width + q_head]));
    }

    float m = -3.402823466e38f;
    float l = 0.0f;
    float acc = 0.0f;
    const uint n_ctx = min(position + 1, max_context);

    for (uint t = 0; t < n_ctx; ++t) {
        const uint t_base = t * (kv_heads * head_dim) + kv_base;
        const float key = float(k_cache[t_base + tid]);
        partial[tid] = q_value * key;
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
        const float value = float(v_cache[t_base + tid]);
        acc = acc * alpha + value * beta;
        l = l * alpha + beta;
        m = m_next;
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float normalized = (l > 0.0f) ? (acc / l) : 0.0f;
    out[q_out_base + tid] = half(clamp(normalized * gate, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_attention_single_token_qh4_gqa8_kv2_d256_rope_cache_to_fp16(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device half* k_cache [[buffer(3)]],
    device half* v_cache [[buffer(4)]],
    device half* out [[buffer(5)]],
    constant uint& q_rows [[buffer(6)]],
    constant uint& position [[buffer(7)]],
    constant uint& max_context [[buffer(8)]],
    uint kv_head [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float inv_sqrt_head_dim = 0.0625f;
    if (kv_head >= kv_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[heads_per_group][simdgroups_per_tg];

    const uint q_head_start = kv_head * heads_per_group;
    const uint kv_base = kv_head * head_dim;
    const uint cache_pos = min(position, max_context - 1);
    const uint cache_base = cache_pos * (kv_heads * head_dim) + kv_base;

    const float current_k = qwen35_08b_rope_component(k, kv_base, tid, position);
    k_cache[cache_base + tid] = half(clamp(current_k, -65504.0f, 65504.0f));
    v_cache[cache_base + tid] = half(clamp(v[kv_base + tid], -65504.0f, 65504.0f));
    threadgroup_barrier(mem_flags::mem_device);

    float q_value[heads_per_group];
    float gate[heads_per_group];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
        q_value[hp] = qwen35_08b_rope_component(q, q_base, tid, position);
        gate[hp] = 1.0f;
        if (q_rows >= q_width * 2) {
            gate[hp] = 1.0f / (1.0f + exp(-q[q_base + head_dim + tid]));
        } else if (q_rows >= q_width + q_heads) {
            gate[hp] = 1.0f / (1.0f + exp(-q[q_width + q_head]));
        }
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        acc[hp] = 0.0f;
    }

    const uint n_ctx = min(position + 1, max_context);
    for (uint t = 0; t < n_ctx; ++t) {
        const uint t_base = t * (kv_heads * head_dim) + kv_base;
        const float key = float(k_cache[t_base + tid]);
        const float value = float(v_cache[t_base + tid]);

        float score_part[heads_per_group];
        for (uint hp = 0; hp < heads_per_group; ++hp) {
            score_part[hp] = q_value[hp] * key;
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
            acc[hp] = acc[hp] * alpha + value * beta;
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        const uint q_out_base = q_head * head_dim;
        const float normalized = (l[hp] > 0.0f) ? (acc[hp] / l[hp]) : 0.0f;
        out[q_out_base + tid] = half(clamp(normalized * gate[hp], -65504.0f, 65504.0f));
    }
}

kernel void qwen35_08b_attention_norm_rope_cache_qh4_gqa8_kv2_d256_to_fp16(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* k_cache [[buffer(5)]],
    device half* v_cache [[buffer(6)]],
    device half* out [[buffer(7)]],
    constant uint& q_rows [[buffer(8)]],
    constant uint& position [[buffer(9)]],
    constant uint& max_context [[buffer(10)]],
    uint kv_head [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float eps = 1.0e-6f;
    constexpr float inv_sqrt_head_dim = 0.0625f;
    if (kv_head >= kv_heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[heads_per_group + 1][simdgroups_per_tg];

    const uint q_head_start = kv_head * heads_per_group;
    const uint kv_base = kv_head * head_dim;
    const uint cache_pos = min(position, max_context - 1);
    const uint cache_base = cache_pos * (kv_heads * head_dim) + kv_base;

    float q_ss[heads_per_group];
    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
        const float qv = q[q_base + tid];
        q_ss[hp] = simd_sum(qv * qv);
        if (simd_lane == 0) {
            partial[hp][simd_group] = q_ss[hp];
        }
    }

    const float kv = k[kv_base + tid];
    float k_ss = simd_sum(kv * kv);
    if (simd_lane == 0) {
        partial[heads_per_group][simd_group] = k_ss;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float q_inv_rms[heads_per_group];
    for (uint hp = 0; hp < heads_per_group; ++hp) {
        float total = 0.0f;
        if (simd_group == 0) {
            total = (tid < simdgroups_per_tg) ? partial[hp][tid] : 0.0f;
            total = simd_sum(total);
            if (simd_lane == 0) {
                partial[hp][0] = total;
            }
        }
    }
    float k_total = 0.0f;
    if (simd_group == 0) {
        k_total = (tid < simdgroups_per_tg) ? partial[heads_per_group][tid] : 0.0f;
        k_total = simd_sum(k_total);
        if (simd_lane == 0) {
            partial[heads_per_group][0] = k_total;
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        q_inv_rms[hp] = rsqrt(partial[hp][0] / float(head_dim) + eps);
    }
    const float k_inv_rms = rsqrt(partial[heads_per_group][0] / float(head_dim) + eps);

    const float current_k = qwen35_08b_rope_component_normed(
        k, k_norm_weight, kv_base, tid, position, k_inv_rms);
    k_cache[cache_base + tid] = half(clamp(current_k, -65504.0f, 65504.0f));
    v_cache[cache_base + tid] = half(clamp(v[kv_base + tid], -65504.0f, 65504.0f));
    threadgroup_barrier(mem_flags::mem_device);

    float q_value[heads_per_group];
    float gate[heads_per_group];
    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group];

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
        q_value[hp] = qwen35_08b_rope_component_normed(
            q, q_norm_weight, q_base, tid, position, q_inv_rms[hp]);
        gate[hp] = 1.0f;
        if (q_rows >= q_width * 2) {
            gate[hp] = 1.0f / (1.0f + exp(-q[q_base + head_dim + tid]));
        } else if (q_rows >= q_width + q_heads) {
            gate[hp] = 1.0f / (1.0f + exp(-q[q_width + q_head]));
        }
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        acc[hp] = 0.0f;
    }

    const uint n_ctx = min(position + 1, max_context);
    for (uint t = 0; t < n_ctx; ++t) {
        const uint t_base = t * (kv_heads * head_dim) + kv_base;
        const float key = float(k_cache[t_base + tid]);
        const float value = float(v_cache[t_base + tid]);

        float score_part[heads_per_group];
        for (uint hp = 0; hp < heads_per_group; ++hp) {
            score_part[hp] = simd_sum(q_value[hp] * key);
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
            acc[hp] = acc[hp] * alpha + value * beta;
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        const uint q_out_base = q_head * head_dim;
        const float normalized = (l[hp] > 0.0f) ? (acc[hp] / l[hp]) : 0.0f;
        out[q_out_base + tid] = half(clamp(normalized * gate[hp], -65504.0f, 65504.0f));
    }
}

template <uint key_block>
static inline void qwen35_08b_attention_norm_rope_cache_qh4_splitk_partial_impl(
    device const float* q,
    device const float* k,
    device const float* v,
    device const half* q_norm_weight,
    device const half* k_norm_weight,
    device half* k_cache,
    device half* v_cache,
    device float* partial_m,
    device float* partial_l,
    device float* partial_acc,
    const uint q_rows,
    const uint position,
    const uint max_context,
    const uint n_key_blocks,
    uint3 tg,
    uint3 tid_pos,
    uint simd_lane,
    uint simd_group,
    threadgroup float* partial
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr uint simdgroups_per_tg = 8;
    constexpr float eps = 1.0e-6f;
    constexpr float inv_sqrt_head_dim = 0.0625f;
    const uint kv_head = tg.x;
    const uint key_block_id = tg.y;
    const uint tid = tid_pos.x;
    if (kv_head >= kv_heads || tid >= head_dim) {
        return;
    }

    const uint q_head_start = kv_head * heads_per_group;
    const uint kv_base = kv_head * head_dim;
    const uint cache_pos = min(position, max_context - 1);
    const uint cache_base = cache_pos * (kv_heads * head_dim) + kv_base;
    const uint out_block_base = (kv_head * n_key_blocks + key_block_id) * heads_per_group;

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
        const float qv = q[q_base + tid];
        const float q_ss = simd_sum(qv * qv);
        if (simd_lane == 0) {
            partial[hp * simdgroups_per_tg + simd_group] = q_ss;
        }
    }

    const float kv = k[kv_base + tid];
    const float k_ss = simd_sum(kv * kv);
    if (simd_lane == 0) {
        partial[heads_per_group * simdgroups_per_tg + simd_group] = k_ss;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        float total = 0.0f;
        if (simd_group == 0) {
            total = (tid < simdgroups_per_tg) ? partial[hp * simdgroups_per_tg + tid] : 0.0f;
            total = simd_sum(total);
            if (simd_lane == 0) {
                partial[hp * simdgroups_per_tg] = total;
            }
        }
    }
    float k_total = 0.0f;
    if (simd_group == 0) {
        k_total = (tid < simdgroups_per_tg) ? partial[heads_per_group * simdgroups_per_tg + tid] : 0.0f;
        k_total = simd_sum(k_total);
        if (simd_lane == 0) {
            partial[heads_per_group * simdgroups_per_tg] = k_total;
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float q_inv_rms[heads_per_group];
    for (uint hp = 0; hp < heads_per_group; ++hp) {
        q_inv_rms[hp] = rsqrt(partial[hp * simdgroups_per_tg] / float(head_dim) + eps);
    }
    const float k_inv_rms = rsqrt(partial[heads_per_group * simdgroups_per_tg] / float(head_dim) + eps);

    const float current_k = qwen35_08b_rope_component_normed(
        k, k_norm_weight, kv_base, tid, position, k_inv_rms);
    if (key_block_id == cache_pos / key_block) {
        k_cache[cache_base + tid] = half(clamp(current_k, -65504.0f, 65504.0f));
        v_cache[cache_base + tid] = half(clamp(v[kv_base + tid], -65504.0f, 65504.0f));
        threadgroup_barrier(mem_flags::mem_device);
    }

    float q_value[heads_per_group];
    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint q_head = q_head_start + hp;
        const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
        q_value[hp] = qwen35_08b_rope_component_normed(
            q, q_norm_weight, q_base, tid, position, q_inv_rms[hp]);
    }

    float m[heads_per_group];
    float l[heads_per_group];
    float acc[heads_per_group];
    for (uint hp = 0; hp < heads_per_group; ++hp) {
        m[hp] = -3.402823466e38f;
        l[hp] = 0.0f;
        acc[hp] = 0.0f;
    }

    const uint n_ctx = min(position + 1, max_context);
    const uint t_start = key_block_id * key_block;
    const uint t_end = min(t_start + key_block, n_ctx);
    for (uint t = t_start; t < t_end; ++t) {
        const uint t_base = t * (kv_heads * head_dim) + kv_base;
        const float key = float(k_cache[t_base + tid]);
        const float value = float(v_cache[t_base + tid]);

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            const float score_part = simd_sum(q_value[hp] * key);
            if (simd_lane == 0) {
                partial[hp * simdgroups_per_tg + simd_group] = score_part;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            float score = 0.0f;
            if (simd_group == 0) {
                score = (tid < simdgroups_per_tg) ? partial[hp * simdgroups_per_tg + tid] : 0.0f;
                score = simd_sum(score);
                if (simd_lane == 0) {
                    partial[hp * simdgroups_per_tg] = score;
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint hp = 0; hp < heads_per_group; ++hp) {
            const float score = partial[hp * simdgroups_per_tg] * inv_sqrt_head_dim;
            const float m_next = max(m[hp], score);
            const float alpha = exp(m[hp] - m_next);
            const float beta = exp(score - m_next);
            acc[hp] = acc[hp] * alpha + value * beta;
            l[hp] = l[hp] * alpha + beta;
            m[hp] = m_next;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint hp = 0; hp < heads_per_group; ++hp) {
        const uint scalar_idx = out_block_base + hp;
        if (tid == 0) {
            partial_m[scalar_idx] = m[hp];
            partial_l[scalar_idx] = l[hp];
        }
        partial_acc[scalar_idx * head_dim + tid] = acc[hp];
    }
}

kernel void qwen35_08b_attention_norm_rope_cache_qh4_splitk128_partial_gqa8_kv2_d256(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* k_cache [[buffer(5)]],
    device half* v_cache [[buffer(6)]],
    device float* partial_m [[buffer(7)]],
    device float* partial_l [[buffer(8)]],
    device float* partial_acc [[buffer(9)]],
    constant uint& q_rows [[buffer(10)]],
    constant uint& position [[buffer(11)]],
    constant uint& max_context [[buffer(12)]],
    constant uint& n_key_blocks [[buffer(13)]],
    uint3 tg [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    threadgroup float partial[(4 + 1) * 8];
    qwen35_08b_attention_norm_rope_cache_qh4_splitk_partial_impl<128>(
        q, k, v, q_norm_weight, k_norm_weight, k_cache, v_cache,
        partial_m, partial_l, partial_acc, q_rows, position, max_context,
        n_key_blocks, tg, tid_pos, simd_lane, simd_group, partial);
}

kernel void qwen35_08b_attention_norm_rope_cache_qh4_splitk256_partial_gqa8_kv2_d256(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* k_cache [[buffer(5)]],
    device half* v_cache [[buffer(6)]],
    device float* partial_m [[buffer(7)]],
    device float* partial_l [[buffer(8)]],
    device float* partial_acc [[buffer(9)]],
    constant uint& q_rows [[buffer(10)]],
    constant uint& position [[buffer(11)]],
    constant uint& max_context [[buffer(12)]],
    constant uint& n_key_blocks [[buffer(13)]],
    uint3 tg [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    threadgroup float partial[(4 + 1) * 8];
    qwen35_08b_attention_norm_rope_cache_qh4_splitk_partial_impl<256>(
        q, k, v, q_norm_weight, k_norm_weight, k_cache, v_cache,
        partial_m, partial_l, partial_acc, q_rows, position, max_context,
        n_key_blocks, tg, tid_pos, simd_lane, simd_group, partial);
}

kernel void qwen35_08b_attention_norm_rope_cache_qh4_splitk512_partial_gqa8_kv2_d256(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* k_cache [[buffer(5)]],
    device half* v_cache [[buffer(6)]],
    device float* partial_m [[buffer(7)]],
    device float* partial_l [[buffer(8)]],
    device float* partial_acc [[buffer(9)]],
    constant uint& q_rows [[buffer(10)]],
    constant uint& position [[buffer(11)]],
    constant uint& max_context [[buffer(12)]],
    constant uint& n_key_blocks [[buffer(13)]],
    uint3 tg [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    threadgroup float partial[(4 + 1) * 8];
    qwen35_08b_attention_norm_rope_cache_qh4_splitk_partial_impl<512>(
        q, k, v, q_norm_weight, k_norm_weight, k_cache, v_cache,
        partial_m, partial_l, partial_acc, q_rows, position, max_context,
        n_key_blocks, tg, tid_pos, simd_lane, simd_group, partial);
}

kernel void qwen35_08b_attention_norm_rope_cache_qh4_splitk1024_partial_gqa8_kv2_d256(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* k_cache [[buffer(5)]],
    device half* v_cache [[buffer(6)]],
    device float* partial_m [[buffer(7)]],
    device float* partial_l [[buffer(8)]],
    device float* partial_acc [[buffer(9)]],
    constant uint& q_rows [[buffer(10)]],
    constant uint& position [[buffer(11)]],
    constant uint& max_context [[buffer(12)]],
    constant uint& n_key_blocks [[buffer(13)]],
    uint3 tg [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]],
    uint simd_group [[simdgroup_index_in_threadgroup]]
) {
    threadgroup float partial[(4 + 1) * 8];
    qwen35_08b_attention_norm_rope_cache_qh4_splitk_partial_impl<1024>(
        q, k, v, q_norm_weight, k_norm_weight, k_cache, v_cache,
        partial_m, partial_l, partial_acc, q_rows, position, max_context,
        n_key_blocks, tg, tid_pos, simd_lane, simd_group, partial);
}

kernel void qwen35_08b_attention_norm_rope_cache_qh4_splitk256_combine_gqa8_kv2_d256_to_fp16(
    device const float* q [[buffer(0)]],
    device const float* partial_m [[buffer(1)]],
    device const float* partial_l [[buffer(2)]],
    device const float* partial_acc [[buffer(3)]],
    device half* out [[buffer(4)]],
    constant uint& q_rows [[buffer(5)]],
    constant uint& n_key_blocks [[buffer(6)]],
    uint kv_head [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint heads_per_group = 4;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    if (kv_head >= kv_heads || tid >= head_dim) {
        return;
    }

    const uint q_head_start = kv_head * heads_per_group;
    for (uint hp = 0; hp < heads_per_group; ++hp) {
        float m = -3.402823466e38f;
        float l = 0.0f;
        float acc = 0.0f;
        for (uint kb = 0; kb < n_key_blocks; ++kb) {
            const uint scalar_idx = (kv_head * n_key_blocks + kb) * heads_per_group + hp;
            const float bm = partial_m[scalar_idx];
            const float bl = partial_l[scalar_idx];
            if (bl <= 0.0f) {
                continue;
            }
            const float m_next = max(m, bm);
            const float alpha = exp(m - m_next);
            const float beta = exp(bm - m_next);
            acc = acc * alpha + partial_acc[scalar_idx * head_dim + tid] * beta;
            l = l * alpha + bl * beta;
            m = m_next;
        }

        const uint q_head = q_head_start + hp;
        const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
        float gate = 1.0f;
        if (q_rows >= q_width * 2) {
            gate = 1.0f / (1.0f + exp(-q[q_base + head_dim + tid]));
        } else if (q_rows >= q_width + q_heads) {
            gate = 1.0f / (1.0f + exp(-q[q_width + q_head]));
        }
        const float normalized = (l > 0.0f) ? (acc / l) : 0.0f;
        out[q_head * head_dim + tid] = half(clamp(normalized * gate, -65504.0f, 65504.0f));
    }
}

kernel void qwen35_08b_attention_norm_rope_cache_gqa8_kv2_d256_to_fp16(
    device const float* q [[buffer(0)]],
    device const float* k [[buffer(1)]],
    device const float* v [[buffer(2)]],
    device const half* q_norm_weight [[buffer(3)]],
    device const half* k_norm_weight [[buffer(4)]],
    device half* k_cache [[buffer(5)]],
    device half* v_cache [[buffer(6)]],
    device half* out [[buffer(7)]],
    constant uint& q_rows [[buffer(8)]],
    constant uint& position [[buffer(9)]],
    constant uint& max_context [[buffer(10)]],
    uint q_head [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[256];

    constexpr uint q_heads = 8;
    constexpr uint kv_heads = 2;
    constexpr uint head_dim = 256;
    constexpr uint q_width = q_heads * head_dim;
    constexpr float eps = 1.0e-6f;
    constexpr float inv_sqrt_head_dim = 0.0625f;
    const uint kv_head = min(q_head / (q_heads / kv_heads), kv_heads - 1);
    const uint q_base = (q_rows >= q_width * 2) ? q_head * head_dim * 2 : q_head * head_dim;
    const uint q_out_base = q_head * head_dim;
    const uint kv_base = kv_head * head_dim;
    const uint cache_pos = min(position, max_context - 1);
    const uint cache_base = cache_pos * (kv_heads * head_dim) + kv_base;

    float q_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float qv = q[q_base + d];
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

    float k_ss = 0.0f;
    for (uint d = tid; d < head_dim; d += 256) {
        const float kv = k[kv_base + d];
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

    const float current_k = qwen35_08b_rope_component_normed(
        k, k_norm_weight, kv_base, tid, position, k_inv_rms);
    k_cache[cache_base + tid] = half(clamp(current_k, -65504.0f, 65504.0f));
    v_cache[cache_base + tid] = half(clamp(v[kv_base + tid], -65504.0f, 65504.0f));
    threadgroup_barrier(mem_flags::mem_device);

    const float q_value = qwen35_08b_rope_component_normed(
        q, q_norm_weight, q_base, tid, position, q_inv_rms);
    float gate = 1.0f;
    if (q_rows >= q_width * 2) {
        gate = 1.0f / (1.0f + exp(-q[q_base + head_dim + tid]));
    } else if (q_rows >= q_width + q_heads) {
        gate = 1.0f / (1.0f + exp(-q[q_width + q_head]));
    }

    float m = -3.402823466e38f;
    float l = 0.0f;
    float acc = 0.0f;
    const uint n_ctx = min(position + 1, max_context);

    for (uint t = 0; t < n_ctx; ++t) {
        const uint t_base = t * (kv_heads * head_dim) + kv_base;
        const float key = float(k_cache[t_base + tid]);
        partial[tid] = q_value * key;
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
        const float value = float(v_cache[t_base + tid]);
        acc = acc * alpha + value * beta;
        l = l * alpha + beta;
        m = m_next;
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float normalized = (l > 0.0f) ? (acc / l) : 0.0f;
    out[q_out_base + tid] = half(clamp(normalized * gate, -65504.0f, 65504.0f));
}
