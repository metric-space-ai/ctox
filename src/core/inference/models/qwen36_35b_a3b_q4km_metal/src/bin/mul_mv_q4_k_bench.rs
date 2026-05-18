// Origin: CTOX
// License: Apache-2.0

//! Isolated microbench for `kernel_mul_mv_q4_K_f32` on this M5.
//! Pre-allocates buffers once per (shape, nsg) cell, dispatches in a
//! tight commit/wait loop, reports median + p95 + min latency,
//! effective bandwidth, and GFLOPS.
//!
//! Reference roofline (from `qwen36-35b-a3b-q4km-metal-roofline`):
//!   sustained DRAM read   ≈ 60.6 GB/s   (1 GiB shared)
//!   DRAM read+write       ≈ 121.2 GB/s
//!
//! For Q4_K_M matvec:
//!   bytes read per call   = m × k × 9/16 + k × 4
//!   bytes written         = m × 4
//!   reaching 50 GB/s read = ~83 % of measured peak → strong evidence
//!                            the kernel is bandwidth-bound, as expected.

#![cfg(feature = "metal")]

use anyhow::Result;

use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::{
        mul_mv_q4_k::MulMvQ4KF32Kernel,
        mul_mv_q4_k_bench::bench_mul_mv_q4_k_f32,
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
        .map(|_| ((xs(&mut s) as f32) / (u32::MAX as f32)) * 2.0 - 1.0)
        .collect()
}

fn synth_weights(m: usize, k: usize, seed: u32) -> Vec<BlockQ4K> {
    (0..m * (k / QK_K))
        .map(|i| synth_block_q4_k(seed.wrapping_add(i as u32 * 7919)))
        .collect()
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-mul-mv-q4k-bench");
    let rt = MetalRuntime::new()?;

    // Canonical Qwen3.6-35B-A3B decode-time matvec shapes (frozen ABI
    // in src/model.rs):
    //   Q-proj         : 4096 × 2048
    //   KV-proj        :  512 × 2048
    //   O-proj         : 2048 × 4096
    //   FFN gate/up    :  512 × 2048   (per expert; we hit this 8 + 1 times per token)
    //   FFN down       : 2048 ×  512   (per expert)
    //   LM-head*       :248320 × 2048   (often Q6_K, sometimes Q4_K)
    let shapes: &[(&str, usize, usize)] = &[
        ("Q-proj",      4096, 2048),
        ("KV-proj",      512, 2048),
        ("O-proj",      2048, 4096),
        ("FFN_gate_up",  512, 2048),
        ("FFN_down",    2048,  512),
    ];

    let nsg_sweep: &[u32] = &[1, 2, 4, 8];

    println!(
        "{:<14} {:>5} {:>5} {:>4}   {:>9}  {:>9}  {:>9}   {:>7}  {:>7}",
        "shape", "m", "k", "nsg", "median µs", "p95 µs", "min µs", "GB/s", "GFLOPS"
    );
    for &(label, m, k) in shapes {
        let weights = synth_weights(m, k, 0xC0FE_BABE ^ (m as u32) ^ ((k as u32) << 8));
        let input = synth_input(k, 0xFEED_FACE ^ (m as u32) ^ ((k as u32) << 8));
        let mut best_dispatch = f64::INFINITY;
        let mut best_nsg = 0u32;
        for &nsg in nsg_sweep {
            // Skip nsg values that don't divide m × N_R0_Q4_K cleanly.
            if m % (nsg as usize * 2) != 0 {
                continue;
            }
            let kernel = MulMvQ4KF32Kernel::new(&rt, nsg)?;
            let r = bench_mul_mv_q4_k_f32(
                &rt,
                &kernel,
                &weights,
                &input,
                m,
                k,
                /*iters=*/ 50,
                /*warmup=*/ 10,
            )?;
            println!(
                "{:<14} {:>5} {:>5} {:>4}   {:>9.1}  {:>9.1}  {:>9.1}   {:>7.1}  {:>7.1}",
                label, m, k, nsg, r.gpu_dispatch_us, r.p95_us, r.min_us, r.bw_gbs, r.gflops
            );
            if r.gpu_dispatch_us < best_dispatch {
                best_dispatch = r.gpu_dispatch_us;
                best_nsg = nsg;
            }
        }
        println!(
            "  → best nsg={} for {} ({:.1} µs, ~{:.1} GB/s)",
            best_nsg,
            label,
            best_dispatch,
            // Rough bw recompute from best dispatch.
            (((m * k * 9) / 16 + k * 4) as f64) / (best_dispatch / 1e6) / 1e9
        );
    }

    println!();
    println!("# context");
    println!(
        "# Roofline (qwen36-35b-a3b-q4km-metal-roofline @ 1 GiB shared):\n\
         #   sustained DRAM read   ≈ 60.6 GB/s\n\
         #   sustained read+write  ≈ 121.2 GB/s\n\
         #\n\
         # Q4_K_M matvec is read-dominated (m × k × 0.5625 weight bytes\n\
         # + k × 4 input bytes ≫ m × 4 output bytes), so the right\n\
         # ceiling to compare against is ~60 GB/s. Anything ≥ 50 GB/s\n\
         # in the column above is ≥ 83 % of measured peak → kernel\n\
         # is bandwidth-bound and further gains require either a\n\
         # different memory layout or fusion with a downstream op."
    );
    Ok(())
}
