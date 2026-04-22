//! Qwen3.5 full-attention block — CUDA layer composition.
//!
//! This is one of two layer shapes in the Qwen3.5-27B hybrid. The
//! other is Gated DeltaNet; the full-attention layers are triggered on
//! every `full_attention_interval`-th index (≈16 of 64 on 27B).
//!
//! # Ops, in order
//!
//! For input hidden state `x`, this layer computes:
//!
//! 1. `tmp   = x` (saved residual)
//! 2. `xf32  = cast_bf16_to_f32(x)`
//! 3. `nf32  = rmsnorm(xf32, attn_norm)`
//! 4. `norm  = cast_f32_to_bf16(nf32)`
//! 5. `q     = norm @ w_q`          `[n_tokens, n_q_heads * head_dim]` bf16
//!    `k     = norm @ w_k`          `[n_tokens, n_kv_heads * head_dim]` bf16
//!    `v     = norm @ w_v`          `[n_tokens, n_kv_heads * head_dim]` bf16
//! 6. reshape `q` to `[n_tokens, n_q_heads, head_dim]`;
//!    reshape `k` to `[n_tokens, n_kv_heads, head_dim]`
//! 7. RoPE in place on `q` and `k` (MRoPE, `rope_dim = head_dim`)
//! 8. Append `k`, `v` into `kv_cache` at the current `n_filled` slot;
//!    advance `n_filled` by `n_tokens`.
//! 9. Per-Q-head attention, with GQA:
//!       - `k_head = kv_cache.k_slab[layer][:n_filled, q_head // gqa_group]`
//!       - `v_head = kv_cache.v_slab[layer][:n_filled, q_head // gqa_group]`
//!       - `scores = q_head @ k_head^T`               `[n_tokens, n_filled]` f32
//!       - `scores *= 1/sqrt(head_dim)` + causal mask
//!       - `attn   = softmax(scores)`                 `[n_tokens, n_filled]` f32
//!       - `out_head = attn_bf16 @ v_head`            `[n_tokens, head_dim]` bf16
//!    Per-head outputs concatenate back into `[n_tokens, q_dim]`.
//! 10. `proj = attn_out @ w_o`                        `[n_tokens, hidden]` bf16
//! 11. `x  ← residual_add_bf16(proj, tmp)`
//!
//! # Scope + TODOs
//!
//! This is the **first-port** layer composition. It is good enough to
//! prove the kernel primitives compose and the KV cache wires through
//! end-to-end; it is not tuned for production throughput.
//!
//! * Weights here ship as plain `CudaTensor<bf16>` and projections use
//!   `launch_matmul_bf16_bf16`. Production weights are Q4_K_M quantized
//!   (`CudaTensor<i8>` of Q4 blocks) and will switch to
//!   `launch_mmvq_q4k_f16/_f32` once a GGUF-loader integration lands.
//!   The struct's fields are therefore left at `CudaTensor<bf16>` — the
//!   Q4K migration is tracked as a separate ticket (Phase 4).
//! * Per-head attention uses host-side O(n_q_heads × n_tokens) device
//!   memcpys to gather strided head views into contiguous staging
//!   tensors. A fused strided-matmul (or FlashAttention kernel) is the
//!   follow-up once correctness is locked.
//! * Causal mask is built on the host and uploaded once per forward.
//!   A fused masked-softmax kernel is a follow-up optimization.
//! * The reference dflash model adds per-head Q/K RMSNorm and packs a
//!   sigmoid gate into the Q projection. **Both land in this port**:
//!   `attn_q_norm` / `attn_k_norm` run after the Q/K reshape-to-per-head
//!   and before RoPE; `w_q_gate` is the second half of the GGUF's
//!   packed `attn_q.weight` and enters as `attn_out *= sigmoid(x @
//!   w_q_gate)` after the attention output and before the `w_o`
//!   projection. See the dflash reference's `build_full_attn_block`
//!   at `dflash/src/qwen35_target_graph.cpp` for the exact ordering.
//! * The layer's `post_attn_norm` slot is held here so the weight
//!   loader has somewhere to put `post_attention_norm.weight`, but
//!   the actual pre-FFN norm happens **in the FFN block** (Agent O's
//!   territory), not inside this layer's forward. Keeping the field
//!   on this struct just means the GGUF's layer-level weight set
//!   fits without a second per-layer struct for norms.
//! * Matmul tile alignment: inputs must be multiples of `TILE=32`
//!   along every axis. This means `n_tokens`, `kv_n_filled`, and all
//!   projection widths must be 32-aligned. `hidden_dim`, `q_dim`,
//!   `kv_dim`, `head_dim` are all ≥32 and naturally aligned for
//!   production Qwen3.5-27B; the smoke test pads `n_tokens` to 32.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use crate::kernels;
use ctox_cuda_primitives::kv_cache::KvCache;
use ctox_cuda_primitives::tensor::CudaTensor;

use crate::config::Qwen35Config;

