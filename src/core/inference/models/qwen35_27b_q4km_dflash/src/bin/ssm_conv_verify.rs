//! Bit-close verifier for the bare-metal `ssm_conv` port.
//!
//! Runs `ssm_conv_f32<true, 128, 4>` (short-token path, apply_silu,
//! d_conv=4 — the Qwen3.5 DeltaNet default) on a small deterministic
//! input and compares against a pure-CPU reference computed with
//! the same formula from ssm-conv.cu:31-51.
//!
//! Kernel semantics (per (i_s batch, tid inner-channel)):
//!   for i = 0..n_t:
//!     if i == 0: x[j]     = src_row[tid*stride_x + j]  for j in [0, d_conv)
//!     else      : x[(i-1) % d_conv] = src_row[tid*stride_x + i + d_conv - 1]
//!     sumf = Σ_j  x[(i + j) % d_conv] * w[j]
//!     y[i, tid] = apply_silu ? silu(sumf) : sumf

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::ssm_conv::{ggml_cuda_op_ssm_conv_f32, SsmConvLayout};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-ssm-conv-verify")]
struct Args {
    /// tokens per sequence (≤32 exercises the short-token path).
    #[arg(long, default_value_t = 16)]
    n_t: i64,
    /// d_inner — must be % 128 == 0 (split_d_inner).
    #[arg(long, default_value_t = 128)]
    n_inner: i64,
    /// Number of sequences in the batch.
    #[arg(long, default_value_t = 2)]
    n_s: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    #[arg(long, default_value_t = 1e-5)]
    tol: f32,
}

fn silu_cpu(x: f32) -> f32 {
    let xd = x as f64;
    (xd * (1.0 / (1.0 + (-xd).exp()))) as f32
}

