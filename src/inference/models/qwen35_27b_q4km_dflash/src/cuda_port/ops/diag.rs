//! Rust port of the diag-op dispatcher in
//! `vendor/ggml-cuda/diag.cu`.
//!
//! ref: vendor/ggml-cuda/diag.cu
//!
//! Materialises `diag(v)` — a (ne0, ne0, ne2, ne3) tensor whose
//! diagonal copies the source vector at `(ne0, 1, ne2, ne3)` and
//! all off-diagonal elements are zero. Single template kernel,
//! instantiated for f32 and f16.
//!
//! # Mangled-name handling
//!
//! `diag_kernel` is a `static __global__` template, so each
//! `(T)` instantiation gets its own PTX `.entry` with a per-TU
//! hash prefix. Resolved by substring-AND on the Itanium
//! template-arg encoding (`IfE` for float, `I6__halfE` for half),
//! identical pattern to fill.rs.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/diag.cuh:3
const CUDA_DIAG_BLOCK_SIZE: c_int = 256;

/// Resolved kernel handles for the two dtype instantiations.
#[derive(Default)]
pub struct DiagKernels {
    pub diag_f32: CUfunction,
    pub diag_f16: CUfunction,
}

pub fn mangled_diag_kernel_f32() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::diag_entries::ENTRIES,
        &[b"diag_kernel", b"IfE"],
    )
}

pub fn mangled_diag_kernel_f16() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::diag_entries::ENTRIES,
        &[b"diag_kernel", b"6__half"],
    )
}

/// Generic launcher shared between the f32 and f16 entry points.
/// ref: vendor/ggml-cuda/diag.cu:62-73
fn diag_cuda_launch(
    func: CUfunction,
    dst: CUdeviceptr,
    src: CUdeviceptr,
    ne0: i64,
    ne1: i64,
    ne2: i64,
    ne3: i64,
    total_elements: i64,
    stream: CUstream,
) -> CUresult {
    let num_blocks =
        ((total_elements + CUDA_DIAG_BLOCK_SIZE as i64 - 1) / CUDA_DIAG_BLOCK_SIZE as i64) as u32;

    // diag_kernel(T * dst, const T * src,
    //             int64_t ne0, int64_t ne1, int64_t ne2, int64_t ne3,
    //             int64_t total_elements) — 7 args.
    let dst_val = dst.0;
    let src_val = src.0;
    let args: [*const c_void; 7] = [
        &dst_val as *const u64 as *const c_void,
        &src_val as *const u64 as *const c_void,
        &ne0 as *const i64 as *const c_void,
        &ne1 as *const i64 as *const c_void,
        &ne2 as *const i64 as *const c_void,
        &ne3 as *const i64 as *const c_void,
        &total_elements as *const i64 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            func,
            num_blocks,
            1,
            1,
            CUDA_DIAG_BLOCK_SIZE as u32,
            1,
            1,
            0,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// ref: vendor/ggml-cuda/diag.cu:36 (f32 branch of switch)
///
/// Caller must have already asserted:
///   ne00 == ne0, ne01 == 1, ne02 == ne2, ne03 == ne3,
///   both tensors contiguous.
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_diag_f32(
    kernels: &DiagKernels,
    dst: CUdeviceptr,
    src: CUdeviceptr,
    ne0: i64,
    ne1: i64,
    ne2: i64,
    ne3: i64,
    total_elements: i64,
    stream: CUstream,
) -> CUresult {
    diag_cuda_launch(
        kernels.diag_f32,
        dst,
        src,
        ne0,
        ne1,
        ne2,
        ne3,
        total_elements,
        stream,
    )
}

/// ref: vendor/ggml-cuda/diag.cu:36 (f16 branch of switch)
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_diag_f16(
    kernels: &DiagKernels,
    dst: CUdeviceptr,
    src: CUdeviceptr,
    ne0: i64,
    ne1: i64,
    ne2: i64,
    ne3: i64,
    total_elements: i64,
    stream: CUstream,
) -> CUresult {
    diag_cuda_launch(
        kernels.diag_f16,
        dst,
        src,
        ne0,
        ne1,
        ne2,
        ne3,
        total_elements,
        stream,
    )
}
