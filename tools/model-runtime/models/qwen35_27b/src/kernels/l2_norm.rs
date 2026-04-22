//! L2 row-normalize Rust wrapper. bf16 in/out with f32 math.
//!
//! `y[i, :] = x[i, :] / sqrt(sum(x[i, :]^2) + eps)` per row.
//!
//! Used on Q and K (one row per head × token) before the GDN
//! recurrence. See `rmsnorm` for the canonical kernel-wrapper
//! conventions (one `.cu` per module, PTX cache via `OnceLock`, no
//! stream sync).

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

// PTX blob comes from the parent module's auto-generated registry.
use super::L2_NORM_PTX;

/// One-shot cache for the loaded kernel function. Same rationale as
/// `rmsnorm`: module load is expensive, hot-path calls this many
/// times per forward pass.
static L2_NORM_BF16_FN: OnceLock<CudaFunction> = OnceLock::new();

fn l2_norm_bf16_fn(device: &Arc<DeviceContext>) -> Result<CudaFunction> {
    if let Some(f) = L2_NORM_BF16_FN.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(L2_NORM_PTX)
        .map_err(|e| anyhow!("l2_norm.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module l2_norm.ptx: {:?}", e))?;
    let f = module
        .load_function("l2_norm_bf16")
        .map_err(|e| anyhow!("load_function l2_norm_bf16: {:?}", e))?;
    let _ = L2_NORM_BF16_FN.set(f.clone());
    Ok(f)
}

/// `y[i, :] ← x[i, :] / sqrt(sum(x[i, :]^2) + eps)`, bf16 in/out.
///
/// Shapes:
///   * `x`: `[n_rows, n_cols]` bf16 row-major
///   * `y`: `[n_rows, n_cols]` bf16 (pre-allocated output, same shape)
///
/// Does not synchronize the stream. Caller syncs at phase boundary.
pub fn launch_l2_norm_bf16(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<bf16>,
    y: &mut CudaTensor<bf16>,
    eps: f32,
) -> Result<()> {
    if x.shape().len() != 2 {
        return Err(anyhow!(
            "l2_norm: x must be 2D [n_rows, n_cols], got {:?}",
            x.shape()
        ));
    }
    if y.shape() != x.shape() {
        return Err(anyhow!(
            "l2_norm: y.shape {:?} != x.shape {:?}",
            y.shape(),
            x.shape()
        ));
    }
    let n_rows = x.shape()[0];
    let n_cols = x.shape()[1];
    if n_rows == 0 || n_cols == 0 {
        // Nothing to do. Avoid launching a 0-block grid.
        return Ok(());
    }

    // Launch: one block per row; block_dim = min(n_cols, 1024) rounded
    // up to a multiple of 32 so every warp is active.
    let mut block_dim = n_cols.min(1024);
    block_dim = block_dim.div_ceil(32) * 32;
    let cfg = LaunchConfig {
        grid_dim: (n_rows as u32, 1, 1),
        block_dim: (block_dim as u32, 1, 1),
        shared_mem_bytes: 0,
    };

    let f = l2_norm_bf16_fn(device)?;
    let stream = device.raw().default_stream();
    let n_cols_i32 = n_cols as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(x.buf())
        .arg(y.buf_mut())
        .arg(&n_cols_i32)
        .arg(&eps);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "l2_norm_bf16 launch (n_rows={} n_cols={}): {:?}",
            n_rows,
            n_cols,
            e
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU reference: per-row L2 normalize. Math in f32 to match the
    /// GPU kernel's intermediate precision; caller is responsible for
    /// bf16-quantizing the inputs first so the comparison isolates
    /// kernel error from input representation error.
    fn l2_norm_cpu_f32(x: &[f32], y: &mut [f32], n_rows: usize, n_cols: usize, eps: f32) {
        for r in 0..n_rows {
            let row = &x[r * n_cols..(r + 1) * n_cols];
            let sum_sq: f32 = row.iter().map(|v| v * v).sum::<f32>();
            let scale = 1.0 / (sum_sq + eps).sqrt();
            let y_row = &mut y[r * n_cols..(r + 1) * n_cols];
            for i in 0..n_cols {
                y_row[i] = row[i] * scale;
            }
        }
    }

    /// Deterministic pseudo-random via simple LCG so the test is host-
    /// independent.
    fn lcg_iter(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        ((*seed >> 16) as f32 / 32768.0) - 1.0
    }

    /// Device-backed end-to-end. Ignored by default — run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture l2_norm_vs_cpu_golden
    ///
    /// Shape [n_rows=16, n_cols=128] matches 16 Qwen3.5 attention
    /// heads x 128 head_dim — i.e., one row per per-head Q/K vector.
    #[test]
    #[ignore]
    fn l2_norm_vs_cpu_golden() {
        let n_rows = 16usize;
        let n_cols = 128usize;
        let numel = n_rows * n_cols;
        let eps = 1e-6f32;

        let mut seed: u32 = 0x9E3779B9;
        let x_host_f32: Vec<f32> = (0..numel).map(|_| lcg_iter(&mut seed)).collect();

        // Quantize inputs to bf16 first, then upcast back to f32 for
        // the CPU reference — this isolates kernel math error from
        // bf16 input representation error.
        let x_bf16: Vec<bf16> = x_host_f32.iter().map(|&v| bf16::from_f32(v)).collect();
        let x_f32_from_bf16: Vec<f32> = x_bf16.iter().map(|v| v.to_f32()).collect();

        let mut y_cpu = vec![0.0f32; numel];
        l2_norm_cpu_f32(&x_f32_from_bf16, &mut y_cpu, n_rows, n_cols, eps);

        // Device run.
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let x = CudaTensor::<bf16>::from_host(dev.clone(), vec![n_rows, n_cols], &x_bf16)
            .expect("upload x");
        let mut y = CudaTensor::<bf16>::zeros(dev.clone(), vec![n_rows, n_cols])
            .expect("alloc y");

        launch_l2_norm_bf16(&dev, &x, &mut y, eps).expect("launch");
        dev.synchronize().expect("sync");

        let y_gpu_bf16 = y.to_host().expect("download y");
        let y_gpu: Vec<f32> = y_gpu_bf16.iter().map(|v| v.to_f32()).collect();

        // bf16 floor: final `__float2bfloat16` rounds to ~2^-7 ≈ 8e-3
        // relative on the worst element. Task tolerance is 5e-3 max_rel
        // OR 2e-3 max_abs — whichever is looser.
        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        for (a, b) in y_cpu.iter().zip(y_gpu.iter()) {
            let d = (a - b).abs();
            if d > max_abs {
                max_abs = d;
            }
            let scale_v = a.abs().max(b.abs()).max(1e-3);
            let rel = d / scale_v;
            if rel > max_rel {
                max_rel = rel;
            }
        }
        eprintln!(
            "l2_norm bf16 diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs, max_rel
        );
        // "Whichever is looser" — pass if EITHER bound holds.
        let ok = max_rel < 5e-3 || max_abs < 2e-3;
        assert!(
            ok,
            "GPU result diverges from CPU golden: max_rel={} max_abs={}",
            max_rel, max_abs
        );
    }
}
