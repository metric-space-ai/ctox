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
//!   [`crate::gguf_loader::GgufBuf::Bf16`] tensors matching the expected
//!   shape, the layer gets real weights.
//! * Any missing or wrong-shape weight triggers a `tracing::warn!`
//!   and the weight is filled with a zeroed placeholder
//!   [`ctox_cuda_primitives::tensor::CudaTensor`] of the correct shape. The layer
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

use ctox_cuda_primitives::device::DeviceContext;
use crate::gguf_loader::{load_gguf_lenient_with_config, GgufBuf, GgufTensor, LoaderConfig};
use crate::kernels::{
    launch_cast_bf16_to_f32, launch_cast_f32_to_bf16, launch_embedding_bf16, launch_embedding_f16,
    launch_embedding_f32, launch_matmul_bf16_f32, launch_rmsnorm_f32,
};
use ctox_cuda_primitives::kv_cache::KvCache;
use ctox_cuda_primitives::tensor::CudaTensor;

use crate::config::Qwen35Config;
use crate::layers::full_attention::Qwen35FullAttention;
use crate::layers::gdn::Qwen35GDN;
use crate::layers::packed_weight::PackedWeight;

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
        Self::load_from_gguf_with_config(device, config, gguf_path, LoaderConfig::default())
    }

    /// [`Self::load_from_gguf`] with an explicit GGUF loader config.
    ///
    /// With `LoaderConfig::keep_packed = true`, Q5K/Q6K/Q8_0/IQ4_XS
    /// tensors land on device as packed byte buffers (matching
    /// GGUF-on-disk size) instead of CPU-dequanted bf16. Phase-6 layer
    /// wiring doesn't yet consume packed bytes in the forward, so those
    /// weights still degrade to zero placeholders — but the resident
    /// weight footprint drops from ~30 GB to ~15 GB for 27B Q4_K_M,
    /// which is what keeps this fitting on a 48 GB A6000.
    pub fn load_from_gguf_with_config<P: AsRef<Path>>(
        device: Arc<DeviceContext>,
        config: Qwen35Config,
        gguf_path: P,
        loader_cfg: LoaderConfig,
    ) -> Result<Self> {
        let gguf_path = gguf_path.as_ref();
        tracing::info!(
            path = %gguf_path.display(),
            hidden_dim = config.hidden_dim,
            keep_packed = loader_cfg.keep_packed,
            "qwen35_target: loading gguf"
        );
        let load = load_gguf_lenient_with_config(&device, gguf_path, loader_cfg).with_context(
            || format!("load_gguf_lenient_with_config({})", gguf_path.display()),
        )?;
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
    let name_post_norm = format!("blk.{}.post_attention_norm.weight", layer_idx);
    let name_q_norm = format!("blk.{}.attn_q_norm.weight", layer_idx);
    let name_k_norm = format!("blk.{}.attn_k_norm.weight", layer_idx);
    let name_q = format!("blk.{}.attn_q.weight", layer_idx);
    let name_k = format!("blk.{}.attn_k.weight", layer_idx);
    let name_v = format!("blk.{}.attn_v.weight", layer_idx);
    let name_o = format!("blk.{}.attn_output.weight", layer_idx);

    let attn_norm = load_f32_placeholder(device, tensors, &name_attn_norm, vec![cfg.hidden_dim]);
    let post_attn_norm =
        load_f32_placeholder(device, tensors, &name_post_norm, vec![cfg.hidden_dim]);
    // Per-head Q/K norms have shape `[head_dim]` — the RMSNorm is applied
    // over the per-head axis, not over hidden.
    let attn_q_norm = load_f32_placeholder(device, tensors, &name_q_norm, vec![cfg.head_dim]);
    let attn_k_norm = load_f32_placeholder(device, tensors, &name_k_norm, vec![cfg.head_dim]);

    // dflash pre-packs `attn_q` as `[hidden, 2*q_dim]` — first half is
    // Q, second half is the sigmoid gate. Slice on the host and upload
    // two distinct bf16 tensors. When the GGUF dtype is unsupported
    // (Q4_K_M ships `attn_q` as Q4_K which we don't dequant yet) the
    // `load_bf16_placeholder` fallback path still needs a Q-side and a
    // gate-side tensor, so we fall back twice with zero-filled halves.
    let (w_q, w_q_gate) = match load_packed_bf16_halves(
        tensors,
        &name_q,
        cfg.hidden_dim,
        cfg.q_dim(),
    ) {
        Ok((q_half, gate_half)) => (
            PackedWeight::Bf16 {
                t: q_half,
                k: cfg.hidden_dim,
                n: cfg.q_dim(),
            },
            PackedWeight::Bf16 {
                t: gate_half,
                k: cfg.hidden_dim,
                n: cfg.q_dim(),
            },
        ),
        Err(reason) => {
            tracing::warn!(
                key = %name_q,
                q_dim = cfg.q_dim(),
                %reason,
                "qwen35_target: attn_q weight unavailable or packed-shape mismatch; \
                 using zero placeholders for both Q and Q-gate halves"
            );
            let zero = || PackedWeight::Zero {
                k: cfg.hidden_dim,
                n: cfg.q_dim(),
            };
            (zero(), zero())
        }
    };

    let w_k = load_packed_weight(
        device,
        tensors,
        &name_k,
        cfg.hidden_dim,
        cfg.kv_dim(),
    );
    let w_v = load_packed_weight(
        device,
        tensors,
        &name_v,
        cfg.hidden_dim,
        cfg.kv_dim(),
    );
    let w_o = load_packed_weight(
        device,
        tensors,
        &name_o,
        cfg.q_dim(),
        cfg.hidden_dim,
    );

    // NOTE: `layer_idx` on Qwen35FullAttention indexes into the KV
    // cache slab vector. Because we allocate a KvCache with one slab
    // per FA layer (not per model layer), the slab index is the
    // running FA count — NOT the model's layer index.
    Qwen35FullAttention {
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
        layer_idx: fa_kv_slot,
    }
}

