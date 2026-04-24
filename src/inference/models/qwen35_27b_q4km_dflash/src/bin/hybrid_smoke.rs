//! Hybrid-executor smoke test.
//!
//! Exercises the combined Rust-native + ggml-cuda-fallback dispatch
//! path on a realistic 3-op chain:
//!
//!   x₁ = rms_norm(x₀, 1e-6)                (Rust-native cuda_port)
//!   x₂ = mul_mat(x₁, W)                    (ggml-cuda fallback)
//!   x₃ = add(x₂, x₁)                       (Rust-native cuda_port)
//!
//! Why this matters: it's the first test where Rust's cuda_port
//! dispatcher and ggml-cuda's dispatcher operate on the SAME
//! device-memory tensors, in sequence, within one logical graph.
//! If this works, we have a path to bringing up the full Qwen3.5
//! forward before mmq/fattn/gdn are ported.
//!
//! The comparison target is `ggml_backend_graph_compute` running
//! the same 3-op graph through ggml-cuda alone. Expected drift is
//! near zero (both sides use the same PTX for rms_norm + add; the
//! only difference is who orchestrates the launches).

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{
    cuInit, cuMemAlloc_v2, cuMemFree_v2, cuStreamSynchronize, ensure_current_context,
    CUdeviceptr, CUstream,
};
use dflash::cuda_port::fallback::{exec_ggml_single, wrap_tensor};
use dflash::cuda_port::graph::{add, rms_norm, DType, ExecCtx, Tensor};
use dflash::cuda_port::module::porter;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-hybrid-smoke")]
struct Args {
    #[arg(long, default_value_t = 128)]
    m: i64, // rows of x
    #[arg(long, default_value_t = 64)]
    n: i64, // cols of x, rows of W (matmul contracts on this)
    #[arg(long, default_value_t = 32)]
    k: i64, // output cols (W has shape [k, n], matmul out [m, k])
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    #[arg(long, default_value_t = 1e-5)]
    tol: f32,
}

fn alloc_f32(n: usize) -> Result<CUdeviceptr> {
    let mut p = CUdeviceptr(0);
    let rc = unsafe { cuMemAlloc_v2(&mut p, (n * 4) as libc::size_t) };
    if rc != 0 {
        return Err(anyhow!("cuMemAlloc_v2: {rc}"));
    }
    Ok(p)
}

