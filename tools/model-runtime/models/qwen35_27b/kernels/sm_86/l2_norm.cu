// L2 row-normalize: y[i,:] = x[i,:] / sqrt(sum(x[i,:]^2) + eps).
//
// Used on Q and K (per-row, one row == one head's head_dim vector)
// before handing to the GDN recurrence. Matches PyTorch's
// `torch.nn.functional.normalize(x, p=2, dim=-1, eps=eps)` semantics,
// where `eps` guards the denominator additively rather than as the
// ggml `rsqrtf(fmaxf(sum, eps*eps))` floor — we keep the additive
// form because it is what Qwen3.5's reference implementation uses on
// the Q/K path (RMSNorm-style eps).
//
// Shapes:
//   x : [n_rows, n_cols]  row-major bf16
//   y : [n_rows, n_cols]  row-major bf16 (same shape as x)
//
// Launch convention:
//   grid  = (n_rows, 1, 1)          one block per row.
//   block = (block_dim, 1, 1)       block_dim = min(n_cols, 1024)
//                                   rounded up to a multiple of 32.
//   shmem = 0 (static __shared__[32] used internally for the warp
//           fan-in; 32 slots = one per warp, sufficient for any
//           block up to 1024 threads).
//
// Extern "C" entry point: `l2_norm_bf16`. Math is done in f32 with
// bf16 loads/stores via `__bfloat162float` / `__float2bfloat16`.
// Load via cudarc's `module.load_function("l2_norm_bf16")`.

#include <cuda_bf16.h>

extern "C" __global__ void l2_norm_bf16(
    const __nv_bfloat16 * __restrict__ x,
    __nv_bfloat16 * __restrict__ y,
    int n_cols,
    float eps
) {
    const int row  = blockIdx.x;
    const int tid  = threadIdx.x;
    const int bdim = blockDim.x;

    const __nv_bfloat16 * x_row = x + (size_t)row * n_cols;
    __nv_bfloat16       * y_row = y + (size_t)row * n_cols;

    // Step 1: each thread accumulates squares over its strided
    // positions. Promote bf16 -> f32 for the multiply to preserve
    // precision during the accumulation.
    float sum_sq = 0.0f;
    for (int i = tid; i < n_cols; i += bdim) {
        const float xi = __bfloat162float(x_row[i]);
        sum_sq += xi * xi;
    }

    // Step 2: warp-level reduction via shfl_xor.
    const unsigned FULL_MASK = 0xffffffffu;
    #pragma unroll
    for (int mask = 16; mask > 0; mask >>= 1) {
        sum_sq += __shfl_xor_sync(FULL_MASK, sum_sq, mask, 32);
    }

    // Step 3: warp-leader writes to shared, first warp reduces
    // across warps. 32 slots = one per warp, enough for a 1024-
    // thread block (32 x 32 = 1024).
    __shared__ float warp_sums[32];
    const int warp_id = tid >> 5;   // tid / 32
    const int lane    = tid & 31;   // tid % 32
    if (lane == 0) {
        warp_sums[warp_id] = sum_sq;
    }
    __syncthreads();

    const int n_warps = (bdim + 31) >> 5;
    if (warp_id == 0) {
        float s = (tid < n_warps) ? warp_sums[lane] : 0.0f;
        #pragma unroll
        for (int mask = 16; mask > 0; mask >>= 1) {
            s += __shfl_xor_sync(FULL_MASK, s, mask, 32);
        }
        if (lane == 0) {
            warp_sums[0] = s;
        }
    }
    __syncthreads();

    // Step 4: broadcast the scale.
    //
    // We use `rsqrtf(sum_sq + eps)` (additive eps), not ggml's
    // `rsqrtf(fmaxf(sum_sq, eps*eps))`. Rationale: Qwen3.5's reference
    // graph uses RMSNorm-flavored eps on the Q/K normalize step, and
    // additive eps is strictly safer for the near-zero-input case
    // (keeps the denominator bounded rather than spiking).
    const float scale = rsqrtf(warp_sums[0] + eps);

    // Step 5: apply scale, write bf16.
    for (int i = tid; i < n_cols; i += bdim) {
        const float xi = __bfloat162float(x_row[i]);
        y_row[i] = __float2bfloat16(xi * scale);
    }
}
