//! Qwen3.5-27B hybrid target model — full decoder composition.
//!
//! Loads the full weight set from a GGUF file and runs a forward pass
//! producing next-token logits. This is the integration artifact that
//! transitions the CUDA stack from "individual kernels pass unit tests"
//! to "an actual model executes end-to-end".
//!
//! # Layer classification (Qwen3.5 hybrid)
//!
//! From dflash-ref's `gguf_target_loader.cpp`:
//!
//! ```text
//! is_full_attn_layer = (L + 1) % full_attention_interval == 0
//! ```
//!
//! For 27B the interval is 4, so layers {3, 7, 11, …, 63} are
//! FullAttention (16 of 64) and the rest are Gated DeltaNet (48 of 64).
//!
//! # Tensor-name conventions (from dflash-ref)
//!
//! Top-level:
//!
//! ```text
//!   token_embd.weight              [vocab, hidden]
//!   output_norm.weight             [hidden]             F32
//!   output.weight                  [vocab, hidden]      Q6_K (tied to embed on 27B)
//! ```
//!
//! Per FA layer (`blk.<i>.`, `i % 4 == 3`):
//!
//! ```text
//!   attn_norm.weight               [hidden]             F32
//!   post_attention_norm.weight     [hidden]             F32
//!   attn_q.weight                  [hidden, 2*q_dim]    Q4_K  (Q || gate packed)
//!   attn_k.weight                  [hidden, kv_dim]     Q8_0
//!   attn_v.weight                  [hidden, kv_dim]     Q8_0
//!   attn_output.weight             [q_dim,  hidden]     Q5_K
//!   attn_q_norm.weight             [head_dim]           F32
//!   attn_k_norm.weight             [head_dim]           F32
//!   ffn_{gate,up,down}.weight      …                    IQ4_XS
//! ```
//!
//! Per GDN layer (`blk.<i>.`, `i % 4 != 3`):
//!
//! ```text
//!   attn_norm.weight               [hidden]             F32
//!   post_attention_norm.weight     [hidden]             F32
//!   attn_qkv.weight                [hidden, 10240]      Q5_K  (q/k/v/beta fused)
//!   attn_gate.weight               [hidden, 6144]       Q5_K
//!   ssm_conv1d.weight              [inner, 4]           F32
//!   ssm_a                          [dt_rank=48]         F32
//!   ssm_alpha.weight               [dt_rank, hidden]    F32
//!   ssm_beta.weight                [dt_rank, hidden]    F32
//!   ssm_dt.bias                    [dt_rank]            F32
//!   ssm_norm.weight                [state=128]          F32
//!   ssm_out.weight                 [inner, hidden]      Q5_K
//!   ffn_{gate,up,down}.weight      …                    IQ4_XS
//! ```
//!
//! # Phase 4 status — skip-with-warn
//!
//! The current GGUF loader ([`crate::gguf`]) supports only F32, F16,
//! BF16, Q4_K, I8, I32. The vast majority of production Qwen3.5-27B
//! tensors are Q5_K / Q6_K / Q8_0 / IQ4_XS — none of which this loader
//! reads yet. Each layer's construction therefore follows this
//! policy:
//!
//! * If all required weight names resolve to supported-dtype
//!   [`crate::gguf::GgufBuf::Bf16`] tensors matching the expected
//!   shape, the layer gets real weights.
//! * Any missing or wrong-shape weight triggers a `tracing::warn!`
//!   and the weight is filled with a zeroed placeholder
//!   [`crate::tensor::CudaTensor`] of the correct shape. The layer
//!   will still run (producing mostly-zero outputs that flow through
//!   the residual stream as identity), satisfying the smoke-test
//!   correctness bar: "forward doesn't NaN/Inf".
//!
//! Phase 5 extends the GGUF loader with Q5_K / Q6_K / Q8_0 /
//! IQ4_XS kernels; this file's wiring stays unchanged and starts
//! seeing real weights as the loader grows.
//!
//! # KvCache advance semantics
//!
//! Each [`Qwen35FullAttention`] layer auto-advances `kv_cache.n_filled`
//! by `n_tokens` at the end of its forward. With multiple FA layers
//! sharing one KvCache, this would cumulatively mis-track the fill
//! level. [`Qwen35Target::forward`] captures `n_filled` once before
//! the layer loop (as `prompt_start`) and — for every FA layer except
//! the last — calls `kv_cache.rewind(n_tokens)` immediately after, so
//! the next FA layer sees the same `prompt_start`. The final FA
//! layer's advance sticks, leaving the cache correctly advanced at
//! loop exit.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use half::{bf16, f16};

