//! Qwen3.5 FFN block — SwiGLU MLP layer composition.
//!
//! Every Qwen3.5 decoder layer (both FullAttention and GDN variants)
//! applies this SwiGLU feed-forward block AFTER its attention /
//! recurrence sub-block. The block composes the kernel primitives in
//! [`crate::kernels`] into:
//!
//! ```text
//!   residual ← hidden
//!   hidden   ← rmsnorm(hidden)             (f32 inside, bf16 out)
//!   gate     ← matmul(hidden, w_gate)      [n_tokens, intermediate_dim]
//!   up       ← matmul(hidden, w_up)        [n_tokens, intermediate_dim]
//!   inter    ← silu(gate) * up             [n_tokens, intermediate_dim]
//!   ffn_out  ← matmul(inter, w_down)       [n_tokens, hidden_dim]
//!   hidden   ← ffn_out + residual
//! ```
//!
//! # Scope of the first port
//!
//! Agent O's deliverable is the layer-composition mechanics — forward
//! threads the primitives end-to-end, validates shapes, and the smoke
//! test confirms non-exploding outputs under small random weights.
//!
//! Agent C's refactor replaces the three bf16 projection fields with
//! [`PackedWeight`] carriers — so Q4_K_M / Q5_K / Q6_K / Q8_0 / IQ4_XS
//! tensors loaded via `LoaderConfig::keep_packed = true` can now flow
//! into the FFN forward without a CPU dequant round-trip. The shape of
//! the forward is unchanged; the only difference is that each projection
//! now routes through [`PackedWeight::matmul_f32`] which stages an f32
//! input tensor, dispatches to the right mmvq / bf16 gemm kernel, and
//! returns an f32 output — bracketed by bf16↔f32 casts to keep the
//! SwiGLU activation and residual add on their bf16 paths.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use crate::kernels::{
    launch_cast_bf16_to_f32, launch_cast_f32_to_bf16, launch_residual_add_bf16,
    launch_rmsnorm_f32, launch_silu_mul_bf16,
};
use ctox_cuda_primitives::tensor::CudaTensor;

use crate::config::Qwen35Config;
use crate::layers::packed_weight::PackedWeight;

/// Weights + config for one Qwen3.5 SwiGLU FFN block.
///
/// The three projection weights are [`PackedWeight`] carriers so the
/// same struct backs dense-bf16 smoke tests and the quantized
/// production GGUF path uniformly — see [`PackedWeight::matmul_f32`]
/// for dispatch details.
pub struct Qwen35FFN {
    /// Pre-FFN RMSNorm weight. `[hidden_dim]` f32 — f32 because the
    /// sum-of-squares reduction inside rmsnorm needs the extra
    /// precision. In GGUF this is `post_attention_norm.weight` (it
    /// fires after the attention residual-add and before the FFN
    /// projection).
    pub pre_norm: CudaTensor<f32>,

    /// Gate projection weight. Logical `[hidden_dim, intermediate_dim]`
    /// with `K = hidden_dim` and `N = intermediate_dim`. Carrier dtype
    /// is whatever the GGUF shipped; dispatch happens inside
    /// [`PackedWeight::matmul_f32`]. Produces the pre-SiLU gate stream.
    pub w_gate: PackedWeight,

    /// Up projection weight. Logical `[hidden_dim, intermediate_dim]`.
    /// Produces the stream multiplied in elementwise after
    /// `silu(gate)`.
    pub w_up: PackedWeight,

    /// Down projection weight. Logical
    /// `[intermediate_dim, hidden_dim]`. Projects the SwiGLU-mixed
    /// activations back into the residual stream width.
    pub w_down: PackedWeight,

    /// Architectural constants — see [`Qwen35Config`]. The FFN uses
    /// `hidden_dim`, `intermediate_dim`, and `rms_eps` from this.
    pub config: Qwen35Config,

    /// Which decoder-layer index this is. Used only for error messages
    /// and future profiling annotations; the FFN math is layer-index-
    /// independent.
    pub layer_idx: usize,
}

