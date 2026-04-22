// Bulk dtype conversion kernels used at dtype boundaries in the layer
// stack — rmsnorm operates in f32 while the activation flow is bf16/f16,
// so we need cheap element-wise casts on the hot path.
//
// Entry points (all pairs we actually wire up):
//   * cast_bf16_to_f32
//   * cast_f32_to_bf16
//   * cast_f16_to_f32
//   * cast_f32_to_f16
//
// Semantics: `y[i] = (target_dtype) x[i]`. For f32→bf16/f16 we use the
// CUDA hardware `__float2bfloat16` / `__float2half` intrinsics, which
// round to nearest even — matching the half crate's `from_f32` on the
// host so the CPU-side round-trip check is bit-exact.
//
// Shapes:
//   x, y : [numel] flat, numel > 0. Both tensors must share the same
//          element count.
//
// Launch convention:
//   grid  = (ceil(numel / 256), 1, 1)
//   block = (256, 1, 1)
//   shmem = 0

#include <cuda_bf16.h>
#include <cuda_fp16.h>

// ---------------------------------------------------------------------------
// bf16 <-> f32
// ---------------------------------------------------------------------------

extern "C" __global__ void cast_bf16_to_f32(
    const __nv_bfloat16 * __restrict__ x,
    float * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) {
        return;
    }
    y[i] = __bfloat162float(x[i]);
}

extern "C" __global__ void cast_f32_to_bf16(
    const float * __restrict__ x,
    __nv_bfloat16 * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) {
        return;
    }
    y[i] = __float2bfloat16(x[i]);
}

// ---------------------------------------------------------------------------
// f16 <-> f32
// ---------------------------------------------------------------------------

extern "C" __global__ void cast_f16_to_f32(
    const __half * __restrict__ x,
    float * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) {
        return;
    }
    y[i] = __half2float(x[i]);
}

extern "C" __global__ void cast_f32_to_f16(
    const float * __restrict__ x,
    __half * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) {
        return;
    }
    y[i] = __float2half(x[i]);
}
