// mmvq_q4k — Q4_K_M matrix-vector matmul with q8_1-quantized activation
// and DP4A-accelerated inner products. Decode hot path.
//
// Ported from llama.cpp / ggml-cuda:
//   deps/llama.cpp/ggml/src/ggml-cuda/mmvq.cu       (mul_mat_vec_q template,
//                                                    nwarps/rows_per_block params)
//   deps/llama.cpp/ggml/src/ggml-cuda/vecdotq.cuh   (vec_dot_q4_K_q8_1,
//                                                    vec_dot_q4_K_q8_1_impl_vmmq)
//   deps/llama.cpp/ggml/src/ggml-cuda/common.cuh    (ggml_cuda_dp4a)
//   deps/llama.cpp/ggml/src/ggml-common.h           (block_q4_K, block_q8_1 layouts)
//
// Upstream license (reproduced verbatim):
//
//   MIT License
//
//   Copyright (c) 2023-2024 The ggml authors
//
//   Permission is hereby granted, free of charge, to any person obtaining a copy
//   of this software and associated documentation files (the "Software"), to deal
//   in the Software without restriction, including without limitation the rights
//   to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//   copies of the Software, and to permit persons to whom the Software is
//   furnished to do so, subject to the following conditions:
//
//   The above copyright notice and this permission notice shall be included in all
//   copies or substantial portions of the Software.
//
//   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//   IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//   FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//   AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//   LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//   OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//   SOFTWARE.
//
// ---------------------------------------------------------------------------
//
// Algorithm summary (port of ggml-cuda's mul_mat_vec_q<Q4_K, ncols_dst=1>):
//
//   * The A matrix is laid out as [n, k] Q4_K_M, row-major over output
//     columns (ggml calls these "rows of vx"). Each row has k/QK_K blocks,
//     each block is 144 bytes.
//   * The activation row x is pre-quantized to q8_1 blocks (36 bytes each,
//     one per 32-element sub-block). K must be a multiple of QK_K=256, so
//     the q8_1 buffer has k/QK8_1 = 8×(k/QK_K) blocks.
//   * We launch `gridDim.x = n` blocks, each with `(warp_size=32, nwarps=4)
//     = 128 threads`. Each block owns one output row (rows_per_cuda_block=1
//     in the ggml generic table for Q4_K with ncols_dst=1).
//   * Each thread iterates its assigned q4_K blocks (kbx) in stride of
//     `blocks_per_iter = vdr*nwarps*warp_size/qi = 2*4*32/32 = 8`.
//   * For a given kbx, 16 threads (qi/vdr = 32/2 = 16) handle it with
//     different `kqs ∈ {0, 2, 4, ..., 30}`. Each thread's call to
//     `vec_dot_q4_K_q8_1` pulls 2 ints of q4_K quants and 4 ints of q8_1
//     quants (covering 64 elements worth of the block) and accumulates
//     one partial f32 via DP4A.
//   * Partial sums fan in through shared memory (nwarps-1 slices) and a
//     final warp-shuffle reduction.

#include <cstdint>
#include <cuda_fp16.h>

#define QK_K 256
#define QK8_1 32
#define QI4_K 32       // QK_K / (4 * QR4_K), QR4_K=2
#define QI8_1 8        // QK8_1 / (4 * QR8_1), QR8_1=1
#define QR4_K 2
#define VDR_Q4_K_Q8_1_MMVQ 2

#define WARP_SIZE 32
#define NWARPS    4    // ggml's calc_nwarps for GENERIC + ncols_dst=1
#define ROWS_PER_CUDA_BLOCK 1

#define BLOCK_Q4K_BYTES 144

// block_q4_K (llama.cpp/ggml-common.h) — byte-for-byte match required.
struct __align__(4) block_q4_K {
    __half   d;
    __half   dmin;
    uint8_t  scales[12];
    uint8_t  qs[128];
};
static_assert(sizeof(block_q4_K) == BLOCK_Q4K_BYTES,
              "block_q4_K must be 144 bytes");

