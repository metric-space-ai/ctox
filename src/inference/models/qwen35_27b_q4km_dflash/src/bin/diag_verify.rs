//! Bit-exact verifier for the bare-metal `diag` port (f32).
//!
//! Materialises diag(src) where src has shape (ne0, 1, ne2, ne3)
//! and dst has shape (ne0, ne0, ne2, ne3). The result must match
//! src[batch, i0] on the diagonal (i1 == i0) and 0 elsewhere.
//!
//! Runs f32 only — f16 shares the same kernel template and is
//! wired through but not verified here because the Qwen3.5
//! forward path uses f32 for diag.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::diag::ggml_cuda_op_diag_f32;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-diag-verify")]
struct Args {
    /// src shape (ne0, 1, ne2, ne3). ne0 corresponds to the scalar
    /// count along the diagonal; ne2 × ne3 is the batch fan-out.
    #[arg(long, default_value_t = 64)]
    ne0: i64,
    #[arg(long, default_value_t = 3)]
    ne2: i64,
    #[arg(long, default_value_t = 2)]
    ne3: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
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
    println!("diag kernel resolved");

    let ne0 = args.ne0;
    let ne1 = ne0; // dst's ne1 == ne0 (square)
    let ne2 = args.ne2;
    let ne3 = args.ne3;

    let n_src = (ne0 * 1 * ne2 * ne3) as usize;
    let n_dst = (ne0 * ne1 * ne2 * ne3) as usize;
    let src_bytes = (n_src * std::mem::size_of::<f32>()) as libc::size_t;
    let dst_bytes = (n_dst * std::mem::size_of::<f32>()) as libc::size_t;

    let mut h_src = vec![0.0_f32; n_src];
    for (i, slot) in h_src.iter_mut().enumerate() {
        // Mix the linear index into something distinguishable so
        // comparisons never collapse by accident.
        *slot = (i as f32 * 0.031).sin() + 0.25 * (i as f32).cos();
    }

    let mut d_src = CUdeviceptr(0);
    let mut d_dst = CUdeviceptr(0);
    unsafe { cuMemAlloc_v2(&mut d_src, src_bytes) };
    unsafe { cuMemAlloc_v2(&mut d_dst, dst_bytes) };

    let rc = unsafe {
        sys::cudaMemcpyAsync(
            d_src.0 as *mut c_void,
            h_src.as_ptr() as *const c_void,
            src_bytes,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(anyhow!("memcpy h→d src: {rc}"));
    }

    let stream = CUstream(std::ptr::null_mut());
    let rc = ggml_cuda_op_diag_f32(
        &kernels.diag,
        d_dst,
        d_src,
        ne0,
        ne1,
        ne2,
        ne3,
        (ne0 * ne1 * ne2 * ne3),
        stream,
    );
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("diag launch failed: {rc}"));
    }
    unsafe { cuStreamSynchronize(stream) };

    let mut h_dst = vec![0.0_f32; n_dst];
    unsafe {
        sys::cudaMemcpyAsync(
            h_dst.as_mut_ptr() as *mut c_void,
            d_dst.0 as *const c_void,
            dst_bytes,
            2,
            std::ptr::null_mut(),
        );
        cuStreamSynchronize(stream);
    }

    unsafe { cuMemFree_v2(d_src) };
    unsafe { cuMemFree_v2(d_dst) };
    unsafe { sys::ggml_backend_free(backend) };

    // CPU reference: iterate exactly like the kernel.
    let mut bad = 0usize;
    let mut first_bad: Option<(usize, f32, f32)> = None;
    for i3 in 0..ne3 {
        for i2 in 0..ne2 {
            for i1 in 0..ne1 {
                for i0 in 0..ne0 {
                    let dst_idx = (((i3 * ne2 + i2) * ne1 + i1) * ne0 + i0) as usize;
                    let expected = if i0 == i1 {
                        let batch_idx = (i3 * ne2 + i2) as usize;
                        let src_idx = batch_idx * ne0 as usize + i0 as usize;
                        h_src[src_idx]
                    } else {
                        0.0
                    };
                    if h_dst[dst_idx] != expected {
                        bad += 1;
                        if first_bad.is_none() {
                            first_bad = Some((dst_idx, h_dst[dst_idx], expected));
                        }
                    }
                }
            }
        }
    }

    if bad > 0 {
        if let Some((idx, got, want)) = first_bad {
            return Err(anyhow!(
                "diag<f32>: {bad}/{n_dst} mismatches (first at {idx}: got {got}, want {want})"
            ));
        }
    }
    println!(
        "diag<f32> PASSED (shape {}x{}x{}x{} = {} elems)",
        ne0, ne1, ne2, ne3, n_dst
    );
    Ok(())
}
