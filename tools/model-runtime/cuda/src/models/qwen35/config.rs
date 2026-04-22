//! Qwen3.5 architectural constants.
//!
//! The baked-in `QWEN35_27B` constant mirrors the brief's target
//! (FullAttention layers — 40 Q heads / 8 KV heads, GQA 5:1). The
//! same struct carries `gdn_ssm_dim` so the GDN layer composition
//! shares one type; production weights load into this config shape.
//!
//! NOTE: Qwen3.5 hybrid's GDN layers use a *different* head-count
//! mapping than its FullAttention layers (dflash reference: 48 V
//! heads / 16 K heads for GDN vs 40/8 for FullAttention). Those
//! per-variant head counts are kept inside the layer structs today
//! and scoped via TODOs; unify at Phase 4 when we wire real weights.

/// Static layer-shape parameters for Qwen3.5 models.
#[derive(Debug, Clone, Copy)]
pub struct Qwen35Config {
    /// Residual-stream width.
    pub hidden_dim: usize,
    /// Number of query heads per FullAttention layer.
    pub n_q_heads: usize,
    /// Number of key/value heads per FullAttention layer. GQA ratio =
    /// `n_q_heads / n_kv_heads`.
    pub n_kv_heads: usize,
    /// Size of each attention head along the feature axis.
    pub head_dim: usize,
    /// SSM recurrent-state width (`S_v` in the GDN kernel). Equals
    /// `head_dim` on 27B.
    pub gdn_ssm_dim: usize,
    /// RoPE base. Brief target: 10_000. Production 27B GGUF uses
    /// 10_000_000; that switch lands with GGUF-driven config in Phase 4.
    pub rope_theta: f32,
    /// RMSNorm epsilon.
    pub rms_eps: f32,
    /// Upper bound on sequence length / KV cache ring size.
    pub max_position_embeddings: usize,
}

impl Qwen35Config {
    /// Qwen3.5 — 27B hybrid.
    pub const QWEN35_27B: Self = Self {
        hidden_dim: 5120,
        n_q_heads: 40,
        n_kv_heads: 8,
        head_dim: 128,
        gdn_ssm_dim: 128,
        rope_theta: 10_000.0,
        rms_eps: 1e-6,
        max_position_embeddings: 131_072,
    };

    /// Convenience factory matching Agent J's GDN-side API.
    /// Returns the same config as [`Self::QWEN35_27B`].
    pub fn qwen35_27b() -> Self {
        Self::QWEN35_27B
    }

    /// `n_q_heads * head_dim` — Q projection output width.
    pub const fn q_dim(&self) -> usize {
        self.n_q_heads * self.head_dim
    }

    /// `n_kv_heads * head_dim` — K/V projection output width.
    pub const fn kv_dim(&self) -> usize {
        self.n_kv_heads * self.head_dim
    }

    /// Number of Q heads per KV head (GQA group size, FullAttention).
    pub const fn gqa_group(&self) -> usize {
        self.n_q_heads / self.n_kv_heads
    }
}
