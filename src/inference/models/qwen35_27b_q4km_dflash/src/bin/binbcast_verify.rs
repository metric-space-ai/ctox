//! Bit-close verifier for the bare-metal `binbcast` port —
//! op_add / op_sub / op_mul on f32×f32→f32, contiguous and equal-
//! shape (no broadcast). Runs the tiled `k_bin_bcast` kernel
//! through the Rust dispatcher and compares against a CPU
//! reference computed with the same operator.
//!
//! Broadcast paths are not exercised here: they go through the same
//! kernel, just with different ne1x/s1x cookies — a follow-up
//! verifier once a case in the Qwen3.5 graph actually hits them.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::binbcast::{
    ggml_cuda_op_add_f32, ggml_cuda_op_mul_f32, ggml_cuda_op_sub_f32, BinBcastTensor,
};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-binbcast-verify")]
struct Args {
    /// Tensor shape (ne0 × ne1 × ne2 × ne3) — all three tensors
    /// share it; we're testing the no-broadcast path.
    #[arg(long, default_value_t = 128)]
    ne0: i64,
    #[arg(long, default_value_t = 64)]
    ne1: i64,
    #[arg(long, default_value_t = 2)]
    ne2: i64,
    #[arg(long, default_value_t = 1)]
    ne3: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    #[arg(long, default_value_t = 1e-6)]
    tol: f32,
}

fn contiguous_strides(ne: [i64; 4]) -> [i64; 4] {
    // s[0] is always 1 (elements, not bytes); upstream collapses
    // /sizeof(T) so this matches.
    let mut s = [1i64; 4];
    for d in 1..4 {
        s[d] = s[d - 1] * ne[d - 1];
    }
    s
}

fn upload(d: &mut CUdeviceptr, host: &[f32]) -> Result<()> {
    let bytes = (host.len() * std::mem::size_of::<f32>()) as libc::size_t;
    unsafe { cuMemAlloc_v2(d, bytes) };
    let rc = unsafe {
        sys::cudaMemcpyAsync(
            d.0 as *mut c_void,
            host.as_ptr() as *const c_void,
            bytes,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(anyhow!("cudaMemcpy h→d: {rc}"));
    }
    Ok(())
}

fn download(d: CUdeviceptr, stream: CUstream, n: usize) -> Result<Vec<f32>> {
    let bytes = (n * std::mem::size_of::<f32>()) as libc::size_t;
    let mut out = vec![0.0_f32; n];
    unsafe {
        sys::cudaMemcpyAsync(
            out.as_mut_ptr() as *mut c_void,
            d.0 as *const c_void,
            bytes,
            2,
            std::ptr::null_mut(),
        );
        cuStreamSynchronize(stream);
    }
    Ok(out)
}

fn run_case(
    name: &str,
    h_a: &[f32],
    h_b: &[f32],
    shape: &BinBcastTensor,
    kernels: &dflash::cuda_port::module::PortedKernels,
    stream: CUstream,
    op: fn(
        &dflash::cuda_port::ops::binbcast::BinBcastKernels,
        CUdeviceptr,
        CUdeviceptr,
        CUdeviceptr,
        &BinBcastTensor,
        &BinBcastTensor,
        &BinBcastTensor,
        CUstream,
    ) -> i32,
    cpu: impl Fn(f32, f32) -> f32,
    tol: f32,
) -> Result<()> {
    let n = h_a.len();
    let mut d_a = CUdeviceptr(0);
    let mut d_b = CUdeviceptr(0);
    let mut d_y = CUdeviceptr(0);
    upload(&mut d_a, h_a)?;
    upload(&mut d_b, h_b)?;
    let bytes = (n * std::mem::size_of::<f32>()) as libc::size_t;
    unsafe { cuMemAlloc_v2(&mut d_y, bytes) };

    let rc = op(&kernels.binbcast, d_a, d_b, d_y, shape, shape, shape, stream);
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("{name} launch failed: {rc}"));
    }
    unsafe { cuStreamSynchronize(stream) };

    let h_y = download(d_y, stream, n)?;

    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for i in 0..n {
        let d = (h_y[i] - cpu(h_a[i], h_b[i])).abs();
        if d > max_abs {
            max_abs = d;
            max_idx = i;
        }
    }

    unsafe { cuMemFree_v2(d_a) };
    unsafe { cuMemFree_v2(d_b) };
    unsafe { cuMemFree_v2(d_y) };

    println!(
        "{name}: max |gpu - cpu| = {:.3e} at idx {}  gpu={:.6e} cpu={:.6e}",
        max_abs,
        max_idx,
        h_y[max_idx],
        cpu(h_a[max_idx], h_b[max_idx])
    );
    if max_abs > tol {
        Err(anyhow!("{name}: FAILED tol {tol:.3e}"))
    } else {
        println!("{name}: PASSED (tol {tol:.3e})");
        Ok(())
    }
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
    println!("binbcast kernels resolved (add, sub, mul)");

    let ne = [args.ne0, args.ne1, args.ne2, args.ne3];
    let s = contiguous_strides(ne);
    let shape = BinBcastTensor { ne, s };
    let n = (ne[0] * ne[1] * ne[2] * ne[3]) as usize;

    let mut h_a = vec![0.0_f32; n];
    let mut h_b = vec![0.0_f32; n];
    for i in 0..n {
        h_a[i] = (i as f32 * 0.017).sin() + 0.5;
        h_b[i] = (i as f32 * 0.031).cos() - 0.25;
    }

    let stream = CUstream(std::ptr::null_mut());

    run_case(
        "add",
        &h_a,
        &h_b,
        &shape,
        kernels,
        stream,
        ggml_cuda_op_add_f32,
        |a, b| a + b,
        args.tol,
    )?;
    run_case(
        "sub",
        &h_a,
        &h_b,
        &shape,
        kernels,
        stream,
        ggml_cuda_op_sub_f32,
        |a, b| a - b,
        args.tol,
    )?;
    run_case(
        "mul",
        &h_a,
        &h_b,
        &shape,
        kernels,
        stream,
        ggml_cuda_op_mul_f32,
        |a, b| a * b,
        args.tol,
    )?;

    unsafe { sys::ggml_backend_free(backend) };
    println!("ALL BINBCAST PASSED");
    Ok(())
}
