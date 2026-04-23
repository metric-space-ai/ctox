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
use crate::layers::packed_weight::PackedWeight;

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
    /// `attn_q.weight`). `[hidden_dim, n_q_heads * head_dim]` with
    /// K=hidden_dim and N=q_dim. Carrier dtype is whatever the GGUF
    /// shipped; dispatch happens inside [`PackedWeight::matmul_f32`].
    pub w_q: PackedWeight,
    /// Q-side gate weight (second half of the GGUF's packed
    /// `attn_q.weight`). `[hidden_dim, n_q_heads * head_dim]`.
    /// Consumed by the sigmoid-gate branch after attention: `attn ←
    /// attn * sigmoid(norm @ w_q_gate)`.
    pub w_q_gate: PackedWeight,
    /// K projection weight. `[hidden_dim, n_kv_heads * head_dim]`.
    pub w_k: PackedWeight,
    /// V projection weight. `[hidden_dim, n_kv_heads * head_dim]`.
    pub w_v: PackedWeight,
    /// Output projection weight. `[n_q_heads * head_dim, hidden_dim]`.
    pub w_o: PackedWeight,
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
        if self.w_q_gate.dims() != (cfg.hidden_dim, cfg.q_dim()) {
            return Err(anyhow!(
                "qwen35 full_attn: w_q_gate dims {:?} != ({}, {})",
                self.w_q_gate.dims(),
                cfg.hidden_dim,
                cfg.q_dim()
            ));
        }
        if self.w_q.dims() != (cfg.hidden_dim, cfg.q_dim()) {
            return Err(anyhow!(
                "qwen35 full_attn: w_q dims {:?} != ({}, {})",
                self.w_q.dims(),
                cfg.hidden_dim,
                cfg.q_dim()
            ));
        }
        if self.w_k.dims() != (cfg.hidden_dim, cfg.kv_dim()) {
            return Err(anyhow!(
                "qwen35 full_attn: w_k dims {:?} != ({}, {})",
                self.w_k.dims(),
                cfg.hidden_dim,
                cfg.kv_dim()
            ));
        }
        if self.w_v.dims() != (cfg.hidden_dim, cfg.kv_dim()) {
            return Err(anyhow!(
                "qwen35 full_attn: w_v dims {:?} != ({}, {})",
                self.w_v.dims(),
                cfg.hidden_dim,
                cfg.kv_dim()
            ));
        }
        if self.w_o.dims() != (cfg.q_dim(), cfg.hidden_dim) {
            return Err(anyhow!(
                "qwen35 full_attn: w_o dims {:?} != ({}, {})",
                self.w_o.dims(),
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
        //
        //    `norm_f32` is now kept as the projection input (replacing
        //    the bf16 round-trip the first-port path did) so the
        //    PackedWeight dispatch can route to whichever of the mmvq
        //    or bf16 matmul kernels matches the on-device carrier.
        let mut hidden_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        kernels::launch_cast_bf16_to_f32(device, hidden, &mut hidden_f32)?;
        let mut norm_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        kernels::launch_rmsnorm_f32(device, &hidden_f32, &self.attn_norm, &mut norm_f32, cfg.rms_eps)?;

        // CTOX_DEBUG_FA_L2: dump per-stage L2 for the first FA layer
        // (layer_idx=0, i.e. model layer 3) on the LAST prompt step
        // (kv_cache.n_filled() == 127 going into this call, i.e. we are
        // processing position 127 which is the last prompt token). Gated
        // by the env var so normal runs are untouched.
        let dump_fa = std::env::var("CTOX_DEBUG_FA_L2").is_ok()
            && kv_cache.n_filled() == 127;
        let lidx = self.layer_idx;
        if dump_fa {
            fa_dbg_dump_bf16(&format!("FA[{}] 00_hidden_in", lidx), hidden, n_tokens, hidden_dim);
            fa_dbg_dump_f32 (&format!("FA[{}] 01_norm_f32",  lidx), &norm_f32, n_tokens, hidden_dim);
        }

        // ── 3. Q/K/V projections — f32·packed → f32, then cast to bf16
        //    for the per-head RMSNorm + RoPE path below. The
        //    `PackedWeight::matmul_f32` dispatch lives in
        //    `layers::packed_weight`; it picks the bf16 cuBLAS gemm for
        //    dense bf16 weights, per-row mmvq for Q*_K / Q8_0 packed
        //    bytes, or a zero memset for unloaded placeholders.
        let q_dim = cfg.q_dim();
        let kv_dim = cfg.kv_dim();

        let mut q_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, q_dim])?;
        self.w_q.matmul_f32(device, &norm_f32, &mut q_f32)?;
        let mut q_flat = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, q_dim])?;
        kernels::launch_cast_f32_to_bf16(device, &q_f32, &mut q_flat)?;

        let mut k_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, kv_dim])?;
        self.w_k.matmul_f32(device, &norm_f32, &mut k_f32)?;
        let mut k_flat = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, kv_dim])?;
        kernels::launch_cast_f32_to_bf16(device, &k_f32, &mut k_flat)?;

        let mut v_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, kv_dim])?;
        self.w_v.matmul_f32(device, &norm_f32, &mut v_f32)?;
        let mut v_flat = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, kv_dim])?;
        kernels::launch_cast_f32_to_bf16(device, &v_f32, &mut v_flat)?;


        // ── 3b. Attention-gate projection. `w_q_gate` is the gate
        //    matrix; the result is fed through a sigmoid and
        //    multiplied into the attention output at step 7g. Both
        //    the sigmoid and the subsequent mul happen in a single
        //    fused `launch_sigmoid_mul_bf16` kernel later — we just
        //    stage the raw gate projection in `q_gate` here.
        let mut q_gate_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, q_dim])?;
        self.w_q_gate.matmul_f32(device, &norm_f32, &mut q_gate_f32)?;
        let mut q_gate = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, q_dim])?;
        kernels::launch_cast_f32_to_bf16(device, &q_gate_f32, &mut q_gate)?;

        if dump_fa {
            fa_dbg_dump_f32(&format!("FA[{}] 02_q_f32_pre_qnorm", lidx), &q_f32, n_tokens, q_dim);
            fa_dbg_dump_f32(&format!("FA[{}] 03_q_gate_pre_sigmoid", lidx), &q_gate_f32, n_tokens, q_dim);
            if let Ok(host) = q_f32.to_host() {
                let last = &host[(n_tokens - 1) * q_dim..n_tokens * q_dim];
                let mut ranked: Vec<(usize, f32)> =
                    last.iter().enumerate().map(|(i, &v)| (i, v.abs())).collect();
                ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let top: Vec<String> = ranked.iter().take(5)
                    .map(|(i, _)| format!("ch{}={:.3}", i, last[*i]))
                    .collect();
                let neg = last.iter().filter(|&&v| v < 0.0).count();
                eprintln!(
                    "FA_DBG FA[{}] 02_q_f32_pre_qnorm TOP5 {} neg={}",
                    lidx, top.join(" "), neg
                );
            }
            if let Ok(host) = q_gate_f32.to_host() {
                let last = &host[(n_tokens - 1) * q_dim..n_tokens * q_dim];
                let mut ranked: Vec<(usize, f32)> =
                    last.iter().enumerate().map(|(i, &v)| (i, v.abs())).collect();
                ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let top: Vec<String> = ranked.iter().take(5)
                    .map(|(i, _)| format!("ch{}={:.3}", i, last[*i]))
                    .collect();
                eprintln!("FA_DBG FA[{}] 03_q_gate_pre_sigmoid TOP5 {}", lidx, top.join(" "));
                // Sign stats: how many are negative?
                let neg = last.iter().filter(|&&v| v < 0.0).count();
                let pos = last.iter().filter(|&&v| v > 0.0).count();
                let mean = last.iter().sum::<f32>() / last.len() as f32;
                eprintln!(
                    "FA_DBG FA[{}] 03_q_gate_pre_sigmoid neg={} pos={} mean={:.4}",
                    lidx, neg, pos, mean
                );
            }
        }


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
        // MRoPE rotates the first `cfg.rope_dim` dims per head (64 on
        // 27B out of head_dim=256). Using head_dim here rotates the
        // whole head and garbles every position-dependent attention
        // score. Sections come from the GGUF's
        // `qwen35.rope.dimension_sections` (`[11,11,10,0]` on 27B).
        kernels::launch_rope_mrope_bf16(
            device,
            &mut q3,
            positions,
            cfg.rope_theta,
            cfg.rope_dim as i32,
            cfg.rope_sections,
        )?;
        kernels::launch_rope_mrope_bf16(
            device,
            &mut k3,
            positions,
            cfg.rope_theta,
            cfg.rope_dim as i32,
            cfg.rope_sections,
        )?;


        // ── 6. Write K/V into the KV cache. Order matters: append both
        //    at the current offset, then advance.
        let v3 = reshape_3d(v_flat, n_tokens, cfg.n_kv_heads, cfg.head_dim)?;
        kv_cache.append_k(self.layer_idx, prompt_start, &k3)?;
        kv_cache.append_v(self.layer_idx, prompt_start, &v3)?;
        kv_cache.advance(n_tokens);

        let kv_len = kv_cache.n_filled();

        // ── 7. Fused attention via flash-attn.
        //
        //    The previous port walked each Q-head in a Python-style
        //    loop: gather q-stripe, gather k/v-stripes from the KV
        //    slab, transpose k, matmul scores, scale+mask, softmax,
        //    cast back to bf16, matmul with v, scatter — 10+ kernel
        //    launches per head × 24 heads per layer × 16 FA layers =
        //    roughly 4k kernel launches per forward just for the
        //    attention math, on top of the per-head gather/scatter
        //    memcpys killed in the A2 head-gather commit.
        //
        //    This replaces the whole head loop with a single
        //    `launch_flash_attn_bf16_kv_slab` call. The KV slab's
        //    `[max_ctx, n_kv_heads, head_dim]` layout is exactly
        //    what flash-attn expects — no per-head gather, no
        //    staged transpose, no intermediate f32 scores buffer.
        //    Causal masking is handled inside the kernel
        //    (`causal=true`): `q_abs = (kv_len - n_tokens) + t`
        //    which equals `prompt_start + t` for our chunked prefill
        //    layout, matching the mask the old `build_causal_mask`
        //    was constructing on the host.
        //
        //    Output is `[n_tokens, n_q_heads, head_dim]` bf16;
        //    relabeled as `[n_tokens, q_dim]` for the downstream
        //    gate-mul and output projection via `CudaTensor::reshape`
        //    (no device work; row-major contiguous storage makes it
        //    a label swap).
        let scale = 1.0f32 / (cfg.head_dim as f32).sqrt();
        let _ = prompt_start; // Causal inside the kernel handles this.
        let mut attn_out_3d = CudaTensor::<bf16>::zeros(
            device.clone(),
            vec![n_tokens, cfg.n_q_heads, cfg.head_dim],
        )?;
        kernels::launch_flash_attn_bf16_kv_slab(
            device,
            &q3,
            kv_cache.k_slab(self.layer_idx),
            kv_cache.v_slab(self.layer_idx),
            None,
            &mut attn_out_3d,
            kv_len,
            cfg.n_kv_heads,
            scale,
            cfg.gqa_group(),
            /* causal */ true,
        )?;
        let mut attn_out = attn_out_3d.reshape(vec![n_tokens, q_dim])?;

        if dump_fa {
            fa_dbg_dump_bf16(&format!("FA[{}] 10_attn_pre_gate", lidx), &attn_out, n_tokens, q_dim);
        }

        // ── 7g. Sigmoid gate elementwise-mul. dflash's
        //    `build_full_attn_block` applies `attn = attn * sigmoid(gate)`
        //    after the attention output is re-flattened into `[q_dim,
        //    n_tokens]`. Both `attn_out` and `q_gate` are
        //    `[n_tokens, q_dim]` row-major — fully elementwise. The
        //    `launch_sigmoid_mul_bf16` kernel fuses the sigmoid and
        //    the multiply into a single launch and, importantly,
        //    keeps everything on the device (the previous host-side
        //    sigmoid + mul blocked graph capture).
        kernels::launch_sigmoid_mul_bf16(device, &q_gate, &mut attn_out)?;

        if dump_fa {
            fa_dbg_dump_bf16(&format!("FA[{}] 12_attn_post_gate", lidx), &attn_out, n_tokens, q_dim);
            // Which INPUT channels to w_o are inflated? Top-5 |v| with
            // indices on the last-token row of `attn_post_gate`.
            if let Ok(host) = attn_out.to_host() {
                let last = &host[(n_tokens - 1) * q_dim..n_tokens * q_dim];
                let mut ranked: Vec<(usize, f32)> =
                    last.iter().enumerate().map(|(i, v)| (i, v.to_f32().abs())).collect();
                ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let top: Vec<String> = ranked.iter().take(5)
                    .map(|(i, v)| format!("ch{}={:.3}(raw={:.3})", i, v, last[*i].to_f32()))
                    .collect();
                eprintln!("FA_DBG FA[{}] 12_attn_post_gate TOP5 {}", lidx, top.join(" "));
            }
        }

        // ── 8. Output projection — `PackedWeight::matmul_f32` takes
        //    and produces f32, so cast `attn_out` (bf16) up to f32
        //    first, then cast the projected f32 result back to bf16 for
        //    the residual add.
        let mut attn_out_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, q_dim])?;
        kernels::launch_cast_bf16_to_f32(device, &attn_out, &mut attn_out_f32)?;
        let mut proj_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        self.w_o.matmul_f32(device, &attn_out_f32, &mut proj_f32)?;
        let mut proj = CudaTensor::<bf16>::zeros(device.clone(), vec![n_tokens, hidden_dim])?;
        kernels::launch_cast_f32_to_bf16(device, &proj_f32, &mut proj)?;

        if dump_fa {
            fa_dbg_dump_f32 (&format!("FA[{}] 13_proj_f32", lidx), &proj_f32, n_tokens, hidden_dim);
            fa_dbg_dump_bf16(&format!("FA[{}] 14_residual", lidx), &residual, n_tokens, hidden_dim);
            // Which output channels are inflated? Top-5 |v| with indices.
            if let Ok(host) = proj_f32.to_host() {
                let last = &host[(n_tokens - 1) * hidden_dim..n_tokens * hidden_dim];
                let mut ranked: Vec<(usize, f32)> =
                    last.iter().enumerate().map(|(i, &v)| (i, v.abs())).collect();
                ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let top: Vec<String> = ranked.iter().take(5)
                    .map(|(i, v)| format!("ch{}={:.2}(raw={:.2})", i, v, last[*i]))
                    .collect();
                eprintln!("FA_DBG FA[{}] 13_proj_f32 TOP5 {}", lidx, top.join(" "));
            }
        }

        // ── CTOX_L3_WO_CPU: CPU-reference w_o matmul for the last
        //    token row. Dequant Q4_K_M bytes off the device, do a
        //    naive f32 reduction, and compare channel-3994 against
        //    the GPU's `proj_f32`. Gated on layer_idx=0 (model L3)
        //    and the last prompt step so it runs exactly once per
        //    diagnostic session.
        if lidx == 0
            && kv_cache.n_filled() == 127
            && std::env::var("CTOX_L3_WO_CPU").is_ok()
        {
            cpu_ref_wo_probe(&attn_out_f32, &self.w_o, &proj_f32, n_tokens, q_dim, hidden_dim)?;
        }

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
/// Logical 3-D reshape of a bf16 activation buffer — no device work.
///
/// Uses `CudaTensor::reshape` to relabel the shape on the existing
/// backing buffer. Earlier revisions allocated a fresh tensor and
/// ran a `memcpy_dtod` for what is a pure label swap on row-major
/// contiguous storage. That's three per-forward allocations + three
/// D2D memcpys gone (one per Q/K/V projection × FA layer).
fn reshape_3d(src: CudaTensor<bf16>, d0: usize, d1: usize, d2: usize) -> Result<CudaTensor<bf16>> {
    src.reshape(vec![d0, d1, d2])
}


