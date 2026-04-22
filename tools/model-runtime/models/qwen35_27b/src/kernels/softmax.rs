//! softmax — numerically stable row softmax over f32 tensors.
//!
//! See `kernels/softmax.cu` for the math. Mirrors the rmsnorm wrapper
//! pattern: one block per row, warp-shuffle fan-in reduction.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use super::SOFTMAX_PTX;

static SOFTMAX_F32_FN: OnceLock<CudaFunction> = OnceLock::new();

fn softmax_f32_fn(device: &Arc<DeviceContext>) -> Result<CudaFunction> {
    if let Some(f) = SOFTMAX_F32_FN.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(SOFTMAX_PTX)
        .map_err(|e| anyhow!("softmax.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module softmax.ptx: {:?}", e))?;
    let f = module
        .load_function("softmax_f32")
        .map_err(|e| anyhow!("load_function softmax_f32: {:?}", e))?;
    let _ = SOFTMAX_F32_FN.set(f.clone());
    Ok(f)
}

/// `y[r, :] = softmax(x[r, :])` for every row.
///
/// Shapes:
///   * `x`, `y`: `[n_rows, n_cols]` f32 row-major
///
/// Does not synchronize the stream.
pub fn launch_softmax_f32(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    if x.shape().len() != 2 {
        return Err(anyhow!(
            "softmax: x must be 2D [n_rows, n_cols], got {:?}",
            x.shape()
        ));
    }
    if y.shape() != x.shape() {
        return Err(anyhow!(
            "softmax: y.shape {:?} != x.shape {:?}",
            y.shape(),
            x.shape()
        ));
    }
    let n_rows = x.shape()[0];
    let n_cols = x.shape()[1];
    if n_rows == 0 || n_cols == 0 {
        return Ok(());
    }

    // block_dim = min(n_cols, 1024) rounded up to a warp.
    let mut block_dim = n_cols.min(1024);
    block_dim = block_dim.div_ceil(32) * 32;
    let cfg = LaunchConfig {
        grid_dim: (n_rows as u32, 1, 1),
        block_dim: (block_dim as u32, 1, 1),
        shared_mem_bytes: 0,
    };

    let f = softmax_f32_fn(device)?;
    let stream = device.raw().default_stream();
    let n_cols_i32 = n_cols as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher.arg(x.buf()).arg(y.buf_mut()).arg(&n_cols_i32);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "softmax_f32 launch (n_rows={} n_cols={}): {:?}",
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

    fn softmax_cpu(x: &[f32], y: &mut [f32], n_cols: usize) {
        let n_rows = x.len() / n_cols;
        for r in 0..n_rows {
            let row_x = &x[r * n_cols..(r + 1) * n_cols];
            let row_y = &mut y[r * n_cols..(r + 1) * n_cols];
            let mut m = f32::NEG_INFINITY;
            for &v in row_x {
                if v > m {
                    m = v;
                }
            }
            let mut sum = 0.0f32;
            for (i, &v) in row_x.iter().enumerate() {
                let e = (v - m).exp();
                row_y[i] = e;
                sum += e;
            }
            let inv = 1.0 / sum;
            for v in row_y.iter_mut() {
                *v *= inv;
            }
        }
    }

    /// Runs on A6000 CI only. `cargo test --features cuda -- --ignored softmax_vs_cpu_golden`.
    #[test]
    #[ignore]
    fn softmax_vs_cpu_golden() {
        // Qwen3.5 vocab = 151936. 32 rows exercises both multi-warp
        // fan-in and heavy column stride per thread.
        let n_rows = 32usize;
        let n_cols = 151936usize;

        let mut seed: u32 = 0xDEADBEEF;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            // Pre-softmax logits are typically in [-10, 10] range. Push
            // beyond that to stress max-subtract stability.
            ((seed >> 16) as f32 / 32768.0 - 1.0) * 15.0
        };
        let x_host: Vec<f32> = (0..n_rows * n_cols).map(|_| rand_f()).collect();

        let mut y_cpu = vec![0.0f32; n_rows * n_cols];
        softmax_cpu(&x_host, &mut y_cpu, n_cols);

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let x = CudaTensor::<f32>::from_host(dev.clone(), vec![n_rows, n_cols], &x_host)
            .expect("upload x");
        let mut y = CudaTensor::<f32>::zeros(dev.clone(), vec![n_rows, n_cols])
            .expect("alloc y");

        launch_softmax_f32(&dev, &x, &mut y).expect("launch");
        dev.synchronize().expect("sync");

        let y_gpu = y.to_host().expect("download y");

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
        eprintln!(
            "softmax diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs, max_rel
        );
        assert!(
            max_rel < 1e-4,
            "GPU softmax diverges from CPU golden: max_rel={}",
            max_rel
        );
    }
}
