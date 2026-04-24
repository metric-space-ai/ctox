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
use super::ops::norm::{
    mangled_rms_norm_f32_b1024, mangled_rms_norm_f32_b256, RmsNormKernels,
};
use super::ops::unary::{mangled_unary_op_f32, UnaryKernels};
use super::ptx::{get_function, load_module, NORM_PTX, UNARY_PTX};

/// All kernel handles the Rust side needs, resolved once.
pub struct PortedKernels {
    /// Kept alive so the module doesn't unload.
    #[allow(dead_code)]
    norm_module: CUmodule,
    #[allow(dead_code)]
    unary_module: CUmodule,
    pub rms_norm: RmsNormKernels,
    pub unary: UnaryKernels,
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

    // unary.cu — unary_op_kernel<op_silu|op_neg|op_exp, float>
    let unary_module = load_module(UNARY_PTX).map_err(|e| format!("unary.ptx: {e}"))?;
    let mut uk = UnaryKernels::default();
    for (slot, needle) in &[
        (&mut uk.silu_f32 as *mut _, b"op_silu".as_slice()),
        (&mut uk.neg_f32 as *mut _, b"op_neg".as_slice()),
        (&mut uk.exp_f32 as *mut _, b"op_exp".as_slice()),
    ] {
        let name = mangled_unary_op_f32(needle).map_err(|e| format!("unary lookup: {e}"))?;
        let f = get_function(unary_module, &name).map_err(|e| format!("unary: {e}"))?;
        // SAFETY: all slot pointers point into the stack-local `uk`.
        unsafe { **slot = f };
    }

    Ok(PortedKernels {
        norm_module,
        unary_module,
        rms_norm: RmsNormKernels { b256, b1024 },
        unary: uk,
    })
}