// block_q8_1 (llama.cpp/ggml-common.h) — 36 bytes per 32-elem block.
struct __align__(4) block_q8_1 {
    __half  d;
    __half  s;
    int8_t  qs[QK8_1];
};
static_assert(sizeof(block_q8_1) == 36, "block_q8_1 must be 36 bytes");

// DP4A wrapper — uses the SIMD int8 dot-product instruction on sm_61+.
// This is THE hot-path intrinsic: __dp4a(a, b, c) = c + sum_{i=0..3}
// ((int8)a[i]) * ((int8)b[i]) in ONE warp instruction. Without it the
// whole point of the q8_1 port evaporates.
static __device__ __forceinline__ int ggml_cuda_dp4a(int a, int b, int c) {
#if __CUDA_ARCH__ >= 610
    return __dp4a(a, b, c);
#else
    const int8_t * a8 = reinterpret_cast<const int8_t *>(&a);
    const int8_t * b8 = reinterpret_cast<const int8_t *>(&b);
    return c + a8[0]*b8[0] + a8[1]*b8[1] + a8[2]*b8[2] + a8[3]*b8[3];
#endif
}

// Warp-lane reduction — ported from common.cuh.
template <int width>
static __device__ __forceinline__ float warp_reduce_sum(float v) {
    const unsigned FULL_MASK = 0xffffffffu;
    #pragma unroll
    for (int mask = width / 2; mask > 0; mask >>= 1) {
        v += __shfl_xor_sync(FULL_MASK, v, mask, width);
    }
    return v;
}

// Ported verbatim from vecdotq.cuh:vec_dot_q4_K_q8_1_impl_vmmq.
// Combines per-sub-block scale/min (sc, m) from the q4_K super-scales with
// the per-q8_1-block (d, s) to produce one f32 partial sum over 64 elements
// (4 pairs of (v0i, v1i) × (u[2i+0], u[2i+1])).
static __device__ __forceinline__ float
vec_dot_q4_K_q8_1_impl_vmmq(
    const int     * __restrict__ v,
    const int     * __restrict__ u,
    const uint8_t * __restrict__ sc,
    const uint8_t * __restrict__ m,
    const __half2 & dm4,
    const float   * __restrict__ d8)
{
    float sumf_d = 0.0f;
    float sumf_m = 0.0f;

    #pragma unroll
    for (int i = 0; i < QR4_K; ++i) {      // i ∈ {0, 1}
        const int v0i = (v[0] >> (4*i)) & 0x0F0F0F0F;  // low / high nibbles
        const int v1i = (v[1] >> (4*i)) & 0x0F0F0F0F;

        // dot1 = dp4a(v0i, u[2i], 0) + dp4a(v1i, u[2i+1], 0) — actual weight·x.
        const int dot1 = ggml_cuda_dp4a(v1i, u[2*i+1],
                         ggml_cuda_dp4a(v0i, u[2*i+0], 0));
        // dot2 = sum of u — lets us subtract the q4_K per-sub-block min in O(1).
        const int dot2 = ggml_cuda_dp4a(0x01010101, u[2*i+1],
                         ggml_cuda_dp4a(0x01010101, u[2*i+0], 0));

        sumf_d += d8[i] * (dot1 * sc[i]);
        sumf_m += d8[i] * (dot2 * m[i]);
    }

    const float2 dm4f = __half22float2(dm4);
    return dm4f.x * sumf_d - dm4f.y * sumf_m;
}

