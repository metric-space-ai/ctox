//! Rust port of the binbcast-op dispatcher in
//! `vendor/ggml-cuda/binbcast.cu`.
//!
//! ref: vendor/ggml-cuda/binbcast.cu
//!
//! This is the elementwise binary-op + broadcast kernel family —
//! the device side of `ggml_add`, `ggml_sub`, `ggml_mul`, `ggml_div`,
//! `ggml_repeat`. Upstream fuses broadcast and elementwise into one
//! kernel template (`k_bin_bcast`), parameterized by a `bin_op` functor
//! and the per-tensor (src0_t, src1_t, dst_t) dtype triple.
//!
//! Scope of the current port — the subset Qwen3.5's forward graph
//! actually exercises:
//!   • ops      : op_add, op_sub, op_mul  (no op_div, no op_repeat,
//!                 no fused add/mul)
//!   • dtypes   : f32 × f32 → f32 only
//!   • path     : `k_bin_bcast` (tiled). The `k_bin_bcast_unravel`
//!                 fallback (triggered when block_nums exceed 65535)
//!                 is not ported — the graph executor's inputs stay
//!                 well under that limit.
//!
//! All the other combinations are valid CUDA code in the vendored .cu
//! (and get compiled into binbcast.ptx), they're just not wired up on
//! the Rust side yet. Adding one is ~20 lines: resolve another
//! mangled entry, one more enum variant, one more `match`.
//!
//! # Mangled-name handling
//!
//! `k_bin_bcast` is templated on a function-pointer functor (`op_add`,
//! `op_sub`, `op_mul`). The functors are `static __device__`, so they
//! get per-TU hashes in their mangled names (`_INTERNAL_xxx_op_add...`).
//! We pin the needle set to:
//!   - `k_bin_bcast` prefix
//!   - `PKfPKfPf` : the dtype-triple parameter packing for f32
//!                  (`const float *`, `const float *`, `float *`)
//!   - functor needle using Itanium length-prefix encoding
//!     (`6op_addE` / `6op_subE` / `6op_mulE`) to disambiguate from
//!     any similarly-named functor.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/binbcast.cu:260 (block_size)
const BIN_BCAST_BLOCK_SIZE: u32 = 128;

/// Binary op discriminator.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
}

/// Resolved kernel handles — one per supported (op, dtype-triple)
/// combination. For now that's three entries (f32/f32/f32 × add,
/// sub, mul).
#[derive(Default)]
pub struct BinBcastKernels {
    pub add_fff: CUfunction,
    pub sub_fff: CUfunction,
    pub mul_fff: CUfunction,
}

impl BinBcastKernels {
    fn func(&self, op: BinOp) -> CUfunction {
        match op {
            BinOp::Add => self.add_fff,
            BinOp::Sub => self.sub_fff,
            BinOp::Mul => self.mul_fff,
        }
    }
}

/// Resolve a single `k_bin_bcast<op_X, float, float, float>` entry
/// in binbcast.ptx for the n_fuse=1 (default) case.
///
/// `bin_bcast_cuda` defaults `n_fuse` to 1 for op_add/sub/mul (only
/// op_repeat uses n_fuse=0). With n_fuse=1 the variadic pack holds
/// exactly one `const float *` — mangled as `JPKfEE`. The call site
/// still passes the non-pack `src1` pointer too (the kernel body
/// ignores it under `if constexpr (sizeof...(src1_ptrs) > 0)`), so
/// the ABI takes 23 pointer/scalar slots total.
///
/// Needle set:
///   • `11k_bin_bcast`  — Itanium length prefix, excludes unravel.
///   • `6op_addE` / `_subE` / `_mulE` — functor name.
///   • `EEfffJPKfEE`    — `EE` closes the functor template-arg (so
///                         the `fff` that follows can't be the tail
///                         of a wider dtype-string like
///                         `6__halfff`), then `fff` is the dtype
///                         triple (float,float,float), then the
///                         pack `JPKfE` (one const-float*) and
///                         close `E`. Rejects f16/mixed triples and
///                         fused n_fuse≥2 variants.
pub fn mangled_k_bin_bcast_fff(op: BinOp) -> Result<&'static [u8], String> {
    let op_needle: &[u8] = match op {
        BinOp::Add => b"6op_addE",
        BinOp::Sub => b"6op_subE",
        BinOp::Mul => b"6op_mulE",
    };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::binbcast_entries::ENTRIES,
        &[b"11k_bin_bcast", op_needle, b"EEfffJPKfEE"],
    )
}

/// Pack the upstream `init_fastdiv_values(d)` calculation.
/// Returns a `(mp, L, d)` uint3 cookie the device-side `fastdiv` /
/// `fastmodulo` intrinsic expects.
///
/// ref: vendor/ggml-cuda/common.cuh:865-880
#[inline]
fn init_fastdiv_values(d_64: u64) -> [u32; 3] {
    assert!(d_64 != 0, "fastdiv divisor must be non-zero");
    assert!(d_64 <= u32::MAX as u64, "fastdiv divisor out of range");
    let d = d_64 as u32;

    let mut l: u32 = 0;
    while l < 32 && (1u32 << l) < d {
        l += 1;
    }

    // mp = ((1 << 32) * ((1 << L) - d) / d + 1)
    let pow2_32: u64 = 1u64 << 32;
    let pow2_l: u64 = 1u64 << l;
    let mp: u32 = ((pow2_32 * (pow2_l - d as u64)) / d as u64 + 1) as u32;

    [mp, l, d]
}

