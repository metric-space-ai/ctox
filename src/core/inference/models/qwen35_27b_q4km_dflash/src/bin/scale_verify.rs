//! Bit-close verifier for `cuda_port::ops::scale`.
//!
//! `dst[i] = scale * x[i] + bias`. Elementwise, f32.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::scale::ggml_cuda_op_scale_f32;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-scale-verify")]
struct Args {
    #[arg(long, default_value_t = 32768)]
    k: i64,
    #[arg(long, default_value_t = 2.5)]
    scale: f32,
    #[arg(long, default_value_t = 0.375)]
    bias: f32,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    #[arg(long, default_value_t = 1e-6)]
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
    println!("scale kernel resolved");

    let n = args.k as usize;
    let bytes = (n * std::mem::size_of::<f32>()) as libc::size_t;
    let mut h_x = vec![0.0_f32; n];
    for i in 0..n {
        h_x[i] = (i as f32 * 0.013).sin() + 0.5;
    }

    let mut d_x = CUdeviceptr(0);
    let mut d_y = CUdeviceptr(0);
    unsafe { cuMemAlloc_v2(&mut d_x, bytes) };
    unsafe { cuMemAlloc_v2(&mut d_y, bytes) };
    let rc = unsafe {
        sys::cudaMemcpyAsync(
            d_x.0 as *mut c_void,
            h_x.as_ptr() as *const c_void,
            bytes,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(anyhow!("memcpy h→d"));
    }

    let stream = CUstream(std::ptr::null_mut());
    let rc = ggml_cuda_op_scale_f32(
        &kernels.scale,
        d_x,
        d_y,
        args.k,
        args.scale,
        args.bias,
        stream,
    );
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("scale launch failed: {rc}"));
    }
    unsafe { cuStreamSynchronize(stream) };

    let mut h_y = vec![0.0_f32; n];
    unsafe {
        sys::cudaMemcpyAsync(
            h_y.as_mut_ptr() as *mut c_void,
            d_y.0 as *const c_void,
            bytes,
            2,
            std::ptr::null_mut(),
        );
        cuStreamSynchronize(stream);
    }

    let mut max_abs = 0.0_f32;
    for i in 0..n {
        let expected = args.scale * h_x[i] + args.bias;
        let d = (h_y[i] - expected).abs();
        if d > max_abs {
            max_abs = d;
        }
    }
    println!(
        "scale (scale={} bias={}): max |gpu - cpu| = {:.3e}",
        args.scale, args.bias, max_abs
    );

    unsafe { cuMemFree_v2(d_x) };
    unsafe { cuMemFree_v2(d_y) };
    unsafe { sys::ggml_backend_free(backend) };

    if max_abs > args.tol {
        Err(anyhow!("FAILED tol {:.3e}", args.tol))
    } else {
        println!("PASSED (tol {:.3e})", args.tol);
        Ok(())
    }
}
