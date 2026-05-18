//! Rust port of the fill-op dispatcher in
//! `vendor/ggml-cuda/fill.cu`.
//!
//! ref: vendor/ggml-cuda/fill.cu
//!
//! Writes a constant scalar to every element of a contiguous output
//! tensor. The `.cu` template is
//! `fill_kernel<T>(T * dst, const int64_t k, const T value)` —
//! compiled by `build.rs` into `fill.ptx`. Two PTX entries are
//! produced, one for `T = float` and one for `T = __half`.
//!
//! For qwen35 the only fill we need on the hot path is f32 (RoPE
//! frequency bookkeeping, mask fills, zero-init workspaces). The
//! f16 variant is still wired up so the Rust dispatcher can fall
//! back for potentially f16 tensors without a second port later.
//!
//! # Mangled-name handling
//!
//! Same trick as unary.rs: `fill_kernel` is a template, so nvcc
//! emits one `.entry` per instantiation. We locate the entries at
//! runtime by scanning `fill.ptx`'s `.entry` list for the stable
//! substrings — `fill_kernel` plus the template-arg discriminator
//! (`IfE` for float, `I6__halfE` for half). That dodges nvcc's
//! per-translation-unit hash mangling for `static` helpers.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/fill.cu:4
const CUDA_FILL_BLOCK_SIZE: c_int = 256;

/// Resolved kernel handles for the two template instantiations.
/// Filled in once per module-load by
/// [`crate::cuda_port::module::init_ported_kernels`].
#[derive(Default)]
pub struct FillKernels {
    pub fill_f32: CUfunction,
    pub fill_f16: CUfunction,
}

/// Resolve `fill_kernel<float>` in fill.ptx via substring match.
/// Needle set picks the Itanium `I f E` template-arg run.
pub fn mangled_fill_kernel_f32() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::fill_entries::ENTRIES,
        &[b"fill_kernel", b"IfE"],
    )
}

/// Resolve `fill_kernel<__half>` in fill.ptx.
/// `__half` mangles as `6__half` (Itanium length-prefixed name).
pub fn mangled_fill_kernel_f16() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::fill_entries::ENTRIES,
        &[b"fill_kernel", b"6__half"],
    )
}

/// Generic launcher — C++ code inlines the grid math at each call
/// site; we factor it out because the two dtype entry points share
/// identical launch geometry. `k` is the total element count.
///
/// ref: vendor/ggml-cuda/fill.cu:25 (num_blocks)
fn fill_cuda_launch<T: Copy>(
    func: CUfunction,
    dst: CUdeviceptr,
    k: i64,
    value: &T,
    stream: CUstream,
) -> CUresult {
    let num_blocks = ((k + CUDA_FILL_BLOCK_SIZE as i64 - 1) / CUDA_FILL_BLOCK_SIZE as i64) as u32;

    // fill_kernel(T * dst, const int64_t k, const T value) — 3 args.
    let dst_val = dst.0;
    let args: [*const c_void; 3] = [
        &dst_val as *const u64 as *const c_void,
        &k as *const i64 as *const c_void,
        value as *const T as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            func,
            num_blocks,
            1,
            1,
            CUDA_FILL_BLOCK_SIZE as u32,
            1,
            1,
            0, // shmem
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// ref: vendor/ggml-cuda/fill.cu:29
///
/// f32 entry. Value passed as an already-unpacked `f32` (the C++
/// path does `memcpy(&value, dst->op_params, sizeof(float))`).
pub fn ggml_cuda_op_fill_f32(
    kernels: &FillKernels,
    dst: CUdeviceptr,
    k: i64,
    value: f32,
    stream: CUstream,
) -> CUresult {
    fill_cuda_launch(kernels.fill_f32, dst, k, &value, stream)
}

/// ref: vendor/ggml-cuda/fill.cu:32
///
/// f16 entry. C++ does `ggml_cuda_cast<half>(value)` on the host;
/// the Rust caller passes the already-converted `half::f16` bit
/// pattern. Value is bit-packed into a `u16` at the ABI boundary
/// because the CUDA __half ABI is "pass as 16-bit integer".
pub fn ggml_cuda_op_fill_f16(
    kernels: &FillKernels,
    dst: CUdeviceptr,
    k: i64,
    value_bits: u16,
    stream: CUstream,
) -> CUresult {
    fill_cuda_launch(kernels.fill_f16, dst, k, &value_bits, stream)
}
