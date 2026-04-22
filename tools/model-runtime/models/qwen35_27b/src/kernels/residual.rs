//! Residual-add element-wise kernel wrapper.
//!
//! `y = x + z` over the full flat buffer — the two residual additions
//! at every transformer layer:
//!   * `hidden = hidden + attn_out`
//!   * `hidden = hidden + mlp_out`
//!
//! See `rmsnorm` for the canonical kernel-wrapper conventions this
//! module follows (one `.cu` per module, PTX cache via `OnceLock`, no
//! stream sync).
//!
//! Same launch shape as `silu_mul`: element-per-thread, block=256,
//! grid=ceil(numel/256).

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

// PTX blob comes from the parent module's auto-generated registry.
use super::RESIDUAL_PTX;

/// Threads per block for the per-element launch. 256 is the canonical
/// sweet-spot for memory-bound elementwise kernels on SM_86 — matches
/// `silu_mul` so the two ops share an occupancy profile.
const BLOCK_DIM: u32 = 256;

static RESIDUAL_ADD_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static RESIDUAL_ADD_BF16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Load the residual PTX module once and pull out the named function.
/// Both f32 and bf16 entry points live in the same compiled PTX, so we
/// reload the module lazily per-entry-point rather than caching the
/// module itself (cudarc's `CudaModule` is the heavy handle; the
/// `CudaFunction` we actually need is cheap to clone).
fn load_fn(
    device: &Arc<DeviceContext>,
    cache: &OnceLock<CudaFunction>,
    entry: &str,
) -> Result<CudaFunction> {
    if let Some(f) = cache.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(RESIDUAL_PTX)
        .map_err(|e| anyhow!("residual.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module residual.ptx: {:?}", e))?;
    let f = module
        .load_function(entry)
        .map_err(|e| anyhow!("load_function {}: {:?}", entry, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// Validate that x/z/y share the same shape and return the numel.
/// Empty tensors are allowed and report `0` so the caller can early-out
/// without launching a zero-grid kernel.
fn validate_shapes<T: ctox_cuda_primitives::tensor::TensorElem>(
    x: &CudaTensor<T>,
    z: &CudaTensor<T>,
    y: &CudaTensor<T>,
) -> Result<usize> {
    if x.shape() != z.shape() {
        return Err(anyhow!(
            "residual_add: x.shape {:?} != z.shape {:?}",
            x.shape(),
            z.shape()
        ));
    }
    if x.shape() != y.shape() {
        return Err(anyhow!(
            "residual_add: x.shape {:?} != y.shape {:?}",
            x.shape(),
            y.shape()
        ));
    }
    Ok(x.numel())
}

fn launch_config_for(numel: usize) -> LaunchConfig {
    let grid = numel.div_ceil(BLOCK_DIM as usize).max(1) as u32;
    LaunchConfig {
        grid_dim: (grid, 1, 1),
        block_dim: (BLOCK_DIM, 1, 1),
        shared_mem_bytes: 0,
    }
}

/// `y ← x + z`, f32 element-wise.
///
/// All three tensors must have identical shape. Does not synchronize
/// the stream — callers sync at phase boundaries.
pub fn launch_residual_add_f32(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    z: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    let numel = validate_shapes(x, z, y)?;
    if numel == 0 {
        return Ok(());
    }
    let cfg = launch_config_for(numel);
    let f = load_fn(device, &RESIDUAL_ADD_F32_FN, "residual_add_f32")?;
    let stream = device.raw().default_stream();
    let numel_i32 = numel as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(x.buf())
        .arg(z.buf())
        .arg(y.buf_mut())
        .arg(&numel_i32);
    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("residual_add_f32 launch (numel={}): {:?}", numel, e))?;
    Ok(())
}

/// `y ← x + z`, bf16 in/out with f32 accum internally.
///
/// All three tensors must have identical shape. Does not synchronize
/// the stream — callers sync at phase boundaries.
pub fn launch_residual_add_bf16(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<bf16>,
    z: &CudaTensor<bf16>,
    y: &mut CudaTensor<bf16>,
) -> Result<()> {
    let numel = validate_shapes(x, z, y)?;
    if numel == 0 {
        return Ok(());
    }
    let cfg = launch_config_for(numel);
    let f = load_fn(device, &RESIDUAL_ADD_BF16_FN, "residual_add_bf16")?;
    let stream = device.raw().default_stream();
    let numel_i32 = numel as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(x.buf())
        .arg(z.buf())
        .arg(y.buf_mut())
        .arg(&numel_i32);
    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("residual_add_bf16 launch (numel={}): {:?}", numel, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random via simple LCG so the test is host-
    /// independent and reproducible across architectures.
    fn lcg_iter(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        // Map to roughly [-1, 1].
        ((*seed >> 16) as f32 / 32768.0) - 1.0
    }

    /// Device-backed end-to-end. Ignored by default — run with:
    ///   cargo test -p ctox-engine-cuda --features cuda --release -- \
    ///       --ignored --nocapture residual_add_vs_cpu_golden
    ///
    /// Shape [n_tokens=8, hidden=5120] matches the Qwen3.5-27B hidden
    /// dimension. Tolerance is bit-exact (max_abs == 0) on both the f32
    /// and bf16 paths: f32 add has no rounding difference between CPU
    /// and GPU for IEEE-754 semantics, and the bf16 path rounds once on
    /// the final store — against a CPU reference that also rounds once
    /// via `bf16::from_f32` (round-to-nearest-even) the results match
    /// bit-for-bit.
    #[test]
    #[ignore]
    fn residual_add_vs_cpu_golden() {
        let n_tokens = 8usize;
        let hidden_dim = 5120usize;
        let numel = n_tokens * hidden_dim;

        let mut seed: u32 = 0x9E3779B9;
        let x_host: Vec<f32> = (0..numel).map(|_| lcg_iter(&mut seed)).collect();
        let z_host: Vec<f32> = (0..numel).map(|_| lcg_iter(&mut seed)).collect();

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));

        // -------- f32 path --------
        {
            // CPU golden — plain f32 add.
            let y_cpu: Vec<f32> = x_host
                .iter()
                .zip(z_host.iter())
                .map(|(a, b)| a + b)
                .collect();

            let x = CudaTensor::<f32>::from_host(dev.clone(), vec![n_tokens, hidden_dim], &x_host)
                .expect("upload x f32");
            let z = CudaTensor::<f32>::from_host(dev.clone(), vec![n_tokens, hidden_dim], &z_host)
                .expect("upload z f32");
            let mut y = CudaTensor::<f32>::zeros(dev.clone(), vec![n_tokens, hidden_dim])
                .expect("alloc y f32");

            launch_residual_add_f32(&dev, &x, &z, &mut y).expect("launch f32");
            dev.synchronize().expect("sync f32");
            let y_gpu = y.to_host().expect("download y f32");

            let mut max_abs = 0.0f32;
            for (a, b) in y_cpu.iter().zip(y_gpu.iter()) {
                let d = (a - b).abs();
                if d > max_abs {
                    max_abs = d;
                }
            }
            eprintln!("residual_add f32  diff: max_abs={:.6e}", max_abs);
            assert_eq!(
                max_abs, 0.0,
                "f32 residual_add must be bit-exact vs CPU: max_abs={}",
                max_abs
            );
        }

        // -------- bf16 path --------
        {
            // Quantize inputs to bf16, then run both CPU and GPU on the
            // exact same bf16 bits. CPU reference computes the add in
            // f32 and stores back via `bf16::from_f32` — same rounding
            // the GPU uses in `__float2bfloat16`, so the comparison is
            // bit-exact.
            let x_bf16: Vec<bf16> = x_host.iter().map(|&v| bf16::from_f32(v)).collect();
            let z_bf16: Vec<bf16> = z_host.iter().map(|&v| bf16::from_f32(v)).collect();
            let y_cpu_bf16: Vec<bf16> = x_bf16
                .iter()
                .zip(z_bf16.iter())
                .map(|(a, b)| bf16::from_f32(a.to_f32() + b.to_f32()))
                .collect();

            let x = CudaTensor::<bf16>::from_host(dev.clone(), vec![n_tokens, hidden_dim], &x_bf16)
                .expect("upload x bf16");
            let z = CudaTensor::<bf16>::from_host(dev.clone(), vec![n_tokens, hidden_dim], &z_bf16)
                .expect("upload z bf16");
            let mut y = CudaTensor::<bf16>::zeros(dev.clone(), vec![n_tokens, hidden_dim])
                .expect("alloc y bf16");

            launch_residual_add_bf16(&dev, &x, &z, &mut y).expect("launch bf16");
            dev.synchronize().expect("sync bf16");
            let y_gpu_bf16 = y.to_host().expect("download y bf16");

            let mut max_abs = 0.0f32;
            for (a, b) in y_cpu_bf16.iter().zip(y_gpu_bf16.iter()) {
                let d = (a.to_f32() - b.to_f32()).abs();
                if d > max_abs {
                    max_abs = d;
                }
            }
            eprintln!("residual_add bf16 diff: max_abs={:.6e}", max_abs);
            assert_eq!(
                max_abs, 0.0,
                "bf16 residual_add must be bit-exact vs CPU reference: max_abs={}",
                max_abs
            );
        }
    }
}
