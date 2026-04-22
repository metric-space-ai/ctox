//! DFlash block-diffusion draft model for Qwen3.5-27B speculative decoding.
//!
//! # What this is
//!
//! A 5-layer Qwen-shaped transformer whose only job is to propose 16
//! tokens at a time for the full 27B target to verify in a single
//! batched forward. The draft is **not** a standalone language model:
//! it consumes a concatenation of the target's last five hidden
//! states (`target_hidden_cat [ctx_len, 5*hidden]`) plus a fresh noise
//! embedding sequence (`noise_embed [16, hidden]`), fuses them via a
//! per-call `target_feat = rms_norm(target_hidden_cat @ fc^T)` (where
//! `fc` ships in HF layout `[hidden, 5*hidden]` and is transposed at
//! load so the matmul sees `[5*hidden, hidden]`), then runs non-causal
//! attention with K/V drawn from **both** the fused target features
//! and the in-step noise sequence. The output is projected through
//! the target's own `lm_head` — shared, not owned here — to emit
//! 16 vocab logits in a single step.
//!
//! Porting source: `dflash-ref/dflash/src/qwen3_dflash_graph.cpp`
//! `build_draft_graph(...)`. Safetensors layout mirrors the
//! reference's `DraftWeights` (58 tensors total, all bf16).
//!
//! # Why this enables speculative decoding
//!
//! Target Qwen3.5-27B at Q4_K_M is ~16 GB of weights. At A6000's
//! 768 GB/s memory bandwidth, a naive single-token decode is hard
//! capped at ~48 tok/s (weights-only, before KV). Our measured
//! bare-metal single-token decode is 52 tok/s at 1024 context —
//! essentially at the memory wall. The FFI reference sustains
//! 100 tok/s because each target forward verifies ~16 draft tokens
//! at once, amortizing the 16 GB weight scan across ~14 committed
//! tokens. That is the entire speed gap.
//!
//! This module implements **the draft side only**. The chain/DDTree
//! verify loop that binds draft + target lives in
//! `target::SpeculativeDecoder` (next commit); the FA layer's
//! existing batched forward path already accepts `n_tokens > 1`
//! which is what verify needs.
//!
//! # Weight layout & dtype at load time
//!
//! Two normalizations happen as the safetensors bundle is uploaded:
//!
//! * **Projection weights** ship in HF `nn.Linear` layout
//!   `[out_features, in_features]`, but `PackedWeight::Bf16` and the
//!   shared `launch_matmul_bf16_f32` wrapper expect `[k=in, n=out]`
//!   row-major. Every projection gets a device-side
//!   `launch_transpose_2d_bf16` during load to flip the orientation
//!   once. This means `w_q.shape() == [hidden, q_dim]` on device even
//!   though the tensor file has `[q_dim, hidden]`.
//!
//! * **RMSNorm gain vectors** arrive as bf16 in the bundle but
//!   `launch_rmsnorm_f32` consumes f32 weights. Rather than cast on
//!   every layer call (5 layers × 4 norms = 20 wasted casts per
//!   forward), each gain gets a one-shot bf16→f32 cast at load and
//!   is stored as `CudaTensor<f32>`.

#![cfg(feature = "cuda")]

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use half::bf16;
use memmap2::Mmap;
use safetensors::SafeTensors;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use crate::kernels::{
    launch_cast_bf16_to_f32, launch_cast_f32_to_bf16, launch_fill_const_f32,
    launch_head_gather_bf16, launch_head_scatter_bf16, launch_matmul_bf16_bf16,
    launch_matmul_bf16_f32, launch_residual_add_bf16, launch_rmsnorm_f32,
    launch_rope_neox_bf16_inplace, launch_scale_add_with_bias_f32, launch_silu_mul_bf16,
    launch_softmax_f32, launch_transpose_2d_bf16,
};
use crate::layers::packed_weight::PackedWeight;

/// All shape constants of the shipping draft match the target's
/// full-attention-layer dimensions. They are **fixed** for the
/// Qwen3.5-27B draft — the reference hard-codes them under
/// `DFLASH27B_DRAFT_*` and `DFLASH27B_TARGET_*` constants, so we
/// mirror that rather than deriving from a JSON config (the
/// shipped `.safetensors` has no JSON sidecar).
#[derive(Debug, Clone, Copy)]
pub struct DraftConfig {
    /// Hidden dimension. Matches the target's hidden so the
    /// draft's output can feed the target's `lm_head` directly.
    pub hidden: usize,
    /// Q projection dim = `n_head * head_dim`.
    pub q_dim: usize,
    /// K/V projection dim = `n_kv_heads * head_dim`.
    pub kv_dim: usize,
    /// SwiGLU intermediate dim.
    pub intermediate: usize,
    /// Number of attention heads.
    pub n_head: usize,
    /// Number of KV heads (GQA — `n_head % n_kv_heads == 0`).
    pub n_kv_heads: usize,
    /// Per-head dimension.
    pub head_dim: usize,
    /// Number of decoder layers.
    pub n_layers: usize,
    /// Block-diffusion proposal length — the draft predicts this many
    /// tokens per forward. Matches `DFLASH27B_DRAFT_BLOCK_SIZE`.
    pub block_size: usize,
    /// Number of target hidden layers that get concatenated into
    /// `target_hidden_cat` as the draft's cross-attention feature
    /// source. Matches `DFLASH27B_DRAFT_HIDDEN_LAYERS`.
    pub target_hidden_layers: usize,
    /// RMSNorm epsilon (shared with target).
    pub rms_eps: f32,
    /// RoPE base (theta). The reference uses `10_000_000.0` for
    /// Qwen3.5; RoPE mode is NEOX-style.
    pub rope_base: f32,
}