/// A single full-attention layer's parameters + config.
///
/// Fields are `CudaTensor<bf16>` in this first port. Production builds
/// will swap the weight tensors to `CudaTensor<i8>` (Q4_K_M blocks) and
/// call `launch_mmvq_q4k_*` instead of `launch_matmul_bf16_bf16`. See
/// the module-level TODO for the migration plan.
pub struct Qwen35FullAttention {
    /// RMSNorm weight applied pre-projection. `[hidden_dim]` f32.
    pub attn_norm: CudaTensor<f32>,
    /// Per-head RMSNorm applied to Q after the Q projection + reshape
    /// to `[n_tokens, n_q_heads, head_dim]` and before RoPE.
    /// `[head_dim]` f32. Loaded from `blk.L.attn_q_norm.weight`.
    pub attn_q_norm: CudaTensor<f32>,
    /// Per-head RMSNorm applied to K. `[head_dim]` f32. Loaded from
    /// `blk.L.attn_k_norm.weight`.
    pub attn_k_norm: CudaTensor<f32>,
    /// Pre-FFN RMSNorm weight — `[hidden_dim]` f32 — loaded from
    /// `blk.L.post_attention_norm.weight`. Applied **in the FFN
    /// block** (Agent O), not here. This struct holds the slot so the
    /// GGUF loader has somewhere to deposit it; the FA forward does
    /// not reference it.
    pub post_attn_norm: CudaTensor<f32>,
    /// Q projection weight (first half of the GGUF's packed
    /// `attn_q.weight`). `[hidden_dim, n_q_heads * head_dim]` bf16,
    /// row-major with K=hidden_dim and N=q_dim.
    pub w_q: CudaTensor<bf16>,
    /// Q-side gate weight (second half of the GGUF's packed
    /// `attn_q.weight`). `[hidden_dim, n_q_heads * head_dim]` bf16.
    /// Consumed by the sigmoid-gate branch after attention: `attn ←
    /// attn * sigmoid(norm @ w_q_gate)`.
    pub w_q_gate: CudaTensor<bf16>,
    /// K projection weight. `[hidden_dim, n_kv_heads * head_dim]` bf16.
    pub w_k: CudaTensor<bf16>,
    /// V projection weight. `[hidden_dim, n_kv_heads * head_dim]` bf16.
    pub w_v: CudaTensor<bf16>,
    /// Output projection weight. `[n_q_heads * head_dim, hidden_dim]` bf16.
    pub w_o: CudaTensor<bf16>,
    /// Architectural constants (heads, head_dim, rope base, eps).
    pub config: Qwen35Config,
    /// Which layer of the hybrid stack this is. Used to index into the
    /// KV cache's per-layer slabs.
    pub layer_idx: usize,
}

