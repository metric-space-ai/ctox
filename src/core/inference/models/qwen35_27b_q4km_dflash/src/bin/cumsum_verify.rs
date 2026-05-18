//! Bit-close verifier for the bare-metal `cumsum` port (f32
//! fallback, no CUB).
//!
//! Inclusive prefix-sum along axis 0 per row. Tolerance is a few ulp
//! because the device-side reduction runs in a different summation
//! order than a sequential host loop — for f32 the observed drift
//! on row-lengths up to a few K is sub-1e-4.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::cumsum::ggml_cuda_op_cumsum_f32;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-cumsum-verify")]
struct Args {
    #[arg(long, default_value_t = 512)]
    ne0: i64,
    #[arg(long, default_value_t = 8)]
    ne1: i64,
    #[arg(long, default_value_t = 2)]
    ne2: i64,
    #[arg(long, default_value_t = 1)]
    ne3: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    /// Tolerance for max |gpu - cpu| per element. Block-parallel
    /// reductions introduce a modest ULP-level drift that scales
    /// with row length — ~1e-4 for 512-wide rows is fine.
    #[arg(long, default_value_t = 5e-4)]
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
    println!("cumsum kernel resolved");

    let ne = [args.ne0, args.ne1, args.ne2, args.ne3];
    let n = (ne[0] * ne[1] * ne[2] * ne[3]) as usize;
    let bytes = (n * std::mem::size_of::<f32>()) as libc::size_t;

    // Contiguous element strides — upstream nb / type_size.
    let s = [
        1i64,
        ne[0],
        ne[0] * ne[1],
        ne[0] * ne[1] * ne[2],
    ];

    let mut h_src = vec![0.0_f32; n];
    for (i, v) in h_src.iter_mut().enumerate() {
        // Keep values small so the running prefix fits in f32 mantissa.
        *v = ((i as f32) * 0.017).sin() * 0.5;
    }

    let mut d_src = CUdeviceptr(0);
    let mut d_dst = CUdeviceptr(0);
    unsafe { cuMemAlloc_v2(&mut d_src, bytes) };
    unsafe { cuMemAlloc_v2(&mut d_dst, bytes) };
    unsafe {
        sys::cudaMemcpyAsync(
            d_src.0 as *mut c_void,
            h_src.as_ptr() as *const c_void,
            bytes,
            1,
            std::ptr::null_mut(),
        );
    }

    let stream = CUstream(std::ptr::null_mut());
    let rc = ggml_cuda_op_cumsum_f32(&kernels.cumsum, d_src, d_dst, ne, s, s, stream);
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("cumsum launch failed: {rc}"));
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

    unsafe { cuMemFree_v2(d_src) };
    unsafe { cuMemFree_v2(d_dst) };
    unsafe { sys::ggml_backend_free(backend) };

    // CPU reference — sequential inclusive scan per row.
    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for i3 in 0..ne[3] {
        for i2 in 0..ne[2] {
            for i1 in 0..ne[1] {
                let base = (i1 * s[1] + i2 * s[2] + i3 * s[3]) as usize;
                let mut acc = 0.0_f64;
                for i0 in 0..ne[0] {
                    acc += h_src[base + i0 as usize] as f64;
                    let diff = (h_dst[base + i0 as usize] as f64 - acc).abs() as f32;
                    if diff > max_abs {
                        max_abs = diff;
                        max_idx = base + i0 as usize;
                    }
                }
            }
        }
    }

    println!(
        "cumsum<f32>: max |gpu - cpu| = {:.3e} at idx {}  gpu={:.6e}",
        max_abs, max_idx, h_dst[max_idx]
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "cumsum FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("cumsum<f32>: PASSED (tol {:.3e})", args.tol);
    Ok(())
}
