#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_stream_rw_u32x4(
    device const uint4* src [[buffer(0)]],
    device uint4* dst [[buffer(1)]],
    constant uint& n_vec4 [[buffer(2)]],
    uint gid [[thread_position_in_grid]]
) {
    if (gid >= n_vec4) {
        return;
    }
    uint4 v = src[gid];
    dst[gid] = uint4(v.x ^ 0x9e3779b9u, v.y + 1u, v.z ^ v.x, v.w + v.y);
}