fn upload(dst: CUdeviceptr, host: &[f32]) -> Result<()> {
    let rc = unsafe {
        sys::cudaMemcpyAsync(
            dst.0 as *mut c_void,
            host.as_ptr() as *const c_void,
            (host.len() * 4) as libc::size_t,
            1,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        Err(anyhow!("memcpy h→d: {rc}"))
    } else {
        Ok(())
    }
}

fn download(src: CUdeviceptr, stream: CUstream, n: usize) -> Result<Vec<f32>> {
    let mut host = vec![0.0_f32; n];
    unsafe {
        sys::cudaMemcpyAsync(
            host.as_mut_ptr() as *mut c_void,
            src.0 as *const c_void,
            (n * 4) as libc::size_t,
            2,
            std::ptr::null_mut(),
        );
        cuStreamSynchronize(stream);
    }
    Ok(host)
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

    // Shape setup. In ggml convention: mul_mat(A [n, m], B [n, k]) → C [m, k]
    // where "n" is the contracted (inner) dim and comes first.
    //   A has shape [n_rows_a, n_cols_a] = [n, m]  (n inner, m outer)
    //   B has shape [n_rows_b, n_cols_b] = [n, k]  (n inner, k outer)
    //   C has shape [m, k]
    //
    // For us: x (rms input) = [n_cols, n_rows] = [n, m]
    //         W (weight)    = [n, k]
    //         x2 = mul_mat(x1, W)  → [m, k]
    //         x3 = add(x2, bias)   → [m, k]
    //
    // RMSNorm runs along axis 0 (inner), so normalizing x (shape [n, m])
    // normalizes each m-slice of n elements.
    let (m, n, k) = (args.m, args.n, args.k);

    let nx = (n * m) as usize; // x
    let nw = (n * k) as usize; // W
    let ny = (m * k) as usize; // y

    let mut h_x = vec![0.0_f32; nx];
    let mut h_w = vec![0.0_f32; nw];
    for (i, v) in h_x.iter_mut().enumerate() {
        *v = ((i as f32) * 0.011).sin() * 2.0 + 0.1 * (i as f32).cos();
    }
    for (i, v) in h_w.iter_mut().enumerate() {
        *v = ((i as f32) * 0.019).cos() * 0.5;
    }
    // "Bias"-like tensor added at the end, shape [m, k] same as mul_mat output.
    let mut h_bias = vec![0.0_f32; ny];
    for (i, v) in h_bias.iter_mut().enumerate() {
        *v = 0.01 * (i as f32);
    }

    let d_x = alloc_f32(nx)?;
    let d_x1 = alloc_f32(nx)?;
    let d_w = alloc_f32(nw)?;
    let d_y = alloc_f32(ny)?;
    let d_bias = alloc_f32(ny)?;
    let d_out = alloc_f32(ny)?;
    upload(d_x, &h_x)?;
    upload(d_w, &h_w)?;
    upload(d_bias, &h_bias)?;

    let t_x = Tensor::contiguous(d_x, DType::F32, [n, m, 1, 1]);
    let t_x1 = Tensor::contiguous(d_x1, DType::F32, [n, m, 1, 1]);
    let t_w = Tensor::contiguous(d_w, DType::F32, [n, k, 1, 1]);
    let t_y = Tensor::contiguous(d_y, DType::F32, [m, k, 1, 1]);
    let t_bias = Tensor::contiguous(d_bias, DType::F32, [m, k, 1, 1]);
    let t_out = Tensor::contiguous(d_out, DType::F32, [m, k, 1, 1]);

    let stream = CUstream(std::ptr::null_mut());
    let ctx = ExecCtx::new(kernels, stream);

    // ── Step 1 — RMSNorm via Rust-native cuda_port ────────────
    let t_start = std::time::Instant::now();
    rms_norm(&ctx, &t_x, &t_x1, 1e-6).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };
    let rms_us = t_start.elapsed().as_micros();
    println!("[hybrid] step 1 (rms_norm, Rust-native): {rms_us} µs");

    // ── Step 2 — mul_mat via ggml-cuda fallback ───────────────
    let t_start = std::time::Instant::now();
    exec_ggml_single(backend, stream, |gctx| unsafe {
        let a = wrap_tensor(gctx, &t_x1, "x1");
        let b = wrap_tensor(gctx, &t_w, "w");
        let c = sys::ggml_mul_mat(gctx, a, b);
        // Point mul_mat's output tensor at our pre-allocated d_y.
        (*c).data = d_y.0 as *mut c_void;
        (*c).nb[0] = 4;
        (*c).nb[1] = (m as usize) * 4;
        (*c).nb[2] = (m as usize) * (k as usize) * 4;
        (*c).nb[3] = (m as usize) * (k as usize) * 4;
        c
    })
    .map_err(|e| anyhow!(e))?;
    let mm_us = t_start.elapsed().as_micros();
    println!("[hybrid] step 2 (mul_mat, ggml-fallback): {mm_us} µs");

    // ── Step 3 — add via Rust-native cuda_port ────────────────
    let t_start = std::time::Instant::now();
    add(&ctx, &t_y, &t_bias, &t_out).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };
    let add_us = t_start.elapsed().as_micros();
    println!("[hybrid] step 3 (add, Rust-native): {add_us} µs");

    let h_got = download(d_out, stream, ny)?;

    // Reference: do the same 3-step chain via ggml-cuda end-to-end.
    // Build graph: x → rms_norm → mul_mat → add → out_ref.
    let d_out_ref = alloc_f32(ny)?;
    exec_ggml_single(backend, stream, |gctx| unsafe {
        let a = wrap_tensor(gctx, &t_x, "x_ref");
        let w = wrap_tensor(gctx, &t_w, "w_ref");
        let bias = wrap_tensor(gctx, &t_bias, "bias_ref");
        let n1 = sys::ggml_rms_norm(gctx, a, 1e-6);
        let mm = sys::ggml_mul_mat(gctx, n1, w);
        let out = sys::ggml_add(gctx, mm, bias);
        (*out).data = d_out_ref.0 as *mut c_void;
        (*out).nb[0] = 4;
        (*out).nb[1] = (m as usize) * 4;
        (*out).nb[2] = (m as usize) * (k as usize) * 4;
        (*out).nb[3] = (m as usize) * (k as usize) * 4;
        out
    })
    .map_err(|e| anyhow!(e))?;
    let h_ref = download(d_out_ref, stream, ny)?;

    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for i in 0..ny {
        let d = (h_got[i] - h_ref[i]).abs();
        if d > max_abs {
            max_abs = d;
            max_idx = i;
        }
    }

    unsafe {
        cuMemFree_v2(d_x);
        cuMemFree_v2(d_x1);
        cuMemFree_v2(d_w);
        cuMemFree_v2(d_y);
        cuMemFree_v2(d_bias);
        cuMemFree_v2(d_out);
        cuMemFree_v2(d_out_ref);
        sys::ggml_backend_free(backend);
    }

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
