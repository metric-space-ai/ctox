// Origin: CTOX
// License: Apache-2.0

//! Full-attention block smoke-test in ONE chained MTLCommandBuffer.
//!
//! Chains every record_* API for the full-attention path at synthetic
//! Qwen3.6 shape (hidden=2048, head_dim=256, num_q_heads=16,
//! num_kv_heads=2, n_tokens=1, ctx=1):
//!
//!   residual ──► rms_norm ──► norm
//!     norm × Qw ──► q_buf       (mul_mv_q4_K, m=4096, k=2048)
//!     norm × Kw ──► k_buf       (mul_mv_q4_K, m=512,  k=2048)
//!     norm × Vw ──► v_buf       (mul_mv_q4_K, m=512,  k=2048)
//!   q_buf, k_buf ──► rope_multi (in-place)
//!   --- SDPA at ctx=1 degenerates: attn = V ---
//!     norm × Gate_w ──► gate_buf (mul_mv_q4_K, m=4096, k=2048)
//!   gate_buf ──► sigmoid ──► gate_sigmoid
//!   gate_sigmoid * v_expanded ──► gated   (bin_mul; v expanded to 4096 lanes)
//!     gated × Ow ──► o_buf       (mul_mv_q4_K, m=2048, k=4096)
//!   residual + o_buf ──► residual'  (bin_add)
//!
//! All in ONE command buffer. Single commit + wait. Times the chain
//! and reports projected per-layer cost × 40 layers as a lower bound
//! on per-token decode latency.

#![cfg(feature = "metal")]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;
use std::time::Instant;

use anyhow::{anyhow, Result};
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLComputeCommandEncoder,
    MTLDevice, MTLResourceOptions,
};

