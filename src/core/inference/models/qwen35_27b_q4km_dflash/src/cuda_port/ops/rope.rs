//! Rust port of the rope-op dispatcher in
//! `vendor/ggml-cuda/rope.cu`.
//!
//! ref: vendor/ggml-cuda/rope.cu
//!
//! Rotary position embeddings. Upstream has four kernel families
//! — `rope_norm`, `rope_neox`, `rope_multi`, `rope_vision` — each
//! templated on `<forward, has_ff, T, D>`. The file compiles to
//! many PTX entries (~32 combinations).
//!
//! Scope of the current port: the two variants Qwen3.5 actually
//! calls through `ggml_rope_ext` and `ggml_rope_multi`:
//!
//!   • `rope_norm<forward=true, has_ff={false,true}, T=f32, D=f32>`
//!   • `rope_multi<forward=true, has_ff={false,true}, T=f32>`
//!
//! `rope_neox`, `rope_vision`, the reverse / back-prop path, and
//! f16 / bf16 dtype variants all live in rope.ptx but aren't wired
//! — porting each is ~10 lines of lookup + dispatch once a caller
//! needs them.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/rope.cuh:3
const CUDA_ROPE_BLOCK_SIZE: c_int = 256;

/// Mirrors upstream `struct rope_corr_dims` (rope.cu:6-8).
#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct RopeCorrDims {
    pub v: [f32; 2],
}

/// Mirrors upstream `struct mrope_sections` (rope.cu:11-13).
#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct MRopeSections {
    pub v: [i32; 4],
}

/// Resolved kernel handles.
#[derive(Default)]
pub struct RopeKernels {
    /// `rope_norm<true, false, float, float>`
    pub norm_f32_no_ff: CUfunction,
    /// `rope_norm<true, true, float, float>`
    pub norm_f32_ff: CUfunction,
    /// `rope_multi<true, false, float>`
    pub multi_f32_no_ff: CUfunction,
    /// `rope_multi<true, true, float>`
    pub multi_f32_ff: CUfunction,
}

/// Resolve `rope_norm<forward=true, has_ff, T=float, D=float>`.
///
/// Needles:
///   • `rope_norm` prefix — excludes neox/multi/vision.
///   • `Lb1E`            — forward=true (`bool` template arg).
///   • `Lb{0|1}E`        — has_ff.
///   • `ffEvPKff` anchor — last float,float dtype pair + `vPK` start
///                         of param list. Discriminates from f16/half
///                         instantiations that carry `6__half`.
pub fn mangled_rope_norm_f32(has_ff: bool) -> Result<&'static [u8], String> {
    // Template args mangle as `ILb<fwd>ELb<ff>Eff` (forward, has_ff,
    // T=f, D=f). Concatenated needle locks the exact combination.
    let combined: &[u8] = if has_ff {
        b"ILb1ELb1Eff"
    } else {
        b"ILb1ELb0Eff"
    };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::rope_entries::ENTRIES,
        // `9rope_norm` length prefix excludes `rope_norm_back` if it
        // ever shows up alongside.
        &[b"9rope_norm", combined],
    )
}

/// Resolve `rope_multi<forward=true, has_ff, T=float>`.
pub fn mangled_rope_multi_f32(has_ff: bool) -> Result<&'static [u8], String> {
    let combined: &[u8] = if has_ff {
        b"ILb1ELb1EfEv"
    } else {
        b"ILb1ELb0EfEv"
    };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::rope_entries::ENTRIES,
        &[b"10rope_multi", combined],
    )
}

/// Bundle of rope parameters — reduces the arg-count of the top-level entry.
#[derive(Copy, Clone, Debug)]
pub struct RopeNormArgs<'a> {
    pub ne00: c_int,
    pub ne01: c_int,
    pub ne02: c_int,
    /// Element-strides for src and dst.
    pub s01: c_int,
    pub s02: c_int,
    pub s03: c_int,
    pub s1: c_int,
    pub s2: c_int,
    pub s3: c_int,
    pub n_dims: c_int,
    /// `ggml_nrows(src0)` — the grid-x size.
    pub nr: c_int,
    /// Pointers passed as-is to the kernel.
    pub pos: CUdeviceptr,
    pub freq_factors: CUdeviceptr, // may be null → `has_ff` must be false
    pub row_indices: CUdeviceptr,  // may be null → set_rows_stride == 0
    pub set_rows_stride: c_int,
    /// Scalars.
    pub freq_scale: f32,
    pub freq_base: f32,
    pub ext_factor: f32,
    pub attn_factor: f32,
    pub corr_dims: RopeCorrDims,
    /// unused here, just to keep the struct generic if callers want to add
    /// things without breaking the args passing.
    pub _phantom: core::marker::PhantomData<&'a ()>,
}