/// `scaled_masked ← scores * scale + mask`, host-loop fallback.
///
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

    // bf16 → f32 on the [n_tokens, n_heads*head_dim] layout. The
    // rmsnorm kernel expects a 2-D [rows, head_dim] f32, so we relabel
    // the shape on the same backing buffer (logical reshape; row-major
    // contiguous storage makes this a no-op). Same trick is applied to
    // the output on the way back. Replaces two D2D memcpys that were
    // shuffling the same bytes through a second allocation.
    let xf32_flat =
        CudaTensor::<f32>::zeros(device.clone(), vec![n_tokens, n_heads * head_dim])?;
    let mut xf32_flat = xf32_flat;
    kernels::launch_cast_bf16_to_f32(device, x, &mut xf32_flat)?;
    let xf32_2d = xf32_flat.reshape(vec![rows, head_dim])?;

    // RMSNorm in f32, per-row over head_dim.
    let mut yf32_2d = CudaTensor::<f32>::zeros(device.clone(), vec![rows, head_dim])?;
    kernels::launch_rmsnorm_f32(device, &xf32_2d, weight, &mut yf32_2d, eps)?;

    // Relabel back to [n_tokens, n_heads*head_dim] for the cast.
    let yf32 = yf32_2d.reshape(vec![n_tokens, n_heads * head_dim])?;

    // f32 → bf16 into x (in-place output of caller).
    kernels::launch_cast_f32_to_bf16(device, &yf32, x)?;
    Ok(())
}

