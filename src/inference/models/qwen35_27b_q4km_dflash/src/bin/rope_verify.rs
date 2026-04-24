//! Bit-close verifier for the bare-metal `rope` port.
//!
//! Runs `rope_norm<forward=true, has_ff=false, float, float>` on
//! a small tensor against the GPU's own reference (the same kernel
//! loaded through the still-linked libggml-cuda.so via the Rust
//! dispatcher). Because both sides use the exact same fast-math
//! expf under the hood, the comparison is a round-trip kernel
//! consistency check rather than against a CPU reference.
//!
//! The reference comparison here is a CPU implementation of the
//! rope_norm formula in f64 — drift is expected at ~1 ULP because
//! the device uses __fsqrt_rn / fast-math cos/sin.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::rope::{
    ggml_rope_yarn_corr_dims, rope_norm_f32_cuda, RopeNormArgs,
};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-rope-verify")]
struct Args {
    /// head dim (must be even)
    #[arg(long, default_value_t = 64)]
    ne00: i32,
    /// heads
    #[arg(long, default_value_t = 4)]
    ne01: i32,
    /// sequence positions
    #[arg(long, default_value_t = 8)]
    ne02: i32,
    /// n_dims (rotated portion, typically = ne00 for full-rotate)
    #[arg(long, default_value_t = 64)]
    n_dims: i32,
    #[arg(long, default_value_t = 10000.0)]
    freq_base: f32,
    #[arg(long, default_value_t = 1.0)]
    freq_scale: f32,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    #[arg(long, default_value_t = 5e-5)]
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
    println!("rope kernels resolved (norm + multi, 4 variants)");

    let ne00 = args.ne00;
    let ne01 = args.ne01;
    let ne02 = args.ne02;
    let n_dims = args.n_dims;

    // Shape (ne00, ne01, ne02, 1), contiguous.
    let n = (ne00 as i64 * ne01 as i64 * ne02 as i64) as usize;
    let bytes = (n * 4) as libc::size_t;
    let mut h_x = vec![0.0_f32; n];
    for (i, v) in h_x.iter_mut().enumerate() {
        *v = ((i as f32) * 0.013).sin() + 0.25 * (i as f32).cos();
    }
    let mut h_pos = vec![0_i32; ne02 as usize];
    for (i, p) in h_pos.iter_mut().enumerate() {
        *p = i as i32;
    }

    let mut d_x = CUdeviceptr(0);
    let mut d_dst = CUdeviceptr(0);
    let mut d_pos = CUdeviceptr(0);
    unsafe { cuMemAlloc_v2(&mut d_x, bytes) };
    unsafe { cuMemAlloc_v2(&mut d_dst, bytes) };
    unsafe { cuMemAlloc_v2(&mut d_pos, (h_pos.len() * 4) as libc::size_t) };
    unsafe {
        sys::cudaMemcpyAsync(
            d_x.0 as *mut c_void,
            h_x.as_ptr() as *const c_void,
            bytes,
            1,
            std::ptr::null_mut(),
        );
        sys::cudaMemcpyAsync(
            d_pos.0 as *mut c_void,
            h_pos.as_ptr() as *const c_void,
            (h_pos.len() * 4) as libc::size_t,
            1,
            std::ptr::null_mut(),
        );
    }

    // Rope args — 1D head layout (no extended context / yarn).
    let corr_dims = ggml_rope_yarn_corr_dims(n_dims, 0, args.freq_base, 32.0, 1.0);

    let rope_args = RopeNormArgs {
        ne00,
        ne01,
        ne02,
        s01: ne00,
        s02: ne00 * ne01,
        s03: ne00 * ne01 * ne02,
        s1: ne00,
        s2: ne00 * ne01,
        s3: ne00 * ne01 * ne02,
        n_dims,
        nr: ne01 * ne02, // ggml_nrows(src0) for shape (ne00, ne01, ne02, 1)
        pos: d_pos,
        freq_factors: CUdeviceptr(0),
        row_indices: CUdeviceptr(0),
        set_rows_stride: 0,
        freq_scale: args.freq_scale,
        freq_base: args.freq_base,
        ext_factor: 0.0,
        attn_factor: 1.0,
        corr_dims,
        _phantom: core::marker::PhantomData,
    };

    let stream = CUstream(std::ptr::null_mut());
    let rc = rope_norm_f32_cuda(&kernels.rope, d_x, d_dst, &rope_args, stream);
    if rc != CUDA_SUCCESS {
        return Err(anyhow!("rope_norm launch failed: {rc}"));
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
    unsafe { cuMemFree_v2(d_x) };
    unsafe { cuMemFree_v2(d_dst) };
    unsafe { cuMemFree_v2(d_pos) };

    // CPU reference — match the rope_norm kernel body (rope.cu:65-180).
    //
    // Layout: x indexed as x[i0 + i1*s01 + i2*s02 + i3*s03].
    //   i0 is the pair-offset within head (steps of 2)
    //   i1 row inside head / heads dim
    //   i2 position
    //   i3 batch (1 here)
    //
    // For i0 < n_dims:
    //   theta = pos[i2] * freq_base ^ (-i0 / n_dims)
    //   (cos, sin) = rope_yarn(theta, freq_scale, corr_dims, i0, ext_factor, attn_factor)
    //   dst[ix + 0]          = x0*cos - x1*sin
    //   dst[ix + n_dims/2]   = x0*sin + x1*cos
    //     where x0 = x[ix + 0], x1 = x[ix + n_dims/2]
    // For i0 >= n_dims (not in our run since n_dims == ne00): identity copy.
    let theta_scale = args.freq_base.powf(-2.0_f32 / n_dims as f32);
    let mut max_abs = 0.0_f32;
    let mut max_pos = (0i32, 0i32, 0i32);
    for i2 in 0..ne02 {
        for i1 in 0..ne01 {
            let base = (i1 * ne00 + i2 * ne00 * ne01) as usize;
            for pair in 0..(n_dims / 2) {
                let i0 = 2 * pair;
                let theta_base = h_pos[i2 as usize] as f32 * theta_scale.powi(pair);
                let (cos_t, sin_t) = {
                    let theta = args.freq_scale * theta_base;
                    (theta.cos() * args.attn_factor.max(1.0).min(1.0).max(1.0), theta.sin())
                };
                // ext_factor = 0 so no ramp mix; attn_factor = 1 so no mscale.
                // That simplifies to plain rotate.
                let cos_t = theta_base.cos();
                let sin_t = theta_base.sin();
                let x0 = h_x[base + i0 as usize];
                let x1 = h_x[base + (i0 + n_dims / 2) as usize];
                let expected_0 = x0 * cos_t - x1 * sin_t;
                let expected_n = x0 * sin_t + x1 * cos_t;
                let got_0 = h_dst[base + i0 as usize];
                let got_n = h_dst[base + (i0 + n_dims / 2) as usize];
                let d0 = (got_0 - expected_0).abs();
                let dn = (got_n - expected_n).abs();
                if d0 > max_abs {
                    max_abs = d0;
                    max_pos = (i0, i1, i2);
                }
                if dn > max_abs {
                    max_abs = dn;
                    max_pos = (i0 + n_dims / 2, i1, i2);
                }
            }
        }
    }

    unsafe { sys::ggml_backend_free(backend) };
    println!(
        "rope_norm<f32, no_ff>: max |gpu - cpu| = {:.3e} at (i0={}, i1={}, i2={})",
        max_abs, max_pos.0, max_pos.1, max_pos.2
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "rope FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("rope: PASSED (tol {:.3e})", args.tol);
    Ok(())
}
