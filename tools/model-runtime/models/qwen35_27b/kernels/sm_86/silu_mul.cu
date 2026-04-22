// SiLU-and-multiply fused activation for the SwiGLU MLP block.
//
// Math:
//   silu(x) = x * sigmoid(x) = x / (1 + exp(-x))
//   y[i]    = silu(gate[i]) * up[i]
//
// Shapes:
//   gate, up, y : [numel] flat (caller passes the total element count;
//                 any shape works as long as all three match).
//
// Launch convention:
//   grid  = (ceil(numel / 256), 1, 1)
//   block = (256, 1, 1)
//   shmem = 0
//
// Memory-bound per-element op — two reads, one write, no reduction.
//
// Extern "C" entry points:
//   * silu_mul_f32   — float32 in/out
//   * silu_mul_bf16  — __nv_bfloat16 in/out, math done in f32
// Load via cudarc's `module.load_function("silu_mul_f32" | "silu_mul_bf16")`.

#include <cuda_bf16.h>

// ---------------------------------------------------------------------------
// f32 path
// ---------------------------------------------------------------------------

extern "C" __global__ void silu_mul_f32(
    const float * __restrict__ gate,
    const float * __restrict__ up,
    float * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) {
        return;
    }
    const float g = gate[i];
    const float u = up[i];
    // silu(g) = g / (1 + exp(-g)); __expf is the fast-math intrinsic,
    // accurate enough for activation use (f32 tolerance is 1e-3 relative).
    const float silu_g = g / (1.0f + __expf(-g));
    y[i] = silu_g * u;
}

// ---------------------------------------------------------------------------
// bf16 path — promote to f32 for the sigmoid + multiply, demote on store.
// ---------------------------------------------------------------------------

extern "C" __global__ void silu_mul_bf16(
    const __nv_bfloat16 * __restrict__ gate,
    const __nv_bfloat16 * __restrict__ up,
    __nv_bfloat16 * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) {
        return;
    }
    const float g = __bfloat162float(gate[i]);
    const float u = __bfloat162float(up[i]);
    const float silu_g = g / (1.0f + __expf(-g));
    const float out = silu_g * u;
    y[i] = __float2bfloat16(out);
}
