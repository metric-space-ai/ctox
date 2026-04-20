//! DFlash block-diffusion draft forward pass.
//!
//! Mirrors `dflash/src/qwen3_dflash_graph.cpp::build_draft_graph` in
//! the reference implementation — same op order, same tensor shapes,
//! same non-causal semantics. The only algorithmic difference is that
//! candle's flash-attention path handles the `Q (q_len) × K,V
//! (ctx_len + q_len)` asymmetric layout via a pre-built attention bias,
//! not via ggml's concat+flash_attn_ext with a null mask — we compute
//! scaled-dot-product attention directly. Numerically identical.
//!
//! No KV cache. No quantisation. Draft stays BF16 on the target device
//! throughout; the whole model is 3.46 GB on VRAM.

use candle_core::{DType, Device, Module, Result, Tensor, D};
use candle_nn::{linear_no_bias, rms_norm, Embedding, Linear, RmsNorm, VarBuilder};

use super::config::DFlashDraftConfig;

/// One decoder block of the draft (Qwen3-style, full attention,
/// cross-attending to captured target features).
#[derive(Debug)]
struct DraftLayer {
    // Pre-attention RMSNorm applied to the noise hidden states.
    input_layernorm: RmsNorm,
    // Post-attention, pre-MLP RMSNorm applied to the residual stream.
    post_attention_layernorm: RmsNorm,

    // Attention projections — no bias (Qwen3 convention).
    q_proj: Linear, // hidden -> q_dim   (num_heads × head_dim)
    k_proj: Linear, // hidden -> kv_dim  (num_kv_heads × head_dim)
    v_proj: Linear, // hidden -> kv_dim
    o_proj: Linear, // q_dim  -> hidden

    // Per-head RMSNorm on Q and K (Qwen3 "qk_norm" variant). Weight
    // shape is `[head_dim]`.
    q_norm: RmsNorm,
    k_norm: RmsNorm,

    // SwiGLU MLP.
    gate_proj: Linear, // hidden -> intermediate
    up_proj: Linear,   // hidden -> intermediate
    down_proj: Linear, // intermediate -> hidden
}

impl DraftLayer {
    fn load(vb: VarBuilder, cfg: &DFlashDraftConfig) -> Result<Self> {
        let hidden = cfg.hidden_size;
        let inter = cfg.intermediate_size;
        let q_dim = cfg.q_dim();
        let kv_dim = cfg.kv_dim();
        let head_dim = cfg.head_dim;
        let eps = cfg.rms_norm_eps;

        let input_layernorm = rms_norm(hidden, eps, vb.pp("input_layernorm"))?;
        let post_attention_layernorm = rms_norm(hidden, eps, vb.pp("post_attention_layernorm"))?;

        let sa = vb.pp("self_attn");
        let q_proj = linear_no_bias(hidden, q_dim, sa.pp("q_proj"))?;
        let k_proj = linear_no_bias(hidden, kv_dim, sa.pp("k_proj"))?;
        let v_proj = linear_no_bias(hidden, kv_dim, sa.pp("v_proj"))?;
        let o_proj = linear_no_bias(q_dim, hidden, sa.pp("o_proj"))?;
        let q_norm = rms_norm(head_dim, eps, sa.pp("q_norm"))?;
        let k_norm = rms_norm(head_dim, eps, sa.pp("k_norm"))?;

        let mlp = vb.pp("mlp");
        let gate_proj = linear_no_bias(hidden, inter, mlp.pp("gate_proj"))?;
        let up_proj = linear_no_bias(hidden, inter, mlp.pp("up_proj"))?;
        let down_proj = linear_no_bias(inter, hidden, mlp.pp("down_proj"))?;

        Ok(Self {
            input_layernorm,
            post_attention_layernorm,
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            q_norm,
            k_norm,
            gate_proj,
            up_proj,
            down_proj,
        })
    }

