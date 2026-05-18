// Origin: CTOX
// License: Apache-2.0

//! MoE indexed-matmul re-bench with persistent buffers.
//!
//! The Stage-3.6 verifier measured 10.2 ms per dispatch at Qwen3.6
//! shape — but that was ENTIRELY buffer-alloc-dominated: each call
//! created a fresh 150 MiB MTLBuffer for the all-experts-stacked
//! weight tensor via newBufferWithBytes (which copies). Real
//! production-path cost (after the BufferPool is populated once at
//! session start) is dramatically lower; this bin measures it.
//!
//! Without an accurate MoE-dispatch cost we cannot project an
//! integrated tg128 number, so this is on the critical path for the
//! Stage-4 Standing Status Card.

#![cfg(feature = "metal")]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;
use std::time::Instant;

use anyhow::{anyhow, Result};
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLComputeCommandEncoder,
    MTLDevice,
};

use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::{
        moe_router::router_softmax_top_k,
        mul_mv_id_q4_k::{record_mul_mv_id_q4_k_f32, MulMvIdQ4KF32Kernel},
        q4_k::{synth_block_q4_k, BlockQ4K, BLOCK_Q4_K_BYTES, QK_K},
    },
    runtime::{BufferPool, MetalRuntime},
};

fn xs(s: &mut u32) -> u32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    *s
}
fn synth_input(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed.wrapping_mul(0x9E37_79B1).wrapping_add(0xDEAD_BEEF);
    (0..n)
        .map(|_| ((xs(&mut s) as f32) / (u32::MAX as f32)) * 2.0 - 1.0)
        .collect()
}
fn synth_experts(n_experts: usize, m: usize, k: usize, seed: u32) -> Vec<BlockQ4K> {
    (0..n_experts * m * (k / QK_K))
        .map(|i| synth_block_q4_k(seed.wrapping_add(i as u32 * 7919)))
        .collect()
}

fn run_chained(
    rt: &MetalRuntime,
    pool: &BufferPool,
    kernel: &MulMvIdQ4KF32Kernel,
    n_experts: usize,
    m: usize,
    k: usize,
    n_tokens: usize,
    n_expert_used: usize,
) -> Result<f64> {
    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("cmd nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("enc nil"))?;
    record_mul_mv_id_q4_k_f32(
        &enc,
        kernel,
        pool.buf("expert_weights")?,
        pool.buf("input")?,
        pool.buf("ids")?,
        pool.buf("out")?,
        n_experts,
        m,
        k,
        n_tokens,
        n_expert_used,
    )?;
    enc.endEncoding();
    let t0 = Instant::now();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };
    Ok(t0.elapsed().as_secs_f64())
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-moe-indexed-bench");
    println!("== MoE indexed-matmul, persistent buffers ==");
    let rt = MetalRuntime::new()?;
    let kernel = MulMvIdQ4KF32Kernel::new(&rt, /*nsg=*/ 4)?;

    // Qwen3.6 expert FFN shape (per-layer, decode):
    //   n_experts=256, top-k=8, hidden=2048, intermediate=512
    // For the matmul itself: m=intermediate (or hidden for down), k=hidden (or intermediate).
    let n_experts = 256usize;
    let n_expert_used = 8usize;
    let n_tokens = 1usize;

    // Three matmul shapes per MoE FFN (one per gate/up/down):
    let cases: &[(&str, usize, usize)] = &[
        ("gate (intermed × hidden)", 512, 2048),
        ("up   (intermed × hidden)", 512, 2048),
        ("down (hidden × intermed)", 2048, 512),
    ];

    for &(label, m, k) in cases {
        let weights = synth_experts(n_experts, m, k, 0xCAFE_BABE);
        let input = synth_input(n_tokens * k, 0xFEED_FACE);

        // Synthetic logits → top-k.
        let logits = synth_input(n_experts, 0xBE11C0DE);
        let mut idx_buf = vec![0u32; n_expert_used];
        let mut w_buf = vec![0f32; n_expert_used];
        router_softmax_top_k(&logits, n_expert_used, &mut idx_buf, &mut w_buf);
        let ids: Vec<i32> = idx_buf.iter().map(|&v| v as i32).collect();

        let mut pool = BufferPool::new(&rt);
        let blocks_per_row = k / QK_K;
        let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
        let weight_bytes = n_experts * m * row_bytes;
        let w_b =
            unsafe { std::slice::from_raw_parts(weights.as_ptr() as *const u8, weight_bytes) };
        // 150 MiB alloc-once cost — the whole point.
        let t_alloc = Instant::now();
        pool.copy_in("expert_weights", w_b)?;
        let alloc_dt = t_alloc.elapsed().as_secs_f64();

        let i_b = unsafe {
            std::slice::from_raw_parts(input.as_ptr() as *const u8, input.len() * size_of::<f32>())
        };
        pool.copy_in("input", i_b)?;
        let ids_b = unsafe {
            std::slice::from_raw_parts(ids.as_ptr() as *const u8, ids.len() * size_of::<i32>())
        };
        pool.copy_in("ids", ids_b)?;
        pool.alloc_zeroed("out", n_tokens * n_expert_used * m * size_of::<f32>())?;

        // Warmup
        for _ in 0..5 {
            let _ = run_chained(&rt, &pool, &kernel, n_experts, m, k, n_tokens, n_expert_used)?;
        }

        // Measure
        let mut times = Vec::with_capacity(50);
        for _ in 0..50 {
            times.push(run_chained(&rt, &pool, &kernel, n_experts, m, k, n_tokens, n_expert_used)?);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let min = times[0];
        let med = times[25];
        let p95 = times[47];

        // Compute bandwidth: 8 experts × m × k × 0.5625 bytes weight + k × 4 input + 8 × m × 4 out
        let weight_traffic = n_expert_used * (m * k * 9) / 16;
        let input_traffic = k * 4;
        let out_traffic = n_expert_used * m * 4;
        let traffic = (weight_traffic + input_traffic + out_traffic) as f64;
        let bw_min = traffic / min / 1e9;
        let flops = 2.0 * n_expert_used as f64 * m as f64 * k as f64;
        let gflops_min = flops / min / 1e9;

        println!();
        println!(
            "  {label:<28}   m={m:>5} k={k:>5}   alloc-once={:>5.0} ms",
            alloc_dt * 1e3
        );
        println!(
            "  → min={:>7.1} µs   med={:>7.1} µs   p95={:>7.1} µs   {:>5.1} GB/s   {:>6.1} GFLOPS",
            min * 1e6,
            med * 1e6,
            p95 * 1e6,
            bw_min,
            gflops_min
        );
    }

    println!();
    println!("== Per-token MoE FFN cost projection (3 matmuls per layer × 40 layers) ==");
    println!("  See bench above; sum the three cells × 40 layers for a tg128 lower-bound contribution.");
    Ok(())
}
