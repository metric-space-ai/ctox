//! SiLU-and-multiply fused activation (SwiGLU MLP).
//!
//! `y = silu(gate) * up` where `silu(x) = x * sigmoid(x)`. Fusing the
//! activation with the elementwise multiply avoids two full sweeps of
//! the hidden tensor — one read+write per op becomes a single pass.
//!
//! See `rmsnorm` for the canonical kernel-wrapper conventions this
//! module follows (one `.cu` per module, PTX cache via `OnceLock`, no
//! stream sync).

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

// PTX blob comes from the parent module's auto-generated registry.
use super::SILU_MUL_PTX;

/// Threads per block for the per-element launch. 256 is the canonical
/// sweet-spot for memory-bound elementwise kernels on SM_86 — enough
/// warps (8) to hide DRAM latency, small enough to leave occupancy
/// headroom for the caller.
const BLOCK_DIM: u32 = 256;

static SILU_MUL_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static SILU_MUL_BF16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Load the silu_mul PTX module once and pull out the named function.
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
    let ptx_src = std::str::from_utf8(SILU_MUL_PTX)
        .map_err(|e| anyhow!("silu_mul.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module silu_mul.ptx: {:?}", e))?;
    let f = module
        .load_function(entry)
        .map_err(|e| anyhow!("load_function {}: {:?}", entry, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// Validate that gate/up/y share the same shape and return the numel.
/// Empty tensors are allowed and report `0` so the caller can early-out
/// without launching a zero-grid kernel.
fn validate_shapes<T: ctox_cuda_primitives::tensor::TensorElem>(
    gate: &CudaTensor<T>,
    up: &CudaTensor<T>,
    y: &CudaTensor<T>,
) -> Result<usize> {
    if gate.shape() != up.shape() {
        return Err(anyhow!(
            "silu_mul: gate.shape {:?} != up.shape {:?}",
            gate.shape(),
            up.shape()
        ));
    }
    if gate.shape() != y.shape() {
        return Err(anyhow!(
            "silu_mul: gate.shape {:?} != y.shape {:?}",
            gate.shape(),
            y.shape()
        ));
    }
    Ok(gate.numel())
}

fn launch_config_for(numel: usize) -> LaunchConfig {
    let grid = numel.div_ceil(BLOCK_DIM as usize).max(1) as u32;
    LaunchConfig {
        grid_dim: (grid, 1, 1),
        block_dim: (BLOCK_DIM, 1, 1),
        shared_mem_bytes: 0,
    }
}

/// `y ← silu(gate) * up`, f32 in/out.
///
/// All three tensors must have identical shape. Does not synchronize
/// the stream — callers sync at phase boundaries.
pub fn launch_silu_mul_f32(
    device: &Arc<DeviceContext>,
    gate: &CudaTensor<f32>,
    up: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    let numel = validate_shapes(gate, up, y)?;
    if numel == 0 {
        return Ok(());
    }
    let cfg = launch_config_for(numel);
    let f = load_fn(device, &SILU_MUL_F32_FN, "silu_mul_f32")?;
    let stream = device.raw().default_stream();
    let numel_i32 = numel as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(gate.buf())
        .arg(up.buf())
        .arg(y.buf_mut())
        .arg(&numel_i32);
    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("silu_mul_f32 launch (numel={}): {:?}", numel, e))?;
    Ok(())
}

/// `y ← silu(gate) * up`, bf16 in/out with f32 math internally.
///
/// All three tensors must have identical shape. Does not synchronize
/// the stream — callers sync at phase boundaries.
pub fn launch_silu_mul_bf16(
    device: &Arc<DeviceContext>,
    gate: &CudaTensor<bf16>,
    up: &CudaTensor<bf16>,
    y: &mut CudaTensor<bf16>,
) -> Result<()> {
    let numel = validate_shapes(gate, up, y)?;
    if numel == 0 {
        return Ok(());
    }
    let cfg = launch_config_for(numel);
    let f = load_fn(device, &SILU_MUL_BF16_FN, "silu_mul_bf16")?;
    let stream = device.raw().default_stream();
    let numel_i32 = numel as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(gate.buf())
        .arg(up.buf())
        .arg(y.buf_mut())
        .arg(&numel_i32);
    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("silu_mul_bf16 launch (numel={}): {:?}", numel, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU reference: `silu(gate) * up` in f32.
    fn silu_mul_cpu_f32(gate: &[f32], up: &[f32], y: &mut [f32]) {
        for i in 0..gate.len() {
            let g = gate[i];
            let silu = g / (1.0 + (-g).exp());
            y[i] = silu * up[i];
        }
    }

    /// Deterministic pseudo-random via simple LCG so the test is host-
    /// independent and reproducible across architectures.
    fn lcg_iter(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        // Map to roughly [-1, 1].
        ((*seed >> 16) as f32 / 32768.0) - 1.0
    }

    /// Device-backed end-to-end. Ignored by default — run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture silu_mul_vs_cpu_golden
    ///
    /// Shape [n_tokens=8, intermediate_dim=13824] matches the
    /// Qwen3.5-27B MLP hidden dimension.
    #[test]
    #[ignore]
    fn silu_mul_vs_cpu_golden() {
        let n_tokens = 8usize;
        let intermediate_dim = 13824usize;
        let numel = n_tokens * intermediate_dim;

        let mut seed: u32 = 0x9E3779B9;
        let gate_host: Vec<f32> = (0..numel).map(|_| lcg_iter(&mut seed)).collect();
        let up_host: Vec<f32> = (0..numel).map(|_| lcg_iter(&mut seed)).collect();

        // CPU golden reference.
        let mut y_cpu = vec![0.0f32; numel];
        silu_mul_cpu_f32(&gate_host, &up_host, &mut y_cpu);

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));

        // -------- f32 path --------
        {
            let gate = CudaTensor::<f32>::from_host(
                dev.clone(),
                vec![n_tokens, intermediate_dim],
                &gate_host,
            )
            .expect("upload gate f32");
            let up = CudaTensor::<f32>::from_host(
                dev.clone(),
                vec![n_tokens, intermediate_dim],
                &up_host,
            )
            .expect("upload up f32");
            let mut y = CudaTensor::<f32>::zeros(
                dev.clone(),
                vec![n_tokens, intermediate_dim],
            )
            .expect("alloc y f32");

            launch_silu_mul_f32(&dev, &gate, &up, &mut y).expect("launch f32");
            dev.synchronize().expect("sync f32");
            let y_gpu = y.to_host().expect("download y f32");

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
                "silu_mul f32  diff: max_abs={:.6e} max_rel={:.6e}",
                max_abs, max_rel
            );
            assert!(
                max_rel < 1e-3,
                "f32 GPU diverges from CPU golden: max_rel={}",
                max_rel
            );
        }

        // -------- bf16 path --------
        {
            // Quantize inputs to bf16 *first*, then upcast back to f32
            // for the CPU reference. This isolates the kernel's math
            // error from the representation error of the inputs —
            // otherwise the CPU golden is unfairly precise and the
            // comparison measures bf16 storage error rather than kernel
            // fidelity. Per-element bf16 round-trip is ~2^-8 relative.
            let gate_bf16: Vec<bf16> =
                gate_host.iter().map(|&v| bf16::from_f32(v)).collect();
            let up_bf16: Vec<bf16> =
                up_host.iter().map(|&v| bf16::from_f32(v)).collect();
            let gate_f32_from_bf16: Vec<f32> =
                gate_bf16.iter().map(|v| v.to_f32()).collect();
            let up_f32_from_bf16: Vec<f32> =
                up_bf16.iter().map(|v| v.to_f32()).collect();
            let mut y_cpu_bf16ref = vec![0.0f32; numel];
            silu_mul_cpu_f32(
                &gate_f32_from_bf16,
                &up_f32_from_bf16,
                &mut y_cpu_bf16ref,
            );

            let gate = CudaTensor::<bf16>::from_host(
                dev.clone(),
                vec![n_tokens, intermediate_dim],
                &gate_bf16,
            )
            .expect("upload gate bf16");
            let up = CudaTensor::<bf16>::from_host(
                dev.clone(),
                vec![n_tokens, intermediate_dim],
                &up_bf16,
            )
            .expect("upload up bf16");
            let mut y = CudaTensor::<bf16>::zeros(
                dev.clone(),
                vec![n_tokens, intermediate_dim],
            )
            .expect("alloc y bf16");

            launch_silu_mul_bf16(&dev, &gate, &up, &mut y).expect("launch bf16");
            dev.synchronize().expect("sync bf16");
            let y_gpu_bf16 = y.to_host().expect("download y bf16");
            let y_gpu: Vec<f32> = y_gpu_bf16.iter().map(|v| v.to_f32()).collect();

            // bf16 has ~7 bits of mantissa — error budget is dominated
            // by the final `__float2bfloat16` round-to-nearest-even on
            // the output. Against a bf16-input CPU reference we expect
            // max_rel on the order of 2^-7 ≈ 8e-3, but the task allows
            // 5e-3 because the math intermediates are f32 and most
            // outputs land cleanly.
            let mut max_abs = 0.0f32;
            let mut max_rel = 0.0f32;
            for (a, b) in y_cpu_bf16ref.iter().zip(y_gpu.iter()) {
                let d = (a - b).abs();
                if d > max_abs {
                    max_abs = d;
                }
                let scale = a.abs().max(b.abs()).max(1e-3);
                let rel = d / scale;
                if rel > max_rel {
                    max_rel = rel;
                }
            }
            eprintln!(
                "silu_mul bf16 diff: max_abs={:.6e} max_rel={:.6e}",
                max_abs, max_rel
            );
            assert!(
                max_rel < 5e-3,
                "bf16 GPU diverges from CPU golden: max_rel={}",
                max_rel
            );
        }
    }
}
