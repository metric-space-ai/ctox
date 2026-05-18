//! Rust port of the cumsum-op dispatcher in
//! `vendor/ggml-cuda/cumsum.cu`.
//!
//! ref: vendor/ggml-cuda/cumsum.cu
//!
//! Inclusive prefix-sum along axis 0 (innermost), independently per
//! (i1, i2, i3) row. Upstream has two kernel variants:
//!
//!   1. `cumsum_cub_kernel<T, BLOCK_SIZE>` — requires
//!      `GGML_CUDA_USE_CUB`, enabled only when CUB is available at
//!      build time AND the inner dim is contiguous AND `ne00 >=
//!      1024`.
//!   2. `cumsum_kernel<T>` — hand-rolled warp+block scan fallback,
//!      works for every layout.
//!
//! Our `build.rs` does **not** set `GGML_CUDA_USE_CUB`, so only the
//! fallback gets compiled into `cumsum.ptx`. The Rust side only
//! wires that one; if we ever opt into CUB we add a second handle.
//!
//! Only f32 is supported — upstream comments out f16 / bf16
//! instantiations, and Qwen3.5's graph only uses f32 here.
//!
//! # Launch config
//!
//! Upstream picks a block size based on the warp size and ne00:
//! `block_size = min(num_warps * warp_size, 256)` where `num_warps =
//! (ne00 + ws - 1) / ws`. For SM_8.6 (A6000) warp_size is always 32,
//! so the host-side math we reproduce here uses 32 literal.
//!
//! Shmem budget: `(block_size + warps_per_block + 2) * sizeof(f32)`.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/cumsum.cuh:3
const CUDA_CUMSUM_BLOCK_SIZE: c_int = 256;

/// Physical warp size on Ampere / Ada / Hopper.
const WARP_SIZE: c_int = 32;

#[derive(Default)]
pub struct CumsumKernels {
    /// `cumsum_kernel<float>` — fallback (non-CUB) scan.
    pub cumsum_f32: CUfunction,
}

pub fn mangled_cumsum_kernel_f32() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::cumsum_entries::ENTRIES,
        // `cumsum_kernel` prefix + `IfE` template-arg run selects
        // the f32 instantiation; the `cumsum_cub_kernel` PTX entry
        // (if present) has a longer name that doesn't start with
        // `cumsum_kernel` so this remains unambiguous.
        &[b"cumsum_kernel", b"IfE"],
    )
}

/// ref: vendor/ggml-cuda/cumsum.cu:213-265
///
/// f32 entry. Strides `s_*` are in **elements** (upstream divides
/// `nb_*` by `sizeof(T)` host-side before launch).
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_cumsum_f32(
    kernels: &CumsumKernels,
    src: CUdeviceptr,
    dst: CUdeviceptr,
    ne: [i64; 4],
    s_src: [i64; 4],
    s_dst: [i64; 4],
    stream: CUstream,
) -> CUresult {
    let num_warps = ((ne[0] + WARP_SIZE as i64 - 1) / WARP_SIZE as i64) as c_int;
    let mut block_size = num_warps * WARP_SIZE;
    if block_size > CUDA_CUMSUM_BLOCK_SIZE {
        block_size = CUDA_CUMSUM_BLOCK_SIZE;
    }
    // Upstream code doesn't guard against block_size == 0 — happens
    // if ne[0] == 0 (empty tensor). Defend here.
    if block_size == 0 {
        return 0; // CUDA_SUCCESS — nothing to launch.
    }
    let warps_per_block = block_size / WARP_SIZE;
    let shmem_size =
        ((block_size + warps_per_block + 2) as usize * std::mem::size_of::<f32>()) as u32;

    let grid_x = ne[1] as u32;
    let grid_y = ne[2] as u32;
    let grid_z = ne[3] as u32;

    // Kernel signature (cumsum.cu:86-90, the non-CUB fallback):
    //   cumsum_kernel(const float * src, float * dst,
    //                 int64_t ne00..ne03,
    //                 int64_t s00..s03,
    //                 int64_t s0..s3)
    // 2 ptrs + 4 ne + 4 src-strides + 4 dst-strides = 14 args.
    let src_val = src.0;
    let dst_val = dst.0;
    let args: [*const c_void; 14] = [
        &src_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &ne[0] as *const i64 as *const c_void,
        &ne[1] as *const i64 as *const c_void,
        &ne[2] as *const i64 as *const c_void,
        &ne[3] as *const i64 as *const c_void,
        &s_src[0] as *const i64 as *const c_void,
        &s_src[1] as *const i64 as *const c_void,
        &s_src[2] as *const i64 as *const c_void,
        &s_src[3] as *const i64 as *const c_void,
        &s_dst[0] as *const i64 as *const c_void,
        &s_dst[1] as *const i64 as *const c_void,
        &s_dst[2] as *const i64 as *const c_void,
        &s_dst[3] as *const i64 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            kernels.cumsum_f32,
            grid_x,
            grid_y,
            grid_z,
            block_size as u32,
            1,
            1,
            shmem_size,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}