/// Debug helper: download a bf16 tensor with logical shape [rows, cols]
/// row-major and dump last-row L2/absmax and first-5 values.
fn fa_dbg_dump_bf16(tag: &str, t: &CudaTensor<bf16>, rows: usize, cols: usize) {
    let host = match t.to_host() {
        Ok(h) => h,
        Err(_) => {
            eprintln!("FA_DBG {} <download-failed>", tag);
            return;
        }
    };
    if host.len() < rows * cols {
        eprintln!(
            "FA_DBG {} <host.len={} < rows*cols={}>",
            tag,
            host.len(),
            rows * cols
        );
        return;
    }
    let last = &host[(rows - 1) * cols..rows * cols];
    let mut sumsq = 0.0f64;
    let mut amax = 0.0f32;
    for &v in last {
        let f = v.to_f32();
        sumsq += (f as f64) * (f as f64);
        let a = f.abs();
        if a > amax {
            amax = a;
        }
    }
    let l2 = sumsq.sqrt() as f32;
    eprintln!(
        "FA_DBG {} rows={} cols={} l2={:.4e} amax={:.4e} row[0..5]={:.3e},{:.3e},{:.3e},{:.3e},{:.3e}",
        tag, rows, cols, l2, amax,
        last[0].to_f32(), last[1].to_f32(), last[2].to_f32(), last[3].to_f32(), last[4].to_f32()
    );
}

