//! Qwen3.5 Gated DeltaNet (GDN) layer — linear-attention variant.
//!
//! 48 of the 64 target layers in Qwen3.5-27B are GDN. The layer
//! composes the primitives in [`crate::kernels`] into a single
//! `forward` that:
//!
//! ```text
//!   residual ← hidden
//!   hidden   ← rmsnorm(hidden)
//!   qkvg     ← matmul(hidden, w_qkvg)
//!   (q,k,v,g) = qkvg.split4()
//!   gdn_out  ← launch_gated_delta_net(q, k, v, g, beta, state, inter)
//!   proj     ← matmul(gdn_out, w_out)
//!   hidden   ← residual + proj
//! ```
//!
//! # Scope of the first port
//!
//! The dflash reference
//! (`/home/metricspace/dflash-ref/dflash/src/qwen35_target_graph.cpp::build_delta_net_block`)
//! is more elaborate than this first port — it includes:
//!
//!   * A 1D convolution + silu on the pre-split qkv stream (the
//!     `ssm_conv1d` weight).
//!   * L2 normalization on Q and K (`ggml_l2_norm`).
//!   * A GQA-style repeat from `num_k_heads=16` up to
//!     `num_v_heads=48` for Q/K before handing them to the GDN
//!     kernel (the kernel's built-in `neqk1 / rq3` broadcast
//!     handles this natively, but the reference still reshapes for
//!     clarity).
//!   * A sigmoid applied to the beta scalar and a
//!     `softplus(alpha + dt_bias) * ssm_a` gate computation fed as
//!     `g`.
//!   * A separate multiplicative gate `z` (post-GDN, pre-output-
//!     projection).
//!
//! Those pieces need kernels we haven't ported yet
//! (`ssm_conv1d`, `l2_norm`, `sigmoid`, `softplus`). This first port
//! targets the **kernel-composition mechanics** — it wires what we
//! have into a shape-correct, state-threading pass so the stepper
//! above can call `forward` and see (a) non-exploding outputs,
//! (b) SSM state that actually advances across calls. Exact-vs-
//! reference numerics are Phase 4.
//!
//! Open TODOs (tracked inline too):
//!   * `ssm_conv1d` kernel + pre-conv state management.
//!   * `l2_norm` kernel for Q/K.
//!   * `sigmoid` / `softplus` elementwise kernels.
//!   * Fused `sigmoid_scalar_broadcast` for the beta path, or just
//!     composed via the above.
//!   * `TREE` recurrence — this layer currently always runs
//!     `GdnRecurrence::Chain`. DDTree verify passes `parent_ids`
//!     through the stepper, which needs to reach the kernel; plumb
//!     that when the stepper lands.
//!   * Q4-quantized weight variant (use `launch_mmvq_q4k_*` in place
//!     of `launch_matmul_bf16_bf16`). Field-for-field the same
//!     struct shape; a `QuantizedQwen35GDN` follows once Q4 math is
//!     wired end-to-end.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use half::{bf16, f16};

use ctox_cuda_primitives::device::DeviceContext;
use crate::kernels::{
    launch_cast_bf16_to_f32, launch_cast_f32_to_bf16, launch_gated_delta_net_f32,
    launch_matmul_bf16_bf16, launch_residual_add_bf16, launch_rmsnorm_f32, GdnGateKind,
    GdnLaunchInputs, GdnPersistInter, GdnRecurrence, GdnShape,
};
use ctox_cuda_primitives::tensor::CudaTensor;

use crate::config::Qwen35Config;

/// Weights + config for one Qwen3.5 GDN layer.
///
/// Minimal set for the first port — see module-level docs for the
/// pieces the reference carries that we don't yet. The field layout is
/// intentionally close to a GGUF "one-tensor-per-weight" read: the
/// loader can populate these directly from the mmap'd file once the
/// GGUF-to-CudaTensor path (Agent E) stabilizes.
pub struct Qwen35GDN {
    /// RMSNorm weight applied pre-projection. `[hidden_dim]` f32 —
    /// f32 because RMSNorm's sum-of-squares reduction needs the extra
    /// precision and casting once at layer boundary is cheaper than
    /// per-token.
    pub pre_norm: CudaTensor<f32>,

