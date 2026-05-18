//! End-to-end smoke test: chains 5 ported ops into a realistic
//! forward-pass-shaped sequence, runs it on the GPU entirely via
//! the `cuda_port::graph` Rust dispatcher (no ggml graph_compute),
//! and compares the final output against a pure-CPU reference.
//!
//! Sequence:
//!
//!   x₀ = input            (f32, 4096 × 32 × 1 × 1)
//!   x₁ = rms_norm(x₀, 1e-6)
//!   x₂ = silu(x₁)
//!   x₃ = scale(x₂, 1.5, -0.25)
//!   x₄ = add(x₀, x₃)         // residual add
//!   x₅ = mul(x₄, x₁)         // gated pattern
//!   → compare x₅ vs CPU ref
//!
//! This is the first test that exercises the Rust dispatch layer
//! end-to-end: tensors flow from op to op with only device-side
//! memory as the intermediate, no CPU round-trips, no ggml fallback
//! for the ported ops in the chain.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream,
};
use dflash::cuda_port::graph::{add, mul, rms_norm, scale, silu, DType, ExecCtx, Tensor};
use dflash::cuda_port::module::porter;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-graph-smoke")]
struct Args {
    #[arg(long, default_value_t = 4096)]
    ncols: i64,
    #[arg(long, default_value_t = 32)]
    nrows: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    /// End-to-end tolerance. Drift accumulates across the 5 ops;
    /// ~1e-5 per op gives us headroom at 5e-5 for the chain.
    #[arg(long, default_value_t = 5e-5)]
    tol: f32,
}

fn alloc_bytes(n: usize) -> Result<CUdeviceptr> {
    let mut p = CUdeviceptr(0);
    let rc = unsafe { cuMemAlloc_v2(&mut p, (n * 4) as libc::size_t) };
    if rc != 0 {
        return Err(anyhow!("cuMemAlloc_v2({n} f32) = {rc}"));
    }
    Ok(p)
}

