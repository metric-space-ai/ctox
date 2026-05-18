#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_matvec_fp16_k1024_f32(
    device const half* x [[buffer(0)]],
    device const half* w [[buffer(1)]],
    device float* y [[buffer(2)]],
    constant uint& rows [[buffer(3)]],
    uint row [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    if (row >= rows) {
        return;
    }

    threadgroup float partial[256];
    float acc = 0.0f;
    const uint cols = 1024;
    const uint base = row * cols;

    for (uint col = tid; col < cols; col += 256) {
        acc += float(w[base + col]) * float(x[col]);
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
