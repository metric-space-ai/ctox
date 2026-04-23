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
//!   attn_qkv.weight                [hidden, 10240]      Q5_K  (q||k||v fused)
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
    launch_cast_bf16_to_f32, launch_embedding_bf16, launch_embedding_f16, launch_embedding_f32,
    launch_rmsnorm_f32,
};
use ctox_cuda_primitives::kv_cache::KvCache;
use ctox_cuda_primitives::tensor::CudaTensor;

use crate::config::Qwen35Config;
use crate::layers::ffn::Qwen35FFN;
use crate::layers::full_attention::Qwen35FullAttention;
use crate::layers::gdn::Qwen35GDN;
use crate::layers::packed_weight::PackedWeight;

/// Attention sub-block variant within one decoder layer.
///
/// Reference: `qwen35_target_graph.cpp::build_qwen35_graph` — every
/// layer runs pre-norm + (FA | GDN) + residual, then post-norm + SwiGLU
/// FFN + residual. Both sub-blocks are mandatory on every layer; only
/// the attention variant alternates (FA on layers where
/// `(i+1) % 4 == 0`, else GDN).
pub enum Qwen35Attention {
    FullAttention(Qwen35FullAttention),
    Gdn(Qwen35GDN),
}

/// One full decoder layer: attention sub-block + FFN sub-block.
///
/// The reference applies BOTH sub-blocks on every layer — attention/GDN
/// then SwiGLU FFN — with residual adds around each. A port that skips
/// the FFN loses every layer's SwiGLU projection and collapses the
/// network to near-identity transforms, which is why the pre-FFN port
/// was emitting a constant argmax regardless of context.
pub struct Qwen35Layer {
    pub attn: Qwen35Attention,
    pub ffn: Qwen35FFN,
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
    /// `[hidden, vocab]` — lm head projection weight. Carrier dtype
    /// depends on the GGUF shipped type (Q6_K on 27B Q4_K_M).
    /// Dispatch happens inside [`PackedWeight::matmul_f32`].
    pub lm_head: PackedWeight,
    /// Total number of tokens in the vocabulary. Cached for
    /// convenience since the embed tensor carries it as `shape()[0]`.
    pub vocab_size: usize,
    /// Number of FullAttention layers (for sizing a KvCache).
    pub n_full_attn: usize,
    /// Number of GDN layers (for sizing the gdn_state vector).
    pub n_gdn: usize,
    /// Optional feature-capture buffer shared with a draft model for
    /// DFlash-style speculative decoding. Shape
    /// `[max_ctx, CAPTURE_LAYERS.len() * hidden_dim]` bf16 when
    /// allocated. Populated during [`Self::forward_with_capture`] at
    /// each position `kv_start + i` for the layer indices listed in
    /// [`CAPTURE_LAYERS`]; consumed by
    /// [`crate::spec_decode::SpeculativeDecoder`] as the draft's
    /// `target_hidden_cat` cross-attention feature source.
    ///
    /// Sized for the whole context (no ring wrap) — a ring variant can
    /// be added later to cap the resident footprint at 128K tokens.
    pub target_feat_buf: Option<CudaTensor<bf16>>,
    pub device: Arc<DeviceContext>,
}

