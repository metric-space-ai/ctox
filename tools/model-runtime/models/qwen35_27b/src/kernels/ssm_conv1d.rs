//! Depth-wise 1-D causal conv + fused SiLU, bf16 in/out.
//!
//! Used by the Qwen3.5 hybrid layer's GDN block on the qkvg pre-mix
//! step. Consumes a rolling (K-1)-row state cache; emits a new
//! (K-1)-row state_out (may alias the input `state`).
//!
//! See `rmsnorm` for the canonical kernel-wrapper conventions
//! (one `.cu` per module, PTX cache via `OnceLock`, no stream sync).

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

// PTX blob comes from the parent module's auto-generated registry.
// The .cu file is now `ssm_conv.cu` — a shim that `#include`s the
// vendored upstream ggml-cuda/ssm-conv.cu (for `ggml_cuda_op_silu_single`
// and the f32 kernel family) and adds the two bf16 entry-point kernels
// the GDN block uses. build.rs names the blob after the .cu stem, so
// it's `SSM_CONV_PTX`.
use super::SSM_CONV_PTX;

/// Threads per block along the channel axis. 256 matches the other
/// memory-bound elementwise kernels (`silu_mul`, `residual`) so the
/// GDN block shares an occupancy profile.
const BLOCK_DIM: u32 = 256;