impl Qwen35FullAttention {
    /// Execute one forward pass. Mutates `hidden` in place and appends
    /// `n_tokens` rows into `kv_cache`'s K/V slabs for this layer.
    ///
    /// Assumes `hidden.shape() == [n_tokens, hidden_dim]` bf16 and
    /// `positions.shape() == [4, n_tokens]` i32 (MRoPE 4-axis).
    ///
    /// All intermediate tensors are allocated from `device`'s default
    /// stream; no stream syncs happen until the caller chooses. The
    /// output tensor is written back into `hidden` via
    /// `launch_residual_add_bf16`.
    pub fn forward(
        &self,
        device: &Arc<DeviceContext>,
        hidden: &mut CudaTensor<bf16>,
        positions: &CudaTensor<i32>,
        kv_cache: &mut KvCache,
    ) -> Result<()> {
        // ── 0. Shape & precondition checks.
        let cfg = &self.config;
        if hidden.shape().len() != 2 {
            return Err(anyhow!(
                "qwen35 full_attn: hidden must be 2D [n_tokens, hidden_dim], got {:?}",
                hidden.shape()
            ));
        }
        let n_tokens = hidden.shape()[0];
        let hidden_dim = hidden.shape()[1];
        if hidden_dim != cfg.hidden_dim {
            return Err(anyhow!(
                "qwen35 full_attn: hidden_dim {} != config.hidden_dim {}",
                hidden_dim,
                cfg.hidden_dim
            ));
        }
        if self.attn_norm.shape() != [cfg.hidden_dim] {
            return Err(anyhow!(
                "qwen35 full_attn: attn_norm shape {:?} != [{}]",
                self.attn_norm.shape(),
                cfg.hidden_dim
            ));
        }
        if self.attn_q_norm.shape() != [cfg.head_dim] {
            return Err(anyhow!(
                "qwen35 full_attn: attn_q_norm shape {:?} != [{}]",
                self.attn_q_norm.shape(),
                cfg.head_dim
            ));
        }
        if self.attn_k_norm.shape() != [cfg.head_dim] {
            return Err(anyhow!(
                "qwen35 full_attn: attn_k_norm shape {:?} != [{}]",
                self.attn_k_norm.shape(),
                cfg.head_dim
            ));
        }
        if self.post_attn_norm.shape() != [cfg.hidden_dim] {
            return Err(anyhow!(
                "qwen35 full_attn: post_attn_norm shape {:?} != [{}]",
                self.post_attn_norm.shape(),
                cfg.hidden_dim
            ));
        }
        if self.w_q_gate.shape() != [cfg.hidden_dim, cfg.q_dim()] {
            return Err(anyhow!(
                "qwen35 full_attn: w_q_gate shape {:?} != [{}, {}]",
                self.w_q_gate.shape(),
                cfg.hidden_dim,
                cfg.q_dim()
            ));
        }
        if self.w_q.shape() != [cfg.hidden_dim, cfg.q_dim()] {
            return Err(anyhow!(
                "qwen35 full_attn: w_q shape {:?} != [{}, {}]",
                self.w_q.shape(),
                cfg.hidden_dim,
                cfg.q_dim()
            ));
        }
        if self.w_k.shape() != [cfg.hidden_dim, cfg.kv_dim()] {
            return Err(anyhow!(
                "qwen35 full_attn: w_k shape {:?} != [{}, {}]",
                self.w_k.shape(),
                cfg.hidden_dim,
                cfg.kv_dim()
            ));
        }
        if self.w_v.shape() != [cfg.hidden_dim, cfg.kv_dim()] {
            return Err(anyhow!(
                "qwen35 full_attn: w_v shape {:?} != [{}, {}]",
                self.w_v.shape(),
                cfg.hidden_dim,
                cfg.kv_dim()
            ));
        }
        if self.w_o.shape() != [cfg.q_dim(), cfg.hidden_dim] {
            return Err(anyhow!(
                "qwen35 full_attn: w_o shape {:?} != [{}, {}]",
                self.w_o.shape(),
                cfg.q_dim(),
                cfg.hidden_dim
            ));
        }
        if kv_cache.head_dim() != cfg.head_dim || kv_cache.n_kv_heads() != cfg.n_kv_heads {
            return Err(anyhow!(
                "qwen35 full_attn: kv_cache shape (heads={} head_dim={}) mismatches config (heads={} head_dim={})",
                kv_cache.n_kv_heads(),
                kv_cache.head_dim(),
                cfg.n_kv_heads,
                cfg.head_dim
            ));
        }
        if self.layer_idx >= kv_cache.n_layers() {
            return Err(anyhow!(
                "qwen35 full_attn: layer_idx {} >= kv_cache.n_layers {}",
                self.layer_idx,
                kv_cache.n_layers()
            ));
        }
        let prompt_start = kv_cache.n_filled();
        if prompt_start + n_tokens > kv_cache.max_ctx() {
            return Err(anyhow!(
                "qwen35 full_attn: prompt_start {} + n_tokens {} > max_ctx {}",
                prompt_start,
                n_tokens,
                kv_cache.max_ctx()
            ));
        }

        // ── 1. Save the residual. We do the add at the end.
        let residual = {
            let mut r = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
            let stream = device.raw().default_stream();
            stream.memcpy_dtod(hidden.buf(), r.buf_mut()).map_err(|e| {
                anyhow!("qwen35 full_attn: residual memcpy_dtod: {:?}", e)
            })?;
            r
        };

        // ── 2. Pre-norm in f32 (RMSNorm is numerically sensitive).
        let mut hidden_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        kernels::launch_cast_bf16_to_f32(device, hidden, &mut hidden_f32)?;
        let mut norm_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        kernels::launch_rmsnorm_f32(device, &hidden_f32, &self.attn_norm, &mut norm_f32, cfg.rms_eps)?;
        let mut norm = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        kernels::launch_cast_f32_to_bf16(device, &norm_f32, &mut norm)?;

        // ── 3. Q/K/V projections (bf16·bf16 → bf16).
        let q_dim = cfg.q_dim();
        let kv_dim = cfg.kv_dim();
        let mut q_flat = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, q_dim])?;
        kernels::launch_matmul_bf16_bf16(device, &norm, &self.w_q, &mut q_flat, n_tokens, hidden_dim, q_dim)?;
        let mut k_flat = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, kv_dim])?;
        kernels::launch_matmul_bf16_bf16(device, &norm, &self.w_k, &mut k_flat, n_tokens, hidden_dim, kv_dim)?;
        let mut v_flat = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, kv_dim])?;
        kernels::launch_matmul_bf16_bf16(device, &norm, &self.w_v, &mut v_flat, n_tokens, hidden_dim, kv_dim)?;

        // ── 3b. Compute the sigmoid-gate tensor for the post-attention
        //    elementwise multiply. The gate projection is `norm @ w_q_gate`
        //    → `[n_tokens, q_dim]` bf16. We apply `sigmoid` on the host
        //    (no kernel yet) and keep it around until after attention.
        //
        //    Why host-sigmoid: the rest of the pipeline only needs
        //    elementwise-mul which we can do via `launch_residual_add`
        //    composed after a host scale, or via a round-trip — same
        //    cost-class as the causal mask's host build above and far
        //    simpler than adding a third kernel for ~kloc of new code.
        //    A fused sigmoid-mul kernel is a Phase-5 optimization.
        let mut q_gate = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, q_dim])?;
        kernels::launch_matmul_bf16_bf16(
            device,
            &norm,
            &self.w_q_gate,
            &mut q_gate,
            n_tokens,
            hidden_dim,
            q_dim,
        )?;
        let q_gate_sig = sigmoid_host_bf16(&q_gate)?;

        // ── 3c. Per-head RMSNorm on Q and K (in place on the flat
        //    tensors). dflash applies these after the per-head reshape
        //    and before RoPE; because the flat layout is row-major
        //    `[n_tokens, n_heads * head_dim]`, it's equivalent to
        //    rmsnorm'ing over the last axis of `[n_tokens * n_heads,
        //    head_dim]` — exactly what `launch_rmsnorm_f32` does.
        //
        //    The rmsnorm kernel is f32-only, so we stage through f32
        //    buffers. This matches the pre-projection norm at step 2.
        per_head_rmsnorm_bf16(
            device,
            &mut q_flat,
            &self.attn_q_norm,
            n_tokens,
            cfg.n_q_heads,
            cfg.head_dim,
            cfg.rms_eps,
        )?;
        per_head_rmsnorm_bf16(
            device,
            &mut k_flat,
            &self.attn_k_norm,
            n_tokens,
            cfg.n_kv_heads,
            cfg.head_dim,
            cfg.rms_eps,
        )?;

        // ── 4. Reshape Q, K into the per-head layout RoPE expects.
        //    `_flat` tensors are `[n_tokens, N]` row-major; the per-head
        //    reinterpretation is just a shape-change on the same buffer.
        //    We don't own a `reshape` op, so we rebuild the tensor via
        //    the uploader-free ctor — but CudaTensor<T>::from_host is
        //    the only public ctor besides zeros, and we already have
        //    the data on device. Solution: use a view-lite reconstruct
        //    via `reshape_like` below.
        let mut q3 = reshape_3d(q_flat, n_tokens, cfg.n_q_heads, cfg.head_dim)?;
        let mut k3 = reshape_3d(k_flat, n_tokens, cfg.n_kv_heads, cfg.head_dim)?;

        // ── 5. RoPE in place (MRoPE — 4-axis positions).
        kernels::launch_rope_mrope_bf16(device, &mut q3, positions, cfg.rope_theta, cfg.head_dim as i32)?;
        kernels::launch_rope_mrope_bf16(device, &mut k3, positions, cfg.rope_theta, cfg.head_dim as i32)?;

        // ── 6. Write K/V into the KV cache. Order matters: append both
        //    at the current offset, then advance.
        let v3 = reshape_3d(v_flat, n_tokens, cfg.n_kv_heads, cfg.head_dim)?;
        kv_cache.append_k(self.layer_idx, prompt_start, &k3)?;
        kv_cache.append_v(self.layer_idx, prompt_start, &v3)?;
        kv_cache.advance(n_tokens);

        let kv_len = kv_cache.n_filled();

        // ── 7. Attention, per Q head. The GQA group maps Q head `h` to
        //    KV head `h / gqa_group`. We gather both the Q head stripe
        //    and the KV slab head stripe into contiguous staging
        //    tensors so the plain matmul kernels accept them.
        //
        //    Output layout matches the Q flat layout: `[n_tokens, q_dim]`
        //    row-major, Q-head-major.
        let mut attn_out = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, q_dim])?;
        let scale = 1.0f32 / (cfg.head_dim as f32).sqrt();
        let mask_host = build_causal_mask(n_tokens, kv_len, prompt_start, scale);

        for q_head in 0..cfg.n_q_heads {
            let kv_head = q_head / cfg.gqa_group();

            // ── 7a. Gather Q head stripe: a strided slice of q3.
            //       q3 is `[n_tokens, n_q_heads, head_dim]` row-major;
            //       per-token stripe for head `q_head` sits at offset
            //       `q_head * head_dim` with stride `q_dim` between tokens.
            let q_head_tensor = gather_head_from_packed(
                device,
                &q3,
                n_tokens,
                cfg.n_q_heads,
                cfg.head_dim,
                q_head,
            )?;

            // ── 7b. Gather K/V stripes from the cache. Slab layout is
            //       `[max_ctx, n_kv_heads, head_dim]` row-major; we need
            //       the first `kv_len` rows of head `kv_head`.
            let k_head_tensor = gather_head_from_kv_slab(
                device,
                kv_cache.k_slab(self.layer_idx),
                kv_len,
                cfg.n_kv_heads,
                cfg.head_dim,
                kv_head,
            )?;
            let v_head_tensor = gather_head_from_kv_slab(
                device,
                kv_cache.v_slab(self.layer_idx),
                kv_len,
                cfg.n_kv_heads,
                cfg.head_dim,
                kv_head,
            )?;

            // ── 7c. scores = q_head · k_head^T. The matmul kernel wants
            //       `A[M,K] · B[K,N]`, so we need k_head transposed.
            //       Materialize the transpose into a staging buffer:
            //       `k_head_T [head_dim, kv_len]`.
            let k_head_t = transpose_2d(device, &k_head_tensor, kv_len, cfg.head_dim)?;

            let mut scores = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, kv_len])?;
            kernels::launch_matmul_bf16_f32(
                device,
                &q_head_tensor,
                &k_head_t,
                &mut scores,
                n_tokens,
                cfg.head_dim,
                kv_len,
            )?;

            // ── 7d. scale + causal mask + softmax, all f32.
            let mut scaled_masked = upload_mask(device, &mask_host, n_tokens, kv_len)?;
            // scaled_masked currently holds the mask. We need to compute
            // `scaled_masked[i,j] = scores[i,j] * scale + mask[i,j]`.
            // We have no fused kernel; fold `scale` into the score matmul
            // by pre-scaling on the host via one scalar multiply plus an
            // elementwise add kernel. Since we don't have an `axpby`
            // yet, implement via residual_add_f32 after pre-scaling the
            // scores on the device: the cheapest general path is a host
            // round-trip + re-upload, which is fine for the smoke test.
            // TODO: replace with a fused scale-add-softmax kernel.
            host_scale_add(device, &mut scores, &mut scaled_masked, scale)?;

            let mut attn_weights =
                CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, kv_len])?;
            kernels::launch_softmax_f32(device, &scaled_masked, &mut attn_weights)?;

            // ── 7e. out_head = attn_weights · v_head. Downcast attn to
            //       bf16 for the bf16 matmul; v_head is already bf16.
            let mut attn_weights_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, kv_len])?;
            kernels::launch_cast_f32_to_bf16(device, &attn_weights, &mut attn_weights_bf16)?;

            let mut out_head =
                CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, cfg.head_dim])?;
            kernels::launch_matmul_bf16_bf16(
                device,
                &attn_weights_bf16,
                &v_head_tensor,
                &mut out_head,
                n_tokens,
                kv_len,
                cfg.head_dim,
            )?;

            // ── 7f. Scatter `out_head` back into the packed attn_out
            //       at the `q_head * head_dim` stripe per token.
            scatter_head_into_packed(
                device,
                &out_head,
                &mut attn_out,
                n_tokens,
                cfg.n_q_heads,
                cfg.head_dim,
                q_head,
            )?;
        }

        // ── 7g. Sigmoid gate elementwise-mul. dflash's
        //    `build_full_attn_block` applies `attn = attn * sigmoid(gate)`
        //    after the attention output is re-flattened into `[q_dim,
        //    n_tokens]`. Here `attn_out` is already `[n_tokens, q_dim]`
        //    row-major and `q_gate_sig` is the same shape — so the mul
        //    is fully elementwise. We do it on the host alongside the
        //    sigmoid computation at step 3b to avoid a dedicated kernel.
        elementwise_mul_host_bf16(&mut attn_out, &q_gate_sig)?;

        // ── 8. Output projection.
        let mut proj = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        kernels::launch_matmul_bf16_bf16(device, &attn_out, &self.w_o, &mut proj, n_tokens, q_dim, hidden_dim)?;

        // ── 9. Residual add. `hidden ← proj + residual` (in-place on hidden).
        //
        //    We can't pass `hidden` as both output and input to
        //    `launch_residual_add_bf16` (borrow-check: `&` and `&mut`
        //    conflict). Route through a small staging tensor.
        let mut summed = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        kernels::launch_residual_add_bf16(device, &proj, &residual, &mut summed)?;
        let stream = device.raw().default_stream();
        stream
            .memcpy_dtod(summed.buf(), hidden.buf_mut())
            .map_err(|e| anyhow!("qwen35 full_attn: final copy back to hidden: {:?}", e))?;
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────────
// Helpers.
// ────────────────────────────────────────────────────────────────────