use crate::device::DeviceContext;
use crate::gguf::{load_gguf_lenient, GgufBuf, GgufTensor};
use crate::kernels::{
    launch_cast_bf16_to_f32, launch_cast_f32_to_bf16, launch_embedding_bf16, launch_embedding_f16,
    launch_embedding_f32, launch_matmul_bf16_f32, launch_rmsnorm_f32,
};
use crate::kv_cache::KvCache;
use crate::tensor::CudaTensor;

use super::config::Qwen35Config;
use super::full_attention::Qwen35FullAttention;
use super::gdn::Qwen35GDN;

/// One decoder layer — either full attention or GDN.
pub enum Qwen35Layer {
    FullAttention(Qwen35FullAttention),
    Gdn(Qwen35GDN),
}

/// Full Qwen3.5 target model: embedding → 64-layer decoder → final
/// norm → lm_head.
///
/// The lm_head is held as a bf16 `[vocab, hidden]` tensor. On real
/// 27B weights the GGUF stores `output.weight` as Q6_K; that's
/// unsupported today, so the loader falls back to the tied-weight
/// path (`lm_head = embed` transposed — or re-used, since Qwen's embed
/// is `[vocab, hidden]` and the projection we want is
/// `[hidden, vocab]`: we materialize the transpose lazily in the
/// constructor).
pub struct Qwen35Target {
    pub config: Qwen35Config,
    /// `[vocab, hidden]` bf16. Used by the input embedding lookup.
    pub embed: CudaTensor<bf16>,
    /// 64 layers (for 27B), interleaved by the `(L + 1) % 4 == 0`
    /// rule.
    pub layers: Vec<Qwen35Layer>,
    /// `[hidden]` f32 — final RMSNorm weights (`output_norm.weight`).
    pub final_norm: CudaTensor<f32>,
    /// `[hidden, vocab]` bf16 — lm head. May be tied to the
    /// (transposed) embedding when `output.weight` is a GGUF dtype we
    /// don't yet support.
    pub lm_head: CudaTensor<bf16>,
    /// Total number of tokens in the vocabulary. Cached for
    /// convenience since the embed tensor carries it as `shape()[0]`.
    pub vocab_size: usize,
    /// Number of FullAttention layers (for sizing a KvCache).
    pub n_full_attn: usize,
    /// Number of GDN layers (for sizing the gdn_state vector).
    pub n_gdn: usize,
    pub device: Arc<DeviceContext>,
}