    /// Forward for one block.
    ///
    /// - `h`: noise hidden states `[B, q_len, hidden]` (where q_len = block_size).
    /// - `target_feat`: captured target features `[B, ctx_len, hidden]` —
    ///   used as cross-attention KV context.
    /// - `cos_q`, `sin_q`: RoPE tables at positions `[ctx_len..ctx_len+q_len]`
    ///   with shape `[q_len, head_dim/2]`.
    /// - `cos_k`, `sin_k`: RoPE tables at positions `[0..ctx_len+q_len]`
    ///   with shape `[total_k, head_dim/2]`.
    /// - `cfg`: draft config (for head counts).
    ///
    /// Returns `[B, q_len, hidden]`.
    #[allow(clippy::too_many_arguments)]
    fn forward(
        &self,
        h: &Tensor,
        target_feat: &Tensor,
        cos_q: &Tensor,
        sin_q: &Tensor,
        cos_k: &Tensor,
        sin_k: &Tensor,
        cfg: &DFlashDraftConfig,
    ) -> Result<Tensor> {
        let (b, q_len, _) = h.dims3()?;
        let ctx_len = target_feat.dim(1)?;
        let total_k = ctx_len + q_len;
        let n_head = cfg.num_attention_heads;
        let n_kv = cfg.num_key_value_heads;
        let head_dim = cfg.head_dim;

        // ── Attention pre-norm on the noise stream.
        let hn = h.apply(&self.input_layernorm)?;

        // ── Q from noise only.  [B, q_len, q_dim]
        //    Reshape → per-head view [B, q_len, n_head, head_dim]
        //    q_norm per-head → reshape back to [B, n_head, q_len, head_dim]
        let mut q = hn
            .apply(&self.q_proj)?
            .reshape((b, q_len, n_head, head_dim))?;
        q = q.apply(&self.q_norm)?;
        let mut q = q.transpose(1, 2)?.contiguous()?; // [B, n_head, q_len, head_dim]

        // ── K and V: concatenate context (from target_feat) and noise.
        let k_ctx = target_feat.apply(&self.k_proj)?; // [B, ctx_len, kv_dim]
        let k_noi = hn.apply(&self.k_proj)?; // [B, q_len,   kv_dim]
        let v_ctx = target_feat.apply(&self.v_proj)?;
        let v_noi = hn.apply(&self.v_proj)?;

        let k_cat = Tensor::cat(&[&k_ctx, &k_noi], 1)?; // [B, total_k, kv_dim]
        let v_cat = Tensor::cat(&[&v_ctx, &v_noi], 1)?;

        // Per-head k_norm; v has no norm.
        let mut k = k_cat
            .reshape((b, total_k, n_kv, head_dim))?
            .apply(&self.k_norm)?
            .transpose(1, 2)?
            .contiguous()?; // [B, n_kv, total_k, head_dim]
        let v = v_cat
            .reshape((b, total_k, n_kv, head_dim))?
            .transpose(1, 2)?
            .contiguous()?; // [B, n_kv, total_k, head_dim]

        // ── NEOX-style RoPE on Q (positions [ctx_len..ctx_len+q_len]) and
        //    K (positions [0..total_k]). candle's `rope` applies the
        //    provided cos/sin table directly; we sliced the correct ranges
        //    outside and pass them in here.
        q = apply_rope(&q, cos_q, sin_q)?;
        k = apply_rope(&k, cos_k, sin_k)?;

        // ── GQA broadcast: replicate each kv head (n_head / n_kv) times.
        let k = repeat_kv(k, n_head / n_kv)?;
        let v = repeat_kv(v, n_head / n_kv)?;

        // ── Scaled dot-product attention, NON-CAUSAL (the draft sees the
        //    full context and the full block as one denoising step).
        let scale = 1.0_f64 / (head_dim as f64).sqrt();
        let attn = {
            let scores = q.matmul(&k.transpose(D::Minus2, D::Minus1)?)?;
            let scores = (scores * scale)?;
            candle_nn::ops::softmax_last_dim(&scores)?.matmul(&v)?
        }; // [B, n_head, q_len, head_dim]

        // ── Back to [B, q_len, q_dim] and output projection with residual.
        let attn = attn
            .transpose(1, 2)?
            .reshape((b, q_len, n_head * head_dim))?
            .apply(&self.o_proj)?;
        let h = (h + attn)?;

        // ── FFN pre-norm + SwiGLU MLP with residual.
        let hf = h.apply(&self.post_attention_layernorm)?;
        let gate = hf.apply(&self.gate_proj)?;
        let up = hf.apply(&self.up_proj)?;
        let ffn_in = (candle_nn::ops::silu(&gate)? * up)?;
        let ffn_out = ffn_in.apply(&self.down_proj)?;
        h + ffn_out
    }
}

/// Apply NEOX-style RoPE to a 4-D tensor `[B, heads, seq, head_dim]`
/// using pre-computed `cos`/`sin` tables of shape `[seq, head_dim/2]`.
/// candle's `rotary_emb::rope` wants 4-D input and broadcasts the
/// `(seq, head_dim/2)` table over batch and heads.
fn apply_rope(x: &Tensor, cos: &Tensor, sin: &Tensor) -> Result<Tensor> {
    candle_nn::rotary_emb::rope(&x.contiguous()?, cos, sin)
}

