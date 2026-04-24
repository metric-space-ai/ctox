//! Bit-close verifier for the bare-metal `softmax` port
//! (soft_max_f32<true, 0, 0, float> fallback, f32 mask).
//!
//! Runs softmax over an (ncols, nrows) f32 tensor — no mask, no
//! sinks, scale=1, max_bias=0 — and compares against a CPU f64
//! reference. Drift is expected at < 1e-6 because the device uses
//! fast-math expf + block-parallel max/sum reductions.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::softmax::{ggml_cuda_op_soft_max, SoftMaxParams};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-softmax-verify")]
struct Args {
    #[arg(long, default_value_t = 256)]
    ncols: i64,
    #[arg(long, default_value_t = 16)]
    nrows: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    #[arg(long, default_value_t = 1e-5)]
    tol: f32,
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
    println!("softmax kernels resolved (f32-mask + f16-mask)");

    let ncols = args.ncols;
    let nrows = args.nrows;
    let n = (ncols * nrows) as usize;
    let bytes = (n * std::mem::size_of::<f32>()) as libc::size_t;

    let mut h_x = vec![0.0_f32; n];
    for (i, v) in h_x.iter_mut().enumerate() {
        // Keep values in a range where expf doesn't blow up.
        *v = ((i as f32) * 0.017).sin() * 2.0;
    }

    let mut d_x = CUdeviceptr(0);
    let mut d_dst = CUdeviceptr(0);
    unsafe { cuMemAlloc_v2(&mut d_x, bytes) };
    unsafe { cuMemAlloc_v2(&mut d_dst, bytes) };
    unsafe {
        sys::cudaMemcpyAsync(
            d_x.0 as *mut c_void,
            h_x.as_ptr() as *const c_void,
            bytes,
            1,
            std::ptr::null_mut(),
        );
    }

    // params — 1 head, shape (ncols, nrows, 1, 1).
    let params = SoftMaxParams {
        nheads: 1,
        n_head_log2: 1,
        _pad0: 0,
        ncols,
        nrows_x: nrows,
        nrows_y: nrows,
        ne00: ncols,
        ne01: nrows,
        ne02: 1,
        ne03: 1,
        nb11: 0,
        nb12: 0,
        nb13: 0,
        ne12: 1,
        ne13: 1,
        scale: 1.0,
        max_bias: 0.0,
        m0: 1.0,
        m1: 1.0,
    };

    let stream = CUstream(std::ptr::null_mut());
    let rc = ggml_cuda_op_soft_max(
        &kernels.softmax,
        d_x,
        CUdeviceptr(0), // no mask
        CUdeviceptr(0), // no sinks
        d_dst,
        &params,
        false, // mask is f32 (and null)
        stream,
    );
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("softmax launch failed: {rc}"));
    }
    unsafe { cuStreamSynchronize(stream) };

    let mut h_dst = vec![0.0_f32; n];
    unsafe {
        sys::cudaMemcpyAsync(
            h_dst.as_mut_ptr() as *mut c_void,
            d_dst.0 as *const c_void,
            bytes,
            2,
            std::ptr::null_mut(),
        );
        cuStreamSynchronize(stream);
    }

    unsafe { cuMemFree_v2(d_x) };
    unsafe { cuMemFree_v2(d_dst) };
    unsafe { sys::ggml_backend_free(backend) };

    // CPU reference — per-row softmax in f64.
    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for r in 0..nrows {
        let base = (r * ncols) as usize;
        let row = &h_x[base..base + ncols as usize];
        let row_max = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max) as f64;
        let mut sum = 0.0_f64;
        let mut exps = vec![0.0_f64; ncols as usize];
        for (i, v) in row.iter().enumerate() {
            let e = (*v as f64 - row_max).exp();
            exps[i] = e;
            sum += e;
        }
        let inv = 1.0 / sum;
        for (i, e) in exps.iter().enumerate() {
            let expected = (e * inv) as f32;
            let diff = (h_dst[base + i] - expected).abs();
            if diff > max_abs {
                max_abs = diff;
                max_idx = base + i;
            }
        }
    }

    println!(
        "softmax<f32-mask>: max |gpu - cpu| = {:.3e} at idx {}  gpu={:.6e}",
        max_abs, max_idx, h_dst[max_idx]
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "softmax FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("softmax<f32-mask>: PASSED (tol {:.3e})", args.tol);
    Ok(())
}
