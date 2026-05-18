// Origin: CTOX
// License: Apache-2.0

//! Per-op verifier for the `kernel_mul_mv_q4_K_f32` MSL port.
//!
//! Drives `metal_port::ops::mul_mv_q4_k::dispatch_mul_mv_q4_k_f32_decode`
//! against synthetic Q4_K weight blocks at the canonical Qwen3.6-35B-A3B
//! decode shapes (frozen kernel ABI in `src/model.rs`):
//!
//! ```text
//! Q-projection         M=4096, K=2048   (16 heads × head_dim=256, hidden=2048)
//! KV-projection        M=512,  K=2048   (2 KV heads × head_dim=256)
//! O-projection         M=2048, K=4096   (hidden ← Q-hidden)
//! MoE expert gate/up   M=512,  K=2048   (per-expert intermediate=512)
//! MoE expert down      M=2048, K=512    (residual ← intermediate)
//! Shared-expert gate   M=512,  K=2048   (same as MoE)
//! ```
//!
//! Plus a tiny synthetic shape (M=128, K=256) for fast iteration.
//!
//! Acceptance gate: max abs error ≤ 1e-3 between GPU and CPU
//! reference. The reference is dequant_q4_k → f32 + naive matmul,
//! both running on identical bytes — so the only error sources are
//! fp accumulator order and f16↔f32 in the Q4_K scale path.

#![cfg(feature = "metal")]

use std::time::Instant;

use anyhow::{bail, Result};
use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::{
        mul_mv_q4_k::{
            cpu_reference_mul_mv_q4_k_f32, dispatch_mul_mv_q4_k_f32_decode, MulMvQ4KF32Kernel,
        },
        q4_k::{synth_block_q4_k, BlockQ4K, QK_K},
    },
    runtime::MetalRuntime,
};

fn xs(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

fn synth_input(k: usize, seed: u32) -> Vec<f32> {
    let mut s = seed.wrapping_mul(0x9E37_79B1).wrapping_add(0xDEAD_BEEF);
    (0..k)
        .map(|_| {
            let v = xs(&mut s);
            // U(-1, 1)
            ((v as f32) / (u32::MAX as f32)) * 2.0 - 1.0
        })
        .collect()
}

fn synth_weights(m: usize, k: usize, seed: u32) -> Vec<BlockQ4K> {
    let blocks_per_row = k / QK_K;
    let total = m * blocks_per_row;
    (0..total)
        .map(|i| synth_block_q4_k(seed.wrapping_add(i as u32 * 7919)))
        .collect()
}

fn run_shape(
    rt: &MetalRuntime,
    kernel: &MulMvQ4KF32Kernel,
    label: &str,
    m: usize,
    k: usize,
) -> Result<()> {
    let weights = synth_weights(m, k, 0xC0FE_BABE ^ (m as u32) ^ ((k as u32) << 8));
    let input = synth_input(k, 0xFEED_FACE ^ (m as u32) ^ ((k as u32) << 8));

    // Warm-up: build pipeline + JIT path + DRAM fault-in.
    let _ = dispatch_mul_mv_q4_k_f32_decode(rt, kernel, &weights, &input, m, k)?;

    let t_gpu_start = Instant::now();
    let n_iters = 5usize;
    let mut gpu_last = Vec::new();
    for _ in 0..n_iters {
        gpu_last = dispatch_mul_mv_q4_k_f32_decode(rt, kernel, &weights, &input, m, k)?;
    }
    let gpu_dt = t_gpu_start.elapsed().as_secs_f64() / n_iters as f64;

    let t_cpu_start = Instant::now();
    let cpu = cpu_reference_mul_mv_q4_k_f32(&weights, &input, m, k);
    let cpu_dt = t_cpu_start.elapsed().as_secs_f64();

    let mut max_abs = 0.0_f64;
    let mut max_rel = 0.0_f64;
    let mut sum_abs = 0.0_f64;
    for (g, c) in gpu_last.iter().zip(cpu.iter()) {
        let d = (*g as f64 - *c as f64).abs();
        if d > max_abs {
            max_abs = d;
        }
        let denom = (c.abs() as f64).max(1e-6);
        let r = d / denom;
        if r > max_rel {
            max_rel = r;
        }
        sum_abs += d;
    }
    let mean_abs = sum_abs / (m as f64);

    // Bandwidth + compute math.
    // weights bytes per dispatch = m * k / 256 * 144 = m * k * 0.5625
    let weight_bytes = (m * k * 9) / 16;
    let input_bytes = k * 4;
    let traffic_gb = (weight_bytes + input_bytes) as f64 / 1e9;
    let bw_gbs = traffic_gb / gpu_dt;
    // 2 × m × k FMA ops, count as flops
    let flops = 2.0 * m as f64 * k as f64;
    let gflops = flops / gpu_dt / 1e9;

    println!(
        "  {label:<14}  m={m:>5} k={k:>5}  \
         gpu={gpu_dt_us:>7.1} µs  cpu_ref={cpu_dt_ms:>6.1} ms  \
         max_abs={max_abs:.3e}  max_rel={max_rel:.3e}  mean_abs={mean_abs:.3e}  \
         {bw_gbs:>5.1} GB/s  {gflops:>5.1} GFLOPS",
        gpu_dt_us = gpu_dt * 1e6,
        cpu_dt_ms = cpu_dt * 1e3,
    );

    if max_abs > 1e-3 {
        bail!(
            "shape {label} failed correctness: max_abs={max_abs:.3e} > 1e-3"
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-mul-mv-q4k-verify");
    let rt = MetalRuntime::new()?;
    // nsg = 4 is upstream's typical autotuned default for Q4_K matvec
    // on Apple Silicon. Stage 5 sweeps {1, 2, 4, 8} per shape.
    let kernel = MulMvQ4KF32Kernel::new(&rt, 4)?;

    let shapes: &[(&str, usize, usize)] = &[
        ("synth_tiny",   128,  256),
        ("Q-proj",      4096, 2048),
        ("KV-proj",      512, 2048),
        ("O-proj",      2048, 4096),
        ("FFN_gate_up",  512, 2048),
        ("FFN_down",    2048,  512),
    ];

    let mut failed = 0usize;
    for &(label, m, k) in shapes {
        if let Err(err) = run_shape(&rt, &kernel, label, m, k) {
            eprintln!("  FAIL: {err:#}");
            failed += 1;
        }
    }
    if failed > 0 {
        bail!("{failed} shape(s) failed correctness");
    }
    println!("OK — all shapes within 1e-3 absolute drift");
    Ok(())
}