fn upload(dst: CUdeviceptr, host: &[f32]) -> Result<()> {
    let bytes = (host.len() * 4) as libc::size_t;
    let rc = unsafe {
        sys::cudaMemcpyAsync(
            dst.0 as *mut c_void,
            host.as_ptr() as *const c_void,
            bytes,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(anyhow!("memcpy h→d: {rc}"));
    }
    Ok(())
}

fn download(src: CUdeviceptr, stream: CUstream, n: usize) -> Result<Vec<f32>> {
    let mut host = vec![0.0_f32; n];
    unsafe {
        sys::cudaMemcpyAsync(
            host.as_mut_ptr() as *mut c_void,
            src.0 as *const c_void,
            (n * 4) as libc::size_t,
            2,
            std::ptr::null_mut(),
        );
        cuStreamSynchronize(stream);
    }
    Ok(host)
}

fn cpu_chain(h_x0: &[f32], ne: [i64; 4], eps: f32) -> Vec<f32> {
    let ncols = ne[0] as usize;
    let nrows = (ne[1] * ne[2] * ne[3]) as usize;
    let n = ncols * nrows;

    // x1 = rms_norm(x0, eps): per-row y = x * rsqrt(mean(x²) + eps)
    let mut x1 = vec![0.0_f32; n];
    for r in 0..nrows {
        let base = r * ncols;
        let mut sum_sq = 0.0_f64;
        for i in 0..ncols {
            let v = h_x0[base + i] as f64;
            sum_sq += v * v;
        }
        let mean = sum_sq / ncols as f64;
        let scale = 1.0 / (mean + eps as f64).sqrt();
        for i in 0..ncols {
            x1[base + i] = (h_x0[base + i] as f64 * scale) as f32;
        }
    }

    // x2 = silu(x1) = x * sigmoid(x)
    let mut x2 = vec![0.0_f32; n];
    for i in 0..n {
        let v = x1[i] as f64;
        let s = 1.0 / (1.0 + (-v).exp());
        x2[i] = (v * s) as f32;
    }

    // x3 = 1.5 * x2 + (-0.25)
    let mut x3 = vec![0.0_f32; n];
    for i in 0..n {
        x3[i] = 1.5 * x2[i] - 0.25;
    }

    // x4 = x0 + x3
    let mut x4 = vec![0.0_f32; n];
    for i in 0..n {
        x4[i] = h_x0[i] + x3[i];
    }

    // x5 = x4 * x1
    let mut x5 = vec![0.0_f32; n];
    for i in 0..n {
        x5[i] = x4[i] * x1[i];
    }
    x5
}

fn main() -> Result<()> {
    let args = Args::parse();

    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!("ggml_backend_cuda_init failed"));
    }
    unsafe { cuInit(0) };
    ensure_current_context(args.cuda_device).map_err(|e| anyhow!("ctx: {e}"))?;
    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!("[smoke] kernels resolved");

    let ne = [args.ncols, args.nrows, 1, 1];
    let n = (ne[0] * ne[1] * ne[2] * ne[3]) as usize;
    let eps = 1e-6_f32;

    // Host input — same deterministic pattern the per-op verifiers use.
    let mut h_x0 = vec![0.0_f32; n];
    for (i, v) in h_x0.iter_mut().enumerate() {
        *v = ((i as f32) * 0.011).sin() * 2.0 + 0.1 * (i as f32).cos();
    }

    // Allocate 5 intermediate buffers on device.
    let d_x0 = alloc_bytes(n)?;
    let d_x1 = alloc_bytes(n)?;
    let d_x2 = alloc_bytes(n)?;
    let d_x3 = alloc_bytes(n)?;
    let d_x4 = alloc_bytes(n)?;
    let d_x5 = alloc_bytes(n)?;
    upload(d_x0, &h_x0)?;

    let t0 = Tensor::contiguous(d_x0, DType::F32, ne);
    let t1 = Tensor::contiguous(d_x1, DType::F32, ne);
    let t2 = Tensor::contiguous(d_x2, DType::F32, ne);
    let t3 = Tensor::contiguous(d_x3, DType::F32, ne);
    let t4 = Tensor::contiguous(d_x4, DType::F32, ne);
    let t5 = Tensor::contiguous(d_x5, DType::F32, ne);

    let stream = CUstream(std::ptr::null_mut());
    let ctx = ExecCtx::new(kernels, stream);

    // ── Execute the chain ─────────────────────────────────────
    let t_exec = std::time::Instant::now();
    rms_norm(&ctx, &t0, &t1, eps).map_err(|e| anyhow!(e))?;
    silu(&ctx, &t1, &t2).map_err(|e| anyhow!(e))?;
    scale(&ctx, &t2, &t3, 1.5, -0.25).map_err(|e| anyhow!(e))?;
    add(&ctx, &t0, &t3, &t4).map_err(|e| anyhow!(e))?;
    mul(&ctx, &t4, &t1, &t5).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };
    let exec_us = t_exec.elapsed().as_micros();
    println!("[smoke] 5-op chain launched + synced in {exec_us} µs");

    // ── Compare against CPU reference ─────────────────────────
    let h_got = download(d_x5, stream, n)?;
    let h_want = cpu_chain(&h_x0, ne, eps);

    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for i in 0..n {
        let d = (h_got[i] - h_want[i]).abs();
        if d > max_abs {
            max_abs = d;
            max_idx = i;
        }
    }

    unsafe {
        cuMemFree_v2(d_x0);
        cuMemFree_v2(d_x1);
        cuMemFree_v2(d_x2);
        cuMemFree_v2(d_x3);
        cuMemFree_v2(d_x4);
        cuMemFree_v2(d_x5);
        sys::ggml_backend_free(backend);
    }

    println!(
        "[smoke] chain end-to-end: max |gpu - cpu| = {:.3e} at idx {} (got {:.6e}, want {:.6e})",
        max_abs, max_idx, h_got[max_idx], h_want[max_idx]
    );

    if max_abs > args.tol {
        return Err(anyhow!(
            "smoke FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("[smoke] PASSED (tol {:.3e})", args.tol);
    Ok(())
}
