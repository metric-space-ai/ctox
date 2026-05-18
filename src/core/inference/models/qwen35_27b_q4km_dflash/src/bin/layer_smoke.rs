//! Full Qwen3.5-shaped SwiGLU FFN block through the hybrid executor.
//!
//! Replicates the FFN half of a transformer layer:
//!
//!   x_normed = rms_norm(x, eps)              (Rust-native)
//!   gate     = mul_mat(x_normed, W_gate)     (ggml-fallback)
//!   up       = mul_mat(x_normed, W_up)       (ggml-fallback)
//!   act      = silu(gate)                    (Rust-native)
//!   gated    = act * up                      (Rust-native)
//!   down     = mul_mat(gated, W_down)        (ggml-fallback)
//!   out      = x + down                      (Rust-native)
//!
//! 8 ops total: 4 Rust-native + 4 ggml-fallback, chained through
//! the same `GgmlBackendCtx`. Compares against pure ggml's
//! ggml_backend_graph_compute of the same 8-op graph.
//!
//! Shapes default to a small Qwen3.5-ish layer (hidden=512,
//! ffn=1408, seq=8) so the test is fast but exercises realistic
//! tensor shapes. CLI args let you scale up to the real 27B-sized
//! layer (hidden=5120, ffn=17408) for a perf sanity check.

