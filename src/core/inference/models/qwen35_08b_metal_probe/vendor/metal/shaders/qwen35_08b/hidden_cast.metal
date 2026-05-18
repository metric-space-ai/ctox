#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_hidden_f32_to_fp16_k1024(
    device const float* src [[buffer(0)]],
    device half* dst [[buffer(1)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 1024) {
        return;
    }

    float v = src[tid];
    v = clamp(v, -65504.0f, 65504.0f);
    dst[tid] = half(v);
}

kernel void qwen35_08b_residual_add_f32_to_fp16_k1024(
    device const half* residual [[buffer(0)]],
    device const float* delta [[buffer(1)]],
    device half* dst [[buffer(2)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 1024) {
        return;
    }

    const float v = clamp(float(residual[tid]) + delta[tid], -65504.0f, 65504.0f);
    dst[tid] = half(v);
}

kernel void qwen35_08b_prefill_residual_add_f32_to_fp16_k1024(
    device const half* residual [[buffer(0)]],
    device const float* delta [[buffer(1)]],
    device half* dst [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    uint tid [[thread_position_in_grid]]
) {
    constexpr uint hidden = 1024;
    if (tid >= tokens * hidden) {
        return;
    }

    const float v = clamp(float(residual[tid]) + delta[tid], -65504.0f, 65504.0f);
    dst[tid] = half(v);
}

kernel void qwen35_08b_prefill_residual_add_fp16_to_fp16_k1024(
    device const half* residual [[buffer(0)]],
    device const half* delta [[buffer(1)]],
    device half* dst [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    uint tid [[thread_position_in_grid]]
) {
    constexpr uint hidden = 1024;
    if (tid >= tokens * hidden) {
        return;
    }

    const float v = clamp(float(residual[tid]) + float(delta[tid]), -65504.0f, 65504.0f);
    dst[tid] = half(v);
}

kernel void qwen35_08b_prefill_rmsnorm_fp16_k1024(
    device const half* src [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device half* dst [[buffer(2)]],
    constant uint& tokens [[buffer(3)]],
    uint token [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    constexpr uint hidden = 1024;
    threadgroup float partial[256];

    if (token >= tokens) {
        return;
    }

    float ss = 0.0f;
    const uint base = token * hidden;
    for (uint col = tid; col < hidden; col += 256) {
        const float v = float(src[base + col]);
        ss += v * v;
    }
    partial[tid] = ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(partial[0] / float(hidden) + 1.0e-6f);
    for (uint col = tid; col < hidden; col += 256) {
        const float v = clamp(float(src[base + col]) * inv_rms * float(norm_weight[col]), -65504.0f, 65504.0f);
        dst[base + col] = half(v);
    }
}

kernel void qwen35_08b_residual_add_f32_to_f32_k1024(
    device const float* residual [[buffer(0)]],
    device const float* delta [[buffer(1)]],
    device float* dst [[buffer(2)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= 1024) {
        return;
    }

    dst[tid] = residual[tid] + delta[tid];
}

kernel void qwen35_08b_rmsnorm_hidden_fp16_k1024(
    device const half* src [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device half* dst [[buffer(2)]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[256];
    float ss = 0.0f;
    for (uint col = tid; col < 1024; col += 256) {
        const float v = float(src[col]);
        ss += v * v;
    }
    partial[tid] = ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(partial[0] / 1024.0f + 1.0e-6f);
    for (uint col = tid; col < 1024; col += 256) {
        const float v = clamp(float(src[col]) * inv_rms * float(norm_weight[col]), -65504.0f, 65504.0f);
        dst[col] = half(v);
    }
}

kernel void qwen35_08b_rmsnorm_hidden_f32_k1024(
    device const float* src [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device float* dst [[buffer(2)]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[256];
    float ss = 0.0f;
    for (uint col = tid; col < 1024; col += 256) {
        const float v = src[col];
        ss += v * v;
    }
    partial[tid] = ss;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    const float inv_rms = rsqrt(partial[0] / 1024.0f + 1.0e-6f);
    for (uint col = tid; col < 1024; col += 256) {
        dst[col] = src[col] * inv_rms * float(norm_weight[col]);
    }
}