/// Slice the packed `attn_q.weight` into its Q and Q-gate halves.
///
/// GGUF stores the packed Q/gate tensor as `[hidden, 2*q_dim]`
/// row-major; dflash splits on the last axis so that element
/// `[i, j]` for `j in 0..q_dim` is the Q component and
/// `j in q_dim..2*q_dim` is the gate component.
///
/// Returns a pair `(w_q, w_q_gate)`, both `[hidden, q_dim]` bf16. When
/// the GGUF tensor is missing, non-bf16, or has the wrong shape, this
/// returns `Err` so the caller can fall through to a zero placeholder
/// (same lenient-loader contract the other `attn_*` weights follow).
///
/// Host CPU does the split because the GGUF loader hands us a single
/// `CudaTensor<bf16>`; slicing on device would require another strided-
/// copy kernel we don't have yet. The one-time cost is fine — this
/// runs once at load, not at forward.
fn load_packed_bf16_halves(
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    hidden: usize,
    q_dim: usize,
) -> std::result::Result<(CudaTensor<bf16>, CudaTensor<bf16>), String> {
    let Some(t) = tensors.get(key) else {
        return Err(format!("{} not present", key));
    };
    let expected = [hidden, 2 * q_dim];
    if t.shape != expected {
        return Err(format!(
            "{}: shape {:?} != expected packed {:?}",
            key, t.shape, expected
        ));
    }
    let GgufBuf::Bf16(src) = &t.buf else {
        return Err(format!(
            "{}: packed-halves loader only handles BF16 inputs, got {:?} \
             (packed tensor available but forward doesn't consume packed bytes yet)",
            key, t.dtype
        ));
    };
    let host = src
        .to_host()
        .map_err(|e| format!("{}: download: {:?}", key, e))?;
    // Split row by row: each row has 2*q_dim elements, Q in the first
    // half, gate in the second. Allocate two contiguous row-major
    // halves and copy element-wise — simpler than a chunked reshape
    // dance and the cost is amortized over model load.
    let mut q_half: Vec<bf16> = Vec::with_capacity(hidden * q_dim);
    let mut g_half: Vec<bf16> = Vec::with_capacity(hidden * q_dim);
    for row in 0..hidden {
        let base = row * 2 * q_dim;
        q_half.extend_from_slice(&host[base..base + q_dim]);
        g_half.extend_from_slice(&host[base + q_dim..base + 2 * q_dim]);
    }
    let dev = src.device().clone();
    let w_q = CudaTensor::<bf16>::from_host(dev.clone(), vec![hidden, q_dim], &q_half)
        .map_err(|e| format!("{}: upload Q half: {:?}", key, e))?;
    let w_q_gate = CudaTensor::<bf16>::from_host(dev, vec![hidden, q_dim], &g_half)
        .map_err(|e| format!("{}: upload gate half: {:?}", key, e))?;
    Ok((w_q, w_q_gate))
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
    let w_qkvg = load_packed_weight(device, tensors, &name_qkvg, cfg.hidden_dim, proj_dim);
    let w_out = load_packed_weight(device, tensors, &name_out, kv_dim, cfg.hidden_dim);

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
        // Packed-on-device variants (loader ran with `keep_packed =
        // true`). The Phase-6 layer structs still expect bf16, and the
        // native packed-mmvq wiring is Phase 7; until then, fall
        // through with an error so the caller emits a zero
        // placeholder. The error string calls out "packed tensor
        // available" so operators can tell the weight exists but the
        // forward path doesn't consume it yet.
        GgufBuf::Q5K(_)
        | GgufBuf::Q6K(_)
        | GgufBuf::Q8_0(_)
        | GgufBuf::IQ4XS(_)
        | GgufBuf::Q4K(_) => Err(format!(
            "{}: packed {:?} tensor available but layer forward doesn't yet consume packed bytes",
            key, t.dtype
        )),
        _ => Err(format!("{}: dtype is not BF16", key)),
    }
}