/// Construct a fresh `[d0, d1, d2]`-shaped `CudaTensor<bf16>` that
/// takes ownership of an existing flat `[d0, d1*d2]` bf16 buffer.
///
/// `CudaTensor` doesn't expose a public `reshape` (deliberately — no
/// strided views), so the cheapest "reshape" we can build is a copy
/// into a fresh tensor with the right shape metadata. This is one
/// extra device-to-device memcpy per projection per forward; a fused
/// `reshape_into(&mut self, shape)` on `CudaTensor` would remove it.
fn reshape_3d(src: CudaTensor<bf16>, d0: usize, d1: usize, d2: usize) -> Result<CudaTensor<bf16>> {
    let dev = src.device().clone();
    if src.numel() != d0 * d1 * d2 {
        return Err(anyhow!(
            "reshape_3d: numel {} != {}*{}*{}",
            src.numel(),
            d0,
            d1,
            d2
        ));
    }
    let mut dst = CudaTensor::<bf16>::zeros(dev.clone(), vec![d0, d1, d2])?;
    let stream = dev.raw().default_stream();
    stream
        .memcpy_dtod(src.buf(), dst.buf_mut())
        .map_err(|e| anyhow!("reshape_3d memcpy_dtod: {:?}", e))?;
    Ok(dst)
}

