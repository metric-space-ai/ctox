//! Hybrid-executor smoke test.
//!
//! Exercises the combined Rust-native + ggml-cuda-fallback dispatch
//! path on a realistic 3-op chain:
//!
//!   x₁ = rms_norm(x₀, 1e-6)                (Rust-native cuda_port)
//!   x₂ = mul_mat(x₁, W)                    (ggml-cuda fallback)
//!   x₃ = add(x₂, bias)                     (Rust-native cuda_port)
//!
//! All tensors are allocated through `GgmlBackendCtx::realize()` so
//! every ggml_tensor has a proper `->buffer` set. The Rust-native
//! path extracts `tensor->data` as a CUdeviceptr via
//! `as_rust_tensor()`; the fallback path uses the same tensor
//! handles in a `compute()` closure.
//!
//! The comparison target is the identical 3-op chain run entirely
//! through ggml_backend_graph_compute. Both sides share kernel PTX,
//! so drift should be ~0.

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{cuInit, ensure_current_context, CUstream};
use dflash::cuda_port::fallback::GgmlBackendCtx;
use dflash::cuda_port::graph::{add, rms_norm, ExecCtx};
use dflash::cuda_port::module::porter;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-hybrid-smoke")]
struct Args {
    #[arg(long, default_value_t = 128)]
    m: i64,
    #[arg(long, default_value_t = 64)]
    n: i64,
    #[arg(long, default_value_t = 32)]
    k: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    #[arg(long, default_value_t = 1e-5)]
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
    println!("[hybrid] kernels resolved");

    // ggml convention: mul_mat(A [n, m], B [n, k]) → C [m, k]
    let (m, n, k) = (args.m, args.n, args.k);
    let nx = (n * m) as usize;
    let nw = (n * k) as usize;
    let ny = (m * k) as usize;

    let mut h_x = vec![0.0_f32; nx];
    let mut h_w = vec![0.0_f32; nw];
    let mut h_bias = vec![0.0_f32; ny];
    for (i, v) in h_x.iter_mut().enumerate() {
        *v = ((i as f32) * 0.011).sin() * 2.0 + 0.1 * (i as f32).cos();
    }
    for (i, v) in h_w.iter_mut().enumerate() {
        *v = ((i as f32) * 0.019).cos() * 0.5;
    }
    for (i, v) in h_bias.iter_mut().enumerate() {
        *v = 0.01 * (i as f32);
    }

    // ══════════════════════════════════════════════════════════
    // Hybrid path — Rust-native rms_norm + Rust-native add,
    // ggml-fallback mul_mat.
    // ══════════════════════════════════════════════════════════
    let mut gctx = GgmlBackendCtx::new(backend).map_err(|e| anyhow!(e))?;
    let t_x = gctx.new_tensor_f32([n, m, 1, 1], "x");
    let t_x1 = gctx.new_tensor_f32([n, m, 1, 1], "x1");
    let t_w = gctx.new_tensor_f32([n, k, 1, 1], "w");
    let t_bias = gctx.new_tensor_f32([m, k, 1, 1], "bias");
    let t_out = gctx.new_tensor_f32([m, k, 1, 1], "out");
    gctx.realize().map_err(|e| anyhow!(e))?;
    println!("[hybrid] tensors realized on backend");

    gctx.upload_f32(t_x, &h_x).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_w, &h_w).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_bias, &h_bias).map_err(|e| anyhow!(e))?;

    let stream = CUstream(std::ptr::null_mut());
    let exec = ExecCtx::new(kernels, stream);

    // Step 1 — RMSNorm (Rust-native)
    let rt_x = gctx.as_rust_tensor(t_x);
    let rt_x1 = gctx.as_rust_tensor(t_x1);
    rms_norm(&exec, &rt_x, &rt_x1, 1e-6).map_err(|e| anyhow!(e))?;
    // Sync so ggml sees rms_norm's writes before mul_mat reads them.
    unsafe { dflash::cuda_port::driver::cuStreamSynchronize(stream) };

    // Step 2 — mul_mat (ggml-cuda fallback). ggml allocates the
    // output tensor from the context's metadata arena (buffer is
    // already backing it — ggml_backend_graph_compute will bind
    // the new tensor into the same buffer).
    let t_mm = gctx
        .compute(|gc| unsafe { sys::ggml_mul_mat(gc, t_x1, t_w) })
        .map_err(|e| anyhow!(e))?;

    // Step 3 — add (Rust-native)
    let rt_mm = gctx.as_rust_tensor(t_mm);
    let rt_bias = gctx.as_rust_tensor(t_bias);
    let rt_out = gctx.as_rust_tensor(t_out);
    add(&exec, &rt_mm, &rt_bias, &rt_out).map_err(|e| anyhow!(e))?;
    unsafe { dflash::cuda_port::driver::cuStreamSynchronize(stream) };

    let h_got = gctx.download_f32(t_out, ny).map_err(|e| anyhow!(e))?;

    // ══════════════════════════════════════════════════════════
    // Reference — same chain, all through ggml.
    // ══════════════════════════════════════════════════════════
    let mut gref = GgmlBackendCtx::new(backend).map_err(|e| anyhow!(e))?;
    let r_x = gref.new_tensor_f32([n, m, 1, 1], "r_x");
    let r_w = gref.new_tensor_f32([n, k, 1, 1], "r_w");
    let r_bias = gref.new_tensor_f32([m, k, 1, 1], "r_bias");
    gref.realize().map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_x, &h_x).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_w, &h_w).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_bias, &h_bias).map_err(|e| anyhow!(e))?;
    let r_out = gref
        .compute(|gc| unsafe {
            let n1 = sys::ggml_rms_norm(gc, r_x, 1e-6);
            let mm = sys::ggml_mul_mat(gc, n1, r_w);
            sys::ggml_add(gc, mm, r_bias)
        })
        .map_err(|e| anyhow!(e))?;
    let h_ref = gref.download_f32(r_out, ny).map_err(|e| anyhow!(e))?;

    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for i in 0..ny {
        let d = (h_got[i] - h_ref[i]).abs();
        if d > max_abs {
            max_abs = d;
            max_idx = i;
        }
    }

    // Drop the contexts before the backend to release the buffers.
    drop(gctx);
    drop(gref);
    unsafe { sys::ggml_backend_free(backend) };

    println!(
        "[hybrid] chain-vs-ggml: max |hybrid - ggml| = {:.3e} at idx {} (got {:.6e}, want {:.6e})",
        max_abs, max_idx, h_got[max_idx], h_ref[max_idx]
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "hybrid FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("[hybrid] PASSED (tol {:.3e})", args.tol);
    Ok(())
}