impl DraftConfig {
    /// Canonical config for the shipping Qwen3.5-27B draft.
    pub const fn qwen35_27b() -> Self {
        Self {
            hidden: 5120,
            q_dim: 4096,
            kv_dim: 1024,
            intermediate: 17408,
            n_head: 32,
            n_kv_heads: 8,
            head_dim: 128,
            n_layers: 5,
            block_size: 16,
            target_hidden_layers: 5,
            rms_eps: 1e-6,
            rope_base: 10_000_000.0,
        }
    }

    /// GQA group factor (Q heads per KV head). Used by the flash-attn
    /// launcher.
    #[inline]
    pub fn gqa_group(&self) -> usize {
        self.n_head / self.n_kv_heads
    }
}

/// Owned per-layer weight tensors. Projection weights are wrapped in
/// `PackedWeight::Bf16` with the logical `[k=in, n=out]` shape on
/// device (transposed from the safetensors' HF `[out, in]` orientation
/// at load time). Norm gain vectors are stored as `CudaTensor<f32>` so
/// `launch_rmsnorm_f32` can consume them without a per-call cast.
///
/// The forward path calls `PackedWeight::matmul_f32` by reference — no
/// cloning or per-layer reconstruction on the hot path.
pub struct DraftLayer {
    /// Pre-attention RMSNorm gain. Shape `[hidden]` f32.
    pub attn_norm: CudaTensor<f32>,
    /// Q projection. Logical `[hidden, q_dim]` bf16.
    pub w_q: PackedWeight,
    /// K projection. Logical `[hidden, kv_dim]` bf16.
    pub w_k: PackedWeight,
    /// V projection. Logical `[hidden, kv_dim]` bf16.
    pub w_v: PackedWeight,
    /// O projection. Logical `[q_dim, hidden]` bf16.
    pub w_o: PackedWeight,
    /// Per-head Q RMSNorm gain. Shape `[head_dim]` f32.
    pub q_norm: CudaTensor<f32>,
    /// Per-head K RMSNorm gain. Shape `[head_dim]` f32.
    pub k_norm: CudaTensor<f32>,
    /// Pre-FFN RMSNorm gain. Shape `[hidden]` f32.
    pub ffn_norm: CudaTensor<f32>,
    /// Gate projection (SwiGLU). Logical `[hidden, intermediate]` bf16.
    pub w_gate: PackedWeight,
    /// Up projection. Logical `[hidden, intermediate]` bf16.
    pub w_up: PackedWeight,
    /// Down projection. Logical `[intermediate, hidden]` bf16.
    pub w_down: PackedWeight,
}

/// Owned top-level draft weights (layers + shared feature fusion).
pub struct DraftWeights {
    pub config: DraftConfig,
    pub layers: Vec<DraftLayer>,
    /// Feature un-packer. Logical `[5*hidden, hidden]` bf16 on device
    /// (transposed from the safetensors' HF `[hidden, 5*hidden]`).
    /// Applied as `target_feat = target_hidden_cat @ fc` →
    /// `[ctx_len, hidden]`, then rms-normalized by `hidden_norm`.
    pub fc: PackedWeight,
    /// RMS gain applied to `target_feat` after `fc`. Shape `[hidden]` f32.
    pub hidden_norm: CudaTensor<f32>,
    /// Final RMS gain applied to the output hidden state before
    /// handing it to `lm_head`. Shape `[hidden]` f32.
    pub out_norm: CudaTensor<f32>,
}

