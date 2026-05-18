// Origin: CTOX
// License: Apache-2.0

//! Isolated microbench helper for `kernel_mul_mv_q4_K_f32`. Separate
//! from the verifier dispatch path because here we **pre-allocate**
//! GPU buffers once and reuse them in a tight commit/wait loop. This
//! is the right measurement for promotion decisions — we want the
//! kernel's steady-state throughput, not allocation overhead.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:7715-7833

#![cfg(feature = "metal")]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;
use std::time::Instant;

use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLComputeCommandEncoder,
    MTLDevice, MTLResourceOptions, MTLSize,
};

use crate::metal_port::ops::mul_mv_q4_k::MulMvQ4KF32Kernel;
use crate::metal_port::ops::q4_k::{BlockQ4K, BLOCK_Q4_K_BYTES, QK_K};
use crate::metal_port::runtime::MetalRuntime;

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

/// Output of one bench run.
pub struct BenchResult {
    pub gpu_dispatch_us: f64,
    pub p95_us: f64,
    pub min_us: f64,
    pub bw_gbs: f64,
    pub gflops: f64,
    pub gpu_out_first8: [f32; 8],
}

/// Pre-allocate the GPU buffers, then time the kernel dispatch +
/// commit + waitUntilCompleted in a tight loop. `iters` total reps,
/// `warmup` discarded reps before timing.
pub fn bench_mul_mv_q4_k_f32(
    rt: &MetalRuntime,
    kernel: &MulMvQ4KF32Kernel,
    weights_q4k: &[BlockQ4K],
    input_f32: &[f32],
    m: usize,
    k: usize,
    iters: usize,
    warmup: usize,
) -> Result<BenchResult> {
    if k % QK_K != 0 {
        return Err(anyhow!("k must be divisible by 256, got {k}"));
    }
    let blocks_per_row = k / QK_K;
    if weights_q4k.len() != m * blocks_per_row {
        return Err(anyhow!("weights len mismatch"));
    }
    if input_f32.len() != k {
        return Err(anyhow!("input len mismatch"));
    }
    if m % (kernel.nsg as usize * N_R0_Q4_K as usize) != 0 {
        return Err(anyhow!(
            "m={} must be divisible by nsg({}) × N_R0_Q4_K({})",
            m,
            kernel.nsg,
            N_R0_Q4_K
        ));
    }

    let device = &rt.device;
    let opts = MTLResourceOptions::MTLResourceStorageModeShared;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let total_weight_bytes = m * row_bytes;

    let weights_nn = NonNull::new(weights_q4k.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("weights ptr null"))?;
    let buf_w = unsafe {
        device.newBufferWithBytes_length_options(weights_nn, total_weight_bytes, opts)
    }
    .ok_or_else(|| anyhow!("buf_w nil"))?;

    let input_nn = NonNull::new(input_f32.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("input ptr null"))?;
    let buf_in = unsafe {
        device.newBufferWithBytes_length_options(
            input_nn,
            input_f32.len() * size_of::<f32>(),
            opts,
        )
    }
    .ok_or_else(|| anyhow!("buf_in nil"))?;

    let buf_out = device
        .newBufferWithLength_options(m * size_of::<f32>(), opts)
        .ok_or_else(|| anyhow!("buf_out nil"))?;

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

    let rows_per_tg = kernel.nsg as usize * N_R0_Q4_K as usize;
    let grid = MTLSize {
        width: m / rows_per_tg,
        height: 1,
        depth: 1,
    };
    let tg = MTLSize {
        width: kernel.nsg as usize * 32,
        height: 1,
        depth: 1,
    };

    let mut dispatch_one = || -> Result<f64> {
        let cmd = rt
            .queue
            .commandBuffer()
            .ok_or_else(|| anyhow!("commandBuffer nil"))?;
        let enc = cmd
            .computeCommandEncoder()
            .ok_or_else(|| anyhow!("encoder nil"))?;
        enc.setComputePipelineState(&kernel.pso_handle());
        let kargs_nn = NonNull::new(&kargs as *const KargsMulMv as *mut c_void)
            .ok_or_else(|| anyhow!("kargs ptr null"))?;
        unsafe {
            enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMv>(), 0);
            enc.setBuffer_offset_atIndex(Some(&buf_w), 0, 1);
            enc.setBuffer_offset_atIndex(Some(&buf_in), 0, 2);
            enc.setBuffer_offset_atIndex(Some(&buf_out), 0, 3);
        }
        enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
        enc.endEncoding();

        let t0 = Instant::now();
        cmd.commit();
        unsafe { cmd.waitUntilCompleted() };
        Ok(t0.elapsed().as_secs_f64())
    };

    for _ in 0..warmup {
        let _ = dispatch_one()?;
    }

    let mut samples: Vec<f64> = Vec::with_capacity(iters);
    for _ in 0..iters {
        samples.push(dispatch_one()?);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[iters / 2];
    let p95 = samples[(iters * 95) / 100];
    let min = samples[0];

    let weight_bytes = (m * k * 9) / 16;
    let input_bytes = k * size_of::<f32>();
    let traffic = (weight_bytes + input_bytes) as f64;
    let bw_gbs = traffic / median / 1e9;
    let flops = 2.0 * m as f64 * k as f64;
    let gflops = flops / median / 1e9;

    let mut first8 = [0.0f32; 8];
    unsafe {
        let src = buf_out.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, first8.as_mut_ptr(), 8.min(m));
    }

    Ok(BenchResult {
        gpu_dispatch_us: median * 1e6,
        p95_us: p95 * 1e6,
        min_us: min * 1e6,
        bw_gbs,
        gflops,
        gpu_out_first8: first8,
    })
}
