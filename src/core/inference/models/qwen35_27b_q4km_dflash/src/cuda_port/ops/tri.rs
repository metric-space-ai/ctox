//! Rust port of the tri-op dispatcher in
//! `vendor/ggml-cuda/tri.cu`.
//!
//! ref: vendor/ggml-cuda/tri.cu
//!
//! Keeps (or zeroes) the upper or lower triangle of a 2-D matrix,
//! with or without the diagonal. Four variants selected by
//! `ggml_tri_type`:
//!
//! | type        | prefix_keep | add_to_split |
//! |-------------|-------------|--------------|
//! | UPPER_DIAG  | false       | 0            |
//! | UPPER       | false       | 1            |
//! | LOWER_DIAG  | true        | 1            |
//! | LOWER       | true        | 0            |
//!
//! Qwen3.5's forward uses LOWER_DIAG for the causal-attention-mask
//! construction and UPPER for zero-out of the DeltaNet's H-state
//! persistent triangle. We port all four variants for completeness —
//! the kernel specializations are all in tri.ptx anyway.
//!
//! # Mangled-name handling
//!
//! `tri_kernel<T, prefix_keep, add_to_split>` — three non-type
//! template args. nvcc emits them as
//! `<T>Lb<0|1>ELi<0|1>E…`. The bool `prefix_keep` mangles as
//! `Lb0E` / `Lb1E` and the int `add_to_split` as `Li0E` / `Li1E`.
//! We resolve at runtime by substring-AND on the stable chunks.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/tri.cuh:3
const CUDA_TRI_BLOCK_SIZE: c_int = 256;

/// Mirrors upstream `enum ggml_tri_type` (ggml.h:645-650).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TriType {
    UpperDiag = 0,
    Upper = 1,
    LowerDiag = 2,
    Lower = 3,
}

impl TriType {
    /// (prefix_keep, add_to_split) per the switch in tri.cu:56-57.
    /// Exposed for callers that want to replicate the mapping.
    pub fn params(self) -> (bool, i32) {
        match self {
            TriType::UpperDiag => (false, 0),
            TriType::Upper => (false, 1),
            TriType::LowerDiag => (true, 1),
            TriType::Lower => (true, 0),
        }
    }
}

/// Resolved kernel handles — four (prefix_keep, add_to_split)
/// combinations per dtype. Qwen3.5 only needs f32, so we skip
/// f16 / bf16 instantiations (they're present in tri.ptx,
/// unwired until a caller needs them).
#[derive(Default)]
pub struct TriKernels {
    pub f32_keep_split0: CUfunction, // prefix_keep=true, add=0 (LOWER)
    pub f32_keep_split1: CUfunction, // prefix_keep=true, add=1 (LOWER_DIAG)
    pub f32_zero_split0: CUfunction, // prefix_keep=false, add=0 (UPPER_DIAG)
    pub f32_zero_split1: CUfunction, // prefix_keep=false, add=1 (UPPER)
}

impl TriKernels {
    fn func(&self, t: TriType) -> CUfunction {
        match t {
            TriType::Lower => self.f32_keep_split0,
            TriType::LowerDiag => self.f32_keep_split1,
            TriType::UpperDiag => self.f32_zero_split0,
            TriType::Upper => self.f32_zero_split1,
        }
    }
}

/// Resolve `tri_kernel<float, prefix_keep, add_to_split>` in tri.ptx.
/// `prefix_keep ∈ {true, false}` maps to Itanium `Lb1E` / `Lb0E`;
/// `add_to_split ∈ {0, 1}` maps to `Li0E` / `Li1E`.
pub fn mangled_tri_kernel_f32(prefix_keep: bool, add_to_split: i32) -> Result<&'static [u8], String> {
    let bn: &[u8] = if prefix_keep { b"Lb1E" } else { b"Lb0E" };
    let add: &[u8] = match add_to_split {
        0 => b"Li0E",
        1 => b"Li1E",
        _ => return Err(format!("add_to_split must be 0 or 1, got {add_to_split}")),
    };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::tri_entries::ENTRIES,
        // `tri_kernel` prefix + `IfLb` (template args start with
        // T=float, then Lb for the bool) pins us to the f32
        // instantiation; the two non-type-arg needles disambiguate
        // the four variants.
        &[b"tri_kernel", b"IfLb", bn, add],
    )
}

/// Generic launcher for a resolved kernel. All strides are in
/// elements (upstream divides by `sizeof(T)` before handing them
/// down — we do the same on the caller side).
#[allow(clippy::too_many_arguments)]
fn tri_cuda_launch(
    func: CUfunction,
    src: CUdeviceptr,
    dst: CUdeviceptr,
    ne: [i64; 4],
    nb_src: [i64; 4],
    nb_dst: [i64; 4],
    stream: CUstream,
) -> CUresult {
    // Grid layout matches tri.cu:52-53 — one block per (i1, i2, i3)
    // tile, 1-D block of 256 threads iterating over i0.
    let grid_x = ne[1] as u32;
    let grid_y = ne[2] as u32;
    let grid_z = ne[3] as u32;

    // 14 args — 2 ptrs + 4 ne + 4 nb_src + 4 nb_dst.
    let src_val = src.0;
    let dst_val = dst.0;
    let args: [*const c_void; 14] = [
        &src_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &ne[0] as *const i64 as *const c_void,
        &ne[1] as *const i64 as *const c_void,
        &ne[2] as *const i64 as *const c_void,
        &ne[3] as *const i64 as *const c_void,
        &nb_src[0] as *const i64 as *const c_void,
        &nb_src[1] as *const i64 as *const c_void,
        &nb_src[2] as *const i64 as *const c_void,
        &nb_src[3] as *const i64 as *const c_void,
        &nb_dst[0] as *const i64 as *const c_void,
        &nb_dst[1] as *const i64 as *const c_void,
        &nb_dst[2] as *const i64 as *const c_void,
        &nb_dst[3] as *const i64 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            func,
            grid_x,
            grid_y,
            grid_z,
            CUDA_TRI_BLOCK_SIZE as u32,
            1,
            1,
            0,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// ref: vendor/ggml-cuda/tri.cu:94-112
///
/// f32 entry point. Strides (`nb_*`) are in **elements**, matching
/// the kernel's arg convention after upstream's `/ sizeof(T)`.
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_tri_f32(
    kernels: &TriKernels,
    src: CUdeviceptr,
    dst: CUdeviceptr,
    ne: [i64; 4],
    nb_src_elems: [i64; 4],
    nb_dst_elems: [i64; 4],
    ttype: TriType,
    stream: CUstream,
) -> CUresult {
    tri_cuda_launch(
        kernels.func(ttype),
        src,
        dst,
        ne,
        nb_src_elems,
        nb_dst_elems,
        stream,
    )
}