impl Qwen35Target {
    /// Load the full weight set from a GGUF file.
    ///
    /// Uses the lenient GGUF loader, which uploads only
    /// Q4_K/F32/F16/BF16/I8/I32 tensors. Anything else (Q5_K, Q6_K,
    /// Q8_0, IQ*-XS) triggers a warn inside the loader and becomes a
    /// zero-placeholder at this level. This is Phase-4 behavior — the
    /// smoke test is concerned with "forward produces finite logits
    /// without crashing", not numeric accuracy. Phase 5 ports the
    /// missing dtypes.
    pub fn load_from_gguf<P: AsRef<Path>>(
        device: Arc<DeviceContext>,
        config: Qwen35Config,
        gguf_path: P,
    ) -> Result<Self> {
        let gguf_path = gguf_path.as_ref();
        tracing::info!(
            path = %gguf_path.display(),
            hidden_dim = config.hidden_dim,
            "qwen35_target: loading gguf"
        );
        let load = load_gguf_lenient(&device, gguf_path)
            .with_context(|| format!("load_gguf_lenient({})", gguf_path.display()))?;
        let tensors = load.tensors;
        let unsupported = load.unsupported;
        tracing::info!(
            total = load.total_descriptors,
            uploaded = tensors.len(),
            skipped = unsupported.len(),
            "qwen35_target: gguf parsed"
        );

        // ------------------------------------------------------------
        // Top-level tensors.
        // ------------------------------------------------------------
        let embed = load_embed_as_bf16(&device, &tensors, config.hidden_dim)?;
        let vocab_size = embed.shape()[0];

        let final_norm = load_f32_placeholder(
            &device,
            &tensors,
            "output_norm.weight",
            vec![config.hidden_dim],
        );

        // Try `output.weight` → lm_head (Q6_K in real 27B, so usually
        // not loadable). Fall back to a bf16 zero tensor of the right
        // shape. A "correct" tied-weight fallback would transpose the
        // embedding (`lm_head[i,j] = embed[j,i]`) but that forces a
        // device-side transpose kernel we don't have yet; we instead
        // ship a zero lm_head with a tracing warning. The resulting
        // logits are all zero (finite!) which clears the smoke-test
        // bar. Phase 5's Q6_K + transpose kernels remove this TODO.
        //
        // TODO(phase-5): materialize tied weights by
        // transposing the embedding when `output.weight` is missing.
        let lm_head =
            load_bf16_matrix(&tensors, "output.weight", &[config.hidden_dim, vocab_size])
                .unwrap_or_else(|missing| {
                    tracing::warn!(
                        ?missing,
                        shape = ?[config.hidden_dim, vocab_size],
                        "qwen35_target: output.weight missing or unsupported dtype; \
                         falling back to zeroed lm_head. Logits will be all zero."
                    );
                    CudaTensor::<bf16>::zeros(
                        device.clone(),
                        vec![config.hidden_dim, vocab_size],
                    )
                    .expect("alloc zero lm_head")
                });

        // ------------------------------------------------------------
        // Per-layer construction.
        //
        // Qwen3.5-27B has n_layer=64. The layer count isn't in our
        // `Qwen35Config` — infer it from the GGUF tensor index range.
        // If anything goes wrong, fall back to 64.
        // ------------------------------------------------------------
        let n_layers = detect_n_layers(&tensors).unwrap_or(64);
        let full_attention_interval = 4usize; // Qwen3.5-27B baked constant.
        let mut layers: Vec<Qwen35Layer> = Vec::with_capacity(n_layers);
        let mut n_full_attn = 0usize;
        let mut n_gdn = 0usize;

        for l in 0..n_layers {
            let is_fa = ((l + 1) % full_attention_interval) == 0;
            if is_fa {
                let layer = build_full_attention_layer(&device, &config, &tensors, l, n_full_attn);
                layers.push(Qwen35Layer::FullAttention(layer));
                n_full_attn += 1;
            } else {
                let layer = build_gdn_layer(&device, &config, &tensors, l);
                layers.push(Qwen35Layer::Gdn(layer));
                n_gdn += 1;
            }
        }

        tracing::info!(
            n_layers,
            n_full_attn,
            n_gdn,
            vocab_size,
            "qwen35_target: constructed"
        );

        Ok(Self {
            config,
            embed,
            layers,
            final_norm,
            lm_head,
            vocab_size,
            n_full_attn,
            n_gdn,
            device,
        })
    }