impl DraftWeights {
    /// Load the 58 tensors of the reference draft `.safetensors` into
    /// device memory. `path` points at the single-file model bundle
    /// (`dflash-ref/dflash/models/draft/model.safetensors`).
    ///
    /// The loader is strict: every expected tensor must be present
    /// with an exactly-matching shape. Any mismatch surfaces with a
    /// message identifying the tensor so a mis-downloaded / truncated
    /// bundle fails fast.
    pub fn load_safetensors(
        device: Arc<DeviceContext>,
        path: impl AsRef<Path>,
        config: DraftConfig,
    ) -> Result<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)
            .with_context(|| format!("draft: open {}", path.display()))?;
        // SAFETY: memmap2 is safe for read-only files that are not
        // concurrently truncated. The model file is user-supplied and
        // not modified during inference.
        let mmap = unsafe { Mmap::map(&file) }
            .with_context(|| format!("draft: mmap {}", path.display()))?;
        let tensors = SafeTensors::deserialize(&mmap)
            .with_context(|| format!("draft: parse safetensors {}", path.display()))?;

        // Helper: fetch a named tensor, validate shape, upload as bf16
        // to the device. Accepts `[a, b]` or `[a]` shapes. The
        // safetensors layout is row-major with the FIRST dim as the
        // "rows" axis — matches our CudaTensor convention.
        let load_bf16 = |name: &str, expected: &[usize]| -> Result<CudaTensor<bf16>> {
            let view = tensors
                .tensor(name)
                .with_context(|| format!("draft: missing tensor {name}"))?;
            if view.dtype() != safetensors::Dtype::BF16 {
                return Err(anyhow!(
                    "draft: tensor {name} is {:?}, expected BF16",
                    view.dtype()
                ));
            }
            if view.shape() != expected {
                return Err(anyhow!(
                    "draft: tensor {name} shape {:?} != expected {:?}",
                    view.shape(),
                    expected
                ));
            }
            // view.data() is a &[u8] aligned to the mmap page. Reinterpret
            // as &[bf16] via bytemuck — bf16 is repr(transparent) over u16.
            let raw: &[u8] = view.data();
            let numel: usize = expected.iter().product();
            let bytes_needed = numel * std::mem::size_of::<bf16>();
            if raw.len() != bytes_needed {
                return Err(anyhow!(
                    "draft: tensor {name} bytes {} != numel*2 = {}",
                    raw.len(),
                    bytes_needed
                ));
            }
            let half_slice: &[bf16] = bytemuck::cast_slice(raw);
            CudaTensor::<bf16>::from_host(device.clone(), expected.to_vec(), half_slice)
                .with_context(|| format!("draft: upload {name}"))
        };

        // Load an HF projection weight `[out=rows, in=cols]` and
        // transpose it on-device to `[in=k, n=out]` = `[cols, rows]`
        // row-major, wrapping the result as `PackedWeight::Bf16` with
        // `k=cols` (in) and `n=rows` (out). The transpose runs once
        // at load and the resulting weight feeds every forward without
        // further shuffling.
        let load_proj = |name: &str, rows: usize, cols: usize| -> Result<PackedWeight> {
            let src = load_bf16(name, &[rows, cols])?;
            let mut dst = CudaTensor::<bf16>::zeros(device.clone(), vec![cols, rows])
                .with_context(|| format!("draft: alloc transposed dst {name}"))?;
            launch_transpose_2d_bf16(&device, &src, &mut dst, rows, cols)
                .with_context(|| format!("draft: transpose {name}"))?;
            Ok(PackedWeight::Bf16 {
                t: dst,
                k: cols,
                n: rows,
            })
        };

        // Load a norm gain (bf16 on disk) and cast it to f32 on-device
        // so `launch_rmsnorm_f32` can consume it without per-call
        // conversion.
        let load_f32_from_bf16 = |name: &str, dim: usize| -> Result<CudaTensor<f32>> {
            let src = load_bf16(name, &[dim])?;
            let mut dst = CudaTensor::<f32>::zeros(device.clone(), vec![dim])
                .with_context(|| format!("draft: alloc f32 dst {name}"))?;
            launch_cast_bf16_to_f32(&device, &src, &mut dst)
                .with_context(|| format!("draft: cast {name} bf16→f32"))?;
            Ok(dst)
        };

        // Top-level shared tensors.
        //
        // `fc` ships as `[hidden, 5*hidden]` in HF row-major; we want
        // `[5*hidden, hidden]` so the feature-fusion matmul reads
        // `target_hidden_cat[ctx_len, 5*hidden] @ fc[5*hidden, hidden]`
        // → `[ctx_len, hidden]`.
        let fc = load_proj(
            "fc.weight",
            config.hidden,
            config.target_hidden_layers * config.hidden,
        )?;
        let hidden_norm = load_f32_from_bf16("hidden_norm.weight", config.hidden)?;
        let out_norm = load_f32_from_bf16("norm.weight", config.hidden)?;

        // Per-layer tensors.
        let mut layers: Vec<DraftLayer> = Vec::with_capacity(config.n_layers);
        for il in 0..config.n_layers {
            let p = |suffix: &str| format!("layers.{il}.{suffix}");

            // Projections — HF `[out, in]` on disk, transposed to
            // `[in, out]` on device, wrapped as `PackedWeight::Bf16`.
            let w_q = load_proj(&p("self_attn.q_proj.weight"), config.q_dim, config.hidden)?;
            let w_k = load_proj(&p("self_attn.k_proj.weight"), config.kv_dim, config.hidden)?;
            let w_v = load_proj(&p("self_attn.v_proj.weight"), config.kv_dim, config.hidden)?;
            let w_o = load_proj(&p("self_attn.o_proj.weight"), config.hidden, config.q_dim)?;
            let q_norm = load_f32_from_bf16(&p("self_attn.q_norm.weight"), config.head_dim)?;
            let k_norm = load_f32_from_bf16(&p("self_attn.k_norm.weight"), config.head_dim)?;

            // Layer norms.
            let attn_norm = load_f32_from_bf16(&p("input_layernorm.weight"), config.hidden)?;
            let ffn_norm =
                load_f32_from_bf16(&p("post_attention_layernorm.weight"), config.hidden)?;

            // SwiGLU MLP projections.
            let w_gate =
                load_proj(&p("mlp.gate_proj.weight"), config.intermediate, config.hidden)?;
            let w_up =
                load_proj(&p("mlp.up_proj.weight"), config.intermediate, config.hidden)?;
            let w_down =
                load_proj(&p("mlp.down_proj.weight"), config.hidden, config.intermediate)?;

            layers.push(DraftLayer {
                attn_norm,
                w_q,
                w_k,
                w_v,
                w_o,
                q_norm,
                k_norm,
                ffn_norm,
                w_gate,
                w_up,
                w_down,
            });
        }

        tracing::info!(
            layers = config.n_layers,
            hidden = config.hidden,
            q_heads = config.n_head,
            kv_heads = config.n_kv_heads,
            block_size = config.block_size,
            "draft model loaded from {}",
            path.display()
        );

        Ok(DraftWeights {
            config,
            layers,
            fc,
            hidden_norm,
            out_norm,
        })
    }
}

