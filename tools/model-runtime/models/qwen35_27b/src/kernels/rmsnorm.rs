//! RMSNorm Rust-side wrapper. Template for all future kernel wrappers.
//!
//! Conventions established here (follow when porting new kernels):
//!
//!   * One Rust module per `.cu` file. Module name mirrors the .cu stem.
//!   * `launch_<kernel>_<dtype>()` is the public entry. Takes
//!     `&Arc<DeviceContext>`, `&CudaTensor<...>` inputs, `&mut
//!     CudaTensor<...>` outputs, plus scalar params.
//!   * Validates input shapes with clear error messages — launching
//!     with mismatched shapes would corrupt memory silently.
//!   * Caches the loaded `CudaFunction` per process via `OnceLock`.
//!     Module loading has nontrivial fixed cost and we call these
//!     kernels hot-path so cache is mandatory.
//!   * Does NOT synchronize the stream — callers sync at phase
//!     boundaries. We want kernel launches to queue.
//!
//! Load chain:
//!   build.rs emits `OUT_DIR/ptx_registry.rs` with `RMSNORM_PTX: &[u8]`.
//!   We `include!` it below, convert to `Ptx::from_src(String)`, call
//!   `ctx.load_module(ptx)`, then `module.load_function("rmsnorm_f32")`.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

// PTX blob comes from the parent module's auto-generated registry.
use super::RMSNORM_PTX;

/// One-shot cache for the loaded kernel function. Safe because we
/// target single-context-per-process; a second device would need
/// its own cache. If/when we go multi-GPU, replace this with
/// something keyed by (ordinal, kernel-name).
static RMSNORM_F32_FN: OnceLock<CudaFunction> = OnceLock::new();

fn rmsnorm_f32_fn(device: &Arc<DeviceContext>) -> Result<CudaFunction> {
    if let Some(f) = RMSNORM_F32_FN.get() {
        return Ok(f.clone());
    }
    // PTX is ASCII text; cudarc wants String.
    let ptx_src = std::str::from_utf8(RMSNORM_PTX)
        .map_err(|e| anyhow!("rmsnorm.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module rmsnorm.ptx: {:?}", e))?;
    let f = module
        .load_function("rmsnorm_f32")
        .map_err(|e| anyhow!("load_function rmsnorm_f32: {:?}", e))?;
    // If another thread races us, OnceLock::set returns Err — we just
    // use whatever got stored. Both copies are equivalent.
    let _ = RMSNORM_F32_FN.set(f.clone());
    Ok(f)
}