use anyhow::{anyhow, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::cuda_port::driver::{cuInit, cuStreamSynchronize, ensure_current_context, CUstream};
use dflash::cuda_port::fallback::GgmlBackendCtx;
use dflash::cuda_port::graph::{add, mul, rms_norm, silu, ExecCtx};
use dflash::cuda_port::module::porter;
use dflash::ffi as sys;

#[derive(Parser, Debug)]
#[command(name = "qwen35-27b-q4km-dflash-layer-smoke")]
struct Args {
    /// Hidden dim (Qwen3.5-27B = 5120). 512 default for fast smoke.
    #[arg(long, default_value_t = 512)]
    hidden: i64,
    /// FFN intermediate dim (Qwen3.5-27B = 17408). 1408 default.
    #[arg(long, default_value_t = 1408)]
    ffn: i64,
    /// Sequence length (tokens).
    #[arg(long, default_value_t = 8)]
    seq: i64,
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    /// Drift tolerance. We observe ~1e-3 (sub-percent) drift vs the
    /// single-graph ggml reference when running the 3 mul_mats as
    /// separate `compute()` calls: ggml's single-graph path plans
    /// intermediates differently, and cuBLAS/ggml-cuda may pick
    /// different tile sizes depending on call-site alone tensor
    /// allocation patterns. 1e-2 is safe headroom; bit-exact would
    /// require the whole layer to go through a single compute().
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

    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!("ggml_backend_cuda_init failed"));
    }
    unsafe { cuInit(0) };
    ensure_current_context(args.cuda_device).map_err(|e| anyhow!("ctx: {e}"))?;
    let kernels = porter().map_err(|e| anyhow!("porter(): {e}"))?;
    println!(
        "[layer] kernels resolved; shapes: hidden={} ffn={} seq={}",
        args.hidden, args.ffn, args.seq
    );

    let h = args.hidden;
    let f = args.ffn;
    let s = args.seq;

    // ggml mul_mat(A [h, f], B [h, s]) → C [f, s]
    // so W_gate / W_up have shape [h, f], W_down has shape [f, h].

    let n_x = (h * s) as usize;
    let n_gu = (h * f) as usize; // each of W_gate, W_up
    let n_dn = (f * h) as usize;

    let mut h_x = vec![0.0_f32; n_x];
    let mut h_wgate = vec![0.0_f32; n_gu];
    let mut h_wup = vec![0.0_f32; n_gu];
    let mut h_wdown = vec![0.0_f32; n_dn];
    det_fill(&mut h_x, 0.0);
    det_fill(&mut h_wgate, 1.0);
    det_fill(&mut h_wup, 2.0);
    det_fill(&mut h_wdown, 3.0);

    // ══════════════════════════════════════════════════════════
    // Hybrid path
    // ══════════════════════════════════════════════════════════
    let mut gctx = GgmlBackendCtx::new(backend).map_err(|e| anyhow!(e))?;
    let t_x = gctx.new_tensor_f32([h, s, 1, 1], "x");
    let t_xn = gctx.new_tensor_f32([h, s, 1, 1], "x_normed");
    let t_wg = gctx.new_tensor_f32([h, f, 1, 1], "W_gate");
    let t_wu = gctx.new_tensor_f32([h, f, 1, 1], "W_up");
    let t_wd = gctx.new_tensor_f32([f, h, 1, 1], "W_down");
    let t_act = gctx.new_tensor_f32([f, s, 1, 1], "silu_gate");
    let t_gated = gctx.new_tensor_f32([f, s, 1, 1], "gated");
    let t_out = gctx.new_tensor_f32([h, s, 1, 1], "out");
    gctx.realize().map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_x, &h_x).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wg, &h_wgate).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wu, &h_wup).map_err(|e| anyhow!(e))?;
    gctx.upload_f32(t_wd, &h_wdown).map_err(|e| anyhow!(e))?;

    let stream = CUstream(std::ptr::null_mut());
    let exec = ExecCtx::new(kernels, stream);

    let t_start = std::time::Instant::now();

    // 1. rms_norm (Rust)
    let rt_x = gctx.as_rust_tensor(t_x);
    let rt_xn = gctx.as_rust_tensor(t_xn);
    rms_norm(&exec, &rt_x, &rt_xn, 1e-6).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    // 2. gate = mul_mat(x_normed, W_gate)  (ggml)
    let t_gate = gctx
        .compute(|gc| unsafe { sys::ggml_mul_mat(gc, t_wg, t_xn) })
        .map_err(|e| anyhow!(e))?;

    // 3. up = mul_mat(x_normed, W_up)  (ggml)
    let t_up = gctx
        .compute(|gc| unsafe { sys::ggml_mul_mat(gc, t_wu, t_xn) })
        .map_err(|e| anyhow!(e))?;

    // 4. act = silu(gate) (Rust)
    let rt_gate = gctx.as_rust_tensor(t_gate);
    let rt_act = gctx.as_rust_tensor(t_act);
    silu(&exec, &rt_gate, &rt_act).map_err(|e| anyhow!(e))?;

    // 5. gated = act * up  (Rust)
    let rt_up = gctx.as_rust_tensor(t_up);
    let rt_gated = gctx.as_rust_tensor(t_gated);
    mul(&exec, &rt_act, &rt_up, &rt_gated).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    // 6. down = mul_mat(gated, W_down)  (ggml)
    let t_down = gctx
        .compute(|gc| unsafe { sys::ggml_mul_mat(gc, t_wd, t_gated) })
        .map_err(|e| anyhow!(e))?;

    // 7. out = x + down  (Rust)
    let rt_down = gctx.as_rust_tensor(t_down);
    let rt_out = gctx.as_rust_tensor(t_out);
    add(&exec, &rt_x, &rt_down, &rt_out).map_err(|e| anyhow!(e))?;
    unsafe { cuStreamSynchronize(stream) };

    let hybrid_us = t_start.elapsed().as_micros();
    println!(
        "[layer] hybrid path: 8-op chain launched + synced in {} µs",
        hybrid_us
    );

    let h_got = gctx.download_f32(t_out, n_x).map_err(|e| anyhow!(e))?;

    // ══════════════════════════════════════════════════════════
    // Reference — pure ggml graph compute
    // ══════════════════════════════════════════════════════════
    let mut gref = GgmlBackendCtx::new(backend).map_err(|e| anyhow!(e))?;
    let r_x = gref.new_tensor_f32([h, s, 1, 1], "r_x");
    let r_wg = gref.new_tensor_f32([h, f, 1, 1], "r_W_gate");
    let r_wu = gref.new_tensor_f32([h, f, 1, 1], "r_W_up");
    let r_wd = gref.new_tensor_f32([f, h, 1, 1], "r_W_down");
    gref.realize().map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_x, &h_x).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wg, &h_wgate).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wu, &h_wup).map_err(|e| anyhow!(e))?;
    gref.upload_f32(r_wd, &h_wdown).map_err(|e| anyhow!(e))?;

    let t_start = std::time::Instant::now();
    let r_out = gref
        .compute(|gc| unsafe {
            let norm = sys::ggml_rms_norm(gc, r_x, 1e-6);
            let gate = sys::ggml_mul_mat(gc, r_wg, norm);
            let up = sys::ggml_mul_mat(gc, r_wu, norm);
            let act = sys::ggml_silu(gc, gate);
            let gated = sys::ggml_mul(gc, act, up);
            let down = sys::ggml_mul_mat(gc, r_wd, gated);
            sys::ggml_add(gc, r_x, down)
        })
        .map_err(|e| anyhow!(e))?;
    let ref_us = t_start.elapsed().as_micros();
    let h_ref = gref.download_f32(r_out, n_x).map_err(|e| anyhow!(e))?;
    println!("[layer] ggml reference: full-graph compute in {} µs", ref_us);

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
        "[layer] hybrid-vs-ggml: max |diff| = {:.3e} at idx {} (got {:.6e}, want {:.6e})",
        max_abs, max_idx, h_got[max_idx], h_ref[max_idx]
    );
    if max_abs > args.tol {
        return Err(anyhow!(
            "layer FAILED tol {:.3e} (got {:.3e})",
            args.tol,
            max_abs
        ));
    }
    println!("[layer] PASSED (tol {:.3e})", args.tol);
    Ok(())
}