fn main() -> Result<()> {
    let args = Args::parse();
    let nc: i64 = 4; // d_conv
    let apply_silu = true;

    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!("ggml_backend_cuda_init failed"));
    }
    unsafe { cuInit(0) };
    ensure_current_context(args.cuda_device).map_err(|e| anyhow!("ctx: {e}"))?;
    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!(
        "[ssm_conv] kernels resolved; shape: n_t={} n_inner={} n_s={}, nc={}, silu={}",
        args.n_t, args.n_inner, args.n_s, nc, apply_silu
    );

    // src0 "conv_x": shape [n_t + nc - 1, n_inner, n_s] contiguous.
    //   per (i_s, tid):  stride_x = n_t + nc - 1 floats
    let in_len = (args.n_t + nc - 1) as usize;
    let n_src0 = in_len * args.n_inner as usize * args.n_s as usize;
    // src1 "weights": shape [nc, n_inner] contiguous.
    let n_src1 = (nc * args.n_inner) as usize;
    // dst: shape [n_inner, n_t, n_s] contiguous.
    let n_dst = args.n_inner as usize * args.n_t as usize * args.n_s as usize;

    let mut h_x = vec![0.0_f32; n_src0];
    let mut h_w = vec![0.0_f32; n_src1];
    for (i, v) in h_x.iter_mut().enumerate() {
        *v = ((i as f32) * 0.013).sin() * 0.5 + 0.25 * (i as f32).cos();
    }
    for (i, v) in h_w.iter_mut().enumerate() {
        *v = ((i as f32) * 0.017).cos() * 0.25;
    }

    let mut d_x = CUdeviceptr(0);
    let mut d_w = CUdeviceptr(0);
    let mut d_y = CUdeviceptr(0);
    let esz = std::mem::size_of::<f32>() as libc::size_t;
    unsafe { cuMemAlloc_v2(&mut d_x, (n_src0 as libc::size_t) * esz) };
    unsafe { cuMemAlloc_v2(&mut d_w, (n_src1 as libc::size_t) * esz) };
    unsafe { cuMemAlloc_v2(&mut d_y, (n_dst as libc::size_t) * esz) };

    let rc = unsafe {
        sys::cudaMemcpyAsync(
            d_x.0 as *mut c_void,
            h_x.as_ptr() as *const c_void,
            (n_src0 as libc::size_t) * esz,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(anyhow!("memcpy x: {rc}"));
    }
    let rc = unsafe {
        sys::cudaMemcpyAsync(
            d_w.0 as *mut c_void,
            h_w.as_ptr() as *const c_void,
            (n_src1 as libc::size_t) * esz,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(anyhow!("memcpy w: {rc}"));
    }

    // Byte strides the kernel expects. Our host contiguous layout:
    //   src0: nb0 = 4, nb1 = stride_x*4, nb2 = stride_x*n_inner*4
    //   src1: nb0 = 4, nb1 = nc*4
    //   dst : nb0 = 4, nb1 = n_inner*4, nb2 = n_inner*n_t*4
    let stride_x_bytes = (in_len * 4) as i32;
    let layout = SsmConvLayout {
        src0_nb0: 4,
        src0_nb1: stride_x_bytes,
        src0_nb2: stride_x_bytes * args.n_inner as i32,
        src1_nb1: (nc * 4) as i32,
        dst_nb0: 4,
        dst_nb1: (args.n_inner * 4) as i32,
        dst_nb2: (args.n_inner * args.n_t * 4) as i32,
    };

    let stream = CUstream(std::ptr::null_mut());
    let rc = ggml_cuda_op_ssm_conv_f32(
        &kernels.ssm_conv,
        apply_silu,
        d_x,
        d_w,
        d_y,
        &layout,
        nc,
        args.n_inner,
        args.n_t,
        args.n_s,
        stream,
    );
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("ssm_conv launch: {rc}"));
    }
    unsafe { cuStreamSynchronize(stream) };

    let mut h_y = vec![0.0_f32; n_dst];
    unsafe {
        sys::cudaMemcpyAsync(
            h_y.as_mut_ptr() as *mut c_void,
            d_y.0 as *const c_void,
            (n_dst as libc::size_t) * esz,
            2,
            std::ptr::null_mut(),
        );
        cuStreamSynchronize(stream);
    }

    // CPU reference — walk the same index arithmetic the kernel uses
    // (ssm-conv.cu:31-51).
    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    let n_t_us = args.n_t as usize;
    let n_in = args.n_inner as usize;
    let nc_us = nc as usize;
    for i_s in 0..args.n_s as usize {
        for tid in 0..n_in {
            // Load initial x (window of size d_conv)
            let mut x = [0.0f32; 9]; // max nc=9
            let row_start = i_s * in_len * n_in + tid * in_len;
            for j in 0..nc_us {
                x[j] = h_x[row_start + j];
            }
            for i in 0..n_t_us {
                if i > 0 {
                    x[(i - 1) % nc_us] = h_x[row_start + i + nc_us - 1];
                }
                let mut sumf = 0.0_f64;
                for j in 0..nc_us {
                    sumf += x[(i + j) % nc_us] as f64 * h_w[tid * nc_us + j] as f64;
                }
                let mut expected = sumf as f32;
                if apply_silu {
                    expected = silu_cpu(expected);
                }
                let dst_idx = i_s * n_in * n_t_us + i * n_in + tid;
                let diff = (h_y[dst_idx] - expected).abs();
                if diff > max_abs {
                    max_abs = diff;
                    max_idx = dst_idx;
                }
            }
        }
    }

    unsafe {
        cuMemFree_v2(d_x);
        cuMemFree_v2(d_w);
        cuMemFree_v2(d_y);
        sys::ggml_backend_free(backend);
    }

    println!(
        "[ssm_conv] max |gpu - cpu| = {:.3e} at idx {} (gpu={:.6e})",
        max_abs, max_idx, h_y[max_idx]
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "ssm_conv FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("[ssm_conv] PASSED (tol {:.3e})", args.tol);
    Ok(())
}
