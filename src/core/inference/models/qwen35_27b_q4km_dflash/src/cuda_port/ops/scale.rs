//! Rust port of the scale-op dispatcher in
//! `vendor/ggml-cuda/scale.cu`.
//!
//! ref: vendor/ggml-cuda/scale.cu
//!
//! Elementwise `dst[i] = scale * src0[i] + bias`. Single kernel
//! variant, f32 only. The C++ dispatcher pulls `scale` + `bias`
//! out of `dst->op_params[0..1]`; since our caller hands us the
//! already-unpacked floats, we skip that step.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/scale.cuh:3
const CUDA_SCALE_BLOCK_SIZE: c_int = 256;

/// ref: vendor/ggml-cuda/scale.cu:3
const MAX_GRIDDIM_X: i64 = 0x7FFFFFFF;

/// Resolved kernel handle.
#[derive(Default)]
pub struct ScaleKernel {
    pub scale_f32: CUfunction,
}

/// Find the mangled name of `scale_f32` in scale.ptx. Top-level
/// file-scope function → stable mangled name (no TU hash), but
/// we still resolve by substring so the lookup pattern is
/// uniform with the other ports.
pub fn mangled_scale_f32() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::scale_entries::ENTRIES,
        &[b"scale_f32"],
    )
}

/// ref: vendor/ggml-cuda/scale.cu:15-18
#[allow(clippy::too_many_arguments)]
pub fn scale_f32_cuda(
    kernel: &ScaleKernel,
    x: CUdeviceptr,
    dst: CUdeviceptr,
    scale: f32,
    bias: f32,
    nelements: i64,
    stream: CUstream,
) -> CUresult {
    // ref: scale.cu:16 — num_blocks = (nelements + BLK - 1) / BLK,
    //                    capped at MAX_GRIDDIM_X.
    let num_blocks =
        (nelements + CUDA_SCALE_BLOCK_SIZE as i64 - 1) / CUDA_SCALE_BLOCK_SIZE as i64;
    let grid_x = num_blocks.min(MAX_GRIDDIM_X) as u32;

    // scale_f32(const float * x, float * dst, const float scale,
    //           const float bias, const int64_t nelements)
    let x_val = x.0;
    let dst_val = dst.0;
    let args: [*const c_void; 5] = [
        &x_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &scale as *const f32 as *const c_void,
        &bias as *const f32 as *const c_void,
        &nelements as *const i64 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            kernel.scale_f32,
            grid_x,
            1,
            1,
            CUDA_SCALE_BLOCK_SIZE as u32,
            1,
            1,
            0,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// ref: vendor/ggml-cuda/scale.cu:20-34
///
/// Op-level entry. Differs from the C++ version: instead of
/// unpacking `scale`/`bias` from `dst->op_params`, the caller
/// passes them explicitly (no ggml_tensor on the Rust side).
pub fn ggml_cuda_op_scale_f32(
    kernel: &ScaleKernel,
    src0: CUdeviceptr,
    dst: CUdeviceptr,
    nelements: i64,
    scale: f32,
    bias: f32,
    stream: CUstream,
) -> CUresult {
    scale_f32_cuda(kernel, src0, dst, scale, bias, nelements, stream)
}
