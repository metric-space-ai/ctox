// Origin: CTOX
// License: Apache-2.0

//! Verifier + N=8..32 sweep bench for the row-batched matvec ext family.
//!
//! Acceptance: max_abs ≤ 1e-3 against an f32 CPU dequant + matmul
//! reference (matvec-style precision since the dequant + accumulation
//! happens entirely in float lanes).

#![cfg(feature = "metal")]

use std::time::Instant;

use anyhow::{bail, Result};
use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::{
        mul_mv_ext_q4_k::{dispatch_mul_mv_ext_q4_k_f32, MulMvExtQ4KF32Kernel},
        mul_mv_q4_k::cpu_reference_mul_mv_q4_k_f32,
        q4_k::{synth_block_q4_k, BlockQ4K, QK_K},
    },
    runtime::MetalRuntime,
};

fn xs(s: &mut u32) -> u32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    *s
}
fn synth_input(elems: usize, seed: u32) -> Vec<f32> {
    let mut s = seed.wrapping_mul(0x9E37_79B1).wrapping_add(0xDEAD_BEEF);
    (0..elems)
        .map(|_| ((xs(&mut s) as f32) / (u32::MAX as f32)) * 2.0 - 1.0)
        .collect()
}
fn synth_weights(m: usize, k: usize, seed: u32) -> Vec<BlockQ4K> {
    (0..m * (k / QK_K))
        .map(|i| synth_block_q4_k(seed.wrapping_add(i as u32 * 7919)))
        .collect()
}

fn cpu_reference_n(weights: &[BlockQ4K], input: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; n * m];
    for r in 0..n {
        let row_input = &input[r * k..(r + 1) * k];
        let row_out = cpu_reference_mul_mv_q4_k_f32(weights, row_input, m, k);
        out[r * m..(r + 1) * m].copy_from_slice(&row_out);
    }
    out
}

fn run_correctness(
    rt: &MetalRuntime,
    label: &str,
    r1ptg: u32,
    nsg: u32,
    nxpsg: u32,
    m: usize,
    k: usize,
    n: usize,
) -> Result<()> {
    let kernel = MulMvExtQ4KF32Kernel::new(rt, r1ptg, nsg, nxpsg)?;
    let weights = synth_weights(m, k, 0xC0FE_BABE ^ (m as u32) ^ ((k as u32) << 8));
    let input = synth_input(n * k, 0xFEED_FACE ^ (m as u32) ^ ((k as u32) << 8));

    let gpu = dispatch_mul_mv_ext_q4_k_f32(rt, &kernel, &weights, &input, m, k, n)?;
    let cpu = cpu_reference_n(&weights, &input, m, k, n);

    let mut max_abs = 0.0_f64;
    for (g, c) in gpu.iter().zip(cpu.iter()) {
        let d = (*g as f64 - *c as f64).abs();
        if d > max_abs {
            max_abs = d;
        }
    }
    println!(
        "  correctness {label:<14} r1={r1ptg} nsg={nsg} nxpsg={nxpsg:>2}  m={m:>5} k={k:>5} n={n:>4}  max_abs={max_abs:.3e}"
    );
    if max_abs > 1e-3 {
        bail!("shape {label} m={m} k={k} n={n}: max_abs {max_abs:.3e} > 1e-3");
    }
    Ok(())
}

fn run_bench(
    rt: &MetalRuntime,
    label: &str,
    r1ptg: u32,
    nsg: u32,
    nxpsg: u32,
    m: usize,
    k: usize,
    n: usize,
) -> Result<f64> {
    let kernel = MulMvExtQ4KF32Kernel::new(rt, r1ptg, nsg, nxpsg)?;
    let weights = synth_weights(m, k, 0xBE11C0DE ^ (m as u32));
    let input = synth_input(n * k, 0xCAFEBABE ^ (n as u32));

    for _ in 0..5 {
        let _ = dispatch_mul_mv_ext_q4_k_f32(rt, &kernel, &weights, &input, m, k, n)?;
    }
    let mut samples = Vec::with_capacity(30);
    for _ in 0..30 {
        let t = Instant::now();
        let _ = dispatch_mul_mv_ext_q4_k_f32(rt, &kernel, &weights, &input, m, k, n)?;
        samples.push(t.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[15];
    let min = samples[0];
    let weight_bytes = (m * k * 9) / 16;
    let input_bytes = n * k * 4;
    let bw_min = (weight_bytes + input_bytes) as f64 / min / 1e9;
    let gflops_min = (2.0 * m as f64 * k as f64 * n as f64) / min / 1e9;
    println!(
        "  bench       {label:<14} r1={r1ptg} nsg={nsg} nxpsg={nxpsg:>2}  m={m:>5} k={k:>5} n={n:>4}   min={:>7.1} µs  med={:>7.1} µs   {:>5.1} GB/s   {:>6.1} GFLOPS",
        min * 1e6,
        median * 1e6,
        bw_min,
        gflops_min
    );
    Ok(min)
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-mul-mv-ext-q4k-verify");
    let rt = MetalRuntime::new()?;

    println!("--- correctness (Q-proj 4096×2048, sweep r1ptg + nxpsg) ---");
    for &r1 in &[2u32, 3, 4, 5] {
        for &nxpsg in &[2u32, 4, 8] {
            run_correctness(&rt, "Q-proj", r1, 4, nxpsg, 4096, 2048, r1 as usize)?;
        }
    }

    println!();
    println!("--- bench (find best r1/nxpsg/nsg per shape × N) ---");
    let shapes: &[(&str, usize, usize)] = &[
        ("Q-proj",      4096, 2048),
        ("O-proj",      2048, 4096),
        ("FFN_gate_up",  512, 2048),
    ];
    for &(label, m, k) in shapes {
        for &n in &[8usize, 16, 24, 32] {
            for &r1 in &[2u32, 3, 4, 5] {
                if n % r1 as usize != 0 {
                    continue;
                }
                for &nxpsg in &[2u32, 4, 8] {
                    let _ = run_bench(&rt, label, r1, 4, nxpsg, m, k, n)?;
                }
            }
        }
        println!();
    }
    println!("OK — see bench above for r1/nxpsg autotune");
    Ok(())
}
