//! Bit-exact verifier for the bare-metal `fill` port.
//!
//! fill_kernel<T>(T * dst, const int64_t k, const T value) — writes
//! the constant `value` to every element of a contiguous buffer.
//! Reference is trivial (all elements must equal `value`); an exact
//! match is expected for both f32 and f16 variants.
//!
//! Runs both variants in sequence. Exits non-zero on any drift.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::fill::{ggml_cuda_op_fill_f16, ggml_cuda_op_fill_f32};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-fill-verify")]
struct Args {
    /// Total element count.
    #[arg(long, default_value_t = 32768)]
    k: i64,
    /// f32 fill value.
    #[arg(long, default_value_t = 3.14159_26)]
    value_f32: f32,
    /// f16 fill value (host-side; converted to half bits before the
    /// launch).
    #[arg(long, default_value_t = -2.5)]
    value_f16: f32,
    /// CUDA device index.
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Bring up the CUDA context via the still-linked ggml-cuda
    // backend (same pattern as the other verifiers).
    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!("ggml_backend_cuda_init failed"));
    }
    unsafe { cuInit(0) };
    ensure_current_context(args.cuda_device).map_err(|e| anyhow!("ctx: {e}"))?;
    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!("fill kernels resolved (f32 + f16)");

    let n = args.k as usize;

    // ── f32 path ─────────────────────────────────────────────────
    {
        let bytes = (n * std::mem::size_of::<f32>()) as libc::size_t;
        let mut d_y = CUdeviceptr(0);
        unsafe { cuMemAlloc_v2(&mut d_y, bytes) };

        let stream = CUstream(std::ptr::null_mut());
        let rc = ggml_cuda_op_fill_f32(&kernels.fill, d_y, args.k, args.value_f32, stream);
        if rc != CUDA_SUCCESS {
            return Err(anyhow!("fill<float> launch failed: {rc}"));
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
        unsafe { cuMemFree_v2(d_y) };

        let mut bad = 0usize;
        for &v in &h_y {
            if v != args.value_f32 {
                bad += 1;
            }
        }
        if bad > 0 {
            return Err(anyhow!(
                "fill<float>: {bad}/{n} elements != {}",
                args.value_f32
            ));
        }
        println!("fill<float>   PASSED (n={n}, value={})", args.value_f32);
    }

    // ── f16 path ─────────────────────────────────────────────────
    {
        let bytes = (n * std::mem::size_of::<u16>()) as libc::size_t;
        let mut d_y = CUdeviceptr(0);
        unsafe { cuMemAlloc_v2(&mut d_y, bytes) };

        // half::f16::to_bits() gives the IEEE-754-2008 binary16 bit
        // pattern CUDA's __half expects.
        let value_bits = half::f16::from_f32(args.value_f16).to_bits();

        let stream = CUstream(std::ptr::null_mut());
        let rc = ggml_cuda_op_fill_f16(&kernels.fill, d_y, args.k, value_bits, stream);
        if rc != CUDA_SUCCESS {
            return Err(anyhow!("fill<__half> launch failed: {rc}"));
        }
        unsafe { cuStreamSynchronize(stream) };

        let mut h_y = vec![0u16; n];
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
        unsafe { cuMemFree_v2(d_y) };

        let mut bad = 0usize;
        for &b in &h_y {
            if b != value_bits {
                bad += 1;
            }
        }
        if bad > 0 {
            return Err(anyhow!(
                "fill<__half>: {bad}/{n} elements != 0x{value_bits:04x}"
            ));
        }
        println!(
            "fill<__half>  PASSED (n={n}, value={} bits=0x{value_bits:04x})",
            args.value_f16
        );
    }

    unsafe { sys::ggml_backend_free(backend) };
    Ok(())
}