static SSM_CONV1D_BF16_FN: OnceLock<CudaFunction> = OnceLock::new();
static SSM_CONV1D_STATE_UPDATE_BF16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Load the ssm_conv1d PTX module once and pull out the named function.
/// Both entry points live in the same compiled PTX; we reload the
/// module lazily per-entry-point — same pattern as `silu_mul`.
fn load_fn(
    device: &Arc<DeviceContext>,
    cache: &OnceLock<CudaFunction>,
    entry: &str,
) -> Result<CudaFunction> {
    if let Some(f) = cache.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(SSM_CONV_PTX)
        .map_err(|e| anyhow!("ssm_conv.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module ssm_conv.ptx: {:?}", e))?;
    let f = module
        .load_function(entry)
        .map_err(|e| anyhow!("load_function {}: {:?}", entry, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// `y[t, c] = silu(sum_{k=0..K-1} w[k, c] * concat(state, x)[t + K-1 - k, c])`
/// for `t in [0, n_tokens)`, `c in [0, n_channels)`; plus
/// `state_out` set to the last `K-1` rows of `concat(state, x)`.
///
/// Shapes:
///   * `x`:          `[n_tokens, n_channels]`  bf16 row-major
///   * `state`:      `[K-1,       n_channels]`  bf16 row-major
///   * `state_out`:  `[K-1,       n_channels]`  bf16 (may alias `state`)
///   * `w`:          `[K,         n_channels]`  bf16 row-major
///   * `y`:          `[n_tokens,  n_channels]`  bf16 (pre-allocated)
///
/// The two-launch structure (conv kernel → state-update kernel) makes
/// `state_out == state` aliasing safe: the state-update kernel runs
/// strictly after the conv kernel on the same stream, so all reads of
/// `state` inside the conv have completed before any writes to it.
///
/// Does not synchronize the stream. Caller syncs at phase boundary.
pub fn launch_ssm_conv1d_bf16(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<bf16>,
    state: &CudaTensor<bf16>,
    state_out: &mut CudaTensor<bf16>,
    w: &CudaTensor<bf16>,
    y: &mut CudaTensor<bf16>,
    kernel_size: usize,
) -> Result<()> {
    // Shape validation. Clear error messages — silent corruption on
    // mismatched shapes is a nightmare to debug downstream.
    if kernel_size < 2 {
        return Err(anyhow!(
            "ssm_conv1d: kernel_size must be >= 2, got {}",
            kernel_size
        ));
    }
    if x.shape().len() != 2 {
        return Err(anyhow!(
            "ssm_conv1d: x must be 2D [n_tokens, n_channels], got {:?}",
            x.shape()
        ));
    }
    if w.shape().len() != 2 {
        return Err(anyhow!(
            "ssm_conv1d: w must be 2D [K, n_channels], got {:?}",
            w.shape()
        ));
    }
    if state.shape().len() != 2 {
        return Err(anyhow!(
            "ssm_conv1d: state must be 2D [K-1, n_channels], got {:?}",
            state.shape()
        ));
    }
    if state_out.shape() != state.shape() {
        return Err(anyhow!(
            "ssm_conv1d: state_out.shape {:?} != state.shape {:?}",
            state_out.shape(),
            state.shape()
        ));
    }
    if y.shape() != x.shape() {
        return Err(anyhow!(
            "ssm_conv1d: y.shape {:?} != x.shape {:?}",
            y.shape(),
            x.shape()
        ));
    }
    let n_tokens = x.shape()[0];
    let n_channels = x.shape()[1];
    if w.shape() != [kernel_size, n_channels] {
        return Err(anyhow!(
            "ssm_conv1d: w.shape {:?} != [K={}, n_channels={}]",
            w.shape(),
            kernel_size,
            n_channels
        ));
    }
    if state.shape() != [kernel_size - 1, n_channels] {
        return Err(anyhow!(
            "ssm_conv1d: state.shape {:?} != [K-1={}, n_channels={}]",
            state.shape(),
            kernel_size - 1,
            n_channels
        ));
    }
    if n_tokens == 0 || n_channels == 0 {
        // Nothing to do. Skip both launches.
        return Ok(());
    }

    let n_tokens_i32 = n_tokens as i32;
    let n_channels_i32 = n_channels as i32;
    let k_i32 = kernel_size as i32;

    let grid_c = (n_channels as u32).div_ceil(BLOCK_DIM).max(1);
    let stream = device.raw().default_stream();

    // --- Kernel 1: conv + fused silu → y ---
    {
        let cfg = LaunchConfig {
            grid_dim: (grid_c, n_tokens as u32, 1),
            block_dim: (BLOCK_DIM, 1, 1),
            shared_mem_bytes: 0,
        };
        let f = load_fn(device, &SSM_CONV1D_BF16_FN, "ssm_conv1d_bf16")?;
        let mut launcher = stream.launch_builder(&f);
        launcher
            .arg(x.buf())
            .arg(state.buf())
            .arg(w.buf())
            .arg(y.buf_mut())
            .arg(&n_tokens_i32)
            .arg(&n_channels_i32)
            .arg(&k_i32);
        unsafe { launcher.launch(cfg) }.map_err(|e| {
            anyhow!(
                "ssm_conv1d_bf16 conv launch (n_tokens={} n_channels={} K={}): {:?}",
                n_tokens,
                n_channels,
                kernel_size,
                e
            )
        })?;
    }

    // --- Kernel 2: state ring rotation → state_out ---
    // Runs on the same stream, so it starts only after the conv
    // kernel's reads of `state` have completed. Safe even when
    // `state_out` aliases `state`.
    {
        let k_m1 = kernel_size - 1;
        let cfg = LaunchConfig {
            grid_dim: (grid_c, k_m1 as u32, 1),
            block_dim: (BLOCK_DIM, 1, 1),
            shared_mem_bytes: 0,
        };
        let f = load_fn(
            device,
            &SSM_CONV1D_STATE_UPDATE_BF16_FN,
            "ssm_conv1d_state_update_bf16",
        )?;
        let mut launcher = stream.launch_builder(&f);
        launcher
            .arg(x.buf())
            .arg(state.buf())
            .arg(state_out.buf_mut())
            .arg(&n_tokens_i32)
            .arg(&n_channels_i32)
            .arg(&k_i32);
        unsafe { launcher.launch(cfg) }.map_err(|e| {
            anyhow!(
                "ssm_conv1d_state_update_bf16 launch (K-1={} n_channels={}): {:?}",
                k_m1,
                n_channels,
                e
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU reference: depth-wise 1-D causal conv + SiLU, math in f32.
    /// Caller must pre-quantize inputs to bf16 (then upcast back to
    /// f32) so that representation error is excluded from the
    /// kernel-fidelity comparison.
    #[allow(clippy::too_many_arguments)]
    fn ssm_conv1d_cpu_f32(
        x: &[f32],
        state: &[f32],
        w: &[f32],
        y: &mut [f32],
        state_out: &mut [f32],
        n_tokens: usize,
        n_channels: usize,
        kernel_size: usize,
    ) {
        let k = kernel_size;
        let k_m1 = k - 1;

        // Build concat(state, x) conceptually via index-remap (no
        // alloc). `pad_row(r)` returns a slice of length n_channels.
        let pad_row = |r: usize| -> &[f32] {
            if r < k_m1 {
                &state[r * n_channels..(r + 1) * n_channels]
            } else {
                let xi = r - k_m1;
                &x[xi * n_channels..(xi + 1) * n_channels]
            }
        };

        // y: conv + silu per (t, c).
        for t in 0..n_tokens {
            for c in 0..n_channels {
                let mut acc = 0.0f32;
                for ki in 0..k {
                    let src_idx = t + k_m1 - ki;
                    let xv = pad_row(src_idx)[c];
                    let wv = w[ki * n_channels + c];
                    acc += wv * xv;
                }
                let silu = acc / (1.0 + (-acc).exp());
                y[t * n_channels + c] = silu;
            }
        }

        // state_out = last K-1 rows of concat(state, x).
        for i in 0..k_m1 {
            let src_idx = n_tokens + i;
            for c in 0..n_channels {
                state_out[i * n_channels + c] = pad_row(src_idx)[c];
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
    ///       --ignored --nocapture ssm_conv1d_vs_cpu_golden
    ///
    /// Shape [n_tokens=16, n_channels=5120] with K=4 roughly matches
    /// the Qwen3.5 GDN pre-conv on a small verify batch — enough to
    /// exercise the multi-block channel fan-out and the state-read
    /// path for the first few tokens.
    #[test]
    #[ignore]
    fn ssm_conv1d_vs_cpu_golden() {
        let n_tokens = 16usize;
        let n_channels = 5120usize;
        let kernel_size = 4usize;
        let k_m1 = kernel_size - 1;

        let mut seed: u32 = 0x9E3779B9;
        let x_host_f32: Vec<f32> =
            (0..n_tokens * n_channels).map(|_| lcg_iter(&mut seed)).collect();
        let state_host_f32: Vec<f32> =
            (0..k_m1 * n_channels).map(|_| lcg_iter(&mut seed)).collect();
        // Keep weights moderate (~[-0.3, 0.3]) so the post-sum magnitude
        // doesn't saturate SiLU — matches the scale of trained conv1d
        // weights and keeps the numerical comparison meaningful.
        let w_host_f32: Vec<f32> = (0..kernel_size * n_channels)
            .map(|_| 0.3 * lcg_iter(&mut seed))
            .collect();

        // Quantize inputs to bf16, then upcast back to f32 for the
        // CPU reference. This isolates kernel error from input
        // representation error (per-element bf16 round-trip ≈ 2^-8).
        let x_bf16: Vec<bf16> = x_host_f32.iter().map(|&v| bf16::from_f32(v)).collect();
        let state_bf16: Vec<bf16> =
            state_host_f32.iter().map(|&v| bf16::from_f32(v)).collect();
        let w_bf16: Vec<bf16> = w_host_f32.iter().map(|&v| bf16::from_f32(v)).collect();
        let x_from_bf16: Vec<f32> = x_bf16.iter().map(|v| v.to_f32()).collect();
        let state_from_bf16: Vec<f32> = state_bf16.iter().map(|v| v.to_f32()).collect();
        let w_from_bf16: Vec<f32> = w_bf16.iter().map(|v| v.to_f32()).collect();

        let mut y_cpu = vec![0.0f32; n_tokens * n_channels];
        let mut state_out_cpu = vec![0.0f32; k_m1 * n_channels];
        ssm_conv1d_cpu_f32(
            &x_from_bf16,
            &state_from_bf16,
            &w_from_bf16,
            &mut y_cpu,
            &mut state_out_cpu,
            n_tokens,
            n_channels,
            kernel_size,
        );

        // Device run.
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let x =
            CudaTensor::<bf16>::from_host(dev.clone(), vec![n_tokens, n_channels], &x_bf16)
                .expect("upload x");
        let state = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![k_m1, n_channels],
            &state_bf16,
        )
        .expect("upload state");
        let w = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![kernel_size, n_channels],
            &w_bf16,
        )
        .expect("upload w");
        let mut y = CudaTensor::<bf16>::zeros(dev.clone(), vec![n_tokens, n_channels])
            .expect("alloc y");
        let mut state_out = CudaTensor::<bf16>::zeros(dev.clone(), vec![k_m1, n_channels])
            .expect("alloc state_out");

        launch_ssm_conv1d_bf16(&dev, &x, &state, &mut state_out, &w, &mut y, kernel_size)
            .expect("launch");
        dev.synchronize().expect("sync");

        let y_gpu: Vec<f32> = y
            .to_host()
            .expect("download y")
            .iter()
            .map(|v| v.to_f32())
            .collect();
        let state_out_gpu: Vec<f32> = state_out
            .to_host()
            .expect("download state_out")
            .iter()
            .map(|v| v.to_f32())
            .collect();

        // --- y diff ---
        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        for (a, b) in y_cpu.iter().zip(y_gpu.iter()) {
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
            "ssm_conv1d y diff:         max_abs={:.6e} max_rel={:.6e}",
            max_abs, max_rel
        );
        assert!(
            max_rel < 5e-3,
            "GPU y diverges from CPU golden: max_rel={} max_abs={}",
            max_rel,
            max_abs
        );

        // --- state_out diff (must be bit-exact vs CPU reference: we
        // only copy bf16 values, no arithmetic) ---
        let mut state_max_abs = 0.0f32;
        for (a, b) in state_out_cpu.iter().zip(state_out_gpu.iter()) {
            let d = (a - b).abs();
            if d > state_max_abs {
                state_max_abs = d;
            }
        }
        eprintln!("ssm_conv1d state_out diff: max_abs={:.6e}", state_max_abs);
        assert_eq!(
            state_max_abs, 0.0,
            "state_out must be bit-exact vs CPU reference: max_abs={}",
            state_max_abs
        );
    }
}