/// Grouped-query expansion: each of `n_kv` heads is replicated `n_rep`
/// times to align with `n_head = n_kv * n_rep`. No-op if `n_rep == 1`.
fn repeat_kv(x: Tensor, n_rep: usize) -> Result<Tensor> {
    if n_rep == 1 {
        return Ok(x);
    }
    let (b, n_kv, seq, head_dim) = x.dims4()?;
    x.unsqueeze(2)?
        .expand((b, n_kv, n_rep, seq, head_dim))?
        .reshape((b, n_kv * n_rep, seq, head_dim))
}

/// Top-level DFlash draft.
///
/// Loading note: the caller supplies the target's token embedding and
/// `lm_head` separately (see [`forward_with_lm_head`]). The draft
/// safetensors have **no** `embed_tokens.weight` or `lm_head.weight` —
/// that's the core space-saving trick of DFlash, and it also guarantees
/// the draft's logits live in the target's exact vocabulary for
/// rejection sampling.
pub struct DFlashDraftModel {
    cfg: DFlashDraftConfig,
    device: Device,
    dtype: DType,

    // Feature-fusion projection: maps 5-target-layers concatenated
    // hidden states down to `hidden_size` so the downstream attention
    // layers see a single unified context representation.
    fc: Linear,              // [fused_target_feature_dim -> hidden]
    hidden_norm: RmsNorm,    // RMSNorm(hidden)

    layers: Vec<DraftLayer>,
    final_norm: RmsNorm,

    // Pre-computed RoPE cos/sin tables covering up to
    // `max_position_embeddings`. Sliced per-call.
    cos: Tensor, // [max_positions, head_dim/2]
    sin: Tensor, // [max_positions, head_dim/2]
}

impl DFlashDraftModel {
    /// Load the draft from a [`VarBuilder`] rooted at the draft's
    /// safetensors (no `model.` prefix — the checkpoint has `fc`,
    /// `hidden_norm`, `layers.N.…`, `norm` directly at the root).
    pub fn load(vb: VarBuilder, cfg: DFlashDraftConfig) -> Result<Self> {
        cfg.validate().map_err(candle_core::Error::msg)?;

        let device = vb.device().clone();
        let dtype = vb.dtype();

        let fc = linear_no_bias(cfg.fused_target_feature_dim(), cfg.hidden_size, vb.pp("fc"))?;
        let hidden_norm = rms_norm(cfg.hidden_size, cfg.rms_norm_eps, vb.pp("hidden_norm"))?;

        let mut layers = Vec::with_capacity(cfg.num_hidden_layers);
        for i in 0..cfg.num_hidden_layers {
            layers.push(DraftLayer::load(vb.pp(format!("layers.{i}")), &cfg)?);
        }
        let final_norm = rms_norm(cfg.hidden_size, cfg.rms_norm_eps, vb.pp("norm"))?;

        // RoPE tables. `max_position_embeddings` in the config (262144
        // for Qwen3.5) is huge; we allocate only what we are likely to
        // use — 16k positions is plenty for the ctx_len+block use case
        // and far less of a startup allocation.
        let rope_span = cfg.max_position_embeddings.min(16_384);
        let (cos, sin) = build_rope_tables(
            cfg.rope_theta as f32,
            cfg.head_dim,
            rope_span,
            &device,
            dtype,
        )?;

        Ok(Self {
            cfg,
            device,
            dtype,
            fc,
            hidden_norm,
            layers,
            final_norm,
            cos,
            sin,
        })
    }

