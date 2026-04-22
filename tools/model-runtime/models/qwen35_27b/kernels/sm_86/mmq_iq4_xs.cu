// mmvq_iq4_xs — IQ4_XS matrix-vector matmul (single-query decode path).
//
// Ported from llama.cpp / ggml-cuda:
//   deps/llama.cpp/ggml/src/ggml-cuda/mmvq.cu       (mul_mat_vec_q template)
//   deps/llama.cpp/ggml/src/ggml-cuda/vecdotq.cuh   (vec_dot_iq4_xs_q8_1)
//   deps/llama.cpp/ggml/src/ggml-common.h           (block_iq4_xs, kvalues_iq4nl)
// Upstream license: MIT (compatible with this repo).
//
// Final Q* variant: Qwen3.5-27B ships the FFN gate/up projections as
// IQ4_XS. The other quant formats (Q4_K_M / Q5_K / Q6_K / Q8_0) already
// have mmvq paths vendored; this file closes the last gap.
//
// Shape conventions (match all other mmq_*.cu shims):
//   A is a [n, k] matrix in IQ4_XS with (k/256) 136-byte blocks per
//   column. Block layout (136 B):
//     half    d;           // 2
//     uint16_t scales_h;   // 2  — 2 high scale bits per 32-elem subblock
//     uint8_t scales_l[4]; // 4  — 4 low scale bits per 32-elem subblock
//     uint8_t qs[128];     // 128 — packed 4-bit codebook indices
//   → 8 sub-blocks × 32 elements = 256 elements per block.
//
//   Per element:
//     ib         = elem_idx / 32                         (0..8)
//     within     = elem_idx % 32
//     ls_low     = (scales_l[ib/2] >> (4*(ib%2))) & 0x0F
//     ls_high    = (scales_h       >> (2*ib))     & 0x03
//     ls         = ls_low | (ls_high << 4)               // 6-bit, 0..63
//     dl         = d * (float)(ls - 32)
//     qhalf      = within < 16 ? lo-nibble : hi-nibble   of qs[ib*16 + within%16]
//     value      = dl * kvalues_iq4nl[qhalf]             // codebook i8
//
// Launch convention (mirrors mmq_q8_0 / mmq_q5k / mmq_q6k):
//   grid  = (ceil(n/2), 1, 1)    NCOLS_Y=2 output columns per block
//   block = (32, NCOLS_Y, 1)     one warp per output column
//   shmem = 0 — partial reductions in registers + warp-shuffle
//
// Two entry points:
//   mmvq_iq4_xs_f32_out     — takes f32 x[k],   writes f32 y[n]
//   mmvq_iq4_xs_q8_1_f32_out — takes q8_1 packed x, writes f32 y[n]
//
// The q8_1 path unpacks the activation inline (per-block d scale + i8
// quants) rather than carrying a separate DP4A path. Same as how we
// handle the correctness-first ports of Q5_K/Q6_K/Q8_0: f32 inner
// product, single `__half2float` per block, no DP4A. The DP4A/table-
// lookup hot path is a follow-up; this kernel locks in correctness.

#include <cstdint>
#include <cuda_fp16.h>

#define QK_K 256
#define QK8_1 32
#define Q8_1_BLOCK_BYTES 36
#define BLOCK_IQ4_XS_BYTES 136
#define IQ4_XS_SUBBLOCKS 8   // QK_K / 32
#define IQ4_XS_SUB_ELEMS 32

// Matches ggml-common.h's block_iq4_xs layout exactly (136 B total).
struct __align__(2) block_iq4_xs {
    __half   d;
    uint16_t scales_h;
    uint8_t  scales_l[4];   // QK_K / 64 = 4
    uint8_t  qs[128];       // QK_K / 2  = 128
};
static_assert(sizeof(block_iq4_xs) == BLOCK_IQ4_XS_BYTES,
              "block_iq4_xs must be 136 bytes");

// Matches ggml-common.h's block_q8_1 (36 B: half d, half s, int8 qs[32]).
struct __align__(4) block_q8_1 {
    __half  d;
    __half  s;
    int8_t  qs[QK8_1];
};
static_assert(sizeof(block_q8_1) == Q8_1_BLOCK_BYTES,
              "block_q8_1 must be 36 bytes");

// IQ4_NL 16-entry codebook — copied verbatim from ggml-common.h's
// `kvalues_iq4nl`. IQ4_XS uses this same table for its 4-bit indices.
__device__ __constant__ static const int8_t kvalues_iq4nl_dev[16] = {
    -127, -104, -83, -65, -49, -35, -22, -10,
       1,   13,  25,  38,  53,  69,  89, 113
};

// Warp-level reduction via shfl_xor. Sums across the 32 lanes.
static __device__ __forceinline__ float warp_reduce_sum_f32(float v) {
    const unsigned FULL_MASK = 0xffffffffu;
    #pragma unroll
    for (int mask = 16; mask > 0; mask >>= 1) {
        v += __shfl_xor_sync(FULL_MASK, v, mask, 32);
    }
    return v;
}

// Decode the 6-bit subblock scale for sub-block `ib` in [0, 8).
// Mirrors `dequant_iq4_xs_to_bf16` in src/gguf_loader.rs and the CPU
// reference `dequantize_row_iq4_xs`.
static __device__ __forceinline__ int32_t
iq4_xs_subblock_scale(const block_iq4_xs * __restrict__ blk, int ib) {
    const uint32_t ls_low  = (blk->scales_l[ib >> 1] >> (4 * (ib & 1))) & 0x0F;
    const uint32_t ls_high = (blk->scales_h          >> (2 * ib))       & 0x03;
    const int32_t  ls      = (int32_t)(ls_low | (ls_high << 4));
    return ls - 32;   // re-center: 6-bit -> signed
}

