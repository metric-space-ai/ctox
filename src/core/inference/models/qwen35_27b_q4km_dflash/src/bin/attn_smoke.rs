//! Qwen3.5-shaped attention block through the hybrid executor.
//!
//!   x                                       # [hidden, seq]
//!   x_normed = rms_norm(x)                  # Rust-native
//!   q = mul_mat(x_normed, W_q)              # ggml-fallback
//!   k = mul_mat(x_normed, W_k)              # ggml-fallback
//!   v = mul_mat(x_normed, W_v)              # ggml-fallback
//!   # [omitted: rope(q), rope(k) — Qwen3.5 M-RoPE needs position ids]
//!   attn = flash_attn_ext(q, k, v, mask, scale, 0, 0) (ggml-fallback)
//!   out_proj = mul_mat(attn, W_o)           # ggml-fallback
//!   out = x + out_proj                      # Rust-native
//!
//! This is the FA2 path. Q/K/V/O projections are separate
//! compute() calls, flash_attn_ext is one more, residual add is
//! Rust-native. RoPE is skipped for this smoke because full M-RoPE
//! requires position-id tensors we're not constructing synthetically
//! here — the same 2 ops are already individually verified.
//!
//! Uses the standard GQA shape convention: n_heads heads of head_dim
//! dims for Q, n_kv_heads heads for K/V. Qwen3.5 defaults:
//!   head_dim=256, n_heads=24, n_kv_heads=4 → Q=[6144,s] KV=[1024,s]
//!
//! Compared against the same 6-op graph run entirely through
//! ggml_backend_graph_compute.

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{cuInit, cuStreamSynchronize, ensure_current_context, CUstream};
use dflash::cuda_port::fallback::GgmlBackendCtx;
use dflash::cuda_port::graph::{add, rms_norm, ExecCtx};
use dflash::cuda_port::module::porter;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-attn-smoke")]
struct Args {
    /// Hidden dim. Default 512 for fast smoke; use 6144 for real 27B Q dim.
    #[arg(long, default_value_t = 512)]
    hidden: i64,
    #[arg(long, default_value_t = 8)]
    n_heads: i64,
    #[arg(long, default_value_t = 2)]
    n_kv_heads: i64,
    #[arg(long, default_value_t = 64)]
    head_dim: i64,
    #[arg(long, default_value_t = 16)]
    seq: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    #[arg(long, default_value_t = 1e-2)]
    tol: f32,
}

