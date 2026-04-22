//! NEOX-style rotary position embedding for the DFlash draft.
//!
//! The draft runs standard 1-axis NEOX RoPE (`theta=10_000_000`);
//! the target's full-attention layer uses 4-axis MRoPE. Rather than
//! overload the MRoPE path with an "all axes equal" mode (which is
//! architecturally uncomfortable and adds fragile template-symbol
//! dependencies), this kernel is a minimal self-contained
//! implementation. See `kernels/sm_86/rope_neox.cu` for the kernel
//! body and its formula.
//!
//! Public entry: [`launch_rope_neox_bf16_inplace`]. In-place on the
//! bf16 Q or K tensor.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use super::ROPE_NEOX_PTX;

const BLOCK_DIM: u32 = 256;

static ROPE_NEOX_FN: OnceLock<CudaFunction> = OnceLock::new();

fn load_fn(device: &Arc<DeviceContext>) -> Result<CudaFunction> {
    if let Some(f) = ROPE_NEOX_FN.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(ROPE_NEOX_PTX)
        .map_err(|e| anyhow!("rope_neox.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module rope_neox.ptx: {:?}", e))?;
    let f = module
        .load_function("rope_neox_bf16_inplace")
        .map_err(|e| anyhow!("load_function rope_neox_bf16_inplace: {:?}", e))?;
    let _ = ROPE_NEOX_FN.set(f.clone());
    Ok(f)
}

/// In-place NEOX rotary position embedding on a bf16 Q or K tensor.
///
/// # Shapes
/// * `qk`  — `[n_tokens, n_heads, head_dim]` bf16, rotated in place.
/// * `pos` — `[n_tokens]` i32.
///
/// `n_dims` is the number of leading dims per head to rotate (must
/// be even and ≤ `head_dim`). For the DFlash draft this equals
/// `head_dim=128`.
///
/// `theta_base` is the RoPE base frequency (10_000_000 for the
/// Qwen3.5-27B draft).
pub fn launch_rope_neox_bf16_inplace(
    device: &Arc<DeviceContext>,
    qk: &mut CudaTensor<bf16>,
    pos: &CudaTensor<i32>,
    n_dims: usize,
    theta_base: f32,
) -> Result<()> {
    if qk.shape().len() != 3 {
        return Err(anyhow!(
            "rope_neox: qk must be 3D [n_tokens, n_heads, head_dim], got {:?}",
            qk.shape()
        ));
    }
    let n_tokens = qk.shape()[0];
    let n_heads = qk.shape()[1];
    let head_dim = qk.shape()[2];

    if pos.numel() < n_tokens {
        return Err(anyhow!(
            "rope_neox: pos.numel()={} < n_tokens={}",
            pos.numel(),
            n_tokens
        ));
    }
    if n_dims == 0 || n_dims > head_dim {
        return Err(anyhow!(
            "rope_neox: n_dims={} must satisfy 0 < n_dims <= head_dim={}",
            n_dims,
            head_dim
        ));
    }
    if n_dims.is_multiple_of(2).not() {
        return Err(anyhow!("rope_neox: n_dims must be even, got {}", n_dims));
    }

    let half = n_dims / 2;
    let total_pairs = n_tokens * n_heads * half;
    if total_pairs == 0 {
        return Ok(());
    }

    let grid = ((total_pairs as u32) + BLOCK_DIM - 1) / BLOCK_DIM;
    let cfg = LaunchConfig {
        grid_dim: (grid, 1, 1),
        block_dim: (BLOCK_DIM, 1, 1),
        shared_mem_bytes: 0,
    };

    let f = load_fn(device)?;
    let stream = device.raw().default_stream();
    let n_tokens_i32 = n_tokens as i32;
    let n_heads_i32 = n_heads as i32;
    let head_dim_i32 = head_dim as i32;
    let n_dims_i32 = n_dims as i32;

    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(qk.buf_mut())
        .arg(pos.buf())
        .arg(&n_tokens_i32)
        .arg(&n_heads_i32)
        .arg(&head_dim_i32)
        .arg(&n_dims_i32)
        .arg(&theta_base);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "rope_neox launch (n_tokens={} n_heads={} head_dim={} n_dims={}): {:?}",
            n_tokens,
            n_heads,
            head_dim,
            n_dims,
            e
        )
    })?;
    Ok(())
}

/// Local helper — Rust's `bool::not()` requires the `Not` trait;
/// re-exporting it here keeps the call site above terse.
trait BoolExt {
    fn not(self) -> bool;
}
impl BoolExt for bool {
    #[inline]
    fn not(self) -> bool {
        !self
    }
}
