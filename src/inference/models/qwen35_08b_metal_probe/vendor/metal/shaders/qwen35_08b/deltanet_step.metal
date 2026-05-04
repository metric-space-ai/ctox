#include <metal_stdlib>
using namespace metal;

static inline float qwen35_08b_sigmoid(float x) {
    const float clamped = clamp(x, -20.0f, 20.0f);
    return 1.0f / (1.0f + exp(-clamped));
}

static inline float qwen35_08b_softplus(float x) {
    const float clamped = clamp(x, -20.0f, 20.0f);
    return select(log(1.0f + exp(clamped)), clamped, clamped > 20.0f);
}

static inline float qwen35_08b_silu(float x) {
    return x / (1.0f + exp(-clamp(x, -20.0f, 20.0f)));
}

kernel void qwen35_08b_deltanet_step_f32_state(
    device const half* q [[buffer(0)]],
    device const half* k [[buffer(1)]],
    device const half* v [[buffer(2)]],
    device const float* beta [[buffer(3)]],
    device const float* gate [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out [[buffer(6)]],
    uint head [[threadgroup_position_in_grid]],
    uint i [[thread_position_in_threadgroup]]
) {
    if (i >= 128) {
        return;
    }

    const uint vec_base = head * 128;
    const uint state_base = head * 128 * 128;

    float kv_mem = 0.0f;
    const float g = gate[head];
    for (uint j = 0; j < 128; ++j) {
        kv_mem += state[state_base + i * 128 + j] * g * float(k[vec_base + j]);
    }

    const float delta = (float(v[vec_base + i]) - kv_mem) * beta[head];

    for (uint j = 0; j < 128; ++j) {
        state[state_base + i * 128 + j] =
            state[state_base + i * 128 + j] * g + float(k[vec_base + j]) * delta;
    }

    threadgroup_barrier(mem_flags::mem_device);

    float acc = 0.0f;
    for (uint j = 0; j < 128; ++j) {
        acc += state[state_base + i * 128 + j] * float(q[vec_base + j]);
    }

    out[vec_base + i] = acc;
}

kernel void qwen35_08b_deltanet_step_rowcache_f32_state(
    device const half* q [[buffer(0)]],
    device const half* k [[buffer(1)]],
    device const half* v [[buffer(2)]],
    device const float* beta [[buffer(3)]],
    device const float* gate [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out [[buffer(6)]],
    uint head [[threadgroup_position_in_grid]],
    uint i [[thread_position_in_threadgroup]]
) {
    if (i >= 128) {
        return;
    }

    constexpr uint head_dim = 128;
    const uint vec_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;
    const uint row_state_base = state_base + i * head_dim;

    thread float row_state[head_dim];
    threadgroup float k_s[head_dim];
    threadgroup float q_s[head_dim];

    k_s[i] = float(k[vec_base + i]);
    q_s[i] = float(q[vec_base + i]);
    for (uint j = 0; j < head_dim; ++j) {
        row_state[j] = state[row_state_base + j];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    const float g = gate[head];
    float kv_mem = 0.0f;
    for (uint j = 0; j < head_dim; ++j) {
        kv_mem += row_state[j] * g * k_s[j];
    }

    const float delta = (float(v[vec_base + i]) - kv_mem) * beta[head];

    float acc = 0.0f;
    for (uint j = 0; j < head_dim; ++j) {
        const float next_state = row_state[j] * g + k_s[j] * delta;
        row_state[j] = next_state;
        state[row_state_base + j] = next_state;
        acc += next_state * q_s[j];
    }

    out[vec_base + i] = acc;
}

kernel void qwen35_08b_deltanet_step_fused_decay_f32_state(
    device const half* q [[buffer(0)]],
    device const half* k [[buffer(1)]],
    device const half* v [[buffer(2)]],
    device const float* beta_raw [[buffer(3)]],
    device const float* alpha_raw [[buffer(4)]],
    device const half* a_log [[buffer(5)]],
    device const half* dt_bias [[buffer(6)]],
    device float* state [[buffer(7)]],
    device float* out [[buffer(8)]],
    uint head [[threadgroup_position_in_grid]],
    uint i [[thread_position_in_threadgroup]]
) {
    if (i >= 128) {
        return;
    }

    const uint vec_base = head * 128;
    const uint state_base = head * 128 * 128;
    const float beta = qwen35_08b_sigmoid(beta_raw[head]);
    const float alpha = qwen35_08b_softplus(alpha_raw[head] + float(dt_bias[head]));
    const float a = exp(clamp(float(a_log[head]), -20.0f, 20.0f));
    const float g = exp(-a * alpha);

    float kv_mem = 0.0f;
    for (uint j = 0; j < 128; ++j) {
        kv_mem += state[state_base + i * 128 + j] * g * float(k[vec_base + j]);
    }

    const float delta = (float(v[vec_base + i]) - kv_mem) * beta;

    for (uint j = 0; j < 128; ++j) {
        state[state_base + i * 128 + j] =
            state[state_base + i * 128 + j] * g + float(k[vec_base + j]) * delta;
    }

    threadgroup_barrier(mem_flags::mem_device);

    float acc = 0.0f;
    for (uint j = 0; j < 128; ++j) {
        acc += state[state_base + i * 128 + j] * float(q[vec_base + j]);
    }

    out[vec_base + i] = acc;
}

kernel void qwen35_08b_deltanet_step_fused_qkv_norm_decay_f32_state(
    device const float* qkv [[buffer(0)]],
    device const float* beta_raw [[buffer(1)]],
    device const float* alpha_raw [[buffer(2)]],
    device const half* a_log [[buffer(3)]],
    device const half* dt_bias [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out [[buffer(6)]],
    uint head [[threadgroup_position_in_grid]],
    uint i [[thread_position_in_threadgroup]]
) {
    if (i >= 128) {
        return;
    }

    constexpr uint head_dim = 128;
    constexpr float eps = 1.0e-6f;
    threadgroup float q_scratch[128];
    threadgroup float k_scratch[128];

    const uint vec_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;

    const float qv = float(half(clamp(qkv[vec_base + i], -65504.0f, 65504.0f)));
    const float kv = float(half(clamp(qkv[2048 + vec_base + i], -65504.0f, 65504.0f)));
    const float vv = float(half(clamp(qkv[4096 + vec_base + i], -65504.0f, 65504.0f)));

    float q_ss = 0.0f;
    float k_ss = 0.0f;
    for (uint j = 0; j < head_dim; ++j) {
        const float qj = float(half(clamp(qkv[vec_base + j], -65504.0f, 65504.0f)));
        const float kj = float(half(clamp(qkv[2048 + vec_base + j], -65504.0f, 65504.0f)));
        q_ss += qj * qj;
        k_ss += kj * kj;
    }

    const float q_l2 = rsqrt(q_ss + float(head_dim) * eps);
    const float k_l2 = rsqrt(k_ss + float(head_dim) * eps);
    const float q_scale = rsqrt(float(head_dim));
    q_scratch[i] = float(half(clamp(qv * q_l2 * q_scale, -65504.0f, 65504.0f)));
    k_scratch[i] = float(half(clamp(kv * k_l2, -65504.0f, 65504.0f)));
    threadgroup_barrier(mem_flags::mem_threadgroup);

    const float beta = qwen35_08b_sigmoid(beta_raw[head]);
    const float alpha = qwen35_08b_softplus(alpha_raw[head] + float(dt_bias[head]));
    const float a = exp(clamp(float(a_log[head]), -20.0f, 20.0f));
    const float g = exp(-a * alpha);

    float kv_mem = 0.0f;
    for (uint j = 0; j < head_dim; ++j) {
        kv_mem += state[state_base + i * head_dim + j] * g * k_scratch[j];
    }

    const float delta = (vv - kv_mem) * beta;

    for (uint j = 0; j < head_dim; ++j) {
        state[state_base + i * head_dim + j] =
            state[state_base + i * head_dim + j] * g + k_scratch[j] * delta;
    }

    float acc = 0.0f;
    for (uint j = 0; j < head_dim; ++j) {
        acc += state[state_base + i * head_dim + j] * q_scratch[j];
    }

    out[vec_base + i] = acc;
}

kernel void qwen35_08b_deltanet_split_qkv_f32_to_fp16_h16d128(
    device const float* qkv [[buffer(0)]],
    device half* q [[buffer(1)]],
    device half* k [[buffer(2)]],
    device half* v [[buffer(3)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 2048) {
        return;
    }

    q[tid] = half(clamp(qkv[tid], -65504.0f, 65504.0f));
    k[tid] = half(clamp(qkv[2048 + tid], -65504.0f, 65504.0f));
    v[tid] = half(clamp(qkv[4096 + tid], -65504.0f, 65504.0f));
}

kernel void qwen35_08b_deltanet_split_qkv_norm_f32_to_fp16_h16d128(
    device const float* qkv [[buffer(0)]],
    device half* q [[buffer(1)]],
    device half* k [[buffer(2)]],
    device half* v [[buffer(3)]],
    uint head [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    if (head >= 16 || tid != 0) {
        return;
    }

    constexpr uint head_dim = 128;
    constexpr float eps = 1.0e-6f;
    const uint base = head * head_dim;
    float q_ss = 0.0f;
    float k_ss = 0.0f;

    for (uint i = 0; i < head_dim; ++i) {
        const float qv = float(half(clamp(qkv[base + i], -65504.0f, 65504.0f)));
        const float kv = float(half(clamp(qkv[2048 + base + i], -65504.0f, 65504.0f)));
        q_ss += qv * qv;
        k_ss += kv * kv;
    }

    const float q_l2 = rsqrt(q_ss + float(head_dim) * eps);
    const float k_l2 = rsqrt(k_ss + float(head_dim) * eps);
    const float q_scale = rsqrt(float(head_dim));
    for (uint i = 0; i < head_dim; ++i) {
        const uint idx = base + i;
        const float qv = float(half(clamp(qkv[idx], -65504.0f, 65504.0f)));
        const float kv = float(half(clamp(qkv[2048 + idx], -65504.0f, 65504.0f)));
        q[idx] = half(clamp(qv * q_l2 * q_scale, -65504.0f, 65504.0f));
        k[idx] = half(clamp(kv * k_l2, -65504.0f, 65504.0f));
        v[idx] = half(clamp(qkv[4096 + idx], -65504.0f, 65504.0f));
    }
}

kernel void qwen35_08b_deltanet_qk_l2norm_scale_h16d128(
    device half* q [[buffer(0)]],
    device half* k [[buffer(1)]],
    uint head [[thread_position_in_grid]]
) {
    if (head >= 16) {
        return;
    }

    constexpr uint head_dim = 128;
    constexpr float eps = 1.0e-6f;
    const uint base = head * head_dim;
    float q_ss = 0.0f;
    float k_ss = 0.0f;
    for (uint i = 0; i < head_dim; ++i) {
        const float qv = float(q[base + i]);
        const float kv = float(k[base + i]);
        q_ss += qv * qv;
        k_ss += kv * kv;
    }

    const float q_l2 = rsqrt(q_ss + float(head_dim) * eps);
    const float k_l2 = rsqrt(k_ss + float(head_dim) * eps);
    const float q_scale = rsqrt(float(head_dim));
    for (uint i = 0; i < head_dim; ++i) {
        const uint idx = base + i;
        q[idx] = half(clamp(float(q[idx]) * q_l2 * q_scale, -65504.0f, 65504.0f));
        k[idx] = half(clamp(float(k[idx]) * k_l2, -65504.0f, 65504.0f));
    }
}

kernel void qwen35_08b_deltanet_causal_conv1d_update_silu_c6144_k4(
    device const float* x [[buffer(0)]],
    device half* conv_state [[buffer(1)]],
    device const half* weight [[buffer(2)]],
    device const half* bias [[buffer(3)]],
    device float* out [[buffer(4)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 6144) {
        return;
    }

    constexpr uint channels = 6144;
    constexpr uint kernel_width = 4;
    const float s0 = float(conv_state[tid]);
    const float s1 = float(conv_state[channels + tid]);
    const float s2 = float(conv_state[2 * channels + tid]);
    const float x_new = x[tid];
    const uint w_base = tid * kernel_width;
    float acc = float(bias[tid]);
    acc += s0 * float(weight[w_base]);
    acc += s1 * float(weight[w_base + 1]);
    acc += s2 * float(weight[w_base + 2]);
    acc += x_new * float(weight[w_base + 3]);
    out[tid] = qwen35_08b_silu(acc);
    conv_state[tid] = half(clamp(s1, -65504.0f, 65504.0f));
    conv_state[channels + tid] = half(clamp(s2, -65504.0f, 65504.0f));
    conv_state[2 * channels + tid] = half(clamp(x_new, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_deltanet_activate_beta_gate_h16(
    device const float* beta_raw [[buffer(0)]],
    device const float* gate_raw [[buffer(1)]],
    device float* beta [[buffer(2)]],
    device float* gate [[buffer(3)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 16) {
        return;
    }

    const float b = clamp(beta_raw[tid], -20.0f, 20.0f);
    const float g = clamp(gate_raw[tid], -20.0f, 20.0f);
    beta[tid] = qwen35_08b_sigmoid(b);
    gate[tid] = qwen35_08b_sigmoid(g);
}

kernel void qwen35_08b_deltanet_activate_beta_decay_h16(
    device const float* beta_raw [[buffer(0)]],
    device const float* alpha_raw [[buffer(1)]],
    device const float* a_log [[buffer(2)]],
    device const float* dt_bias [[buffer(3)]],
    device float* beta [[buffer(4)]],
    device float* decay [[buffer(5)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 16) {
        return;
    }

    beta[tid] = qwen35_08b_sigmoid(beta_raw[tid]);
    const float alpha = qwen35_08b_softplus(alpha_raw[tid] + dt_bias[tid]);
    const float a = exp(clamp(a_log[tid], -20.0f, 20.0f));
    decay[tid] = exp(-a * alpha);
}

kernel void qwen35_08b_deltanet_apply_z_gate_f32_to_fp16_k2048(
    device const float* delta_out [[buffer(0)]],
    device const float* z [[buffer(1)]],
    device half* gated [[buffer(2)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 2048) {
        return;
    }

    const float zv = clamp(z[tid], -20.0f, 20.0f);
    const float gate = 1.0f / (1.0f + exp(-zv));
    const float out = clamp(delta_out[tid] * gate, -65504.0f, 65504.0f);
    gated[tid] = half(out);
}

kernel void qwen35_08b_deltanet_gated_rmsnorm_f32_to_fp16_h16d128(
    device const float* delta_out [[buffer(0)]],
    device const float* z [[buffer(1)]],
    device const float* norm_weight [[buffer(2)]],
    device half* gated [[buffer(3)]],
    uint head [[thread_position_in_grid]]
) {
    if (head >= 16) {
        return;
    }

    constexpr uint head_dim = 128;
    constexpr float eps = 1.0e-6f;
    const uint base = head * head_dim;
    float ss = 0.0f;
    for (uint i = 0; i < head_dim; ++i) {
        const float v = delta_out[base + i];
        ss += v * v;
    }
    const float inv_rms = rsqrt(ss / float(head_dim) + eps);
    for (uint i = 0; i < head_dim; ++i) {
        const uint idx = base + i;
        const float zv = clamp(z[idx], -20.0f, 20.0f);
        const float gate = qwen35_08b_silu(zv);
        const float out = delta_out[idx] * inv_rms * norm_weight[i] * gate;
        gated[idx] = half(clamp(out, -65504.0f, 65504.0f));
    }
}