/// Draft model handle — owns the weights and will eventually hold
/// per-forward scratch buffers (sized for `block_size × hidden` Q
/// and `ctx_len × hidden` cross-features).
pub struct DraftModel {
    pub weights: DraftWeights,
    pub device: Arc<DeviceContext>,
}

impl DraftModel {
    /// Load the draft bundle from a `.safetensors` path.
    pub fn load_from_safetensors(
        device: Arc<DeviceContext>,
        path: impl AsRef<Path>,
        config: DraftConfig,
    ) -> Result<Self> {
        let weights = DraftWeights::load_safetensors(device.clone(), path, config)?;
        Ok(Self { weights, device })
    }

    /// Config accessor.
    #[inline]
    pub fn config(&self) -> &DraftConfig {
        &self.weights.config
    }

    /// Draft forward — propose `block_size` tokens given the target's
    /// recent hidden-state history.
    ///
    /// # Inputs
    /// * `noise_embed` — `[block_size, hidden]` bf16. Seed sequence;
    ///   the reference embeds `[last_commit_tok, MASK, MASK, ..., MASK]`
    ///   through the target's `tok_embd` to produce this.
    /// * `target_hidden_cat` — `[ctx_len, target_hidden_layers * hidden]`
    ///   bf16. The last `target_hidden_layers` hidden states of the
    ///   target, concatenated along the feature axis per position.
    /// * `positions_q` — `[block_size]` i32.
    /// * `positions_k` — `[ctx_len + block_size]` i32.
    /// * `lm_head` — the target's LM head `[hidden, vocab]` bf16 (or
    ///   quantized — handed through the `PackedWeight` dispatch).
    ///
    /// # Output
    /// Logits of shape `[block_size, vocab]` f32 on device.
    ///
    /// # Algorithm
    ///
    /// Mirrors `dflash-ref::build_draft_graph`:
    ///
    ///   1. Feature fusion — project the concatenated target hidden
    ///      stack through `fc`, RMSNorm it, cast to bf16. Produces
    ///      `target_feat [ctx_len, hidden]`. This is the K/V source
    ///      for cross-attention and is reused across all layers; we
    ///      compute both the bf16 copy (for attention K/V inputs) and
    ///      the f32 copy (for the matmul into `w_k` / `w_v`) once and
    ///      reuse.
    ///   2. `h = noise_embed` (copied into a fresh bf16 buffer so the
    ///      caller's input isn't mutated; subsequent layers
    ///      write-through `h`).
    ///   3. Per-layer loop (5 layers):
    ///      a. RMSNorm h → hn
    ///      b. Q = hn @ w_q, reshape + per-head q_norm
    ///      c. K, V concat(target_feat, hn) → `[total_k, kv_dim]`,
    ///         reshape + per-head k_norm
    ///      d. NEOX RoPE in place on Q and K
    ///      e. Non-causal attention — per-head loop: transpose K,
    ///         matmul Q·Kᵀ in f32, scale by `1/√head_dim`, softmax
    ///         (no mask), cast to bf16, matmul probs·V in bf16,
    ///         scatter back into `attn_out [block_size, q_dim]`.
    ///      f. `h ← h + (attn_out @ w_o)` residual
    ///      g. RMSNorm + SwiGLU MLP: `h ← h + (w_down @ silu(w_gate)·w_up)`
    ///   4. Final RMSNorm on h → out_hidden f32.
    ///   5. logits = out_hidden @ lm_head, returned as f32.
    pub fn forward(
        &self,
        noise_embed: &CudaTensor<bf16>,
        target_hidden_cat: &CudaTensor<bf16>,
        positions_q: &CudaTensor<i32>,
        positions_k: &CudaTensor<i32>,
        lm_head: &PackedWeight,
    ) -> Result<CudaTensor<f32>> {
        let cfg = self.weights.config;
        let device = &self.device;
        let stream = device.raw().default_stream();

        // Shape validation. Everything downstream assumes these hold;
        // surfacing the mismatch here gives a clear error rather than
        // a cryptic kernel shape complaint three launches deep.
        let q_len = cfg.block_size;
        let hidden = cfg.hidden;
        let q_dim = cfg.q_dim;
        let kv_dim = cfg.kv_dim;
        let head_dim = cfg.head_dim;
        let n_head = cfg.n_head;
        let n_kv = cfg.n_kv_heads;
        let gqa = cfg.gqa_group();
        let target_feat_dim = cfg.target_hidden_layers * hidden;
        let eps = cfg.rms_eps;
        let theta = cfg.rope_base;

        if noise_embed.shape() != [q_len, hidden] {
            return Err(anyhow!(
                "draft.forward: noise_embed.shape {:?} != [{}, {}]",
                noise_embed.shape(),
                q_len,
                hidden
            ));
        }
        if target_hidden_cat.shape().len() != 2
            || target_hidden_cat.shape()[1] != target_feat_dim
        {
            return Err(anyhow!(
                "draft.forward: target_hidden_cat.shape {:?} != [ctx_len, {}]",
                target_hidden_cat.shape(),
                target_feat_dim
            ));
        }
        let ctx_len = target_hidden_cat.shape()[0];
        let total_k = ctx_len + q_len;
        if positions_q.numel() < q_len {
            return Err(anyhow!(
                "draft.forward: positions_q.numel()={} < block_size={}",
                positions_q.numel(),
                q_len
            ));
        }
        if positions_k.numel() < total_k {
            return Err(anyhow!(
                "draft.forward: positions_k.numel()={} < ctx_len+block_size={}",
                positions_k.numel(),
                total_k
            ));
        }
        let (lm_k, lm_n) = lm_head.dims();
        if lm_k != hidden {
            return Err(anyhow!(
                "draft.forward: lm_head.k={} != hidden={}",
                lm_k,
                hidden
            ));
        }
        let vocab = lm_n;

        // ─────────────────────────────────────────────────────────────
        // Step 1. Feature fusion
        //
        //   target_feat_f32 [ctx_len, hidden] = target_hidden_cat @ fc
        //   target_feat_f32 = rmsnorm(target_feat_f32, hidden_norm)
        //   target_feat_bf16 = cast(target_feat_f32)
        //
        // `fc` is stored on device as `[target_feat_dim, hidden]`
        // (transposed at load), so `PackedWeight::Bf16 { k=target_feat_dim,
        // n=hidden }` routes through cuBLAS with the right shapes.
        // ─────────────────────────────────────────────────────────────
        let target_hidden_cat_f32 = {
            let mut t = CudaTensor::<f32>::zeros(
                device.clone(),
                vec![ctx_len, target_feat_dim],
            )?;
            launch_cast_bf16_to_f32(device, target_hidden_cat, &mut t)?;
            t
        };
        let mut target_feat_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![ctx_len, hidden])?;
        self.weights
            .fc
            .matmul_f32(device, &target_hidden_cat_f32, &mut target_feat_f32)?;

