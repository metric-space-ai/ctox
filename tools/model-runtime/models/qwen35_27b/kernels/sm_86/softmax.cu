// softmax — numerically stable row-softmax.
//
// Used at:
//   * attention (when running without FlashAttention) over scores rows.
//   * DDTree verify path (top-K row softmax over logits).
//
// Math (per row):
//   m     = max_i x[i]
//   e[i]  = exp(x[i] - m)
//   s     = sum_i e[i]
//   y[i]  = e[i] / s
//
// Shapes:
//   x, y  [n_rows, n_cols]  row-major f32
//
// Launch convention (mirrors rmsnorm):
//   grid  = (n_rows, 1, 1)
//   block = (block_dim, 1, 1)  block_dim = min(n_cols, 1024) rounded
//                              up to a warp (32).
//   shmem = 0 (uses __shared__[32] for the warp fan-in, same pattern
//           as rmsnorm).
//
// Extern "C" entry point: `softmax_f32`.

extern "C" __global__ void softmax_f32(
    const float * __restrict__ x,
    float * __restrict__ y,
    int n_cols
) {
    const int row  = blockIdx.x;
    const int tid  = threadIdx.x;
    const int bdim = blockDim.x;

    const float * x_row = x + (size_t)row * n_cols;
    float       * y_row = y + (size_t)row * n_cols;

    const unsigned FULL_MASK = 0xffffffffu;

    // Pass 1: per-thread max over strided columns.
    // Negative-infinity init so empty strides stay neutral.
    float m = -INFINITY;
    for (int i = tid; i < n_cols; i += bdim) {
        const float v = x_row[i];
        m = fmaxf(m, v);
    }

    // Warp-level max reduction via shfl_xor.
    #pragma unroll
    for (int mask = 16; mask > 0; mask >>= 1) {
        m = fmaxf(m, __shfl_xor_sync(FULL_MASK, m, mask, 32));
    }

    // Warp leaders publish to shared, then warp 0 reduces across warps.
    __shared__ float warp_mem[32];
    const int warp_id = tid >> 5;
    const int lane    = tid & 31;
    if (lane == 0) {
        warp_mem[warp_id] = m;
    }
    __syncthreads();

    const int n_warps = (bdim + 31) >> 5;
    if (warp_id == 0) {
        float r = (tid < n_warps) ? warp_mem[lane] : -INFINITY;
        #pragma unroll
        for (int mask = 16; mask > 0; mask >>= 1) {
            r = fmaxf(r, __shfl_xor_sync(FULL_MASK, r, mask, 32));
        }
        if (lane == 0) {
            warp_mem[0] = r;
        }
    }
    __syncthreads();

    const float row_max = warp_mem[0];

    // Pass 2: write e = exp(x - m) into y, per-thread partial sum.
    float sum = 0.0f;
    for (int i = tid; i < n_cols; i += bdim) {
        const float e = __expf(x_row[i] - row_max);
        y_row[i] = e;
        sum += e;
    }

    // Warp-level sum reduction.
    #pragma unroll
    for (int mask = 16; mask > 0; mask >>= 1) {
        sum += __shfl_xor_sync(FULL_MASK, sum, mask, 32);
    }

    // Reuse warp_mem[] for sum fan-in. Sync first so the earlier
    // max-reduction is finished reading it.
    __syncthreads();
    if (lane == 0) {
        warp_mem[warp_id] = sum;
    }
    __syncthreads();

    if (warp_id == 0) {
        float s = (tid < n_warps) ? warp_mem[lane] : 0.0f;
        #pragma unroll
        for (int mask = 16; mask > 0; mask >>= 1) {
            s += __shfl_xor_sync(FULL_MASK, s, mask, 32);
        }
        if (lane == 0) {
            warp_mem[0] = s;
        }
    }
    __syncthreads();

    const float inv_sum = 1.0f / warp_mem[0];

    // Pass 3: normalize in-place on y.
    for (int i = tid; i < n_cols; i += bdim) {
        y_row[i] *= inv_sum;
    }
}
