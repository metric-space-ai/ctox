// Origin: CTOX
// License: Apache-2.0

//! Persistent-buffer bench harness — proves the per-dispatch
//! buffer-alloc cost of the existing dispatcher path is real and
//! quantifies how much the Stage-4 layer-block driver gains by
//! reusing buffers.
//!
//! Both paths invoke the same `kernel_mul_mv_q4_K_f32` MSL kernel.
//! Difference: path A creates a fresh MTLBuffer per dispatch (current
//! dispatcher); path B allocates once into a `BufferPool` and reuses
//! across the timed loop.

#![cfg(feature = "metal")]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;
use std::time::Instant;

use anyhow::{anyhow, Result};
use objc2_metal::{
    MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLComputeCommandEncoder, MTLDevice,
    MTLSize,
};

use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::{
        mul_mv_q4_k::{dispatch_mul_mv_q4_k_f32_decode, MulMvQ4KF32Kernel},
        q4_k::{synth_block_q4_k, BlockQ4K, BLOCK_Q4_K_BYTES, QK_K},
    },
    runtime::{BufferPool, MetalRuntime},
};

#[repr(C)]
#[derive(Clone, Copy)]
struct KargsMulMv {
    ne00: i32,
    ne01: i32,
    ne02: i32,
    _pad0: u32,
    nb00: u64,
    nb01: u64,
    nb02: u64,
    nb03: u64,
    ne10: i32,
    ne11: i32,
    ne12: i32,
    _pad1: u32,
    nb10: u64,
    nb11: u64,
    nb12: u64,
    nb13: u64,
    ne0: i32,
    ne1: i32,
    nr0: i32,
    r2: i16,
    r3: i16,
}

const N_R0_Q4_K: u32 = 2;

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

fn one_persistent_dispatch(
    rt: &MetalRuntime,
    pool: &BufferPool,
    kernel: &MulMvQ4KF32Kernel,
    kargs: &KargsMulMv,
    grid: MTLSize,
    tg: MTLSize,
) -> Result<f64> {
    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("encoder nil"))?;
    let pso = kernel.pso_handle();
    enc.setComputePipelineState(&pso);

    let kargs_nn = NonNull::new(kargs as *const KargsMulMv as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMv>(), 0);
        enc.setBuffer_offset_atIndex(Some(pool.buf("weights")?), 0, 1);
        enc.setBuffer_offset_atIndex(Some(pool.buf("input")?), 0, 2);
        enc.setBuffer_offset_atIndex(Some(pool.buf("input")?), 0, 3);
        enc.setBuffer_offset_atIndex(Some(pool.buf("output")?), 0, 4);
    }
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    enc.endEncoding();

    let t0 = Instant::now();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };
    Ok(t0.elapsed().as_secs_f64())
}

fn bench_persistent(
    rt: &MetalRuntime,
    label: &str,
    m: usize,
    k: usize,
    iters: usize,
    warmup: usize,
) -> Result<(f64, f64, f64)> {
    let weights = synth_weights(m, k, 0xC0FE_BABE);
    let input = synth_input(k, 0xFEED_FACE);
    let kernel = MulMvQ4KF32Kernel::new(rt, /*nsg=*/ 4)?;

    // Build the buffer pool ONCE before timing.
    let mut pool = BufferPool::new(rt);
    let w_bytes = unsafe {
        std::slice::from_raw_parts(
            weights.as_ptr() as *const u8,
            weights.len() * BLOCK_Q4_K_BYTES,
        )
    };
    pool.copy_in("weights", w_bytes)?;
    let i_bytes = unsafe {
        std::slice::from_raw_parts(input.as_ptr() as *const u8, input.len() * size_of::<f32>())
    };
    pool.copy_in("input", i_bytes)?;
    pool.alloc_zeroed("output", m * size_of::<f32>())?;

    let blocks_per_row = k / QK_K;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let total_weight_bytes = m * row_bytes;
    let kargs = KargsMulMv {
        ne00: k as i32,
        ne01: m as i32,
        ne02: 1,
        _pad0: 0,
        nb00: 0,
        nb01: row_bytes as u64,
        nb02: total_weight_bytes as u64,
        nb03: total_weight_bytes as u64,
        ne10: k as i32,
        ne11: 1,
        ne12: 1,
        _pad1: 0,
        nb10: size_of::<f32>() as u64,
        nb11: (k * size_of::<f32>()) as u64,
        nb12: (k * size_of::<f32>()) as u64,
        nb13: (k * size_of::<f32>()) as u64,
        ne0: m as i32,
        ne1: 1,
        nr0: 1,
        r2: 1,
        r3: 1,
    };
    let nsg = 4;
    let rows_per_tg = nsg * N_R0_Q4_K as usize;
    let grid = MTLSize {
        width: m / rows_per_tg,
        height: 1,
        depth: 1,
    };
    let tg = MTLSize {
        width: nsg * 32,
        height: 1,
        depth: 1,
    };

    for _ in 0..warmup {
        let _ = one_persistent_dispatch(rt, &pool, &kernel, &kargs, grid, tg)?;
    }
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        samples.push(one_persistent_dispatch(rt, &pool, &kernel, &kargs, grid, tg)?);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = samples[0];
    let med = samples[iters / 2];
    let p95 = samples[(iters * 95) / 100];
    println!(
        "  PERSIST  {label:<14} m={m} k={k}  min={:>7.1} µs  med={:>7.1} µs  p95={:>7.1} µs",
        min * 1e6,
        med * 1e6,
        p95 * 1e6
    );
    Ok((min, med, p95))
}

fn bench_per_dispatch(
    rt: &MetalRuntime,
    label: &str,
    m: usize,
    k: usize,
    iters: usize,
    warmup: usize,
) -> Result<(f64, f64, f64)> {
    let weights = synth_weights(m, k, 0xC0FE_BABE);
    let input = synth_input(k, 0xFEED_FACE);
    let kernel = MulMvQ4KF32Kernel::new(rt, /*nsg=*/ 4)?;

    for _ in 0..warmup {
        let _ = dispatch_mul_mv_q4_k_f32_decode(rt, &kernel, &weights, &input, m, k)?;
    }
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        let _ = dispatch_mul_mv_q4_k_f32_decode(rt, &kernel, &weights, &input, m, k)?;
        samples.push(t0.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = samples[0];
    let med = samples[iters / 2];
    let p95 = samples[(iters * 95) / 100];
    println!(
        "  PER-CALL {label:<14} m={m} k={k}  min={:>7.1} µs  med={:>7.1} µs  p95={:>7.1} µs",
        min * 1e6,
        med * 1e6,
        p95 * 1e6
    );
    Ok((min, med, p95))
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-persistent-buffer-demo");
    let rt = MetalRuntime::new()?;

    let shapes: &[(&str, usize, usize)] = &[
        ("Q-proj", 4096, 2048),
        ("KV-proj", 512, 2048),
        ("O-proj", 2048, 4096),
        ("FFN_up", 512, 2048),
    ];

    for &(label, m, k) in shapes {
        let (min_p, med_p, _) = bench_per_dispatch(&rt, label, m, k, 50, 5)?;
        let (min_b, med_b, _) = bench_persistent(&rt, label, m, k, 50, 5)?;
        let speedup_min = min_p / min_b;
        let speedup_med = med_p / med_b;
        println!(
            "  → {label:<14} speedup: min={:.2}×  med={:.2}×",
            speedup_min, speedup_med
        );
        println!();
    }

    Ok(())
}
