// ref: vendor/metal/dflash-mlx-ref/dflash_mlx/verify_qmm.py:130-243
//
// Verify-specialized int4 quantized matmul — M=16 simdgroup-MMA
// variant with K-split + double-buffered staging of the dequantized
// B-tile. Per threadgroup we decode two (BK × BN) B-tiles in ping-pong
// fashion so the matmul loop overlaps dequant work with the MMA.
//
// Output is `partials[KP, BM, N]` — the K-dimension is split across
// `KP` thread-groups on grid.z; the Rust driver does a reduce pass
// over `tg_k_part` to sum the partial outputs into the final `y`.
//
// Build variants:
//
//   INPUT_DTYPE_{BF16,F16}
//   DFLASH_QMM_GROUP_SIZE ∈ {32, 64, 128}

#include <metal_stdlib>
#include <metal_simdgroup>
#include <metal_simdgroup_matrix>

using namespace metal;

#if !defined(INPUT_DTYPE_BF16) && !defined(INPUT_DTYPE_F16)
  #define INPUT_DTYPE_BF16
#endif
#if defined(INPUT_DTYPE_BF16)
  using T = bfloat;
#elif defined(INPUT_DTYPE_F16)
  using T = half;
#endif

#ifndef DFLASH_QMM_GROUP_SIZE
  #define DFLASH_QMM_GROUP_SIZE 64
#endif

#if (DFLASH_QMM_GROUP_SIZE == 32)
  #define KERNEL_NAME_GS verify_mma2big_pipe_gs32
#elif (DFLASH_QMM_GROUP_SIZE == 64)
  #define KERNEL_NAME_GS verify_mma2big_pipe_gs64
#elif (DFLASH_QMM_GROUP_SIZE == 128)
  #define KERNEL_NAME_GS verify_mma2big_pipe_gs128
#else
  #error "Unsupported DFLASH_QMM_GROUP_SIZE"
#endif

#if defined(INPUT_DTYPE_BF16)
  #define _CONCAT2(a, b) a##b
  #define CONCAT2(a, b)  _CONCAT2(a, b)
  #define KERNEL_NAME    CONCAT2(KERNEL_NAME_GS, _bf16)
#elif defined(INPUT_DTYPE_F16)
  #define _CONCAT2(a, b) a##b
  #define CONCAT2(a, b)  _CONCAT2(a, b)
  #define KERNEL_NAME    CONCAT2(KERNEL_NAME_GS, _fp16)
#endif

// Stage one (BK × BN) tile of dequantized B into `slot` of `B_tile`.
// Keep as an inline helper rather than a macro to stay readable — the
// two callers are in the prologue and inside the k0 loop.
template <int BK, int BN, int GS, int BM_UNUSED>
METAL_FUNC void stage_b_tile(
    threadgroup T*          B_tile_slot,
    device const uint32_t*  w_q,
    device const T*         scales,
    device const T*         biases,
    int                     n0,
    int                     K_by_8,
    int                     K_by_gs,
    int                     k0_stage,
    int                     dq_k_a,
    int                     dq_n_a,
    int                     dq_k_b,
    int                     dq_n_b
) {
    {
        int n_global    = n0 + dq_n_a;
        int k_base      = k0_stage + dq_k_a * 8;
        uint32_t packed = w_q[n_global * K_by_8 + (k_base >> 3)];
        float s         = float(scales[n_global * K_by_gs + (k_base / GS)]);
        float b         = float(biases[n_global * K_by_gs + (k_base / GS)]);
        for (int ki = 0; ki < 8; ++ki) {
            uint32_t nib = (packed >> (ki * 4)) & 0xFu;
            B_tile_slot[(dq_k_a * 8 + ki) * BN + dq_n_a] = T(float(nib) * s + b);
        }
    }
    {
        int n_global    = n0 + dq_n_b;
        int k_base      = k0_stage + dq_k_b * 8;
        uint32_t packed = w_q[n_global * K_by_8 + (k_base >> 3)];
        float s         = float(scales[n_global * K_by_gs + (k_base / GS)]);
        float b         = float(biases[n_global * K_by_gs + (k_base / GS)]);
        for (int ki = 0; ki < 8; ++ki) {
            uint32_t nib = (packed >> (ki * 4)) & 0xFu;
            B_tile_slot[(dq_k_b * 8 + ki) * BN + dq_n_b] = T(float(nib) * s + b);
        }
    }
}