/// ref: vendor/ggml-cuda/rope.cu:331-370
#[allow(clippy::too_many_arguments)]
pub fn rope_norm_f32_cuda(
    kernels: &RopeKernels,
    x: CUdeviceptr,
    dst: CUdeviceptr,
    args: &RopeNormArgs<'_>,
    stream: CUstream,
) -> CUresult {
    assert!(args.ne00 % 2 == 0, "rope: ne00 must be even");
    let has_ff = args.freq_factors.0 != 0;
    let func = if has_ff {
        kernels.norm_f32_ff
    } else {
        kernels.norm_f32_no_ff
    };

    // Grid / block: ref rope.cu:355-357.
    let block_dims = (1u32, CUDA_ROPE_BLOCK_SIZE as u32, 1u32);
    let n_blocks_x = (args.ne00 + 2 * CUDA_ROPE_BLOCK_SIZE - 1) / (2 * CUDA_ROPE_BLOCK_SIZE);
    let grid_dims = (args.nr as u32, n_blocks_x as u32, 1u32);

    // theta_scale = powf(freq_base, -2.0f / n_dims)
    let theta_scale = args.freq_base.powf(-2.0_f32 / args.n_dims as f32);

    // Kernel signature (rope.cu:43-64):
    //   rope_norm(const T * x, D * dst,
    //             int ne00, int ne01, int ne02,
    //             int s01, int s02, int s03,
    //             int s1,  int s2,  int s3,
    //             int n_dims,
    //             const int32_t * pos,
    //             float freq_scale, float ext_factor, float attn_factor,
    //             rope_corr_dims corr_dims,
    //             float theta_scale,
    //             const float * freq_factors,
    //             const int64_t * row_indices,
    //             int set_rows_stride)
    // = 2 ptrs + 9 int + ptr + 3 float + RopeCorrDims(2 float) + 1 float + 2 ptr + 1 int = 21 slots.
    let x_val = x.0;
    let dst_val = dst.0;
    let pos_val = args.pos.0;
    let ff_val = args.freq_factors.0;
    let ri_val = args.row_indices.0;

    let kargs: [*const c_void; 21] = [
        &x_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &args.ne00 as *const c_int as *const c_void,
        &args.ne01 as *const c_int as *const c_void,
        &args.ne02 as *const c_int as *const c_void,
        &args.s01 as *const c_int as *const c_void,
        &args.s02 as *const c_int as *const c_void,
        &args.s03 as *const c_int as *const c_void,
        &args.s1 as *const c_int as *const c_void,
        &args.s2 as *const c_int as *const c_void,
        &args.s3 as *const c_int as *const c_void,
        &args.n_dims as *const c_int as *const c_void,
        &pos_val as *const u64 as *const c_void,
        &args.freq_scale as *const f32 as *const c_void,
        &args.ext_factor as *const f32 as *const c_void,
        &args.attn_factor as *const f32 as *const c_void,
        &args.corr_dims as *const RopeCorrDims as *const c_void,
        &theta_scale as *const f32 as *const c_void,
        &ff_val as *const u64 as *const c_void,
        &ri_val as *const u64 as *const c_void,
        &args.set_rows_stride as *const c_int as *const c_void,
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
            0,
            stream,
            kargs.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// Bundle of M-RoPE parameters.
#[derive(Copy, Clone, Debug)]
pub struct RopeMultiArgs {
    pub ne00: c_int,
    pub ne01: c_int,
    pub ne02: c_int,
    pub s01: c_int,
    pub s02: c_int,
    pub s03: c_int,
    pub s1: c_int,
    pub s2: c_int,
    pub s3: c_int,
    pub n_dims: c_int,
    pub nr: c_int,
    pub pos: CUdeviceptr,
    pub freq_factors: CUdeviceptr,
    pub freq_scale: f32,
    pub freq_base: f32,
    pub ext_factor: f32,
    pub attn_factor: f32,
    pub corr_dims: RopeCorrDims,
    pub sections: MRopeSections,
    pub is_imrope: bool,
}

/// ref: vendor/ggml-cuda/rope.cu:414-453
#[allow(clippy::too_many_arguments)]
pub fn rope_multi_f32_cuda(
    kernels: &RopeKernels,
    x: CUdeviceptr,
    dst: CUdeviceptr,
    args: &RopeMultiArgs,
    stream: CUstream,
) -> CUresult {
    assert!(args.ne00 % 2 == 0, "rope_multi: ne00 must be even");
    let has_ff = args.freq_factors.0 != 0;
    let func = if has_ff {
        kernels.multi_f32_ff
    } else {
        kernels.multi_f32_no_ff
    };

    let block_dims = (1u32, CUDA_ROPE_BLOCK_SIZE as u32, 1u32);
    let n_blocks_x = (args.ne00 + 2 * CUDA_ROPE_BLOCK_SIZE - 1) / (2 * CUDA_ROPE_BLOCK_SIZE);
    let grid_dims = (args.nr as u32, n_blocks_x as u32, 1u32);

    let theta_scale = args.freq_base.powf(-2.0_f32 / args.n_dims as f32);

    // Kernel signature (rope.cu:182-203):
    //   rope_multi(const T * x, T * dst,
    //              int ne00, ne01, ne02,
    //              int s01, s02, s03,
    //              int s1, s2, s3,
    //              int n_dims,
    //              const int32_t * pos,
    //              float freq_scale, ext_factor, attn_factor,
    //              rope_corr_dims, float theta_scale,
    //              const float * freq_factors,
    //              mrope_sections sections, bool is_imrope)
    // = 20 slots.
    let x_val = x.0;
    let dst_val = dst.0;
    let pos_val = args.pos.0;
    let ff_val = args.freq_factors.0;
    let imrope_u8: u8 = args.is_imrope as u8;

    let kargs: [*const c_void; 21] = [
        &x_val as *const u64 as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &args.ne00 as *const c_int as *const c_void,
        &args.ne01 as *const c_int as *const c_void,
        &args.ne02 as *const c_int as *const c_void,
        &args.s01 as *const c_int as *const c_void,
        &args.s02 as *const c_int as *const c_void,
        &args.s03 as *const c_int as *const c_void,
        &args.s1 as *const c_int as *const c_void,
        &args.s2 as *const c_int as *const c_void,
        &args.s3 as *const c_int as *const c_void,
        &args.n_dims as *const c_int as *const c_void,
        &pos_val as *const u64 as *const c_void,
        &args.freq_scale as *const f32 as *const c_void,
        &args.ext_factor as *const f32 as *const c_void,
        &args.attn_factor as *const f32 as *const c_void,
        &args.corr_dims as *const RopeCorrDims as *const c_void,
        &theta_scale as *const f32 as *const c_void,
        &ff_val as *const u64 as *const c_void,
        &args.sections as *const MRopeSections as *const c_void,
        &imrope_u8 as *const u8 as *const c_void,
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
            0,
            stream,
            kargs.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// Host-side port of `ggml_rope_yarn_corr_dims` — computes the two
/// extrapolation-ramp boundaries from `n_ctx_orig`, `freq_base`,
/// `beta_fast`, `beta_slow`.
///
/// ref: vendor/ggml-include/ggml.h (declaration); the actual formula
/// in lucebox's ggml.c matches:
///   max(0, floor(n_ctx_orig * log(base / (2πb)) / (2 log(base))))
#[inline]
pub fn ggml_rope_yarn_corr_dims(
    n_dims: c_int,
    n_ctx_orig: c_int,
    freq_base: f32,
    beta_fast: f32,
    beta_slow: f32,
) -> RopeCorrDims {
    fn corr(n_rot: f32, n_ctx_orig: f32, freq_base: f32) -> f32 {
        let inv = (n_ctx_orig as f32)
            * (freq_base.ln() - (2.0 * std::f32::consts::PI * n_rot).ln());
        let denom = 2.0 * freq_base.ln();
        0.0f32.max((inv / denom).floor())
    }
    let low = corr(beta_fast, n_ctx_orig as f32, freq_base);
    let high = corr(beta_slow, n_ctx_orig as f32, freq_base);
    RopeCorrDims {
        v: [low, (n_dims as f32 - 1.0).min(high)],
    }
}
