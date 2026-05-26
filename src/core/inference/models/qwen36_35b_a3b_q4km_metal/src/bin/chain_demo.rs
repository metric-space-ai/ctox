// Origin: CTOX
// License: AGPL-3.0-only

//! Chained-command-buffer demo: prove that RMSNorm followed by Q-proj
//! can run in ONE MTLCommandBuffer with shared persistent buffers, vs
//! TWO command buffers one per op (the current per-op-dispatcher path).
//!
//! This is the foundational proof for the Stage-4 layer-block driver:
//! every kernel in a layer is recorded into one encoder, the GPU runs
//! them back-to-back without CPU↔GPU sync between them, and we save
//! the per-call commit/wait round-trip ON TOP of the persistent buffer
//! win measured in `persistent_buffer_demo`.

#![cfg(feature = "metal")]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;
use std::time::Instant;

use anyhow::{anyhow, Result};
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLComputeCommandEncoder,
    MTLDevice, MTLResourceOptions, MTLSize,
};

use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::{
        mul_mv_q4_k::{record_mul_mv_q4_k_f32, MulMvQ4KF32Kernel},
        q4_k::{synth_block_q4_k, BlockQ4K, BLOCK_Q4_K_BYTES, QK_K},
        rms_norm::{record_rms_norm_f32, RmsNormF32Kernel},
    },
    runtime::{BufferPool, MetalRuntime},
};

fn xs(s: &mut u32) -> u32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    *s
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

fn build_pool(
    rt: &MetalRuntime,
    weights: &[BlockQ4K],
    input: &[f32],
    m: usize,
    k: usize,
) -> Result<BufferPool> {
    let mut pool = BufferPool::new(rt);
    let blocks_per_row = k / QK_K;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;

    let w_bytes = unsafe {
        std::slice::from_raw_parts(weights.as_ptr() as *const u8, m * row_bytes)
    };
    pool.copy_in("Q_w", w_bytes)?;
    let i_bytes = unsafe {
        std::slice::from_raw_parts(input.as_ptr() as *const u8, input.len() * size_of::<f32>())
    };
    pool.copy_in("residual", i_bytes)?;
    pool.alloc_zeroed("normed", input.len() * size_of::<f32>())?;
    pool.alloc_zeroed("Q_out", m * size_of::<f32>())?;
    Ok(pool)
}

/// Path A: TWO command buffers — rms_norm commits + waits, then mul_mv
/// commits + waits. Persistent buffers via the pool, but each op is
/// its own GPU sync.
fn run_two_buffers(
    rt: &MetalRuntime,
    pool: &BufferPool,
    rms: &RmsNormF32Kernel,
    mvq: &MulMvQ4KF32Kernel,
    rows: usize,
    cols: usize,
    m: usize,
) -> Result<f64> {
    let t0 = Instant::now();
    // RMSNorm
    let cmd1 = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer nil"))?;
    let enc1 = cmd1
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("encoder nil"))?;
    record_rms_norm_f32(
        &enc1,
        rms,
        &rt.device,
        pool.buf("residual")?,
        pool.buf("normed")?,
        rows,
        cols,
        1e-6,
    )?;
    enc1.endEncoding();
    cmd1.commit();
    unsafe { cmd1.waitUntilCompleted() };

    // Q proj
    let cmd2 = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("cmd nil"))?;
    let enc2 = cmd2
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("enc nil"))?;
    record_mul_mv_q4_k_f32(
        &enc2,
        mvq,
        pool.buf("Q_w")?,
        pool.buf("normed")?,
        pool.buf("Q_out")?,
        m,
        cols,
    )?;
    enc2.endEncoding();
    cmd2.commit();
    unsafe { cmd2.waitUntilCompleted() };

    Ok(t0.elapsed().as_secs_f64())
}

