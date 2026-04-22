// quantize_q8_1 — f32 activation → packed q8_1 blocks.
//
// Ported from llama.cpp / ggml-cuda:
//   deps/llama.cpp/ggml/src/ggml-cuda/quantize.cu    (quantize_q8_1 kernel,
//                                                     quantize_row_q8_1_cuda host launcher)
//   deps/llama.cpp/ggml/src/ggml-cuda/quantize.cuh   (CUDA_QUANTIZE_BLOCK_SIZE)
//   deps/llama.cpp/ggml/src/ggml-common.h            (block_q8_1 layout)
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
// Math:
//   Per 32-element block of x (QK8_1 = 32):
//     d = max(|x|) / 127
//     q[i] = round(x[i] / d)                  (int8, clamped implicitly)
//     s = sum(x[i])                           (stored as f16; vec_dot uses it
//                                              to subtract q4_K mins in bulk)
//
// Output packing (block_q8_1, 36 bytes):
//   +0:  half  d      // per-block scale
//   +2:  half  s      // per-block row-sum (pre-scale)
//   +4:  int8[32] qs  // quantized values
//
// Layout: the output buffer is a contiguous array of
// ceil(K / QK8_1) packed blocks. For K=4096 → 128 blocks × 36B = 4608 bytes.
//
// Launch convention:
//   grid  = (ceil(K / CUDA_QUANTIZE_BLOCK_SIZE), 1, 1)
//   block = (CUDA_QUANTIZE_BLOCK_SIZE, 1, 1)  — 256 threads per CTA
//   shmem = 0 — warp-level reductions across 32-lane sub-warps.
//
// One CUDA thread per f32 input. Threads 0..31 cover block 0, 32..63 block 1,
// etc. Warp-level shfl_xor reductions compute per-block (amax, sum) and the
// lane-0 thread of each 32-lane sub-warp writes the half2 (d, s) header.

#include <cstdint>
#include <cuda_fp16.h>

#define QK8_1 32
#define CUDA_QUANTIZE_BLOCK_SIZE 256

// Matches llama.cpp's block_q8_1 layout exactly (2 halves + 32 int8 = 36B).
// We pack d and s as two separate halves — vec_dot_q4_K_q8_1 reads the d/s
// pair via `__low2float` / `__high2float` on the half2 view, so the byte
// layout must match bit-for-bit.
struct __align__(4) block_q8_1 {
    __half  d;
    __half  s;
    int8_t  qs[QK8_1];
};
static_assert(sizeof(block_q8_1) == 36, "block_q8_1 must be 36 bytes");

// Warp-wide reductions over `width` lanes (32 for QK8_1 = 32).
// __shfl_xor_sync with FULL_MASK — every thread in the sub-warp MUST
// participate or we get undefined values. The ggml reference uses a
// templated warp_reduce_* with width=QK8_1.
template <int width>
static __device__ __forceinline__ float warp_reduce_sum(float v) {
    const unsigned FULL_MASK = 0xffffffffu;
    #pragma unroll
    for (int mask = width / 2; mask > 0; mask >>= 1) {
        v += __shfl_xor_sync(FULL_MASK, v, mask, width);
    }
    return v;
}

template <int width>
static __device__ __forceinline__ float warp_reduce_max(float v) {
    const unsigned FULL_MASK = 0xffffffffu;
    #pragma unroll
    for (int mask = width / 2; mask > 0; mask >>= 1) {
        v = fmaxf(v, __shfl_xor_sync(FULL_MASK, v, mask, width));
    }
    return v;
}

// Entry point — 1:1 port of ggml-cuda's quantize_q8_1 kernel for the
// simple 1-D case (ne1 = ne2 = ne3 = 1, no strides, no ids). The full
// reference handles multi-channel/multi-sample inputs; we only need the
// activation-row case for MMVQ decode.
//
// Arguments:
//   x      [K] f32 input (device pointer)
//   y      packed q8_1 output buffer. Byte count = ceil(K/32) * 36.
//   K      number of input elements. Padded to a multiple of QK8_1
//          at the caller — threads past `ne00` quantize as zero, so
//          partial tail blocks store zeros with d=0, s=0.
//   ne00   logical element count. When K > ne00 the tail is padded with
//          zeros (matches the reference's behavior for row_padding).
extern "C" __launch_bounds__(CUDA_QUANTIZE_BLOCK_SIZE, 1)
__global__ void quantize_q8_1_f32(
    const float * __restrict__ x,
    void * __restrict__ vy,
    int K,
    int ne00
) {
    const int i0 = blockDim.x * blockIdx.x + threadIdx.x;
    if (i0 >= K) {
        return;
    }

    block_q8_1 * y = reinterpret_cast<block_q8_1 *>(vy);

    const int ib  = i0 / QK8_1;   // block index
    const int iqs = i0 % QK8_1;   // within-block position

    // Out-of-bounds lanes contribute 0 to the reductions — matches
    // ggml's row_padding behavior when K > ne00.
    const float xi = (i0 < ne00) ? x[i0] : 0.0f;

    float amax = fabsf(xi);
    float sum  = xi;

    amax = warp_reduce_max<QK8_1>(amax);
    sum  = warp_reduce_sum<QK8_1>(sum);

    const float  d = amax / 127.0f;
    const int8_t q = (amax == 0.0f) ? (int8_t)0 : (int8_t)roundf(xi / d);

    y[ib].qs[iqs] = q;

    // Only lane 0 of each 32-lane sub-warp writes the scale/sum header.
    if (iqs != 0) {
        return;
    }

    y[ib].d = __float2half(d);
    y[ib].s = __float2half(sum);
}
