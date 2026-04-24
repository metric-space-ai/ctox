//! Full Qwen3.5 transformer block (attention + FFN) through the
//! hybrid executor. Combines `attn_smoke` and `layer_smoke` into a
//! single 2-sub-block chain:
//!
//!   x                                       # [hidden, seq]
//!   ── attention half ──
//!   x1 = rms_norm(x)                        # Rust
//!   q = mul_mat(x1, Wq); k = mul_mat(..Wk); v = mul_mat(..Wv)
//!   attn = flash_attn_ext(q, k, v)          # (Q/K/V + FA + Wo grouped in 1 ggml compute)
//!   attn_out = mul_mat(attn, Wo)
//!   x2 = x + attn_out                       # Rust residual
//!   ── ffn half ──
//!   x3 = rms_norm(x2)                       # Rust
//!   gate = mul_mat(x3, W_gate)              # ggml
//!   up   = mul_mat(x3, W_up)                # ggml
//!   act  = silu(gate)                       # Rust
//!   gated = act * up                        # Rust
//!   down = mul_mat(gated, W_down)           # ggml
//!   out  = x2 + down                        # Rust residual
//!
//! Compared against the same DAG run entirely through ggml.
//! This is effectively one transformer-layer forward pass.

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{cuInit, cuStreamSynchronize, ensure_current_context, CUstream};
use dflash::cuda_port::fallback::GgmlBackendCtx;
use dflash::cuda_port::graph::{add, mul, rms_norm, silu, ExecCtx};
use dflash::cuda_port::module::porter;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-block-smoke")]
struct Args {
    #[arg(long, default_value_t = 512)]
    hidden: i64,
    #[arg(long, default_value_t = 8)]
    n_heads: i64,
    #[arg(long, default_value_t = 2)]
    n_kv_heads: i64,
    #[arg(long, default_value_t = 64)]
    head_dim: i64,
    #[arg(long, default_value_t = 1408)]
    ffn: i64,
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
    let f = args.ffn;
    let s = args.seq;
    let q_dim = nh * hd;
    let kv_dim = nkv * hd;
    assert_eq!(q_dim, h, "q_dim must equal hidden");

    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!("ggml_backend_cuda_init failed"));
    }
    unsafe { cuInit(0) };
    ensure_current_context(args.cuda_device).map_err(|e| anyhow!("ctx: {e}"))?;
    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!(
        "[block] hidden={h} heads={nh}/{nkv}×{hd} ffn={f} seq={s}"
    );

    let n_x = (h * s) as usize;

    let mut h_x = vec![0.0_f32; n_x];
    let mut h_wq = vec![0.0_f32; (h * q_dim) as usize];
    let mut h_wk = vec![0.0_f32; (h * kv_dim) as usize];
    let mut h_wv = vec![0.0_f32; (h * kv_dim) as usize];
    let mut h_wo = vec![0.0_f32; (q_dim * h) as usize];
    let mut h_wg = vec![0.0_f32; (h * f) as usize];
    let mut h_wu = vec![0.0_f32; (h * f) as usize];
    let mut h_wd = vec![0.0_f32; (f * h) as usize];
    det_fill(&mut h_x, 0.0);
    det_fill(&mut h_wq, 1.0);
    det_fill(&mut h_wk, 2.0);
    det_fill(&mut h_wv, 3.0);
    det_fill(&mut h_wo, 4.0);
    det_fill(&mut h_wg, 5.0);
    det_fill(&mut h_wu, 6.0);
    det_fill(&mut h_wd, 7.0);

    let scale = 1.0_f32 / (hd as f32).sqrt();

    // ═══ Hybrid path ═══════════════════════════════════════════
    let mut gctx = GgmlBackendCtx::new(backend).map_err(|e| anyhow!(e))?;
    let t_x = gctx.new_tensor_f32([h, s, 1, 1], "x");
    let t_x1 = gctx.new_tensor_f32([h, s, 1, 1], "x1");
    let t_x2 = gctx.new_tensor_f32([h, s, 1, 1], "x2");
    let t_x3 = gctx.new_tensor_f32([h, s, 1, 1], "x3");
    let t_act = gctx.new_tensor_f32([f, s, 1, 1], "act");
    let t_gated = gctx.new_tensor_f32([f, s, 1, 1], "gated");
    let t_out = gctx.new_tensor_f32([h, s, 1, 1], "out");
    let t_wq = gctx.new_tensor_f32([h, q_dim, 1, 1], "Wq");
    let t_wk = gctx.new_tensor_f32([h, kv_dim, 1, 1], "Wk");
    let t_wv = gctx.new_tensor_f32([h, kv_dim, 1, 1], "Wv");
    let t_wo = gctx.new_tensor_f32([q_dim, h, 1, 1], "Wo");
    let t_wg = gctx.new_tensor_f32([h, f, 1, 1], "Wgate");
    let t_wu = gctx.new_tensor_f32([h, f, 1, 1], "Wup");
    let t_wd = gctx.new_tensor_f32([f, h, 1, 1], "Wdown");
    gctx.realize().map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_x, &h_x).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wq, &h_wq).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wk, &h_wk).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wv, &h_wv).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wo, &h_wo).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wg, &h_wg).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wu, &h_wu).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wd, &h_wd).map_err(|e| anyhow!(e))?;

    let stream = CUstream(std::ptr::null_mut());
    let exec = ExecCtx::new(kernels, stream);

    let t_start = std::time::Instant::now();

    // Attention half
    let rt_x = gctx.as_rust_tensor(t_x);
    let rt_x1 = gctx.as_rust_tensor(t_x1);
    rms_norm(&exec, &rt_x, &rt_x1, 1e-6).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    let t_attn_proj = gctx
        .compute(|gc| unsafe {
            let q = sys::ggml_mul_mat(gc, t_wq, t_x1);
            let k = sys::ggml_mul_mat(gc, t_wk, t_x1);
            let v = sys::ggml_mul_mat(gc, t_wv, t_x1);
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
            sys::ggml_mul_mat(gc, t_wo, flat)
        })
        .map_err(|e| anyhow!(e))?;

    let rt_attn = gctx.as_rust_tensor(t_attn_proj);
    let rt_x2 = gctx.as_rust_tensor(t_x2);
    add(&exec, &rt_x, &rt_attn, &rt_x2).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    // FFN half
    let rt_x3 = gctx.as_rust_tensor(t_x3);
    rms_norm(&exec, &rt_x2, &rt_x3, 1e-6).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    let t_gate = gctx
        .compute(|gc| unsafe { sys::ggml_mul_mat(gc, t_wg, t_x3) })
        .map_err(|e| anyhow!(e))?;
    let t_up = gctx
        .compute(|gc| unsafe { sys::ggml_mul_mat(gc, t_wu, t_x3) })
        .map_err(|e| anyhow!(e))?;

    let rt_gate = gctx.as_rust_tensor(t_gate);
    let rt_act = gctx.as_rust_tensor(t_act);
    silu(&exec, &rt_gate, &rt_act).map_err(|e| anyhow!(e))?;
    let rt_up = gctx.as_rust_tensor(t_up);
    let rt_gated = gctx.as_rust_tensor(t_gated);
    mul(&exec, &rt_act, &rt_up, &rt_gated).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    let t_down = gctx
        .compute(|gc| unsafe { sys::ggml_mul_mat(gc, t_wd, t_gated) })
        .map_err(|e| anyhow!(e))?;

    let rt_down = gctx.as_rust_tensor(t_down);
    let rt_out = gctx.as_rust_tensor(t_out);
    add(&exec, &rt_x2, &rt_down, &rt_out).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    let hybrid_us = t_start.elapsed().as_micros();
    println!("[block] hybrid: {hybrid_us} µs");
    let h_got = gctx.download_f32(t_out, n_x).map_err(|e| anyhow!(e))?;

    // ═══ Reference — same full block, all ggml ═════════════════
    let mut gref = GgmlBackendCtx::new(backend).map_err(|e| anyhow!(e))?;
    let r_x = gref.new_tensor_f32([h, s, 1, 1], "r_x");
    let r_wq = gref.new_tensor_f32([h, q_dim, 1, 1], "r_Wq");
    let r_wk = gref.new_tensor_f32([h, kv_dim, 1, 1], "r_Wk");
    let r_wv = gref.new_tensor_f32([h, kv_dim, 1, 1], "r_Wv");
    let r_wo = gref.new_tensor_f32([q_dim, h, 1, 1], "r_Wo");
    let r_wg = gref.new_tensor_f32([h, f, 1, 1], "r_Wg");
    let r_wu = gref.new_tensor_f32([h, f, 1, 1], "r_Wu");
    let r_wd = gref.new_tensor_f32([f, h, 1, 1], "r_Wd");
    gref.realize().map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_x, &h_x).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wq, &h_wq).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wk, &h_wk).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wv, &h_wv).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wo, &h_wo).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wg, &h_wg).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wu, &h_wu).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wd, &h_wd).map_err(|e| anyhow!(e))?;

    let t_start = std::time::Instant::now();
    let r_out = gref
        .compute(|gc| unsafe {
            // Attention
            let n1 = sys::ggml_rms_norm(gc, r_x, 1e-6);
            let q = sys::ggml_mul_mat(gc, r_wq, n1);
            let k = sys::ggml_mul_mat(gc, r_wk, n1);
            let v = sys::ggml_mul_mat(gc, r_wv, n1);
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
            let x2 = sys::ggml_add(gc, r_x, proj);
            // FFN
            let n2 = sys::ggml_rms_norm(gc, x2, 1e-6);
            let gate = sys::ggml_mul_mat(gc, r_wg, n2);
            let up = sys::ggml_mul_mat(gc, r_wu, n2);
            let act = sys::ggml_silu(gc, gate);
            let gated = sys::ggml_mul(gc, act, up);
            let down = sys::ggml_mul_mat(gc, r_wd, gated);
            sys::ggml_add(gc, x2, down)
        })
        .map_err(|e| anyhow!(e))?;
    let ref_us = t_start.elapsed().as_micros();
    let h_ref = gref.download_f32(r_out, n_x).map_err(|e| anyhow!(e))?;
    println!("[block] ggml reference: {ref_us} µs");

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
        "[block] max |diff| = {:.3e} at idx {} (got {:.6e}, want {:.6e})",
        max_abs, max_idx, h_got[max_idx], h_ref[max_idx]
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "block FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("[block] PASSED (tol {:.3e})", args.tol);
    Ok(())
}
