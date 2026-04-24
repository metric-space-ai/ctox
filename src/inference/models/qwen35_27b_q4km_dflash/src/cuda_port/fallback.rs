//! ggml-cuda fallback for ops not yet ported to Rust.
//!
//! Bridges the Rust-native graph executor (`cuda_port::graph::Tensor`)
//! to the C++ dispatcher (`libggml-cuda.so`) for ops where the
//! Rust port isn't done yet (mmq, fattn, gated_delta_net,
//! ssm_conv-non-tree).
//!
//! # Approach: route device memory through ggml's backend allocator
//!
//! ggml-cuda's dispatcher identifies which backend owns a tensor by
//! looking at `tensor->buffer`. That field is set when a tensor is
//! allocated through `ggml_backend_alloc_ctx_tensors(ctx, backend)`.
//! If we allocate our own device memory via `cuMemAlloc_v2` and
//! wrap it as a ggml_tensor with `buffer = null`, ggml_mul_mat
//! segfaults inside its dispatcher.
//!
//! So: we let ggml allocate the tensors (giving them proper
//! buffer metadata), then both the Rust-native path and the
//! ggml-cuda-fallback path use `tensor->data` as the device
//! pointer. Interop works because `tensor->data` IS the CUdeviceptr
//! — ggml-cuda's buffer is just a thin wrapper over CUDA memory.
//!
//! # Typical usage
//!
//! ```ignore
//! let mut gctx = GgmlBackendCtx::new(backend)?;
//! let t_x = gctx.new_tensor_f32([n, m, 1, 1], "x");
//! let t_w = gctx.new_tensor_f32([n, k, 1, 1], "w");
//! gctx.realize()?; // allocates ggml_backend_buffer + sets tensor->data
//!
//! // upload data via ggml_backend_tensor_set (abstracts memcpyH2D)
//! gctx.upload(t_x, &h_x);
//! gctx.upload(t_w, &h_w);
//!
//! // Rust-native path: extract CUdeviceptr from tensor->data
//! let d_x = gctx.as_cuda_ptr(t_x);
//! rms_norm(&exec, &rust_tensor_wrapping(d_x), ...)?;
//!
//! // Fallback path: build an op graph and compute
//! gctx.compute(|ctx| {
//!     let mm = sys::ggml_mul_mat(ctx, t_x, t_w);
//!     // mm is the output — ggml will allocate it on the same backend
//!     mm
//! })?;
//! ```

use std::ffi::CString;
use std::ptr;

use crate::cuda_port::driver::CUdeviceptr;
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

/// A ggml context bundled with its backend + the backend buffer that
/// backs all tensors created through it. Dropping the struct tears
/// everything down in the right order (buffer → context → nothing;
/// the backend is caller-owned).
pub struct GgmlBackendCtx {
    backend: sys::ggml_backend_t,
    ctx: *mut sys::ggml_context,
    buffer: sys::ggml_backend_buffer_t, // null until realize()
}

impl GgmlBackendCtx {
    /// Build a fresh context that will sit on `backend`. Allocates a
    /// scratch metadata area big enough for a few hundred tensors —
    /// device memory itself isn't touched here, only the host-side
    /// `ggml_tensor` headers.
    pub fn new(backend: sys::ggml_backend_t) -> Result<Self, String> {
        let params = sys::ggml_init_params {
            mem_size: 16 * 1024 * 1024, // 16 MiB for tensor metadata
            mem_buffer: ptr::null_mut(),
            no_alloc: true, // buffers come from the backend allocator at realize()
        };
        let ctx = unsafe { sys::ggml_init(params) };
        if ctx.is_null() {
            return Err("ggml_init returned null".into());
        }
        Ok(Self {
            backend,
            ctx,
            buffer: ptr::null_mut(),
        })
    }

    /// Reserve a tensor slot. Device memory is still unallocated;
    /// call [`realize`] after declaring all tensors to get a real
    /// CUdeviceptr out of `tensor->data`.
    pub fn new_tensor(
        &self,
        dtype: DType,
        ne: [i64; 4],
        name: &str,
    ) -> *mut sys::ggml_tensor {
        let t = unsafe {
            sys::ggml_new_tensor_4d(self.ctx, dtype_to_ggml(dtype), ne[0], ne[1], ne[2], ne[3])
        };
        if t.is_null() {
            return ptr::null_mut();
        }
        let cname = CString::new(name).unwrap_or_default();
        unsafe { sys::ggml_set_name(t, cname.as_ptr()) };
        t
    }

    /// Convenience shorthand for f32 tensors.
    pub fn new_tensor_f32(&self, ne: [i64; 4], name: &str) -> *mut sys::ggml_tensor {
        self.new_tensor(DType::F32, ne, name)
    }

    /// Allocate a backend buffer large enough for every declared
    /// tensor and bind each tensor's `->data` pointer into it. After
    /// this returns, `tensor->data` is a device address that both
    /// ggml-cuda and our Rust dispatcher can use.
    pub fn realize(&mut self) -> Result<(), String> {
        if !self.buffer.is_null() {
            return Err("already realized".into());
        }
        let buf = unsafe { sys::ggml_backend_alloc_ctx_tensors(self.ctx, self.backend) };
        if buf.is_null() {
            return Err("ggml_backend_alloc_ctx_tensors returned null".into());
        }
        self.buffer = buf;
        Ok(())
    }