impl Qwen35FFN {
    /// One forward pass over a batch of `n_tokens` tokens.
    ///
    /// Mutates `hidden` in place: `hidden ← hidden + ffn(rmsnorm(hidden))`.
    ///
    /// Requires:
    ///   * `hidden.shape() == [n_tokens, hidden_dim]`, bf16
    ///   * `n_tokens % 32 == 0` (matmul tile alignment — only the bf16
    ///     gemm path needs this; the per-row mmvq path tolerates any
    ///     n_tokens, but we keep the uniform check so callers see the
    ///     same alignment contract across layers).
    ///   * `hidden_dim` and `intermediate_dim` already 32-aligned
    ///     (true for production Qwen3.5-27B: 5120 and 13824).
    ///
    /// Does not synchronize the stream — callers sync at phase
    /// boundaries (end-of-layer or end-of-forward).
    pub fn forward(
        &self,
        device: &Arc<DeviceContext>,
        hidden: &mut CudaTensor<bf16>,
    ) -> Result<()> {
        // ------------------------------------------------------------
        // 0. Shape validation.
        // ------------------------------------------------------------
        let cfg = &self.config;
        let hidden_shape = hidden.shape();
        if hidden_shape.len() != 2 {
            return Err(anyhow!(
                "qwen35 ffn layer {}: hidden must be 2D [n_tokens, hidden_dim], got {:?}",
                self.layer_idx,
                hidden_shape
            ));
        }
        let n_tokens = hidden_shape[0];
        let hidden_dim = hidden_shape[1];
        if hidden_dim != cfg.hidden_dim {
            return Err(anyhow!(
                "qwen35 ffn layer {}: hidden_dim {} != config.hidden_dim {}",
                self.layer_idx,
                hidden_dim,
                cfg.hidden_dim
            ));
        }
        let inter = cfg.intermediate_dim;

        // Matmul tile alignment: kernel requires M, K, N all divisible
        // by 32. hidden_dim and intermediate_dim are picked to satisfy
        // this on production weights; n_tokens is the caller's
        // responsibility (pad to 32 at the prompt boundary).
        if !n_tokens.is_multiple_of(32) {
            return Err(anyhow!(
                "qwen35 ffn layer {}: matmul requires n_tokens divisible by 32 (got {})",
                self.layer_idx,
                n_tokens
            ));
        }
        if !hidden_dim.is_multiple_of(32) {
            return Err(anyhow!(
                "qwen35 ffn layer {}: matmul requires hidden_dim divisible by 32 (got {})",
                self.layer_idx,
                hidden_dim
            ));
        }
        if !inter.is_multiple_of(32) {
            return Err(anyhow!(
                "qwen35 ffn layer {}: matmul requires intermediate_dim divisible by 32 (got {})",
                self.layer_idx,
                inter
            ));
        }

        if self.pre_norm.shape() != [hidden_dim] {
            return Err(anyhow!(
                "qwen35 ffn layer {}: pre_norm.shape {:?} != [{}]",
                self.layer_idx,
                self.pre_norm.shape(),
                hidden_dim
            ));
        }
        if self.w_gate.dims() != (hidden_dim, inter) {
            return Err(anyhow!(
                "qwen35 ffn layer {}: w_gate dims {:?} != ({}, {})",
                self.layer_idx,
                self.w_gate.dims(),
                hidden_dim,
                inter
            ));
        }
        if self.w_up.dims() != (hidden_dim, inter) {
            return Err(anyhow!(
                "qwen35 ffn layer {}: w_up dims {:?} != ({}, {})",
                self.layer_idx,
                self.w_up.dims(),
                hidden_dim,
                inter
            ));
        }
        if self.w_down.dims() != (inter, hidden_dim) {
            return Err(anyhow!(
                "qwen35 ffn layer {}: w_down dims {:?} != ({}, {})",
                self.layer_idx,
                self.w_down.dims(),
                inter,
                hidden_dim
            ));
        }

        // ------------------------------------------------------------
        // 1. Save the residual. We add it back at the end.
        //
        //    Device-to-device memcpy via the default stream avoids a
        //    host round-trip. Mirrors `Qwen35FullAttention::forward`.
        // ------------------------------------------------------------
        let residual = {
            let mut r = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
            let stream = device.raw().default_stream();
            stream.memcpy_dtod(hidden.buf(), r.buf_mut()).map_err(|e| {
                anyhow!(
                    "qwen35 ffn layer {}: residual memcpy_dtod: {:?}",
                    self.layer_idx,
                    e
                )
            })?;
            r
        };

        // ------------------------------------------------------------
        // 2. Pre-norm in f32 (RMSNorm is numerically sensitive). We
        //    keep `norm_f32` as the projection input — the PackedWeight
        //    dispatch takes f32 in / f32 out and routes to whichever of
        //    the mmvq / bf16 gemm kernels matches the on-device weight
        //    carrier. Mirrors the full-attention path.
        // ------------------------------------------------------------
        let mut hidden_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        launch_cast_bf16_to_f32(device, hidden, &mut hidden_f32)?;

        let mut norm_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        launch_rmsnorm_f32(device, &hidden_f32, &self.pre_norm, &mut norm_f32, cfg.rms_eps)?;

        // ------------------------------------------------------------
        // 3. Parallel gate + up projections, both f32 in/out via the
        //    PackedWeight dispatch, then downcast to bf16 for the fused
        //    SwiGLU kernel.
        //
        //    Fused gate/up is tracked as a Phase-5 optimization — a
        //    wide projection + split would halve the matmul count.
        // ------------------------------------------------------------
        let mut gate_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, inter])?;
        self.w_gate.matmul_f32(device, &norm_f32, &mut gate_f32)?;
        let mut gate = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, inter])?;
        launch_cast_f32_to_bf16(device, &gate_f32, &mut gate)?;

        let mut up_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, inter])?;
        self.w_up.matmul_f32(device, &norm_f32, &mut up_f32)?;
        let mut up = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, inter])?;
        launch_cast_f32_to_bf16(device, &up_f32, &mut up)?;

        // ------------------------------------------------------------
        // 4. Fused SwiGLU activation: inter = silu(gate) * up.
        //    silu_mul_bf16 runs f32 math internally and stores bf16.
        // ------------------------------------------------------------
        let mut hidden_inter =
            CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, inter])?;
        launch_silu_mul_bf16(device, &gate, &up, &mut hidden_inter)?;

        // ------------------------------------------------------------
        // 5. Down projection: ffn_out = inter · w_down  [n, hidden].
        //    Cast inter up to f32 for the PackedWeight dispatch, then
        //    cast the projected result back to bf16 for the residual
        //    add. Same pattern the FA layer uses for its output
        //    projection.
        // ------------------------------------------------------------
        let mut inter_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, inter])?;
        launch_cast_bf16_to_f32(device, &hidden_inter, &mut inter_f32)?;
        let mut ffn_out_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        self.w_down.matmul_f32(device, &inter_f32, &mut ffn_out_f32)?;
        let mut ffn_out =
            CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        launch_cast_f32_to_bf16(device, &ffn_out_f32, &mut ffn_out)?;

        // ------------------------------------------------------------
        // 6. Residual add: hidden ← ffn_out + residual.
        //
        //    `launch_residual_add_bf16` requires distinct buffers for
        //    the two inputs and the output; route through a staging
        //    tensor and copy back, mirroring the full-attention path.
        // ------------------------------------------------------------
        let mut summed =
            CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        launch_residual_add_bf16(device, &ffn_out, &residual, &mut summed)?;
        let stream = device.raw().default_stream();
        stream
            .memcpy_dtod(summed.buf(), hidden.buf_mut())
            .map_err(|e| {
                anyhow!(
                    "qwen35 ffn layer {}: final copy back to hidden: {:?}",
                    self.layer_idx,
                    e
                )
            })?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Integration smoke test — A6000-only, run with --ignored.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random via LCG — host-independent so the
    /// test is reproducible across architectures.
    fn lcg_iter(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        // Map to roughly [-1, 1].
        ((*seed >> 16) as f32 / 32768.0) - 1.0
    }

    fn random_bf16(n: usize, seed: &mut u32, amplitude: f32) -> Vec<bf16> {
        (0..n)
            .map(|_| bf16::from_f32(lcg_iter(seed) * amplitude))
            .collect()
    }

    /// `qwen35_ffn_smoke` — ignored, A6000-only.
    ///
    /// Builds a `Qwen35FFN` with synthetic random bf16 weights (wrapped
    /// in `PackedWeight::Bf16`) scaled to keep SiLU output bounded, runs
    /// one forward on a random `[32, 5120]` bf16 activation, and asserts:
    ///   * output shape preserved
    ///   * no NaN / Inf
    ///   * max absolute value stays below 10.0 (residual-stream values
    ///     shouldn't blow up under small random weights)
    ///
    /// The Bf16 variant of `PackedWeight` exercises the bf16 cuBLAS gemm
    /// path with an f32 accumulator — same numerical contract as the
    /// pre-refactor `launch_matmul_bf16_bf16` direct call, so the
    /// finiteness bounds still hold.
    ///
    /// Exact-vs-reference comparison is Phase 6.
    ///
    /// Run:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture qwen35_ffn_smoke
    #[test]
    #[ignore]
    fn qwen35_ffn_smoke() {
        // Config matches the production 27B shapes: hidden=5120,
        // intermediate=13824. Both 32-aligned so the matmul tiles.
        let cfg = Qwen35Config::QWEN35_27B;
        let hidden_dim = cfg.hidden_dim;
        let inter = cfg.intermediate_dim;
        let n_tokens = 32usize; // 32-aligned for the matmul kernel

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let mut seed: u32 = 0xC0FFEE;

        // Weights. Xavier-scale amplitude: picked so the composed
        // SwiGLU output stays within ~1 std at the production 27B
        // shapes (fan-in 5120 for gate/up, 13824 for down). The brief
        // suggested [-0.05, 0.05] but at these fan-ins that puts the
        // pre-residual output around ~50 (empirically measured ≈49.5),
        // exceeding the max_abs<10 sanity bound — so we scale ~5×
        // tighter. The test is structural ("forward composes, no
        // NaN/Inf, doesn't blow up"); exact numerics vs the reference
        // is Phase 6.
        let amp = 0.01f32;
        let w_gate_host = random_bf16(hidden_dim * inter, &mut seed, amp);
        let w_up_host = random_bf16(hidden_dim * inter, &mut seed, amp);
        let w_down_host = random_bf16(inter * hidden_dim, &mut seed, amp);

        // Pre-norm weights: RMSNorm ≈ identity when weights are near 1.
        let pre_norm_host: Vec<f32> = (0..hidden_dim)
            .map(|_| 1.0 + 0.1 * lcg_iter(&mut seed))
            .collect();

        let pre_norm = CudaTensor::<f32>::from_host(
            dev.clone(),
            vec![hidden_dim],
            &pre_norm_host,
        )
        .expect("upload pre_norm");
        let w_gate_t = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![hidden_dim, inter],
            &w_gate_host,
        )
        .expect("upload w_gate");
        let w_gate = PackedWeight::Bf16 {
            t: w_gate_t,
            k: hidden_dim,
            n: inter,
        };
        let w_up_t = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![hidden_dim, inter],
            &w_up_host,
        )
        .expect("upload w_up");
        let w_up = PackedWeight::Bf16 {
            t: w_up_t,
            k: hidden_dim,
            n: inter,
        };
        let w_down_t = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![inter, hidden_dim],
            &w_down_host,
        )
        .expect("upload w_down");
        let w_down = PackedWeight::Bf16 {
            t: w_down_t,
            k: inter,
            n: hidden_dim,
        };

        let ffn = Qwen35FFN {
            pre_norm,
            w_gate,
            w_up,
            w_down,
            config: cfg,
            layer_idx: 0,
        };

        // Hidden activation. Small amplitude so rmsnorm's sum-of-
        // squares stays in-range at hidden_dim=5120.
        let hidden_host = random_bf16(n_tokens * hidden_dim, &mut seed, 0.25);
        let mut hidden = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![n_tokens, hidden_dim],
            &hidden_host,
        )
        .expect("upload hidden");
        let expected_shape = [n_tokens, hidden_dim];

        ffn.forward(&dev, &mut hidden).expect("ffn forward");
        dev.synchronize().expect("synchronize");

        // ── Assertions.
        assert_eq!(
            hidden.shape(),
            expected_shape,
            "hidden shape preserved by ffn forward"
        );

        let out_host = hidden.to_host().expect("download hidden");
        let mut n_nan = 0usize;
        let mut n_inf = 0usize;
        let mut max_abs = 0.0f32;
        for v in &out_host {
            let f = v.to_f32();
            if f.is_nan() {
                n_nan += 1;
            } else if f.is_infinite() {
                n_inf += 1;
            } else if f.abs() > max_abs {
                max_abs = f.abs();
            }
        }
        eprintln!(
            "qwen35_ffn_smoke: shape={:?} n_nan={} n_inf={} max_abs={:.4e}",
            hidden.shape(),
            n_nan,
            n_inf,
            max_abs,
        );
        assert_eq!(n_nan, 0, "ffn output contains {} NaN", n_nan);
        assert_eq!(n_inf, 0, "ffn output contains {} Inf", n_inf);
        assert!(
            max_abs < 10.0,
            "ffn output max_abs={} exceeds 10.0 sanity bound",
            max_abs
        );
    }
}
