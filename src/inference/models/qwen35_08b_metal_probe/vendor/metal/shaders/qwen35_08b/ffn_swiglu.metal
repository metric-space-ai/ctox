#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_ffn_swiglu_fp16(
    device const half* x [[buffer(0)]],
    device const half* gate_w [[buffer(1)]],
    device const half* up_w [[buffer(2)]],
    device const half* down_w [[buffer(3)]],
    device float* y [[buffer(4)]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float partial[256];
    threadgroup float activated[3584];

    for (uint row = tid; row < 3584; row += 256) {
        float gate = 0.0f;
        float up = 0.0f;
        const uint base = row * 1024;
        for (uint col = 0; col < 1024; ++col) {
            const float xv = float(x[col]);
            gate += float(gate_w[base + col]) * xv;
            up += float(up_w[base + col]) * xv;
        }
        const float sig = 1.0f / (1.0f + exp(-gate));
        activated[row] = gate * sig * up;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint row = tid; row < 1024; row += 256) {
        float acc = 0.0f;
        const uint base = row * 3584;
        for (uint col = 0; col < 3584; ++col) {
            acc += float(down_w[base + col]) * activated[col];
        }
        y[row] = acc;
    }
}