// Core kernel body, templated on output type (float or __half) AND on
// the activation source. Activation is delivered via an accessor
// functor `get_x(int idx) -> float` so the same body services both the
// "raw f32 x" and "packed q8_1 x" entry points without duplication.
template <typename out_t, typename XFn>
static __device__ __forceinline__ void mmvq_iq4_xs_body(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    out_t * __restrict__ y,
    XFn get_x
) {
    // threadIdx.y picks which of the 2 columns this warp owns.
    const int col = blockIdx.x * 2 + threadIdx.y;
    if (col >= n) return;

    const int lane = threadIdx.x;            // 0..31
    const int blocks_per_col = k / QK_K;     // whole blocks per column

    const block_iq4_xs * col_blocks =
        reinterpret_cast<const block_iq4_xs *>(a_bytes) +
        (size_t)col * blocks_per_col;

    float acc = 0.0f;

    // Within a block: 8 sub-blocks × 32 elems. Split the 32 lanes into
    // 2 halves × 16 lanes: lane `l` with l<16 owns the "low nibble"
    // half of each sub-block, l>=16 owns the "high nibble" half. Each
    // lane processes 4 sub-blocks (sub indices = il*2 + {0,1,2,3} for
    // il=0; but we simply iterate ib in {lane_group, lane_group+4}).
    //
    // Concretely the mapping is symmetric with dequant_iq4_xs_to_bf16:
    //   lane l -> (nibble = l>=16 ? hi : lo, within = l % 16)
    // and we iterate all 8 sub-blocks serially, since a warp ≠ sub-block
    // (each subblock has 32 elems but the codebook lookup is per-nibble
    // so 16 lanes do it in lockstep; serially over ib keeps indexing
    // trivial and matches the Q5_K/Q6_K style).
    const int nibble = lane >> 4;          // 0 = lo nibble, 1 = hi nibble
    const int within = lane & 15;          // 0..15
    const int sub_off = nibble * 16;       // position inside subblock (0 or 16)

    for (int b = 0; b < blocks_per_col; ++b) {
        const block_iq4_xs * blk = col_blocks + b;
        const float d = __half2float(blk->d);

        #pragma unroll
        for (int ib = 0; ib < IQ4_XS_SUBBLOCKS; ++ib) {
            const float dl = d * (float)iq4_xs_subblock_scale(blk, ib);

            const uint8_t qb = blk->qs[ib * 16 + within];
            const int idx4 = nibble == 0 ? (qb & 0x0F) : (qb >> 4);
            const int code = (int)kvalues_iq4nl_dev[idx4];

            // Position in the (dequantized) weight row:
            //   block base = b * QK_K
            //   sub base   = ib * IQ4_XS_SUB_ELEMS
            //   within sub = sub_off + within  (0..15 low, 16..31 high)
            const int elem_idx = b * QK_K
                               + ib * IQ4_XS_SUB_ELEMS
                               + sub_off + within;

            acc += dl * (float)code * get_x(elem_idx);
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

// ---- Entry point 1: f32 x -------------------------------------------------

struct XFnF32 {
    const float * __restrict__ x;
    __device__ __forceinline__ float operator()(int i) const { return x[i]; }
};

extern "C" __global__ void mmvq_iq4_xs_f32_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    float * __restrict__ y
) {
    mmvq_iq4_xs_body<float>(a_bytes, k, n, y, XFnF32{x});
}

extern "C" __global__ void mmvq_iq4_xs_f16_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const float * __restrict__ x,
    __half * __restrict__ y
) {
    mmvq_iq4_xs_body<__half>(a_bytes, k, n, y, XFnF32{x});
}

// ---- Entry point 2: pre-quantized q8_1 x ----------------------------------
//
// Accessor that reads from the packed q8_1 buffer. Each 32-element
// q8_1 block stores (half d, half s, int8 qs[32]); we return
// `d * qs[within]` — a per-element f32 dequant. The caller must have
// produced `x_q8_1` via launch_quantize_q8_1_f32 over the same k.

struct XFnQ8_1 {
    const uint8_t * __restrict__ x_bytes;
    __device__ __forceinline__ float operator()(int i) const {
        const int blk_idx = i >> 5;                    // i / 32
        const int within  = i & 31;                    // i % 32
        const block_q8_1 * blk = reinterpret_cast<const block_q8_1 *>(x_bytes) + blk_idx;
        const float d = __half2float(blk->d);
        const float q = (float)(int)blk->qs[within];
        return d * q;
    }
};

extern "C" __global__ void mmvq_iq4_xs_q8_1_f32_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const uint8_t * __restrict__ x_q8_1,
    float * __restrict__ y
) {
    mmvq_iq4_xs_body<float>(a_bytes, k, n, y, XFnQ8_1{x_q8_1});
}

extern "C" __global__ void mmvq_iq4_xs_q8_1_f16_out(
    const uint8_t * __restrict__ a_bytes,
    int k,
    int n,
    const uint8_t * __restrict__ x_q8_1,
    __half * __restrict__ y
) {
    mmvq_iq4_xs_body<__half>(a_bytes, k, n, y, XFnQ8_1{x_q8_1});
}