/// Compute launch dims for the tiled `k_bin_bcast` path, matching
/// the upstream formula exactly.
///
/// ref: vendor/ggml-cuda/binbcast.cu:260-270
fn compute_block_dims(ne0: i64, ne1: i64, ne2: i64, ne3: i64) -> ([u32; 3], [u32; 3]) {
    let block_size = BIN_BCAST_BLOCK_SIZE;
    let hne0 = std::cmp::max(ne0 / 2, 1);

    let bx = std::cmp::min(hne0 as u32, block_size);
    let by = std::cmp::min(ne1 as u32, block_size / bx);
    let bz = std::cmp::min(
        std::cmp::min((ne2 * ne3) as u32, block_size / bx / by),
        64u32,
    );

    let gx = ((hne0 as u32 + bx - 1) / bx).max(1);
    let gy = ((ne1 as u32 + by - 1) / by).max(1);
    let gz = (((ne2 * ne3) as u32 + bz - 1) / bz).max(1);

    ([bx, by, bz], [gx, gy, gz])
}

/// Shape / stride bundle for a single src or dst. Strides are in
/// **elements of the respective dtype**, matching the kernel's
/// `sXX = nbXX / sizeof(dtype)` convention.
#[derive(Copy, Clone, Debug)]
pub struct BinBcastTensor {
    pub ne: [i64; 4],
    /// (s00, s01, s02, s03) — element strides.
    pub s: [i64; 4],
}