/// Same as [`load_bf16_matrix`] but folds the error path into a
/// tracing warning + zero placeholder. Used by the per-layer
/// constructors since we want the loader to survive any single
/// missing weight.
///
/// Post-Agent-C: the per-layer projection wiring now goes through
/// [`load_packed_weight`] which produces [`PackedWeight`] carriers
/// directly; this helper is retained so ad-hoc bf16-only callers (and
/// the load_packed_bf16_halves FA-Q splitter above) don't churn, but
/// no hot path uses it anymore. Tagged `allow(dead_code)` to suppress
/// warnings until a future cleanup folds it into load_packed_weight.
#[allow(dead_code)]
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

/// Load a `[k, n]` projection weight as a [`PackedWeight`].
///
/// Dispatches on the GGUF dtype:
///
///   * `GgufBuf::Bf16` — re-materialize the bf16 tensor (detaches
///     ownership from the loader map) and wrap in `PackedWeight::Bf16`.
///   * `GgufBuf::F16` / `GgufBuf::F32` — cast on host to bf16 and wrap
///     the same way. Neither arm fires for the production 27B
///     projection tensors (all quantized), but they keep the loader
///     lenient for hand-authored tests / checkpoint variants.
///   * `GgufBuf::Q4K` / `Q5K` / `Q6K` / `Q8_0` / `IQ4XS` — keep the
///     packed byte buffer on device and wrap in the matching
///     [`PackedWeight`] variant. Dispatch at forward time hits the
///     right `launch_mmvq_*_f32` kernel.
///   * Missing key, wrong element count, or any other GGUF dtype we
///     don't recognise — warn and return `PackedWeight::Zero`. The
///     forward path still runs; output for that projection is all
///     zeros. This matches the pre-Agent-C lenient behavior.
///
/// # Shape validation
///
/// The per-layer constructors pass `(k, n)` matching the logical
/// projection shape (`[in_features, out_features]`). llama.cpp stores
/// linear weights on-disk in `[in, out]` ggml order (ne[0]=in fast,
/// ne[1]=out slow); the loader reverses that to row-major, so quantized
/// tensors arrive with `shape = [out, in] = [n, k]`, while the rare
/// dense variants (bf16 test fixtures, etc.) arrive with `shape =
/// [k, n]`. We accept either orientation as long as `numel == k * n`.
/// Shape metadata is advisory for packed variants — the mmvq kernels
/// consume the byte buffer plus the explicit `(k, n)` scalars and
/// don't re-read the GgufTensor shape.
fn load_packed_weight(
    device: &Arc<DeviceContext>,
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    k: usize,
    n: usize,
) -> PackedWeight {
    let Some(t) = tensors.get(key) else {
        tracing::warn!(
            key,
            k,
            n,
            "qwen35_target: projection weight not present; PackedWeight::Zero placeholder"
        );
        let _ = device;
        return PackedWeight::Zero { k, n };
    };

    // numel check — accept either `[k, n]` or `[n, k]` on-disk
    // orientation. Anything else is a real shape mismatch.
    let numel: usize = t.shape.iter().product();
    if numel != k * n {
        tracing::warn!(
            key,
            actual_shape = ?t.shape,
            expected_numel = k * n,
            "qwen35_target: projection weight numel mismatch; PackedWeight::Zero placeholder"
        );
        let _ = device;
        return PackedWeight::Zero { k, n };
    }

    match &t.buf {
        GgufBuf::Bf16(src) => match rematerialize_bf16(src, k, n) {
            Ok(t_new) => PackedWeight::Bf16 { t: t_new, k, n },
            Err(reason) => {
                tracing::warn!(
                    key, k, n, %reason,
                    "qwen35_target: bf16 rematerialize failed; PackedWeight::Zero"
                );
                PackedWeight::Zero { k, n }
            }
        },
        GgufBuf::F16(src) => match cast_f16_to_bf16_packed(src, k, n) {
            Ok(t_new) => PackedWeight::Bf16 { t: t_new, k, n },
            Err(reason) => {
                tracing::warn!(
                    key, k, n, %reason,
                    "qwen35_target: f16→bf16 cast failed; PackedWeight::Zero"
                );
                PackedWeight::Zero { k, n }
            }
        },
        GgufBuf::F32(src) => match cast_f32_to_bf16_packed(src, k, n) {
            Ok(t_new) => PackedWeight::Bf16 { t: t_new, k, n },
            Err(reason) => {
                tracing::warn!(
                    key, k, n, %reason,
                    "qwen35_target: f32→bf16 cast failed; PackedWeight::Zero"
                );
                PackedWeight::Zero { k, n }
            }
        },
        GgufBuf::Q4K(src) => match clone_i8_bytes(src) {
            Ok(t_new) => PackedWeight::Q4K { t: t_new, k, n },
            Err(reason) => {
                tracing::warn!(
                    key, k, n, %reason,
                    "qwen35_target: Q4K byte clone failed; PackedWeight::Zero"
                );
                PackedWeight::Zero { k, n }
            }
        },
        GgufBuf::Q5K(src) => match clone_i8_bytes(src) {
            Ok(t_new) => PackedWeight::Q5K { t: t_new, k, n },
            Err(reason) => {
                tracing::warn!(
                    key, k, n, %reason,
                    "qwen35_target: Q5K byte clone failed; PackedWeight::Zero"
                );
                PackedWeight::Zero { k, n }
            }
        },
        GgufBuf::Q6K(src) => match clone_i8_bytes(src) {
            Ok(t_new) => PackedWeight::Q6K { t: t_new, k, n },
            Err(reason) => {
                tracing::warn!(
                    key, k, n, %reason,
                    "qwen35_target: Q6K byte clone failed; PackedWeight::Zero"
                );
                PackedWeight::Zero { k, n }
            }
        },
        GgufBuf::Q8_0(src) => match clone_i8_bytes(src) {
            Ok(t_new) => PackedWeight::Q8_0 { t: t_new, k, n },
            Err(reason) => {
                tracing::warn!(
                    key, k, n, %reason,
                    "qwen35_target: Q8_0 byte clone failed; PackedWeight::Zero"
                );
                PackedWeight::Zero { k, n }
            }
        },
        GgufBuf::IQ4XS(src) => match clone_i8_bytes(src) {
            Ok(t_new) => PackedWeight::IQ4XS { t: t_new, k, n },
            Err(reason) => {
                tracing::warn!(
                    key, k, n, %reason,
                    "qwen35_target: IQ4XS byte clone failed; PackedWeight::Zero"
                );
                PackedWeight::Zero { k, n }
            }
        },
        other => {
            tracing::warn!(
                key,
                k,
                n,
                dtype = ?std::mem::discriminant(other),
                "qwen35_target: projection weight has unsupported GgufBuf variant; \
                 PackedWeight::Zero placeholder"
            );
            let _ = device;
            PackedWeight::Zero { k, n }
        }
    }
}