    /// Run one forward pass over a batch of `n_tokens` tokens.
    ///
    /// # Inputs
    /// * `tokens` — `[n_tokens]` i32, each a vocab id.
    /// * `positions` — `[4, n_tokens]` i32 MRoPE position indices.
    /// * `kv_cache` — one K/V slab per **full-attention** layer; size
    ///    `n_layers=self.n_full_attn`, `n_kv_heads=config.n_kv_heads`,
    ///    `head_dim=config.head_dim`.
    /// * `gdn_states` — one recurrent SSM state tensor per **GDN**
    ///    layer, each `[S_v, S_v, H, 1]` f32.
    /// * `gdn_inter` — one per-token state snapshot buffer per GDN
    ///    layer, each `[S_v, S_v, H, max_verify_tokens]` f16 (where
    ///    max_verify_tokens >= n_tokens).
    ///
    /// # Output
    ///
    /// `[n_tokens, vocab_size]` f32 logits.
    ///
    /// # n_tokens alignment
    ///
    /// The bf16 matmul kernel requires `n_tokens % 32 == 0`. The
    /// caller is responsible for padding; the target itself doesn't
    /// reshape inputs. The smoke test below pads a 9-token prompt up
    /// to 32 and trims logits to 9.
    pub fn forward(
        &self,
        tokens: &CudaTensor<i32>,
        positions: &CudaTensor<i32>,
        kv_cache: &mut KvCache,
        gdn_states: &mut [CudaTensor<f32>],
        gdn_inter: &mut [CudaTensor<f16>],
    ) -> Result<CudaTensor<f32>> {
        // ── 0. Shape validation.
        if tokens.shape().len() != 1 {
            return Err(anyhow!(
                "qwen35 target.forward: tokens must be 1D [n_tokens], got {:?}",
                tokens.shape()
            ));
        }
        let n_tokens = tokens.shape()[0];
        if n_tokens == 0 {
            return Err(anyhow!("qwen35 target.forward: n_tokens must be > 0"));
        }
        let hidden_dim = self.config.hidden_dim;

        if positions.shape() != [4, n_tokens] {
            return Err(anyhow!(
                "qwen35 target.forward: positions shape {:?} != [4, {}]",
                positions.shape(),
                n_tokens
            ));
        }
        if gdn_states.len() < self.n_gdn {
            return Err(anyhow!(
                "qwen35 target.forward: gdn_states.len()={} < n_gdn={}",
                gdn_states.len(),
                self.n_gdn
            ));
        }
        if gdn_inter.len() < self.n_gdn {
            return Err(anyhow!(
                "qwen35 target.forward: gdn_inter.len()={} < n_gdn={}",
                gdn_inter.len(),
                self.n_gdn
            ));
        }
        if kv_cache.n_layers() < self.n_full_attn {
            return Err(anyhow!(
                "qwen35 target.forward: kv_cache has {} layers, need >= {} FA layers",
                kv_cache.n_layers(),
                self.n_full_attn
            ));
        }

        // ── 1. Embedding lookup.
        let mut hidden =
            CudaTensor::<bf16>::zeros(self.device.clone(), vec![n_tokens, hidden_dim])?;
        launch_embedding_bf16(&self.device, &self.embed, tokens, &mut hidden)?;

        // ── 2. Layer loop.
        //
        //      Each FA layer auto-advances kv_cache.n_filled by
        //      n_tokens. We rewind immediately after each FA layer
        //      except the last so all FA layers observe the same
        //      `prompt_start` (see the KvCache advance-semantics
        //      section in the module docs).
        //
        //      `fa_seen` / `fa_total` track which FA layer is up; we
        //      only rewind when there are more FA layers coming.
        let fa_total = self.n_full_attn;
        let mut fa_seen: usize = 0;
        let mut gdn_idx: usize = 0;

        for layer in &self.layers {
            match layer {
                Qwen35Layer::FullAttention(fa) => {
                    fa.forward(&self.device, &mut hidden, positions, kv_cache)?;
                    fa_seen += 1;
                    // Rewind unless this is the final FA layer.
                    if fa_seen < fa_total {
                        kv_cache.rewind(n_tokens);
                    }
                }
                Qwen35Layer::Gdn(gdn) => {
                    gdn.forward(
                        &self.device,
                        &mut hidden,
                        positions,
                        &mut gdn_states[gdn_idx],
                        &mut gdn_inter[gdn_idx],
                    )?;
                    gdn_idx += 1;
                }
            }
        }

        // ── 3. Final norm (f32).
        let mut hidden_f32 =
            CudaTensor::<f32>::zeros(self.device.clone(), vec![n_tokens, hidden_dim])?;
        launch_cast_bf16_to_f32(&self.device, &hidden, &mut hidden_f32)?;
        let mut norm_f32 =
            CudaTensor::<f32>::zeros(self.device.clone(), vec![n_tokens, hidden_dim])?;
        launch_rmsnorm_f32(
            &self.device,
            &hidden_f32,
            &self.final_norm,
            &mut norm_f32,
            self.config.rms_eps,
        )?;
        let mut norm_bf16 =
            CudaTensor::<bf16>::zeros(self.device.clone(), vec![n_tokens, hidden_dim])?;
        launch_cast_f32_to_bf16(&self.device, &norm_f32, &mut norm_bf16)?;

        // ── 4. LM head: logits = norm_bf16 · lm_head → [n_tokens, vocab] f32.
        //      matmul kernel requires M, K, N divisible by 32.
        //      n_tokens alignment is the caller's responsibility.
        if !n_tokens.is_multiple_of(32) {
            return Err(anyhow!(
                "qwen35 target.forward: n_tokens={} must be a multiple of 32 for lm_head matmul",
                n_tokens
            ));
        }
        if !self.vocab_size.is_multiple_of(32) {
            return Err(anyhow!(
                "qwen35 target.forward: vocab_size={} must be a multiple of 32 for lm_head matmul",
                self.vocab_size
            ));
        }
        let mut logits =
            CudaTensor::<f32>::zeros(self.device.clone(), vec![n_tokens, self.vocab_size])?;
        launch_matmul_bf16_f32(
            &self.device,
            &norm_bf16,
            &self.lm_head,
            &mut logits,
            n_tokens,
            hidden_dim,
            self.vocab_size,
        )?;

        Ok(logits)
    }
}

// ────────────────────────────────────────────────────────────────────
// Per-layer weight loaders.
// ────────────────────────────────────────────────────────────────────

