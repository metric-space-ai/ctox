// mmvq_q8_0 — Q8_0 matrix-vector matmul (single-query decode path).
//
// Ported from llama.cpp / ggml-cuda:
//   deps/llama.cpp/ggml/src/ggml-cuda/mmvq.cu       (mul_mat_vec_q template)
//   deps/llama.cpp/ggml/src/ggml-cuda/vecdotq.cuh   (vec_dot_q8_0_q8_1)
//   deps/llama.cpp/ggml/src/ggml-cuda/convert.cu    (dequantize_row_q8_0)
// Upstream license: MIT (compatible with this repo).
//
// First-pass port (see mmq_q4k task): the reference quantizes the
// activation row `x` to q8_1 on input and uses DP4A. We dequantize
// Q8_0 to f32 and compute a plain f32 inner product instead —
// correctness first, performance second. The DP4A/q8_1 path is a
// follow-up.
//
// TODO: port the batched mmq_q8_0 (mat-mat) path. This file only
//       provides the mmvq (batch = 1) variant used by decode.
// TODO: fuse with a q8_1 activation quantization for ~2-3× speedup.
//
// Math:
//   A is a [n, k] matrix in Q8_0 with (k/32) 34-byte blocks per
//   column (k must be a multiple of 32). Q8_0 block:
//     half   d;        // super-scale
//     int8_t qs[32];   // 32 signed quantized values
//   → 32 elements per block. Per element: y = d * qs[i].
//
// x is the f32 input vector of length k. y is the f32 (or f16) output
// vector of length n. We compute: y[col] = sum_{i in 0..k} A[col,i] * x[i].
// Row-major over columns: block b of column `col` starts at
// (col * blocks_per_col + b) * 34 bytes in the packed buffer.
//
// Launch convention (mirrors mmq_q4k / mmq_q6k):
//   grid  = (n / NCOLS_Y, 1, 1)  with NCOLS_Y=2 output columns per block
//   block = (32, NCOLS_Y, 1)     one warp per output column
//   shmem = 0 — partial reductions in registers + warp-shuffle
//
// Work distribution within a warp: each lane processes exactly one of
// the 32 elements in each block. We loop over the `blocks_per_col`
// blocks, accumulating into a per-lane float, then warp-reduce at
// the end.

#include <cstdint>
#include <cuda_fp16.h>

#define Q8_0_BLOCK_ELEMS 32
#define BLOCK_Q8_0_BYTES 34

// Matches llama.cpp's block_q8_0 layout exactly.
struct __align__(2) block_q8_0 {
    __half d;
    int8_t qs[32];
};
static_assert(sizeof(block_q8_0) == BLOCK_Q8_0_BYTES,
              "block_q8_0 must be 34 bytes");

// Warp-level reduction via shfl_xor. Sums across the 32 lanes.
static __device__ __forceinline__ float warp_reduce_sum_f32(float v) {
    const unsigned FULL_MASK = 0xffffffffu;
    #pragma unroll
    for (int mask = 16; mask > 0; mask >>= 1) {
        v += __shfl_xor_sync(FULL_MASK, v, mask, 32);
    }
    return v;
}

// Core kernel body, templated on output type (float or __half).
template <typename out_t>
static __device__ __forceinline__ void mmvq_q8_0_body(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    out_t * __restrict__ y
) {
    // threadIdx.y picks which of the 2 columns this warp owns.
    const int col = blockIdx.x * 2 + threadIdx.y;
    if (col >= n) return;

    const int lane = threadIdx.x;                     // 0..31
    const int blocks_per_col = k / Q8_0_BLOCK_ELEMS;  // whole blocks per column

    // Base pointer to this column's blocks in the packed Q8_0 buffer.
    const block_q8_0 * col_blocks =
        reinterpret_cast<const block_q8_0 *>(a_bytes) +
        (size_t)col * blocks_per_col;

    float acc = 0.0f;

    // Each lane owns element `lane` within each block. Iterate blocks
    // serially, accumulating the dequantized product.
    for (int b = 0; b < blocks_per_col; ++b) {
        const block_q8_0 * blk = col_blocks + b;
        const float d = __half2float(blk->d);

        const int q = (int)blk->qs[lane];             // signed i8
        const int x_idx = b * Q8_0_BLOCK_ELEMS + lane;
        const float xv = x[x_idx];

        acc += (d * (float)q) * xv;
    }

    // Warp reduces to lane 0 and writes the scalar result.
    acc = warp_reduce_sum_f32(acc);
    if (lane == 0) {
        if constexpr (sizeof(out_t) == sizeof(float)) {
            *reinterpret_cast<float *>(&y[col]) = acc;
        } else {
            y[col] = __float2half(acc);
        }
    }
}

extern "C" __global__ void mmvq_q8_0_f32_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    float * __restrict__ y
) {
    mmvq_q8_0_body<float>(a_bytes, k, n, x, y);
}

extern "C" __global__ void mmvq_q8_0_f16_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    __half * __restrict__ y
) {
    mmvq_q8_0_body<__half>(a_bytes, k, n, x, y);
}