/// Debug helper: same as [`fa_dbg_dump_bf16`] but for f32 tensors.
fn fa_dbg_dump_f32(tag: &str, t: &CudaTensor<f32>, rows: usize, cols: usize) {
    let host = match t.to_host() {
        Ok(h) => h,
        Err(_) => {
            eprintln!("FA_DBG {} <download-failed>", tag);
            return;
        }
    };
    if host.len() < rows * cols {
        eprintln!(
            "FA_DBG {} <host.len={} < rows*cols={}>",
            tag,
            host.len(),
            rows * cols
        );
        return;
    }
    let last = &host[(rows - 1) * cols..rows * cols];
    let mut sumsq = 0.0f64;
    let mut amax = 0.0f32;
    for &v in last {
        sumsq += (v as f64) * (v as f64);
        let a = v.abs();
        if a > amax {
            amax = a;
        }
    }
    let l2 = sumsq.sqrt() as f32;
    eprintln!(
        "FA_DBG {} rows={} cols={} l2={:.4e} amax={:.4e} row[0..5]={:.3e},{:.3e},{:.3e},{:.3e},{:.3e}",
        tag, rows, cols, l2, amax,
        last[0], last[1], last[2], last[3], last[4]
    );
}

/// CPU-reference w_o matmul for the last-token row.
///
/// Downloads the f32 attention-post-gate input, the raw Q4_K_M bytes
/// for the `w_o` weight, dequants them on the host, and performs a
/// naive `hidden[j] = sum_i(x[i] * w[i, j])` reduction. Used by the
/// L3-FA ch-3994 probe — if CPU and GPU agree on the spike, the
/// weight row genuinely produces that output from this input; if they
/// disagree, we have a data-dependent mmvq kernel bug.
///
/// Only supports `PackedWeight::Q4K` (the shipping 27B format for
/// `attn_output.weight`). Other variants log a skip.
fn cpu_ref_wo_probe(
    attn_in_f32: &CudaTensor<f32>,
    w_o: &PackedWeight,
    gpu_proj_f32: &CudaTensor<f32>,
    n_tokens: usize,
    q_dim: usize,
    hidden_dim: usize,
) -> Result<()> {
    use crate::kernels::mmq_q4k::{dequant_q4k_block, Q4K_BLOCK_BYTES_PUB, Q4K_BLOCK_ELEMS_PUB};

    let (k, n) = w_o.dims();
    if k != q_dim || n != hidden_dim {
        eprintln!(
            "CPU_WO_PROBE: w_o dims ({},{}) != (q_dim,hidden) ({},{}), skipping",
            k, n, q_dim, hidden_dim
        );
        return Ok(());
    }

    let raw_bytes: Vec<u8> = match w_o {
        PackedWeight::Q4K { t, .. } => t
            .to_host()
            .map_err(|e| anyhow!("CPU_WO_PROBE: download w_o bytes: {:?}", e))?
            .into_iter()
            .map(|b| b as u8)
            .collect(),
        other => {
            let name = match other {
                PackedWeight::Bf16 { .. } => "Bf16",
                PackedWeight::Q4K { .. } => "Q4K",
                PackedWeight::Q5K { .. } => "Q5K",
                PackedWeight::Q6K { .. } => "Q6K",
                PackedWeight::Q8_0 { .. } => "Q8_0",
                PackedWeight::IQ4XS { .. } => "IQ4XS",
                PackedWeight::Zero { .. } => "Zero",
            };
            eprintln!(
                "CPU_WO_PROBE: w_o variant {} unsupported (Q4K only), skipping",
                name
            );
            return Ok(());
        }
    };

    let x_host = attn_in_f32
        .to_host()
        .map_err(|e| anyhow!("CPU_WO_PROBE: download attn_in_f32: {:?}", e))?;
    let x_last = &x_host[(n_tokens - 1) * q_dim..n_tokens * q_dim];

    let blocks_per_col = k / Q4K_BLOCK_ELEMS_PUB;
    let expected_bytes = blocks_per_col * n * Q4K_BLOCK_BYTES_PUB;
    if raw_bytes.len() < expected_bytes {
        return Err(anyhow!(
            "CPU_WO_PROBE: w_o bytes {} < expected {} (k={},n={})",
            raw_bytes.len(),
            expected_bytes,
            k,
            n
        ));
    }

    // CPU reduction: for each output column j, sum x[i] * w[i, j]
    // over i in [0, k). Layout: bytes[j * blocks_per_col * 144 + b *
    // 144 .. + 144] = Q4K block covering rows b*256..(b+1)*256 of col j.
    let mut cpu_proj = vec![0.0f32; n];
    let mut dq = [0.0f32; 256];
    for j in 0..n {
        let col_off = j * blocks_per_col * Q4K_BLOCK_BYTES_PUB;
        let mut acc = 0.0f64;
        for b in 0..blocks_per_col {
            let off = col_off + b * Q4K_BLOCK_BYTES_PUB;
            let mut block = [0u8; 144];
            block.copy_from_slice(&raw_bytes[off..off + Q4K_BLOCK_BYTES_PUB]);
            dequant_q4k_block(&block, &mut dq);
            let base = b * Q4K_BLOCK_ELEMS_PUB;
            for l in 0..Q4K_BLOCK_ELEMS_PUB {
                acc += (x_last[base + l] as f64) * (dq[l] as f64);
            }
        }
        cpu_proj[j] = acc as f32;
    }

    // Download GPU result for the same row.
    let gpu_host = gpu_proj_f32
        .to_host()
        .map_err(|e| anyhow!("CPU_WO_PROBE: download gpu proj_f32: {:?}", e))?;
    let gpu_last = &gpu_host[(n_tokens - 1) * n..n_tokens * n];

    // Log channel 3994 comparison + top-5 CPU channels + delta stats.
    let probe_ch = 3994usize.min(n - 1);
    let cpu_v = cpu_proj[probe_ch];
    let gpu_v = gpu_last[probe_ch];
    eprintln!(
        "CPU_WO_PROBE ch{}: cpu={:.4} gpu={:.4} delta={:.4}",
        probe_ch, cpu_v, gpu_v, gpu_v - cpu_v
    );

    // Top-5 CPU output channels (by |v|).
    let mut ranked_cpu: Vec<(usize, f32)> =
        cpu_proj.iter().enumerate().map(|(i, &v)| (i, v.abs())).collect();
    ranked_cpu.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let top_cpu: Vec<String> = ranked_cpu
        .iter()
        .take(5)
        .map(|(i, v)| format!("ch{}={:.2}(raw={:.2})", i, v, cpu_proj[*i]))
        .collect();
    eprintln!("CPU_WO_PROBE TOP5_CPU {}", top_cpu.join(" "));

    // Aggregate L2 + amax of the CPU reduction for direct comparison
    // with `fa_dbg_dump_f32 13_proj_f32`.
    let mut sumsq = 0.0f64;
    let mut amax = 0.0f32;
    for &v in &cpu_proj {
        sumsq += (v as f64) * (v as f64);
        let a = v.abs();
        if a > amax {
            amax = a;
        }
    }
    eprintln!(
        "CPU_WO_PROBE cpu_last l2={:.4e} amax={:.4e}",
        sumsq.sqrt() as f32,
        amax
    );

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

        // Weights are bf16 random tensors wrapped in
        // `PackedWeight::Bf16`. The forward path dispatches through
        // the Bf16 variant which stages x to bf16 + calls cuBLAS
        // bf16→f32 gemm — same numerical contract as the old
        // `launch_matmul_bf16_bf16` path (f32 accumulator), so the
        // smoke test's finiteness assertions remain valid.
        let w_q_host = random_bf16(cfg.hidden_dim * cfg.q_dim(), &mut seed, 0.02);
        let w_q_t = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim, cfg.q_dim()],
            &w_q_host,
        )
        .expect("upload w_q");
        let w_q = PackedWeight::Bf16 {
            t: w_q_t,
            k: cfg.hidden_dim,
            n: cfg.q_dim(),
        };

        let w_k_host = random_bf16(cfg.hidden_dim * cfg.kv_dim(), &mut seed, 0.02);
        let w_k_t = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim, cfg.kv_dim()],
            &w_k_host,
        )
        .expect("upload w_k");
        let w_k = PackedWeight::Bf16 {
            t: w_k_t,
            k: cfg.hidden_dim,
            n: cfg.kv_dim(),
        };

        let w_v_host = random_bf16(cfg.hidden_dim * cfg.kv_dim(), &mut seed, 0.02);
        let w_v_t = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim, cfg.kv_dim()],
            &w_v_host,
        )
        .expect("upload w_v");
        let w_v = PackedWeight::Bf16 {
            t: w_v_t,
            k: cfg.hidden_dim,
            n: cfg.kv_dim(),
        };

        let w_o_host = random_bf16(cfg.q_dim() * cfg.hidden_dim, &mut seed, 0.02);
        let w_o_t = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.q_dim(), cfg.hidden_dim],
            &w_o_host,
        )
        .expect("upload w_o");
        let w_o = PackedWeight::Bf16 {
            t: w_o_t,
            k: cfg.q_dim(),
            n: cfg.hidden_dim,
        };

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
        let w_q_gate_t = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![cfg.hidden_dim, cfg.q_dim()],
            &w_q_gate_host,
        )
        .expect("upload w_q_gate");
        let w_q_gate = PackedWeight::Bf16 {
            t: w_q_gate_t,
            k: cfg.hidden_dim,
            n: cfg.q_dim(),
        };

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