        let mut target_feat_norm_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![ctx_len, hidden])?;
        launch_rmsnorm_f32(
            device,
            &target_feat_f32,
            &self.weights.hidden_norm,
            &mut target_feat_norm_f32,
            eps,
        )?;

        // Drop the pre-norm buffer (the rmsnorm output lives in
        // target_feat_norm_f32). The f32 side is what feeds each
        // layer's K / V projections; the reference also materializes
        // a bf16 copy for KV-side casts but we avoid that round-trip
        // by keeping target_feat in f32 and letting the
        // `PackedWeight::Bf16::matmul_f32` dispatch stage its own bf16
        // x-view per call.
        drop(target_feat_f32);
        let target_feat_f32 = target_feat_norm_f32;

        // ─────────────────────────────────────────────────────────────
        // Step 2. Initialize h_bf16 from noise_embed (copy — the caller
        // owns noise_embed and we must not mutate it).
        // ─────────────────────────────────────────────────────────────
        let mut h_bf16 = CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, hidden])?;
        stream
            .memcpy_dtod(noise_embed.buf(), h_bf16.buf_mut())
            .map_err(|e| anyhow!("draft.forward: noise_embed → h copy: {:?}", e))?;

        // ─────────────────────────────────────────────────────────────
        // Step 3. Layer loop.
        // ─────────────────────────────────────────────────────────────
        for il in 0..cfg.n_layers {
            let layer = &self.weights.layers[il];

            // ── 3a. Attn pre-norm. h_bf16 → h_f32 → hn_f32.
            let mut h_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, hidden])?;
            launch_cast_bf16_to_f32(device, &h_bf16, &mut h_f32)?;
            let mut hn_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, hidden])?;
            launch_rmsnorm_f32(device, &h_f32, &layer.attn_norm, &mut hn_f32, eps)?;

            // ── 3b. Q projection + per-head q_norm.
            //
            //   q_f32 [q_len, q_dim] = hn_f32 @ w_q
            //   reshape to [q_len * n_head, head_dim]
            //   rmsnorm(q, q_norm, eps) — weight has shape [head_dim]
            //   cast back to bf16 in [q_len, n_head, head_dim]
            let mut q_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, q_dim])?;
            layer.w_q.matmul_f32(device, &hn_f32, &mut q_f32)?;

            let q_reshape_in = q_f32.reshape(vec![q_len * n_head, head_dim])?;
            let mut q_normed_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len * n_head, head_dim])?;
            launch_rmsnorm_f32(
                device,
                &q_reshape_in,
                &layer.q_norm,
                &mut q_normed_f32,
                eps,
            )?;
            // Reshape back to [q_len, n_head, head_dim] for RoPE.
            let q_normed_f32 = q_normed_f32.reshape(vec![q_len, n_head, head_dim])?;
            let mut q_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, n_head, head_dim])?;
            launch_cast_f32_to_bf16(device, &q_normed_f32, &mut q_bf16)?;

            // ── 3c. K / V projections from `target_feat` AND `hn`,
            //        concatenated along the token axis into a single
            //        `[total_k, kv_dim]` K (same for V).
            //
            // Computing each branch separately avoids a large
            // concat-in-activation-space: the bf16 K/V for the attention
            // matmul are built from the two f32 halves in a combined
            // buffer, then cast + per-head-normed + RoPE'd as one.
            let mut k_ctx_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![ctx_len, kv_dim])?;
            layer
                .w_k
                .matmul_f32(device, &target_feat_f32, &mut k_ctx_f32)?;
            let mut k_new_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, kv_dim])?;
            layer.w_k.matmul_f32(device, &hn_f32, &mut k_new_f32)?;

            let mut v_ctx_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![ctx_len, kv_dim])?;
            layer
                .w_v
                .matmul_f32(device, &target_feat_f32, &mut v_ctx_f32)?;
            let mut v_new_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, kv_dim])?;
            layer.w_v.matmul_f32(device, &hn_f32, &mut v_new_f32)?;

            // Merge into combined [total_k, kv_dim] via two D2D
            // memcpys — the ctx rows occupy the leading
            // `ctx_len * kv_dim` elements, the new rows the tail.
            let mut k_combined_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![total_k, kv_dim])?;
            let mut v_combined_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![total_k, kv_dim])?;
            {
                let ctx_len_kv = ctx_len * kv_dim;
                let tail_len_kv = q_len * kv_dim;

                let k_ctx_src = k_ctx_f32.buf().slice(0..ctx_len_kv);
                let mut k_ctx_dst = k_combined_f32.buf_mut().slice_mut(0..ctx_len_kv);
                stream
                    .memcpy_dtod(&k_ctx_src, &mut k_ctx_dst)
                    .map_err(|e| anyhow!("draft.forward: K ctx concat: {:?}", e))?;

                let k_new_src = k_new_f32.buf().slice(0..tail_len_kv);
                let mut k_new_dst =
                    k_combined_f32.buf_mut().slice_mut(ctx_len_kv..ctx_len_kv + tail_len_kv);
                stream
                    .memcpy_dtod(&k_new_src, &mut k_new_dst)
                    .map_err(|e| anyhow!("draft.forward: K new concat: {:?}", e))?;

                let v_ctx_src = v_ctx_f32.buf().slice(0..ctx_len_kv);
                let mut v_ctx_dst = v_combined_f32.buf_mut().slice_mut(0..ctx_len_kv);
                stream
                    .memcpy_dtod(&v_ctx_src, &mut v_ctx_dst)
                    .map_err(|e| anyhow!("draft.forward: V ctx concat: {:?}", e))?;

                let v_new_src = v_new_f32.buf().slice(0..tail_len_kv);
                let mut v_new_dst =
                    v_combined_f32.buf_mut().slice_mut(ctx_len_kv..ctx_len_kv + tail_len_kv);
                stream
                    .memcpy_dtod(&v_new_src, &mut v_new_dst)
                    .map_err(|e| anyhow!("draft.forward: V new concat: {:?}", e))?;
            }
            drop(k_ctx_f32);
            drop(k_new_f32);
            drop(v_ctx_f32);
            drop(v_new_f32);

            // Per-head k_norm — reshape K to [total_k * n_kv, head_dim].
            let k_reshape_in = k_combined_f32.reshape(vec![total_k * n_kv, head_dim])?;
            let mut k_normed_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![total_k * n_kv, head_dim])?;
            launch_rmsnorm_f32(
                device,
                &k_reshape_in,
                &layer.k_norm,
                &mut k_normed_f32,
                eps,
            )?;
            let k_normed_f32 = k_normed_f32.reshape(vec![total_k, n_kv, head_dim])?;
            let mut k_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![total_k, n_kv, head_dim])?;
            launch_cast_f32_to_bf16(device, &k_normed_f32, &mut k_bf16)?;

            // V has no per-head norm; cast directly.
            let v_combined_f32 = v_combined_f32.reshape(vec![total_k, n_kv, head_dim])?;
            let mut v_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![total_k, n_kv, head_dim])?;
            launch_cast_f32_to_bf16(device, &v_combined_f32, &mut v_bf16)?;

            // ── 3d. NEOX RoPE on Q and K, in place.
            //
            // The reference applies RoPE to both sides but only with the
            // new-token positions for the q-side and the concatenated
            // positions for the k-side, which is what the caller-supplied
            // `positions_q` / `positions_k` encode.
            launch_rope_neox_bf16_inplace(device, &mut q_bf16, positions_q, head_dim, theta)?;
            launch_rope_neox_bf16_inplace(device, &mut k_bf16, positions_k, head_dim, theta)?;

            // ── 3e. Non-causal attention (naive per-head loop).
            //
            // `launch_flash_attn_bf16` only supports head_dim=256 for
            // the target's layer; the draft uses head_dim=128 so we
            // fall back to the correctness-first loop: gather per-head
            // Q/K/V, transpose K, matmul scores in f32, apply scale +
            // softmax (no mask), matmul probs·V in bf16, scatter into
            // attn_out. A head-fused kernel is a future optimization.
            //
            // TODO(SPEC.2c): add a 1/√d pre-scaled softmax kernel or a
            // head_dim≤128 flash-attention variant so we can drop the
            // per-head loop. Today's launcher list exposes neither.
            let scale = 1.0f32 / (head_dim as f32).sqrt();
            let mut attn_out_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, n_head, head_dim])?;

            // Per-head scratch that the inner loop reuses. The inner
            // loop writes into these in fixed shapes; we allocate them
            // once outside to cut the 32-head allocation count by 5×.
            let mut q_head_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, head_dim])?;
            let mut k_head_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![total_k, head_dim])?;
            let mut v_head_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![total_k, head_dim])?;
            let mut k_head_t_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![head_dim, total_k])?;
            let mut scores_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, total_k])?;
            let mut scores_scaled_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, total_k])?;
            let mut zero_bias_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, total_k])?;
            launch_fill_const_f32(device, &mut zero_bias_f32, 0.0f32)?;
            let mut probs_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, total_k])?;
            let mut probs_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, total_k])?;
            let mut out_head_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, head_dim])?;

            for q_head in 0..n_head {
                let kv_head = q_head / gqa;

                // Gather per-head stripes.
                launch_head_gather_bf16(
                    device,
                    &q_bf16,
                    &mut q_head_bf16,
                    q_len,
                    n_head,
                    head_dim,
                    q_head,
                )?;
                launch_head_gather_bf16(
                    device,
                    &k_bf16,
                    &mut k_head_bf16,
                    total_k,
                    n_kv,
                    head_dim,
                    kv_head,
                )?;
                launch_head_gather_bf16(
                    device,
                    &v_bf16,
                    &mut v_head_bf16,
                    total_k,
                    n_kv,
                    head_dim,
                    kv_head,
                )?;

                // Kᵀ [head_dim, total_k].
                launch_transpose_2d_bf16(
                    device,
                    &k_head_bf16,
                    &mut k_head_t_bf16,
                    total_k,
                    head_dim,
                )?;

                // Scores = Q · Kᵀ in f32. [q_len, head_dim] × [head_dim, total_k]
                launch_matmul_bf16_f32(
                    device,
                    &q_head_bf16,
                    &k_head_t_bf16,
                    &mut scores_f32,
                    q_len,
                    head_dim,
                    total_k,
                )?;

                // Scale by 1/sqrt(head_dim). We write into
                // `scores_scaled_f32 = scores * scale + 0` using the
                // scale-add-with-bias op against a zero bias. This is
                // one extra buffer but reuses the existing fused op;
                // no dedicated scale kernel exists at this list.
                launch_scale_add_with_bias_f32(
                    device,
                    &scores_f32,
                    &zero_bias_f32,
                    &mut scores_scaled_f32,
                    scale,
                )?;

                // Softmax (no mask — non-causal for the draft).
                launch_softmax_f32(device, &scores_scaled_f32, &mut probs_f32)?;

                // Cast probs to bf16 for the probs·V matmul.
                launch_cast_f32_to_bf16(device, &probs_f32, &mut probs_bf16)?;

                // out_head = probs @ V. [q_len, total_k] × [total_k, head_dim]
                launch_matmul_bf16_bf16(
                    device,
                    &probs_bf16,
                    &v_head_bf16,
                    &mut out_head_bf16,
                    q_len,
                    total_k,
                    head_dim,
                )?;

                // Scatter back into attn_out_bf16[:, q_head, :].
                launch_head_scatter_bf16(
                    device,
                    &out_head_bf16,
                    &mut attn_out_bf16,
                    q_len,
                    n_head,
                    head_dim,
                    q_head,
                )?;
            }

            // ── 3f. Output projection + residual.
            //
            //   attn_out_2d = reshape attn_out_bf16 to [q_len, q_dim]
            //   attn_out_f32 = cast to f32
            //   proj_f32 [q_len, hidden] = attn_out_f32 @ w_o
            //   proj_bf16 = cast to bf16
            //   h_new = residual_add(h_bf16, proj_bf16)
            //
            // We allocate `h_new` separately and swap at the end — the
            // residual_add kernel requires output ≠ either input.
            let attn_out_bf16 = attn_out_bf16.reshape(vec![q_len, q_dim])?;
            let mut attn_out_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, q_dim])?;
            launch_cast_bf16_to_f32(device, &attn_out_bf16, &mut attn_out_f32)?;

            let mut proj_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, hidden])?;
            layer.w_o.matmul_f32(device, &attn_out_f32, &mut proj_f32)?;

            let mut proj_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, hidden])?;
            launch_cast_f32_to_bf16(device, &proj_f32, &mut proj_bf16)?;

            let mut h_after_attn =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, hidden])?;
            launch_residual_add_bf16(device, &h_bf16, &proj_bf16, &mut h_after_attn)?;
            h_bf16 = h_after_attn;

            // ── 3g. FFN pre-norm.
            let mut h2_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, hidden])?;
            launch_cast_bf16_to_f32(device, &h_bf16, &mut h2_f32)?;
            let mut hf_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, hidden])?;
            launch_rmsnorm_f32(device, &h2_f32, &layer.ffn_norm, &mut hf_f32, eps)?;

            // ── 3h. SwiGLU.
            let mut gate_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, cfg.intermediate])?;
            layer.w_gate.matmul_f32(device, &hf_f32, &mut gate_f32)?;
            let mut up_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, cfg.intermediate])?;
            layer.w_up.matmul_f32(device, &hf_f32, &mut up_f32)?;

            let mut gate_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, cfg.intermediate])?;
            launch_cast_f32_to_bf16(device, &gate_f32, &mut gate_bf16)?;
            let mut up_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, cfg.intermediate])?;
            launch_cast_f32_to_bf16(device, &up_f32, &mut up_bf16)?;

            let mut gu_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, cfg.intermediate])?;
            launch_silu_mul_bf16(device, &gate_bf16, &up_bf16, &mut gu_bf16)?;

            let mut gu_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, cfg.intermediate])?;
            launch_cast_bf16_to_f32(device, &gu_bf16, &mut gu_f32)?;

            let mut ffn_out_f32 =
                CudaTensor::<f32>::zeros(device.clone(), vec![q_len, hidden])?;
            layer.w_down.matmul_f32(device, &gu_f32, &mut ffn_out_f32)?;

            let mut ffn_out_bf16 =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, hidden])?;
            launch_cast_f32_to_bf16(device, &ffn_out_f32, &mut ffn_out_bf16)?;

            let mut h_after_ffn =
                CudaTensor::<bf16>::zeros(device.clone(), vec![q_len, hidden])?;
            launch_residual_add_bf16(device, &h_bf16, &ffn_out_bf16, &mut h_after_ffn)?;
            h_bf16 = h_after_ffn;
        }

        // ─────────────────────────────────────────────────────────────
        // Step 4. Final RMSNorm + lm_head projection.
        // ─────────────────────────────────────────────────────────────
        let mut h_final_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, hidden])?;
        launch_cast_bf16_to_f32(device, &h_bf16, &mut h_final_f32)?;
        let mut out_hidden_f32 =
            CudaTensor::<f32>::zeros(device.clone(), vec![q_len, hidden])?;
        launch_rmsnorm_f32(
            device,
            &h_final_f32,
            &self.weights.out_norm,
            &mut out_hidden_f32,
            eps,
        )?;

        let mut logits_f32 = CudaTensor::<f32>::zeros(device.clone(), vec![q_len, vocab])?;
        lm_head.matmul_f32(device, &out_hidden_f32, &mut logits_f32)?;

        Ok(logits_f32)
    }
}