/// Target layer indices whose post-FFN hidden state feeds the draft
/// model's `target_hidden_cat` cross-attention features. Matches the
/// reference's `qwen35_target_graph.cpp::CAPTURE_LAYERS`.
pub const CAPTURE_LAYERS: [usize; 5] = [1, 16, 31, 46, 61];

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

        // `output.weight` → lm_head. On the production 27B GGUF this
        // is Q6_K packed bytes; Agent C's full GgufBuf → PackedWeight
        // dispatch now produces `PackedWeight::Q6K`, which the forward
        // path routes through `launch_mmvq_q6k_f32`. If the weight is
        // missing (some checkpoints tie lm_head to the embedding), the
        // dispatch falls back to a Zero placeholder with a tracing
        // warning, and logits come out all-zeros.
        //
        // TODO(future): materialize tied weights by transposing the
        // embedding when `output.weight` is missing, via a device-side
        // transpose kernel. Until then, 27B runs (which ship a real
        // `output.weight`) see a real lm_head.
        let lm_head = load_packed_weight(
            &device,
            &tensors,
            "output.weight",
            config.hidden_dim,
            vocab_size,
        );
        tracing::info!(
            variant = packed_variant_name(&lm_head),
            dims = ?lm_head.dims(),
            "qwen35_target: lm_head loaded"
        );

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
            let attn = if is_fa {
                let layer =
                    build_full_attention_layer(&device, &config, &tensors, l, n_full_attn);
                n_full_attn += 1;
                Qwen35Attention::FullAttention(layer)
            } else {
                let layer = build_gdn_layer(&device, &config, &tensors, l);
                n_gdn += 1;
                Qwen35Attention::Gdn(layer)
            };
            let ffn = build_ffn_layer(&device, &config, &tensors, l);
            layers.push(Qwen35Layer { attn, ffn });
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
            target_feat_buf: None,
            device,
        })
    }

    /// Allocate (or reuse) a feature-capture buffer sized for
    /// `max_ctx` tokens. Shape `[max_ctx, CAPTURE_LAYERS.len() * hidden]`
    /// bf16. Idempotent if `self.target_feat_buf` is already sized at
    /// least `max_ctx`.
    ///
    /// Call once during speculative-decode setup; afterwards
    /// [`Self::forward_with_capture`] writes into the buffer as the
    /// target consumes prompt / verify tokens.
    pub fn ensure_capture_buf(&mut self, max_ctx: usize) -> Result<()> {
        let feat_dim = CAPTURE_LAYERS.len() * self.config.hidden_dim;
        let needs_alloc = match &self.target_feat_buf {
            Some(t) => t.shape().first().copied().unwrap_or(0) < max_ctx,
            None => true,
        };
        if needs_alloc {
            self.target_feat_buf = Some(CudaTensor::<bf16>::zeros(
                self.device.clone(),
                vec![max_ctx, feat_dim],
            )?);
        }
        Ok(())
    }

    /// Read-only accessor for the capture buffer. Used by the
    /// speculative decoder to build `target_hidden_cat` slices.
    pub fn capture_buf(&self) -> Option<&CudaTensor<bf16>> {
        self.target_feat_buf.as_ref()
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
        gdn_conv_states: &mut [CudaTensor<f32>],
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
        if gdn_conv_states.len() < self.n_gdn {
            return Err(anyhow!(
                "qwen35 target.forward: gdn_conv_states.len()={} < n_gdn={}",
                gdn_conv_states.len(),
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
            match &layer.attn {
                Qwen35Attention::FullAttention(fa) => {
                    fa.forward(&self.device, &mut hidden, positions, kv_cache)?;
                    fa_seen += 1;
                    // Rewind unless this is the final FA layer.
                    if fa_seen < fa_total {
                        kv_cache.rewind(n_tokens);
                    }
                }
                Qwen35Attention::Gdn(gdn) => {
                    gdn.forward(
                        &self.device,
                        &mut hidden,
                        positions,
                        &mut gdn_states[gdn_idx],
                        &mut gdn_inter[gdn_idx],
                        &mut gdn_conv_states[gdn_idx],
                    )?;
                    gdn_idx += 1;
                }
            }
            // SwiGLU FFN sub-block runs on every layer (FA and GDN
            // alike) after the attention residual add. Reference:
            // `qwen35_target_graph.cpp::build_qwen35_graph` lines
            // 736-742 — `rms_norm_mul(cur, attn_post_norm)` → SwiGLU
            // → `cur = ggml_add(ffn, ffn_residual)`. `Qwen35FFN::
            // forward` bundles the pre-norm + SwiGLU + residual.
            layer.ffn.forward(&self.device, &mut hidden)?;
        }

        // ── 3. Final norm (f32). Keep the output in f32 — the
        //      PackedWeight lm_head dispatch takes f32 in / f32 out,
        //      so we don't need the bf16 round-trip the pre-Agent-C
        //      path used.
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

        // ── 4. LM head: logits = norm · lm_head → [n_tokens, vocab] f32.
        //      Alignment constraints come from the underlying bf16 gemm
        //      kernel (for PackedWeight::Bf16); the per-row mmvq path
        //      (for Q*_K / Q8_0 / IQ4_XS) doesn't need them but we
        //      preserve the check so callers see a uniform contract.
let mut logits =
            CudaTensor::<f32>::zeros(self.device.clone(), vec![n_tokens, self.vocab_size])?;
        self.lm_head.matmul_f32(&self.device, &norm_f32, &mut logits)?;

        Ok(logits)
    }

    /// Diagnostic single-chunk forward with per-layer hidden-state
    /// statistics printed to stderr.
    ///
    /// Same math as [`Self::forward`] but after every decoder layer it
    /// downloads the current bf16 hidden tensor, converts to f32 on the
    /// host, and prints per-layer L2 norm and absmax. Used exclusively
    /// by the `qwen35-fwd-diag` binary to localize the first layer that
    /// produces anomalous activations versus the reference.
    ///
    /// Returns the full `[n_tokens, vocab_size]` logits tensor — callers
    /// typically argmax the final row.
    pub fn forward_diag(
        &self,
        tokens: &CudaTensor<i32>,
        positions: &CudaTensor<i32>,
        kv_cache: &mut KvCache,
        gdn_states: &mut [CudaTensor<f32>],
        gdn_inter: &mut [CudaTensor<f16>],
        gdn_conv_states: &mut [CudaTensor<f32>],
    ) -> Result<CudaTensor<f32>> {
        if tokens.shape().len() != 1 {
            return Err(anyhow!(
                "qwen35 target.forward_diag: tokens must be 1D, got {:?}",
                tokens.shape()
            ));
        }
        let n_tokens = tokens.shape()[0];
        if n_tokens == 0 {
            return Err(anyhow!("qwen35 target.forward_diag: n_tokens must be > 0"));
        }
        let hidden_dim = self.config.hidden_dim;

        let mut hidden =
            CudaTensor::<bf16>::zeros(self.device.clone(), vec![n_tokens, hidden_dim])?;
        launch_embedding_bf16(&self.device, &self.embed, tokens, &mut hidden)?;

        // Dump L2 of the raw embedding lookup for the last row only —
        // this is the input to layer 0 and should be non-zero and finite.
        let (l2_embed, amax_embed) = bf16_last_row_l2_and_absmax(&hidden, n_tokens, hidden_dim)?;
        eprintln!(
            "DIAG L2[embed] last_row_l2={:.6e} absmax={:.6e}",
            l2_embed, amax_embed
        );

        let fa_total = self.n_full_attn;
        let mut fa_seen: usize = 0;
        let mut gdn_idx: usize = 0;

        for (il, layer) in self.layers.iter().enumerate() {
            let is_fa = matches!(layer.attn, Qwen35Attention::FullAttention(_));
            match &layer.attn {
                Qwen35Attention::FullAttention(fa) => {
                    fa.forward(&self.device, &mut hidden, positions, kv_cache)?;
                    fa_seen += 1;
                    if fa_seen < fa_total {
                        kv_cache.rewind(n_tokens);
                    }
                }
                Qwen35Attention::Gdn(gdn) => {
                    gdn.forward(
                        &self.device,
                        &mut hidden,
                        positions,
                        &mut gdn_states[gdn_idx],
                        &mut gdn_inter[gdn_idx],
                        &mut gdn_conv_states[gdn_idx],
                    )?;
                    gdn_idx += 1;
                }
            }
            let (l2_attn, amax_attn) =
                bf16_last_row_l2_and_absmax(&hidden, n_tokens, hidden_dim)?;
            layer.ffn.forward(&self.device, &mut hidden)?;
            let (l2_post, amax_post) =
                bf16_last_row_l2_and_absmax(&hidden, n_tokens, hidden_dim)?;
            eprintln!(
                "DIAG L2[{:02}] {} post_attn_l2={:.6e} amax={:.6e} post_ffn_l2={:.6e} amax={:.6e}",
                il,
                if is_fa { "FA " } else { "GDN" },
                l2_attn,
                amax_attn,
                l2_post,
                amax_post,
            );
        }

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

        let mut logits =
            CudaTensor::<f32>::zeros(self.device.clone(), vec![n_tokens, self.vocab_size])?;
        self.lm_head.matmul_f32(&self.device, &norm_f32, &mut logits)?;
        Ok(logits)
    }

    /// [`Self::forward`] with per-layer feature capture. Runs the
    /// same layer loop but, after each of the layer indices in
    /// [`CAPTURE_LAYERS`] has executed, copies its post-FFN hidden-state
    /// rows into `self.target_feat_buf` at positions
    /// `[kv_start..kv_start + n_tokens]`, column offset
    /// `capture_idx * hidden`. The capture buffer is the draft model's
    /// `target_hidden_cat` cross-attention feature source.
    ///
    /// Call [`Self::ensure_capture_buf`] once at spec-decode setup
    /// before the first `forward_with_capture` call; without it this
    /// method errors.
    ///
    /// `kv_start` must match the KV cache's `n_filled` BEFORE the
    /// call — i.e. the absolute token-position of `tokens[0]`. The
    /// speculative decoder passes `kv_cache.n_filled()` here.
    ///
    /// Correctness-first port: capture writes use one
    /// `memcpy_dtod` per (capture_layer, token) pair — 5 × n_tokens
    /// small launches. A single strided-copy kernel is a future
    /// perf pass.
    pub fn forward_with_capture(
        &mut self,
        tokens: &CudaTensor<i32>,
        positions: &CudaTensor<i32>,
        kv_cache: &mut KvCache,
        gdn_states: &mut [CudaTensor<f32>],
        gdn_inter: &mut [CudaTensor<f16>],
        gdn_conv_states: &mut [CudaTensor<f32>],
        kv_start: usize,
    ) -> Result<CudaTensor<f32>> {
        // ── 0. Shape validation. Same checks as `forward`.
        if tokens.shape().len() != 1 {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: tokens must be 1D [n_tokens], got {:?}",
                tokens.shape()
            ));
        }
        let n_tokens = tokens.shape()[0];
        if n_tokens == 0 {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: n_tokens must be > 0"
            ));
        }
        let hidden_dim = self.config.hidden_dim;
        if positions.shape() != [4, n_tokens] {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: positions shape {:?} != [4, {}]",
                positions.shape(),
                n_tokens
            ));
        }
        if gdn_states.len() < self.n_gdn {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: gdn_states.len()={} < n_gdn={}",
                gdn_states.len(),
                self.n_gdn
            ));
        }
        if gdn_inter.len() < self.n_gdn {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: gdn_inter.len()={} < n_gdn={}",
                gdn_inter.len(),
                self.n_gdn
            ));
        }
        if gdn_conv_states.len() < self.n_gdn {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: gdn_conv_states.len()={} < n_gdn={}",
                gdn_conv_states.len(),
                self.n_gdn
            ));
        }
        if kv_cache.n_layers() < self.n_full_attn {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: kv_cache has {} layers, need >= {} FA layers",
                kv_cache.n_layers(),
                self.n_full_attn
            ));
        }
        if self.target_feat_buf.is_none() {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: target_feat_buf not allocated — \
                 call ensure_capture_buf first"
            ));
        }
        let feat_dim = CAPTURE_LAYERS.len() * hidden_dim;
        let feat_cap = self
            .target_feat_buf
            .as_ref()
            .map(|t| t.shape().first().copied().unwrap_or(0))
            .unwrap_or(0);
        if kv_start + n_tokens > feat_cap {
            return Err(anyhow!(
                "qwen35 target.forward_with_capture: kv_start+n_tokens={} exceeds \
                 target_feat_buf capacity {}",
                kv_start + n_tokens,
                feat_cap
            ));
        }
        {
            let expected_last = CAPTURE_LAYERS.iter().copied().max().unwrap_or(0);
            if self.layers.len() <= expected_last {
                return Err(anyhow!(
                    "qwen35 target.forward_with_capture: model has {} layers but \
                     CAPTURE_LAYERS needs index {}",
                    self.layers.len(),
                    expected_last
                ));
            }
        }

        // ── 1. Embedding lookup.
        let mut hidden =
            CudaTensor::<bf16>::zeros(self.device.clone(), vec![n_tokens, hidden_dim])?;
        launch_embedding_bf16(&self.device, &self.embed, tokens, &mut hidden)?;

        // ── 2. Layer loop with capture.
        let fa_total = self.n_full_attn;
        let mut fa_seen: usize = 0;
        let mut gdn_idx: usize = 0;

        let stream = self.device.raw().default_stream();
        // Element size inside target_feat_buf (bf16 = 2 bytes per elem,
        // but cudarc slice APIs are typed so we operate in element
        // counts, not bytes).
        let hidden_elems = hidden_dim; // elements per (capture_idx, position) strip.

        for (il, layer) in self.layers.iter().enumerate() {
            match &layer.attn {
                Qwen35Attention::FullAttention(fa) => {
                    fa.forward(&self.device, &mut hidden, positions, kv_cache)?;
                    fa_seen += 1;
                    if fa_seen < fa_total {
                        kv_cache.rewind(n_tokens);
                    }
                }
                Qwen35Attention::Gdn(gdn) => {
                    gdn.forward(
                        &self.device,
                        &mut hidden,
                        positions,
                        &mut gdn_states[gdn_idx],
                        &mut gdn_inter[gdn_idx],
                        &mut gdn_conv_states[gdn_idx],
                    )?;
                    gdn_idx += 1;
                }
            }
            // SwiGLU FFN — same contract as the non-capture path. We
            // capture AFTER the FFN residual so the `target_feat_buf`
            // content matches the reference's `cur` at loop-tail
            // (post-FFN, post-residual), which is what the draft's
            // cross-attention consumes in
            // `spec_decode::build_target_hidden_cat`.
            layer.ffn.forward(&self.device, &mut hidden)?;

            // Capture hook: after running layer `il`, if it's one of
            // CAPTURE_LAYERS, blit the current hidden into the buffer.
            let capture_idx = CAPTURE_LAYERS.iter().position(|&c| c == il);
            if let Some(k) = capture_idx {
                // Destination columns for this capture: [k*hidden_dim,
                // (k+1)*hidden_dim) inside each row of length
                // `feat_dim`.
                let feat_buf = self
                    .target_feat_buf
                    .as_mut()
                    .expect("target_feat_buf: checked above");
                let hidden_src = hidden.buf();
                let dst_buf = feat_buf.buf_mut();
                for t in 0..n_tokens {
                    let src_start = t * hidden_elems;
                    let src_end = src_start + hidden_elems;
                    let dst_row = kv_start + t;
                    let dst_start = dst_row * feat_dim + k * hidden_elems;
                    let dst_end = dst_start + hidden_elems;
                    let src_view = hidden_src.slice(src_start..src_end);
                    let mut dst_view = dst_buf.slice_mut(dst_start..dst_end);
                    stream.memcpy_dtod(&src_view, &mut dst_view).map_err(|e| {
                        anyhow!(
                            "qwen35 target.forward_with_capture: feat capture \
                             memcpy_dtod (layer={} capture_idx={} tok={}): {:?}",
                            il,
                            k,
                            t,
                            e
                        )
                    })?;
                }
            }
        }

        // ── 3. Final norm.
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

        // ── 4. LM head.
        let mut logits =
            CudaTensor::<f32>::zeros(self.device.clone(), vec![n_tokens, self.vocab_size])?;
        self.lm_head.matmul_f32(&self.device, &norm_f32, &mut logits)?;

        Ok(logits)
    }

    /// Snapshot a single GDN recurrent state into a new tensor.
    ///
    /// Correctness-first: allocates a fresh `CudaTensor<f32>` with the
    /// same shape and issues a device-to-device memcpy. For the 48-GDN
    /// 27B target this is 48 × `[S_v, S_v, H_v, 1]` ≈ 48 × 128×128×48
    /// f32 ≈ 144 MB per snapshot, which is fine in absolute terms but
    /// adds ~144 MB of allocation churn per spec-decode step.
    ///
    /// TODO(perf): replace with a single preallocated shadow buffer
    /// + in-place memcpy — no alloc per step. See
    /// `test_dflash_lib.cpp::snapshot_ssm_state` for the cached-shadow
    /// pattern we want to mirror. Day-one port prefers bit-accurate
    /// semantics over allocation count.
    pub fn snapshot_gdn_states(
        &self,
        gdn_states: &[CudaTensor<f32>],
    ) -> Result<Vec<CudaTensor<f32>>> {
        let stream = self.device.raw().default_stream();
        let mut out: Vec<CudaTensor<f32>> = Vec::with_capacity(gdn_states.len());
        for (i, src) in gdn_states.iter().enumerate() {
            let mut dst =
                CudaTensor::<f32>::zeros(self.device.clone(), src.shape().to_vec())?;
            stream
                .memcpy_dtod(src.buf(), dst.buf_mut())
                .map_err(|e| anyhow!("snapshot_gdn_states: layer {}: {:?}", i, e))?;
            out.push(dst);
        }
        Ok(out)
    }

    /// Restore GDN recurrent states from a snapshot taken by
    /// [`Self::snapshot_gdn_states`]. Counterpart to that call: each
    /// entry in `snapshot` memcpy's into the matching entry in
    /// `gdn_states`. Shapes must match.
    pub fn restore_gdn_states(
        &self,
        gdn_states: &mut [CudaTensor<f32>],
        snapshot: &[CudaTensor<f32>],
    ) -> Result<()> {
        if gdn_states.len() != snapshot.len() {
            return Err(anyhow!(
                "restore_gdn_states: gdn_states.len()={} != snapshot.len()={}",
                gdn_states.len(),
                snapshot.len()
            ));
        }
        let stream = self.device.raw().default_stream();
        for (i, (dst, src)) in gdn_states.iter_mut().zip(snapshot.iter()).enumerate() {
            if dst.shape() != src.shape() {
                return Err(anyhow!(
                    "restore_gdn_states: layer {} shape {:?} != snapshot {:?}",
                    i,
                    dst.shape(),
                    src.shape()
                ));
            }
            stream
                .memcpy_dtod(src.buf(), dst.buf_mut())
                .map_err(|e| anyhow!("restore_gdn_states: layer {}: {:?}", i, e))?;
        }
        Ok(())
    }

    /// Chunked prefill matching dflash's reference implementation in
    /// `test_dflash_lib.cpp::run_dflash_gen_loop` (the `// ── Prefill:`
    /// section). Walks the prompt in ubatches of up to
    /// [`Self::prefill_ubatch_for`] tokens — 16 for prompts ≤ 2048, 192
    /// otherwise — and for each chunk runs a full [`Self::forward`]
    /// which advances the KV cache and the recurrent GDN state.
    ///
    /// # Why chunked?
    ///
    /// The GDN layer's `gdn_inter` buffer has shape
    /// `[S_v, S_v, H, n_tokens]` and is sized per forward call. Calling
    /// [`Self::forward`] in one shot with a full prompt of length N
    /// would allocate `O(N * S_v^2 * H)` bytes per GDN layer (~22 MB
    /// per layer at N=8192, S_v=128, H=48), which OOMs long before the
    /// 128K context target. The reference dodges this by running
    /// prefill in 192-token ubatches, sizing `gdn_inter` for the chunk
    /// only, and relying on the GDN layer's in-place update of
    /// `gdn_state` to carry the recurrent state across chunks.
    ///
    /// # Inputs
    /// * `prompt_tokens` — `[prompt_len]` i32. Must be on device.
    /// * `kv_cache` — fresh (or reset) KV cache. Advanced by
    ///    `prompt_len` over the course of this call.
    /// * `gdn_states` — one `[S_v, S_v, H, 1]` f32 tensor per GDN
    ///    layer, updated in-place by each chunk's forward.
    ///
    /// # Output
    /// `[1, vocab_size]` f32 logits for the final prompt position
    /// (seeds the greedy/sampled first decode step).
    ///
    /// # Discipline
    /// * Chunk boundary == KV advance boundary: `kv_cache.n_filled`
    ///   advances by `chunk_size` per chunk.
    /// * `gdn_states` persist across chunks (SSM recurrent state from
    ///   chunk N is input to chunk N+1).
    /// * `gdn_inter` is allocated *inside* this function, sized
    ///   `[S_v, S_v, H, PREFILL_UBATCH]`, and reused across chunks.
    ///   It is never sized for the whole prompt.
    pub fn prefill(
        &self,
        prompt_tokens: &CudaTensor<i32>,
        kv_cache: &mut KvCache,
        gdn_states: &mut [CudaTensor<f32>],
        gdn_conv_states: &mut [CudaTensor<f32>],
    ) -> Result<CudaTensor<f32>> {
        if prompt_tokens.shape().len() != 1 {
            return Err(anyhow!(
                "qwen35 target.prefill: prompt_tokens must be 1D [prompt_len], got {:?}",
                prompt_tokens.shape()
            ));
        }
        let prompt_len = prompt_tokens.shape()[0];
        if prompt_len == 0 {
            return Err(anyhow!("qwen35 target.prefill: prompt_len must be > 0"));
        }
        if gdn_states.len() < self.n_gdn {
            return Err(anyhow!(
                "qwen35 target.prefill: gdn_states.len()={} < n_gdn={}",
                gdn_states.len(),
                self.n_gdn
            ));
        }
        if gdn_conv_states.len() < self.n_gdn {
            return Err(anyhow!(
                "qwen35 target.prefill: gdn_conv_states.len()={} < n_gdn={}",
                gdn_conv_states.len(),
                self.n_gdn
            ));
        }
        if kv_cache.n_layers() < self.n_full_attn {
            return Err(anyhow!(
                "qwen35 target.prefill: kv_cache has {} layers, need >= {} FA layers",
                kv_cache.n_layers(),
                self.n_full_attn
            ));
        }

        let ubatch = Self::prefill_ubatch_for(prompt_len);

        // Per-chunk gdn_inter buffer, sized for the ubatch (never the
        // full prompt). Matches dflash-ref's bounded intermediate-state
        // region.
        let cfg = self.config;
        let s_v = cfg.gdn_ssm_dim;
        let h_v = cfg.gdn_num_v_heads;
        let mut gdn_inter: Vec<CudaTensor<f16>> = Vec::with_capacity(self.n_gdn);
        for _ in 0..self.n_gdn {
            gdn_inter.push(CudaTensor::<f16>::zeros(
                self.device.clone(),
                vec![s_v, s_v, h_v, ubatch],
            )?);
        }

        // Pull the prompt into host memory once so we can slice per
        // chunk without issuing device→device slice copies (prompt
        // tokens are i32 and small — for a 16K prompt this is 64 KB).
        let prompt_host = prompt_tokens
            .to_host()
            .map_err(|e| anyhow!("qwen35 target.prefill: download prompt: {:?}", e))?;

        // Walk the prompt in chunks. After each chunk the KV cache is
        // advanced by chunk_n inside the FA layers' forward, and the
        // GDN recurrent state is updated in-place inside the GDN
        // layers' forward. The per-chunk logits are discarded except
        // for the final chunk's last row.
        let mut last_logits: Option<CudaTensor<f32>> = None;
        let mut start: usize = 0;
        while start < prompt_len {
            let chunk_n = std::cmp::min(ubatch, prompt_len - start);

            // Slice prompt tokens for this chunk.
            let tk = CudaTensor::<i32>::from_host(
                self.device.clone(),
                vec![chunk_n],
                &prompt_host[start..start + chunk_n],
            )
            .map_err(|e| anyhow!("qwen35 target.prefill: upload chunk tokens: {:?}", e))?;

            // MRoPE 4D positions: first 3 axes = absolute position
            // (start + i), axis 3 = 0 for plain text. Matches
            // dflash-ref's `pf_pos_buf` layout.
            let mut pos_host = vec![0i32; 4 * chunk_n];
            for i in 0..chunk_n {
                let p = (start + i) as i32;
                pos_host[i] = p;
                pos_host[chunk_n + i] = p;
                pos_host[2 * chunk_n + i] = p;
                // pos_host[3 * chunk_n + i] already 0.
            }
            let pos = CudaTensor::<i32>::from_host(
                self.device.clone(),
                vec![4, chunk_n],
                &pos_host,
            )
            .map_err(|e| anyhow!("qwen35 target.prefill: upload chunk positions: {:?}", e))?;

            let logits = self
                .forward(&tk, &pos, kv_cache, gdn_states, &mut gdn_inter, gdn_conv_states)
                .map_err(|e| {
                    anyhow!(
                        "qwen35 target.prefill: chunk forward start={} chunk_n={}: {:?}",
                        start,
                        chunk_n,
                        e
                    )
                })?;
            last_logits = Some(logits);
            start += chunk_n;
        }

        // Pull the last chunk's final-row logits and repackage as
        // `[1, vocab_size]` for the caller.
        let logits = last_logits
            .ok_or_else(|| anyhow!("qwen35 target.prefill: no chunks were run"))?;
        let shape = logits.shape().to_vec();
        if shape.len() != 2 || shape[1] != self.vocab_size {
            return Err(anyhow!(
                "qwen35 target.prefill: chunk logits shape {:?} not [_, {}]",
                shape,
                self.vocab_size
            ));
        }
        let chunk_n = shape[0];
        let vocab = self.vocab_size;
        let host = logits
            .to_host()
            .map_err(|e| anyhow!("qwen35 target.prefill: download chunk logits: {:?}", e))?;
        let last_row_start = (chunk_n - 1) * vocab;
        let last_row = &host[last_row_start..last_row_start + vocab];
        let out = CudaTensor::<f32>::from_host(self.device.clone(), vec![1, vocab], last_row)
            .map_err(|e| anyhow!("qwen35 target.prefill: upload last-row logits: {:?}", e))?;
        Ok(out)
    }

    /// Returns the prefill ubatch size to use for a given prompt
    /// length. Matches dflash-ref's
    /// `test_dflash_lib.cpp::run_dflash_gen_loop`:
    ///
    /// ```text
    /// int prefill_ubatch_env = (prompt_len_auto > 2048) ? 192 : 16;
    /// ```
    ///
    /// 16 for prompts ≤ 2048 (matches the DFlash block_size and
    /// chain-verify q_len so per-chunk FA drift is smallest); 192 for
    /// longer prompts where prefill time dominates. 192 is also the
    /// ceiling dictated by the gated_delta_net intermediate-state OOM
    /// envelope; see the comment above `PREFILL_UBATCH` in
    /// `test_dflash_lib.cpp`.
    #[inline]
    pub fn prefill_ubatch_for(prompt_len: usize) -> usize {
        if prompt_len > 2048 {
            192
        } else {
            16
        }
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
    // Q, second half is the sigmoid gate. `load_packed_halves` returns
    // two `PackedWeight`s with whatever carrier the GGUF shipped
    // (Bf16 for synthetic test fixtures, Q4K/Q5K/Q6K/Q8_0/IQ4XS for
    // real-model block-quant layouts). When the tensor is missing, has
    // the wrong shape/numel, or has an unsupported variant, both halves
    // fall back to `PackedWeight::Zero` placeholders so the forward
    // still runs.
    let (w_q, w_q_gate) = match load_packed_halves(
        tensors,
        &name_q,
        cfg.hidden_dim,
        cfg.q_dim(),
    ) {
        Ok(pair) => pair,
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
/// GGUF stores the packed Q/gate tensor logically as `[hidden, 2*q_dim]`;
/// dflash splits on the output axis so that element `[i, j]` for
/// `j in 0..q_dim` is the Q component and `j in q_dim..2*q_dim` is the
/// gate component.
///
/// Carrier-specific layouts at the byte level:
///
///   * **Bf16** (test fixtures, hand-authored checkpoints). Device
///     tensor shape is `[hidden, 2*q_dim]` row-major over the logical
///     `[in, out]`. Splitting is a per-row strided copy (first `q_dim`
///     elements of each row go to Q, last `q_dim` to gate).
///
///   * **Q4K / Q5K / Q6K / Q8_0 / IQ4XS** (production 27B). The mmvq
///     kernels consume block-quant bytes laid out as `[n, k]` row-major
///     — i.e. for each of `n = 2*q_dim` output rows, a contiguous run
///     of `(k/block_elems) * block_bytes` block-quant bytes over the
///     `k = hidden` reduction axis. The llama.cpp loader reverses the
///     on-disk ggml `[in=hidden, out=2*q_dim]` shape to row-major
///     `[out=2*q_dim, in=hidden]`, which matches the mmvq expectation
///     exactly. That means the Q/gate split lands on a **row boundary
///     in the packed byte stream**: the first `q_dim` rows are Q, the
///     last `q_dim` rows are gate, byte-contiguous, no bit-level
///     splitting. `hidden` must be a multiple of the format's block
///     width (256 for everything except Q8_0 which uses 32) — the
///     GGUF loader already enforces this at tensor-load time, so
///     `hidden = 5120 = 20*256` on 27B is safe unconditionally.
///
/// Returns a pair `(w_q, w_q_gate)` of `PackedWeight`s, both logically
/// `[hidden, q_dim]`. When the GGUF tensor is missing, has the wrong
/// numel, or lands in an unsupported `GgufBuf` variant, this returns
/// `Err` so the caller can fall through to a zero placeholder.
///
/// Host CPU does the split because the GGUF loader hands us a single
/// `CudaTensor<bf16>` or `CudaTensor<i8>`; slicing on device would
/// require another strided-copy kernel we don't have yet. The one-time
/// cost is fine — this runs once at model load, not on the forward
/// hot path.
fn load_packed_halves(
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    hidden: usize,
    q_dim: usize,
) -> std::result::Result<(PackedWeight, PackedWeight), String> {
    let Some(t) = tensors.get(key) else {
        return Err(format!("{} not present", key));
    };
    // Accept either on-disk orientation as long as numel matches the
    // expected packed `[hidden, 2*q_dim]` count. Bf16 test fixtures
    // ship shape `[hidden, 2*q_dim]`; quantized loaders reverse ggml
    // order to `[2*q_dim, hidden]`. Either works — the carrier-specific
    // arms below consume the right layout.
    let numel: usize = t.shape.iter().product();
    let expected_numel = hidden * 2 * q_dim;
    if numel != expected_numel {
        return Err(format!(
            "{}: shape {:?} numel {} != expected {} (= hidden*2*q_dim = {}*{})",
            key,
            t.shape,
            numel,
            expected_numel,
            hidden,
            2 * q_dim
        ));
    }
    match &t.buf {
        GgufBuf::Bf16(src) => split_bf16_packed_halves(src, key, hidden, q_dim),
        GgufBuf::Q4K(src) => split_block_quant_halves(
            src,
            key,
            hidden,
            q_dim,
            256,
            144,
            PackedQuant::Q4K,
        ),
        GgufBuf::Q5K(src) => split_block_quant_halves(
            src,
            key,
            hidden,
            q_dim,
            256,
            176,
            PackedQuant::Q5K,
        ),
        GgufBuf::Q6K(src) => split_block_quant_halves(
            src,
            key,
            hidden,
            q_dim,
            256,
            210,
            PackedQuant::Q6K,
        ),
        GgufBuf::Q8_0(src) => split_block_quant_halves(
            src,
            key,
            hidden,
            q_dim,
            32,
            34,
            PackedQuant::Q8_0,
        ),
        GgufBuf::IQ4XS(src) => split_block_quant_halves(
            src,
            key,
            hidden,
            q_dim,
            256,
            136,
            PackedQuant::IQ4XS,
        ),
        other => Err(format!(
            "{}: packed-halves loader does not handle GgufBuf variant {:?}",
            key,
            std::mem::discriminant(other)
        )),
    }
}

/// Tag used to pick the right `PackedWeight` constructor in
/// [`split_block_quant_halves`]. Local-only — kernel dispatch still
/// goes through [`PackedWeight::matmul_f32`].
#[derive(Debug, Clone, Copy)]
enum PackedQuant {
    Q4K,
    Q5K,
    Q6K,
    Q8_0,
    IQ4XS,
}

/// Split a bf16 `[hidden, 2*q_dim]` device tensor into two `[hidden,
/// q_dim]` bf16 halves, row-strided. See the `load_packed_halves`
/// docstring.
fn split_bf16_packed_halves(
    src: &CudaTensor<bf16>,
    key: &str,
    hidden: usize,
    q_dim: usize,
) -> std::result::Result<(PackedWeight, PackedWeight), String> {
    let host = src
        .to_host()
        .map_err(|e| format!("{}: download bf16: {:?}", key, e))?;
    if host.len() != hidden * 2 * q_dim {
        return Err(format!(
            "{}: bf16 host.len {} != hidden*2*q_dim {}",
            key,
            host.len(),
            hidden * 2 * q_dim
        ));
    }
    // Per-head interleaved unpacking. Source is `[hidden, 2*q_dim]`
    // row-major (the bf16 test-fixture convention documented above).
    // The 2*q_dim column axis is ordered per-head as
    // `[Q[h=0, 0..head_dim], gate[h=0, 0..head_dim], Q[h=1, ...], ...]`
    // (see `split_block_quant_halves` for the graph-side reasoning).
    // De-interleave into two `[hidden, q_dim]` halves whose
    // column order is `[Q[h=0], Q[h=1], ...]` / `[gate[h=0], gate[h=1], ...]`.
    let head_dim = 256usize; // Qwen3.5 FA heads are 256-wide uniformly.
    if q_dim % head_dim != 0 {
        return Err(format!(
            "{}: q_dim={} not a multiple of head_dim={}",
            key, q_dim, head_dim
        ));
    }
    let n_heads = q_dim / head_dim;
    let mut q_half: Vec<bf16> = Vec::with_capacity(hidden * q_dim);
    let mut g_half: Vec<bf16> = Vec::with_capacity(hidden * q_dim);
    for hidden_row in 0..hidden {
        let row_base = hidden_row * 2 * q_dim;
        for h in 0..n_heads {
            let head_base = row_base + h * 2 * head_dim;
            q_half.extend_from_slice(&host[head_base..head_base + head_dim]);
            g_half
                .extend_from_slice(&host[head_base + head_dim..head_base + 2 * head_dim]);
        }
    }
    let dev = src.device().clone();
    let w_q = CudaTensor::<bf16>::from_host(dev.clone(), vec![hidden, q_dim], &q_half)
        .map_err(|e| format!("{}: upload Q bf16 half: {:?}", key, e))?;
    let w_q_gate = CudaTensor::<bf16>::from_host(dev, vec![hidden, q_dim], &g_half)
        .map_err(|e| format!("{}: upload gate bf16 half: {:?}", key, e))?;
    Ok((
        PackedWeight::Bf16 { t: w_q, k: hidden, n: q_dim },
        PackedWeight::Bf16 { t: w_q_gate, k: hidden, n: q_dim },
    ))
}

/// Split a block-quantized device tensor — laid out as `[n=2*q_dim,
/// k=hidden]` row-major blocks — into two `[n=q_dim, k=hidden]`
/// packed halves. See the `load_packed_halves` docstring for the
/// byte-layout rationale.
///
/// `block_elems` is the number of logical elements per block
/// (256 for the K-quants / IQ4XS, 32 for Q8_0). `block_bytes` is the
/// per-block byte count (144 for Q4K, 176 Q5K, 210 Q6K, 34 Q8_0,
/// 136 IQ4XS). `hidden` must be a multiple of `block_elems`; the
/// GGUF loader enforces this upstream, but we re-check defensively
/// so a misconfigured caller hits a loud error instead of silently
/// slicing across a block boundary.
fn split_block_quant_halves(
    src: &CudaTensor<i8>,
    key: &str,
    hidden: usize,
    q_dim: usize,
    block_elems: usize,
    block_bytes: usize,
    variant: PackedQuant,
) -> std::result::Result<(PackedWeight, PackedWeight), String> {
    if !hidden.is_multiple_of(block_elems) {
        return Err(format!(
            "{}: hidden={} not a multiple of block_elems={} for {:?}",
            key, hidden, block_elems, variant
        ));
    }
    let row_bytes = (hidden / block_elems) * block_bytes;
    let total_rows = 2 * q_dim;
    let expected_total_bytes = total_rows * row_bytes;
    let host = src
        .to_host()
        .map_err(|e| format!("{}: download {:?} bytes: {:?}", key, variant, e))?;
    if host.len() != expected_total_bytes {
        return Err(format!(
            "{}: {:?} host.len {} != expected {} (= 2*q_dim*(hidden/block_elems)*block_bytes = {}*{}*{})",
            key,
            variant,
            host.len(),
            expected_total_bytes,
            total_rows,
            hidden / block_elems,
            block_bytes,
        ));
    }
    // Per-head interleaved unpacking.
    //
    // The reference reshapes the fused projection's output as
    // `[head_dim*2, n_head, n_tokens]` (`qwen35_target_graph.cpp`
    // lines 294-304), which means the GGUF's 2*q_dim output axis is
    // ordered per-head as `[Q[h=0, 0..head_dim], gate[h=0, 0..head_dim],
    // Q[h=1, 0..head_dim], gate[h=1, 0..head_dim], ...]`. NOT
    // `[all-Q, all-gate]`. A naive split_at(q_dim) grabs Q+gate of
    // the first half of heads and Q+gate of the second half — total
    // garbage for an attention matmul. This de-interleaves at load
    // time: head h's Q rows land at `q_half[h*head_dim, (h+1)*head_dim)`
    // and gate rows at the same slot in `g_half`, producing two
    // `[hidden, q_dim]` carriers with heads in standard order.
    //
    // `q_dim = n_q_heads * head_dim` so the per-head row block is
    // `head_dim`. We gather at byte granularity because the
    // quantized block layout puts one block-row per output channel.
    let head_dim = q_dim / {
        // Work out n_heads from q_dim. q_dim = n_q_heads * head_dim.
        // For Qwen3.5-27B n_q_heads = 24, head_dim = 256. For smaller
        // test fixtures we infer from the common case that head_dim
        // is 256 (FA's shipping head_dim) — callers that want a
        // different split should reshape before calling. The assert
        // here fires if q_dim is not 256-aligned so a misconfigured
        // caller doesn't silently land a wrong layout.
        //
        // We special-case head_dim = 256 here; any Qwen3.5 variant
        // (FA layers have head_dim=256 uniformly) passes through.
        256usize
    };
    let head_rows = head_dim;
    if q_dim % head_rows != 0 {
        return Err(format!(
            "{}: q_dim={} not a multiple of head_dim={}",
            key, q_dim, head_rows
        ));
    }
    let n_heads = q_dim / head_rows;
    // Each head block is 2*head_dim rows wide (Q then gate); total
    // 2*q_dim rows across n_heads head-blocks.
    let head_block_bytes = 2 * head_rows * row_bytes; // Q+gate bytes for one head.
    let half_bytes = q_dim * row_bytes; // = n_heads * head_rows * row_bytes.
    let mut q_host = Vec::with_capacity(half_bytes);
    let mut g_host = Vec::with_capacity(half_bytes);
    // CTOX_QGATE_SPLIT={interleave,naive,swap_per_head,swap_naive}
    //   interleave (default): Q=first 256 cols, gate=next 256 cols, per head.
    //   naive: Q=cols 0..q_dim, gate=cols q_dim..2*q_dim (two big halves).
    //   swap_per_head: per-head but gate-first-then-Q instead of Q-then-gate.
    //   swap_naive: naive with halves swapped (gate first, Q second).
    // This is an explicit bug-lateralization knob for the L3 FA chan3994
    // investigation. Remove once the correct variant is locked in.
    let mode = std::env::var("CTOX_QGATE_SPLIT").unwrap_or_else(|_| "interleave".to_string());
    match mode.as_str() {
        "naive" => {
            q_host.extend_from_slice(&host[0..half_bytes]);
            g_host.extend_from_slice(&host[half_bytes..2 * half_bytes]);
        }
        "swap_naive" => {
            q_host.extend_from_slice(&host[half_bytes..2 * half_bytes]);
            g_host.extend_from_slice(&host[0..half_bytes]);
        }
        "swap_per_head" => {
            for h in 0..n_heads {
                let head_base = h * head_block_bytes;
                let q_start = head_base;
                let q_end = q_start + head_rows * row_bytes;
                let g_start = q_end;
                let g_end = g_start + head_rows * row_bytes;
                // gate first in source, emit as Q
                q_host.extend_from_slice(&host[g_start..g_end]);
                g_host.extend_from_slice(&host[q_start..q_end]);
            }
        }
        _ => {
            for h in 0..n_heads {
                let head_base = h * head_block_bytes;
                let q_start = head_base;
                let q_end = q_start + head_rows * row_bytes;
                let g_start = q_end;
                let g_end = g_start + head_rows * row_bytes;
                q_host.extend_from_slice(&host[q_start..q_end]);
                g_host.extend_from_slice(&host[g_start..g_end]);
            }
        }
    }
    let dev = src.device().clone();
    let q_tensor = CudaTensor::<i8>::from_host(dev.clone(), vec![half_bytes], &q_host)
        .map_err(|e| format!("{}: upload Q {:?} half ({} bytes): {:?}", key, variant, half_bytes, e))?;
    let g_tensor = CudaTensor::<i8>::from_host(dev, vec![half_bytes], &g_host)
        .map_err(|e| format!("{}: upload gate {:?} half ({} bytes): {:?}", key, variant, half_bytes, e))?;
    let (w_q, w_q_gate) = match variant {
        PackedQuant::Q4K => (
            PackedWeight::Q4K { t: q_tensor, k: hidden, n: q_dim },
            PackedWeight::Q4K { t: g_tensor, k: hidden, n: q_dim },
        ),
        PackedQuant::Q5K => (
            PackedWeight::Q5K { t: q_tensor, k: hidden, n: q_dim },
            PackedWeight::Q5K { t: g_tensor, k: hidden, n: q_dim },
        ),
        PackedQuant::Q6K => (
            PackedWeight::Q6K { t: q_tensor, k: hidden, n: q_dim },
            PackedWeight::Q6K { t: g_tensor, k: hidden, n: q_dim },
        ),
        PackedQuant::Q8_0 => (
            PackedWeight::Q8_0 { t: q_tensor, k: hidden, n: q_dim },
            PackedWeight::Q8_0 { t: g_tensor, k: hidden, n: q_dim },
        ),
        PackedQuant::IQ4XS => (
            PackedWeight::IQ4XS { t: q_tensor, k: hidden, n: q_dim },
            PackedWeight::IQ4XS { t: g_tensor, k: hidden, n: q_dim },
        ),
    };
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
    let name_ssm_alpha = format!("blk.{}.ssm_alpha.weight", layer_idx);
    let name_ssm_beta = format!("blk.{}.ssm_beta.weight", layer_idx);
    let name_ssm_a = format!("blk.{}.ssm_a", layer_idx);
    let name_ssm_dt_bias = format!("blk.{}.ssm_dt.bias", layer_idx);
    let name_ssm_conv1d = format!("blk.{}.ssm_conv1d.weight", layer_idx);
    let name_attn_gate = format!("blk.{}.attn_gate.weight", layer_idx);
    let name_ssm_norm = format!("blk.{}.ssm_norm.weight", layer_idx);

    let pre_norm = load_f32_placeholder(device, tensors, &name_attn_norm, vec![cfg.hidden_dim]);

    // Real 27B layout (from dflash qwen35_target_graph.cpp):
    //   attn_qkv.weight = Q5_K [hidden, qkv_proj_dim]
    //     qkv_proj_dim = 2 * num_k_heads * head_k_dim  (Q||K)
    //                  +     num_v_heads * head_v_dim  (V)
    //                  = 2*16*128 + 48*128 = 10240
    //   ssm_out.weight = Q5_K [num_v_heads*head_v_dim, hidden]
    //                  = [6144, 5120]
    // With `head_k_dim == head_v_dim == gdn_ssm_dim`, both of these
    // are derived from the typed config via helper accessors. The
    // previous h*4*gdn_ssm_dim assumption was from the pre-real-
    // weights port and mismatched the GGUF, causing 48 of 64 layers
    // to fall through to PackedWeight::Zero.
    let qkv_proj_dim = cfg.gdn_qkv_proj_dim();
    let inner_dim = cfg.gdn_inner_dim();
    let dt_rank = cfg.gdn_num_v_heads;
    let conv_kernel_size = 4usize;

    let w_qkvg = load_packed_weight(device, tensors, &name_qkvg, cfg.hidden_dim, qkv_proj_dim);
    let w_out = load_packed_weight(device, tensors, &name_out, inner_dim, cfg.hidden_dim);

    // ssm_alpha / ssm_beta: logical [dt_rank, hidden], stored
    // [hidden, dt_rank] in GGUF ne-order. `load_packed_weight` takes
    // (k=in, n=out) matching the logical matmul `[n_tokens, hidden] @
    // [hidden, dt_rank]`. F32 weights are cast to bf16 on load via the
    // PackedWeight::Bf16 variant.
    let ssm_alpha = load_packed_weight(device, tensors, &name_ssm_alpha, cfg.hidden_dim, dt_rank);
    let ssm_beta = load_packed_weight(device, tensors, &name_ssm_beta, cfg.hidden_dim, dt_rank);

    // ssm_a: [dt_rank] f32 per-head scale (negative values — stored
    // as -exp(A_log) in the shipping checkpoint). Fallback to zeros
    // not ones here: ones would collapse softplus(alpha)*1 into the
    // stand-in regime and silently corrupt 48 layers. Zeros at least
    // produce g=0 which is a well-defined (no-decay) recurrence.
    let ssm_a = load_f32_vector_zero_fallback(device, tensors, &name_ssm_a, dt_rank);
    let ssm_dt_bias =
        load_f32_vector_zero_fallback(device, tensors, &name_ssm_dt_bias, dt_rank);

    // ssm_conv1d.weight: logical shape [conv_channels, kernel_size],
    // stored in GGUF with ne = [kernel_size, conv_channels] (kernel
    // axis fast). Our CUDA kernel consumes `w[k * n_channels + c]`,
    // i.e. `[K, n_channels]` row-major with `k` slow — the transpose
    // of the GGUF layout. Load + transpose once at load time so the
    // per-forward kernel gets coalesced reads on the channel axis.
    let ssm_conv1d_weight = load_conv1d_weight_transposed(
        device,
        tensors,
        &name_ssm_conv1d,
        conv_kernel_size,
        qkv_proj_dim,
    );

    // attn_gate.weight: [hidden, inner_dim] (Q4_K on production 27B).
    let attn_gate = load_packed_weight(device, tensors, &name_attn_gate, cfg.hidden_dim, inner_dim);

    // ssm_norm.weight: [head_v_dim=s_v] f32. Use ones as fallback —
    // that's the RMSNorm identity.
    let ssm_norm = load_f32_placeholder(
        device,
        tensors,
        &name_ssm_norm,
        vec![cfg.gdn_ssm_dim],
    );

    Qwen35GDN {
        pre_norm,
        w_qkvg,
        w_out,
        ssm_alpha,
        ssm_beta,
        ssm_a,
        ssm_dt_bias,
        ssm_conv1d_weight,
        attn_gate,
        ssm_norm,
        config: cfg,
        layer_idx,
    }
}

/// Build the SwiGLU FFN sub-block for one decoder layer. Loads the
/// `post_attention_norm.weight` + `ffn_{gate,up,down}.weight` tensors
/// from the GGUF via [`load_packed_weight`] / [`load_f32_placeholder`]
/// and assembles a [`Qwen35FFN`] ready for the layer loop.
///
/// Shape invariants (matching the reference's
/// `gguf_target_loader.cpp`):
///   * `post_attention_norm.weight` — `[hidden]` f32.
///   * `ffn_gate.weight` — logical `[hidden, intermediate]`; IQ4_XS on
///     production 27B.
///   * `ffn_up.weight` — same shape + dtype as `ffn_gate`.
///   * `ffn_down.weight` — logical `[intermediate, hidden]`.
///
/// Applied on every layer (FA and GDN alike) after the attention
/// residual add — matches the reference loop at
/// `qwen35_target_graph.cpp` lines 736-742.
fn build_ffn_layer(
    device: &Arc<DeviceContext>,
    config: &Qwen35Config,
    tensors: &HashMap<String, GgufTensor>,
    layer_idx: usize,
) -> Qwen35FFN {
    let cfg = *config;
    let name_post_norm = format!("blk.{}.post_attention_norm.weight", layer_idx);
    let name_gate = format!("blk.{}.ffn_gate.weight", layer_idx);
    let name_up = format!("blk.{}.ffn_up.weight", layer_idx);
    let name_down = format!("blk.{}.ffn_down.weight", layer_idx);

    let pre_norm = load_f32_placeholder(device, tensors, &name_post_norm, vec![cfg.hidden_dim]);
    let w_gate = load_packed_weight(
        device,
        tensors,
        &name_gate,
        cfg.hidden_dim,
        cfg.intermediate_dim,
    );
    let w_up = load_packed_weight(
        device,
        tensors,
        &name_up,
        cfg.hidden_dim,
        cfg.intermediate_dim,
    );
    let w_down = load_packed_weight(
        device,
        tensors,
        &name_down,
        cfg.intermediate_dim,
        cfg.hidden_dim,
    );

    Qwen35FFN {
        pre_norm,
        w_gate,
        w_up,
        w_down,
        config: cfg,
        layer_idx,
    }
}

/// Read a `[n]` f32 vector (e.g. ssm_a / ssm_dt.bias). Falls back to a
/// zero-filled tensor with a tracing warning on any mismatch or dtype
/// other than F32. Zero fallback is chosen deliberately (vs ones) so
/// the GDN forward produces a well-defined (no-decay / zero-bias)
/// recurrence instead of silently applying wrong multipliers.
fn load_f32_vector_zero_fallback(
    device: &Arc<DeviceContext>,
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    expected_len: usize,
) -> CudaTensor<f32> {
    let placeholder = || {
        CudaTensor::<f32>::zeros(device.clone(), vec![expected_len])
            .expect("alloc f32 vec zero placeholder")
    };
    let Some(t) = tensors.get(key) else {
        tracing::warn!(
            key,
            expected_len,
            "qwen35_target: {} missing; using zero placeholder",
            key
        );
        return placeholder();
    };
    let numel: usize = t.shape.iter().product();
    if numel != expected_len {
        tracing::warn!(
            key,
            shape = ?t.shape,
            expected_len,
            "qwen35_target: f32 vector numel mismatch; zero placeholder"
        );
        return placeholder();
    }
    match &t.buf {
        GgufBuf::F32(src) => match src.to_host() {
            Ok(host) => CudaTensor::<f32>::from_host(device.clone(), vec![expected_len], &host)
                .unwrap_or_else(|e| {
                    tracing::warn!(key, error = %e, "qwen35_target: upload f32 vector failed");
                    placeholder()
                }),
            Err(e) => {
                tracing::warn!(key, error = %e, "qwen35_target: download f32 vector failed");
                placeholder()
            }
        },
        _ => {
            tracing::warn!(
                key,
                shape = ?t.shape,
                "qwen35_target: f32 vector dtype not F32; zero placeholder"
            );
            placeholder()
        }
    }
}

/// Load `ssm_conv1d.weight` and transpose from the GGUF's
/// `[kernel_size, n_channels]` (kernel-axis-fast) layout to the
/// `[kernel_size, n_channels]` (channel-axis-fast) layout the
/// ssm_conv1d kernel consumes. Outer shape is the same, but the
/// element order along the fast axis differs — GGUF stores `(k fast,
/// c slow)` and our kernel reads `buf[k * n_channels + c]` which
/// requires `(c fast, k slow)`.
///
/// Implemented as a download → host transpose → upload, since this
/// runs once at load time (~48 layers × 4 × 10240 f32 = 8 MB total).
fn load_conv1d_weight_transposed(
    device: &Arc<DeviceContext>,
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    kernel_size: usize,
    n_channels: usize,
) -> CudaTensor<f32> {
    let expected_numel = kernel_size * n_channels;
    let placeholder = || {
        CudaTensor::<f32>::zeros(device.clone(), vec![kernel_size, n_channels])
            .expect("alloc ssm_conv1d_weight zero placeholder")
    };
    let Some(t) = tensors.get(key) else {
        tracing::warn!(
            key,
            kernel_size,
            n_channels,
            "qwen35_target: {} missing; using zero placeholder",
            key
        );
        return placeholder();
    };
    let numel: usize = t.shape.iter().product();
    if numel != expected_numel {
        tracing::warn!(
            key,
            shape = ?t.shape,
            kernel_size,
            n_channels,
            "qwen35_target: ssm_conv1d.weight numel mismatch; zero placeholder"
        );
        return placeholder();
    }
    let raw = match &t.buf {
        GgufBuf::F32(src) => match src.to_host() {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(key, error = %e, "qwen35_target: download ssm_conv1d.weight failed");
                return placeholder();
            }
        },
        _ => {
            tracing::warn!(
                key,
                shape = ?t.shape,
                "qwen35_target: ssm_conv1d.weight dtype not F32; zero placeholder"
            );
            return placeholder();
        }
    };
    // Transpose + kernel-axis flip. GGUF raw stores `raw[c*K + k_ggml]`
    // where `k_ggml = 0` is the oldest tap (matching ggml_ssm_conv's
    // shift-register convention `sumf += x[(i+j) % K] * w[j]`). Our
    // CUDA conv kernel walks `k_gpu = 0..K-1` with `k_gpu=0` pairing
    // with the NEWEST tap (`src_idx = t + K-1 - k_gpu`). To reconcile,
    // flip the kernel axis at load: `w_gpu[k_gpu] = w_ggml[K-1-k_gpu]`.
    // Done once at load, not on the hot path.
    let mut host = vec![0f32; expected_numel];
    for k_gpu in 0..kernel_size {
        let k_ggml = kernel_size - 1 - k_gpu;
        for c in 0..n_channels {
            host[k_gpu * n_channels + c] = raw[c * kernel_size + k_ggml];
        }
    }
    CudaTensor::<f32>::from_host(device.clone(), vec![kernel_size, n_channels], &host)
        .unwrap_or_else(|e| {
            tracing::warn!(key, error = %e, "qwen35_target: upload transposed ssm_conv1d.weight failed");
            placeholder()
        })
}

/// Read a `[rows, cols]` f32 matrix. Falls back to zeros on any
/// mismatch or non-F32 dtype. Used for the small f32 weights
/// (`ssm_conv1d.weight`) that stay in f32 at forward time.
#[allow(dead_code)]
fn load_f32_matrix_zero_fallback(
    device: &Arc<DeviceContext>,
    tensors: &HashMap<String, GgufTensor>,
    key: &str,
    rows: usize,
    cols: usize,
) -> CudaTensor<f32> {
    let expected_numel = rows * cols;
    let placeholder = || {
        CudaTensor::<f32>::zeros(device.clone(), vec![rows, cols])
            .expect("alloc f32 matrix zero placeholder")
    };
    let Some(t) = tensors.get(key) else {
        tracing::warn!(
            key,
            rows,
            cols,
            "qwen35_target: {} missing; using zero placeholder",
            key
        );
        return placeholder();
    };
    let numel: usize = t.shape.iter().product();
    if numel != expected_numel {
        tracing::warn!(
            key,
            shape = ?t.shape,
            rows,
            cols,
            "qwen35_target: f32 matrix numel mismatch; zero placeholder"
        );
        return placeholder();
    }
    match &t.buf {
        GgufBuf::F32(src) => match src.to_host() {
            Ok(host) => CudaTensor::<f32>::from_host(device.clone(), vec![rows, cols], &host)
                .unwrap_or_else(|e| {
                    tracing::warn!(key, error = %e, "qwen35_target: upload f32 matrix failed");
                    placeholder()
                }),
            Err(e) => {
                tracing::warn!(key, error = %e, "qwen35_target: download f32 matrix failed");
                placeholder()
            }
        },
        _ => {
            tracing::warn!(
                key,
                shape = ?t.shape,
                "qwen35_target: f32 matrix dtype not F32; zero placeholder"
            );
            placeholder()
        }
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
        GgufBuf::Q4K(src) => {
            // Embedding kernels consume a bf16 `[vocab, hidden]` matrix
            // directly — no mmvq wiring into embedding_lookup — so we
            // host-dequant Q4_K blocks once at load. ~2.5 GB for the 27B
            // `[248320, 5120]` embed; one-time, off the hot path.
            let raw = match src.to_host() {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(key, error = %e, "qwen35_target: download Q4K embed failed; zero placeholder");
                    return CudaTensor::<bf16>::zeros(device.clone(), vec![vocab, hidden_dim]);
                }
            };
            // Reinterpret `&[i8]` as `&[u8]` for the Q4_K block parser.
            let bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(raw.as_ptr() as *const u8, raw.len())
            };
            let host_bf16 = match dequant_q4_k_to_bf16(bytes, vocab * hidden_dim) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(key, error = %e, "qwen35_target: Q4K dequant failed; zero placeholder");
                    return CudaTensor::<bf16>::zeros(device.clone(), vec![vocab, hidden_dim]);
                }
            };
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

