#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_rms_matvec_fp16_k1024_f32(
    device const half* x [[buffer(0)]],
    device const half* norm_weight [[buffer(1)]],
    device const half* w [[buffer(2)]],
    device float* y [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    uint row [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    if (row >= rows) {
        return;
    }

    threadgroup float partial[256];

    float ss = 0.0f;
    for (uint col = tid; col < 1024; col += 256) {
        float v = float(x[col]);
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
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float acc = 0.0f;
    const uint base = row * 1024;
    for (uint col = tid; col < 1024; col += 256) {
        float normed = float(x[col]) * inv_rms * float(norm_weight[col]);
        acc += float(w[base + col]) * normed;
    }

    partial[tid] = acc;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        y[row] = partial[0];
    }
}
