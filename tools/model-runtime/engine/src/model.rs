//! The `Model` trait — sole abstraction over per-model crates.
//!
//! Each model crate (e.g. `ctox-qwen35-27b`) provides a type that
//! implements this trait. CTOX's serving loop drives inference only
//! through it — never touching model-specific types like
//! `Qwen35Target`, `CudaTensor`, or any CUDA primitive directly.
//!
//! Design stance: the trait is deliberately small. Streaming, batch
//! composition, and KV-pool orchestration live in
//! [`crate::serving`] (landing when the first two model crates are
//! in place and we can see what's genuinely common vs
//! model-specific).

use anyhow::Result;

/// Input to a forward pass — token ids plus any auxiliary state the
/// serving layer tracks per sequence.
#[derive(Debug, Clone)]
pub struct ModelInput<'a> {
    /// Flat sequence of input tokens for the current step. For
    /// chat-style serving this is `[last_committed_token]` after
    /// prefill completes; for prefill it's the whole prompt.
    pub tokens: &'a [i32],

    /// Absolute position of each token in its sequence — used to
    /// drive RoPE / MRoPE. For simple text models this is
    /// `[past_kv_len, past_kv_len + 1, ...]`. Multi-axis variants
    /// (Qwen3.5 MRoPE) require axis 0..2 to hold the text position;
    /// model-specific code fans this out internally.
    pub positions: &'a [i32],

    /// Running KV-cache fill count before this call.
    pub past_kv_len: usize,
}

/// Output of a forward pass — next-token distribution and internal
/// bookkeeping.
#[derive(Debug)]
pub struct ModelOutput {
    /// Log-probabilities (or raw logits, model-specific) over the
    /// vocabulary for the LAST position of the input.
    pub logits: Vec<f32>,

    /// Number of KV-cache slots consumed by this call. Sequence
    /// state layer adds this to `past_kv_len` to advance.
    pub advanced_kv: usize,
}

/// Blanket trait every model crate implements.
///
/// Intentionally not `async_trait` today — kernel launches are
/// synchronous from the CPU's perspective. The async-ness happens
/// one level up in the serving layer (stream composition across
/// concurrent sequences).
pub trait Model: Send + Sync {
    /// Human-readable identifier — e.g. `"qwen35-27b-q4km"`.
    fn id(&self) -> &'static str;

    /// Vocabulary size — needed by samplers.
    fn vocab_size(&self) -> usize;

    /// Tokenize a string → ids.
    fn encode(&self, text: &str) -> Result<Vec<i32>>;

    /// Detokenize ids → string.
    fn decode(&self, ids: &[i32]) -> Result<String>;

    /// Run one forward pass for a single sequence.
    fn forward(&mut self, input: ModelInput<'_>) -> Result<ModelOutput>;
}
