//! Rust port of the softmax-op dispatcher in
//! `vendor/ggml-cuda/softmax.cu`.
//!
//! ref: vendor/ggml-cuda/softmax.cu
//!
//! Fused softmax with optional mask + alibi slope + attention-sink.
//! Upstream has:
//!   • `soft_max_f32<use_shared, ncols_template, block_size_template, T>`
//!     — the main kernel. 8 ncols specializations × 2 mask dtypes
//!     × 2 block-size specializations = ~32 instantiations.
//!   • `soft_max_f32_parallelize_cols` + `..._single_row`
//!     — cooperative launch for very wide softmax (>8192 cols
//!     beyond smpbo).
//!   • `soft_max_back_f32` — backward pass (not on the forward
//!     path).
//!
//! Scope of the current port: the runtime-generic
//! `soft_max_f32<true, 0, 0, T>` fallback for T ∈ {f32, f16}
//! (mask dtype). This handles any (ncols, block_size) pair that
//! fits within the device's max shared-memory-per-block budget
//! (`smpbo`) — i.e. effectively all Qwen3.5 attention softmaxes up
//! to the 128K context. Wider softmaxes would need either the
//! cooperative-launch path (pool alloc for tmp_maxs/tmp_sums) or
//! the `use_shared=false` variant.
//!
//! # Mangled-name handling
//!
//! `soft_max_f32` is a `static __global__` template. nvcc emits
//! one PTX entry per unique (use_shared, ncols_template,
//! block_size_template, T) combo. We resolve by substring-AND on
//! the functor name prefix + the 4 template args.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/softmax.cuh (CUDA_SOFT_MAX_BLOCK_SIZE)
const CUDA_SOFT_MAX_BLOCK_SIZE: c_int = 1024;
/// Warp size on Ampere / Ada / Hopper.
const WARP_SIZE: c_int = 32;

/// Mirrors upstream `struct soft_max_params` (softmax.cu:25-46).
/// Field order + types kept bit-identical so we can hand this to
/// the kernel by value via cuLaunchKernel.
#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct SoftMaxParams {
    pub nheads: i64,
    pub n_head_log2: u32,
    // 4 bytes of padding to align the next i64 on 8.
    _pad0: u32,
    pub ncols: i64,
    pub nrows_x: i64,
    pub nrows_y: i64,
    pub ne00: i64,
    pub ne01: i64,
    pub ne02: i64,
    pub ne03: i64,
    pub nb11: i64,
    pub nb12: i64,
    pub nb13: i64,
    pub ne12: i64,
    pub ne13: i64,
    pub scale: f32,
    pub max_bias: f32,
    pub m0: f32,
    pub m1: f32,
}

/// Resolved kernel handles for the two mask-dtype variants we wire.
#[derive(Default)]
pub struct SoftMaxKernels {
    /// `soft_max_f32<true, 0, 0, float>` — mask is f32 (or null).
    pub f32_mask_f32: CUfunction,
    /// `soft_max_f32<true, 0, 0, half>` — mask is f16.
    pub f32_mask_f16: CUfunction,
}

/// Resolve one instantiation by substring-AND match.
/// Needles pin:
///   • `12soft_max_f32` — Itanium length prefix (12 chars).
///   • `Lb1ELi0ELi0E`   — template args `<true, 0, 0>` in
///                        Itanium encoding.
///   • T-discriminator: `fE` for f32, `6__halfE` for f16.
pub fn mangled_soft_max_f32_fallback(mask_is_f16: bool) -> Result<&'static [u8], String> {
    let t: &[u8] = if mask_is_f16 { b"6__halfE" } else { b"fE" };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::softmax_entries::ENTRIES,
        &[b"12soft_max_f32", b"Lb1ELi0ELi0E", t],
    )
}

/// ref: vendor/ggml-cuda/softmax.cu:319-364 (non-cooperative path)
///
/// Single entry-point covering mask-f32 and mask-f16 paths. Upstream
/// picks the block size with `nth = WARP_SIZE; while nth < ncols_x
/// && nth < 1024: nth *= 2` — reproduced here.
///
/// `mask` may be a null device ptr; the kernel then reads `mask ==
/// nullptr` inside and skips the mask add. Same for `sinks` (the
/// attention-sink scalar-per-head buffer, used by GPT-OSS-style
/// attention; Qwen3.5 usually passes null).
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_soft_max(
    kernels: &SoftMaxKernels,
    x: CUdeviceptr,
    mask: CUdeviceptr,
    sinks: CUdeviceptr,
    dst: CUdeviceptr,
    params: &SoftMaxParams,
    mask_is_f16: bool,
    stream: CUstream,
) -> CUresult {
    let func = if mask_is_f16 {
        kernels.f32_mask_f16
    } else {
        kernels.f32_mask_f32
    };

    // ref: softmax.cu:327-333
    let mut nth = WARP_SIZE;
    let ncols_x = params.ncols as c_int;
    while nth < ncols_x && nth < CUDA_SOFT_MAX_BLOCK_SIZE {
        nth *= 2;
    }

    let block_dims = (nth as u32, 1u32, 1u32);
    let grid_dims = (
        params.ne01 as u32,
        params.ne02 as u32,
        params.ne03 as u32,
    );

    // shared-mem budget: PAD(ncols, WARP_SIZE) + WARP_SIZE floats.
    let padded = (ncols_x + WARP_SIZE - 1) & !(WARP_SIZE - 1);
    let nbytes_shared = ((padded + WARP_SIZE) as usize * std::mem::size_of::<f32>()) as u32;

    // Kernel signature (softmax.cu:54-57):
    //   (const float * x, const T * mask, const float * sinks, float * dst,
    //    const soft_max_params p)
    // → 4 pointers + 1 struct passed by value.
    let x_val = x.0;
    let mask_val = mask.0;
    let sinks_val = sinks.0;
    let dst_val = dst.0;
    let args: [*const c_void; 5] = [
        &x_val as *const u64 as *const c_void,
        &mask_val as *const u64 as *const c_void,
        &sinks_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        params as *const SoftMaxParams as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            func,
            grid_dims.0,
            grid_dims.1,
            grid_dims.2,
            block_dims.0,
            block_dims.1,
            block_dims.2,
            nbytes_shared,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `soft_max_params` must have the exact C layout the kernel
    /// expects. Spot-check size + offsets against the C struct
    /// (18 scalars; 12 × i64 + 4 × f32 + 1 × u32 + 1 × u32 pad =
    /// 12*8 + 4*4 + 2*4 = 96 + 16 + 8 = 120 bytes on x86_64 Linux).
    #[test]
    fn params_size_matches_upstream() {
        assert_eq!(std::mem::size_of::<SoftMaxParams>(), 120);
    }
}
