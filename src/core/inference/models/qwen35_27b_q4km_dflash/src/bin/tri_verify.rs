//! Bit-exact verifier for the bare-metal `tri` port (f32).
//!
//! Runs each of the four ggml_tri_type variants on a contiguous f32
//! matrix and compares the device output against a CPU reference
//! that walks the exact same index arithmetic the kernel uses.
//! All four compares must be bit-equal (no arithmetic — just
//! copies and zeros).

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::tri::{ggml_cuda_op_tri_f32, TriType};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-tri-verify")]
struct Args {
    #[arg(long, default_value_t = 32)]
    ne0: i64,
    #[arg(long, default_value_t = 32)]
    ne1: i64,
    #[arg(long, default_value_t = 2)]
    ne2: i64,
    #[arg(long, default_value_t = 1)]
    ne3: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
}

/// Contiguous nb (in elements): nb[0]=1, then cumulative products.
fn contiguous_nb(ne: [i64; 4]) -> [i64; 4] {
    let mut nb = [1i64; 4];
    for d in 1..4 {
        nb[d] = nb[d - 1] * ne[d - 1];
    }
    nb
}

fn tri_cpu(ttype: TriType, src: &[f32], ne: [i64; 4], nb: [i64; 4]) -> Vec<f32> {
    let n = (ne[0] * ne[1] * ne[2] * ne[3]) as usize;
    let mut dst = vec![0.0_f32; n];
    let (prefix_keep, add_to_split) = match ttype {
        TriType::Lower => (true, 0i64),
        TriType::LowerDiag => (true, 1i64),
        TriType::UpperDiag => (false, 0i64),
        TriType::Upper => (false, 1i64),
    };
    for i3 in 0..ne[3] {
        for i2 in 0..ne[2] {
            for i1 in 0..ne[1] {
                let base = i1 * nb[1] + i2 * nb[2] + i3 * nb[3];
                let split = i1 + add_to_split;
                for i0 in 0..ne[0] {
                    let idx = (base + i0) as usize;
                    if prefix_keep {
                        dst[idx] = if i0 < split { src[idx] } else { 0.0 };
                    } else {
                        dst[idx] = if i0 < split { 0.0 } else { src[idx] };
                    }
                }
            }
        }
    }
    dst
}

fn run(ttype: TriType, h_src: &[f32], ne: [i64; 4], nb: [i64; 4], cuda_device: i32) -> Result<()> {
    let n = h_src.len();
    let bytes = (n * std::mem::size_of::<f32>()) as libc::size_t;

    let mut d_src = CUdeviceptr(0);
    let mut d_dst = CUdeviceptr(0);
    unsafe { cuMemAlloc_v2(&mut d_src, bytes) };
    unsafe { cuMemAlloc_v2(&mut d_dst, bytes) };
    let rc = unsafe {
        sys::cudaMemcpyAsync(
            d_src.0 as *mut c_void,
            h_src.as_ptr() as *const c_void,
            bytes,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(anyhow!("memcpy h→d src: {rc}"));
    }

    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;

    let stream = CUstream(std::ptr::null_mut());
    let rc = ggml_cuda_op_tri_f32(&kernels.tri, d_src, d_dst, ne, nb, nb, ttype, stream);
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("tri launch failed: {rc}"));
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

    let expect = tri_cpu(ttype, h_src, ne, nb);
    let mut bad = 0usize;
    let mut first: Option<(usize, f32, f32)> = None;
    for i in 0..n {
        if h_dst[i] != expect[i] {
            bad += 1;
            if first.is_none() {
                first = Some((i, h_dst[i], expect[i]));
            }
        }
    }

    if bad > 0 {
        if let Some((idx, got, want)) = first {
            return Err(anyhow!(
                "tri<{ttype:?}>: {bad}/{n} mismatches (first at {idx}: got {got}, want {want})"
            ));
        }
    }
    println!("tri<{ttype:?}>: PASSED ({n} elems)");
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!("ggml_backend_cuda_init failed"));
    }
    unsafe { cuInit(0) };
    ensure_current_context(args.cuda_device).map_err(|e| anyhow!("ctx: {e}"))?;
    let _kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!("tri kernels resolved (4 variants)");

    let ne = [args.ne0, args.ne1, args.ne2, args.ne3];
    let nb = contiguous_nb(ne);
    let n = (ne[0] * ne[1] * ne[2] * ne[3]) as usize;

    let mut h_src = vec![0.0_f32; n];
    for (i, slot) in h_src.iter_mut().enumerate() {
        *slot = (i as f32 * 0.013).sin() + 0.7 * (i as f32 * 0.007).cos();
    }

    run(TriType::Lower, &h_src, ne, nb, args.cuda_device)?;
    run(TriType::LowerDiag, &h_src, ne, nb, args.cuda_device)?;
    run(TriType::Upper, &h_src, ne, nb, args.cuda_device)?;
    run(TriType::UpperDiag, &h_src, ne, nb, args.cuda_device)?;

    unsafe { sys::ggml_backend_free(backend) };
    println!("ALL TRI PASSED");
    Ok(())
}
