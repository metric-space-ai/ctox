//! Runtime module registry for the cuda_port bare-metal dispatcher.
//!
//! A CUDA context lives once per process (owned by ggml_backend_cuda
//! during the transition, later by a direct Rust context). Every
//! bare-metal op dispatcher needs the matching PTX module loaded
//! into that context, and each kernel's mangled name resolved to
//! a `CUfunction` handle. That's what this module holds.
//!
//! We use a process-wide `OnceLock<PortedKernels>` — lazy-initialized
//! on the first [`porter`] call. Currently only the norm PTX is
//! wired; subsequent op ports add one field each to `PortedKernels`
//! and one load + lookup in [`init_ported_kernels`].

use std::sync::OnceLock;

use super::driver::CUmodule;
use super::ops::binbcast::{mangled_k_bin_bcast_fff, BinBcastKernels, BinOp};
use super::ops::diag::{mangled_diag_kernel_f16, mangled_diag_kernel_f32, DiagKernels};
use super::ops::fill::{mangled_fill_kernel_f16, mangled_fill_kernel_f32, FillKernels};
use super::ops::norm::{
    mangled_rms_norm_f32_b1024, mangled_rms_norm_f32_b256, RmsNormKernels,
};
use super::ops::scale::{mangled_scale_f32, ScaleKernel};
use super::ops::concat::{mangled_concat_dim, mangled_concat_non_cont, ConcatKernels};
use super::ops::cpy::{mangled_cpy_scalar, CpyDtype, CpyKernels};
use super::ops::cumsum::{mangled_cumsum_kernel_f32, CumsumKernels};
use super::ops::rope::{mangled_rope_multi_f32, mangled_rope_norm_f32, RopeKernels};
use super::ops::solve_tri::{mangled_solve_tri_f32_general, SolveTriKernels};
use super::ops::pad::{mangled_pad_f32, PadKernels};
use super::ops::tri::{mangled_tri_kernel_f32, TriKernels};
use super::ops::unary::{mangled_unary_op_f32, UnaryKernels};
use super::ptx::{
    get_function, load_module, BINBCAST_PTX, CONCAT_PTX, CPY_PTX, CUMSUM_PTX, DIAG_PTX, FILL_PTX,
    NORM_PTX, PAD_PTX, ROPE_PTX, SCALE_PTX, SOLVE_TRI_PTX, TRI_PTX, UNARY_PTX,
};

/// All kernel handles the Rust side needs, resolved once.
pub struct PortedKernels {
    /// Kept alive so the module doesn't unload.
    #[allow(dead_code)]
    norm_module: CUmodule,
    #[allow(dead_code)]
    unary_module: CUmodule,
    #[allow(dead_code)]
    scale_module: CUmodule,
    #[allow(dead_code)]
    fill_module: CUmodule,
    #[allow(dead_code)]
    diag_module: CUmodule,
    #[allow(dead_code)]
    binbcast_module: CUmodule,
    #[allow(dead_code)]
    tri_module: CUmodule,
    #[allow(dead_code)]
    pad_module: CUmodule,
    #[allow(dead_code)]
    cumsum_module: CUmodule,
    #[allow(dead_code)]
    concat_module: CUmodule,
    #[allow(dead_code)]
    cpy_module: CUmodule,
    #[allow(dead_code)]
    solve_tri_module: CUmodule,
    #[allow(dead_code)]
    rope_module: CUmodule,
    pub rms_norm: RmsNormKernels,
    pub unary: UnaryKernels,
    pub scale: ScaleKernel,
    pub fill: FillKernels,
    pub diag: DiagKernels,
    pub binbcast: BinBcastKernels,
    pub tri: TriKernels,
    pub pad: PadKernels,
    pub cumsum: CumsumKernels,
    pub concat: ConcatKernels,
    pub cpy: CpyKernels,
    pub solve_tri: SolveTriKernels,
    pub rope: RopeKernels,
}

// SAFETY: `CUmodule` / `CUfunction` are opaque device-side handles.
// They're safe to share across threads — CUDA's own context
// management serializes access to the underlying driver. We treat
// them as `Send + Sync` to store in a `OnceLock`.
unsafe impl Send for PortedKernels {}
unsafe impl Sync for PortedKernels {}