/// Host-side dequantize Q4_K blocks to bf16. Ports the reference
/// `dequantize_row_q4_K` from llama.cpp's `ggml-quants.c`. Per-block
/// layout is `{ d: f16, dmin: f16, scales: [u8; 12], qs: [u8; 128] }`
/// = 144 bytes / 256 elements.
///
/// Two sub-blocks of 32 elements share a scale/min pair via
/// `get_scale_min_k4` (the 12-byte packed-scales decoder that
/// Q4_K and Q5_K both use). We duplicate the decoder here rather than
/// depend on the private `gguf_loader` helper since that helper isn't
/// part of the crate's public surface.
///
/// This only fires on the token embedding path (the loader keeps
/// Q4_K weights packed for the mmvq kernels in all other cases). ~1s
/// at 27B load time, so a `for` loop in Rust is fine.
fn dequant_q4_k_to_bf16(bytes: &[u8], n_elems: usize) -> Result<Vec<bf16>> {
    const BLOCK: usize = 256;
    const BLOCK_BYTES: usize = 144;
    if !n_elems.is_multiple_of(BLOCK) {
        return Err(anyhow!(
            "Q4_K dequant: n_elems {} not a multiple of {}",
            n_elems,
            BLOCK
        ));
    }
    let nb = n_elems / BLOCK;
    if bytes.len() != nb * BLOCK_BYTES {
        return Err(anyhow!(
            "Q4_K dequant: got {} bytes, expected {} ({} blocks)",
            bytes.len(),
            nb * BLOCK_BYTES,
            nb
        ));
    }
    let mut out: Vec<bf16> = Vec::with_capacity(n_elems);
    // Per block: d(2) dmin(2) scales[12] qs[128].
    for i in 0..nb {
        let b = &bytes[i * BLOCK_BYTES..(i + 1) * BLOCK_BYTES];
        let d_bits = u16::from_le_bytes([b[0], b[1]]);
        let dmin_bits = u16::from_le_bytes([b[2], b[3]]);
        let d = f16::from_bits(d_bits).to_f32();
        let min = f16::from_bits(dmin_bits).to_f32();
        let scales = &b[4..16];
        let qs = &b[16..144];

        let mut is = 0usize;
        let mut ql_off = 0usize;
        for _ in (0..BLOCK).step_by(64) {
            let (sc0, m0) = q4k_get_scale_min(is, scales);
            let d1 = d * sc0 as f32;
            let m1 = min * m0 as f32;
            let (sc1, m1s) = q4k_get_scale_min(is + 1, scales);
            let d2 = d * sc1 as f32;
            let m2 = min * m1s as f32;

            // low nibble for 32 elements (sub-block 2j)
            for l in 0..32 {
                let q = (qs[ql_off + l] & 0x0F) as i32;
                let v = d1 * (q as f32) - m1;
                out.push(bf16::from_f32(v));
            }
            // high nibble for 32 elements (sub-block 2j+1)
            for l in 0..32 {
                let q = ((qs[ql_off + l] >> 4) & 0x0F) as i32;
                let v = d2 * (q as f32) - m2;
                out.push(bf16::from_f32(v));
            }

            ql_off += 32;
            is += 2;
        }
    }
    Ok(out)
}