/// Extract head `h` from a `[n_tokens, n_heads, head_dim]` bf16
/// tensor into a contiguous `[n_tokens, head_dim]` tensor, one
/// `memcpy_dtod` per token. O(n_tokens) device dispatches.
///
/// TODO: Collapse into a single strided gather kernel once it becomes
///       a hotspot. Per-token memcpy_dtod is strictly a correctness-
///       first path.
fn gather_head_from_packed(
    device: &Arc<DeviceContext>,
    packed: &CudaTensor<bf16>,
    n_tokens: usize,
    n_heads: usize,
    head_dim: usize,
    head: usize,
) -> Result<CudaTensor<bf16>> {
    debug_assert_eq!(packed.shape(), [n_tokens, n_heads, head_dim]);
    let mut dst = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, head_dim])?;
    let stream = device.raw().default_stream();
    let stride = n_heads * head_dim;
    for t in 0..n_tokens {
        let src_start = t * stride + head * head_dim;
        let src_end = src_start + head_dim;
        let dst_start = t * head_dim;
        let dst_end = dst_start + head_dim;
        let src_view = packed.buf().slice(src_start..src_end);
        let mut dst_view = dst.buf_mut().slice_mut(dst_start..dst_end);
        stream.memcpy_dtod(&src_view, &mut dst_view).map_err(|e| {
            anyhow!(
                "gather_head_from_packed memcpy_dtod (t={} head={}): {:?}",
                t,
                head,
                e
            )
        })?;
    }
    Ok(dst)
}

/// Scatter a `[n_tokens, head_dim]` tensor into slot `head` of a
/// `[n_tokens, n_heads, head_dim]` destination. Mirror of
/// `gather_head_from_packed`.
fn scatter_head_into_packed(
    device: &Arc<DeviceContext>,
    head_tensor: &CudaTensor<bf16>,
    packed: &mut CudaTensor<bf16>,
    n_tokens: usize,
    n_heads: usize,
    head_dim: usize,
    head: usize,
) -> Result<()> {
    debug_assert_eq!(head_tensor.shape(), [n_tokens, head_dim]);
    debug_assert_eq!(packed.shape(), [n_tokens, n_heads * head_dim]);
    let stream = device.raw().default_stream();
    let stride = n_heads * head_dim;
    for t in 0..n_tokens {
        let dst_start = t * stride + head * head_dim;
        let dst_end = dst_start + head_dim;
        let src_start = t * head_dim;
        let src_end = src_start + head_dim;
        let src_view = head_tensor.buf().slice(src_start..src_end);
        let mut dst_view = packed.buf_mut().slice_mut(dst_start..dst_end);
        stream.memcpy_dtod(&src_view, &mut dst_view).map_err(|e| {
            anyhow!(
                "scatter_head_into_packed memcpy_dtod (t={} head={}): {:?}",
                t,
                head,
                e
            )
        })?;
    }
    Ok(())
}

