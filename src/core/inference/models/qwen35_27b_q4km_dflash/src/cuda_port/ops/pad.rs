//! Rust port of the pad-op dispatcher in
//! `vendor/ggml-cuda/pad.cu`.
//!
//! ref: vendor/ggml-cuda/pad.cu
//!
//! Copies src into a zero-padded dst with per-axis left/right
//! padding counts, or circularly wraps src to fill dst when
//! `circular` is set. f32 only — the kernel itself isn't templated.
//!
//! # Mangled-name handling
//!
//! `pad_f32` is `static __global__`, so nvcc adds a per-TU hash
//! (`_INTERNAL_xxx_pad_cu_yyy`). Resolved by substring match on
//! the function name, not the hash.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/pad.cuh:3
const CUDA_PAD_BLOCK_SIZE: c_int = 256;

/// Resolved handle for `pad_f32`.
#[derive(Default)]
pub struct PadKernels {
    pub pad_f32: CUfunction,
}

pub fn mangled_pad_f32() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::pad_entries::ENTRIES,
        // `pad_f32` prefix is unique in pad.ptx (only one kernel).
        &[b"pad_f32"],
    )
}

/// Per-axis left/right padding counts — matches `dst->op_params[0..8]`
/// on the upstream path.
#[derive(Copy, Clone, Debug, Default)]
pub struct PadParams {
    pub lp: [i32; 4],
    pub rp: [i32; 4],
    pub circular: bool,
}

/// ref: vendor/ggml-cuda/pad.cu:64-74
///
/// `src_elem_strides` is `(s00, s01, s02, s03)` — stride in src
/// elements (upstream divides `nb00…nb03` by sizeof(float) before
/// the launch). `dst_ne` is `(ne0, ne1, ne2, ne3)` for the output.
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_pad_f32(
    kernels: &PadKernels,
    src: CUdeviceptr,
    dst: CUdeviceptr,
    src_elem_strides: [usize; 4],
    dst_ne: [i32; 4],
    params: PadParams,
    stream: CUstream,
) -> CUresult {
    let num_blocks = ((dst_ne[0] + CUDA_PAD_BLOCK_SIZE - 1) / CUDA_PAD_BLOCK_SIZE) as u32;
    let grid_x = num_blocks;
    let grid_y = dst_ne[1] as u32;
    let grid_z = (dst_ne[2] * dst_ne[3]) as u32;

    // Kernel signature (pad_f32, 1-to-1 order):
    //   (const float * src,
    //    size_t s00, size_t s01, size_t s02, size_t s03,
    //    float * dst,
    //    int lp0, int rp0, int lp1, int rp1,
    //    int lp2, int rp2, int lp3, int rp3,
    //    int ne0, int ne1, int ne2, int ne3,
    //    bool circular)
    // Total = 19 scalar/ptr args (size_t is 8 bytes on linux-x64).
    let src_val = src.0;
    let dst_val = dst.0;
    let s00 = src_elem_strides[0] as u64;
    let s01 = src_elem_strides[1] as u64;
    let s02 = src_elem_strides[2] as u64;
    let s03 = src_elem_strides[3] as u64;

    let lp0 = params.lp[0];
    let rp0 = params.rp[0];
    let lp1 = params.lp[1];
    let rp1 = params.rp[1];
    let lp2 = params.lp[2];
    let rp2 = params.rp[2];
    let lp3 = params.lp[3];
    let rp3 = params.rp[3];

    let ne0 = dst_ne[0];
    let ne1 = dst_ne[1];
    let ne2 = dst_ne[2];
    let ne3 = dst_ne[3];

    let circ = params.circular as u8;

    let args: [*const c_void; 19] = [
        &src_val as *const u64 as *const c_void,
        &s00 as *const u64 as *const c_void,
        &s01 as *const u64 as *const c_void,
        &s02 as *const u64 as *const c_void,
        &s03 as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &lp0 as *const i32 as *const c_void,
        &rp0 as *const i32 as *const c_void,
        &lp1 as *const i32 as *const c_void,
        &rp1 as *const i32 as *const c_void,
        &lp2 as *const i32 as *const c_void,
        &rp2 as *const i32 as *const c_void,
        &lp3 as *const i32 as *const c_void,
        &rp3 as *const i32 as *const c_void,
        &ne0 as *const i32 as *const c_void,
        &ne1 as *const i32 as *const c_void,
        &ne2 as *const i32 as *const c_void,
        &ne3 as *const i32 as *const c_void,
        &circ as *const u8 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            kernels.pad_f32,
            grid_x,
            grid_y,
            grid_z,
            CUDA_PAD_BLOCK_SIZE as u32,
            1,
            1,
            0,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}
