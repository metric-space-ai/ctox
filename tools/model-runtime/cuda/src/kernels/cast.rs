//! Bulk dtype conversion kernels.
//!
//! The layer stack straddles dtypes — rmsnorm runs in f32 while the
//! activation stream is bf16/f16 — so we need cheap element-wise casts
//! on the hot path. This module exposes the four combinations we
//! actually wire up:
//!
//!   * `launch_cast_bf16_to_f32`
//!   * `launch_cast_f32_to_bf16`
//!   * `launch_cast_f16_to_f32`
//!   * `launch_cast_f32_to_f16`
//!
//! The f32→half rounding matches the `half` crate's `from_f32`
//! (round-to-nearest-even), so a bf16→f32→bf16 (or f16→f32→f16) round
//! trip against values that were representable in the source dtype to
//! start with is bit-exact.
//!
//! See `rmsnorm` for the canonical kernel-wrapper conventions this
//! module follows (one `.cu` per module, PTX cache via `OnceLock`, no
//! stream sync). Launch shape is element-per-thread, block=256,
//! grid=ceil(numel/256), matching `silu_mul` and `residual`.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::{bf16, f16};

use crate::device::DeviceContext;
use crate::tensor::CudaTensor;

// PTX blob comes from the parent module's auto-generated registry.
use super::CAST_PTX;

const BLOCK_DIM: u32 = 256;

static CAST_BF16_TO_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static CAST_F32_TO_BF16_FN: OnceLock<CudaFunction> = OnceLock::new();
static CAST_F16_TO_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static CAST_F32_TO_F16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Load the cast PTX module once and pull out the named function. All
/// four entry points live in the same compiled PTX, so we reload the
/// module lazily per-entry-point rather than caching the module itself
/// (cudarc's `CudaModule` is the heavy handle; the `CudaFunction` we
/// actually need is cheap to clone).
fn load_fn(
    device: &Arc<DeviceContext>,
    cache: &OnceLock<CudaFunction>,
    entry: &str,
) -> Result<CudaFunction> {
    if let Some(f) = cache.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(CAST_PTX)
        .map_err(|e| anyhow!("cast.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module cast.ptx: {:?}", e))?;
    let f = module
        .load_function(entry)
        .map_err(|e| anyhow!("load_function {}: {:?}", entry, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// Require equal shapes and a non-zero element count, returning the
/// numel. Callers pre-allocate `y` matching `x`'s shape.
fn validate_shapes<Tx: crate::tensor::TensorElem, Ty: crate::tensor::TensorElem>(
    x: &CudaTensor<Tx>,
    y: &CudaTensor<Ty>,
) -> Result<usize> {
    if x.shape() != y.shape() {
        return Err(anyhow!(
            "cast: x.shape {:?} != y.shape {:?}",
            x.shape(),
            y.shape()
        ));
    }
    let numel = x.numel();
    if numel == 0 {
        return Err(anyhow!("cast: numel must be > 0"));
    }
    Ok(numel)
}

fn launch_config_for(numel: usize) -> LaunchConfig {
    let grid = numel.div_ceil(BLOCK_DIM as usize).max(1) as u32;
    LaunchConfig {
        grid_dim: (grid, 1, 1),
        block_dim: (BLOCK_DIM, 1, 1),
        shared_mem_bytes: 0,
    }
}

/// `y ← (f32) x`, bf16 → f32. Lossless (bf16 is a strict subset of f32).
pub fn launch_cast_bf16_to_f32(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<bf16>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    let numel = validate_shapes(x, y)?;
    let cfg = launch_config_for(numel);
    let f = load_fn(device, &CAST_BF16_TO_F32_FN, "cast_bf16_to_f32")?;
    let stream = device.raw().default_stream();
    let numel_i32 = numel as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher.arg(x.buf()).arg(y.buf_mut()).arg(&numel_i32);
    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("cast_bf16_to_f32 launch (numel={}): {:?}", numel, e))?;
    Ok(())
}

/// `y ← (bf16) x`, f32 → bf16. Rounds to nearest even, matching the
/// `half` crate's `bf16::from_f32`.
pub fn launch_cast_f32_to_bf16(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<bf16>,
) -> Result<()> {
    let numel = validate_shapes(x, y)?;
    let cfg = launch_config_for(numel);
    let f = load_fn(device, &CAST_F32_TO_BF16_FN, "cast_f32_to_bf16")?;
    let stream = device.raw().default_stream();
    let numel_i32 = numel as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher.arg(x.buf()).arg(y.buf_mut()).arg(&numel_i32);
    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("cast_f32_to_bf16 launch (numel={}): {:?}", numel, e))?;
    Ok(())
}

/// `y ← (f32) x`, f16 → f32. Lossless (f16 is a strict subset of f32).
pub fn launch_cast_f16_to_f32(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f16>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    let numel = validate_shapes(x, y)?;
    let cfg = launch_config_for(numel);
    let f = load_fn(device, &CAST_F16_TO_F32_FN, "cast_f16_to_f32")?;
    let stream = device.raw().default_stream();
    let numel_i32 = numel as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher.arg(x.buf()).arg(y.buf_mut()).arg(&numel_i32);
    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("cast_f16_to_f32 launch (numel={}): {:?}", numel, e))?;
    Ok(())
}

