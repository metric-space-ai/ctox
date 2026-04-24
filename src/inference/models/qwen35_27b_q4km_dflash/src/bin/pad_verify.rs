//! Bit-exact verifier for the bare-metal `pad` port (f32).
//!
//! Pads a contiguous src tensor into a larger contiguous dst with
//! per-axis left/right padding counts. Runs both non-circular and
//! circular modes and bit-compares against a CPU reference that
//! walks the same kernel-side index arithmetic.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream, CUDA_SUCCESS,
};
use dflash::cuda_port::module::porter;
use dflash::cuda_port::ops::pad::{ggml_cuda_op_pad_f32, PadParams};
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-pad-verify")]
struct Args {
    /// src shape.
    #[arg(long, default_value_t = 8)]
    src_ne0: i32,
    #[arg(long, default_value_t = 4)]
    src_ne1: i32,
    #[arg(long, default_value_t = 2)]
    src_ne2: i32,
    #[arg(long, default_value_t = 1)]
    src_ne3: i32,
    /// Left/right pads on each axis.
    #[arg(long, default_value_t = 1)]
    lp0: i32,
    #[arg(long, default_value_t = 2)]
    rp0: i32,
    #[arg(long, default_value_t = 0)]
    lp1: i32,
    #[arg(long, default_value_t = 1)]
    rp1: i32,
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
    println!("pad kernel resolved");

    let src_ne = [args.src_ne0, args.src_ne1, args.src_ne2, args.src_ne3];
    let lp = [args.lp0, args.lp1, 0, 0];
    let rp = [args.rp0, args.rp1, 0, 0];
    let dst_ne = [
        src_ne[0] + lp[0] + rp[0],
        src_ne[1] + lp[1] + rp[1],
        src_ne[2] + lp[2] + rp[2],
        src_ne[3] + lp[3] + rp[3],
    ];

    // Contiguous element strides.
    let src_s = [
        1usize,
        src_ne[0] as usize,
        (src_ne[0] * src_ne[1]) as usize,
        (src_ne[0] * src_ne[1] * src_ne[2]) as usize,
    ];

    let n_src = (src_ne[0] * src_ne[1] * src_ne[2] * src_ne[3]) as usize;
    let n_dst = (dst_ne[0] * dst_ne[1] * dst_ne[2] * dst_ne[3]) as usize;

    let mut h_src = vec![0.0_f32; n_src];
    for (i, v) in h_src.iter_mut().enumerate() {
        *v = (i as f32 * 0.11 + 1.0).sin();
    }

    let mut d_src = CUdeviceptr(0);
    let mut d_dst = CUdeviceptr(0);
    unsafe { cuMemAlloc_v2(&mut d_src, (n_src * 4) as libc::size_t) };
    unsafe { cuMemAlloc_v2(&mut d_dst, (n_dst * 4) as libc::size_t) };
    unsafe {
        sys::cudaMemcpyAsync(
            d_src.0 as *mut c_void,
            h_src.as_ptr() as *const c_void,
            (n_src * 4) as libc::size_t,
            1,
            std::ptr::null_mut(),
        );
    }

    for circular in [false, true] {
        let params = PadParams {
            lp,
            rp,
            circular,
        };
        let stream = CUstream(std::ptr::null_mut());
        let rc = ggml_cuda_op_pad_f32(&kernels.pad, d_src, d_dst, src_s, dst_ne, params, stream);
        if rc != CUDA_SUCCESS {
            return Err(anyhow!(
                "pad<circular={circular}> launch failed: {rc}"
            ));
        }
        unsafe { cuStreamSynchronize(stream) };

        let mut h_dst = vec![0.0_f32; n_dst];
        unsafe {
            sys::cudaMemcpyAsync(
                h_dst.as_mut_ptr() as *mut c_void,
                d_dst.0 as *const c_void,
                (n_dst * 4) as libc::size_t,
                2,
                std::ptr::null_mut(),
            );
            cuStreamSynchronize(stream);
        }

        // CPU reference — mirrors the kernel exactly.
        let mut bad = 0usize;
        let mut first: Option<(usize, f32, f32)> = None;
        for i3 in 0..dst_ne[3] {
            for i2 in 0..dst_ne[2] {
                for i1 in 0..dst_ne[1] {
                    for i0 in 0..dst_ne[0] {
                        let dst_idx = (i3 * (dst_ne[0] * dst_ne[1] * dst_ne[2])
                            + i2 * (dst_ne[0] * dst_ne[1])
                            + i1 * dst_ne[0]
                            + i0) as usize;
                        let expected = if !circular {
                            if i0 >= lp[0]
                                && i0 < dst_ne[0] - rp[0]
                                && i1 >= lp[1]
                                && i1 < dst_ne[1] - rp[1]
                                && i2 >= lp[2]
                                && i2 < dst_ne[2] - rp[2]
                                && i3 >= lp[3]
                                && i3 < dst_ne[3] - rp[3]
                            {
                                let i00 = (i0 - lp[0]) as usize;
                                let i01 = (i1 - lp[1]) as usize;
                                let i02 = (i2 - lp[2]) as usize;
                                let i03 = (i3 - lp[3]) as usize;
                                let sidx =
                                    i03 * src_s[3] + i02 * src_s[2] + i01 * src_s[1] + i00 * src_s[0];
                                h_src[sidx]
                            } else {
                                0.0
                            }
                        } else {
                            let ne00 = dst_ne[0] - lp[0] - rp[0];
                            let ne01 = dst_ne[1] - lp[1] - rp[1];
                            let ne02 = dst_ne[2] - lp[2] - rp[2];
                            let ne03 = dst_ne[3] - lp[3] - rp[3];
                            let wrap = |c: i32, sz: i32| ((c + sz) % sz) as usize;
                            let i00 = wrap(i0 - lp[0], ne00);
                            let i01 = wrap(i1 - lp[1], ne01);
                            let i02 = wrap(i2 - lp[2], ne02);
                            let i03 = wrap(i3 - lp[3], ne03);
                            let sidx =
                                i03 * src_s[3] + i02 * src_s[2] + i01 * src_s[1] + i00 * src_s[0];
                            h_src[sidx]
                        };
                        if h_dst[dst_idx] != expected {
                            bad += 1;
                            if first.is_none() {
                                first = Some((dst_idx, h_dst[dst_idx], expected));
                            }
                        }
                    }
                }
            }
        }

        if bad > 0 {
            if let Some((idx, got, want)) = first {
                return Err(anyhow!(
                    "pad<circular={circular}>: {bad}/{n_dst} mismatches (first at {idx}: got {got}, want {want})"
                ));
            }
        }
        println!("pad<circular={circular}>: PASSED (dst {n_dst} elems)");
    }

    unsafe { cuMemFree_v2(d_src) };
    unsafe { cuMemFree_v2(d_dst) };
    unsafe { sys::ggml_backend_free(backend) };
    println!("ALL PAD PASSED");
    Ok(())
}
