// Origin: CTOX
// License: Apache-2.0

//! End-to-end MoE-decode verifier.  Builds a synthetic mini-MoE
//! (n_experts=16, top-k=4, m=128, k=256) — much smaller than the
//! 256/8/512/2048 of Qwen3.6 so the CPU reference finishes fast —
//! drives the Rust router + indexed Q4_K matvec, and byte-compares
//! the per-slot expert outputs against an f32 CPU dequant + matmul.
//!
//! Plus a Qwen3.6-shaped bench (n_experts=256, top-k=8, m=512, k=2048)
//! that runs only the GPU side and times steady-state per-token cost.

#![cfg(feature = "metal")]

use std::time::Instant;

use anyhow::{bail, Result};

use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::{
        moe_router::router_softmax_top_k,
        mul_mv_id_q4_k::{
            cpu_reference_mul_mv_id_q4_k_f32, dispatch_mul_mv_id_q4_k_f32, MulMvIdQ4KF32Kernel,
        },
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
fn synth_experts(n_experts: usize, m: usize, k: usize, seed: u32) -> Vec<BlockQ4K> {
    (0..n_experts * m * (k / QK_K))
        .map(|i| synth_block_q4_k(seed.wrapping_add(i as u32 * 7919)))
        .collect()
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-mul-mv-id-q4k-verify");
    let rt = MetalRuntime::new()?;
    let kernel = MulMvIdQ4KF32Kernel::new(&rt, /*nsg=*/ 4)?;

    // ── Correctness on a small synthetic MoE block ───────────────────
    let n_experts = 16usize;
    let n_expert_used = 4usize;
    let m = 128usize;
    let k = 256usize;
    let n_tokens = 1usize;

    let weights = synth_experts(n_experts, m, k, 0xC0FE_BABE);
    let input = synth_input(n_tokens * k, 0xFEED_FACE);

    // Synthetic router logits (random over n_experts).
    let logits = synth_input(n_tokens * n_experts, 0xBE11_CAFE);
    let mut ids = vec![0i32; n_tokens * n_expert_used];
    let mut weights_topk = vec![0f32; n_tokens * n_expert_used];
    for t in 0..n_tokens {
        let mut idx_buf = vec![0u32; n_expert_used];
        let mut w_buf = vec![0f32; n_expert_used];
        router_softmax_top_k(
            &logits[t * n_experts..(t + 1) * n_experts],
            n_expert_used,
            &mut idx_buf,
            &mut w_buf,
        );
        for slot in 0..n_expert_used {
            ids[t * n_expert_used + slot] = idx_buf[slot] as i32;
            weights_topk[t * n_expert_used + slot] = w_buf[slot];
        }
    }

    let gpu = dispatch_mul_mv_id_q4_k_f32(
        &rt,
        &kernel,
        &weights,
        &input,
        &ids,
        n_experts,
        m,
        k,
        n_tokens,
        n_expert_used,
    )?;
    let cpu = cpu_reference_mul_mv_id_q4_k_f32(
        &weights,
        &input,
        &ids,
        n_experts,
        m,
        k,
        n_tokens,
        n_expert_used,
    );

    let mut max_abs = 0.0_f64;
    for (g, c) in gpu.iter().zip(cpu.iter()) {
        let d = (*g as f64 - *c as f64).abs();
        if d > max_abs {
            max_abs = d;
        }
    }
    println!(
        "  correctness  n_experts={n_experts} top_k={n_expert_used} m={m} k={k} n_tokens={n_tokens}  max_abs={max_abs:.3e}"
    );
    if max_abs > 1e-3 {
        bail!("MoE indexed matvec correctness drift {max_abs:.3e} > 1e-3");
    }

    // Caller-side weighted-sum check: assemble the per-token MoE output.
    let mut moe_out_gpu = vec![0.0f32; n_tokens * m];
    for t in 0..n_tokens {
        for slot in 0..n_expert_used {
            let w = weights_topk[t * n_expert_used + slot];
            for r in 0..m {
                moe_out_gpu[t * m + r] += w * gpu[(t * n_expert_used + slot) * m + r];
            }
        }
    }

    let mut moe_out_cpu = vec![0.0f32; n_tokens * m];
    for t in 0..n_tokens {
        for slot in 0..n_expert_used {
            let w = weights_topk[t * n_expert_used + slot];
            for r in 0..m {
                moe_out_cpu[t * m + r] += w * cpu[(t * n_expert_used + slot) * m + r];
            }
        }
    }

    let mut max_abs2 = 0.0_f64;
    for (g, c) in moe_out_gpu.iter().zip(moe_out_cpu.iter()) {
        let d = (*g as f64 - *c as f64).abs();
        if d > max_abs2 {
            max_abs2 = d;
        }
    }
    println!("  weighted-sum end-to-end MoE  max_abs={max_abs2:.3e}");
    if max_abs2 > 1e-3 {
        bail!("MoE end-to-end drift {max_abs2:.3e} > 1e-3");
    }

    // ── Bench at Qwen3.6 shape (no CPU ref — too big) ────────────────
    println!();
    println!("--- bench at Qwen3.6-35B-A3B FFN expert shape ---");
    let n_experts_q = 256usize;
    let n_expert_used_q = 8usize;
    let m_q = 512usize;
    let k_q = 2048usize;

    let weights_q = synth_experts(n_experts_q, m_q, k_q, 0xBE11C0DE);
    let input_q = synth_input(k_q, 0xCAFEBABE);
    let logits_q = synth_input(n_experts_q, 0xFADE_BABE);
    let mut ids_q = vec![0i32; n_expert_used_q];
    let mut wts_q = vec![0f32; n_expert_used_q];
    {
        let mut idx_buf = vec![0u32; n_expert_used_q];
        let mut w_buf = vec![0f32; n_expert_used_q];
        router_softmax_top_k(&logits_q, n_expert_used_q, &mut idx_buf, &mut w_buf);
        for slot in 0..n_expert_used_q {
            ids_q[slot] = idx_buf[slot] as i32;
            wts_q[slot] = w_buf[slot];
        }
    }

    // Warmup
    for _ in 0..5 {
        let _ = dispatch_mul_mv_id_q4_k_f32(
            &rt, &kernel, &weights_q, &input_q, &ids_q,
            n_experts_q, m_q, k_q, 1, n_expert_used_q,
        )?;
    }
    let mut samples = Vec::with_capacity(30);
    for _ in 0..30 {
        let t = Instant::now();
        let _ = dispatch_mul_mv_id_q4_k_f32(
            &rt, &kernel, &weights_q, &input_q, &ids_q,
            n_experts_q, m_q, k_q, 1, n_expert_used_q,
        )?;
        samples.push(t.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = samples[0];
    let med = samples[15];
    // Bytes streamed: 8 experts × m × k × 0.5625 + k × 4 + 8 × m × 4
    let weight_bytes = n_expert_used_q * (m_q * k_q * 9) / 16;
    let input_bytes = k_q * 4;
    let out_bytes = n_expert_used_q * m_q * 4;
    let traffic = (weight_bytes + input_bytes + out_bytes) as f64;
    let bw_min = traffic / min / 1e9;
    let flops = 2.0 * n_expert_used_q as f64 * m_q as f64 * k_q as f64;
    let gflops = flops / min / 1e9;

    println!(
        "  bench  n_experts={n_experts_q} top_k={n_expert_used_q} m={m_q} k={k_q}   min={:>7.1} µs   med={:>7.1} µs   {:>5.1} GB/s  {:>6.1} GFLOPS",
        min * 1e6,
        med * 1e6,
        bw_min,
        gflops,
    );

    println!();
    println!("OK");
    Ok(())
}
