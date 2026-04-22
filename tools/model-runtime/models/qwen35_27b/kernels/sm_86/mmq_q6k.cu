// mmvq_q6k — Q6_K matrix-vector matmul (single-query decode path).
//
// Ported from llama.cpp / ggml-cuda:
//   deps/llama.cpp/ggml/src/ggml-cuda/mmvq.cu       (mul_mat_vec_q template)
//   deps/llama.cpp/ggml/src/ggml-cuda/vecdotq.cuh   (vec_dot_q6_K_q8_1)
//   deps/llama.cpp/ggml/src/ggml-cuda/convert.cu    (dequantize_block_q6_K)
// Upstream license: MIT (compatible with this repo).
//
// First-pass port (see mmq_q4k task): the reference quantizes the
// activation row `x` to q8_1 on input and uses DP4A. We dequantize Q6
// to f32 and compute a plain f32 inner product instead — correctness
// first, performance second. The DP4A/q8_1 path is a follow-up.
//
// TODO: port the batched mmq_q6k (mat-mat) path. This file only
//       provides the mmvq (batch = 1) variant used by decode.
// TODO: fuse with a q8_1 activation quantization for ~2-3× speedup.
//
// Math:
//   A is a [n, k] matrix in Q6_K with (k/256) 210-byte blocks per
//   column (k must be a multiple of 256). Q6_K block:
//     uint8_t ql[128];     // low 4 bits of each of 256 quants, 2 per byte
//     uint8_t qh[64];      // high 2 bits of each of 256 quants, 4 per byte
//     int8_t  scales[16];  // per-16-element signed scale
//     half    d;           // super-block scale
//   → 16 sub-scales × 16 elements = 256 elements per block.
//
//   The reference `dequantize_row_q6_K` walks the block in two halves
//   of 128 elements (n = 0 and n = 128). Within each half, for
//   l in 0..32:
//     is  = l / 16 (0 or 1)
//     q1 = ((ql_n[l]      & 0x0F) | ((qh_n[l] >> 0) & 3) << 4) - 32
//     q2 = ((ql_n[l + 32] & 0x0F) | ((qh_n[l] >> 2) & 3) << 4) - 32
//     q3 = ((ql_n[l]      >>   4) | ((qh_n[l] >> 4) & 3) << 4) - 32
//     q4 = ((ql_n[l + 32] >>   4) | ((qh_n[l] >> 6) & 3) << 4) - 32
//     y[l +  0] = d * sc_n[is + 0] * q1
//     y[l + 32] = d * sc_n[is + 2] * q2
//     y[l + 64] = d * sc_n[is + 4] * q3
//     y[l + 96] = d * sc_n[is + 6] * q4
//   where ql_n = ql + n/2, qh_n = qh + n/4, sc_n = scales + n/16
//   (mirroring dequant_q6_k_to_bf16 in src/gguf.rs).
//
// x is the f32 input vector of length k. y is the f32 (or f16) output
// vector of length n. We compute: y[col] = sum_{i in 0..k} A[col,i] * x[i].
// Row-major over columns: block b of column `col` starts at
// (col * blocks_per_col + b) * 210 bytes in the packed buffer.
//
// Launch convention (mirrors mmq_q4k):
//   grid  = (n / NCOLS_Y, 1, 1)  with NCOLS_Y=2 output columns per block
//   block = (32, NCOLS_Y, 1)     one warp per output column
//   shmem = 0 — partial reductions in registers + warp-shuffle

#include <cstdint>
#include <cuda_fp16.h>

#define QK_K 256
#define BLOCK_Q6K_BYTES 210

// Matches llama.cpp's block_q6_K layout exactly.
struct __align__(2) block_q6_K {
    uint8_t ql[128];
    uint8_t qh[64];
    int8_t  scales[16];
    __half  d;
};
static_assert(sizeof(block_q6_K) == BLOCK_Q6K_BYTES,
              "block_q6_K must be 210 bytes");

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
static __device__ __forceinline__ void mmvq_q6k_body(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    out_t * __restrict__ y
) {
    // threadIdx.y picks which of the 2 columns this warp owns.
    const int col = blockIdx.x * 2 + threadIdx.y;
    if (col >= n) return;

    const int lane = threadIdx.x;            // 0..31
    const int blocks_per_col = k / QK_K;     // whole blocks per column

    // Base pointer to this column's blocks in the packed Q6_K buffer.
    const block_q6_K * col_blocks =
        reinterpret_cast<const block_q6_K *>(a_bytes) +
        (size_t)col * blocks_per_col;

    float acc = 0.0f;

    // Each warp iterates blocks. Within a block there are two 128-
    // element halves (n_half = 0, 1). Each half produces 128 outputs
    // = 4 quads of 32 elems. We map the 32 lanes 1:1 to `l in 0..32`
    // within each half.
    for (int b = 0; b < blocks_per_col; ++b) {
        const block_q6_K * blk = col_blocks + b;
        const float d = __half2float(blk->d);

        #pragma unroll
        for (int n_half = 0; n_half < 2; ++n_half) {
            const int n_off = n_half * 128;

            // Sub-pointers into this half (see header comment).
            const uint8_t * ql_n = blk->ql + (n_off / 2);   // 64 bytes
            const uint8_t * qh_n = blk->qh + (n_off / 4);   // 32 bytes
            const int8_t  * sc_n = blk->scales + (n_off / 16); // 8 i8's

            const int l = lane;        // 0..31
            const int is = l >> 4;     // 0 or 1

            // Decode the four 6-bit quants for this (l, n_half).
            const uint8_t qll0 = ql_n[l];
            const uint8_t qll1 = ql_n[l + 32];
            const uint8_t qhh  = qh_n[l];
            const int q1 = (int)((qll0 & 0x0F) | (((qhh >> 0) & 3) << 4)) - 32;
            const int q2 = (int)((qll1 & 0x0F) | (((qhh >> 2) & 3) << 4)) - 32;
            const int q3 = (int)((qll0 >>   4) | (((qhh >> 4) & 3) << 4)) - 32;
            const int q4 = (int)((qll1 >>   4) | (((qhh >> 6) & 3) << 4)) - 32;

            const float s0 = (float)sc_n[is + 0];
            const float s2 = (float)sc_n[is + 2];
            const float s4 = (float)sc_n[is + 4];
            const float s6 = (float)sc_n[is + 6];

            // Element positions inside the block: n_off + l + {0, 32, 64, 96}.
            const int x_base = b * QK_K + n_off + l;
            const float x0 = x[x_base +  0];
            const float x1 = x[x_base + 32];
            const float x2 = x[x_base + 64];
            const float x3 = x[x_base + 96];

            acc += (d * s0 * (float)q1) * x0;
            acc += (d * s2 * (float)q2) * x1;
            acc += (d * s4 * (float)q3) * x2;
            acc += (d * s6 * (float)q4) * x3;
        }
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

extern "C" __global__ void mmvq_q6k_f32_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    float * __restrict__ y
) {
    mmvq_q6k_body<float>(a_bytes, k, n, x, y);
}

extern "C" __global__ void mmvq_q6k_f16_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    __half * __restrict__ y
) {
    mmvq_q6k_body<__half>(a_bytes, k, n, x, y);
}
