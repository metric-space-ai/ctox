// Vendored from llama.cpp ggml-cuda/getrows.cu.
// The ONLY modification is in the vendor file itself: `static` is
// stripped from `k_get_rows_float` so the template specializations we
// instantiate below get externally-linkable PTX symbols (cudarc's
// driver loader can only resolve non-internal symbols). All other
// logic is upstream.
//
// Exposed specializations (driven by the Rust wrapper in
// src/kernels/embedding.rs, which maps our [vocab, hidden] 2D gather
// onto upstream's ne* / nb* parameterization with ne11=ne12=1):
//
//   k_get_rows_float<__nv_bfloat16, __nv_bfloat16>  — bf16 weight → bf16 out
//   k_get_rows_float<__half,        __nv_bfloat16>  — f16  weight → bf16 out
//   k_get_rows_float<float,         __nv_bfloat16>  — f32  weight → bf16 out
//
// Explicit instantiations below force the compiler to emit the PTX
// symbols (with C++-mangled names that the Rust side looks up).
#include "../../vendor/ggml-cuda/common.cuh"
#include "../../vendor/ggml-cuda/getrows.cuh"
#include "../../vendor/ggml-cuda/convert.cuh"
#include "../../vendor/ggml-cuda/getrows.cu"

// Force PTX emission of the three specializations our wrapper loads.
// Template `__global__` functions aren't instantiated unless referenced;
// these addresses-of ensure each mangled symbol appears in the PTX.
template __global__ void k_get_rows_float<__nv_bfloat16, __nv_bfloat16>(
    const __nv_bfloat16 * __restrict__, const int32_t * __restrict__, __nv_bfloat16 * __restrict__,
    const int64_t, const int64_t, const int64_t,
    const size_t, const size_t, const size_t,
    const size_t, const size_t, const size_t,
    const size_t, const size_t, const size_t);

template __global__ void k_get_rows_float<__half, __nv_bfloat16>(
    const __half * __restrict__, const int32_t * __restrict__, __nv_bfloat16 * __restrict__,
    const int64_t, const int64_t, const int64_t,
    const size_t, const size_t, const size_t,
    const size_t, const size_t, const size_t,
    const size_t, const size_t, const size_t);

template __global__ void k_get_rows_float<float, __nv_bfloat16>(
    const float * __restrict__, const int32_t * __restrict__, __nv_bfloat16 * __restrict__,
    const int64_t, const int64_t, const int64_t,
    const size_t, const size_t, const size_t,
    const size_t, const size_t, const size_t,
    const size_t, const size_t, const size_t);
