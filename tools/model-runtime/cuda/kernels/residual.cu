// Residual-add element-wise kernel. The transformer residual stream
// wires up as:
//
//   hidden = hidden + attn_out
//   hidden = hidden + mlp_out
//
// i.e. y[i] = x[i] + z[i] over the full flat buffer. Same dtype on all
// three tensors; bf16 promotes to f32 internally for the add so the
// rounding lives in the single final `__float2bfloat16` store (matches
// the accuracy convention established by silu_mul_bf16).
//
// Shapes:
//   x, z, y : [numel] flat — caller passes the total element count, any
//             shape works as long as all three match.
//
// Launch convention:
//   grid  = (ceil(numel / 256), 1, 1)
//   block = (256, 1, 1)
//   shmem = 0
//
// Extern "C" entry points:
//   * residual_add_f32   — float32 in/out
//   * residual_add_bf16  — __nv_bfloat16 in/out, f32 accum internally
// Load via cudarc's `module.load_function("residual_add_f32" |
// "residual_add_bf16")`.

#include <cuda_bf16.h>

// ---------------------------------------------------------------------------
// f32 path
// ---------------------------------------------------------------------------

extern "C" __global__ void residual_add_f32(
    const float * __restrict__ x,
    const float * __restrict__ z,
    float * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) {
        return;
    }
    y[i] = x[i] + z[i];
}

// ---------------------------------------------------------------------------
// bf16 path — promote to f32 for the add, demote on store.
// ---------------------------------------------------------------------------

extern "C" __global__ void residual_add_bf16(
    const __nv_bfloat16 * __restrict__ x,
    const __nv_bfloat16 * __restrict__ z,
    __nv_bfloat16 * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) {
        return;
    }
    const float a = __bfloat162float(x[i]);
    const float b = __bfloat162float(z[i]);
    y[i] = __float2bfloat16(a + b);
}
