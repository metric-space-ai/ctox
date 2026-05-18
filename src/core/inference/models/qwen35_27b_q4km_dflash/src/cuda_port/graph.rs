//! Minimal Rust-native graph executor for the cuda_port migration.
//!
//! Purpose: glue the 17 ported op dispatchers into end-to-end
//! sequences (e.g. "RMSNorm → SiLU → Scale" chains) so we can
//! validate them in a realistic forward-pass context, not just
//! isolated per-op verifiers.
//!
//! Design principles:
//!   • **No ggml tensors.** Everything is raw `CUdeviceptr` + shape.
//!     Adapters can bridge to ggml_tensor where needed.
//!   • **Caller-owned memory.** The graph doesn't allocate or free
//!     device memory — each [`Tensor`] wraps a `CUdeviceptr` the
//!     caller already allocated. Simplifies lifetimes drastically.
//!   • **Single-stream.** All ops go to one `CUstream` — the
//!     caller's responsibility to sync when needed. Concurrent
//!     multi-stream comes later.
//!   • **Hybrid dispatch.** Every op has two candidate backends:
//!       1. Rust-native via `cuda_port::ops::*` (preferred, faster
//!          to iterate)
//!       2. A ggml-cuda fallback (via `sys::ggml_*` calls) that
//!          re-uses the existing C++ dispatcher through a wrapped
//!          `ggml_cgraph` of one node. Used for mmq/fattn/gdn
//!          until those are ported.
//!     The dispatcher picks automatically based on whether the op
//!     is implemented on the Rust side.

use std::os::raw::c_int;

use crate::cuda_port::driver::{CUdeviceptr, CUstream};
use crate::cuda_port::module::PortedKernels;

/// Element dtype. Only the variants Qwen3.5 actually uses.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DType {
    F32,
    F16,
    BF16,
    I32,
    Q4K,  // quantized — dispatch must use mmq path
}

impl DType {
    /// Size of one element in bytes. For quantized types this is
    /// the super-block size; callers need block-aware indexing.
    pub fn elem_size(self) -> usize {
        match self {
            DType::F32 | DType::I32 => 4,
            DType::F16 | DType::BF16 => 2,
            DType::Q4K => 144, // GGML Q4_K block = 256 elems in 144 bytes
        }
    }
}

/// A tensor handle: shape + element strides + device pointer.
/// Strides are in **elements**, not bytes.
#[derive(Copy, Clone, Debug)]
pub struct Tensor {
    pub data: CUdeviceptr,
    pub dtype: DType,
    pub ne: [i64; 4],
    /// Element strides along each axis. `s[0]` is always 1 for
    /// contiguous tensors; we carry it anyway so stride-aware ops
    /// (permuted views) work without a separate layout struct.
    pub s: [i64; 4],
}

impl Tensor {
    /// Build a contiguous tensor around an already-allocated device
    /// pointer. Strides computed as cumulative products of `ne`.
    pub fn contiguous(data: CUdeviceptr, dtype: DType, ne: [i64; 4]) -> Self {
        let mut s = [1i64; 4];
        for d in 1..4 {
            s[d] = s[d - 1] * ne[d - 1];
        }
        Self { data, dtype, ne, s }
    }

    pub fn nelements(&self) -> i64 {
        self.ne[0] * self.ne[1] * self.ne[2] * self.ne[3]
    }

    pub fn nbytes(&self) -> usize {
        self.nelements() as usize * self.dtype.elem_size()
    }
}

/// Execution context — groups the resolved PortedKernels + stream.
/// Every op call takes one of these. The caller is expected to
/// `ensure_current_context()` before the first call on a thread.
pub struct ExecCtx<'a> {
    pub kernels: &'a PortedKernels,
    pub stream: CUstream,
}

impl<'a> ExecCtx<'a> {
    pub fn new(kernels: &'a PortedKernels, stream: CUstream) -> Self {
        Self { kernels, stream }
    }
}