/// Pull a contiguous `[kv_len, head_dim]` slice out of a KV slab. Slab
/// layout is `[max_ctx, n_kv_heads, head_dim]` row-major, so per-token
/// elements for `head = h` live at offset `t*stride + h*head_dim`.
fn gather_head_from_kv_slab(
    device: &Arc<DeviceContext>,
    slab: &cudarc::driver::CudaSlice<bf16>,
    kv_len: usize,
    n_kv_heads: usize,
    head_dim: usize,
    head: usize,
) -> Result<CudaTensor<bf16>> {
    let mut dst = CudaTensor::<bf16>::zeros(device.clone(), vec![kv_len, head_dim])?;
    let stream = device.raw().default_stream();
    let stride = n_kv_heads * head_dim;
    for t in 0..kv_len {
        let src_start = t * stride + head * head_dim;
        let src_end = src_start + head_dim;
        let dst_start = t * head_dim;
        let dst_end = dst_start + head_dim;
        let src_view = slab.slice(src_start..src_end);
        let mut dst_view = dst.buf_mut().slice_mut(dst_start..dst_end);
        stream.memcpy_dtod(&src_view, &mut dst_view).map_err(|e| {
            anyhow!(
                "gather_head_from_kv_slab memcpy_dtod (t={} head={}): {:?}",
                t,
                head,
                e
            )
        })?;
    }
    Ok(dst)
}

/// Host-side 2-D transpose via download/reupload. Used to produce
/// `K^T` for the attention score matmul. O(n_rows × n_cols) host
/// memory — fine for head-sized workloads (kv_len × head_dim = 128 ×
/// 128 = 16K elements per head at decode-start).
///
/// TODO: replace with a proper device-side transpose kernel.
fn transpose_2d(
    device: &Arc<DeviceContext>,
    src: &CudaTensor<bf16>,
    rows: usize,
    cols: usize,
) -> Result<CudaTensor<bf16>> {
    debug_assert_eq!(src.shape(), [rows, cols]);
    let host = src.to_host()?;
    let mut transposed: Vec<bf16> = vec![bf16::ZERO; rows * cols];
    for r in 0..rows {
        for c in 0..cols {
            transposed[c * rows + r] = host[r * cols + c];
        }
    }
    CudaTensor::<bf16>::from_host(device.clone(), vec![cols, rows], &transposed)
}

/// Build an `[n_tokens, kv_len]` f32 mask. Positions beyond the causal
/// boundary get `-inf`; valid positions contribute `0.0` (i.e. no
/// additive shift to the score). The prompt_start offset shifts the
/// query index into absolute context space.
fn build_causal_mask(n_tokens: usize, kv_len: usize, prompt_start: usize, _scale: f32) -> Vec<f32> {
    let mut mask = vec![0.0f32; n_tokens * kv_len];
    for i in 0..n_tokens {
        for j in 0..kv_len {
            if j > prompt_start + i {
                mask[i * kv_len + j] = f32::NEG_INFINITY;
            }
        }
    }
    mask
}

fn upload_mask(
    device: &Arc<DeviceContext>,
    mask: &[f32],
    n_tokens: usize,
    kv_len: usize,
) -> Result<CudaTensor<f32>> {
    CudaTensor::<f32>::from_host(device.clone(), vec![n_tokens, kv_len], mask)
}

/// `scaled_masked ← scores * scale + mask`, host-loop fallback.
///
/// We lack a fused scale+add kernel, so download both, compute, then
/// re-upload. Scores are `[n_tokens, kv_len]` — at n_tokens=32,
/// kv_len=32 this is 1 KiB of f32, vanishingly small; at production
/// sizes this path must be replaced.
///
/// TODO: fused scale+add+softmax kernel.
fn host_scale_add(
    _device: &Arc<DeviceContext>,
    scores: &mut CudaTensor<f32>,
    scaled_masked: &mut CudaTensor<f32>,
    scale: f32,
) -> Result<()> {
    let scores_host = scores.to_host()?;
    let mask_host = scaled_masked.to_host()?;
    if scores_host.len() != mask_host.len() {
        return Err(anyhow!(
            "host_scale_add: scores.len {} != mask.len {}",
            scores_host.len(),
            mask_host.len()
        ));
    }
    let out_host: Vec<f32> = scores_host
        .iter()
        .zip(mask_host.iter())
        .map(|(s, m)| s * scale + m)
        .collect();
    let dev = scaled_masked.device().clone();
    let shape = scaled_masked.shape().to_vec();
    *scaled_masked = CudaTensor::<f32>::from_host(dev, shape, &out_host)?;
    Ok(())
}