#[cfg(test)]
mod tests {
    //! Integration tests — only meaningful with the real draft bundle
    //! on disk. Gated behind an env var so `cargo test` stays green on
    //! machines without the fixture.

    use super::*;

    const DRAFT_PATH_ENV: &str = "CTOX_QWEN35_DRAFT_SAFETENSORS";

    fn draft_path() -> Option<std::path::PathBuf> {
        std::env::var_os(DRAFT_PATH_ENV).map(std::path::PathBuf::from)
    }

    #[test]
    #[ignore]
    fn draft_loads_clean() {
        let Some(path) = draft_path() else {
            eprintln!(
                "skipping: set {DRAFT_PATH_ENV} to the reference draft model.safetensors to run this test"
            );
            return;
        };

        let dev =
            Arc::new(DeviceContext::new(0).expect("cuda init for draft_loads_clean"));
        let cfg = DraftConfig::qwen35_27b();
        let model = DraftModel::load_from_safetensors(dev, &path, cfg)
            .expect("load draft safetensors");

        assert_eq!(model.weights.layers.len(), cfg.n_layers);
        // `fc` is stored transposed: on-disk `[hidden, 5*hidden]` → on-device
        // `[5*hidden, hidden]` so the feature-fusion matmul consumes it
        // as the `[k, n]` right-hand operand.
        assert_eq!(
            model.weights.fc.dims(),
            (cfg.target_hidden_layers * cfg.hidden, cfg.hidden)
        );
        assert_eq!(model.weights.hidden_norm.shape(), &[cfg.hidden]);
        assert_eq!(model.weights.out_norm.shape(), &[cfg.hidden]);
        for (il, layer) in model.weights.layers.iter().enumerate() {
            assert_eq!(layer.attn_norm.shape(), &[cfg.hidden], "layer {il}");
            // Projection weights are stored transposed from HF `[out, in]`
            // to `[in=k, out=n]` on device.
            assert_eq!(layer.w_q.dims(), (cfg.hidden, cfg.q_dim), "layer {il}");
            assert_eq!(layer.w_k.dims(), (cfg.hidden, cfg.kv_dim), "layer {il}");
            assert_eq!(layer.w_v.dims(), (cfg.hidden, cfg.kv_dim), "layer {il}");
            assert_eq!(layer.w_o.dims(), (cfg.q_dim, cfg.hidden), "layer {il}");
            assert_eq!(layer.q_norm.shape(), &[cfg.head_dim], "layer {il}");
            assert_eq!(layer.k_norm.shape(), &[cfg.head_dim], "layer {il}");
            assert_eq!(layer.ffn_norm.shape(), &[cfg.hidden], "layer {il}");
            assert_eq!(
                layer.w_gate.dims(),
                (cfg.hidden, cfg.intermediate),
                "layer {il}"
            );
            assert_eq!(
                layer.w_up.dims(),
                (cfg.hidden, cfg.intermediate),
                "layer {il}"
            );
            assert_eq!(
                layer.w_down.dims(),
                (cfg.intermediate, cfg.hidden),
                "layer {il}"
            );
        }

        eprintln!(
            "draft: loaded {} layers, hidden={}, block_size={}",
            cfg.n_layers, cfg.hidden, cfg.block_size
        );
    }
}
