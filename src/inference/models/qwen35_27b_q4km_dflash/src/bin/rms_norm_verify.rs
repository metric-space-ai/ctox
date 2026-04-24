//! Bit-exact-ish verifier for the bare-metal `rms_norm` port.
//!
//! Allocates a known f32 tensor on the CUDA device, runs it through
//! `cuda_port::ops::norm::ggml_cuda_op_rms_norm` (our Rust port of
//! the ggml-cuda host-side dispatcher, launching the vendored
//! `rms_norm_f32<256/1024,false,false>` kernel via PTX + Driver API),
//! then compares the device-side output against a CPU reference
//! implementation of RMSNorm.
//!
//! RMSNorm reference, per row:
//!
//! ```text
//! y[i] = x[i] / sqrt(mean(x[0..ncols]^2) + eps)
//! ```
//!
//! Tolerance: the kernel uses `rsqrt` + fused f32 math; the CPU
//! reference does the same sequence of ops in f32. For ncols up to
//! ~4k we expect exact equality; for larger ncols small ULP drift
//! is possible from the warp-level reduction order. Default
//! threshold: `abs_max_diff < 1e-5`.
//!
//! Usage:
//!
//! ```text
//! qwen35-27b-q4km-dflash-rms-norm-verify [--ncols N] [--nrows N] [--eps E]
//! ```

use std::ffi::c_void;
use std::os::raw::c_int;