use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::{
        elementwise::{
            build_mul_kernel, build_sigmoid_kernel, record_bin_contig_f32, record_unary_contig_f32,
        },
        mul_mv_q4_k::{record_mul_mv_q4_k_f32, MulMvQ4KF32Kernel},
        q4_k::{synth_block_q4_k, BlockQ4K, BLOCK_Q4_K_BYTES, QK_K},
        rms_norm::{record_rms_norm_f32, RmsNormF32Kernel},
        rope::{record_rope_multi_f32, RopeMultiF32Kernel, RopeShape},
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
fn synth_weights(m: usize, k: usize, seed: u32) -> Vec<BlockQ4K> {
    (0..m * (k / QK_K))
        .map(|i| synth_block_q4_k(seed.wrapping_add(i as u32 * 7919)))
        .collect()
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-attn-block-demo");
    println!("== Full-attention block, single chained command buffer ==");
    let rt = MetalRuntime::new()?;

    // Qwen3.6 frozen ABI shape
    const HIDDEN: usize = 2048;
    const HEAD_DIM: usize = 256;
    const NUM_Q_HEADS: usize = 16;
    const NUM_KV_HEADS: usize = 2;
    const Q_OUT: usize = NUM_Q_HEADS * HEAD_DIM; // 4096
    const KV_OUT: usize = NUM_KV_HEADS * HEAD_DIM; // 512
    const O_IN: usize = Q_OUT; // 4096
    const O_OUT: usize = HIDDEN; // 2048

    // Build kernels
    let rms = RmsNormF32Kernel::new(&rt)?;
    let mvq = MulMvQ4KF32Kernel::new(&rt, /*nsg=*/ 4)?;
    let rope = RopeMultiF32Kernel::new(&rt, /*imrope=*/ true)?;
    let sigmoid = build_sigmoid_kernel(&rt)?;
    let mul_kernel = build_mul_kernel(&rt)?;
    let add_kernel =
        ctox_qwen36_35b_a3b_q4km_metal::metal_port::ops::elementwise::build_add_kernel(&rt)?;

    // Synth weights: Q, K, V, Gate, O. All in Q4_K_M.
    let q_w = synth_weights(Q_OUT, HIDDEN, 0xBA5E_BA11);
    let k_w = synth_weights(KV_OUT, HIDDEN, 0xC0FE_BABE);
    let v_w = synth_weights(KV_OUT, HIDDEN, 0xFEED_FACE);
    let gate_w = synth_weights(Q_OUT, HIDDEN, 0xCAFE_BABE);
    let o_w = synth_weights(O_OUT, O_IN, 0xDEAD_BEEF);

    let residual = synth_input(HIDDEN, 0xFADE_BABE);

    // Build the BufferPool
    let mut pool = BufferPool::new(&rt);
    let blocks_per_row = HIDDEN / QK_K;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let q_w_bytes =
        unsafe { std::slice::from_raw_parts(q_w.as_ptr() as *const u8, Q_OUT * row_bytes) };
    pool.copy_in("q_w", q_w_bytes)?;
    let k_w_bytes =
        unsafe { std::slice::from_raw_parts(k_w.as_ptr() as *const u8, KV_OUT * row_bytes) };
    pool.copy_in("k_w", k_w_bytes)?;
    let v_w_bytes =
        unsafe { std::slice::from_raw_parts(v_w.as_ptr() as *const u8, KV_OUT * row_bytes) };
    pool.copy_in("v_w", v_w_bytes)?;
    let gate_w_bytes =
        unsafe { std::slice::from_raw_parts(gate_w.as_ptr() as *const u8, Q_OUT * row_bytes) };
    pool.copy_in("gate_w", gate_w_bytes)?;
    let o_blocks = O_IN / QK_K;
    let o_row_bytes = o_blocks * BLOCK_Q4_K_BYTES;
    let o_w_bytes =
        unsafe { std::slice::from_raw_parts(o_w.as_ptr() as *const u8, O_OUT * o_row_bytes) };
    pool.copy_in("o_w", o_w_bytes)?;
    let residual_bytes = unsafe {
        std::slice::from_raw_parts(residual.as_ptr() as *const u8, HIDDEN * size_of::<f32>())
    };
    pool.copy_in("residual", residual_bytes)?;

    pool.alloc_zeroed("norm", HIDDEN * size_of::<f32>())?;
    pool.alloc_zeroed("q_buf", Q_OUT * size_of::<f32>())?;
    pool.alloc_zeroed("k_buf", KV_OUT * size_of::<f32>())?;
    pool.alloc_zeroed("v_buf", KV_OUT * size_of::<f32>())?;
    pool.alloc_zeroed("gate_buf", Q_OUT * size_of::<f32>())?;
    pool.alloc_zeroed("gate_sigmoid", Q_OUT * size_of::<f32>())?;
    // V is 512 lanes; "expanded" to 4096 = repeat 8× over GQA group.
    // For ctx=1 SDPA simplification: just take V and broadcast to Q_OUT lanes
    // by treating each Q-head as belonging to a KV-head (group=8). For the
    // smoke test, we use a CPU pre-broadcast; production path computes this
    // inline within SDPA.
    pool.alloc_zeroed("v_expanded", Q_OUT * size_of::<f32>())?;
    pool.alloc_zeroed("attn_gated", Q_OUT * size_of::<f32>())?;
    pool.alloc_zeroed("o_buf", HIDDEN * size_of::<f32>())?;

    // Position buffer for RoPE: [n_tokens=1, 4_axes] int32. For decode at
    // pos 0 all 4 axes are 0; we pre-fill them so the kernel reads valid data.
    let pos_buf_data: Vec<i32> = vec![0i32; 4];
    let pos_bytes = unsafe {
        std::slice::from_raw_parts(
            pos_buf_data.as_ptr() as *const u8,
            pos_buf_data.len() * size_of::<i32>(),
        )
    };
    pool.copy_in("pos", pos_bytes)?;

    println!("  pool: {} buffers populated", pool.len());

    // Bench: 30 iterations of the full attn-block chain in ONE cmd buffer.
    let iters = 30;
    let warmup = 5;
    let mut times = Vec::with_capacity(iters);

    let q_rope = RopeShape {
        head_dim: HEAD_DIM as u32,
        n_heads: NUM_Q_HEADS as u32,
        n_tokens: 1,
        batch: 1,
        n_dims_rotated: 64,
        sect: [11, 11, 10, 0],
        freq_base: 1.0e7,
    };
    let k_rope = RopeShape {
        head_dim: HEAD_DIM as u32,
        n_heads: NUM_KV_HEADS as u32,
        n_tokens: 1,
        batch: 1,
        n_dims_rotated: 64,
        sect: [11, 11, 10, 0],
        freq_base: 1.0e7,
    };

    for iter in 0..(iters + warmup) {
        let cmd = rt
            .queue
            .commandBuffer()
            .ok_or_else(|| anyhow!("cmd nil"))?;
        let enc = cmd
            .computeCommandEncoder()
            .ok_or_else(|| anyhow!("enc nil"))?;

        // 1. RMSNorm pre-attn
        record_rms_norm_f32(
            &enc,
            &rms,
            &rt.device,
            pool.buf("residual")?,
            pool.buf("norm")?,
            1,
            HIDDEN,
            1e-6,
        )?;
        // 2. Q / K / V projections
        record_mul_mv_q4_k_f32(
            &enc,
            &mvq,
            pool.buf("q_w")?,
            pool.buf("norm")?,
            pool.buf("q_buf")?,
            Q_OUT,
            HIDDEN,
        )?;
        record_mul_mv_q4_k_f32(
            &enc,
            &mvq,
            pool.buf("k_w")?,
            pool.buf("norm")?,
            pool.buf("k_buf")?,
            KV_OUT,
            HIDDEN,
        )?;
        record_mul_mv_q4_k_f32(
            &enc,
            &mvq,
            pool.buf("v_w")?,
            pool.buf("norm")?,
            pool.buf("v_buf")?,
            KV_OUT,
            HIDDEN,
        )?;
        // 3. M-RoPE in-place (dst==src buffer)
        record_rope_multi_f32(
            &enc,
            &rope,
            pool.buf("q_buf")?,
            pool.buf("pos")?,
            pool.buf("q_buf")?,
            &q_rope,
        )?;
        record_rope_multi_f32(
            &enc,
            &rope,
            pool.buf("k_buf")?,
            pool.buf("pos")?,
            pool.buf("k_buf")?,
            &k_rope,
        )?;
        // 4. Gate projection
        record_mul_mv_q4_k_f32(
            &enc,
            &mvq,
            pool.buf("gate_w")?,
            pool.buf("norm")?,
            pool.buf("gate_buf")?,
            Q_OUT,
            HIDDEN,
        )?;
        // 5. Sigmoid on gate
        record_unary_contig_f32(
            &enc,
            &sigmoid,
            pool.buf("gate_buf")?,
            pool.buf("gate_sigmoid")?,
            Q_OUT,
        )?;
        // 6. SDPA degenerate (ctx=1): expand V (KV_OUT=512) → Q_OUT (4096)
        //    by GQA group=8. We don't have a native expand kernel yet, so
        //    for the smoke test we use bin_mul as a placeholder: gate_sigmoid
        //    × v_expanded. The v_expanded buffer is pre-allocated but empty
        //    here — the smoke test focuses on chain composition, not SDPA
        //    correctness (covered separately when wakeup #5 wires KV cache).
        record_bin_contig_f32(
            &enc,
            &mul_kernel,
            pool.buf("gate_sigmoid")?,
            pool.buf("v_expanded")?,
            pool.buf("attn_gated")?,
            Q_OUT,
        )?;
        // 7. O projection
        record_mul_mv_q4_k_f32(
            &enc,
            &mvq,
            pool.buf("o_w")?,
            pool.buf("attn_gated")?,
            pool.buf("o_buf")?,
            HIDDEN,
            O_IN,
        )?;
        // 8. Residual add
        record_bin_contig_f32(
            &enc,
            &add_kernel,
            pool.buf("residual")?,
            pool.buf("o_buf")?,
            pool.buf("residual")?,
            HIDDEN,
        )?;

        enc.endEncoding();

        let t0 = Instant::now();
        cmd.commit();
        unsafe { cmd.waitUntilCompleted() };
        let dt = t0.elapsed().as_secs_f64();
        if iter >= warmup {
            times.push(dt);
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = times[0];
    let med = times[iters / 2];
    let p95 = times[(iters * 95) / 100];
    println!();
    println!("== Per-block timing (10 dispatches in one chained cmd buffer) ==");
    println!(
        "  min={:>7.1} µs   med={:>7.1} µs   p95={:>7.1} µs",
        min * 1e6,
        med * 1e6,
        p95 * 1e6
    );
    let projected_per_token_ms_lower = min * 1e3 * 10.0; // 10 full-attn layers / token
    let projected_per_token_ms_upper = med * 1e3 * 10.0;
    let projected_tps_min = 1000.0 / projected_per_token_ms_upper;
    let projected_tps_max = 1000.0 / projected_per_token_ms_lower;
    println!();
    println!("== Projection onto integrated decode tg128 ==");
    println!(
        "  10 full-attn layers / token × per-block: {:.2}-{:.2} ms / token (full-attn share only)",
        projected_per_token_ms_lower, projected_per_token_ms_upper
    );
    println!(
        "  → tg projection from full-attn ALONE: {:.1}-{:.1} t/s",
        projected_tps_min, projected_tps_max
    );
    println!(
        "  Shim baseline: tg128 = 36.1 t/s — full-attn budget alone shouldn't exceed ~30 ms / token"
    );
    println!();
    println!("== Standing Status Card ==");
    println!("  Baseline (shim):  pp4096=672   pp16384=558  tg128=36.1");
    println!("  Stretch target:   pp4096=~770  pp16384=~670 tg128=~46");
    println!("  Rust engine:      pp/tg N/A    (still per-block, 30 of 40 layers + MoE FFN missing)");
    println!("  Per-block min/med {:.1} / {:.1} µs covers full-attn portion only.", min*1e6, med*1e6);
    Ok(())
}
