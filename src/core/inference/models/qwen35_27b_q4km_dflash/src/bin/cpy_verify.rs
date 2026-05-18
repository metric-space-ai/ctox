//! Bit-close verifier for the bare-metal `cpy` port.
//!
//! Exercises the generic `cpy_scalar<cpy_1_scalar<src, dst>>`
//! kernel for three dtype pairs:
//!   • f32 → f32  (exact)
//!   • f32 → f16  (f16 rounding — compare after casting back)
//!   • f16 → f16  (exact)
//!
//! Uses contiguous layouts so nb* strides match the natural element
//! order; the kernel still walks per-axis strides so the generic
//! path is exercised end-to-end.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::cpy::{ggml_cuda_op_cpy_scalar, CpyDtype};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-cpy-verify")]
struct Args {
    #[arg(long, default_value_t = 17)]
    ne0: i64,
    #[arg(long, default_value_t = 13)]
    ne1: i64,
    #[arg(long, default_value_t = 5)]
    ne2: i64,
    #[arg(long, default_value_t = 3)]
    ne3: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
}

fn contig_nb(ne: [i64; 4], elem_size: i64) -> [i64; 4] {
    let mut nb = [elem_size; 4];
    for d in 1..4 {
        nb[d] = nb[d - 1] * ne[d - 1];
    }
    nb
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
    println!("cpy kernels resolved (f32→f32, f32→f16, f16→f16)");

    let ne = [args.ne0, args.ne1, args.ne2, args.ne3];
    let n = (ne[0] * ne[1] * ne[2] * ne[3]) as usize;

    // Build host-side source buffers (f32 pattern; f16 path pre-converts).
    let mut h_src_f32 = vec![0.0_f32; n];
    for (i, v) in h_src_f32.iter_mut().enumerate() {
        *v = ((i as f32) * 0.031).sin() + 0.1 * (i as f32).cos();
    }

    // ---- f32 → f32 ----
    {
        let bytes = (n * 4) as libc::size_t;
        let nb = contig_nb(ne, 4);
        let mut d_src = CUdeviceptr(0);
        let mut d_dst = CUdeviceptr(0);
        unsafe { cuMemAlloc_v2(&mut d_src, bytes) };
        unsafe { cuMemAlloc_v2(&mut d_dst, bytes) };
        unsafe {
            sys::cudaMemcpyAsync(
                d_src.0 as *mut c_void,
                h_src_f32.as_ptr() as *const c_void,
                bytes,
                1,
                std::ptr::null_mut(),
            );
        }
        let stream = CUstream(std::ptr::null_mut());
        let rc = ggml_cuda_op_cpy_scalar(
            &kernels.cpy,
            CpyDtype::F32ToF32,
            d_src,
            d_dst,
            n as i64,
            [ne[0], ne[1], ne[2]],
            nb,
            [ne[0], ne[1], ne[2]],
            nb,
            stream,
        );
        if rc != CUDA_SUCCESS {
            return Err(anyhow!("f32→f32 launch: {rc}"));
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

        if h_dst != h_src_f32 {
            return Err(anyhow!("f32→f32: mismatch"));
        }
        println!("cpy<f32→f32>: PASSED ({n} elems)");
    }

    // ---- f32 → f16 ----
    {
        let src_bytes = (n * 4) as libc::size_t;
        let dst_bytes = (n * 2) as libc::size_t;
        let src_nb = contig_nb(ne, 4);
        let dst_nb = contig_nb(ne, 2);
        let mut d_src = CUdeviceptr(0);
        let mut d_dst = CUdeviceptr(0);
        unsafe { cuMemAlloc_v2(&mut d_src, src_bytes) };
        unsafe { cuMemAlloc_v2(&mut d_dst, dst_bytes) };
        unsafe {
            sys::cudaMemcpyAsync(
                d_src.0 as *mut c_void,
                h_src_f32.as_ptr() as *const c_void,
                src_bytes,
                1,
                std::ptr::null_mut(),
            );
        }
        let stream = CUstream(std::ptr::null_mut());
        let rc = ggml_cuda_op_cpy_scalar(
            &kernels.cpy,
            CpyDtype::F32ToF16,
            d_src,
            d_dst,
            n as i64,
            [ne[0], ne[1], ne[2]],
            src_nb,
            [ne[0], ne[1], ne[2]],
            dst_nb,
            stream,
        );
        if rc != CUDA_SUCCESS {
            return Err(anyhow!("f32→f16 launch: {rc}"));
        }
        unsafe { cuStreamSynchronize(stream) };

        let mut h_dst = vec![0u16; n];
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

        // Reference: host-side half-round of h_src_f32.
        let mut bad = 0usize;
        let mut first: Option<(usize, u16, u16)> = None;
        for i in 0..n {
            let expect = half::f16::from_f32(h_src_f32[i]).to_bits();
            if h_dst[i] != expect {
                bad += 1;
                if first.is_none() {
                    first = Some((i, h_dst[i], expect));
                }
            }
        }
        if bad > 0 {
            if let Some((idx, got, want)) = first {
                return Err(anyhow!(
                    "f32→f16: {bad}/{n} mismatches (first at {idx}: got 0x{got:04x}, want 0x{want:04x})"
                ));
            }
        }
        println!("cpy<f32→f16>: PASSED ({n} elems, bit-exact half-round)");
    }

    // ---- f16 → f16 ----
    {
        let bytes = (n * 2) as libc::size_t;
        let nb = contig_nb(ne, 2);
        let h_src_f16: Vec<u16> = h_src_f32
            .iter()
            .map(|&x| half::f16::from_f32(x).to_bits())
            .collect();

        let mut d_src = CUdeviceptr(0);
        let mut d_dst = CUdeviceptr(0);
        unsafe { cuMemAlloc_v2(&mut d_src, bytes) };
        unsafe { cuMemAlloc_v2(&mut d_dst, bytes) };
        unsafe {
            sys::cudaMemcpyAsync(
                d_src.0 as *mut c_void,
                h_src_f16.as_ptr() as *const c_void,
                bytes,
                1,
                std::ptr::null_mut(),
            );
        }
        let stream = CUstream(std::ptr::null_mut());
        let rc = ggml_cuda_op_cpy_scalar(
            &kernels.cpy,
            CpyDtype::F16ToF16,
            d_src,
            d_dst,
            n as i64,
            [ne[0], ne[1], ne[2]],
            nb,
            [ne[0], ne[1], ne[2]],
            nb,
            stream,
        );
        if rc != CUDA_SUCCESS {
            return Err(anyhow!("f16→f16 launch: {rc}"));
        }
        unsafe { cuStreamSynchronize(stream) };

        let mut h_dst = vec![0u16; n];
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

        if h_dst != h_src_f16 {
            return Err(anyhow!("f16→f16: mismatch"));
        }
        println!("cpy<f16→f16>: PASSED ({n} elems)");
    }

    unsafe { sys::ggml_backend_free(backend) };
    println!("ALL CPY PASSED");
    Ok(())
}