    /// Draft forward — the core per-step call the pipeline issues.
    ///
    /// Inputs:
    /// - `noise_embeds`: `[B, block_size, hidden]` — produced by running
    ///   the target's token embedding on `[last_target_tok, MASK × 15]`.
    /// - `target_hidden_cat`: `[B, ctx_len, target_layer_ids.len() × hidden]`
    ///   — the captured target features concatenated along the feature
    ///   dimension (not along the sequence dimension).
    ///
    /// Returns: hidden states `[B, block_size, hidden]` — the caller
    /// projects these through the target's `lm_head` to get draft
    /// logits. See [`Self::forward_with_lm_head`] for the convenience
    /// wrapper that does this in one call.
    pub fn forward_hidden(
        &self,
        noise_embeds: &Tensor,
        target_hidden_cat: &Tensor,
    ) -> Result<Tensor> {
        // ── Shape checks — fail cheap with clear messages before any
        //    matmul blows up.
        let (b, q_len, h_dim) = noise_embeds.dims3()?;
        if q_len != self.cfg.block_size {
            candle_core::bail!(
                "DFlashDraftModel: noise_embeds dim=1 is {q_len}, expected block_size={}",
                self.cfg.block_size
            );
        }
        if h_dim != self.cfg.hidden_size {
            candle_core::bail!(
                "DFlashDraftModel: noise_embeds dim=2 is {h_dim}, expected hidden_size={}",
                self.cfg.hidden_size
            );
        }
        let (b2, ctx_len, cat_dim) = target_hidden_cat.dims3()?;
        if b2 != b {
            candle_core::bail!(
                "DFlashDraftModel: batch mismatch noise_embeds={b} target_hidden_cat={b2}"
            );
        }
        if cat_dim != self.cfg.fused_target_feature_dim() {
            candle_core::bail!(
                "DFlashDraftModel: target_hidden_cat dim=2 is {cat_dim}, expected {}",
                self.cfg.fused_target_feature_dim()
            );
        }
        let total_k = ctx_len + q_len;
        let rope_span = self.cos.dim(0)?;
        if total_k > rope_span {
            candle_core::bail!(
                "DFlashDraftModel: ctx_len+block_size = {total_k} exceeds RoPE span {rope_span}"
            );
        }

        // ── Feature fusion: fc then hidden_norm. Result is the
        //    per-token "target feature" that attention uses as KV.
        let target_feat = target_hidden_cat
            .apply(&self.fc)?
            .apply(&self.hidden_norm)?;

        // ── Pre-slice RoPE tables for Q (positions [ctx_len..ctx_len+q_len])
        //    and K (positions [0..total_k]). Shared across all layers.
        let cos_q = self.cos.narrow(0, ctx_len, q_len)?;
        let sin_q = self.sin.narrow(0, ctx_len, q_len)?;
        let cos_k = self.cos.narrow(0, 0, total_k)?;
        let sin_k = self.sin.narrow(0, 0, total_k)?;

        // ── Decoder stack.
        let mut h = noise_embeds.contiguous()?;
        for layer in &self.layers {
            h = layer.forward(&h, &target_feat, &cos_q, &sin_q, &cos_k, &sin_k, &self.cfg)?;
        }
        h.apply(&self.final_norm)
    }

    /// Convenience wrapper: build the input embeddings from `input_ids`
    /// using the provided target `embed_tokens`, run [`forward_hidden`],
    /// then project through the provided target `lm_head` closure.
    ///
    /// The draft has no embedding / lm-head of its own — this is the
    /// whole point of the "shared lm_head" design. The two are passed
    /// in rather than held internally so the caller can share the
    /// target's live tensors (cheap) instead of cloning them into the
    /// draft (3+ GB extra VRAM).
    ///
    /// `lm_head` is a closure rather than a `&Linear` because the
    /// target's live lm_head is typically an `Arc<dyn QuantMethod>`
    /// after ISQ (Q4_K_M in the DFlash reference). The closure
    /// abstracts over "apply a fp32/bf16 `Linear`", "apply a
    /// `QuantMethod::qmethod_matmul`", or any future tiled lm_head
    /// variant — the draft doesn't care as long as the output is
    /// `[..., vocab_size]`.
    pub fn forward_with_lm_head<F>(
        &self,
        input_ids: &Tensor,
        target_hidden_cat: &Tensor,
        target_embed: &Embedding,
        lm_head: F,
    ) -> Result<Tensor>
    where
        F: FnOnce(&Tensor) -> Result<Tensor>,
    {
        let noise_embeds = target_embed.forward(input_ids)?;
        let h = self.forward_hidden(&noise_embeds, target_hidden_cat)?;
        lm_head(&h)
    }

    pub fn config(&self) -> &DFlashDraftConfig {
        &self.cfg
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn dtype(&self) -> DType {
        self.dtype
    }
}

/// Build `(cos, sin)` RoPE tables of shape `[max_positions, head_dim/2]`.
///
/// Separate from `layers::RotaryEmbedding::new` because (a) the draft
/// needs raw `(seq, head_dim/2)` tables sliced per-call, not the
/// per-request-offset lookup the engine's helper provides, and (b)
/// keeping this module dependency-free from the engine's layer stack
/// makes it easy to load and test the draft in isolation.
fn build_rope_tables(
    base: f32,
    head_dim: usize,
    max_positions: usize,
    device: &Device,
    dtype: DType,
) -> Result<(Tensor, Tensor)> {
    let inv_freq: Vec<f32> = (0..head_dim)
        .step_by(2)
        .map(|i| 1.0 / base.powf(i as f32 / head_dim as f32))
        .collect();
    let inv_freq_len = inv_freq.len();
    let inv_freq = Tensor::from_vec(inv_freq, (1, inv_freq_len), device)?;

    let t = Tensor::arange(0u32, max_positions as u32, device)?
        .to_dtype(DType::F32)?
        .reshape((max_positions, 1))?;
    let freqs = t.matmul(&inv_freq)?;
    let cos = freqs.cos()?.to_dtype(dtype)?.contiguous()?;
    let sin = freqs.sin()?.to_dtype(dtype)?.contiguous()?;
    Ok((cos, sin))
}