// Ported verbatim from vecdotq.cuh:vec_dot_q4_K_q8_1 — computes one
// partial f32 for (row, kbx, kqs). Unlike the reference we skip the
// `kbx` offset argument because we already point vbq at the right row.
static __device__ __forceinline__ float
vec_dot_q4_K_q8_1(
    const block_q4_K * __restrict__ bq4_K,
    const block_q8_1 * __restrict__ bq8_1,
    int iqs)
{
    int    v[2];
    int    u[2 * QR4_K];
    float  d8[QR4_K];

    // iqs is in 0,2..30. bq8_offset = (iqs/2)/(QI8_1/2) * QR4_K, i.e. picks
    // one of {0,2,4,6} — the pair of q8_1 blocks that line up with this
    // q4_K super-sub-block.
    const int bq8_offset = QR4_K * ((iqs/2) / (QI8_1/2));

    // Read two ints of q4_K quants, 16 bytes apart (covers 64 nibbles).
    const int * q4 = reinterpret_cast<const int *>(bq4_K->qs
                                                   + 16 * bq8_offset
                                                   + 4  * ((iqs/2) % 4));
    v[0] = q4[0];
    v[1] = q4[4];

    // Decode the 4 relevant (sc, m) 6-bit values from the 12-byte scale
    // array. This is the ggml packing: first 4 sub-blocks put sc/m in
    // bytes 0..7 low-6 bits; the last 4 split across 4..11 with 2 high
    // bits of the first group. j = bq8_offset/2 picks which pair we need.
    const uint16_t * scales = reinterpret_cast<const uint16_t *>(bq4_K->scales);
    uint16_t aux[2];
    const int j = bq8_offset / 2;
    if (j < 2) {
        aux[0] = scales[j + 0] & 0x3f3f;
        aux[1] = scales[j + 2] & 0x3f3f;
    } else {
        aux[0] = ((scales[j + 2] >> 0) & 0x0f0f)
               | ((scales[j - 2] & 0xc0c0) >> 2);
        aux[1] = ((scales[j + 2] >> 4) & 0x0f0f)
               | ((scales[j - 0] & 0xc0c0) >> 2);
    }
    const uint8_t * sc = reinterpret_cast<const uint8_t *>(aux);
    const uint8_t * m  = sc + 2;

    #pragma unroll
    for (int i = 0; i < QR4_K; ++i) {
        const block_q8_1 * bq8i = bq8_1 + bq8_offset + i;
        // __low2float on the half2 view of (d, s) — d is the low half.
        d8[i] = __half2float(bq8i->d);

        const int * q8 = reinterpret_cast<const int *>(bq8i->qs)
                       + ((iqs/2) % 4);
        u[2*i + 0] = q8[0];
        u[2*i + 1] = q8[4];
    }

    // Recompose the (d, dmin) half2 from the struct — vec_dot_impl_vmmq
    // expects one packed half2 so it can do a single __half22float2.
    const __half2 dm4 = __halves2half2(bq4_K->d, bq4_K->dmin);
    return vec_dot_q4_K_q8_1_impl_vmmq(v, u, sc, m, dm4, d8);
}

