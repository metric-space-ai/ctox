// RMSNorm — template kernel establishing the per-kernel conventions
// for the ctox-engine-cuda crate. Follow this pattern when porting
// further ops (silu_mul, softmax, mmq_q4k, gated_delta_net, …).
//
// Math:
//   rms  = sqrt(mean(x^2) + eps)
//   y[i] = (x[i] / rms) * weight[i]
//
// Shapes:
//   x      [n_tokens, hidden_dim]  row-major f32
//   weight [hidden_dim]            f32
//   y      [n_tokens, hidden_dim]  f32 (pre-allocated output)
//
// Launch convention:
//   grid  = (n_tokens, 1, 1)
//   block = (block_dim, 1, 1)  where block_dim = min(hidden_dim, 1024)
//                              rounded up to a multiple of 32 (warp size).
//   shmem = 0 (we use a statically-sized __shared__[32] for the warp-
//           reduction fan-in; 32 is the max number of warps in a 1024-
//           thread block, so the slot is always sufficient).
//
// Extern "C" entry point: `rmsnorm_f32`. Load via cudarc's
// `module.load_function("rmsnorm_f32")`.

extern "C" __global__ void rmsnorm_f32(
    const float * __restrict__ x,
    const float * __restrict__ weight,
    float * __restrict__ y,
    int hidden_dim,
    float eps
) {
    const int token = blockIdx.x;
    const int tid   = threadIdx.x;
    const int bdim  = blockDim.x;

    const float * x_row = x + (size_t)token * hidden_dim;
    float       * y_row = y + (size_t)token * hidden_dim;

    // Step 1: each thread sums squares over its strided positions.
    float sum_sq = 0.0f;
    for (int i = tid; i < hidden_dim; i += bdim) {
        const float xi = x_row[i];
        sum_sq += xi * xi;
    }

    // Step 2: warp-level reduction via shfl_xor.
    const unsigned FULL_MASK = 0xffffffffu;
    #pragma unroll
    for (int mask = 16; mask > 0; mask >>= 1) {
        sum_sq += __shfl_xor_sync(FULL_MASK, sum_sq, mask, 32);
    }

    // Step 3: warp-leader writes to shared, then first warp reduces
    // across warps. 32 slots = one per warp, enough for a 1024-thread
    // block (32 × 32 = 1024).
    __shared__ float warp_sums[32];
    const int warp_id = tid >> 5;   // tid / 32
    const int lane    = tid & 31;   // tid % 32
    if (lane == 0) {
        warp_sums[warp_id] = sum_sq;
    }
    __syncthreads();

    // First warp: gather the n_warps partial sums and reduce again.
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

    // Step 4: broadcast the RMS scale to every thread.
    const float mean_sq = warp_sums[0] / (float)hidden_dim;
    const float scale   = rsqrtf(mean_sq + eps);

    // Step 5: apply scale × weight, write to y_row.
    for (int i = tid; i < hidden_dim; i += bdim) {
        y_row[i] = x_row[i] * scale * weight[i];
    }
}