/// Path B: ONE command buffer — record both kernels, single commit + wait.
/// This is the Stage-4 layer-block driver pattern.
fn run_one_buffer(
    rt: &MetalRuntime,
    pool: &BufferPool,
    rms: &RmsNormF32Kernel,
    mvq: &MulMvQ4KF32Kernel,
    rows: usize,
    cols: usize,
    m: usize,
) -> Result<f64> {
    let t0 = Instant::now();
    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("cmd nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("enc nil"))?;
    record_rms_norm_f32(
        &enc,
        rms,
        &rt.device,
        pool.buf("residual")?,
        pool.buf("normed")?,
        rows,
        cols,
        1e-6,
    )?;
    record_mul_mv_q4_k_f32(
        &enc,
        mvq,
        pool.buf("Q_w")?,
        pool.buf("normed")?,
        pool.buf("Q_out")?,
        m,
        cols,
    )?;
    enc.endEncoding();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };
    Ok(t0.elapsed().as_secs_f64())
}

fn read_first_n_floats(pool: &BufferPool, key: &str, n: usize) -> Result<Vec<f32>> {
    let buf = pool.buf(key)?;
    let mut out = vec![0.0f32; n];
    unsafe {
        let p = (**buf).contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(p, out.as_mut_ptr(), n);
    }
    Ok(out)
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-chain-demo");
    let rt = MetalRuntime::new()?;
    let rms = RmsNormF32Kernel::new(&rt)?;
    let mvq = MulMvQ4KF32Kernel::new(&rt, /*nsg=*/ 4)?;

    // Qwen3.6 attn-block shape: hidden=2048, single token, Q_proj 4096×2048.
    let rows = 1usize;
    let cols = 2048usize;
    let m = 4096usize;

    let weights = synth_weights(m, cols, 0xC0FE_BABE);
    let input = synth_input(rows * cols, 0xFEED_FACE);
    let pool = build_pool(&rt, &weights, &input, m, cols)?;

    // Correctness check: both paths produce the same output.
    run_two_buffers(&rt, &pool, &rms, &mvq, rows, cols, m)?;
    let out_two = read_first_n_floats(&pool, "Q_out", m)?;
    run_one_buffer(&rt, &pool, &rms, &mvq, rows, cols, m)?;
    let out_one = read_first_n_floats(&pool, "Q_out", m)?;
    let mut max_abs = 0.0_f64;
    for (a, b) in out_two.iter().zip(out_one.iter()) {
        let d = (*a as f64 - *b as f64).abs();
        if d > max_abs {
            max_abs = d;
        }
    }
    println!("  correctness  one-buffer vs two-buffers  max_abs = {max_abs:.3e}");
    if max_abs > 1e-3 {
        anyhow::bail!("chained vs unchained drift too high: {max_abs:.3e}");
    }

    // Bench both paths.
    println!("--- bench (50 reps + 5 warmup) ---");
    let mut times_two = Vec::with_capacity(50);
    let mut times_one = Vec::with_capacity(50);
    for _ in 0..5 {
        run_two_buffers(&rt, &pool, &rms, &mvq, rows, cols, m)?;
        run_one_buffer(&rt, &pool, &rms, &mvq, rows, cols, m)?;
    }
    for _ in 0..50 {
        times_two.push(run_two_buffers(&rt, &pool, &rms, &mvq, rows, cols, m)?);
        times_one.push(run_one_buffer(&rt, &pool, &rms, &mvq, rows, cols, m)?);
    }
    times_two.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times_one.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min_two = times_two[0];
    let med_two = times_two[25];
    let p95_two = times_two[47];
    let min_one = times_one[0];
    let med_one = times_one[25];
    let p95_one = times_one[47];
    println!(
        "  TWO buffers  min={:>7.1} µs  med={:>7.1} µs  p95={:>7.1} µs",
        min_two * 1e6,
        med_two * 1e6,
        p95_two * 1e6
    );
    println!(
        "  ONE buffer   min={:>7.1} µs  med={:>7.1} µs  p95={:>7.1} µs",
        min_one * 1e6,
        med_one * 1e6,
        p95_one * 1e6
    );
    println!(
        "  → speedup    min={:.2}×  med={:.2}×",
        min_two / min_one,
        med_two / med_one
    );
    Ok(())
}
