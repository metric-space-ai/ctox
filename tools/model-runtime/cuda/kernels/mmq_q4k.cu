// mmvq_q4k — Q4_K_M matrix-vector matmul (single-query decode path).
//
// Ported from llama.cpp / ggml-cuda:
//   deps/llama.cpp/ggml/src/ggml-cuda/mmvq.cu      (mul_mat_vec_q template)
//   deps/llama.cpp/ggml/src/ggml-cuda/vecdotq.cuh  (vec_dot_q4_K_q8_1)
//   deps/llama.cpp/ggml/src/ggml-cuda/convert.cu   (dequantize_block_q4_K)
// Upstream license: MIT (compatible with this repo).
//
// First-pass port (see task A): the reference quantizes the activation
// row `x` to q8_1 on input and uses DP4A. We dequantize Q4 to f32 and
// compute a plain f32 inner product instead — correctness first,
// performance second. The DP4A/q8_1 path is a follow-up.
//
// TODO: port the batched mmq_q4k (mat-mat) path. This file only
//       provides the mmvq (batch = 1) variant used by decode.
// TODO: fuse with a q8_1 activation quantization for ~2-3× speedup.
//
// Math:
//   A is a [n, k] matrix in Q4_K_M with (k/256) 144-byte blocks per
//   column (k must be a multiple of 256). Q4_K_M block:
//     half   d;              // super-block scale
//     half   dmin;           // super-block min
//     uint8_t scales[12];    // 6-bit packed per-sub-block (sc, m)
//     uint8_t qs[128];       // 4-bit packed quants, 2 per byte
//   → 8 sub-blocks × 32 elements = 256 elements per block.
//   Per-sub-block j in 0..8:  y_l = d * sc_j * q_l - dmin * m_j
//   where (sc_j, m_j) come from `get_scale_min_k4` over `scales[12]`.
//
// x is the f32 input vector of length k. y is the f32 (or f16) output
// vector of length n. We compute: y[col] = sum_{i in 0..k} A[col,i] * x[i].
// Row-major over columns: block b of column `col` starts at
// (col * blocks_per_col + b) * 144 bytes in the packed buffer.
//
// Launch convention (mirrors ggml-cuda's mmvq):
//   grid  = (n / NCOLS_Y, 1, 1)  with NCOLS_Y=2 output columns per block
//   block = (32, NCOLS_Y, 1)     one warp per output column
//   shmem = 0 — partial reductions in registers + warp-shuffle

#include <cstdint>
#include <cuda_fp16.h>

#define QK_K 256
#define BLOCK_Q4K_BYTES 144
#define Q4K_SUBBLOCKS 8       // QK_K / 32
#define Q4K_SUB_ELEMS 32

// Matches llama.cpp's block_q4_K layout exactly.
struct __align__(4) block_q4_K {
    __half  d;
    __half  dmin;
    uint8_t scales[12];
    uint8_t qs[128];
};
static_assert(sizeof(block_q4_K) == BLOCK_Q4K_BYTES,
              "block_q4_K must be 144 bytes");

// Port of `get_scale_min_k4` from ggml-cuda/convert.cu. Given a
// sub-block index j in [0, 8), decode (scale, min) from the packed
// 12-byte `scales` array.
static __device__ __forceinline__ void
q4k_get_scale_min(int j, const uint8_t * q, uint8_t & d, uint8_t & m) {
    if (j < 4) {
        d = q[j]     & 63;
        m = q[j + 4] & 63;
    } else {
        d = (q[j + 4] & 0x0F) | ((q[j - 4] >> 6) << 4);
        m = (q[j + 4] >>   4) | ((q[j    ] >> 6) << 4);
    }
}

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
static __device__ __forceinline__ void mmvq_q4k_body(
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

    // Base pointer to this column's blocks in the packed Q4_K_M buffer.
    const block_q4_K * col_blocks =
        reinterpret_cast<const block_q4_K *>(a_bytes) +
        (size_t)col * blocks_per_col;

    float acc = 0.0f;

    // Each warp iterates blocks. Within a block, each lane handles one
    // of 32 disjoint elements (two 16-lane halves on the 4-bit byte
    // layout: low nibble → element L, high nibble → element L+32).
    for (int b = 0; b < blocks_per_col; ++b) {
        const block_q4_K * blk = col_blocks + b;
        const float dall = __half2float(blk->d);
        const float dmin = __half2float(blk->dmin);

        // Sub-block index within this 256-element block. The 32 lanes
        // split into 4 groups of 8: (sub_a, sub_b) = (2*il, 2*il+1)
        // where il = lane / 8, mirroring dequantize_block_q4_K in
        // convert.cu. This keeps memory access stride-4 across lanes.
        const int il = lane >> 3;   // 0..3
        const int ir = lane & 7;    // 0..7
        const int n_elem = 4;

        // Decode the two scale/min pairs for sub-blocks 2*il, 2*il+1.
        uint8_t sc_a, m_a, sc_b, m_b;
        q4k_get_scale_min(2*il + 0, blk->scales, sc_a, m_a);
        q4k_get_scale_min(2*il + 1, blk->scales, sc_b, m_b);
        const float d1 = dall * (float)sc_a;
        const float m1 = dmin * (float)m_a;
        const float d2 = dall * (float)sc_b;
        const float m2 = dmin * (float)m_b;

        // Pointer into qs for this lane's 4-byte run.
        const uint8_t * q = blk->qs + 32*il + n_elem*ir;
        // Corresponding positions in the input vector x. Block base
        // is b*256; within block, sub_a starts at 64*il+4*ir, sub_b
        // starts at the same offset + 32.
        const int x_base = b * QK_K + 64 * il + n_elem * ir;
        const float * x_a = x + x_base;
        const float * x_b = x + x_base + 32;

        #pragma unroll
        for (int l = 0; l < n_elem; ++l) {
            const uint8_t qb = q[l];
            const float ya = d1 * (float)(qb & 0xF) - m1;
            const float yb = d2 * (float)(qb >>  4) - m2;
            acc += ya * x_a[l];
            acc += yb * x_b[l];
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

extern "C" __global__ void mmvq_q4k_f32_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    float * __restrict__ y
) {
    mmvq_q4k_body<float>(a_bytes, k, n, x, y);
}

extern "C" __global__ void mmvq_q4k_f16_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    __half * __restrict__ y
) {
    mmvq_q4k_body<__half>(a_bytes, k, n, x, y);
}