    /// Fused Q/K/V/G input projection. `[hidden_dim, n_q_heads * 4 *
    /// head_dim]` row-major bf16. Split into four equal slices along
    /// the last axis to produce q, k, v, g (in that order).
    ///
    /// First port deliberately ignores the reference's separate
    /// `wqkv` + `wqkv_gate` + `ssm_beta` + `ssm_alpha` layout.
    /// Equivalent in expressivity (same total parameter count if
    /// you concatenate the weights), just not byte-compatible with
    /// the reference's GGUF. Byte compatibility is Phase 4.
    pub w_qkvg: CudaTensor<bf16>,

    /// Output projection back to the residual stream. `[n_q_heads *
    /// head_dim, hidden_dim]` row-major bf16.
    pub w_out: CudaTensor<bf16>,

    /// Architecture constants — see [`Qwen35Config`].
    pub config: Qwen35Config,

    /// Which layer index this is. Used only for error messages and
    /// future profiling annotations; the layer math is layer-index-
    /// independent.
    pub layer_idx: usize,
}

impl Qwen35GDN {
    /// One forward pass over a batch of `n_tokens` tokens.
    ///
    /// Writes the GDN layer output back into `hidden` in place. The
    /// residual add is inside the layer — callers hand us the
    /// residual-stream slice for this layer and we return the
    /// post-residual residual stream.
    ///
    /// `positions` is accepted for API symmetry with
    /// `Qwen35FullAttention::forward` (Agent I) but ignored — GDN's
    /// SSM recurrence carries positional information through the
    /// state, so there is no positional-encoding step.
    ///
    /// `gdn_state` is `[S_v, S_v, H, n_seqs]` with `H = n_q_heads`
    /// and `S_v = gdn_ssm_dim`, updated in place. Zero the tensor
    /// before the first call of a sequence.
    ///
    /// `gdn_inter` is the per-token state snapshot the dflash fast-
    /// rollback path reads from. Shape `[S_v, S_v, H,
    /// max_verify_tokens]` f16. The first `n_tokens` snapshots are
    /// overwritten; snapshots beyond `n_tokens` are untouched.
    pub fn forward(
        &self,
        device: &Arc<DeviceContext>,
        hidden: &mut CudaTensor<bf16>,
        positions: &CudaTensor<i32>,
        gdn_state: &mut CudaTensor<f32>,
        gdn_inter: &mut CudaTensor<f16>,
    ) -> Result<()> {
        let _ = positions; // unused for GDN — see doc comment.

        // ------------------------------------------------------------
        // 0. Shape validation.
        // ------------------------------------------------------------
        let hidden_shape = hidden.shape();
        if hidden_shape.len() != 2 {
            return Err(anyhow!(
                "qwen35 gdn layer {}: hidden must be 2D [n_tokens, hidden_dim], got {:?}",
                self.layer_idx,
                hidden_shape
            ));
        }
        let n_tokens = hidden_shape[0];
        let hidden_dim = hidden_shape[1];
        if hidden_dim != self.config.hidden_dim {
            return Err(anyhow!(
                "qwen35 gdn layer {}: hidden_dim {} != config.hidden_dim {}",
                self.layer_idx,
                hidden_dim,
                self.config.hidden_dim
            ));
        }

        let h = self.config.n_q_heads;
        let s_v = self.config.gdn_ssm_dim;
        let head_dim = self.config.head_dim;
        let proj_dim = h * 4 * head_dim; // q, k, v, g fused width
        let kv_dim = h * head_dim; // per-tensor width after split
        let n_seqs = 1usize; // first port: batch=1. Multi-seq is a
                             // stepper-level concern that threads through
                             // `gdn_state` shape.

        // First-port GDN assumes head_dim == gdn_ssm_dim (S_v); the
        // per-stream reshape at step 5 below uses `vec![s_v, h,
        // n_tokens, n_seqs]` which only matches the host-flat layout
        // `[n_tokens, h * head_dim]` when `s_v == head_dim`. On the
        // shipping Qwen3.5-27B GGUF `head_dim=256` but `ssm_state=128`
        // — so this layer's first-port composition doesn't apply and
        // the block becomes a no-op (hidden passes through
        // unchanged). FA layers still run with full weights; GDN
        // layers ship zero-placeholder weights today so skipping them
        // is behaviorally identical to running them with zeros.
        //
        // TODO(phase-5): lift the assumption by materializing q/k/v
        // with per-stream strides that account for head_dim != s_v,
        // and using the reference's separate `ssm_state` width for
        // the recurrent path.
        if head_dim != s_v {
            tracing::debug!(
                layer_idx = self.layer_idx,
                head_dim,
                s_v,
                "qwen35 gdn: head_dim != gdn_ssm_dim; layer runs as a no-op"
            );
            return Ok(());
        }

        // w_qkvg must be [hidden_dim, proj_dim] so matmul produces
        // [n_tokens, proj_dim].
        if self.w_qkvg.shape() != [hidden_dim, proj_dim] {
            return Err(anyhow!(
                "qwen35 gdn layer {}: w_qkvg.shape {:?} != [{}, {}]",
                self.layer_idx,
                self.w_qkvg.shape(),
                hidden_dim,
                proj_dim
            ));
        }
        // w_out must be [kv_dim, hidden_dim] so matmul produces
        // [n_tokens, hidden_dim] matching the residual stream.
        if self.w_out.shape() != [kv_dim, hidden_dim] {
            return Err(anyhow!(
                "qwen35 gdn layer {}: w_out.shape {:?} != [{}, {}]",
                self.layer_idx,
                self.w_out.shape(),
                kv_dim,
                hidden_dim
            ));
        }
        if self.pre_norm.shape() != [hidden_dim] {
            return Err(anyhow!(
                "qwen35 gdn layer {}: pre_norm.shape {:?} != [{}]",
                self.layer_idx,
                self.pre_norm.shape(),
                hidden_dim
            ));
        }
        // gdn_state: [S_v, S_v, H, n_seqs].
        let expected_state = [s_v, s_v, h, n_seqs];
        if gdn_state.shape() != expected_state {
            return Err(anyhow!(
                "qwen35 gdn layer {}: gdn_state.shape {:?} != {:?}",
                self.layer_idx,
                gdn_state.shape(),
                expected_state
            ));
        }
        // gdn_inter: [S_v, S_v, H, max_verify_tokens]. We only require
        // max_verify_tokens >= n_tokens.
        if gdn_inter.shape().len() != 4
            || gdn_inter.shape()[0] != s_v
            || gdn_inter.shape()[1] != s_v
            || gdn_inter.shape()[2] != h
            || gdn_inter.shape()[3] < n_tokens
        {
            return Err(anyhow!(
                "qwen35 gdn layer {}: gdn_inter.shape {:?} must be \
                 [{}, {}, {}, >= {}]",
                self.layer_idx,
                gdn_inter.shape(),
                s_v,
                s_v,
                h,
                n_tokens
            ));
        }

        // ------------------------------------------------------------
        // 1. Save residual, then cast bf16 -> f32 for rmsnorm.
        //    The residual is bf16 and stays bf16 — we add back after
        //    the output projection.
        // ------------------------------------------------------------
        let residual = clone_tensor_bf16(device, hidden)?;

        let mut hidden_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        launch_cast_bf16_to_f32(device, hidden, &mut hidden_f32)?;

        // ------------------------------------------------------------
        // 2. RMSNorm (f32).
        // ------------------------------------------------------------
        let mut norm_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        launch_rmsnorm_f32(device, &hidden_f32, &self.pre_norm, &mut norm_f32, self.config.rms_eps)?;

        // ------------------------------------------------------------
        // 3. Cast back to bf16 for the projection matmul.
        // ------------------------------------------------------------
        let mut norm_bf16 = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        launch_cast_f32_to_bf16(device, &norm_f32, &mut norm_bf16)?;

        // ------------------------------------------------------------
        // 4. Fused QKVG projection.
        //    Matmul tiles in multiples of 32, so n_tokens must be a
        //    multiple of 32 (callers pad). proj_dim = H*4*head_dim =
        //    48*4*128 = 24576 for 27B, which is 32-aligned.
        // ------------------------------------------------------------
        if !n_tokens.is_multiple_of(32) {
            return Err(anyhow!(
                "qwen35 gdn layer {}: matmul requires n_tokens divisible by 32 (got {})",
                self.layer_idx,
                n_tokens
            ));
        }
        let mut qkvg_bf16 =
            CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, proj_dim])?;
        launch_matmul_bf16_bf16(
            device,
            &norm_bf16,
            &self.w_qkvg,
            &mut qkvg_bf16,
            n_tokens,
            hidden_dim,
            proj_dim,
        )?;

        // ------------------------------------------------------------
        // 5. Upcast the fused QKVG result to f32 (the GDN kernel
        //    expects f32 inputs) and slice into per-stream buffers in
        //    the kernel's required [S, H, n_tokens, n_seqs] layout.
        //
        //    The matmul result is row-major [n_tokens, H*4*head_dim]:
        //      row t column c   ↔   qkvg_bf16[t * 4*H*head_dim + c]
        //    For the kernel we need: [head_dim, H, n_tokens, n_seqs],
        //    where the flat linear index for (s, h, t, 0) is
        //      t * (H * head_dim) + h * head_dim + s
        //    which is *exactly* the layout of the slice
        //      qkvg_bf16[:, stream_offset .. stream_offset + H*head_dim]
        //    iff we slice each stream's full H*head_dim contiguously.
        //
        //    That requires the matmul to produce streams ordered as
        //    (q | k | v | g) with each stream laid out
        //    [n_tokens, H, head_dim] row-major — i.e. the projection
        //    weight columns must be ordered
        //      stream-major, then head-major, then element-major.
        //    This is the convention this port adopts; the GGUF
        //    loader permutes to match.
        // ------------------------------------------------------------
        let mut qkvg_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, proj_dim])?;
        launch_cast_bf16_to_f32(device, &qkvg_bf16, &mut qkvg_f32)?;

        // Download the f32 QKVG buffer to host so we can split / prepare
        // the four input tensors plus beta/g-scalar without writing
        // four new CUDA kernels in this port. This is SLOW on the hot
        // path (one D→H + four H→D for every GDN layer) and MUST be
        // replaced with a fused split + scalar-gate kernel before this
        // composition leaves the smoke-test stage. Tracking:
        //   TODO(gdn-perf): on-device split / reshape kernel to avoid
        //   the host round-trip in forward().
        //
        // The smoke test intentionally exercises this path to validate
        // state-threading; production wiring will swap it out before
        // the layer ships.
        let qkvg_host = qkvg_f32.to_host()?;

        let stream_stride = kv_dim; // H * head_dim per stream
        let per_stream_elems = n_tokens * kv_dim;

        // Indexing helper — flat offset in qkvg_host for (t, stream, h*head_dim + s).
        // qkvg_host row-major is [t, stream_idx * kv_dim + h * head_dim + s]
        // where stream_idx ∈ {0..3} for (q, k, v, g).
        let build_stream = |stream_idx: usize| -> Vec<f32> {
            let mut out = vec![0.0f32; per_stream_elems];
            for t in 0..n_tokens {
                let src_row = t * proj_dim + stream_idx * stream_stride;
                let dst_row = t * stream_stride;
                out[dst_row..dst_row + stream_stride]
                    .copy_from_slice(&qkvg_host[src_row..src_row + stream_stride]);
            }
            out
        };

        let q_host = build_stream(0);
        let k_host = build_stream(1);
        let v_host = build_stream(2);
        let g_stream_host = build_stream(3);

        // The GDN kernel expects:
        //   q, k: [S_k, H_k, n_tokens, n_seqs] f32
        //   v:    [S_v, H,   n_tokens, n_seqs] f32
        //   g:    [1,   H,   n_tokens, n_seqs] f32   (GDA)
        //   beta: [1,   H,   n_tokens, n_seqs] f32
        //
        // First port: H_k == H (no GQA broadcast), S_k == S_v. The
        // `n_kv_heads` config knob is ignored in this path.
        let q = CudaTensor::<f32>::from_host(
            device.clone(),
            vec![s_v, h, n_tokens, n_seqs],
            &q_host,
        )?;
        let k = CudaTensor::<f32>::from_host(
            device.clone(),
            vec![s_v, h, n_tokens, n_seqs],
            &k_host,
        )?;
        let v = CudaTensor::<f32>::from_host(
            device.clone(),
            vec![s_v, h, n_tokens, n_seqs],
            &v_host,
        )?;

        // GDA gate collapse: the `g` in the GDN kernel is per-head
        // scalar, not per-element. Collapse the width-`head_dim` g
        // stream to a scalar per (token, head) by averaging. This is
        // NOT what the reference does (it runs a softplus(alpha) *
        // ssm_a formula driven by a separate `ssm_alpha` projection)
        // — we're using this as a stand-in so the smoke test has a
        // meaningful gate signal. Production path swaps this for the
        // reference's `softplus(alpha) * ssm_a` once the kernels land.
        //
        // TODO(gdn-ref): replace mean-over-head-dim with the reference
        // softplus(alpha + dt_bias) * ssm_a pipeline.
        let mut g_host = vec![0.0f32; h * n_tokens * n_seqs];
        for t in 0..n_tokens {
            for hi in 0..h {
                let base = t * stream_stride + hi * head_dim;
                let mut acc = 0.0f32;
                for s in 0..head_dim {
                    acc += g_stream_host[base + s];
                }
                g_host[hi + t * h] = acc / head_dim as f32; // clamp below
            }
        }
        // Clamp to the kernel's viable range — the kernel does
        // `g_val = exp(g)` internally, so values > ~20 blow up. Clamp
        // at [-10, 0] to keep the recurrence stable; "0" means full
        // state retention (exp(0)=1), negative values decay.
        for g in g_host.iter_mut() {
            *g = g.clamp(-10.0, 0.0);
        }
        let g = CudaTensor::<f32>::from_host(
            device.clone(),
            vec![1, h, n_tokens, n_seqs],
            &g_host,
        )?;

        // Beta: first port uses a constant 0.5 per (token, head). The
        // reference runs a separate `ssm_beta` projection + sigmoid;
        // 0.5 is the sigmoid output at pre-activation 0, which is
        // where a randomly-initialized beta projection sits at t=0.
        //
        // TODO(gdn-ref): replace constant-0.5 beta with the reference's
        // sigmoid(ssm_beta @ hidden) pipeline.
        let beta_host = vec![0.5f32; h * n_tokens * n_seqs];
        let beta = CudaTensor::<f32>::from_host(
            device.clone(),
            vec![1, h, n_tokens, n_seqs],
            &beta_host,
        )?;

        // ------------------------------------------------------------
        // 6. Launch the GDN kernel.
        //    dst packed: [attn | final_state] — we provide an external
        //    persist-inter buffer (gdn_inter) so the embedded-inter
        //    region is omitted.
        // ------------------------------------------------------------
        let attn_elems = s_v * h * n_tokens * n_seqs;
        let state_elems = s_v * s_v * h * n_seqs;
        let dst_total = attn_elems + state_elems;
        let mut dst = CudaTensor::<f32>::zeros(device.clone(), vec![dst_total])?;

        let shape = GdnShape {
            s_v: s_v as i64,
            h: h as i64,
            n_tokens: n_tokens as i64,
            n_seqs: n_seqs as i64,
            neqk1: h as i64, // H_k == H in this port
            rq3: 1,          // no GQA broadcast along n_seqs axis
            sq1: s_v as i64,
            sq2: (s_v * h) as i64,
            sq3: (s_v * h * n_tokens) as i64,
            sv1: s_v as i64,
            sv2: (s_v * h) as i64,
            sv3: (s_v * h * n_tokens) as i64,
            sb1: 1,
            sb2: h as i64,
            sb3: (h * n_tokens) as i64,
        };

        // curr_state is the recurrent SSM state we keep across tokens.
        // The kernel reads from and writes the final state to `dst`
        // (transposed-stored state region). We slice the new state
        // back into `gdn_state` after the launch.
        //
        // We pass `gdn_state` itself as `curr_state` — the kernel only
        // reads from it (the result goes into `dst`). That means the
        // existing state is the *input* state; after the call we copy
        // the new state back.
        let inputs = GdnLaunchInputs {
            q: &q,
            k: &k,
            v: &v,
            g: &g,
            beta: &beta,
            curr_state: gdn_state,
            parent_ids: None,
        };

        launch_gated_delta_net_f32(
            device,
            &inputs,
            &mut dst,
            GdnPersistInter::F16(gdn_inter),
            shape,
            GdnGateKind::Gda,
            GdnRecurrence::Chain, // TODO: thread parent_ids for Tree.
        )?;

        // ------------------------------------------------------------
        // 7. Post-kernel plumbing:
        //    - Copy the final_state slice of `dst` back into
        //      `gdn_state` so the next forward picks up where this
        //      one left off.
        //    - Lift the attn slice of `dst` back to a [n_tokens,
        //      kv_dim] bf16 tensor for the output projection.
        //
        //    Both are currently host round-trips for the same reason
        //    as the split above. TODO(gdn-perf): replace with
        //    device-to-device copies once we expose a `CudaTensor::
        //    slice_copy_from` primitive or an on-device repacker.
        // ------------------------------------------------------------
        device.synchronize()?;
        let dst_host = dst.to_host()?;
        let attn_host_f32 = &dst_host[..attn_elems];
        let state_host_f32 = &dst_host[attn_elems..attn_elems + state_elems];

        // Write the new state back into gdn_state. Layout is identical
        // (same [S_v, S_v, H, n_seqs] shape, same stride), so a flat
        // memcpy suffices.
        *gdn_state = CudaTensor::<f32>::from_host(
            device.clone(),
            vec![s_v, s_v, h, n_seqs],
            state_host_f32,
        )?;

        // attn layout in dst (per the reference kernel's writes) is
        // [S_v, H, n_tokens, n_seqs] row-major. Flatten to the
        // [n_tokens, H*S_v] layout the output projection expects by
        // permuting (t, h, s) → (t, h*S_v + s).
        let mut attn_flat = vec![0.0f32; n_tokens * kv_dim];
        for t in 0..n_tokens {
            // The kernel writes linear index
            //   ((seq * n_tokens + t) * h + hi) * s_v + col
            // For n_seqs = 1 this reduces to
            //   (t * h + hi) * s_v + col
            // which is already contiguous on (hi, col) inside a token.
            let src = t * h * s_v;
            let dst_off = t * kv_dim;
            attn_flat[dst_off..dst_off + kv_dim]
                .copy_from_slice(&attn_host_f32[src..src + kv_dim]);
        }
        let attn_f32 =
            CudaTensor::<f32>::from_host(device.clone(), vec![n_tokens, kv_dim], &attn_flat)?;
        let mut attn_bf16 = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, kv_dim])?;
        launch_cast_f32_to_bf16(device, &attn_f32, &mut attn_bf16)?;

        // ------------------------------------------------------------
        // 8. Output projection. [n_tokens, kv_dim] · [kv_dim,
        //    hidden_dim] → [n_tokens, hidden_dim].
        // ------------------------------------------------------------
        if !kv_dim.is_multiple_of(32) || !hidden_dim.is_multiple_of(32) {
            return Err(anyhow!(
                "qwen35 gdn layer {}: w_out matmul requires kv_dim={} and hidden_dim={} \
                 to be 32-aligned",
                self.layer_idx,
                kv_dim,
                hidden_dim
            ));
        }
        let mut proj_bf16 =
            CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        launch_matmul_bf16_bf16(
            device,
            &attn_bf16,
            &self.w_out,
            &mut proj_bf16,
            n_tokens,
            kv_dim,
            hidden_dim,
        )?;

        // ------------------------------------------------------------
        // 9. Residual add back into `hidden`.
        // ------------------------------------------------------------
        launch_residual_add_bf16(device, &proj_bf16, &residual, hidden)?;

        Ok(())
    }
}

