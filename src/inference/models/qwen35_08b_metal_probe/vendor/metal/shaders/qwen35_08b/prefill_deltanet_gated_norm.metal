#include <metal_stdlib>
using namespace metal;

static inline float qwen35_08b_prefill_delta_norm_silu(float x) {
    return x / (1.0f + exp(-clamp(x, -20.0f, 20.0f)));
}

kernel void qwen35_08b_prefill_deltanet_gated_rmsnorm_tok_h16d128_f32_to_fp16(
    device const float* delta_tokens [[buffer(0)]],
    device const float* z_tokens [[buffer(1)]],
    device const float* norm_weight [[buffer(2)]],
    device half* gated_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
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

    threadgroup float partial[head_dim];

    const uint base = token * width + head * head_dim;
    const float v = delta_tokens[base + tid];
    partial[tid] = v * v;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 64; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(partial[0] / float(head_dim) + eps);
    const float gate = qwen35_08b_prefill_delta_norm_silu(z_tokens[base + tid]);
    const float out = v * inv_rms * norm_weight[tid] * gate;
    gated_tokens[base + tid] = half(clamp(out, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_deltanet_gated_rmsnorm_qkvz_tok_h16d128_f32_to_fp16(
    device const float* delta_tokens [[buffer(0)]],
    device const float* qkvz_tokens [[buffer(1)]],
    device const float* norm_weight [[buffer(2)]],
    device half* gated_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint qkv_width = width * 3;
    constexpr uint qkvz_width = qkv_width + width;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint tid = tid_pos.x;
    if (token >= tokens || head >= heads || tid >= head_dim) {
        return;
    }

    threadgroup float partial[head_dim];

    const uint base = token * width + head * head_dim;
    const float v = delta_tokens[base + tid];
    partial[tid] = v * v;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 64; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(partial[0] / float(head_dim) + eps);
    const float gate = qwen35_08b_prefill_delta_norm_silu(
        qkvz_tokens[token * qkvz_width + qkv_width + head * head_dim + tid]);
    const float out = v * inv_rms * norm_weight[tid] * gate;
    gated_tokens[base + tid] = half(clamp(out, -65504.0f, 65504.0f));
}

kernel void qwen35_08b_prefill_deltanet_gated_rmsnorm_simd32x4_tok_h16d128_f32_to_fp16(
    device const float* delta_tokens [[buffer(0)]],
    device const float* z_tokens [[buffer(1)]],
    device const float* norm_weight [[buffer(2)]],
    device half* gated_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint cols_per_lane = 4;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint lane = tid_pos.x;
    if (token >= tokens || head >= heads || lane >= 32) {
        return;
    }

    const uint base = token * width + head * head_dim;
    const uint col_base = lane * cols_per_lane;
    float local_sum = 0.0f;
    float values[cols_per_lane];
    for (uint j = 0; j < cols_per_lane; ++j) {
        const uint col = col_base + j;
        const float v = delta_tokens[base + col];
        values[j] = v;
        local_sum += v * v;
    }
    const float sum = simd_sum(local_sum);
    const float inv_rms = rsqrt(sum / float(head_dim) + eps);
    for (uint j = 0; j < cols_per_lane; ++j) {
        const uint col = col_base + j;
        const float gate = qwen35_08b_prefill_delta_norm_silu(z_tokens[base + col]);
        const float out = values[j] * inv_rms * norm_weight[col] * gate;
        gated_tokens[base + col] = half(clamp(out, -65504.0f, 65504.0f));
    }
}

kernel void qwen35_08b_prefill_deltanet_gated_rmsnorm_qkvz_simd32x4_tok_h16d128_f32_to_fp16(
    device const float* delta_tokens [[buffer(0)]],
    device const float* qkvz_tokens [[buffer(1)]],
    device const float* norm_weight [[buffer(2)]],
    device half* gated_tokens [[buffer(3)]],
    constant uint& tokens [[buffer(4)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    uint3 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint qkv_width = width * 3;
    constexpr uint qkvz_width = qkv_width + width;
    constexpr uint cols_per_lane = 4;
    constexpr float eps = 1.0e-6f;

    const uint token = tg_pos.x;
    const uint head = tg_pos.y;
    const uint lane = tid_pos.x;
    if (token >= tokens || head >= heads || lane >= 32) {
        return;
    }

    const uint base = token * width + head * head_dim;
    const uint z_base = token * qkvz_width + qkv_width + head * head_dim;
    const uint col_base = lane * cols_per_lane;
    float local_sum = 0.0f;
    float values[cols_per_lane];
    for (uint j = 0; j < cols_per_lane; ++j) {
        const uint col = col_base + j;
        const float v = delta_tokens[base + col];
        values[j] = v;
        local_sum += v * v;
    }
    const float sum = simd_sum(local_sum);
    const float inv_rms = rsqrt(sum / float(head_dim) + eps);
    for (uint j = 0; j < cols_per_lane; ++j) {
        const uint col = col_base + j;
        const float gate = qwen35_08b_prefill_delta_norm_silu(qkvz_tokens[z_base + col]);
        const float out = values[j] * inv_rms * norm_weight[col] * gate;
        gated_tokens[base + col] = half(clamp(out, -65504.0f, 65504.0f));
    }
}