/// In-place per-head RMSNorm on a `[n_tokens, n_heads * head_dim]` bf16
/// tensor with weight `[head_dim]` f32.
///
/// The row-major `[n_tokens, n_heads * head_dim]` layout is bit-equal
/// to `[n_tokens * n_heads, head_dim]` row-major; the kernel normalizes
/// per row (per head-per-token). We stage through f32 buffers since
/// the current RMSNorm kernel is f32-only.
///
/// Cost: one cast (bf16 → f32), two D2D memcpys to reshape into + back
/// out of a 2-D f32 tensor of shape `[n_tokens*n_heads, head_dim]`, the
/// rmsnorm itself, and one cast (f32 → bf16). Identical cost-class to
/// the existing `reshape_3d` helper used at RoPE time; a fused bf16
/// rmsnorm kernel would collapse this down to a single launch but is
/// phase-5 material.
fn per_head_rmsnorm_bf16(
    device: &Arc<DeviceContext>,
    x: &mut CudaTensor<bf16>,
    weight: &CudaTensor<f32>,
    n_tokens: usize,
    n_heads: usize,
    head_dim: usize,
    eps: f32,
) -> Result<()> {
    debug_assert_eq!(x.shape(), [n_tokens, n_heads * head_dim]);
    debug_assert_eq!(weight.shape(), [head_dim]);

    let rows = n_tokens * n_heads;

    // bf16 → f32 on the [n_tokens, n_heads*head_dim] layout.
    let mut xf32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, n_heads * head_dim])?;
    kernels::launch_cast_bf16_to_f32(device, x, &mut xf32)?;

    // Reshape xf32 into [rows, head_dim] via D2D memcpy. Same pattern
    // the bf16 path uses at RoPE time.
    let mut xf32_2d = CudaTensor::<f32>::zeros(device.clone(), vec![rows, head_dim])?;
    {
        let stream = device.raw().default_stream();
        stream
            .memcpy_dtod(xf32.buf(), xf32_2d.buf_mut())
            .map_err(|e| anyhow!("per_head_rmsnorm_bf16: reshape-to-2d memcpy_dtod: {:?}", e))?;
    }

    // RMSNorm in f32, per-row over head_dim.
    let mut yf32_2d = CudaTensor::<f32>::zeros(device.clone(), vec![rows, head_dim])?;
    kernels::launch_rmsnorm_f32(device, &xf32_2d, weight, &mut yf32_2d, eps)?;

    // Reshape back to [n_tokens, n_heads*head_dim] f32.
    let mut yf32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, n_heads * head_dim])?;
    {
        let stream = device.raw().default_stream();
        stream
            .memcpy_dtod(yf32_2d.buf(), yf32.buf_mut())
            .map_err(|e| anyhow!("per_head_rmsnorm_bf16: reshape-from-2d memcpy_dtod: {:?}", e))?;
    }

    // f32 → bf16 into x (in-place output of caller).
    kernels::launch_cast_f32_to_bf16(device, &yf32, x)?;
    Ok(())
}

/// Compute `sigmoid` of a bf16 tensor on the host and return the result
/// as a fresh device tensor of matching shape.
///
/// We have no sigmoid kernel yet; the gate path runs once per forward
/// on `n_tokens * q_dim` bf16 elements, i.e. ~200 KiB at 32×6144 — fine
/// for a host round-trip. A fused sigmoid-mul kernel folds both this
/// and `elementwise_mul_host_bf16` into one launch; tracked as a
/// phase-5 optimization.
fn sigmoid_host_bf16(x: &CudaTensor<bf16>) -> Result<CudaTensor<bf16>> {
    let host = x.to_host()?;
    let out_host: Vec<bf16> = host
        .iter()
        .map(|v| {
            let f = v.to_f32();
            bf16::from_f32(1.0 / (1.0 + (-f).exp()))
        })
        .collect();
    CudaTensor::<bf16>::from_host(x.device().clone(), x.shape().to_vec(), &out_host)
}