/// `y ← (x / sqrt(mean(x²) + eps)) * weight`
///
/// Shapes:
///   * `x`:      `[n_tokens, hidden_dim]` f32 row-major
///   * `weight`: `[hidden_dim]`            f32
///   * `y`:      `[n_tokens, hidden_dim]` f32 (pre-allocated output)
///
/// Does not synchronize the stream. Caller syncs at phase boundary.
pub fn launch_rmsnorm_f32(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    weight: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
    eps: f32,
) -> Result<()> {
    // Shape validation.
    if x.shape().len() != 2 {
        return Err(anyhow!(
            "rmsnorm: x must be 2D [n_tokens, hidden_dim], got {:?}",
            x.shape()
        ));
    }
    if weight.shape().len() != 1 {
        return Err(anyhow!(
            "rmsnorm: weight must be 1D [hidden_dim], got {:?}",
            weight.shape()
        ));
    }
    if y.shape() != x.shape() {
        return Err(anyhow!(
            "rmsnorm: y.shape {:?} != x.shape {:?}",
            y.shape(),
            x.shape()
        ));
    }
    let n_tokens = x.shape()[0];
    let hidden_dim = x.shape()[1];
    if weight.shape()[0] != hidden_dim {
        return Err(anyhow!(
            "rmsnorm: weight dim {} != x hidden_dim {}",
            weight.shape()[0],
            hidden_dim
        ));
    }
    if n_tokens == 0 || hidden_dim == 0 {
        // Nothing to do. Avoid launching a 0-block grid.
        return Ok(());
    }

    // Launch config: one block per token, block_dim = hidden_dim rounded
    // up to a multiple of 32 (warp), capped at 1024 (CUDA max block size).
    let mut block_dim = hidden_dim.min(1024);
    block_dim = block_dim.div_ceil(32) * 32;
    let cfg = LaunchConfig {
        grid_dim: (n_tokens as u32, 1, 1),
        block_dim: (block_dim as u32, 1, 1),
        shared_mem_bytes: 0,
    };

    let f = rmsnorm_f32_fn(device)?;
    let stream = device.raw().default_stream();
    let hidden_dim_i32 = hidden_dim as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(x.buf())
        .arg(weight.buf())
        .arg(y.buf_mut())
        .arg(&hidden_dim_i32)
        .arg(&eps);

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("rmsnorm_f32 launch (n_tokens={} hidden={}): {:?}", n_tokens, hidden_dim, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU reference — used by the on-host integration test.
    fn rmsnorm_cpu(x: &[f32], weight: &[f32], y: &mut [f32], eps: f32) {
        let n_tokens = x.len() / weight.len();
        let hidden_dim = weight.len();
        for t in 0..n_tokens {
            let row = &x[t * hidden_dim..(t + 1) * hidden_dim];
            let mean_sq: f32 = row.iter().map(|v| v * v).sum::<f32>() / hidden_dim as f32;
            let scale = 1.0 / (mean_sq + eps).sqrt();
            let y_row = &mut y[t * hidden_dim..(t + 1) * hidden_dim];
            for i in 0..hidden_dim {
                y_row[i] = row[i] * scale * weight[i];
            }
        }
    }

    /// Device-backed end-to-end. Ignored by default — run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture rmsnorm
    #[test]
    #[ignore]
    fn rmsnorm_vs_cpu_golden() {
        // Use a shape representative of Qwen3.5-27B (hidden=5120) so the
        // test exercises the warp fan-in path with >1 warp per block.
        let n_tokens = 8usize;
        let hidden_dim = 5120usize;
        let eps = 1e-6f32;

        // Deterministic pseudo-random via simple LCG so the test is
        // host-independent.
        let mut seed: u32 = 0x9E3779B9;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            // Map to roughly [-1, 1].
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };
        let x_host: Vec<f32> = (0..n_tokens * hidden_dim).map(|_| rand_f()).collect();
        let w_host: Vec<f32> = (0..hidden_dim).map(|_| rand_f().abs() + 0.1).collect();

        // CPU golden.
        let mut y_cpu = vec![0.0f32; n_tokens * hidden_dim];
        rmsnorm_cpu(&x_host, &w_host, &mut y_cpu, eps);

        // Device run.
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let x = CudaTensor::<f32>::from_host(
            dev.clone(),
            vec![n_tokens, hidden_dim],
            &x_host,
        )
        .expect("upload x");
        let w = CudaTensor::<f32>::from_host(dev.clone(), vec![hidden_dim], &w_host)
            .expect("upload w");
        let mut y = CudaTensor::<f32>::zeros(dev.clone(), vec![n_tokens, hidden_dim])
            .expect("alloc y");

        launch_rmsnorm_f32(&dev, &x, &w, &mut y, eps).expect("launch");
        dev.synchronize().expect("sync");

        let y_gpu = y.to_host().expect("download y");

        // Compare. RMSNorm sums-of-squares over 5120 f32s in different
        // orders between CPU sequential and GPU warp-reduction; we
        // expect relative drift on the order of 5120 × machine_eps ≈ 3e-4.
        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        for (a, b) in y_cpu.iter().zip(y_gpu.iter()) {
            let d = (a - b).abs();
            if d > max_abs {
                max_abs = d;
            }
            let scale = a.abs().max(b.abs()).max(1e-6);
            let rel = d / scale;
            if rel > max_rel {
                max_rel = rel;
            }
        }
        eprintln!("rmsnorm diff: max_abs={:.6e} max_rel={:.6e}", max_abs, max_rel);
        // Tight tolerance — f32 RMSNorm with 5120 elements should match
        // within a few machine_eps.
        assert!(
            max_rel < 1e-3,
            "GPU result diverges from CPU golden: max_rel={}",
            max_rel
        );
    }
}