use anyhow::{anyhow, Context, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, CUdeviceptr, CUDA_SUCCESS,
    CUstream,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::norm::ggml_cuda_op_rms_norm;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-rms-norm-verify")]
struct Args {
    /// Feature dim (row width). Chooses the <256> or <1024> kernel.
    #[arg(long, default_value_t = 5120)]
    ncols: i32,
    /// Number of rows to process.
    #[arg(long, default_value_t = 16)]
    nrows: i32,
    /// RMSNorm eps.
    #[arg(long, default_value_t = 1e-6)]
    eps: f32,
    /// CUDA device index.
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    /// Max abs-diff threshold. Passes if the observed max is below.
    #[arg(long, default_value_t = 1e-5)]
    tol: f32,
}

/// CPU reference for rms_norm_f32: per-row
/// `y[i] = x[i] / sqrt(mean(x^2) + eps)`.
fn rms_norm_cpu(x: &[f32], y: &mut [f32], ncols: usize, nrows: usize, eps: f32) {
    assert_eq!(x.len(), ncols * nrows);
    assert_eq!(y.len(), ncols * nrows);
    for row in 0..nrows {
        let off = row * ncols;
        // mean(x^2)
        let mut ss = 0.0_f64;
        for i in 0..ncols {
            let v = x[off + i] as f64;
            ss += v * v;
        }
        let mean = ss / (ncols as f64);
        let scale = 1.0_f64 / (mean + eps as f64).sqrt();
        for i in 0..ncols {
            y[off + i] = (x[off + i] as f64 * scale) as f32;
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // 1. Bring up the CUDA backend via ggml. This creates a primary
    //    context on the requested device and registers it with the
    //    driver — our Driver-API calls share that context, so we
    //    don't need to call cuCtxCreate ourselves.
    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!(
            "ggml_backend_cuda_init failed for device {}",
            args.cuda_device
        ));
    }

    // 2. cuInit is a no-op after ggml initialized — idempotent.
    let rc = unsafe { cuInit(0) };
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("cuInit: {}", dflash::cuda_port::driver::error_string(rc)));
    }

    // 3. Resolve the ported kernels (loads norm.ptx, looks up the
    //    two mangled rms_norm_f32 entries).
    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!("ok: rms_norm kernels resolved (b256 + b1024)");

    // 4. Allocate input + output on the device.
    let ncols = args.ncols as usize;
    let nrows = args.nrows as usize;
    let n_elem = ncols * nrows;
    let bytes = (n_elem * std::mem::size_of::<f32>()) as libc::size_t;

    let mut d_x = CUdeviceptr(0);
    let mut d_y = CUdeviceptr(0);
    let rc = unsafe { cuMemAlloc_v2(&mut d_x, bytes) };
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("cuMemAlloc x: {}", dflash::cuda_port::driver::error_string(rc)));
    }
    let rc = unsafe { cuMemAlloc_v2(&mut d_y, bytes) };
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("cuMemAlloc y: {}", dflash::cuda_port::driver::error_string(rc)));
    }

    // 5. Fill host-side input with a reproducible pattern, copy to
    //    device. We use a small-amplitude sinusoid so the mean is
    //    non-trivial and the normalizer is stable.
    let mut h_x = vec![0.0_f32; n_elem];
    for row in 0..nrows {
        for i in 0..ncols {
            let phase = (i as f32 * 0.0173).sin() + (row as f32 * 0.091).cos();
            h_x[row * ncols + i] = 0.5 + 0.25 * phase;
        }
    }
    // cudaMemcpy host→device (sync).
    let rc_cp = unsafe {
        sys::cudaMemcpyAsync(
            d_x.0 as *mut c_void,
            h_x.as_ptr() as *const c_void,
            bytes,
            /* cudaMemcpyHostToDevice = 1 */ 1,
            std::ptr::null_mut(),
        )
    };
    if rc_cp != 0 {
        return Err(anyhow!("cudaMemcpyAsync h→d: code {rc_cp}"));
    }

    // 6. Launch our Rust-ported dispatcher.
    //    Shape: [ncols, nrows, 1, 1]. Byte strides: [4, 4*ncols, 4*ncols*nrows, same].
    let ne00 = args.ncols;
    let ne01 = args.nrows;
    let ne02: c_int = 1;
    let ne03: c_int = 1;
    let nb00: i64 = 4;
    let nb01: i64 = 4 * (args.ncols as i64);
    let nb02: i64 = nb01 * (args.nrows as i64);
    let nb03: i64 = nb02;

    let stream: CUstream = CUstream(std::ptr::null_mut()); // default stream
    let rc = ggml_cuda_op_rms_norm(
        &kernels.rms_norm,
        d_x,
        d_y,
        ne00,
        ne01,
        ne02,
        ne03,
        nb00,
        nb01,
        nb02,
        nb03,
        args.eps,
        stream,
    );
    if rc != CUDA_SUCCESS {
        return Err(anyhow!(
            "cuLaunchKernel rms_norm: {}",
            dflash::cuda_port::driver::error_string(rc)
        ));
    }
    let rc = unsafe { cuStreamSynchronize(stream) };
    if rc != CUDA_SUCCESS {
        return Err(anyhow!(
            "cuStreamSynchronize: {}",
            dflash::cuda_port::driver::error_string(rc)
        ));
    }
    println!(
        "ok: dispatched rms_norm_f32<{}> × {} rows",
        if ne00 < 1024 { 256 } else { 1024 },
        ne01
    );

    // 7. Copy device output back to host.
    let mut h_y_gpu = vec![0.0_f32; n_elem];
    let rc_cp = unsafe {
        sys::cudaMemcpyAsync(
            h_y_gpu.as_mut_ptr() as *mut c_void,
            d_y.0 as *const c_void,
            bytes,
            /* cudaMemcpyDeviceToHost = 2 */ 2,
            std::ptr::null_mut(),
        )
    };
    if rc_cp != 0 {
        return Err(anyhow!("cudaMemcpyAsync d→h: code {rc_cp}"));
    }
    // Sync default stream to complete the memcpy.
    let rc = unsafe { cuStreamSynchronize(stream) };
    if rc != CUDA_SUCCESS {
        return Err(anyhow!(
            "cuStreamSynchronize (post-memcpy): {}",
            dflash::cuda_port::driver::error_string(rc)
        ));
    }

    // 8. CPU reference.
    let mut h_y_ref = vec![0.0_f32; n_elem];
    rms_norm_cpu(&h_x, &mut h_y_ref, ncols, nrows, args.eps);

    // 9. Compare.
    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for i in 0..n_elem {
        let d = (h_y_gpu[i] - h_y_ref[i]).abs();
        if d > max_abs {
            max_abs = d;
            max_idx = i;
        }
    }
    println!(
        "max |gpu - ref| = {:.3e} at index {} (ncols={}, nrows={}, eps={})",
        max_abs, max_idx, ncols, nrows, args.eps
    );
    println!(
        "  gpu[{}] = {:.9e}  ref[{}] = {:.9e}",
        max_idx, h_y_gpu[max_idx], max_idx, h_y_ref[max_idx]
    );

    // 10. Free + teardown.
    let _ = unsafe { cuMemFree_v2(d_x) };
    let _ = unsafe { cuMemFree_v2(d_y) };
    unsafe { sys::ggml_backend_free(backend) };

    if max_abs > args.tol {
        return Err(anyhow!("VERIFY FAILED: max abs diff {max_abs:.3e} > tol {:.3e}", args.tol));
    }
    println!("VERIFY PASSED (tol {:.3e})", args.tol);
    Ok(())
}