/// In-place `x ← x * y` elementwise for bf16 tensors of identical
/// shape, on the host. Paired with [`sigmoid_host_bf16`] for the
/// attention gate mul.
fn elementwise_mul_host_bf16(x: &mut CudaTensor<bf16>, y: &CudaTensor<bf16>) -> Result<()> {
    if x.shape() != y.shape() {
        return Err(anyhow!(
            "elementwise_mul_host_bf16: shape {:?} != {:?}",
            x.shape(),
            y.shape()
        ));
    }
    let xh = x.to_host()?;
    let yh = y.to_host()?;
    let out: Vec<bf16> = xh
        .iter()
        .zip(yh.iter())
        .map(|(a, b)| bf16::from_f32(a.to_f32() * b.to_f32()))
        .collect();
    let dev = x.device().clone();
    let shape = x.shape().to_vec();
    *x = CudaTensor::<bf16>::from_host(dev, shape, &out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic LCG — produces reproducible pseudo-random bf16
    /// weights/activations for the smoke path.
    fn lcg_iter(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        ((*seed >> 16) as f32 / 32768.0) - 1.0
    }

    fn random_bf16(n: usize, seed: &mut u32, amplitude: f32) -> Vec<bf16> {
        (0..n)
            .map(|_| bf16::from_f32(lcg_iter(seed) * amplitude))
            .collect()
    }

    fn random_f32(n: usize, seed: &mut u32, amplitude: f32) -> Vec<f32> {
        (0..n).map(|_| lcg_iter(seed) * amplitude).collect()
    }

    /// End-to-end smoke: run forward on synthetic-random weights for a
    /// single layer, assert shapes + `n_filled` advance + no NaN/Inf.
    ///
    /// Matmul tile alignment forces `n_tokens % 32 == 0`, so we use 32
    /// rather than the brief's suggested 16 (both satisfy the "≥1 row,
    /// cache advances" correctness gate).
    ///
    /// Run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///     --ignored --nocapture qwen35_full_attention_smoke
    #[test]
    #[ignore]
    fn qwen35_full_attention_smoke() {
        let cfg = Qwen35Config::QWEN35_27B;
        let n_tokens: usize = 32;
        let max_ctx: usize = 128;

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let mut seed: u32 = 0x9E3779B9;

        // Weights. `attn_norm` in f32; projections in bf16 (see the
        // module-level TODO for the Q4K migration).
        let attn_norm_host: Vec<f32> = (0..cfg.hidden_dim)
            .map(|_| lcg_iter(&mut seed).abs() * 0.5 + 0.5)
            .collect();
        let attn_norm = CudaTensor::<f32>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim],
            &attn_norm_host,
        )
        .expect("upload attn_norm");

        let w_q_host = random_bf16(cfg.hidden_dim * cfg.q_dim(), &mut seed, 0.02);
        let w_q = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim, cfg.q_dim()],
            &w_q_host,
        )
        .expect("upload w_q");

        let w_k_host = random_bf16(cfg.hidden_dim * cfg.kv_dim(), &mut seed, 0.02);
        let w_k = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim, cfg.kv_dim()],
            &w_k_host,
        )
        .expect("upload w_k");

        let w_v_host = random_bf16(cfg.hidden_dim * cfg.kv_dim(), &mut seed, 0.02);
        let w_v = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim, cfg.kv_dim()],
            &w_v_host,
        )
        .expect("upload w_v");

        let w_o_host = random_bf16(cfg.q_dim() * cfg.hidden_dim, &mut seed, 0.02);
        let w_o = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.q_dim(), cfg.hidden_dim],
            &w_o_host,
        )
        .expect("upload w_o");

        // Per-head Q/K norms (shape `[head_dim]`) and post-attn norm
        // (shape `[hidden_dim]`). All three were added in Phase 4 along
        // with `w_q_gate`. Synthesize small-amplitude positive weights
        // the same way `attn_norm` is built — RMSNorm blows up on
        // negative weights feeding into bf16, and the smoke path just
        // needs "forward doesn't NaN/Inf".
        let attn_q_norm_host: Vec<f32> = (0..cfg.head_dim)
            .map(|_| lcg_iter(&mut seed).abs() * 0.5 + 0.5)
            .collect();
        let attn_q_norm =
            CudaTensor::<f32>::from_host(dev.clone(), vec![cfg.head_dim], &attn_q_norm_host)
                .expect("upload attn_q_norm");

        let attn_k_norm_host: Vec<f32> = (0..cfg.head_dim)
            .map(|_| lcg_iter(&mut seed).abs() * 0.5 + 0.5)
            .collect();
        let attn_k_norm =
            CudaTensor::<f32>::from_host(dev.clone(), vec![cfg.head_dim], &attn_k_norm_host)
                .expect("upload attn_k_norm");

        let post_attn_norm_host: Vec<f32> = (0..cfg.hidden_dim)
            .map(|_| lcg_iter(&mut seed).abs() * 0.5 + 0.5)
            .collect();
        let post_attn_norm =
            CudaTensor::<f32>::from_host(dev.clone(), vec![cfg.hidden_dim], &post_attn_norm_host)
                .expect("upload post_attn_norm");

        let w_q_gate_host = random_bf16(cfg.hidden_dim * cfg.q_dim(), &mut seed, 0.02);
        let w_q_gate = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim, cfg.q_dim()],
            &w_q_gate_host,
        )
        .expect("upload w_q_gate");

        let layer = Qwen35FullAttention {
            attn_norm,
            attn_q_norm,
            attn_k_norm,
            post_attn_norm,
            w_q,
            w_q_gate,
            w_k,
            w_v,
            w_o,
            config: cfg,
            layer_idx: 0,
        };

        let mut kv_cache = KvCache::new(dev.clone(), 1, max_ctx, cfg.n_kv_heads, cfg.head_dim)
            .expect("alloc kv cache");
        assert_eq!(kv_cache.n_filled(), 0);

        // Activation. Small amplitude so RMSNorm's `sqrt(mean_sq + eps)`
        // doesn't overflow bf16 in the re-cast on the way back.
        let hidden_host = random_bf16(n_tokens * cfg.hidden_dim, &mut seed, 0.25);
        let mut hidden = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![n_tokens, cfg.hidden_dim],
            &hidden_host,
        )
        .expect("upload hidden");
        let expected_shape = [n_tokens, cfg.hidden_dim];

        // Positions: monotonic 1, 2, …, n_tokens on all 4 MRoPE axes.
        let mut positions_host = vec![0i32; 4 * n_tokens];
        for t in 0..n_tokens {
            let pos = (t + 1) as i32;
            positions_host[t] = pos;
            positions_host[n_tokens + t] = pos;
            positions_host[2 * n_tokens + t] = pos;
            positions_host[3 * n_tokens + t] = 0;
        }
        let positions = CudaTensor::<i32>::from_host(
            dev.clone(),
            vec![4, n_tokens],
            &positions_host,
        )
        .expect("upload positions");

        layer
            .forward(&dev, &mut hidden, &positions, &mut kv_cache)
            .expect("forward");
        dev.synchronize().expect("synchronize");

        // ── Assertions.
        assert_eq!(kv_cache.n_filled(), n_tokens,
            "n_filled should advance by n_tokens after one forward");
        assert_eq!(hidden.shape(), expected_shape,
            "hidden shape preserved by forward");

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
            "qwen35 full_attn smoke: shape={:?} n_filled={} n_nan={} n_inf={} max_abs={:.4e}",
            hidden.shape(),
            kv_cache.n_filled(),
            n_nan,
            n_inf,
            max_abs
        );
        assert_eq!(n_nan, 0, "found {} NaN values in output", n_nan);
        assert_eq!(n_inf, 0, "found {} Inf values in output", n_inf);
    }
}