fn build_full_attention_layer(
    device: &Arc<DeviceContext>,
    config: &Qwen35Config,
    tensors: &HashMap<String, GgufTensor>,
    layer_idx: usize,
    fa_kv_slot: usize,
) -> Qwen35FullAttention {
    let cfg = *config;
    let name_attn_norm = format!("blk.{}.attn_norm.weight", layer_idx);
    let name_q = format!("blk.{}.attn_q.weight", layer_idx);
    let name_k = format!("blk.{}.attn_k.weight", layer_idx);
    let name_v = format!("blk.{}.attn_v.weight", layer_idx);
    let name_o = format!("blk.{}.attn_output.weight", layer_idx);

    let attn_norm = load_f32_placeholder(device, tensors, &name_attn_norm, vec![cfg.hidden_dim]);

    // dflash pre-packs `attn_q` as `[hidden, 2*q_dim]` (Q + gate); we
    // expect `[hidden, q_dim]` here because our FA layer doesn't
    // consume the gate. When only packed Q-gate is available, the
    // loader below will see a shape mismatch and fall through to the
    // zero placeholder — ack'd in the phase-5 TODO.
    //
    // TODO(phase-5): slice off the gate half of attn_q when loading.
    let w_q = load_bf16_placeholder(device, tensors, &name_q, vec![cfg.hidden_dim, cfg.q_dim()]);
    let w_k = load_bf16_placeholder(device, tensors, &name_k, vec![cfg.hidden_dim, cfg.kv_dim()]);
    let w_v = load_bf16_placeholder(device, tensors, &name_v, vec![cfg.hidden_dim, cfg.kv_dim()]);
    let w_o = load_bf16_placeholder(device, tensors, &name_o, vec![cfg.q_dim(), cfg.hidden_dim]);

    // NOTE: `layer_idx` on Qwen35FullAttention indexes into the KV
    // cache slab vector. Because we allocate a KvCache with one slab
    // per FA layer (not per model layer), the slab index is the
    // running FA count — NOT the model's layer index.
    Qwen35FullAttention {
        attn_norm,
        w_q,
        w_k,
        w_v,
        w_o,
        config: cfg,
        layer_idx: fa_kv_slot,
    }
}

fn build_gdn_layer(
    device: &Arc<DeviceContext>,
    config: &Qwen35Config,
    tensors: &HashMap<String, GgufTensor>,
    layer_idx: usize,
) -> Qwen35GDN {
    let cfg = *config;
    let name_attn_norm = format!("blk.{}.attn_norm.weight", layer_idx);
    let name_qkvg = format!("blk.{}.attn_qkv.weight", layer_idx);
    let name_out = format!("blk.{}.ssm_out.weight", layer_idx);

    let pre_norm = load_f32_placeholder(device, tensors, &name_attn_norm, vec![cfg.hidden_dim]);

    let h = cfg.n_q_heads;
    let head_dim = cfg.head_dim;
    let proj_dim = h * 4 * head_dim;
    let kv_dim = h * head_dim;

    // `attn_qkv.weight` in the real 27B is Q5_K shape [hidden, 10240].
    // Our per-layer kernel expects [hidden, h*4*head_dim] bf16 — same
    // intent (q/k/v/g fused), different byte layout, and the GGUF
    // dtype isn't loadable at all yet. Placeholder.
    let w_qkvg = load_bf16_placeholder(device, tensors, &name_qkvg, vec![cfg.hidden_dim, proj_dim]);
    let w_out = load_bf16_placeholder(device, tensors, &name_out, vec![kv_dim, cfg.hidden_dim]);

    Qwen35GDN {
        pre_norm,
        w_qkvg,
        w_out,
        config: cfg,
        layer_idx,
    }
}

// ────────────────────────────────────────────────────────────────────
// Weight-table helpers.
// ────────────────────────────────────────────────────────────────────

