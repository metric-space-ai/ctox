//! Rust port of the unary-op host-side dispatchers in
//! `vendor/ggml-cuda/unary.cu`.
//!
//! ref: vendor/ggml-cuda/unary.cu
//!
//! The kernel template `unary_op_kernel<op, T>` at line 114-123
//! stays in the vendored .cu file, compiled by `build.rs` to
//! `unary.ptx`. We port the host-side launcher `unary_cuda` (~line
//! 124) and the per-op entry points that delegate to it
//! (`ggml_cuda_op_silu`, `…_neg`, `…_exp`, plus the generic
//! `ggml_cuda_op_unary<op>` they all route through).
//!
//! For qwen35 the only three variants we actually exercise are
//! `op_silu`, `op_neg`, `op_exp` — all on f32 tensors. Other
//! unary ops (gelu, tanh, sigmoid, …) are not ported because
//! qwen35's forward graph never calls them.
//!
//! # Mangled-name handling
//!
//! `op_silu` / `op_neg` / `op_exp` are `static __device__` functors
//! in `unary.cu`, so nvcc gives their mangled names a per-translation
//! -unit hash (e.g. `_INTERNAL_9608ce77_8_unary_cu_eb6d5366`). That
//! hash changes between builds. Instead of hard-coding the full
//! mangled name we scan the compiled PTX's `.entry` list for a
//! unique match on two stable substrings: the template's `unary_op_kernel`
//! prefix + the functor identifier (`op_silu` / `op_neg` / `op_exp`) +
//! the `T=float` discriminator. See
//! [`mangled_unary_op_f32`](mangled_unary_op_f32).

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/unary.cuh:4
const CUDA_NEG_BLOCK_SIZE: c_int = 256;

/// Resolved kernel handles for the unary-op variants qwen35 uses.
/// One `CUfunction` per `(op, dtype)` pair. Every handle starts
/// null-valued and is filled in by
/// [`crate::cuda_port::module::init_ported_kernels`].
#[derive(Default)]
pub struct UnaryKernels {
    pub silu_f32: CUfunction,
    pub neg_f32: CUfunction,
    pub exp_f32: CUfunction,
}

impl Default for CUfunction {
    fn default() -> Self {
        CUfunction(std::ptr::null_mut())
    }
}

/// Resolve a `unary_op_kernel<op_<name>, float>` PTX entry by
/// scanning `unary.ptx`'s `.entry` list for the unique match on
/// the functor name needle (e.g. `b"op_silu"`). The kernel's T is
/// pinned to `float` (which demangles as `f` in the suffix
/// `EfEvP…`), so we also require `b"EfEvPK"` — present in the
/// float-instantiation but absent in the `__half` twin.
pub fn mangled_unary_op_f32(op_needle: &[u8]) -> Result<Vec<u8>, String> {
    let e = crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::unary_entries::ENTRIES,
        &[b"unary_op_kernel", op_needle, b"EfEvPK"],
    )?;
    // `find_entry` returns a NUL-terminated slice; hand it back as owned.
    Ok(e.to_vec())
}

/// ref: vendor/ggml-cuda/unary.cu:124-128
///
/// Generic launcher the per-op dispatchers delegate to. `k` is the
/// total element count (`ggml_nelements(src0)`), grid is sized for
/// `CUDA_NEG_BLOCK_SIZE` threads per block, 1-D.
pub fn unary_cuda_f32(
    func: CUfunction,
    x: CUdeviceptr,
    dst: CUdeviceptr,
    k: c_int,
    stream: CUstream,
) -> CUresult {
    // ref: unary.cu:126
    let num_blocks = ((k + CUDA_NEG_BLOCK_SIZE - 1) / CUDA_NEG_BLOCK_SIZE) as u32;

    // unary_op_kernel(const T *x, T *dst, const int k) — 3 args.
    let x_val = x.0;
    let dst_val = dst.0;
    let args: [*const c_void; 3] = [
        &x_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &k as *const c_int as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            func,
            num_blocks,
            1,
            1,
            CUDA_NEG_BLOCK_SIZE as u32,
            1,
            1,
            0, // shmem
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

// ─── Per-op entry points ──────────────────────────────────────────
//
// Each mirrors the C++ dispatcher that specializes
// `ggml_cuda_op_unary<op>` for the given op functor.

/// ref: vendor/ggml-cuda/unary.cu:177-179
pub fn ggml_cuda_op_silu_f32(
    kernels: &UnaryKernels,
    src0: CUdeviceptr,
    dst: CUdeviceptr,
    k: c_int,
    stream: CUstream,
) -> CUresult {
    unary_cuda_f32(kernels.silu_f32, src0, dst, k, stream)
}

/// ref: vendor/ggml-cuda/unary.cu:157-159
pub fn ggml_cuda_op_neg_f32(
    kernels: &UnaryKernels,
    src0: CUdeviceptr,
    dst: CUdeviceptr,
    k: c_int,
    stream: CUstream,
) -> CUresult {
    unary_cuda_f32(kernels.neg_f32, src0, dst, k, stream)
}

/// ref: vendor/ggml-cuda/unary.cu:201-203
pub fn ggml_cuda_op_exp_f32(
    kernels: &UnaryKernels,
    src0: CUdeviceptr,
    dst: CUdeviceptr,
    k: c_int,
    stream: CUstream,
) -> CUresult {
    unary_cuda_f32(kernels.exp_f32, src0, dst, k, stream)
}
