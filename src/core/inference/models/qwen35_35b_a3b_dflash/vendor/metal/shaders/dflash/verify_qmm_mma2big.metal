// ref: vendor/metal/dflash-mlx-ref/dflash_mlx/verify_qmm.py:28-127
//
// Verify-specialized int4 quantized matmul — M=16 simdgroup-MMA
// variant with no K-split. One threadgroup per output-tile of
// (BM=16, BN=32); each threadgroup uses 4 simdgroups (128 threads) and
// stages a (BK=32, BN=32) chunk of B into threadgroup memory per BK-step.
//
// Build variants (select via `-D`):
//
//   INPUT_DTYPE_{BF16,F16}   picks `T` (the dequant + simdgroup matrix type)
//   DFLASH_QMM_GROUP_SIZE    group size for the 4-bit packing (32, 64, or 128)

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
  #define KERNEL_NAME_GS verify_mma2big_gs32
#elif (DFLASH_QMM_GROUP_SIZE == 64)
  #define KERNEL_NAME_GS verify_mma2big_gs64
#elif (DFLASH_QMM_GROUP_SIZE == 128)
  #define KERNEL_NAME_GS verify_mma2big_gs128
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

kernel void KERNEL_NAME(
    device const T*         x                            [[buffer(0)]],
    device const uint32_t*  w_q                          [[buffer(1)]],
    device const T*         scales                       [[buffer(2)]],
    device const T*         biases                       [[buffer(3)]],
    constant  int&          M_size                       [[buffer(4)]],
    constant  int&          K_size                       [[buffer(5)]],
    constant  int&          N_size                       [[buffer(6)]],
    device       T*         y                            [[buffer(7)]],
    uint3  thread_position_in_threadgroup [[thread_position_in_threadgroup]],
    uint3  threadgroup_position_in_grid   [[threadgroup_position_in_grid]]
) {
    constexpr int BM     = 16;
    constexpr int BN     = 32;
    constexpr int BK     = 32;
    constexpr int BK_SUB = 8;
    constexpr int GS     = DFLASH_QMM_GROUP_SIZE;

    uint tid   = thread_position_in_threadgroup.x;
    uint sg_id = tid / 32;
    uint tg_n  = threadgroup_position_in_grid.y;

    int K       = K_size;
    int N       = N_size;
    int K_by_8  = K / 8;
    int K_by_gs = K / GS;
    int n0      = int(tg_n) * BN;

    threadgroup T B_tile[BK * BN];

    simdgroup_matrix<T, 8, 8> a_top, a_bot, b_L, b_R;
    simdgroup_matrix<float, 8, 8> c_tL = simdgroup_matrix<float, 8, 8>(0.0f);
    simdgroup_matrix<float, 8, 8> c_tR = simdgroup_matrix<float, 8, 8>(0.0f);
    simdgroup_matrix<float, 8, 8> c_bL = simdgroup_matrix<float, 8, 8>(0.0f);
    simdgroup_matrix<float, 8, 8> c_bR = simdgroup_matrix<float, 8, 8>(0.0f);

    int t_a    = int(tid);
    int t_b    = int(tid) + 64;
    int dq_k_a = t_a / BN, dq_n_a = t_a % BN;
    int dq_k_b = t_b / BN, dq_n_b = t_b % BN;
    int sg_n_off = int(sg_id) * 16;

    for (int k0 = 0; k0 < K; k0 += BK) {
        {
            int n_global    = n0 + dq_n_a;
            int k_base      = k0 + dq_k_a * 8;
            uint32_t packed = w_q[n_global * K_by_8 + (k_base >> 3)];
            float s         = float(scales[n_global * K_by_gs + (k_base / GS)]);
            float b         = float(biases[n_global * K_by_gs + (k_base / GS)]);
            for (int ki = 0; ki < 8; ++ki) {
                uint32_t nib = (packed >> (ki * 4)) & 0xFu;
                B_tile[(dq_k_a * 8 + ki) * BN + dq_n_a] = T(float(nib) * s + b);
            }
        }
        {
            int n_global    = n0 + dq_n_b;
            int k_base      = k0 + dq_k_b * 8;
            uint32_t packed = w_q[n_global * K_by_8 + (k_base >> 3)];
            float s         = float(scales[n_global * K_by_gs + (k_base / GS)]);
            float b         = float(biases[n_global * K_by_gs + (k_base / GS)]);
            for (int ki = 0; ki < 8; ++ki) {
                uint32_t nib = (packed >> (ki * 4)) & 0xFu;
                B_tile[(dq_k_b * 8 + ki) * BN + dq_n_b] = T(float(nib) * s + b);
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (int ks = 0; ks < BK / BK_SUB; ++ks) {
            simdgroup_load(a_top, x + k0 + ks * BK_SUB,                  K);
            simdgroup_load(a_bot, x + 8 * K + k0 + ks * BK_SUB,          K);
            simdgroup_load(b_L, B_tile + ks * BK_SUB * BN + sg_n_off,         BN);
            simdgroup_load(b_R, B_tile + ks * BK_SUB * BN + sg_n_off + 8,     BN);
            simdgroup_multiply_accumulate(c_tL, a_top, b_L, c_tL);
            simdgroup_multiply_accumulate(c_tR, a_top, b_R, c_tR);
            simdgroup_multiply_accumulate(c_bL, a_bot, b_L, c_bL);
            simdgroup_multiply_accumulate(c_bR, a_bot, b_R, c_bR);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    simdgroup_matrix<T, 8, 8> c_tL_T, c_tR_T, c_bL_T, c_bR_T;
    c_tL_T.thread_elements()[0] = T(c_tL.thread_elements()[0]);
    c_tL_T.thread_elements()[1] = T(c_tL.thread_elements()[1]);
    c_tR_T.thread_elements()[0] = T(c_tR.thread_elements()[0]);
    c_tR_T.thread_elements()[1] = T(c_tR.thread_elements()[1]);
    c_bL_T.thread_elements()[0] = T(c_bL.thread_elements()[0]);
    c_bL_T.thread_elements()[1] = T(c_bL.thread_elements()[1]);
    c_bR_T.thread_elements()[0] = T(c_bR.thread_elements()[0]);
    c_bR_T.thread_elements()[1] = T(c_bR.thread_elements()[1]);
    simdgroup_store(c_tL_T, y + n0 + sg_n_off,                  N);
    simdgroup_store(c_tR_T, y + n0 + sg_n_off + 8,              N);
    simdgroup_store(c_bL_T, y + 8 * N + n0 + sg_n_off,          N);
    simdgroup_store(c_bR_T, y + 8 * N + n0 + sg_n_off + 8,      N);
}
