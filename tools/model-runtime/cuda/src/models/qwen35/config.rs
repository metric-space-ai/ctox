//! Qwen3.5 architectural constants.
//!
//! Source of truth is the GGUF metadata we ship with the model; the
//! baked-in `QWEN35_27B` constant here mirrors those values so call
//! sites that know they want 27B can avoid threading a loaded config
//! through every kernel launch. Smaller variants (8B, 15B) should
//! declare their own constants following the same shape.

/// Static layer-shape parameters shared across every full-attention
/// block in a single model. These are architectural — they do not
/// change per layer.
#[derive(Debug, Clone, Copy)]
pub struct Qwen35Config {
    /// Residual-stream width. Attention norm + residual adds are sized
    /// to this.
    pub hidden_dim: usize,
    /// Number of query heads per attention layer.
    pub n_q_heads: usize,
    /// Number of key/value heads per attention layer. GQA ratio is
    /// `n_q_heads / n_kv_heads` (5:1 on 27B — each KV head is shared by
    /// 5 Q heads).
    pub n_kv_heads: usize,
    /// Size of each attention head along the feature axis.
    pub head_dim: usize,
    /// RoPE base (`θ` in `θ^{-2i/d}`). 10_000 on the brief's baseline;
    /// production 27B GGUF ships 10_000_000 via `qwen35.rope.freq_base`.
    pub rope_theta: f32,
    /// RMSNorm epsilon.
    pub rms_eps: f32,
    /// Upper bound for the KV cache along its ring axis. Used to size
    /// the cache allocation; does not affect per-step math.
    pub max_position_embeddings: usize,
}

impl Qwen35Config {
    /// Qwen3.5 — 27B hybrid. Matches the parameter counts in the brief;
    /// the dflash reference tree uses a downsized `27B` (24 heads,
    /// head_dim=256) for its compressed-target forward and is NOT the
    /// canonical production variant we target here.
    pub const QWEN35_27B: Self = Self {
        hidden_dim: 5120,
        n_q_heads: 40,
        n_kv_heads: 8,
        head_dim: 128,
        rope_theta: 10_000.0,
        rms_eps: 1e-6,
        max_position_embeddings: 131_072,
    };

    /// Convenience — `n_q_heads * head_dim`, the Q projection's output
    /// width (and the attention output width pre-`w_o`).
    pub const fn q_dim(&self) -> usize {
        self.n_q_heads * self.head_dim
    }

    /// Convenience — `n_kv_heads * head_dim`, the K/V projection's
    /// output width (per-layer KV cache slot width).
    pub const fn kv_dim(&self) -> usize {
        self.n_kv_heads * self.head_dim
    }

    /// Number of Q heads that share each KV head (GQA group size).
    pub const fn gqa_group(&self) -> usize {
        self.n_q_heads / self.n_kv_heads
    }
}