/// llama.cpp's `get_scale_min_k4` — decode a 6-bit scale + 6-bit min
/// from the 12-byte packed `scales` array shared by Q4_K / Q5_K blocks.
#[inline]
fn q4k_get_scale_min(j: usize, scales: &[u8]) -> (u8, u8) {
    if j < 4 {
        (scales[j] & 63, scales[j + 4] & 63)
    } else {
        let d = (scales[j + 4] & 0x0F) | ((scales[j - 4] >> 6) << 4);
        let m = (scales[j + 4] >> 4) | ((scales[j] >> 6) << 4);
        (d, m)
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
/// the load_packed_halves FA-Q splitter above) don't churn, but
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
/// Human-readable name for a [`PackedWeight`] variant, for loader-time
/// tracing. Runs once per weight at load and never in the forward hot
/// path.
fn packed_variant_name(w: &PackedWeight) -> &'static str {
    match w {
        PackedWeight::Bf16 { .. } => "Bf16",
        PackedWeight::Q4K { .. } => "Q4K",
        PackedWeight::Q5K { .. } => "Q5K",
        PackedWeight::Q6K { .. } => "Q6K",
        PackedWeight::Q8_0 { .. } => "Q8_0",
        PackedWeight::IQ4XS { .. } => "IQ4XS",
        PackedWeight::Zero { .. } => "Zero",
    }
}

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
/// Download the last row of a `[n_tokens, hidden_dim]` bf16 tensor and
/// return `(l2, absmax)` computed in f32 on the host. Diagnostic-only —
/// used by [`Qwen35Target::forward_diag`] to dump per-layer activation
/// health without adding a device kernel.
fn bf16_last_row_l2_and_absmax(
    hidden: &CudaTensor<bf16>,
    n_tokens: usize,
    hidden_dim: usize,
) -> Result<(f32, f32)> {
    let host = hidden
        .to_host()
        .map_err(|e| anyhow!("diag: download hidden: {:?}", e))?;
    if host.len() < n_tokens * hidden_dim {
        return Err(anyhow!(
            "diag: hidden host buf len {} < n_tokens*hidden_dim = {}*{}",
            host.len(),
            n_tokens,
            hidden_dim
        ));
    }
    let last_row_start = (n_tokens - 1) * hidden_dim;
    let last_row = &host[last_row_start..last_row_start + hidden_dim];
    let mut sumsq = 0.0f64;
    let mut absmax = 0.0f32;
    for &v in last_row {
        let f = v.to_f32();
        sumsq += (f as f64) * (f as f64);
        let a = f.abs();
        if a > absmax {
            absmax = a;
        }
    }
    let l2 = (sumsq.sqrt()) as f32;
    Ok((l2, absmax))
}

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

        // GDN state + inter, one per GDN layer. H is the GDN value-
        // head count (48 on 27B), NOT FA's n_q_heads — the two head
        // schemes are decoupled.
        let s_v = cfg.gdn_ssm_dim;
        let h = cfg.gdn_num_v_heads;
        let qkv_proj_dim = cfg.gdn_qkv_proj_dim();
        let mut gdn_states: Vec<CudaTensor<f32>> = Vec::with_capacity(target.n_gdn);
        let mut gdn_inter: Vec<CudaTensor<f16>> = Vec::with_capacity(target.n_gdn);
        let mut gdn_conv_states: Vec<CudaTensor<f32>> =
            Vec::with_capacity(target.n_gdn);
        for _ in 0..target.n_gdn {
            gdn_states.push(
                CudaTensor::<f32>::zeros(dev.clone(), vec![s_v, s_v, h, 1])
                    .expect("alloc gdn state"),
            );
            gdn_inter.push(
                CudaTensor::<f16>::zeros(dev.clone(), vec![s_v, s_v, h, n_tokens])
                    .expect("alloc gdn inter"),
            );
            gdn_conv_states.push(
                CudaTensor::<f32>::zeros(dev.clone(), vec![3, qkv_proj_dim])
                    .expect("alloc gdn conv state"),
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
                &mut gdn_conv_states,
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
        let h = cfg.gdn_num_v_heads;
        let qkv_proj_dim = cfg.gdn_qkv_proj_dim();
        let mut gdn_states: Vec<CudaTensor<f32>> = Vec::with_capacity(target.n_gdn);
        let mut gdn_inter: Vec<CudaTensor<f16>> = Vec::with_capacity(target.n_gdn);
        let mut gdn_conv_states: Vec<CudaTensor<f32>> =
            Vec::with_capacity(target.n_gdn);
        for _ in 0..target.n_gdn {
            gdn_states.push(
                CudaTensor::<f32>::zeros(dev.clone(), vec![s_v, s_v, h, 1])
                    .expect("alloc gdn state"),
            );
            gdn_inter.push(
                CudaTensor::<f16>::zeros(dev.clone(), vec![s_v, s_v, h, n_tokens])
                    .expect("alloc gdn inter"),
            );
            gdn_conv_states.push(
                CudaTensor::<f32>::zeros(dev.clone(), vec![3, qkv_proj_dim])
                    .expect("alloc gdn conv state"),
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
                &mut gdn_conv_states,
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

    /// Prefill + decode tok/s microbench on the HumanEval prompt,
    /// repeated to 128 tokens. Loads the 27B with keep_packed=true,
    /// runs:
    ///
    ///   1. 3× warmup forwards of 32 tokens to JIT-cache CUDA modules
    ///   2. 5× 128-token forwards — report prefill tok/s
    ///   3. 64× 32-token forwards continuing the KV cache — report
    ///      "pseudo-decode" tok/s (32 toks per chunk because the bf16
    ///      matmul kernel's M-dim tile is 32; true 1-token decode
    ///      needs a small-M kernel variant later)
    ///
    /// Reference numbers from Phase-1 FFI on same HumanEval prompt:
    ///   - chain: ~77 tok/s
    ///   - DDTree: ~94 tok/s
    /// Those are from the dflash reference calling hand-tuned
    /// ggml-cuda kernels via a C++ harness. Our bare-metal stack
    /// runs the SAME underlying kernels (vendored verbatim) via a
    /// Rust+cudarc launcher, so per-kernel throughput should match;
    /// any delta is launcher overhead + our slightly-imperfect
    /// fattn-mma (Path B wmma vs upstream mma.sync).
    ///
    /// Run:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///     --ignored --nocapture qwen35_prefill_decode_bench
    #[test]
    #[ignore]
    fn qwen35_prefill_decode_bench() {
        use std::time::Instant;

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let meta = crate::gguf_loader::parse_qwen35_metadata(QWEN35_27B_GGUF)
            .expect("parse_qwen35_metadata");
        let cfg = Qwen35Config::from_metadata(&meta, 128);

        eprintln!("bench: loading 27B (keep_packed=true)...");
        let t_load = Instant::now();
        let target = Qwen35Target::load_from_gguf_with_config(
            dev.clone(),
            cfg,
            QWEN35_27B_GGUF,
            crate::gguf_loader::LoaderConfig { keep_packed: true },
        )
        .expect("load_from_gguf_with_config");
        let load_s = t_load.elapsed().as_secs_f64();
        eprintln!(
            "bench: model loaded in {:.2}s (vocab={} layers={})",
            load_s,
            target.vocab_size,
            target.layers.len()
        );

        // HumanEval prompt repeated to fill 128 tokens (same 9-token
        // pattern the Phase-1 FFI bench used).
        let base: [i32; 9] = [7734, 264, 6185, 36974, 883, 13094, 6326, 61369, 25];
        let prefill_len = 128usize;
        let mut prefill_tokens = vec![0i32; prefill_len];
        for i in 0..prefill_len {
            prefill_tokens[i] = base[i % 9];
        }

        let max_ctx = 4096usize;

        // Setup helper — fresh KV cache + GDN states for a single run.
        let setup_state = |dev: &Arc<DeviceContext>,
                           cfg: &Qwen35Config,
                           target: &Qwen35Target,
                           max_ctx: usize,
                           n_inter: usize|
         -> (
            KvCache,
            Vec<CudaTensor<f32>>,
            Vec<CudaTensor<f16>>,
            Vec<CudaTensor<f32>>,
        ) {
            let kv_cache = KvCache::new(
                dev.clone(),
                target.n_full_attn,
                max_ctx,
                cfg.n_kv_heads,
                cfg.head_dim,
            )
            .expect("alloc kv cache");
            let s_v = cfg.gdn_ssm_dim;
            let h = cfg.gdn_num_v_heads;
            let qkv_proj_dim = cfg.gdn_qkv_proj_dim();
            let mut gdn_states: Vec<CudaTensor<f32>> = Vec::with_capacity(target.n_gdn);
            let mut gdn_inter: Vec<CudaTensor<f16>> = Vec::with_capacity(target.n_gdn);
            let mut gdn_conv_states: Vec<CudaTensor<f32>> =
                Vec::with_capacity(target.n_gdn);
            for _ in 0..target.n_gdn {
                gdn_states.push(
                    CudaTensor::<f32>::zeros(dev.clone(), vec![s_v, s_v, h, 1])
                        .expect("alloc gdn state"),
                );
                gdn_inter.push(
                    CudaTensor::<f16>::zeros(dev.clone(), vec![s_v, s_v, h, n_inter])
                        .expect("alloc gdn inter"),
                );
                gdn_conv_states.push(
                    CudaTensor::<f32>::zeros(dev.clone(), vec![3, qkv_proj_dim])
                        .expect("alloc gdn conv state"),
                );
            }
            (kv_cache, gdn_states, gdn_inter, gdn_conv_states)
        };

        // Build tokens + positions for `n_tokens` starting at position
        // `start`.
        let build_input = |tokens_slice: &[i32],
                           start: usize,
                           n_tokens: usize,
                           dev: &Arc<DeviceContext>|
         -> (CudaTensor<i32>, CudaTensor<i32>) {
            let t = CudaTensor::<i32>::from_host(
                dev.clone(),
                vec![n_tokens],
                &tokens_slice[..n_tokens],
            )
            .expect("upload tokens");
            let mut pos = vec![0i32; 4 * n_tokens];
            for i in 0..n_tokens {
                let p = (start + i) as i32;
                pos[i] = p;
                pos[n_tokens + i] = p;
                pos[2 * n_tokens + i] = p;
                pos[3 * n_tokens + i] = 0;
            }
            let positions =
                CudaTensor::<i32>::from_host(dev.clone(), vec![4, n_tokens], &pos)
                    .expect("upload positions");
            (t, positions)
        };

        // ── Warmup: 3× 32-token forwards on a throwaway state ─────
        eprintln!("bench: warmup...");
        {
            let (mut kv, mut gs, mut gi, mut gcs) =
                setup_state(&dev, &cfg, &target, max_ctx, 32);
            let (tk, pos) = build_input(&prefill_tokens, 0, 32, &dev);
            for _ in 0..3 {
                kv.reset();
                for s in gs.iter_mut() {
                    // Zero reset via zeros realloc
                }
                let _ = target
                    .forward(&tk, &pos, &mut kv, &mut gs, &mut gi, &mut gcs)
                    .expect("warmup forward");
                dev.synchronize().ok();
            }
        }

        // ── Prefill bench: 5× 128-token forward ─────────────────
        eprintln!("bench: prefill (5× 128 tokens)...");
        let mut prefill_ms_samples: Vec<f64> = Vec::new();
        for _ in 0..5 {
            let (mut kv, mut gs, mut gi, mut gcs) =
                setup_state(&dev, &cfg, &target, max_ctx, prefill_len);
            let (tk, pos) = build_input(&prefill_tokens, 0, prefill_len, &dev);
            dev.synchronize().ok();
            let t0 = Instant::now();
            let _ = target
                .forward(&tk, &pos, &mut kv, &mut gs, &mut gi, &mut gcs)
                .expect("prefill forward");
            dev.synchronize().ok();
            prefill_ms_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        prefill_ms_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let prefill_median_ms = prefill_ms_samples[2];
        let prefill_tok_s = prefill_len as f64 * 1000.0 / prefill_median_ms;

        // ── Decode bench: continue from prefill, 8× 32-tok chunks ─
        //
        // We'd prefer 1-token decode but the bf16 matmul kernel tiles
        // M in blocks of 32 (from the ggml-cuda cuBLAS path); passing
        // n_tokens=1 would require a small-M kernel variant we don't
        // have yet. 32-token chunks give a lower-bound "effective
        // decode when batching is available" number. Each chunk
        // advances the KV cache by 32.
        eprintln!("bench: decode (8× 32-tok chunks continuing from prefill)...");
        // gdn_inter has to accommodate the largest single forward that
        // runs on this state — i.e. the 128-token prefill we're about
        // to feed in. Sizing it for the 32-token decode chunk makes the
        // GDN shape guard trip on the prefill call. (This only started
        // mattering once the GDN early-return was removed; previously
        // the layer no-op'd and never inspected `gdn_inter.shape`.)
        let (mut kv, mut gs, mut gi, mut gcs) =
            setup_state(&dev, &cfg, &target, max_ctx, prefill_len);
        let (tk_prefill, pos_prefill) = build_input(&prefill_tokens, 0, prefill_len, &dev);
        let _ = target
            .forward(
                &tk_prefill,
                &pos_prefill,
                &mut kv,
                &mut gs,
                &mut gi,
                &mut gcs,
            )
            .expect("prefill for decode");
        dev.synchronize().ok();

        let mut decode_ms_samples: Vec<f64> = Vec::new();
        let mut past = prefill_len;
        let decode_chunk = 32usize;
        let decode_chunks = 8usize;
        let chunk_tokens: Vec<i32> = (0..decode_chunk).map(|i| base[i % 9]).collect();
        for _ in 0..decode_chunks {
            let (tk, pos) = build_input(&chunk_tokens, past, decode_chunk, &dev);
            dev.synchronize().ok();
            let t0 = Instant::now();
            let _ = target
                .forward(&tk, &pos, &mut kv, &mut gs, &mut gi, &mut gcs)
                .expect("decode forward");
            dev.synchronize().ok();
            decode_ms_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
            past += decode_chunk;
        }
        decode_ms_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let decode_median_ms = decode_ms_samples[decode_chunks / 2];
        let decode_tok_s = decode_chunk as f64 * 1000.0 / decode_median_ms;

        // ── Report ───────────────────────────────────────────────
        eprintln!("\n=== QWEN3.5-27B BARE-METAL BENCH (A6000, sm_86) ===");
        eprintln!("Model load (cold disk → device):  {:>8.2} s", load_s);
        eprintln!("Prefill (128 tok, median of 5):   {:>8.2} ms  =>  {:>7.2} tok/s",
                  prefill_median_ms, prefill_tok_s);
        eprintln!("Pseudo-decode (32-tok chunks):    {:>8.2} ms  =>  {:>7.2} tok/s",
                  decode_median_ms, decode_tok_s);
        eprintln!();
        eprintln!("Reference (dflash FFI, Phase 1):");
        eprintln!("  chain:  ~77 tok/s");
        eprintln!("  DDTree: ~94 tok/s");
        eprintln!();
        eprintln!("Prefill raw samples (ms):  {:?}", prefill_ms_samples);
        eprintln!("Decode-chunk raw (ms):     {:?}", decode_ms_samples);
    }
}