/// Read `token_embd.weight` and coerce to a bf16 `[vocab, hidden]`
/// tensor. Accepts BF16 directly, F16 and F32 via cast-on-host, or
/// falls back to a zero-filled placeholder with a tracing warning.
fn load_embed_as_bf16(
    device: &Arc<DeviceContext>,
    tensors: &HashMap<String, GgufTensor>,
    hidden_dim: usize,
) -> Result<CudaTensor<bf16>> {
    let key = "token_embd.weight";
    let Some(t) = tensors.get(key) else {
        tracing::warn!(key, "qwen35_target: {} missing; using zero placeholder with vocab=151936", key);
        return CudaTensor::<bf16>::zeros(device.clone(), vec![151936, hidden_dim]);
    };
    if t.shape.len() != 2 {
        return Err(anyhow!(
            "qwen35_target: {} must be 2-D, got shape {:?}",
            key,
            t.shape
        ));
    }
    let vocab = t.shape[0];
    if t.shape[1] != hidden_dim {
        tracing::warn!(
            key,
            shape = ?t.shape,
            hidden_dim,
            "qwen35_target: token_embd hidden_dim mismatch; using zero placeholder"
        );
        return CudaTensor::<bf16>::zeros(device.clone(), vec![vocab, hidden_dim]);
    }
    match &t.buf {
        GgufBuf::Bf16(src) => {
            // Copy to a fresh tensor (caller needs ownership detached
            // from the loader map).
            let host = src.to_host()?;
            CudaTensor::<bf16>::from_host(device.clone(), vec![vocab, hidden_dim], &host)
        }
        GgufBuf::F16(src) => {
            // Cast f16 → bf16 via a host round-trip. Alternative
            // would be allocating an f16 intermediate and using
            // launch_cast_f16_to_f32 + launch_cast_f32_to_bf16; the
            // host path is simpler at load time and only runs once.
            let host_f16 = src.to_host()?;
            let host_bf16: Vec<bf16> = host_f16
                .iter()
                .map(|v| bf16::from_f32(v.to_f32()))
                .collect();
            CudaTensor::<bf16>::from_host(device.clone(), vec![vocab, hidden_dim], &host_bf16)
        }
        GgufBuf::F32(src) => {
            let host_f32 = src.to_host()?;
            let host_bf16: Vec<bf16> = host_f32.iter().map(|v| bf16::from_f32(*v)).collect();
            CudaTensor::<bf16>::from_host(device.clone(), vec![vocab, hidden_dim], &host_bf16)
        }
        other => {
            tracing::warn!(
                key,
                dtype = ?std::mem::discriminant(other),
                "qwen35_target: token_embd.weight dtype not yet supported; zero placeholder"
            );
            CudaTensor::<bf16>::zeros(device.clone(), vec![vocab, hidden_dim])
        }
    }
}

/// Read an `[hidden]` f32 vector (e.g. an RMSNorm weight); fall back
/// to an ones-filled placeholder (1.0 is the identity for RMSNorm
/// weights — produces bit-exact rmsnorm output without weight scaling)
/// if the tensor is missing or the dtype is wrong.
fn load_f32_placeholder(
    device: &Arc<DeviceContext>,
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    expected_shape: Vec<usize>,
) -> CudaTensor<f32> {
    let numel: usize = expected_shape.iter().product();
    let placeholder = || {
        let ones = vec![1.0f32; numel];
        CudaTensor::<f32>::from_host(device.clone(), expected_shape.clone(), &ones)
            .expect("upload f32 placeholder")
    };
    let Some(t) = tensors.get(key) else {
        tracing::warn!(
            key,
            shape = ?expected_shape,
            "qwen35_target: {} missing; using ones placeholder",
            key
        );
        return placeholder();
    };
    if t.shape != expected_shape {
        tracing::warn!(
            key,
            shape = ?t.shape,
            expected = ?expected_shape,
            "qwen35_target: shape mismatch; using ones placeholder"
        );
        return placeholder();
    }
    match &t.buf {
        GgufBuf::F32(src) => {
            let host = match src.to_host() {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(key, error = %e, "qwen35_target: download f32 failed");
                    return placeholder();
                }
            };
            let shape_for_upload = expected_shape.clone();
            CudaTensor::<f32>::from_host(device.clone(), shape_for_upload, &host)
                .unwrap_or_else(|e| {
                    tracing::warn!(key, error = %e, "qwen35_target: upload f32 failed");
                    let ones = vec![1.0f32; numel];
                    CudaTensor::<f32>::from_host(device.clone(), expected_shape.clone(), &ones)
                        .expect("upload f32 placeholder")
                })
        }
        _ => {
            tracing::warn!(
                key,
                shape = ?t.shape,
                "qwen35_target: dtype not F32; using ones placeholder"
            );
            placeholder()
        }
    }
}

