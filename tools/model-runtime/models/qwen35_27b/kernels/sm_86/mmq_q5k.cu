// mmvq_q5k — Q5_K matrix-vector matmul (single-query decode path).
//
// Ported from llama.cpp / ggml-cuda:
//   deps/llama.cpp/ggml/src/ggml-cuda/mmvq.cu      (mul_mat_vec_q template)
//   deps/llama.cpp/ggml/src/ggml-cuda/vecdotq.cuh  (vec_dot_q5_K_q8_1)
//   deps/llama.cpp/ggml/src/ggml-cuda/convert.cu   (dequantize_block_q5_K)
//   deps/llama.cpp/ggml/src/ggml-quants.c          (dequantize_row_q5_K)
// Upstream license: MIT (compatible with this repo).
//
// First-pass port (matches mmq_q4k): the reference quantizes the
// activation row `x` to q8_1 on input and uses DP4A. We dequantize Q5
// to f32 and compute a plain f32 inner product instead — correctness
// first, performance second. The DP4A/q8_1 path is a follow-up.
//
// TODO: port the batched mmq_q5k (mat-mat) path. This file only
//       provides the mmvq (batch = 1) variant used by decode.
// TODO: fuse with a q8_1 activation quantization for ~2-3× speedup.
//
// Math:
//   A is a [n, k] matrix in Q5_K with (k/256) 176-byte blocks per
//   column (k must be a multiple of 256). Q5_K block:
//     half   d;              // super-block scale
//     half   dmin;           // super-block min
//     uint8_t scales[12];    // 6-bit packed per-sub-block (sc, m)
//     uint8_t qh[32];        // high bit per element, 1 bit per elem
//     uint8_t qs[128];       // 4-bit packed low nibbles, 2 per byte
//   → 8 sub-blocks × 32 elements = 256 elements per block.
//
//   Per element (following dequantize_row_q5_K):
//     sub_id  = elem_idx / 32                    (0..8)
//     within  = elem_idx % 32
//     (sc, m) = get_scale_min_k4(sub_id, scales)
//     low4    = (qs[within_byte] >> nibble_shift) & 0xF
//     high1   = (qh[within]   >> sub_id) & 0x1
//     q       = low4 | (high1 << 4)              // 5-bit unsigned
//     value   = d * sc * q - dmin * m
//
//   Specifically the reference groups 64 elements at a time (two
//   adjacent sub-blocks 2j, 2j+1): the low nibble of qs feeds sub-block
//   2j with high-bit mask u1 = (1 << (2j)), the high nibble feeds
//   sub-block 2j+1 with high-bit mask u2 = (2 << (2j)). This file
//   mirrors that grouping directly, reusing the 32-lane (il, ir, l)
//   decomposition from mmq_q4k so a single warp processes one block per
//   pass in lockstep.
//
// x is the f32 input vector of length k. y is the f32 (or f16) output
// vector of length n. We compute: y[col] = sum_{i in 0..k} A[col,i] * x[i].
// Row-major over columns: block b of column `col` starts at
// (col * blocks_per_col + b) * 176 bytes in the packed buffer.
//
// Launch convention (mirrors ggml-cuda's mmvq and our mmq_q4k):
//   grid  = (n / NCOLS_Y, 1, 1)  with NCOLS_Y=2 output columns per block
//   block = (32, NCOLS_Y, 1)     one warp per output column
//   shmem = 0 — partial reductions in registers + warp-shuffle

#include <cstdint>
#include <cuda_fp16.h>

#define QK_K 256
#define BLOCK_Q5K_BYTES 176
#define Q5K_SUBBLOCKS 8       // QK_K / 32
#define Q5K_SUB_ELEMS 32

// Matches llama.cpp's block_q5_K layout exactly.
struct __align__(4) block_q5_K {
    __half  d;
    __half  dmin;
    uint8_t scales[12];
    uint8_t qh[32];
    uint8_t qs[128];
};
static_assert(sizeof(block_q5_K) == BLOCK_Q5K_BYTES,
              "block_q5_K must be 176 bytes");

