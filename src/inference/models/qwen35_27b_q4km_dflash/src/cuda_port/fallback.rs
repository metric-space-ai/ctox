//! ggml-cuda fallback for ops not yet ported to Rust.
//!
//! **STATUS: WIP — wrap_tensor + exec_ggml_single segfaults inside
//! ggml-cuda's mul_mat dispatcher because `tensor->buffer` is null.
//! ggml-cuda derives the backend from the buffer pointer; our
//! "wrap the raw CUdeviceptr" approach leaves it unset. Two
//! workable fixes:**
//!
//!  1. **Route device memory through ggml's backend allocator**
//!     (`ggml_backend_alloc_buffer(ggml_backend_cuda, bytes)`) so
//!     every tensor has a proper ggml_backend_buffer attached.
//!     Then the Rust-native path extracts `tensor->data` as a
//!     CUdeviceptr instead of us allocating `cuMemAlloc_v2` ourselves.
//!  2. **Call ggml-cuda's per-op dispatcher directly** rather than
//!     through `ggml_backend_graph_compute` — bypasses the buffer
//!     indirection but requires reverse-engineering ggml-cuda's
//!     internal per-node ABI.
//!
//! (1) is cleaner and will be the path once the hybrid executor
//! needs mmq/fattn/gdn. For now, the per-op verifiers + the
//! pure-Rust `graph_smoke` test prove the ported ops work; the
//! hybrid-fallback path is still needed before the full forward
//! can run, but won't get fixed as a drive-by — it's worth its own
//! focused session to integrate with ggml's backend buffer API.
//!
//! Below is the scaffold for that future integration — exports
//! kept stable so callers can land.
//!
//! This lets the Rust executor cover the **whole** Qwen3.5 forward
//! pass today — ported ops go through our bare-metal dispatcher,
//! unported ops transparently fall back to ggml-cuda. Once an op
//! gets ported for real, swapping out its fallback entry in the
//! executor is a one-line change.
//!
//! Design notes:
//!   • Each fallback call allocates a throwaway ggml_context per
//!     op. Overhead per op: ~2µs. Not a hot-path problem for the
//!     ops we use fallback for (matmul, FA — big ops).
//!   • The fallback never touches host memory; tensors are
//!     declared with the `data` field pointing at our existing
//!     CUdeviceptr.
//!   • Stream semantics: we sync the passed `CUstream` before
//!     handing off to ggml_backend_graph_compute to preserve
//!     ordering with preceding Rust-dispatched ops.

use std::ffi::c_void;
use std::ptr;

use crate::cuda_port::driver::{cuStreamSynchronize, CUstream};
use crate::cuda_port::graph::{DType, Tensor};
use crate::ffi as sys;

/// Map our `DType` to `ggml_type`.
pub fn dtype_to_ggml(t: DType) -> sys::ggml_type {
    use sys::ggml_type::*;
    match t {
        DType::F32 => GGML_TYPE_F32,
        DType::F16 => GGML_TYPE_F16,
        DType::BF16 => GGML_TYPE_BF16,
        DType::I32 => GGML_TYPE_I32,
        DType::Q4K => GGML_TYPE_Q4_K,
    }
}

/// Construct a ggml_tensor pointing at our device memory.
/// Allocates a `ggml_tensor_overhead()`-sized slot inside `ctx`
/// and fills the shape/stride/dtype/data fields manually. Does
/// not touch GGML's backend buffer — `buffer` stays null, since
/// the data already lives on the device.
///
/// # Safety
///
/// Caller must ensure:
///   • `ctx` is valid and alive for the duration of this tensor's use
///   • `t.data` points at a device buffer large enough for the
///     declared shape (`nelements × elem_size` bytes)
pub unsafe fn wrap_tensor(
    ctx: *mut sys::ggml_context,
    t: &Tensor,
    name: &str,
) -> *mut sys::ggml_tensor {
    let dims = {
        let dtype = dtype_to_ggml(t.dtype);
        sys::ggml_new_tensor_4d(ctx, dtype, t.ne[0], t.ne[1], t.ne[2], t.ne[3])
    };
    if dims.is_null() {
        return ptr::null_mut();
    }
    // Override the data pointer — ggml would have allocated its
    // own buffer if we went through the normal path; we point at
    // our already-allocated CUdeviceptr instead.
    (*dims).data = t.data.0 as *mut c_void;
    // Override strides in bytes (ggml uses nb[i] as a byte stride
    // array). Our tensor's s[i] is in elements.
    let esz = t.dtype.elem_size() as usize;
    (*dims).nb[0] = esz;
    for i in 1..4 {
        (*dims).nb[i] = (t.s[i] as usize) * esz;
    }
    // Name helps diagnostics if ggml aborts.
    let cname = std::ffi::CString::new(name).unwrap_or_default();
    sys::ggml_set_name(dims, cname.as_ptr());
    dims
}

/// Execute a single ggml op through the backend compute path.
///
/// `build` is a closure that takes a `ggml_context *` and returns
/// the single `ggml_tensor *` node that's the op's output. The
/// caller constructs the op there using `sys::ggml_*` calls, then
/// we build a 1-node graph from it and run
/// `ggml_backend_graph_compute`.
///
/// The result is written in-place into whichever Tensor backs the
/// op's output — caller is responsible for pointing the op's
/// output tensor at the right CUdeviceptr.
///
/// Example: wrapping `ggml_mul_mat` as a fallback —
///
/// ```ignore
/// let dst = exec_ggml_single(
///     backend,
///     stream,
///     |ctx| {
///         let a = wrap_tensor(ctx, &a_t, "a");
///         let b = wrap_tensor(ctx, &b_t, "b");
///         sys::ggml_mul_mat(ctx, a, b)  // returns *mut ggml_tensor
///     },
/// )?;
/// ```
pub fn exec_ggml_single<F>(
    backend: sys::ggml_backend_t,
    stream: CUstream,
    build: F,
) -> Result<(), String>
where
    F: FnOnce(*mut sys::ggml_context) -> *mut sys::ggml_tensor,
{
    // Sync the stream so Rust-dispatched ops landing earlier
    // complete before ggml's kernel launches observe the data.
    unsafe { cuStreamSynchronize(stream) };

    // ggml_init_params — small scratch context for the graph.
    let params = sys::ggml_init_params {
        mem_size: 4 * 1024 * 1024, // 4 MiB scratch — plenty for 1 node
        mem_buffer: ptr::null_mut(),
        no_alloc: true, // we point tensors at existing CUdeviceptrs
    };
    let ctx = unsafe { sys::ggml_init(params) };
    if ctx.is_null() {
        return Err("ggml_init returned null".into());
    }

    let out = build(ctx);
    if out.is_null() {
        unsafe { sys::ggml_free(ctx) };
        return Err("fallback build closure returned null".into());
    }

    // Construct a 1-node graph. Size 2048 nodes is upstream default.
    let gf = unsafe { sys::ggml_new_graph_custom(ctx, 2048, false) };
    if gf.is_null() {
        unsafe { sys::ggml_free(ctx) };
        return Err("ggml_new_graph_custom returned null".into());
    }
    unsafe { sys::ggml_build_forward_expand(gf, out) };

    let rc = unsafe { sys::ggml_backend_graph_compute(backend, gf) };
    unsafe { sys::ggml_free(ctx) };
    match rc {
        sys::ggml_status::GGML_STATUS_SUCCESS => Ok(()),
        other => Err(format!("ggml_backend_graph_compute: status={:?}", other)),
    }
}
