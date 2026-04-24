//! Bit-close verifier for the bare-metal `solve_tri` port
//! (fast kernel, `<0,0>` general-case specialization).
//!
//! Computes X = B · A^(-1) where A is upper-triangular, non-unit-diag.
//! Reference: host-side back-substitution using f64 accumulators.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::solve_tri::ggml_cuda_op_solve_tri_f32;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-solve-tri-verify")]
struct Args {
    #[arg(long, default_value_t = 32)]
    n: i32,
    #[arg(long, default_value_t = 8)]
    k: i32,
    #[arg(long, default_value_t = 2)]
    ne02: i64,
    #[arg(long, default_value_t = 1)]
    ne03: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    /// Tolerance per output element (triangular solve compounds
    /// rounding error, so this is fairly loose).
    #[arg(long, default_value_t = 2e-3)]
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
    println!("solve_tri<f32,0,0> kernel resolved");

    let n = args.n as i64;
    let k = args.k as i64;
    let total_batches = args.ne02 * args.ne03;

    // A: (n, n, ne02, ne03) — upper triangular, contiguous.
    // B, X: (k, n, ne02, ne03) — contiguous (k = innermost).
    let a_elems = (n * n * total_batches) as usize;
    let b_elems = (k * n * total_batches) as usize;

    // Host A: upper triangular with positive diagonal to stay
    // well-conditioned.
    let mut h_a = vec![0.0_f32; a_elems];
    for batch in 0..total_batches {
        let base = (batch * n * n) as usize;
        for i in 0..n as usize {
            for j in 0..n as usize {
                let v = if i <= j {
                    // Make diagonal-dominant: diag ~ 2-3, off-diag < 1.
                    if i == j {
                        2.0 + 0.5 * (i as f32 + batch as f32).sin()
                    } else {
                        0.1 * ((i * 7 + j * 13 + batch as usize * 5) as f32 * 0.001).sin()
                    }
                } else {
                    0.0
                };
                h_a[base + i * n as usize + j] = v;
            }
        }
    }

    let mut h_b = vec![0.0_f32; b_elems];
    for (i, v) in h_b.iter_mut().enumerate() {
        *v = (i as f32 * 0.007).sin() + 0.2;
    }

    let a_bytes = (a_elems * 4) as libc::size_t;
    let b_bytes = (b_elems * 4) as libc::size_t;
    let mut d_a = CUdeviceptr(0);
    let mut d_b = CUdeviceptr(0);
    let mut d_x = CUdeviceptr(0);
    unsafe { cuMemAlloc_v2(&mut d_a, a_bytes) };
    unsafe { cuMemAlloc_v2(&mut d_b, b_bytes) };
    unsafe { cuMemAlloc_v2(&mut d_x, b_bytes) };
    unsafe {
        sys::cudaMemcpyAsync(
            d_a.0 as *mut c_void,
            h_a.as_ptr() as *const c_void,
            a_bytes,
            1,
            std::ptr::null_mut(),
        );
        sys::cudaMemcpyAsync(
            d_b.0 as *mut c_void,
            h_b.as_ptr() as *const c_void,
            b_bytes,
            1,
            std::ptr::null_mut(),
        );
    }

    // Stride packs (in **elements**, matching the kernel's
    // nb*/sizeof(float) convention).
    let nb02 = n * n;
    let nb03 = n * n * args.ne02;
    let nb12 = k * n;
    let nb13 = k * n * args.ne02;
    let nb2 = k * n;
    let nb3 = k * n * args.ne02;

    let stream = CUstream(std::ptr::null_mut());
    let rc = ggml_cuda_op_solve_tri_f32(
        &kernels.solve_tri,
        d_a,
        d_b,
        d_x,
        args.n,
        args.k,
        args.ne02,
        args.ne03,
        nb02,
        nb03,
        nb12,
        nb13,
        nb2,
        nb3,
        stream,
    );
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("solve_tri launch failed: {rc}"));
    }
    unsafe { cuStreamSynchronize(stream) };

    let mut h_x = vec![0.0_f32; b_elems];
    unsafe {
        sys::cudaMemcpyAsync(
            h_x.as_mut_ptr() as *mut c_void,
            d_x.0 as *const c_void,
            b_bytes,
            2,
            std::ptr::null_mut(),
        );
        cuStreamSynchronize(stream);
    }
    unsafe { cuMemFree_v2(d_a) };
    unsafe { cuMemFree_v2(d_b) };
    unsafe { cuMemFree_v2(d_x) };
    unsafe { sys::ggml_backend_free(backend) };

    // CPU reference: solve X · A = B for each batch via back-substitution.
    // X_batch[j, col] = (B_batch[j, col] - Σ_{i<j} A_batch[i, j] · X_batch[i, col]) / A_batch[j, j]
    // (Upper-tri, right-side-solve.)
    let n_us = n as usize;
    let k_us = k as usize;
    let mut max_abs = 0.0_f32;
    let mut max_pos = (0usize, 0usize, 0usize);
    for batch in 0..total_batches as usize {
        let a_base = batch * n_us * n_us;
        let bx_base = batch * k_us * n_us;
        let mut x_ref = vec![0.0_f64; k_us * n_us];
        for col in 0..k_us {
            for j in 0..n_us {
                let mut sum = h_b[bx_base + j * k_us + col] as f64;
                for i in 0..j {
                    sum -= h_a[a_base + i * n_us + j] as f64 * x_ref[i * k_us + col];
                }
                x_ref[j * k_us + col] = sum / h_a[a_base + j * n_us + j] as f64;
            }
        }
        for j in 0..n_us {
            for col in 0..k_us {
                let got = h_x[bx_base + j * k_us + col];
                let want = x_ref[j * k_us + col] as f32;
                let diff = (got - want).abs();
                if diff > max_abs {
                    max_abs = diff;
                    max_pos = (batch, j, col);
                }
            }
        }
    }

    println!(
        "solve_tri<f32, n={} k={}>: max |gpu - cpu| = {:.3e} at (batch={}, j={}, col={})",
        args.n, args.k, max_abs, max_pos.0, max_pos.1, max_pos.2
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "solve_tri FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("solve_tri: PASSED (tol {:.3e})", args.tol);
    Ok(())
}