static PORTED: OnceLock<Result<PortedKernels, String>> = OnceLock::new();

/// Lazy-init + access. First call loads every ported PTX module and
/// resolves the kernel handles. Subsequent calls return the cached
/// handles. Errors are also cached so the failure path is explicit
/// and doesn't retry silently.
pub fn porter() -> Result<&'static PortedKernels, &'static str> {
    match PORTED.get_or_init(init_ported_kernels) {
        Ok(p) => Ok(p),
        Err(e) => Err(e.as_str()),
    }
}

fn init_ported_kernels() -> Result<PortedKernels, String> {
    // norm.cu — rms_norm_f32<256/1024, false, false>
    let norm_module = load_module(NORM_PTX).map_err(|e| format!("norm.ptx: {e}"))?;
    let b256 = get_function(
        norm_module,
        &mangled_rms_norm_f32_b256().map_err(|e| format!("rms_norm<256> lookup: {e}"))?,
    )
    .map_err(|e| format!("rms_norm<256>: {e}"))?;
    let b1024 = get_function(
        norm_module,
        &mangled_rms_norm_f32_b1024().map_err(|e| format!("rms_norm<1024> lookup: {e}"))?,
    )
    .map_err(|e| format!("rms_norm<1024>: {e}"))?;

    // unary.cu — unary_op_kernel<op_silu|op_neg|op_exp, float>.
    //
    // Itanium mangles the nested-name `op_<X>` as `<len><name>E`
    // (length byte, identifier, closing E). Strict needles include
    // both so `op_exp` doesn't collide with `op_expm1`, etc.
    let unary_module = load_module(UNARY_PTX).map_err(|e| format!("unary.ptx: {e}"))?;
    let mut uk = UnaryKernels::default();
    for (slot, needle) in &[
        (&mut uk.silu_f32 as *mut _, b"7op_siluE".as_slice()),
        (&mut uk.neg_f32 as *mut _, b"6op_negE".as_slice()),
        (&mut uk.exp_f32 as *mut _, b"6op_expE".as_slice()),
        (&mut uk.sigmoid_f32 as *mut _, b"10op_sigmoidE".as_slice()),
        (&mut uk.softplus_f32 as *mut _, b"11op_softplusE".as_slice()),
    ] {
        let name = mangled_unary_op_f32(needle).map_err(|e| format!("unary lookup: {e}"))?;
        let f = get_function(unary_module, &name).map_err(|e| format!("unary: {e}"))?;
        // SAFETY: all slot pointers point into the stack-local `uk`.
        unsafe { **slot = f };
    }

    // scale.cu — scale_f32
    let scale_module = load_module(SCALE_PTX).map_err(|e| format!("scale.ptx: {e}"))?;
    let scale_fn = get_function(
        scale_module,
        mangled_scale_f32().map_err(|e| format!("scale_f32 lookup: {e}"))?,
    )
    .map_err(|e| format!("scale_f32: {e}"))?;
    let scale = ScaleKernel { scale_f32: scale_fn };

    // fill.cu — fill_kernel<float> + fill_kernel<__half>
    let fill_module = load_module(FILL_PTX).map_err(|e| format!("fill.ptx: {e}"))?;
    let fill_f32 = get_function(
        fill_module,
        mangled_fill_kernel_f32().map_err(|e| format!("fill<float> lookup: {e}"))?,
    )
    .map_err(|e| format!("fill<float>: {e}"))?;
    let fill_f16 = get_function(
        fill_module,
        mangled_fill_kernel_f16().map_err(|e| format!("fill<__half> lookup: {e}"))?,
    )
    .map_err(|e| format!("fill<__half>: {e}"))?;
    let fill = FillKernels { fill_f32, fill_f16 };

    // diag.cu — diag_kernel<float> + diag_kernel<__half>
    let diag_module = load_module(DIAG_PTX).map_err(|e| format!("diag.ptx: {e}"))?;
    let diag_f32 = get_function(
        diag_module,
        mangled_diag_kernel_f32().map_err(|e| format!("diag<float> lookup: {e}"))?,
    )
    .map_err(|e| format!("diag<float>: {e}"))?;
    let diag_f16 = get_function(
        diag_module,
        mangled_diag_kernel_f16().map_err(|e| format!("diag<__half> lookup: {e}"))?,
    )
    .map_err(|e| format!("diag<__half>: {e}"))?;
    let diag = DiagKernels { diag_f32, diag_f16 };

    // binbcast.cu — k_bin_bcast<op_*, float, float, float>
    //   • add/sub/mul use n_fuse=1 (pack has one element)
    //   • repeat    uses n_fuse=0 (empty pack)
    let binbcast_module =
        load_module(BINBCAST_PTX).map_err(|e| format!("binbcast.ptx: {e}"))?;
    let mut bb = BinBcastKernels::default();
    for (slot, op) in [
        (&mut bb.add_fff as *mut _, BinOp::Add),
        (&mut bb.sub_fff as *mut _, BinOp::Sub),
        (&mut bb.mul_fff as *mut _, BinOp::Mul),
        (&mut bb.repeat_fff as *mut _, BinOp::Repeat),
    ] {
        let name = mangled_k_bin_bcast_fff(op)
            .map_err(|e| format!("binbcast {op:?} lookup: {e}"))?;
        let f = get_function(binbcast_module, name)
            .map_err(|e| format!("binbcast {op:?}: {e}"))?;
        // SAFETY: each slot points at a CUfunction field in the
        // stack-local `bb` we keep alive until the final Ok(...).
        unsafe { *slot = f };
    }

    // tri.cu — tri_kernel<float, prefix_keep, add_to_split>
    let tri_module = load_module(TRI_PTX).map_err(|e| format!("tri.ptx: {e}"))?;
    let mut tk = TriKernels::default();
    for (slot, (prefix_keep, add)) in [
        (&mut tk.f32_keep_split0 as *mut _, (true, 0i32)),  // LOWER
        (&mut tk.f32_keep_split1 as *mut _, (true, 1i32)),  // LOWER_DIAG
        (&mut tk.f32_zero_split0 as *mut _, (false, 0i32)), // UPPER_DIAG
        (&mut tk.f32_zero_split1 as *mut _, (false, 1i32)), // UPPER
    ] {
        let name = mangled_tri_kernel_f32(prefix_keep, add)
            .map_err(|e| format!("tri<f32,{prefix_keep},{add}> lookup: {e}"))?;
        let f = get_function(tri_module, name)
            .map_err(|e| format!("tri<f32,{prefix_keep},{add}>: {e}"))?;
        // SAFETY: slot points into the stack-local `tk`.
        unsafe { *slot = f };
    }

    // pad.cu — pad_f32 (single kernel, f32 only).
    let pad_module = load_module(PAD_PTX).map_err(|e| format!("pad.ptx: {e}"))?;
    let pad_fn = get_function(
        pad_module,
        mangled_pad_f32().map_err(|e| format!("pad_f32 lookup: {e}"))?,
    )
    .map_err(|e| format!("pad_f32: {e}"))?;
    let pad = PadKernels { pad_f32: pad_fn };

    // cumsum.cu — cumsum_kernel<float> (non-CUB fallback).
    let cumsum_module =
        load_module(CUMSUM_PTX).map_err(|e| format!("cumsum.ptx: {e}"))?;
    let cumsum_fn = get_function(
        cumsum_module,
        mangled_cumsum_kernel_f32().map_err(|e| format!("cumsum<float> lookup: {e}"))?,
    )
    .map_err(|e| format!("cumsum<float>: {e}"))?;
    let cumsum = CumsumKernels { cumsum_f32: cumsum_fn };

    // concat.cu — 3 contiguous dim kernels + 4 non-cont template instantiations
    let concat_module =
        load_module(CONCAT_PTX).map_err(|e| format!("concat.ptx: {e}"))?;
    let mut ck = ConcatKernels::default();
    for (slot, dim) in [
        (&mut ck.dim0 as *mut _, 0i32),
        (&mut ck.dim1 as *mut _, 1i32),
        (&mut ck.dim2 as *mut _, 2i32),
    ] {
        let name = mangled_concat_dim(dim)
            .map_err(|e| format!("concat_dim{dim} lookup: {e}"))?;
        let f = get_function(concat_module, name)
            .map_err(|e| format!("concat_dim{dim}: {e}"))?;
        unsafe { *slot = f };
    }
    for d in 0..4i32 {
        let name = mangled_concat_non_cont(d)
            .map_err(|e| format!("concat_non_cont<{d}> lookup: {e}"))?;
        let f = get_function(concat_module, name)
            .map_err(|e| format!("concat_non_cont<{d}>: {e}"))?;
        ck.non_cont[d as usize] = f;
    }

    // cpy.cu — 3 dtype-pair kernels (f32→f32, f32→f16, f16→f16)
    let cpy_module = load_module(CPY_PTX).map_err(|e| format!("cpy.ptx: {e}"))?;
    let mut cp = CpyKernels::default();
    for (slot, dtype) in [
        (&mut cp.f32_f32 as *mut _, CpyDtype::F32ToF32),
        (&mut cp.f32_f16 as *mut _, CpyDtype::F32ToF16),
        (&mut cp.f16_f16 as *mut _, CpyDtype::F16ToF16),
    ] {
        let name = mangled_cpy_scalar(dtype)
            .map_err(|e| format!("cpy {dtype:?} lookup: {e}"))?;
        let f = get_function(cpy_module, name)
            .map_err(|e| format!("cpy {dtype:?}: {e}"))?;
        unsafe { *slot = f };
    }

    // solve_tri.cu — `solve_tri_f32_fast<0, 0>` general-case only.
    let solve_tri_module =
        load_module(SOLVE_TRI_PTX).map_err(|e| format!("solve_tri.ptx: {e}"))?;
    let solve_tri_fn = get_function(
        solve_tri_module,
        mangled_solve_tri_f32_general()
            .map_err(|e| format!("solve_tri<0,0> lookup: {e}"))?,
    )
    .map_err(|e| format!("solve_tri<0,0>: {e}"))?;
    let solve_tri = SolveTriKernels {
        fast_f32_general: solve_tri_fn,
    };

    // rope.cu — rope_norm + rope_multi, f32, 2 has_ff variants each.
    let rope_module = load_module(ROPE_PTX).map_err(|e| format!("rope.ptx: {e}"))?;
    let mut rope = RopeKernels::default();
    rope.norm_f32_no_ff = get_function(
        rope_module,
        mangled_rope_norm_f32(false).map_err(|e| format!("rope_norm no_ff lookup: {e}"))?,
    )
    .map_err(|e| format!("rope_norm no_ff: {e}"))?;
    rope.norm_f32_ff = get_function(
        rope_module,
        mangled_rope_norm_f32(true).map_err(|e| format!("rope_norm ff lookup: {e}"))?,
    )
    .map_err(|e| format!("rope_norm ff: {e}"))?;
    rope.multi_f32_no_ff = get_function(
        rope_module,
        mangled_rope_multi_f32(false).map_err(|e| format!("rope_multi no_ff lookup: {e}"))?,
    )
    .map_err(|e| format!("rope_multi no_ff: {e}"))?;
    rope.multi_f32_ff = get_function(
        rope_module,
        mangled_rope_multi_f32(true).map_err(|e| format!("rope_multi ff lookup: {e}"))?,
    )
    .map_err(|e| format!("rope_multi ff: {e}"))?;

    Ok(PortedKernels {
        norm_module,
        unary_module,
        scale_module,
        fill_module,
        diag_module,
        binbcast_module,
        tri_module,
        pad_module,
        cumsum_module,
        concat_module,
        cpy_module,
        solve_tri_module,
        rope_module,
        rms_norm: RmsNormKernels { b256, b1024 },
        unary: uk,
        scale,
        fill,
        diag,
        binbcast: bb,
        tri: tk,
        pad,
        cumsum,
        concat: ck,
        cpy: cp,
        solve_tri,
        rope,
    })
}