/// Launch `k_bin_bcast<op, float, float, float>` for the
/// f32×f32→f32 case.
///
/// Follows the upstream dispatcher's argument layout exactly. The
/// caller must have already run the "collapse" pass if it wants
/// the matching perf; passing uncollapsed strides is legal (the
/// kernel just doesn't make use of the contiguous-combine).
///
/// ref: vendor/ggml-cuda/binbcast.cu:300-315 (non-unravel branch)
#[allow(clippy::too_many_arguments)]
pub fn launch_bin_bcast_f32(
    kernels: &BinBcastKernels,
    op: BinOp,
    src0: CUdeviceptr,
    src1: CUdeviceptr,
    dst: CUdeviceptr,
    dst_shape: &BinBcastTensor,
    src0_shape: &BinBcastTensor,
    src1_shape: &BinBcastTensor,
    stream: CUstream,
) -> CUresult {
    let ne0 = dst_shape.ne[0];
    let ne1 = dst_shape.ne[1];
    let ne2 = dst_shape.ne[2];
    let ne3 = dst_shape.ne[3];

    let (block_dims, grid_dims) = compute_block_dims(ne0, ne1, ne2, ne3);

    // Sanity: the unravel fallback fires when block_nums.y or .z > 65535.
    // We don't support that path yet — caller should have picked a smaller
    // batch or we need to port k_bin_bcast_unravel.
    if grid_dims[1] > 65535 || grid_dims[2] > 65535 {
        // Bail with a recognisable CUresult — using ERROR_INVALID_VALUE (1).
        return 1;
    }

    // fastdiv cookie for ne3 (used inside the kernel to split the
    // flattened z-dim into (i2, i3)).
    let ne3_fd = init_fastdiv_values(ne3 as u64);
    // src1 dims — each used as a fastmodulo divisor.
    let ne10_fd = init_fastdiv_values(src1_shape.ne[0] as u64);
    let ne11_fd = init_fastdiv_values(src1_shape.ne[1] as u64);
    let ne12_fd = init_fastdiv_values(src1_shape.ne[2] as u64);
    let ne13_fd = init_fastdiv_values(src1_shape.ne[3] as u64);

    // Scalar-typed locals used for `&x as *const _` args — i32 where
    // the kernel takes `int`, i32 where it takes `const int`.
    let ne0_i = ne0 as c_int;
    let ne1_i = ne1 as c_int;
    let ne2_i = ne2 as c_int;
    // dst strides
    let s1 = dst_shape.s[1] as c_int;
    let s2 = dst_shape.s[2] as c_int;
    let s3 = dst_shape.s[3] as c_int;
    // src0 strides
    let s00 = src0_shape.s[0] as c_int;
    let s01 = src0_shape.s[1] as c_int;
    let s02 = src0_shape.s[2] as c_int;
    let s03 = src0_shape.s[3] as c_int;
    // src1 strides
    let s10 = src1_shape.s[0] as c_int;
    let s11 = src1_shape.s[1] as c_int;
    let s12 = src1_shape.s[2] as c_int;
    let s13 = src1_shape.s[3] as c_int;

    let src0_val = src0.0;
    let src1_val = src1.0;
    let dst_val = dst.0;

    // Kernel signature (see binbcast.cu:31-54):
    //   k_bin_bcast(src0, src1, dst,
    //               int ne0, int ne1, int ne2, uint3 ne3,
    //               uint3 ne10, uint3 ne11, uint3 ne12, uint3 ne13,
    //               int s1, int s2, int s3,
    //               int s00, int s01, int s02, int s03,
    //               int s10, int s11, int s12, int s13,
    //               src1_ptrs...)
    // For n_fuse=1 the pack expands to one `const float *`. Total =
    // 3 ptrs + 3 ints + 5 uint3 + 11 ints + 1 pack-ptr = 23 args.
    //
    // The non-pack `src1` pointer is still passed — the kernel body
    // ignores it under `if constexpr (sizeof...(src1_ptrs) > 0)`,
    // but the ABI slot is present.
    let args: [*const c_void; 23] = [
        &src0_val as *const u64 as *const c_void,
        &src1_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &ne0_i as *const c_int as *const c_void,
        &ne1_i as *const c_int as *const c_void,
        &ne2_i as *const c_int as *const c_void,
        &ne3_fd as *const [u32; 3] as *const c_void,
        &ne10_fd as *const [u32; 3] as *const c_void,
        &ne11_fd as *const [u32; 3] as *const c_void,
        &ne12_fd as *const [u32; 3] as *const c_void,
        &ne13_fd as *const [u32; 3] as *const c_void,
        &s1 as *const c_int as *const c_void,
        &s2 as *const c_int as *const c_void,
        &s3 as *const c_int as *const c_void,
        &s00 as *const c_int as *const c_void,
        &s01 as *const c_int as *const c_void,
        &s02 as *const c_int as *const c_void,
        &s03 as *const c_int as *const c_void,
        &s10 as *const c_int as *const c_void,
        &s11 as *const c_int as *const c_void,
        &s12 as *const c_int as *const c_void,
        &s13 as *const c_int as *const c_void,
        // Pack element 0: const float * — upstream passes
        // `dst->src[1]->data`, which for the non-fused ggml_cuda_op_add
        // is the same buffer as src1_dd.
        &src1_val as *const u64 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            kernels.func(op),
            grid_dims[0],
            grid_dims[1],
            grid_dims[2],
            block_dims[0],
            block_dims[1],
            block_dims[2],
            0, // shmem
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// ref: vendor/ggml-cuda/binbcast.cu:397-407
///
/// Thin wrappers that fix the op — mirror the upstream entry points
/// (`ggml_cuda_op_add`, `_sub`, `_mul`) but take the already-unpacked
/// shape/stride bundles instead of ggml_tensor pointers.
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_add_f32(
    kernels: &BinBcastKernels,
    src0: CUdeviceptr,
    src1: CUdeviceptr,
    dst: CUdeviceptr,
    dst_shape: &BinBcastTensor,
    src0_shape: &BinBcastTensor,
    src1_shape: &BinBcastTensor,
    stream: CUstream,
) -> CUresult {
    launch_bin_bcast_f32(
        kernels,
        BinOp::Add,
        src0,
        src1,
        dst,
        dst_shape,
        src0_shape,
        src1_shape,
        stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_sub_f32(
    kernels: &BinBcastKernels,
    src0: CUdeviceptr,
    src1: CUdeviceptr,
    dst: CUdeviceptr,
    dst_shape: &BinBcastTensor,
    src0_shape: &BinBcastTensor,
    src1_shape: &BinBcastTensor,
    stream: CUstream,
) -> CUresult {
    launch_bin_bcast_f32(
        kernels,
        BinOp::Sub,
        src0,
        src1,
        dst,
        dst_shape,
        src0_shape,
        src1_shape,
        stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_mul_f32(
    kernels: &BinBcastKernels,
    src0: CUdeviceptr,
    src1: CUdeviceptr,
    dst: CUdeviceptr,
    dst_shape: &BinBcastTensor,
    src0_shape: &BinBcastTensor,
    src1_shape: &BinBcastTensor,
    stream: CUstream,
) -> CUresult {
    launch_bin_bcast_f32(
        kernels,
        BinOp::Mul,
        src0,
        src1,
        dst,
        dst_shape,
        src0_shape,
        src1_shape,
        stream,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values from a direct CPU run of
    /// `init_fastdiv_values` in the vendored common.cuh: d=256 →
    /// (mp, L, d) = (0, 8, 256).
    #[test]
    fn fastdiv_power_of_two() {
        assert_eq!(init_fastdiv_values(256), [0, 8, 256]);
        assert_eq!(init_fastdiv_values(1), [0, 0, 1]);
        assert_eq!(init_fastdiv_values(2), [0, 1, 2]);
    }

    #[test]
    fn fastdiv_small_prime() {
        // d=7 → L=3, mp = (2^32 * (8 - 7) / 7 + 1) = 613566757
        let v = init_fastdiv_values(7);
        assert_eq!(v[1], 3);
        assert_eq!(v[2], 7);
        // Spot-check the upstream formula on the host.
        let pow2_32: u64 = 1u64 << 32;
        let expected_mp = ((pow2_32 * 1) / 7 + 1) as u32;
        assert_eq!(v[0], expected_mp);
    }
}