// Port of `get_scale_min_k4` from ggml-cuda/convert.cu. Given a
// sub-block index j in [0, 8), decode (scale, min) from the packed
// 12-byte `scales` array. Identical to the helper in mmq_q4k.cu;
// duplicated here so each kernel TU is self-contained.
static __device__ __forceinline__ void
q5k_get_scale_min(int j, const uint8_t * q, uint8_t & d, uint8_t & m) {
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
static __device__ __forceinline__ void mmvq_q5k_body(
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

    // Base pointer to this column's blocks in the packed Q5_K buffer.
    const block_q5_K * col_blocks =
        reinterpret_cast<const block_q5_K *>(a_bytes) +
        (size_t)col * blocks_per_col;

    float acc = 0.0f;

    // Each warp iterates blocks. Within a block, the 32 lanes split
    // into 4 groups of 8: (sub_a, sub_b) = (2*il, 2*il+1) where
    // il = lane / 8. Each lane handles n_elem=4 consecutive positions
    // within the sub-block. This mirrors dequantize_block_q5_K in
    // convert.cu and keeps memory access stride-4 across lanes.
    for (int b = 0; b < blocks_per_col; ++b) {
        const block_q5_K * blk = col_blocks + b;
        const float dall = __half2float(blk->d);
        const float dmin = __half2float(blk->dmin);

        const int il = lane >> 3;   // 0..3
        const int ir = lane & 7;    // 0..7
        const int n_elem = 4;

        // Decode the two scale/min pairs for sub-blocks 2*il, 2*il+1.
        uint8_t sc_a, m_a, sc_b, m_b;
        q5k_get_scale_min(2*il + 0, blk->scales, sc_a, m_a);
        q5k_get_scale_min(2*il + 1, blk->scales, sc_b, m_b);
        const float d1 = dall * (float)sc_a;
        const float m1 = dmin * (float)m_a;
        const float d2 = dall * (float)sc_b;
        const float m2 = dmin * (float)m_b;

        // Pointer into qs for this lane's 4-byte run (low nibble = sub_a,
        // high nibble = sub_b). Same layout as Q4_K.
        const uint8_t * q = blk->qs + 32*il + n_elem*ir;
        // qh is indexed by within-block position (0..31), shared across
        // both sub-blocks: bit (2*il)   → high bit of sub_a element,
        //                  bit (2*il+1) → high bit of sub_b element.
        const uint8_t * qh = blk->qh + n_elem*ir;
        const uint8_t u1_mask = (uint8_t)(1u << (2*il));
        const uint8_t u2_mask = (uint8_t)(2u << (2*il));

        // Corresponding positions in the input vector x. Block base is
        // b*256; within block, sub_a starts at 64*il+4*ir, sub_b starts
        // at the same offset + 32. Identical to the Q4_K layout because
        // Q5_K reuses the same 12-byte scale packing and 4-bit low
        // nibble layout — only the extra high bit is new.
        const int x_base = b * QK_K + 64 * il + n_elem * ir;
        const float * x_a = x + x_base;
        const float * x_b = x + x_base + 32;

        #pragma unroll
        for (int l = 0; l < n_elem; ++l) {
            const uint8_t qb = q[l];
            const uint8_t hb = qh[l];
            // 5-bit quants: low nibble + high bit (from qh) shifted into
            // bit 4. Values are unsigned in [0, 31].
            const int qa = (int)(qb & 0xF) + (((hb & u1_mask) != 0) ? 16 : 0);
            const int qc = (int)(qb >>  4) + (((hb & u2_mask) != 0) ? 16 : 0);
            const float ya = d1 * (float)qa - m1;
            const float yb = d2 * (float)qc - m2;
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

extern "C" __global__ void mmvq_q5k_f32_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    float * __restrict__ y
) {
    mmvq_q5k_body<float>(a_bytes, k, n, x, y);
}

extern "C" __global__ void mmvq_q5k_f16_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    __half * __restrict__ y
) {
    mmvq_q5k_body<__half>(a_bytes, k, n, x, y);
}
