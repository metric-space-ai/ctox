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
use super::ops::diag::{mangled_diag_kernel_f16, mangled_diag_kernel_f32, DiagKernels};
use super::ops::fill::{mangled_fill_kernel_f16, mangled_fill_kernel_f32, FillKernels};
use super::ops::norm::{
    mangled_rms_norm_f32_b1024, mangled_rms_norm_f32_b256, RmsNormKernels,
};
use super::ops::scale::{mangled_scale_f32, ScaleKernel};
use super::ops::unary::{mangled_unary_op_f32, UnaryKernels};
use super::ptx::{
    get_function, load_module, DIAG_PTX, FILL_PTX, NORM_PTX, SCALE_PTX, UNARY_PTX,
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
    pub rms_norm: RmsNormKernels,
    pub unary: UnaryKernels,
    pub scale: ScaleKernel,
    pub fill: FillKernels,
    pub diag: DiagKernels,
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

    Ok(PortedKernels {
        norm_module,
        unary_module,
        scale_module,
        fill_module,
        diag_module,
        rms_norm: RmsNormKernels { b256, b1024 },
        unary: uk,
        scale,
        fill,
        diag,
    })
}
