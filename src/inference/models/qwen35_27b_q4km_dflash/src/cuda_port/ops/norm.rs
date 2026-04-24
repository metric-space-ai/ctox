//! Rust port of the host-side dispatchers in
//! `vendor/ggml-cuda/norm.cu`.
//!
//! ref: vendor/ggml-cuda/norm.cu
//!
//! The kernel templates themselves (`__global__ void rms_norm_f32
//! <block_size, do_multiply, do_add>` at line 153+) stay in
//! `norm.cu` — compiled by `build.rs` into `norm.ptx`. This file
//! ports only the host-side launchers and the op-dispatcher entry.
//!
//! For qwen35_27b_q4km_dflash the only entry this model actually
//! uses is `ggml_cuda_op_rms_norm` (plain RMSNorm, no fused
//! multiply/add). Other norm variants (group_norm, l2_norm,
//! rms_norm_fused, rms_norm_back, etc.) are intentionally not
//! ported here — qwen35's forward graph doesn't hit them, and
//! CLAUDE.md rule 5 demands we port only what the model needs.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{
    cuLaunchKernel, CUDA_SUCCESS, CUdeviceptr, CUfunction, CUresult, CUstream,
};

/// Kernel warp size (Ampere, Hopper, and Ada all expose 32).
/// ref: vendor/ggml-cuda/common.cuh (WARP_SIZE define)
const WARP_SIZE: c_int = 32;

/// Resolve the two `rms_norm_f32<N,false,false>` kernel names at
/// runtime by scanning the build-time-parsed PTX entries for
/// substring matches. More robust than hard-coded strings —
/// future upstream renames / ABI tweaks just need the crate
/// recompiled, not a patch here.
pub fn mangled_rms_norm_f32_b256() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::norm_entries::ENTRIES,
        &[b"rms_norm_f32", b"Li256", b"Lb0ELb0"],
    )
}

pub fn mangled_rms_norm_f32_b1024() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::norm_entries::ENTRIES,
        &[b"rms_norm_f32", b"Li1024", b"Lb0ELb0"],
    )
}

/// Handle pair — the two kernel instantiations the dispatcher picks
/// between. Resolved once per module-load by
/// `cuModuleGetFunction(module, MANGLED_RMS_NORM_F32_{B256,B1024})`.
pub struct RmsNormKernels {
    pub b256: CUfunction,
    pub b1024: CUfunction,
}