    /// Upload host f32 data to a tensor already realized on the GPU.
    pub fn upload_f32(
        &self,
        t: *mut sys::ggml_tensor,
        host: &[f32],
    ) -> Result<(), String> {
        if t.is_null() || self.buffer.is_null() {
            return Err("upload_f32: tensor null or context not realized".into());
        }
        let bytes = host.len() * std::mem::size_of::<f32>();
        unsafe {
            sys::ggml_backend_tensor_set(t, host.as_ptr() as *const _, 0, bytes);
        }
        Ok(())
    }

    /// Download a tensor back to host f32. `n` must match the
    /// tensor's element count.
    pub fn download_f32(&self, t: *mut sys::ggml_tensor, n: usize) -> Result<Vec<f32>, String> {
        let mut out = vec![0.0_f32; n];
        let bytes = n * std::mem::size_of::<f32>();
        unsafe {
            sys::ggml_backend_tensor_get(t, out.as_mut_ptr() as *mut _, 0, bytes);
        }
        Ok(out)
    }

    /// Extract a [`Tensor`] view suitable for the Rust-native
    /// dispatcher. The returned Tensor aliases the ggml tensor's
    /// device memory — no copy, just a handle translation.
    pub fn as_rust_tensor(&self, t: *mut sys::ggml_tensor) -> Tensor {
        unsafe {
            let ne = (*t).ne;
            let dtype = match (*t).type_ {
                sys::ggml_type::GGML_TYPE_F32 => DType::F32,
                sys::ggml_type::GGML_TYPE_F16 => DType::F16,
                sys::ggml_type::GGML_TYPE_BF16 => DType::BF16,
                sys::ggml_type::GGML_TYPE_I32 => DType::I32,
                sys::ggml_type::GGML_TYPE_Q4_K => DType::Q4K,
                other => panic!("as_rust_tensor: unsupported ggml_type {:?}", other),
            };
            let esz = dtype.elem_size() as i64;
            // ggml's nb[] is byte strides; we want element strides.
            let s = [
                ((*t).nb[0] as i64) / esz,
                ((*t).nb[1] as i64) / esz,
                ((*t).nb[2] as i64) / esz,
                ((*t).nb[3] as i64) / esz,
            ];
            Tensor {
                data: CUdeviceptr((*t).data as u64),
                dtype,
                ne,
                s,
            }
        }
    }

    /// Build a single-op subgraph rooted at whatever tensor the
    /// closure returns, allocate it via `ggml_gallocr_alloc_graph`
    /// (so any op-created intermediates get a real buffer), then
    /// compute it via `ggml_backend_graph_compute`.
    ///
    /// Works for any op whose inputs live inside this context —
    /// including ops we haven't ported (mmq, fattn, gated_delta_net).
    pub fn compute<F>(&self, build: F) -> Result<*mut sys::ggml_tensor, String>
    where
        F: FnOnce(*mut sys::ggml_context) -> *mut sys::ggml_tensor,
    {
        let out = build(self.ctx);
        if out.is_null() {
            return Err("compute: build closure returned null".into());
        }
        let gf = unsafe { sys::ggml_new_graph_custom(self.ctx, 2048, false) };
        if gf.is_null() {
            return Err("ggml_new_graph_custom returned null".into());
        }
        unsafe { sys::ggml_build_forward_expand(gf, out) };

        // Allocate any op-created intermediates (including `out` itself
        // if it wasn't among the pre-declared tensors). Without this
        // the mul_mat output has no backing buffer → segfault.
        let buft = unsafe { sys::ggml_backend_get_default_buffer_type(self.backend) };
        let alloc = unsafe { sys::ggml_gallocr_new(buft) };
        if alloc.is_null() {
            return Err("ggml_gallocr_new returned null".into());
        }
        let ok = unsafe { sys::ggml_gallocr_alloc_graph(alloc, gf) };
        if !ok {
            unsafe { sys::ggml_gallocr_free(alloc) };
            return Err("ggml_gallocr_alloc_graph failed".into());
        }

        let rc = unsafe { sys::ggml_backend_graph_compute(self.backend, gf) };
        unsafe { sys::ggml_gallocr_free(alloc) };
        match rc {
            sys::ggml_status::GGML_STATUS_SUCCESS => Ok(out),
            other => Err(format!("ggml_backend_graph_compute: status={:?}", other)),
        }
    }

    pub fn ctx(&self) -> *mut sys::ggml_context {
        self.ctx
    }
}

impl Drop for GgmlBackendCtx {
    fn drop(&mut self) {
        unsafe {
            if !self.buffer.is_null() {
                sys::ggml_backend_buffer_free(self.buffer);
                self.buffer = ptr::null_mut();
            }
            if !self.ctx.is_null() {
                sys::ggml_free(self.ctx);
                self.ctx = ptr::null_mut();
            }
            // backend is caller-owned — don't free.
        }
    }
}