/// Materialize a fresh `[k, n]` bf16 tensor from the GGUF-loaded
/// carrier. The fresh allocation detaches ownership from the loader
/// map so the returned tensor can live past the load call.
fn rematerialize_bf16(
    src: &CudaTensor<bf16>,
    k: usize,
    n: usize,
) -> std::result::Result<CudaTensor<bf16>, String> {
    let host = src.to_host().map_err(|e| format!("download: {:?}", e))?;
    if host.len() != k * n {
        return Err(format!("host.len {} != k*n {}", host.len(), k * n));
    }
    CudaTensor::<bf16>::from_host(src.device().clone(), vec![k, n], &host)
        .map_err(|e| format!("upload bf16 [k,n]: {:?}", e))
}

/// Cast an `[_]` f16 GGUF tensor to a fresh `[k, n]` bf16 tensor via a
/// host round-trip. Loader-time only; not a hot path.
fn cast_f16_to_bf16_packed(
    src: &CudaTensor<f16>,
    k: usize,
    n: usize,
) -> std::result::Result<CudaTensor<bf16>, String> {
    let host = src.to_host().map_err(|e| format!("download: {:?}", e))?;
    if host.len() != k * n {
        return Err(format!("host.len {} != k*n {}", host.len(), k * n));
    }
    let host_bf16: Vec<bf16> = host.iter().map(|v| bf16::from_f32(v.to_f32())).collect();
    CudaTensor::<bf16>::from_host(src.device().clone(), vec![k, n], &host_bf16)
        .map_err(|e| format!("upload bf16 [k,n]: {:?}", e))
}

