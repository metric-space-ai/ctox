//! Rust port of the cpy-op dispatcher in
//! `vendor/ggml-cuda/cpy.cu`.
//!
//! ref: vendor/ggml-cuda/cpy.cu
//!
//! Elementwise copy + dtype conversion between two tensors with
//! arbitrary (potentially broadcastable) layouts. Upstream has a
//! large family of kernels:
//!
//!   • `cpy_scalar_contiguous<src_t, dst_t>` — fast path when both
//!     sides are contiguous.
//!   • `cpy_scalar<cpy_1_scalar<src_t, dst_t>>` — generic scalar
//!     path, indexes through per-axis strides.
//!   • `cpy_scalar_transpose<T>` — transposed 2D copy.
//!   • `cpy_f32_q<cpy_blck_…>` — quantizing copies (q4_0, q4_1,
//!     q5_0, q5_1, q8_0, iq4_nl).
//!   • `cpy_q_f32<cpy_blck_…>` — dequantizing copies.
//!
//! This port covers the Qwen3.5 hot path: the **generic
//! `cpy_scalar<cpy_1_scalar<src, dst>>`** kernel for three dtype
//! pairs that actually fire in the forward graph:
//!
//!   • f32 → f32   — reshape/view materialisation
//!   • f32 → f16   — KV-cache stores
//!   • f16 → f16   — KV-cache slot passthrough
//!
//! The contiguous-fast-path and transposed variants are pure perf
//! optimisations over the generic kernel. Adding them later is
//! ~20 lines each; until a profile says the generic path costs us
//! cycles, it's equivalent on correctness.
//!
//! Quantizing copies (f32↔q8_0/q4_0/...) are NOT ported here — the
//! Qwen3.5 forward path doesn't copy into quantized tensors at
//! runtime. Weights stay in their pre-baked quantized form.
//!
//! # Mangled-name handling
//!
//! `cpy_scalar<F>` is templated on a function pointer to a
//! `cpy_1_scalar<src, dst>` instantiation. Three entries land in
//! `cpy.ptx` for our three dtype pairs. We resolve by substring
//! search — both the outer kernel name `cpy_scalar` AND the inner
//! functor's dtype signature (`IfEfE` for (float,float), `IfE6__half`
//! for (float,__half), `I6__halfS` for (half,half)).

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/cpy.cuh:3
const CUDA_CPY_BLOCK_SIZE: c_int = 64;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CpyDtype {
    F32ToF32,
    F32ToF16,
    F16ToF16,
}

#[derive(Default)]
pub struct CpyKernels {
    pub f32_f32: CUfunction,
    pub f32_f16: CUfunction,
    pub f16_f16: CUfunction,
}

impl CpyKernels {
    fn func(&self, d: CpyDtype) -> CUfunction {
        match d {
            CpyDtype::F32ToF32 => self.f32_f32,
            CpyDtype::F32ToF16 => self.f32_f16,
            CpyDtype::F16ToF16 => self.f16_f16,
        }
    }
}

/// Resolve `cpy_scalar<cpy_1_scalar<src, dst>>` in cpy.ptx.
///
/// The outer template arg is a function-pointer NTTP pointing at
/// a `cpy_1_scalar<src, dst>` instantiation. nvcc mangles that as
/// an `IXad…EE` block whose inside contains the inner template's
/// dtype pair. Pragmatic needle set: the two-char pair selectors
/// observed in the emitted PTX.
pub fn mangled_cpy_scalar(dtype: CpyDtype) -> Result<&'static [u8], String> {
    // Needle tying to the inner `cpy_1_scalar<src,dst>`'s dtype
    // run. In Itanium:
    //   (float, float)      → `If f` split as `If … f E`
    //   (float, __half)     → `If 6__half`
    //   (__half, __half)    → `I 6__half S_` (S_ = backref)
    let dtype_needle: &[u8] = match dtype {
        CpyDtype::F32ToF32 => b"IffEvPKcPc",
        CpyDtype::F32ToF16 => b"If6__halfEvPKcPc",
        CpyDtype::F16ToF16 => b"I6__halfS_EvPKcPc",
    };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::cpy_entries::ENTRIES,
        &[b"cpy_scalar", dtype_needle],
    )
}

/// ref: vendor/ggml-cuda/cpy.cu:199-234 (`ggml_cpy_scalar_cuda`, non-transposed branch)
///
/// Generic launcher for the scalar cpy path. `ne_total` is the
/// total number of destination elements to copy
/// (`ggml_nelements(src0)` upstream). All stride args are in
/// **bytes** — the kernel indexes `char*` buffers.
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_cpy_scalar(
    kernels: &CpyKernels,
    dtype: CpyDtype,
    cx: CUdeviceptr,
    cdst: CUdeviceptr,
    ne_total: i64,
    src_ne: [i64; 3], // ne00, ne01, ne02
    src_nb: [i64; 4], // nb00, nb01, nb02, nb03
    dst_ne: [i64; 3], // ne10, ne11, ne12
    dst_nb: [i64; 4], // nb10, nb11, nb12, nb13
    stream: CUstream,
) -> CUresult {
    let num_blocks = ((ne_total + CUDA_CPY_BLOCK_SIZE as i64 - 1)
        / CUDA_CPY_BLOCK_SIZE as i64) as u32;

    // Kernel signature (cpy.cu:14-18):
    //   cpy_scalar(const char * cx, char * cdst,
    //              int64_t ne,
    //              int64_t ne00, int64_t ne01, int64_t ne02,
    //              int64_t nb00, int64_t nb01, int64_t nb02,
    //              int64_t nb03,
    //              int64_t ne10, int64_t ne11, int64_t ne12,
    //              int64_t nb10, int64_t nb11, int64_t nb12,
    //              int64_t nb13)
    // = 2 ptrs + 15 scalar slots = 17 args.
    let cx_val = cx.0;
    let dst_val = cdst.0;

    let args: [*const c_void; 17] = [
        &cx_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &ne_total as *const i64 as *const c_void,
        &src_ne[0] as *const i64 as *const c_void,
        &src_ne[1] as *const i64 as *const c_void,
        &src_ne[2] as *const i64 as *const c_void,
        &src_nb[0] as *const i64 as *const c_void,
        &src_nb[1] as *const i64 as *const c_void,
        &src_nb[2] as *const i64 as *const c_void,
        &src_nb[3] as *const i64 as *const c_void,
        &dst_ne[0] as *const i64 as *const c_void,
        &dst_ne[1] as *const i64 as *const c_void,
        &dst_ne[2] as *const i64 as *const c_void,
        &dst_nb[0] as *const i64 as *const c_void,
        &dst_nb[1] as *const i64 as *const c_void,
        &dst_nb[2] as *const i64 as *const c_void,
        &dst_nb[3] as *const i64 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            kernels.func(dtype),
            num_blocks,
            1,
            1,
            CUDA_CPY_BLOCK_SIZE as u32,
            1,
            1,
            0,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}
