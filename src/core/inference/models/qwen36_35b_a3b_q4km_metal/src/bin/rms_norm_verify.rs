// Origin: CTOX
// License: Apache-2.0

//! Per-op verifier for the `rms_norm_f32` MSL port.
//!
//! Drives `metal_port::ops::rms_norm::dispatch_rms_norm_f32` against
//! synthetic input shapes representative of Qwen3.6-35B-A3B's
//! kernel ABI (hidden=2048, head_dim=256, 1 row per token in the
//! decode hot path) and byte-compares against the f32 CPU reference.
//!
//! Acceptance: max_abs_err ≤ 1e-5 over the whole tensor. (RMSNorm is
//! a parallel reduce + scale, so f32 fp accumulator order can drift
//! by a few ULP even with -fno-fast-math; 1e-5 covers that without
//! masking real bugs.)

#![cfg(feature = "metal")]

use anyhow::{bail, Result};
use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::rms_norm::{dispatch_rms_norm_f32, rms_norm_f32_cpu, RmsNormF32Kernel},
    runtime::MetalRuntime,
};

fn run_shape(rt: &MetalRuntime, kernel: &RmsNormF32Kernel, rows: usize, cols: usize, eps: f32) -> Result<()> {
    // Deterministic pseudo-random fill so the run is reproducible.
    let n = rows * cols;
    let mut x = Vec::with_capacity(n);
    let mut state: u32 = 0xC0FE_BAADu32 ^ (rows as u32).wrapping_mul(2654435761) ^ (cols as u32);
    for _ in 0..n {
        // xorshift32
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        // map to ~U(-1, 1)
        let v = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        x.push(v);
    }

    let gpu = dispatch_rms_norm_f32(rt, kernel, &x, rows, cols, eps)?;
    // CPU reference is per-row.
    let mut cpu = Vec::with_capacity(n);
    for r in 0..rows {
        let row = &x[r * cols..(r + 1) * cols];
        cpu.extend(rms_norm_f32_cpu(row, eps));
    }

    let mut max_abs = 0.0_f64;
    let mut sum_abs = 0.0_f64;
    let mut sum_sq = 0.0_f64;
    for (g, c) in gpu.iter().zip(cpu.iter()) {
        let d = (*g as f64 - *c as f64).abs();
        if d > max_abs {
            max_abs = d;
        }
        sum_abs += d;
        sum_sq += d * d;
    }
    let mean_abs = sum_abs / (n as f64);
    let rms = (sum_sq / n as f64).sqrt();

    println!(
        "  shape rows={rows:>5} cols={cols:>5} eps={eps:>9.2e}  \
         max_abs={max_abs:.3e}  mean_abs={mean_abs:.3e}  rms={rms:.3e}"
    );
    if max_abs > 1e-5 {
        bail!(
            "rms_norm_f32 GPU vs CPU drift exceeded 1e-5 \
             (rows={rows} cols={cols} max_abs={max_abs:.3e})"
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-rms-norm-verify");
    let rt = MetalRuntime::new()?;
    let kernel = RmsNormF32Kernel::new(&rt)?;

    // Shape sweep: row counts that exercise the prefill / decode hot
    // path, cols pinned to the Qwen3.6 hidden_size (2048) and to the
    // per-head-dim (256, 64) where intermediate RMSNorms might be
    // applied. cols=131 is an off-power-of-two sanity check.
    let shapes = [
        (1, 2048, 1e-6),     // single-token decode, full hidden
        (8, 2048, 1e-6),     // micro-batch decode
        (32, 2048, 1e-6),    // small prefill chunk
        (128, 2048, 1e-6),   // larger prefill chunk
        (1, 256, 1e-6),      // per-head dim
        (1, 64, 1e-6),       // RoPE rotated lanes
        (4, 131, 1e-6),      // odd width
        (1, 4096, 1e-6),     // Q-projection width
        (1, 2048, 0.0),      // eps=0 boundary
    ];
    let mut failures = 0;
    for (rows, cols, eps) in shapes {
        if let Err(err) = run_shape(&rt, &kernel, rows, cols, eps) {
            eprintln!("  FAIL: {err:#}");
            failures += 1;
        }
    }
    if failures > 0 {
        bail!("{failures} shape(s) failed");
    }
    println!("OK — all shapes within 1e-5 absolute drift");
    Ok(())
}
