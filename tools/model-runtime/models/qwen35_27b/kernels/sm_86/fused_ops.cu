// Small fused element-wise kernels that used to live as host
// round-trips in `layers/full_attention.rs`. Each host round-trip was
// a hard stop for CUDA graph capture, so collapsing them onto the
// device unblocks A3 (decode graph capture) as well as trimming the
// 5+ per-attention-head host syncs that add up across 16 FA layers.
//
// Entry points:
//   * scale_add_f32      — y[i] = x[i] * scale + y[i]
//                          (used for the score * (1/sqrt(d_k)) + mask
//                           pre-softmax.)
//   * sigmoid_mul_bf16   — y[i] *= sigmoid(x[i])
//                          (fuses the attention gate's sigmoid and the
//                           subsequent elementwise mul into one kernel;
//                           replaces both `sigmoid_host_bf16` and
//                           `elementwise_mul_host_bf16`.)
//   * sigmoid_bf16       — y[i] = sigmoid(x[i])
//                          (kept in case a caller wants the standalone
//                           sigmoid without the mul — e.g. for GDN's
//                           gate computation, future work.)
//
// Launch convention (all three):
//   grid  = (ceil(numel / 256), 1, 1)
//   block = (256, 1, 1)
//   shmem = 0

#include <cuda_bf16.h>
#include <math.h>

extern "C" __global__ void scale_add_f32(
    const float * __restrict__ x,
    float * __restrict__ y,   // in: mask; out: scores*scale + mask
    float scale,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) return;
    y[i] = x[i] * scale + y[i];
}

// Same as scale_add_f32 but reads the bias (mask) from a separate
// buffer so the caller can keep a SINGLE pre-uploaded mask on the
// device and reuse it across N attention heads without re-copying
// into `y` between calls.
extern "C" __global__ void scale_add_with_bias_f32(
    const float * __restrict__ x,      // scores [numel]
    const float * __restrict__ bias,   // shared mask [numel]
    float * __restrict__ y,            // out: x*scale + bias
    float scale,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) return;
    y[i] = x[i] * scale + bias[i];
}

// bf16 sigmoid via f32 conversion (no hardware bf16 math on sm_86 —
// the ~8k-element gate tensor does not benefit from bf16 ops anyway).
extern "C" __global__ void sigmoid_mul_bf16(
    const __nv_bfloat16 * __restrict__ x,  // sigmoid input
    __nv_bfloat16 * __restrict__ y,        // multiplicand, overwritten
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) return;
    const float xf = __bfloat162float(x[i]);
    const float yf = __bfloat162float(y[i]);
    const float sig = 1.0f / (1.0f + expf(-xf));
    y[i] = __float2bfloat16(yf * sig);
}

extern "C" __global__ void sigmoid_bf16(
    const __nv_bfloat16 * __restrict__ x,
    __nv_bfloat16 * __restrict__ y,
    int numel
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= numel) return;
    const float xf = __bfloat162float(x[i]);
    y[i] = __float2bfloat16(1.0f / (1.0f + expf(-xf)));
}

// 2-D transpose: src [rows, cols] -> dst [cols, rows]. Naive one-thread
// -per-element copy with index flip; fine for the attention head
// shapes we use (kv_len × head_dim ≲ 16K elements per call). The old
// implementation downloaded src, transposed on host, re-uploaded —
// 384 host round-trips per forward at 16 FA layers × 24 Q-heads.
extern "C" __global__ void transpose_2d_bf16(
    const __nv_bfloat16 * __restrict__ src,  // [rows, cols]
    __nv_bfloat16 * __restrict__ dst,        // [cols, rows]
    int rows,
    int cols
) {
    const int total = rows * cols;
    const int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total) return;
    const int r = idx / cols;
    const int c = idx - r * cols;
    dst[c * rows + r] = src[idx];
}