/// `y ← (f16) x`, f32 → f16. Rounds to nearest even, matching the
/// `half` crate's `f16::from_f32`.
pub fn launch_cast_f32_to_f16(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    let numel = validate_shapes(x, y)?;
    let cfg = launch_config_for(numel);
    let f = load_fn(device, &CAST_F32_TO_F16_FN, "cast_f32_to_f16")?;
    let stream = device.raw().default_stream();
    let numel_i32 = numel as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher.arg(x.buf()).arg(y.buf_mut()).arg(&numel_i32);
    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("cast_f32_to_f16 launch (numel={}): {:?}", numel, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random via simple LCG so the test is host-
    /// independent and reproducible across architectures.
    fn lcg_iter(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        ((*seed >> 16) as f32 / 32768.0) - 1.0
    }

    /// Device-backed end-to-end. Ignored by default — run with:
    ///   cargo test -p ctox-engine-cuda --features cuda --release -- \
    ///       --ignored --nocapture cast_roundtrip_vs_cpu_golden
    ///
    /// Shape [n_tokens=8, hidden=5120] matches the Qwen3.5-27B hidden
    /// dimension. We exercise two round trips:
    ///   * bf16 → f32 → bf16 must equal the starting bf16 bits.
    ///   * f16  → f32 → f16  must equal the starting f16  bits.
    /// Both are bit-exact: values representable in the source dtype
    /// survive promotion to f32 (lossless) and the final narrowing uses
    /// round-to-nearest-even, which on an exactly-representable input
    /// is the identity.
    #[test]
    #[ignore]
    fn cast_roundtrip_vs_cpu_golden() {
        let n_tokens = 8usize;
        let hidden_dim = 5120usize;
        let numel = n_tokens * hidden_dim;
        let shape = vec![n_tokens, hidden_dim];

        let mut seed: u32 = 0x9E3779B9;
        let f32_host: Vec<f32> = (0..numel).map(|_| lcg_iter(&mut seed)).collect();

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));

        // -------- bf16 round trip --------
        {
            // Start from bf16 bits so values are exactly representable.
            let bf16_start: Vec<bf16> = f32_host.iter().map(|&v| bf16::from_f32(v)).collect();

            let x_bf16 = CudaTensor::<bf16>::from_host(dev.clone(), shape.clone(), &bf16_start)
                .expect("upload bf16 start");
            let mut mid_f32 =
                CudaTensor::<f32>::zeros(dev.clone(), shape.clone()).expect("alloc mid f32");
            launch_cast_bf16_to_f32(&dev, &x_bf16, &mut mid_f32).expect("launch bf16->f32");

            let mut out_bf16 =
                CudaTensor::<bf16>::zeros(dev.clone(), shape.clone()).expect("alloc out bf16");
            launch_cast_f32_to_bf16(&dev, &mid_f32, &mut out_bf16).expect("launch f32->bf16");
            dev.synchronize().expect("sync bf16 roundtrip");

            let bf16_end = out_bf16.to_host().expect("download bf16 end");

            let mut max_abs = 0.0f32;
            for (a, b) in bf16_start.iter().zip(bf16_end.iter()) {
                let d = (a.to_f32() - b.to_f32()).abs();
                if d > max_abs {
                    max_abs = d;
                }
            }
            eprintln!("cast bf16 roundtrip diff: max_abs={:.6e}", max_abs);
            assert_eq!(
                max_abs, 0.0,
                "bf16→f32→bf16 must be lossless: max_abs={}",
                max_abs
            );
        }

        // -------- f16 round trip --------
        {
            // Start from f16 bits so values are exactly representable.
            // Reuse the same f32 source — LCG output fits comfortably in
            // f16's [-65504, 65504] range and its ~11-bit mantissa.
            let f16_start: Vec<f16> = f32_host.iter().map(|&v| f16::from_f32(v)).collect();

            let x_f16 = CudaTensor::<f16>::from_host(dev.clone(), shape.clone(), &f16_start)
                .expect("upload f16 start");
            let mut mid_f32 =
                CudaTensor::<f32>::zeros(dev.clone(), shape.clone()).expect("alloc mid f32");
            launch_cast_f16_to_f32(&dev, &x_f16, &mut mid_f32).expect("launch f16->f32");

            let mut out_f16 =
                CudaTensor::<f16>::zeros(dev.clone(), shape.clone()).expect("alloc out f16");
            launch_cast_f32_to_f16(&dev, &mid_f32, &mut out_f16).expect("launch f32->f16");
            dev.synchronize().expect("sync f16 roundtrip");

            let f16_end = out_f16.to_host().expect("download f16 end");

            let mut max_abs = 0.0f32;
            for (a, b) in f16_start.iter().zip(f16_end.iter()) {
                let d = (a.to_f32() - b.to_f32()).abs();
                if d > max_abs {
                    max_abs = d;
                }
            }
            eprintln!("cast f16  roundtrip diff: max_abs={:.6e}", max_abs);
            assert_eq!(
                max_abs, 0.0,
                "f16→f32→f16 must be lossless: max_abs={}",
                max_abs
            );
        }
    }
}