/// ref: vendor/ggml-cuda/norm.cu:297-308
///
/// Picks the `<256>` vs `<1024>` instantiation based on `ncols`,
/// sets the launch config identical to the C++ path (grid =
/// `(nrows, nchannels, nsamples)`, block = `(block_size, 1, 1)`,
/// shared-mem = 32·sizeof(float) iff `block_size > WARP_SIZE` else
/// 0), then calls `cuLaunchKernel`.
///
/// Arguments match the C++ launcher one-for-one.
#[allow(clippy::too_many_arguments)]
pub fn rms_norm_f32_cuda(
    kernels: &RmsNormKernels,
    x: CUdeviceptr,
    dst: CUdeviceptr,
    ncols: c_int,
    nrows: c_int,
    nchannels: c_int,
    nsamples: c_int,
    stride_row: i64,
    stride_channel: i64,
    stride_sample: i64,
    eps: f32,
    stream: CUstream,
) -> CUresult {
    // ref: norm.cu:300 (blocks_num)
    let grid_x = nrows as u32;
    let grid_y = nchannels as u32;
    let grid_z = nsamples as u32;

    // ref: norm.cu:301-306 (pick block size + function)
    let (func, block_x) = if ncols < 1024 {
        (kernels.b256, 256_u32)
    } else {
        (kernels.b1024, 1024_u32)
    };

    // ref: norm.cu:303,306 — shmem 32·sizeof(float) when
    //      block_dims.x > WARP_SIZE, else 0.
    let shared_mem: u32 = if (block_x as c_int) > WARP_SIZE {
        32 * std::mem::size_of::<f32>() as u32
    } else {
        0
    };

    // Kernel args — pointers to each argument (cuLaunchKernel's ABI).
    //
    // `rms_norm_f32<N, do_multiply=false, do_add=false>` has 23
    // template parameters total (see vendor/ggml-cuda/norm.cu:74-98).
    // The C++ caller omits the trailing ones because they default,
    // but cuLaunchKernel sees the full PTX parameter list and we
    // must push every slot. For do_multiply/do_add = false the
    // kernel skips the corresponding code paths (via `if constexpr`),
    // so the zero values we pass are never dereferenced / used.
    //
    // Layout (ref: norm.cu:74-98):
    //   PKf        const float *  x
    //   Pf         float *        dst
    //   i          int            ncols
    //   l l l      int64_t × 3    stride_{row,channel,sample}
    //   f          float          eps
    //   PKf        const float *  mul               (default nullptr)
    //   l l l      int64_t × 3    mul_stride_{row,channel,sample}
    //   uint3 ×4                  mul_{ncols,nrows,nchannels,nsamples}_packed
    //   PKf        const float *  add               (default nullptr)
    //   l l l      int64_t × 3    add_stride_{row,channel,sample}
    //   uint3 ×4                  add_{ncols,nrows,nchannels,nsamples}_packed
    let x_val = x.0;
    let dst_val = dst.0;
    let null_ptr: u64 = 0;
    let zero_i64: i64 = 0;
    let zero_u3: [u32; 3] = [0, 0, 0];
    let args: [*const c_void; 23] = [
        &x_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &ncols as *const c_int as *const c_void,
        &stride_row as *const i64 as *const c_void,
        &stride_channel as *const i64 as *const c_void,
        &stride_sample as *const i64 as *const c_void,
        &eps as *const f32 as *const c_void,
        // mul defaults
        &null_ptr as *const u64 as *const c_void,
        &zero_i64 as *const i64 as *const c_void,
        &zero_i64 as *const i64 as *const c_void,
        &zero_i64 as *const i64 as *const c_void,
        &zero_u3 as *const [u32; 3] as *const c_void,
        &zero_u3 as *const [u32; 3] as *const c_void,
        &zero_u3 as *const [u32; 3] as *const c_void,
        &zero_u3 as *const [u32; 3] as *const c_void,
        // add defaults
        &null_ptr as *const u64 as *const c_void,
        &zero_i64 as *const i64 as *const c_void,
        &zero_i64 as *const i64 as *const c_void,
        &zero_i64 as *const i64 as *const c_void,
        &zero_u3 as *const [u32; 3] as *const c_void,
        &zero_u3 as *const [u32; 3] as *const c_void,
        &zero_u3 as *const [u32; 3] as *const c_void,
        &zero_u3 as *const [u32; 3] as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            func,
            grid_x,
            grid_y,
            grid_z,
            block_x,
            1,
            1,
            shared_mem,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// ref: vendor/ggml-cuda/norm.cu:452-475
///
/// The op-level entry — extracts tensor metadata (shape, strides,
/// op_params), asserts shape constraints, and delegates to
/// `rms_norm_f32_cuda`. Unlike the C++ version we take explicit
/// args rather than a ggml_tensor so the caller (a Rust-side
/// compute graph) doesn't need to fake up a ggml_tensor struct.
///
/// Mirrors the C++ body line-for-line, minus the
/// `GGML_TENSOR_UNARY_OP_LOCALS` macro which is expanded inline
/// here (ne00..ne03 = shape, nb00..nb03 = byte strides).
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_rms_norm(
    kernels: &RmsNormKernels,
    src0_d: CUdeviceptr,
    dst_d: CUdeviceptr,
    // shape
    ne00: c_int,
    ne01: c_int,
    ne02: c_int,
    ne03: c_int,
    // byte strides (src0)
    nb00: i64,
    nb01: i64,
    nb02: i64,
    nb03: i64,
    // op param
    eps: f32,
    // cuda
    stream: CUstream,
) -> CUresult {
    // ref: norm.cu:465 `GGML_ASSERT(eps >= 0.0f)`
    debug_assert!(eps >= 0.0, "rms_norm eps must be >= 0.0");

    // ref: norm.cu:467-471 — ts0 = sizeof(f32) here since we've
    // asserted src0.type == F32 at the graph layer; byte strides
    // divided by ts0 give the element-count strides the kernel
    // wants.
    let ts0 = std::mem::size_of::<f32>() as i64;
    debug_assert_eq!(nb00, ts0, "src0 must be contiguous along dim 0");
    let s01 = nb01 / ts0;
    let s02 = nb02 / ts0;
    let s03 = nb03 / ts0;

    // ref: norm.cu:473
    rms_norm_f32_cuda(
        kernels, src0_d, dst_d, ne00, ne01, ne02, ne03, s01, s02, s03, eps, stream,
    )
}

/// Return `Ok(())` on a successful launch, `Err(msg)` on a CUDA
/// driver error (with `cuGetErrorString` rendering).
pub fn check(rc: CUresult, what: &str) -> Result<(), String> {
    if rc == CUDA_SUCCESS {
        Ok(())
    } else {
        Err(format!("{what}: {}", super::super::driver::error_string(rc)))
    }
}