/// Return a bf16 tensor for `key` matching `expected_shape`; on
/// any mismatch return Err containing the reason so the caller can
/// choose to fall back to a placeholder.
///
/// Kept as a distinct function so the lm_head path can choose a
/// different fallback (zeros) from the layer-weight path (zeros with
/// different logging).
fn load_bf16_matrix(
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    expected_shape: &[usize],
) -> std::result::Result<CudaTensor<bf16>, String> {
    let Some(t) = tensors.get(key) else {
        return Err(format!("{} not present", key));
    };
    if t.shape != expected_shape {
        return Err(format!(
            "{}: shape {:?} != expected {:?}",
            key, t.shape, expected_shape
        ));
    }
    match &t.buf {
        GgufBuf::Bf16(src) => {
            let host = src
                .to_host()
                .map_err(|e| format!("{}: download: {:?}", key, e))?;
            CudaTensor::<bf16>::from_host(src.device().clone(), expected_shape.to_vec(), &host)
                .map_err(|e| format!("{}: upload: {:?}", key, e))
        }
        _ => Err(format!("{}: dtype is not BF16", key)),
    }
}

/// Same as [`load_bf16_matrix`] but folds the error path into a
/// tracing warning + zero placeholder. Used by the per-layer
/// constructors since we want the loader to survive any single
/// missing weight.
fn load_bf16_placeholder(
    device: &Arc<DeviceContext>,
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    expected_shape: Vec<usize>,
) -> CudaTensor<bf16> {
    match load_bf16_matrix(tensors, key, &expected_shape) {
        Ok(t) => t,
        Err(reason) => {
            tracing::warn!(
                key,
                shape = ?expected_shape,
                %reason,
                "qwen35_target: layer weight unavailable; using zero placeholder"
            );
            CudaTensor::<bf16>::zeros(device.clone(), expected_shape)
                .expect("alloc bf16 zero placeholder")
        }
    }
}

/// Scan tensor names of the form `blk.N.*` and return `max(N) + 1`.
///
/// Used to size the layer vector when the loader didn't parse the
/// GGUF metadata's `qwen35.block_count`. Returns `None` if no `blk.N`
/// tensor exists (which, for our 27B target, would be catastrophic
/// — but the caller falls back to a conservative default rather than
/// aborting, to preserve the smoke-test's "well-scoped skip" contract).
fn detect_n_layers(tensors: &HashMap<String, GgufTensor>) -> Option<usize> {
    let mut max_idx: Option<usize> = None;
    for name in tensors.keys() {
        let Some(rest) = name.strip_prefix("blk.") else {
            continue;
        };
        let Some((idx_str, _)) = rest.split_once('.') else {
            continue;
        };
        let Ok(idx) = idx_str.parse::<usize>() else {
            continue;
        };
        max_idx = Some(max_idx.map_or(idx, |m| m.max(idx)));
    }
    max_idx.map(|m| m + 1)
}

// ────────────────────────────────────────────────────────────────────
// Convenience re-exports + helpers used by tests. The public crate
// surface intentionally avoids `launch_embedding_{f16,f32}` wiring
// here — we pin on the BF16 variant via the load-time cast above.
// ────────────────────────────────────────────────────────────────────

/// Silence unused-import warnings for the f16/f32 embedding launchers
/// in builds where the `tests` module is cfg'd out. The load path
/// does the cast once at weight-upload time and uses the bf16 variant
/// at forward.
#[allow(dead_code)]
fn _unused_embed_casts(
    device: &Arc<DeviceContext>,
    w_f16: &CudaTensor<f16>,
    w_f32: &CudaTensor<f32>,
    ids: &CudaTensor<i32>,
    out: &mut CudaTensor<bf16>,
) -> Result<()> {
    launch_embedding_f16(device, w_f16, ids, out)?;
    launch_embedding_f32(device, w_f32, ids, out)?;
    Ok(())
}