// Core mmvq body. Templated on output type (float or __half).
//
// vx              packed Q4_K_M matrix, `n` rows × (k/QK_K) blocks each
// vy              packed Q8_1 activation, (k/QK8_1) blocks
// dst             output vector of length `n`
// ncols_x = k     number of input cols (must be multiple of QK_K)
// nrows_dst = n   number of output rows
template <typename out_t>
static __device__ __forceinline__ void mmvq_q4k_body_q8_1(
    const void * __restrict__ vx,
    const void * __restrict__ vy,
    out_t      * __restrict__ dst,
    int ncols_x,
    int nrows_dst)
{
    constexpr int qk  = QK_K;
    constexpr int qi  = QI4_K;
    constexpr int vdr = VDR_Q4_K_Q8_1_MMVQ;
    constexpr int rows_per_cuda_block = ROWS_PER_CUDA_BLOCK;
    constexpr int warp_size = WARP_SIZE;
    constexpr int nwarps = NWARPS;

    const int tid  = warp_size * threadIdx.y + threadIdx.x;
    const int row0 = rows_per_cuda_block * blockIdx.x;
    if (row0 >= nrows_dst) return;

    const int blocks_per_row_x = ncols_x / qk;
    constexpr int blocks_per_iter = vdr * nwarps * warp_size / qi;  // = 8

    // Partial f32 accumulator (ncols_dst=1, rows_per_cuda_block=1).
    float tmp = 0.0f;

    const block_q8_1 * y = reinterpret_cast<const block_q8_1 *>(vy);
    // kbx_offset = row0 * stride_row_x. Since A is row-major with
    // stride_row_x = blocks_per_row_x (tightly packed), this is
    // row0 * blocks_per_row_x in block units.
    const int kbx_offset = row0 * blocks_per_row_x;

    // First kbx for this thread + per-iter stride mirror ggml's
    // mul_mat_vec_q template. tid/(qi/vdr) = tid/16 groups 16 threads
    // to each kbx inside one iteration.
    for (int kbx = tid / (qi / vdr);
             kbx < blocks_per_row_x;
             kbx += blocks_per_iter) {
        const int kby = kbx * (qk / QK8_1);        // 8 q8_1 blocks per q4_K block
        const int kqs = vdr * (tid % (qi / vdr));  // 2 * (tid % 16) ∈ {0,2,..,30}

        const block_q4_K * bq4_K_row = reinterpret_cast<const block_q4_K *>(vx)
                                     + kbx_offset;
        tmp += vec_dot_q4_K_q8_1(&bq4_K_row[kbx], &y[kby], kqs);
    }

    // Cross-warp fan-in. One f32 slot per (warp, lane).
    __shared__ float tmp_shared[nwarps - 1 > 0 ? nwarps - 1 : 1][warp_size];
    if (threadIdx.y > 0) {
        tmp_shared[threadIdx.y - 1][threadIdx.x] = tmp;
    }
    __syncthreads();
    if (threadIdx.y > 0) return;

    // Warp 0 aggregates the other warps' lane partials, then reduces
    // across the 32 lanes.
    #pragma unroll
    for (int l = 0; l < nwarps - 1; ++l) {
        tmp += tmp_shared[l][threadIdx.x];
    }
    tmp = warp_reduce_sum<warp_size>(tmp);

    // Lane 0 writes the scalar result. (rows_per_cuda_block == 1, so
    // only one lane's value is meaningful after the warp reduction.)
    if (threadIdx.x == 0 && row0 < nrows_dst) {
        if constexpr (sizeof(out_t) == sizeof(float)) {
            *reinterpret_cast<float *>(&dst[row0]) = tmp;
        } else {
            dst[row0] = __float2half(tmp);
        }
    }
}

// Direct-path entry points — caller supplies pre-quantized q8_1 x.
extern "C" __launch_bounds__(NWARPS * WARP_SIZE, 1)
__global__ void mmvq_q4k_q8_1_f32_out(
    const uint8_t * __restrict__ a_bytes,    // Q4_K_M packed [n, k/QK_K * 144 bytes]
    const uint8_t * __restrict__ x_q8_1,     // Q8_1 packed   [k/QK8_1 * 36 bytes]
    float         * __restrict__ y,          // f32 output    [n]
    int ncols_x,                             // k
    int nrows_dst                            // n
) {
    mmvq_q4k_body_q8_1<float>(a_bytes, x_q8_1, y, ncols_x, nrows_dst);
}

extern "C" __launch_bounds__(NWARPS * WARP_SIZE, 1)
__global__ void mmvq_q4k_q8_1_f16_out(
    const uint8_t * __restrict__ a_bytes,
    const uint8_t * __restrict__ x_q8_1,
    __half        * __restrict__ y,
    int ncols_x,
    int nrows_dst
) {
    mmvq_q4k_body_q8_1<__half>(a_bytes, x_q8_1, y, ncols_x, nrows_dst);
}