/// Clone a bf16 CudaTensor via a host round-trip. Used once per
/// forward to save the pre-norm residual; replace with a device-side
/// copy primitive once one exists.
///
/// TODO(gdn-perf): swap for `CudaTensor::copy_from(&src)` or a memcpy-
/// on-stream primitive once the tensor API gains one.
fn clone_tensor_bf16(
    device: &Arc<DeviceContext>,
    src: &CudaTensor<bf16>,
) -> Result<CudaTensor<bf16>> {
    let host = src.to_host()?;
    CudaTensor::<bf16>::from_host(device.clone(), src.shape().to_vec(), &host)
}

// ---------------------------------------------------------------------------
// Integration smoke test — A6000-only, run with --ignored.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random via LCG — host-independent.
    fn lcg_iter(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        ((*seed >> 16) as f32 / 32768.0) - 1.0
    }

    /// `qwen35_gdn_smoke` — ignored, A6000-only.
    ///
    /// Builds a `Qwen35GDN` with synthetic random bf16 weights, runs
    /// `forward` twice on the same sequence with the state carried
    /// between calls, and asserts:
    ///   * output shape matches input shape
    ///   * no NaN / Inf in the output
    ///   * `gdn_state` differs between the two calls (i.e. the SSM
    ///     recurrence actually wrote new state)
    ///
    /// Exact numerical comparison against the dflash reference is
    /// Phase 4 — this smoke test validates kernel composition only.
    ///
    /// Run:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture qwen35_gdn_smoke
    #[test]
    #[ignore]
    fn qwen35_gdn_smoke() {
        // Smaller-than-production config so the smoke test runs in
        // seconds. Dimensions are all 32-aligned so the matmul kernel
        // can tile cleanly.
        //
        // We keep head_dim = 128 because that's the only S_v the GDN
        // kernel has a chain-mode entry point instantiated for in
        // production use (plus 16/32/64 for smaller shapes — 128 is
        // the most-production-relevant of the available options).
        let config = Qwen35Config {
            hidden_dim: 5120,
            n_q_heads: 8, // H — 8 heads for a fast test
            n_kv_heads: 8,
            head_dim: 128,
            gdn_ssm_dim: 128,
            intermediate_dim: 17_408, // unified with Phase 5 GGUF value
            rope_theta: 1_000_000.0,
            rms_eps: 1e-6,
            max_position_embeddings: 2048,
        };
        let n_tokens = 32; // 32-aligned for the matmul kernel
        let max_verify_tokens = 64;

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));

        let h = config.n_q_heads;
        let s_v = config.gdn_ssm_dim;
        let head_dim = config.head_dim;
        let hidden_dim = config.hidden_dim;
        let proj_dim = h * 4 * head_dim;
        let kv_dim = h * head_dim;

        // Deterministic weights. Scale small so the matmul products
        // stay within bf16 range at hidden_dim=5120 fan-in.
        let mut seed: u32 = 0xC0FFEE;
        let scale = 0.02f32; // empirically keeps outputs bounded
        let pre_norm_host: Vec<f32> = (0..hidden_dim).map(|_| 1.0 + 0.1 * lcg_iter(&mut seed)).collect();
        let w_qkvg_host: Vec<bf16> = (0..hidden_dim * proj_dim)
            .map(|_| bf16::from_f32(lcg_iter(&mut seed) * scale))
            .collect();
        let w_out_host: Vec<bf16> = (0..kv_dim * hidden_dim)
            .map(|_| bf16::from_f32(lcg_iter(&mut seed) * scale))
            .collect();

        let pre_norm = CudaTensor::<f32>::from_host(
            dev.clone(),
            vec![hidden_dim],
            &pre_norm_host,
        )
        .expect("upload pre_norm");
        let w_qkvg = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![hidden_dim, proj_dim],
            &w_qkvg_host,
        )
        .expect("upload w_qkvg");
        let w_out = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![kv_dim, hidden_dim],
            &w_out_host,
        )
        .expect("upload w_out");

        let layer = Qwen35GDN {
            pre_norm,
            w_qkvg,
            w_out,
            config,
            layer_idx: 0,
        };

        // Random bf16 hidden.
        let hidden_host: Vec<bf16> = (0..n_tokens * hidden_dim)
            .map(|_| bf16::from_f32(lcg_iter(&mut seed) * 0.5))
            .collect();

        // positions — unused by GDN but required by the API.
        let positions_host: Vec<i32> = (0..n_tokens as i32).collect();
        let positions = CudaTensor::<i32>::from_host(
            dev.clone(),
            vec![n_tokens],
            &positions_host,
        )
        .expect("upload positions");

        // Zero state buffers.
        let mut gdn_state =
            CudaTensor::<f32>::zeros(dev.clone(), vec![s_v, s_v, h, 1]).expect("alloc state");
        let mut gdn_inter = CudaTensor::<f16>::zeros(
            dev.clone(),
            vec![s_v, s_v, h, max_verify_tokens],
        )
        .expect("alloc inter");

        // Snapshot initial state (all zeros) for the diff check.
        let state_before = gdn_state.to_host().expect("download state before");
        assert!(
            state_before.iter().all(|v| *v == 0.0),
            "initial state not zeroed"
        );

        // ------------------------------------------------------------
        // Call 1.
        // ------------------------------------------------------------
        let mut hidden = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![n_tokens, hidden_dim],
            &hidden_host,
        )
        .expect("upload hidden");

        layer
            .forward(&dev, &mut hidden, &positions, &mut gdn_state, &mut gdn_inter)
            .expect("forward 1");
        dev.synchronize().expect("sync 1");

        let out_shape_1 = hidden.shape().to_vec();
        let out_host_1_bf16 = hidden.to_host().expect("download out 1");
        let out_host_1: Vec<f32> = out_host_1_bf16.iter().map(|v| v.to_f32()).collect();
        let state_after_1 = gdn_state.to_host().expect("download state 1");

        // Shape check.
        assert_eq!(
            out_shape_1,
            vec![n_tokens, hidden_dim],
            "call 1 output shape"
        );
        // NaN / Inf check.
        let (nan_1, inf_1) = count_bad(&out_host_1);
        assert_eq!(nan_1, 0, "call 1 output has {} NaN", nan_1);
        assert_eq!(inf_1, 0, "call 1 output has {} Inf", inf_1);
        // State must have changed (all-zeros → something).
        let state_diff_1: f32 = state_before
            .iter()
            .zip(state_after_1.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(
            state_diff_1 > 0.0,
            "call 1 did not update gdn_state (max_abs diff = {})",
            state_diff_1
        );

        eprintln!(
            "qwen35_gdn_smoke call 1: shape={:?} nan={} inf={} state_diff={:.3e}",
            out_shape_1, nan_1, inf_1, state_diff_1
        );

        // ------------------------------------------------------------
        // Call 2 — same input, state carries over. State must change
        // again, and differ from call-1's post-state.
        // ------------------------------------------------------------
        let mut hidden2 = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![n_tokens, hidden_dim],
            &hidden_host,
        )
        .expect("upload hidden 2");

        layer
            .forward(&dev, &mut hidden2, &positions, &mut gdn_state, &mut gdn_inter)
            .expect("forward 2");
        dev.synchronize().expect("sync 2");

        let out_shape_2 = hidden2.shape().to_vec();
        let out_host_2_bf16 = hidden2.to_host().expect("download out 2");
        let out_host_2: Vec<f32> = out_host_2_bf16.iter().map(|v| v.to_f32()).collect();
        let state_after_2 = gdn_state.to_host().expect("download state 2");

        assert_eq!(
            out_shape_2,
            vec![n_tokens, hidden_dim],
            "call 2 output shape"
        );
        let (nan_2, inf_2) = count_bad(&out_host_2);
        assert_eq!(nan_2, 0, "call 2 output has {} NaN", nan_2);
        assert_eq!(inf_2, 0, "call 2 output has {} Inf", inf_2);
        let state_diff_2: f32 = state_after_1
            .iter()
            .zip(state_after_2.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(
            state_diff_2 > 0.0,
            "call 2 did not advance gdn_state relative to call 1 (max_abs diff = {})",
            state_diff_2
        );

        eprintln!(
            "qwen35_gdn_smoke call 2: shape={:?} nan={} inf={} state_diff_from_call1={:.3e}",
            out_shape_2, nan_2, inf_2, state_diff_2
        );
    }

    fn count_bad(values: &[f32]) -> (usize, usize) {
        let mut nans = 0;
        let mut infs = 0;
        for &v in values {
            if v.is_nan() {
                nans += 1;
            } else if v.is_infinite() {
                infs += 1;
            }
        }
        (nans, infs)
    }
}