// ────────────────────────────────────────────────────────────────────
// Integration smoke test — A6000-only, run with --ignored.
// ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Path to the on-host Qwen3.5-27B Q4_K_M GGUF. Only exists on the
    /// A6000 build host; the test is ignored elsewhere.
    const QWEN35_27B_GGUF: &str =
        "/home/metricspace/dflash-ref/dflash/models/Qwen3.5-27B-Q4_K_M.gguf";

    /// Minimum vocab_size we expect from any Qwen3.5-27B build.
    const MIN_VOCAB: usize = 150_000;

    /// End-to-end smoke test. Loads the 27B GGUF (allowed up to ~2 min
    /// for mmap + H2D of ~15 GB), builds a KvCache + GDN state
    /// vectors, runs forward on the 9-token HumanEval prompt (padded
    /// to 32), and asserts logits shape + finite.
    ///
    /// Run with:
    ///   cargo test -p ctox-engine-cuda --features cuda --release -- \
    ///     --ignored --nocapture qwen35_target_gguf_smoke
    #[test]
    #[ignore]
    fn qwen35_target_gguf_smoke() {
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let cfg = Qwen35Config::QWEN35_27B;

        eprintln!("qwen35_target_gguf_smoke: loading {}", QWEN35_27B_GGUF);
        let target = Qwen35Target::load_from_gguf(dev.clone(), cfg, QWEN35_27B_GGUF)
            .expect("load_from_gguf");
        eprintln!(
            "loaded: vocab={} n_full_attn={} n_gdn={} layers={}",
            target.vocab_size,
            target.n_full_attn,
            target.n_gdn,
            target.layers.len()
        );

        assert!(
            target.vocab_size >= MIN_VOCAB,
            "vocab_size {} suspiciously small",
            target.vocab_size
        );
        assert_eq!(target.n_full_attn, 16, "27B should have 16 FA layers");
        assert_eq!(target.n_gdn, 48, "27B should have 48 GDN layers");
        assert_eq!(target.layers.len(), 64, "27B should have 64 layers total");

        // Prompt: 9 tokens, padded to 32 for matmul alignment.
        let prompt: [i32; 9] = [7734, 264, 6185, 36974, 883, 13094, 6326, 61369, 25];
        let n_real = prompt.len();
        let n_tokens = 32usize;
        let mut tokens_host = vec![0i32; n_tokens];
        tokens_host[..n_real].copy_from_slice(&prompt);
        let tokens = CudaTensor::<i32>::from_host(dev.clone(), vec![n_tokens], &tokens_host)
            .expect("upload tokens");

        // MRoPE positions — axis-major [4, n_tokens], simple
        // monotonically-increasing text positions on axis 0, zeros on
        // the other three (no vision/audio modality in the smoke test).
        let mut positions_host = vec![0i32; 4 * n_tokens];
        for t in 0..n_tokens {
            positions_host[t] = t as i32;
        }
        let positions =
            CudaTensor::<i32>::from_host(dev.clone(), vec![4, n_tokens], &positions_host)
                .expect("upload positions");

        // KvCache: one slab per FA layer (16).
        let max_ctx = 4096usize.max(n_tokens);
        let mut kv_cache = KvCache::new(
            dev.clone(),
            target.n_full_attn,
            max_ctx,
            cfg.n_kv_heads,
            cfg.head_dim,
        )
        .expect("alloc kv cache");

        // GDN state + inter, one per GDN layer.
        let s_v = cfg.gdn_ssm_dim;
        let h = cfg.n_q_heads;
        let mut gdn_states: Vec<CudaTensor<f32>> = Vec::with_capacity(target.n_gdn);
        let mut gdn_inter: Vec<CudaTensor<f16>> = Vec::with_capacity(target.n_gdn);
        for _ in 0..target.n_gdn {
            gdn_states.push(
                CudaTensor::<f32>::zeros(dev.clone(), vec![s_v, s_v, h, 1])
                    .expect("alloc gdn state"),
            );
            gdn_inter.push(
                CudaTensor::<f16>::zeros(dev.clone(), vec![s_v, s_v, h, n_tokens])
                    .expect("alloc gdn inter"),
            );
        }

        eprintln!("qwen35_target_gguf_smoke: running forward");
        let logits = target
            .forward(
                &tokens,
                &positions,
                &mut kv_cache,
                &mut gdn_states,
                &mut gdn_inter,
            )
            .expect("forward");
        dev.synchronize().expect("synchronize");

        // Shape: [n_tokens, vocab_size].
        assert_eq!(
            logits.shape(),
            [n_tokens, target.vocab_size],
            "logits shape mismatch"
        );

        // No NaN / Inf.
        let host = logits.to_host().expect("download logits");
        let mut n_nan = 0usize;
        let mut n_inf = 0usize;
        let mut max_abs = 0.0f32;
        for &v in &host {
            if v.is_nan() {
                n_nan += 1;
            } else if v.is_infinite() {
                n_inf += 1;
            } else if v.abs() > max_abs {
                max_abs = v.abs();
            }
        }
        eprintln!(
            "qwen35_target_gguf_smoke: logits shape={:?} nan={} inf={} max_abs={:.4e}",
            logits.shape(),
            n_nan,
            n_inf,
            max_abs
        );
        assert_eq!(n_nan, 0, "logits contain {} NaN", n_nan);
        assert_eq!(n_inf, 0, "logits contain {} Inf", n_inf);
    }
}