fn det_fill(v: &mut [f32], seed: f32) {
    let norm = (v.len() as f32).sqrt();
    for (i, s) in v.iter_mut().enumerate() {
        *s = ((i as f32 * 0.013 + seed).sin() * 0.5
            + (i as f32 * 0.007 + seed * 2.0).cos() * 0.25)
            / norm;
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let h = args.hidden;
    let nh = args.n_heads;
    let nkv = args.n_kv_heads;
    let hd = args.head_dim;
    let s = args.seq;
    let q_dim = nh * hd;
    let kv_dim = nkv * hd;

    assert_eq!(q_dim, h, "q_dim must equal hidden for this smoke");

    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!("ggml_backend_cuda_init failed"));
    }
    unsafe { cuInit(0) };
    ensure_current_context(args.cuda_device).map_err(|e| anyhow!("ctx: {e}"))?;
    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!(
        "[attn] hidden={h} n_heads={nh} n_kv_heads={nkv} head_dim={hd} seq={s}"
    );

    let n_x = (h * s) as usize;
    let n_wq = (h * q_dim) as usize;
    let n_wkv = (h * kv_dim) as usize;
    let n_wo = (q_dim * h) as usize;

    let mut h_x = vec![0.0_f32; n_x];
    let mut h_wq = vec![0.0_f32; n_wq];
    let mut h_wk = vec![0.0_f32; n_wkv];
    let mut h_wv = vec![0.0_f32; n_wkv];
    let mut h_wo = vec![0.0_f32; n_wo];
    det_fill(&mut h_x, 0.0);
    det_fill(&mut h_wq, 1.0);
    det_fill(&mut h_wk, 2.0);
    det_fill(&mut h_wv, 3.0);
    det_fill(&mut h_wo, 4.0);

    let scale = 1.0_f32 / (hd as f32).sqrt();

    // ═══ Hybrid path ═══════════════════════════════════════════
    let mut gctx = GgmlBackendCtx::new(backend).map_err(|e| anyhow!(e))?;
    let t_x = gctx.new_tensor_f32([h, s, 1, 1], "x");
    let t_xn = gctx.new_tensor_f32([h, s, 1, 1], "x_normed");
    let t_wq = gctx.new_tensor_f32([h, q_dim, 1, 1], "Wq");
    let t_wk = gctx.new_tensor_f32([h, kv_dim, 1, 1], "Wk");
    let t_wv = gctx.new_tensor_f32([h, kv_dim, 1, 1], "Wv");
    let t_wo = gctx.new_tensor_f32([q_dim, h, 1, 1], "Wo");
    let t_out = gctx.new_tensor_f32([h, s, 1, 1], "out");
    gctx.realize().map_err(|e| anyhow!(e))?;

    gctx.upload_f32(t_x, &h_x).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wq, &h_wq).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wk, &h_wk).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wv, &h_wv).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wo, &h_wo).map_err(|e| anyhow!(e))?;

    let stream = CUstream(std::ptr::null_mut());
    let exec = ExecCtx::new(kernels, stream);

    let t_start = std::time::Instant::now();

    // 1. rms_norm (Rust)
    let rt_x = gctx.as_rust_tensor(t_x);
    let rt_xn = gctx.as_rust_tensor(t_xn);
    rms_norm(&exec, &rt_x, &rt_xn, 1e-6).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    // 2-5. Q/K/V proj + FA + O proj through ggml, as one compute()
    //      — reduces kernel-scheduling drift vs reference.
    let t_attn = gctx
        .compute(|gc| unsafe {
            let q_flat = sys::ggml_mul_mat(gc, t_wq, t_xn); // [q_dim, s]
            let k_flat = sys::ggml_mul_mat(gc, t_wk, t_xn); // [kv_dim, s]
            let v_flat = sys::ggml_mul_mat(gc, t_wv, t_xn); // [kv_dim, s]

            // Reshape flat projections into (head_dim, n_heads, seq, 1)
            // and permute to (head_dim, seq, n_heads, 1) — the layout
            // ggml_flash_attn_ext expects.
            let q_3d = sys::ggml_reshape_3d(gc, q_flat, hd, nh, s);
            let k_3d = sys::ggml_reshape_3d(gc, k_flat, hd, nkv, s);
            let v_3d = sys::ggml_reshape_3d(gc, v_flat, hd, nkv, s);

            let q_p = sys::ggml_cont(gc, sys::ggml_permute(gc, q_3d, 0, 2, 1, 3));
            let k_p = sys::ggml_cont(gc, sys::ggml_permute(gc, k_3d, 0, 2, 1, 3));
            let v_p = sys::ggml_cont(gc, sys::ggml_permute(gc, v_3d, 0, 2, 1, 3));

            // FA with no mask (null), scale=1/sqrt(head_dim), max_bias=0, softcap=0.
            let attn = sys::ggml_flash_attn_ext(
                gc,
                q_p,
                k_p,
                v_p,
                std::ptr::null_mut(),
                scale,
                0.0,
                0.0,
            );

            // Permute back to (q_dim, seq) layout and project to hidden.
            let attn_flat = sys::ggml_cont_2d(gc, attn, q_dim, s);
            sys::ggml_mul_mat(gc, t_wo, attn_flat) // [h, s]
        })
        .map_err(|e| anyhow!(e))?;

    // 6. residual (Rust)
    let rt_attn = gctx.as_rust_tensor(t_attn);
    let rt_out = gctx.as_rust_tensor(t_out);
    add(&exec, &rt_x, &rt_attn, &rt_out).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    let hybrid_us = t_start.elapsed().as_micros();
    println!("[attn] hybrid: {hybrid_us} µs");

    let h_got = gctx.download_f32(t_out, n_x).map_err(|e| anyhow!(e))?;

    // ═══ Reference — same graph, all ggml ══════════════════════
    let mut gref = GgmlBackendCtx::new(backend).map_err(|e| anyhow!(e))?;
    let r_x = gref.new_tensor_f32([h, s, 1, 1], "r_x");
    let r_wq = gref.new_tensor_f32([h, q_dim, 1, 1], "r_Wq");
    let r_wk = gref.new_tensor_f32([h, kv_dim, 1, 1], "r_Wk");
    let r_wv = gref.new_tensor_f32([h, kv_dim, 1, 1], "r_Wv");
    let r_wo = gref.new_tensor_f32([q_dim, h, 1, 1], "r_Wo");
    gref.realize().map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_x, &h_x).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wq, &h_wq).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wk, &h_wk).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wv, &h_wv).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wo, &h_wo).map_err(|e| anyhow!(e))?;

    let t_start = std::time::Instant::now();
    let r_out = gref
        .compute(|gc| unsafe {
            let norm = sys::ggml_rms_norm(gc, r_x, 1e-6);
            let q = sys::ggml_mul_mat(gc, r_wq, norm);
            let k = sys::ggml_mul_mat(gc, r_wk, norm);
            let v = sys::ggml_mul_mat(gc, r_wv, norm);
            let q3 = sys::ggml_reshape_3d(gc, q, hd, nh, s);
            let k3 = sys::ggml_reshape_3d(gc, k, hd, nkv, s);
            let v3 = sys::ggml_reshape_3d(gc, v, hd, nkv, s);
            let qp = sys::ggml_cont(gc, sys::ggml_permute(gc, q3, 0, 2, 1, 3));
            let kp = sys::ggml_cont(gc, sys::ggml_permute(gc, k3, 0, 2, 1, 3));
            let vp = sys::ggml_cont(gc, sys::ggml_permute(gc, v3, 0, 2, 1, 3));
            let fa = sys::ggml_flash_attn_ext(
                gc,
                qp,
                kp,
                vp,
                std::ptr::null_mut(),
                scale,
                0.0,
                0.0,
            );
            let flat = sys::ggml_cont_2d(gc, fa, q_dim, s);
            let proj = sys::ggml_mul_mat(gc, r_wo, flat);
            sys::ggml_add(gc, r_x, proj)
        })
        .map_err(|e| anyhow!(e))?;
    let ref_us = t_start.elapsed().as_micros();
    let h_ref = gref.download_f32(r_out, n_x).map_err(|e| anyhow!(e))?;
    println!("[attn] ggml reference: {ref_us} µs");

    let mut max_abs = 0.0_f32;
    let mut max_idx = 0usize;
    for i in 0..n_x {
        let d = (h_got[i] - h_ref[i]).abs();
        if d > max_abs {
            max_abs = d;
            max_idx = i;
        }
    }

    drop(gctx);
    drop(gref);
    unsafe { sys::ggml_backend_free(backend) };

    println!(
        "[attn] max |diff| = {:.3e} at idx {} (got {:.6e}, want {:.6e})",
        max_abs, max_idx, h_got[max_idx], h_ref[max_idx]
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "attn FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("[attn] PASSED (tol {:.3e})", args.tol);
    Ok(())
}
