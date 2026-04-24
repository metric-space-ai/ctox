//! Bit-close verifier for the bare-metal unary-op port (silu, neg, exp on f32).
//!
//! Same structure as `rms_norm_verify`: pull up a CUDA primary
//! context (via ggml_backend_cuda_init + ensure_current_context),
//! resolve `cuda_port::ops::unary::UnaryKernels`, allocate an f32
//! input + output on the device, fill host-side with a known
//! pattern, launch each unary op through the ported dispatcher,
//! copy back, compare against a CPU reference.
//!
//! Tolerances:
//!   - silu uses fast-math expf → ~2e-6 per element worst case
//!   - neg is exact
//!   - exp fast-math expf → ~2e-6 per element worst case

use std::ffi::c_void;
use std::os::raw::c_int;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::unary::{
    ggml_cuda_op_exp_f32, ggml_cuda_op_neg_f32, ggml_cuda_op_silu_f32,
};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-unary-verify")]
struct Args {
    /// Total element count.
    #[arg(long, default_value_t = 32768)]
    k: i32,
    /// CUDA device index.
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    /// Max abs-diff threshold.
    #[arg(long, default_value_t = 1e-5)]
    tol: f32,
}

fn silu_cpu(x: f32) -> f32 {
    let s = 1.0 / (1.0 + (-x as f64).exp());
    (x as f64 * s) as f32
}
fn neg_cpu(x: f32) -> f32 {
    -x
}
fn exp_cpu(x: f32) -> f32 {
    (x as f64).exp() as f32
}

fn run_op(
    name: &str,
    h_x: &[f32],
    tol: f32,
    dispatch: impl FnOnce(CUdeviceptr, CUdeviceptr, c_int, CUstream) -> u32,
    cpu: impl Fn(f32) -> f32,
    bytes: libc::size_t,
    k: c_int,
) -> Result<()> {
    let n_elem = h_x.len();

    let mut d_x = CUdeviceptr(0);
    let mut d_y = CUdeviceptr(0);
    let rc = unsafe { cuMemAlloc_v2(&mut d_x, bytes) };
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("{name}: cuMemAlloc x"));
    }
    let rc = unsafe { cuMemAlloc_v2(&mut d_y, bytes) };
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("{name}: cuMemAlloc y"));
    }

    let rc_cp = unsafe {
        sys::cudaMemcpyAsync(
            d_x.0 as *mut c_void,
            h_x.as_ptr() as *const c_void,
            bytes,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc_cp != 0 {
        return Err(anyhow!("{name}: cudaMemcpyAsync h→d = {rc_cp}"));
    }

    let stream: CUstream = CUstream(std::ptr::null_mut());
    let rc = dispatch(d_x, d_y, k, stream);
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("{name}: cuLaunchKernel"));
    }
    unsafe { cuStreamSynchronize(stream) };

    let mut h_y = vec![0.0_f32; n_elem];
    let rc_cp = unsafe {
        sys::cudaMemcpyAsync(
            h_y.as_mut_ptr() as *mut c_void,
            d_y.0 as *const c_void,
            bytes,
            2,
            std::ptr::null_mut(),
        )
    };
    if rc_cp != 0 {
        return Err(anyhow!("{name}: cudaMemcpyAsync d→h = {rc_cp}"));
    }
    unsafe { cuStreamSynchronize(stream) };

    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for i in 0..n_elem {
        let d = (h_y[i] - cpu(h_x[i])).abs();
        if d > max_abs {
            max_abs = d;
            max_idx = i;
        }
    }
    println!(
        "{name}: max |gpu - cpu| = {:.3e} at idx {}  gpu={:.6e} cpu={:.6e}",
        max_abs,
        max_idx,
        h_y[max_idx],
        cpu(h_x[max_idx])
    );

    unsafe { cuMemFree_v2(d_x) };
    unsafe { cuMemFree_v2(d_y) };

    if max_abs > tol {
        Err(anyhow!("{name}: FAILED tol {tol:.3e}"))
    } else {
        println!("{name}: PASSED (tol {:.3e})", tol);
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
    ensure_current_context(args.cuda_device).map_err(|e| anyhow!("context: {e}"))?;
    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!("unary kernels resolved: silu_f32 / neg_f32 / exp_f32");

    let n = args.k as usize;
    let bytes = (n * std::mem::size_of::<f32>()) as libc::size_t;
    let mut h_x = vec![0.0_f32; n];
    for i in 0..n {
        // Range roughly [-2, 2] — covers silu's bend, exp's stable zone.
        let t = (i as f32) / (n as f32);
        h_x[i] = 4.0 * t - 2.0 + (i as f32 * 0.017).sin() * 0.1;
    }

    run_op(
        "silu",
        &h_x,
        args.tol,
        |x, y, k, s| ggml_cuda_op_silu_f32(&kernels.unary, x, y, k, s),
        silu_cpu,
        bytes,
        args.k,
    )?;

    run_op(
        "neg",
        &h_x,
        args.tol,
        |x, y, k, s| ggml_cuda_op_neg_f32(&kernels.unary, x, y, k, s),
        neg_cpu,
        bytes,
        args.k,
    )?;

    run_op(
        "exp",
        &h_x,
        args.tol,
        |x, y, k, s| ggml_cuda_op_exp_f32(&kernels.unary, x, y, k, s),
        exp_cpu,
        bytes,
        args.k,
    )?;

    unsafe { sys::ggml_backend_free(backend) };
    println!("ALL UNARY PASSED");
    Ok(())
}