kernel void KERNEL_NAME(
    device const T*         x                            [[buffer(0)]],
    device const uint32_t*  w_q                          [[buffer(1)]],
    device const T*         scales                       [[buffer(2)]],
    device const T*         biases                       [[buffer(3)]],
    constant  int&          M_size                       [[buffer(4)]],
    constant  int&          K_size                       [[buffer(5)]],
    constant  int&          N_size                       [[buffer(6)]],
    constant  int&          K_parts                      [[buffer(7)]],
    device       float*     partials                     [[buffer(8)]],
    uint3  thread_position_in_threadgroup [[thread_position_in_threadgroup]],
    uint3  threadgroup_position_in_grid   [[threadgroup_position_in_grid]]
) {
    constexpr int BM     = 16;
    constexpr int BN     = 32;
    constexpr int BK     = 32;
    constexpr int BK_SUB = 8;
    constexpr int GS     = DFLASH_QMM_GROUP_SIZE;

    uint tid       = thread_position_in_threadgroup.x;
    uint sg_id     = tid / 32;
    uint tg_n      = threadgroup_position_in_grid.y;
    uint tg_k_part = threadgroup_position_in_grid.z;

    int K       = K_size;
    int N       = N_size;
    int KP      = K_parts;
    int K_by_8  = K / 8;
    int K_by_gs = K / GS;
    int n0      = int(tg_n) * BN;
    int k_slice = K / KP;
    int k_begin = k_slice * int(tg_k_part);
    int k_end   = k_begin + k_slice;

    threadgroup T B_tile[2][BK * BN];

    simdgroup_matrix<T, 8, 8> a_top, a_bot, b_L, b_R;
    simdgroup_matrix<float, 8, 8> c_tL = simdgroup_matrix<float, 8, 8>(0.0f);
    simdgroup_matrix<float, 8, 8> c_tR = simdgroup_matrix<float, 8, 8>(0.0f);
    simdgroup_matrix<float, 8, 8> c_bL = simdgroup_matrix<float, 8, 8>(0.0f);
    simdgroup_matrix<float, 8, 8> c_bR = simdgroup_matrix<float, 8, 8>(0.0f);

    int t_a      = int(tid);
    int t_b      = int(tid) + 64;
    int dq_k_a   = t_a / BN, dq_n_a = t_a % BN;
    int dq_k_b   = t_b / BN, dq_n_b = t_b % BN;
    int sg_n_off = int(sg_id) * 16;

    stage_b_tile<BK, BN, GS, BM>(
        B_tile[0], w_q, scales, biases, n0, K_by_8, K_by_gs, k_begin,
        dq_k_a, dq_n_a, dq_k_b, dq_n_b);
    threadgroup_barrier(mem_flags::mem_threadgroup);

    int read_slot = 0;
    for (int k0 = k_begin; k0 < k_end; k0 += BK) {
        int write_slot = 1 - read_slot;
        int k0_next    = k0 + BK;

        if (k0_next < k_end) {
            stage_b_tile<BK, BN, GS, BM>(
                B_tile[write_slot], w_q, scales, biases, n0, K_by_8, K_by_gs, k0_next,
                dq_k_a, dq_n_a, dq_k_b, dq_n_b);
        }

        for (int ks = 0; ks < BK / BK_SUB; ++ks) {
            simdgroup_load(a_top, x + k0 + ks * BK_SUB,                           K);
            simdgroup_load(a_bot, x + 8 * K + k0 + ks * BK_SUB,                   K);
            simdgroup_load(b_L, B_tile[read_slot] + ks * BK_SUB * BN + sg_n_off,     BN);
            simdgroup_load(b_R, B_tile[read_slot] + ks * BK_SUB * BN + sg_n_off + 8, BN);
            simdgroup_multiply_accumulate(c_tL, a_top, b_L, c_tL);
            simdgroup_multiply_accumulate(c_tR, a_top, b_R, c_tR);
            simdgroup_multiply_accumulate(c_bL, a_bot, b_L, c_bL);
            simdgroup_multiply_accumulate(c_bR, a_bot, b_R, c_bR);
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
        read_slot = write_slot;
    }

    int part_off = int(tg_k_part) * BM * N;
    simdgroup_store(c_tL, partials + part_off + n0 + sg_n_off,                  N);
    simdgroup_store(c_tR, partials + part_off + n0 + sg_n_off + 8,              N);
    simdgroup_store(c_bL, partials + part_off + 8 * N + n0 + sg_n_off,          N);
    simdgroup_store(c_bR, partials + part_off + 8 * N + n0 + sg_n_off + 8,      N);
}
