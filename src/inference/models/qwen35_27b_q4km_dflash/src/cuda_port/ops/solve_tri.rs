//! Rust port of the solve_tri-op dispatcher in
//! `vendor/ggml-cuda/solve_tri.cu`.
//!
//! ref: vendor/ggml-cuda/solve_tri.cu
//!
//! Triangular solve X = B · A^(-1) (right-side, upper-triangular,
//! non-unit-diagonal). Used by Qwen3.5's DeltaNet tree chunk update.
//!
//! Upstream has three paths:
//!   1. `solve_tri_f32_fast<n_template, k_template>` — warp-based
//!      parallel reduction, takes the fast path when `n ≤ 64` and
//!      `k ≤ 32`. Many template instantiations: `<0,0>` (fully
//!      runtime) plus ten `<64, K>` for K ∈ {32,16,14,12,10,8,6,4,2,1}.
//!   2. `get_batch_pointers` — helper kernel that scatters per-batch
//!      pointers into a device array for the cuBLAS fallback.
//!   3. `cublasStrsmBatched` — cuBLAS-backed fallback for larger n/k.
//!
//! Scope of the current port: **only the `<0,0>` general-case fast
//! kernel**. That covers every (n, k) ≤ (64, 32) case — the specialized
//! template instantiations give identical numerical output, just
//! better perf. Adding them later is ~40 lines of lookup + dispatch.
//!
//! The cuBLAS fallback is NOT wired because:
//!   • linking cuBLAS pulls in another runtime library (a no-go
//!     under the bare-metal-Rust rules)
//!   • Qwen3.5's DeltaNet uses chunk size 64 and state dim 32, so
//!     the fast path covers every runtime hit.

use std::ffi::c_void;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// Physical warp size on Ampere / Ada / Hopper.
const WARP_SIZE: u32 = 32;

#[derive(Default)]
pub struct SolveTriKernels {
    /// `solve_tri_f32_fast<0, 0>` — general-case fast kernel.
    pub fast_f32_general: CUfunction,
}

/// Resolve `solve_tri_f32_fast<0, 0>` in solve_tri.ptx.
/// Needles:
///   • `solve_tri_f32_fast` — kernel name anchor
///   • `Li0ELi0E`          — both template non-type args = 0 (Li0E twice)
pub fn mangled_solve_tri_f32_general() -> Result<&'static [u8], String> {
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::solve_tri_entries::ENTRIES,
        &[b"solve_tri_f32_fast", b"Li0ELi0E"],
    )
}

/// ref: vendor/ggml-cuda/solve_tri.cu:183-250 (the `n != 64` / default branch)
///
/// Host-side init of the `ne02` fastdiv cookie mirrors the upstream
/// helper (see `binbcast.rs::init_fastdiv_values`). For the generic
/// `<0, 0>` specialization we pass `n` and `k` as runtime args; the
/// template args go through as `Li0ELi0E` on the PTX side.
///
/// A (n × n, ne02, ne03) — lower-triangular factor, contiguous
/// (strides in elements).
/// B, X (n × k, ne02, ne03) — RHS / output, contiguous.
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_solve_tri_f32(
    kernels: &SolveTriKernels,
    a: CUdeviceptr,
    b: CUdeviceptr,
    x: CUdeviceptr,
    n: i32,
    k: i32,
    ne02: i64,
    ne03: i64,
    nb02: i64, // A stride along axis-2 in elements
    nb03: i64, // A stride along axis-3 in elements
    nb12: i64, // B stride along axis-2 in elements
    nb13: i64, // B stride along axis-3 in elements
    nb2: i64,  // X stride along axis-2 in elements
    nb3: i64,  // X stride along axis-3 in elements
    stream: CUstream,
) -> CUresult {
    // ref: solve_tri.cu:197-199 — block = (WARP_SIZE, k, 1), grid = (ne02*ne03,).
    let block_x = WARP_SIZE;
    let block_y = k as u32;
    let grid_x = (ne02 * ne03) as u32;

    // Pre-compute the uint3 fastdiv cookie for ne02 (upstream calls
    // `init_fastdiv_values((uint32_t)ne02)` at solve_tri.cu:197).
    let ne02_fd = crate::cuda_port::ops::binbcast_fastdiv(ne02 as u64);

    // Kernel signature (solve_tri.cu:91-103):
    //   solve_tri_f32_fast(
    //       const float *A, const float *B, float *X,
    //       uint3 ne02,
    //       size_t nb02, size_t nb03,
    //       size_t nb12, size_t nb13,
    //       size_t nb2,  size_t nb3,
    //       int n_arg, int k_arg)
    // = 3 ptrs + uint3 + 6 size_t + 2 int = 12 arg slots total.
    let a_val = a.0;
    let b_val = b.0;
    let x_val = x.0;
    let nb02_u = nb02 as u64;
    let nb03_u = nb03 as u64;
    let nb12_u = nb12 as u64;
    let nb13_u = nb13 as u64;
    let nb2_u = nb2 as u64;
    let nb3_u = nb3 as u64;

    let args: [*const c_void; 12] = [
        &a_val as *const u64 as *const c_void,
        &b_val as *const u64 as *const c_void,
        &x_val as *const u64 as *const c_void,
        &ne02_fd as *const [u32; 3] as *const c_void,
        &nb02_u as *const u64 as *const c_void,
        &nb03_u as *const u64 as *const c_void,
        &nb12_u as *const u64 as *const c_void,
        &nb13_u as *const u64 as *const c_void,
        &nb2_u as *const u64 as *const c_void,
        &nb3_u as *const u64 as *const c_void,
        &n as *const i32 as *const c_void,
        &k as *const i32 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            kernels.fast_f32_general,
            grid_x,
            1,
            1,
            block_x,
            block_y,
            1,
            0,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}