// ═══════════════════════════════════════════════════════════════
// High-level op entries — thin wrappers over cuda_port::ops::* that
// take/return `Tensor` instead of raw (ptr, shape, stride) tuples.
// Only the ops we've already ported are listed. Unported ops
// (mul_mat, flash_attn_ext, gated_delta_net, ssm_conv-non-tree)
// will grow their entries as their ports land.
// ═══════════════════════════════════════════════════════════════

/// In-place RMSNorm: `dst[i] = x[i] / sqrt(mean(x²) + eps)`.
/// `x` and `dst` must have the same shape; both f32.
pub fn rms_norm(
    ctx: &ExecCtx<'_>,
    x: &Tensor,
    dst: &Tensor,
    eps: f32,
) -> Result<(), String> {
    assert_eq!(x.dtype, DType::F32);
    assert_eq!(dst.dtype, DType::F32);
    assert_eq!(x.ne, dst.ne);
    use crate::cuda_port::ops::norm::ggml_cuda_op_rms_norm;
    // The upstream op expects byte strides (nb00..nb03). Our tensor
    // carries element strides, so multiply by elem_size.
    let esz = x.dtype.elem_size() as i64;
    let rc = ggml_cuda_op_rms_norm(
        &ctx.kernels.rms_norm,
        x.data,
        dst.data,
        x.ne[0] as c_int,
        x.ne[1] as c_int,
        x.ne[2] as c_int,
        x.ne[3] as c_int,
        x.s[0] * esz,
        x.s[1] * esz,
        x.s[2] * esz,
        x.s[3] * esz,
        eps,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("rms_norm launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst = scale * x + bias`, elementwise f32.
pub fn scale(
    ctx: &ExecCtx<'_>,
    x: &Tensor,
    dst: &Tensor,
    scale: f32,
    bias: f32,
) -> Result<(), String> {
    use crate::cuda_port::ops::scale::ggml_cuda_op_scale_f32;
    let rc = ggml_cuda_op_scale_f32(
        &ctx.kernels.scale,
        x.data,
        dst.data,
        x.nelements(),
        scale,
        bias,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("scale launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst[i] = silu(x[i]) = x[i] * sigmoid(x[i])`, f32.
pub fn silu(ctx: &ExecCtx<'_>, x: &Tensor, dst: &Tensor) -> Result<(), String> {
    use crate::cuda_port::ops::unary::ggml_cuda_op_silu_f32;
    let rc = ggml_cuda_op_silu_f32(
        &ctx.kernels.unary,
        x.data,
        dst.data,
        x.nelements() as c_int,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("silu launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst = a + b`, elementwise f32 (no broadcast).
pub fn add(
    ctx: &ExecCtx<'_>,
    a: &Tensor,
    b: &Tensor,
    dst: &Tensor,
) -> Result<(), String> {
    use crate::cuda_port::ops::binbcast::{
        ggml_cuda_op_add_f32, BinBcastTensor,
    };
    let shape = BinBcastTensor { ne: dst.ne, s: dst.s };
    let rc = ggml_cuda_op_add_f32(
        &ctx.kernels.binbcast,
        a.data,
        b.data,
        dst.data,
        &shape,
        &shape,
        &shape,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("add launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst = a * b`, elementwise f32 (no broadcast).
pub fn mul(
    ctx: &ExecCtx<'_>,
    a: &Tensor,
    b: &Tensor,
    dst: &Tensor,
) -> Result<(), String> {
    use crate::cuda_port::ops::binbcast::{
        ggml_cuda_op_mul_f32, BinBcastTensor,
    };
    let shape = BinBcastTensor { ne: dst.ne, s: dst.s };
    let rc = ggml_cuda_op_mul_f32(
        &ctx.kernels.binbcast,
        a.data,
        b.data,
        dst.data,
        &shape,
        &shape,
        &shape,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("mul launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst = a - b`, elementwise f32 (no broadcast).
pub fn sub(
    ctx: &ExecCtx<'_>,
    a: &Tensor,
    b: &Tensor,
    dst: &Tensor,
) -> Result<(), String> {
    use crate::cuda_port::ops::binbcast::{
        ggml_cuda_op_sub_f32, BinBcastTensor,
    };
    let shape = BinBcastTensor { ne: dst.ne, s: dst.s };
    let rc = ggml_cuda_op_sub_f32(
        &ctx.kernels.binbcast,
        a.data,
        b.data,
        dst.data,
        &shape,
        &shape,
        &shape,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("sub launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst[i] = sigmoid(x[i])`, f32.
pub fn sigmoid(ctx: &ExecCtx<'_>, x: &Tensor, dst: &Tensor) -> Result<(), String> {
    use crate::cuda_port::ops::unary::ggml_cuda_op_sigmoid_f32;
    let rc = ggml_cuda_op_sigmoid_f32(
        &ctx.kernels.unary,
        x.data,
        dst.data,
        x.nelements() as c_int,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("sigmoid launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst[i] = -x[i]`, f32.
pub fn neg(ctx: &ExecCtx<'_>, x: &Tensor, dst: &Tensor) -> Result<(), String> {
    use crate::cuda_port::ops::unary::ggml_cuda_op_neg_f32;
    let rc = ggml_cuda_op_neg_f32(
        &ctx.kernels.unary,
        x.data,
        dst.data,
        x.nelements() as c_int,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("neg launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst[i] = exp(x[i])`, f32.
pub fn exp(ctx: &ExecCtx<'_>, x: &Tensor, dst: &Tensor) -> Result<(), String> {
    use crate::cuda_port::ops::unary::ggml_cuda_op_exp_f32;
    let rc = ggml_cuda_op_exp_f32(
        &ctx.kernels.unary,
        x.data,
        dst.data,
        x.nelements() as c_int,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("exp launch: {rc}"))
    } else {
        Ok(())
    }
}

/// `dst[i] = softplus(x[i]) = log(1 + exp(x[i]))`, f32.
pub fn softplus(ctx: &ExecCtx<'_>, x: &Tensor, dst: &Tensor) -> Result<(), String> {
    use crate::cuda_port::ops::unary::ggml_cuda_op_softplus_f32;
    let rc = ggml_cuda_op_softplus_f32(
        &ctx.kernels.unary,
        x.data,
        dst.data,
        x.nelements() as c_int,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("softplus launch: {rc}"))
    } else {
        Ok(())
    }
}

/// Fill `dst` with a constant f32 value.
pub fn fill_f32(ctx: &ExecCtx<'_>, dst: &Tensor, value: f32) -> Result<(), String> {
    use crate::cuda_port::ops::fill::ggml_cuda_op_fill_f32;
    let rc = ggml_cuda_op_fill_f32(
        &ctx.kernels.fill,
        dst.data,
        dst.nelements(),
        value,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("fill launch: {rc}"))
    } else {
        Ok(())
    }
}

/// Cumsum along axis 0 (inclusive prefix sum per row), f32.
pub fn cumsum(ctx: &ExecCtx<'_>, x: &Tensor, dst: &Tensor) -> Result<(), String> {
    use crate::cuda_port::ops::cumsum::ggml_cuda_op_cumsum_f32;
    let rc = ggml_cuda_op_cumsum_f32(
        &ctx.kernels.cumsum,
        x.data,
        dst.data,
        x.ne,
        x.s,
        dst.s,
        ctx.stream,
    );
    if rc != 0 {
        Err(format!("cumsum launch: {rc}"))
    } else {
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tensor_contiguous_strides() {
        let t = Tensor::contiguous(CUdeviceptr(0), DType::F32, [8, 4, 2, 1]);
        assert_eq!(t.s, [1, 8, 32, 64]);
        assert_eq!(t.nelements(), 64);
        assert_eq!(t.nbytes(), 256);
    }
}