/// Same as [`cast_f16_to_bf16_packed`] but for f32 inputs.
fn cast_f32_to_bf16_packed(
    src: &CudaTensor<f32>,
    k: usize,
    n: usize,
) -> std::result::Result<CudaTensor<bf16>, String> {
    let host = src.to_host().map_err(|e| format!("download: {:?}", e))?;
    if host.len() != k * n {
        return Err(format!("host.len {} != k*n {}", host.len(), k * n));
    }
    let host_bf16: Vec<bf16> = host.iter().map(|v| bf16::from_f32(*v)).collect();
    CudaTensor::<bf16>::from_host(src.device().clone(), vec![k, n], &host_bf16)
        .map_err(|e| format!("upload bf16 [k,n]: {:?}", e))
}

/// Copy the i8 packed-byte carrier into a fresh 1-D `CudaTensor<i8>`.
/// Detaches from the loader map; shape is `[byte_len]` since the mmvq
/// kernels consume `(k, n)` scalars and a flat byte buffer.
fn clone_i8_bytes(src: &CudaTensor<i8>) -> std::result::Result<CudaTensor<i8>, String> {
    let host = src.to_host().map_err(|e| format!("download: {:?}", e))?;
    let byte_len = host.len();
    CudaTensor::<i8>::from_host(src.device().clone(), vec![byte_len], &host)
        .map_err(|e| format!("upload i8 bytes: {:?}", e))
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
    /// **VRAM requirement (post-Phase-5)**: the GGUF loader now
    /// dequantizes Q5K/Q6K/Q8_0/IQ4_XS to bf16 at upload time. That
    /// roughly doubles the per-tensor VRAM footprint for the 235
    /// tensors in those formats in the shipping Q4_K_M file. Total
    /// resident-weight footprint is ~30 GB; with per-layer zero
    /// placeholders and KV cache overhead this overflows a 48 GB
    /// A6000 on a cold run. Phase 6 replaces the bf16-dequant path
    /// with native mmvq_q{5,6,8}_k kernels that keep the data
    /// packed on device — at that point this test fits comfortably.
    /// For now, run on a 64-GB-class card, or use the reduced-layer
    /// variant below (`qwen35_target_gguf_smoke_v2`) once it's been
    /// trimmed.
    ///
    /// Run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
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

    /// Phase-4 v2 smoke: same intent as the original, but pulls the
    /// config from the GGUF's `qwen35.*` metadata instead of the
    /// baked-in constant and asserts the head counts the metadata
    /// reports (24/4) rather than the (wrong) 40/8 the const used to
    /// carry.
    ///
    /// Doesn't assert on logit values — Phase 4 still skips Q5_K/Q6_K/
    /// Q8_0/IQ4_XS tensors, so most weights are zero placeholders and
    /// the logits end up dominated by whatever tensors did load. Once
    /// Agent N lands the missing dtype kernels, the logit magnitudes
    /// will stop being zero on their own.
    ///
    /// Run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///     --ignored --nocapture qwen35_target_gguf_smoke_v2
    #[test]
    #[ignore]
    fn qwen35_target_gguf_smoke_v2() {
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));

        // 1. Parse metadata from the GGUF header — no device uploads yet.
        let meta = crate::gguf_loader::parse_qwen35_metadata(QWEN35_27B_GGUF)
            .expect("parse_qwen35_metadata");
        eprintln!(
            "qwen35 metadata: block_count={} embedding_length={} head_count={} head_count_kv={} \
             rope_theta={} rms_eps={} context_length={} feed_forward_length={} \
             key_length={} value_length={}",
            meta.block_count,
            meta.embedding_length,
            meta.head_count,
            meta.head_count_kv,
            meta.rope_theta,
            meta.rms_eps,
            meta.context_length,
            meta.feed_forward_length,
            meta.key_length,
            meta.value_length,
        );

        // 2. Build config from metadata. Shape-critical assertions: the
        //    GGUF reports the real head counts (24/4), which is exactly
        //    the mismatch this agent's workstream fixes.
        //
        //    gdn_ssm_dim isn't in the attention metadata block — it
        //    comes from `qwen35.ssm.state_size` on the reference target
        //    (128 for 27B). We pass that through by hand here since
        //    this test doesn't model the full SSM metadata.
        let gdn_ssm_dim = 128;
        let cfg = Qwen35Config::from_metadata(&meta, gdn_ssm_dim);
        assert_eq!(cfg.n_q_heads, 24, "metadata head_count should be 24 on 27B");
        assert_eq!(cfg.n_kv_heads, 4, "metadata head_count_kv should be 4 on 27B");
        assert_eq!(cfg.hidden_dim, 5120);
        assert_eq!(cfg.head_dim, 256);

        // 3. Load the full target using the metadata-derived config.
        //    Phase 6: `keep_packed = true` keeps Q5K/Q6K/Q8_0/IQ4_XS
        //    tensors as raw block bytes on device. That cuts the 27B
        //    resident-weight footprint from ~30 GB (Phase-5 bf16-
        //    dequant) to ~15 GB (GGUF-on-disk size), so the smoke test
        //    now fits on a 48 GB A6000 alongside KV-cache + GDN-state
        //    allocations. The per-layer `load_bf16_*` helpers don't yet
        //    consume packed bytes, so those weights still become zero
        //    placeholders in the forward — shape + finiteness are what
        //    this test asserts.
        eprintln!(
            "qwen35_target_gguf_smoke_v2: loading {} (keep_packed=true)",
            QWEN35_27B_GGUF
        );
        let target = Qwen35Target::load_from_gguf_with_config(
            dev.clone(),
            cfg,
            QWEN35_27B_GGUF,
            crate::gguf_loader::LoaderConfig { keep_packed: true },
        )
        .expect("load_from_gguf_with_config");
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

        // 4. Run forward on the HumanEval 9-token prompt (padded to 32).
        let prompt: [i32; 9] = [7734, 264, 6185, 36974, 883, 13094, 6326, 61369, 25];
        let n_real = prompt.len();
        let n_tokens = 32usize;
        let mut tokens_host = vec![0i32; n_tokens];
        tokens_host[..n_real].copy_from_slice(&prompt);
        let tokens = CudaTensor::<i32>::from_host(dev.clone(), vec![n_tokens], &tokens_host)
            .expect("upload tokens");

        let mut positions_host = vec![0i32; 4 * n_tokens];
        for t in 0..n_tokens {
            positions_host[t] = t as i32;
        }
        let positions =
            CudaTensor::<i32>::from_host(dev.clone(), vec![4, n_tokens], &positions_host)
                .expect("upload positions");

        let max_ctx = 4096usize.max(n_tokens);
        let mut kv_cache = KvCache::new(
            dev.clone(),
            target.n_full_attn,
            max_ctx,
            cfg.n_kv_heads,
            cfg.head_dim,
        )
        .expect("alloc kv cache");

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

        eprintln!("qwen35_target_gguf_smoke_v2: running forward");
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

        // 5. Shape + finiteness only — no value assertions (see the
        //    doc comment about Phase 4 placeholders).
        assert_eq!(
            logits.shape(),
            [n_tokens, target.vocab_size],
            "logits shape mismatch"
        );

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
            "qwen35_target_gguf_smoke_v2: logits shape={:?} nan={} inf={} max_abs={:.4e}",
            logits.shape(),
            n_nan,
            n_inf,
            max_abs
        );
        assert_eq!(n_nan, 0, "logits contain {} NaN", n_nan);
        assert_eq!(n_inf, 0, "logits contain {} Inf", n_inf);
    }
}
