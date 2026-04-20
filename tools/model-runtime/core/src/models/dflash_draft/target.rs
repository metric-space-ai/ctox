//! Target-side abstraction for DFlash speculative decoding.
//!
//! The target in a DFlash pipeline is always a hybrid Qwen3.5 (dense
//! attention every 4 layers, Gated DeltaNet in between, 64 layers for
//! the 27B variant). But from the stepper's point of view (commit
//! that follows this one), the target only needs to expose three
//! capabilities:
//!
//!   1. run a forward of `input_ids` with feature capture at the
//!      configured layer indices,
//!   2. expose the token embedding, for the draft to use,
//!   3. expose the lm_head, for the draft to use.
//!
//! Nothing else — the KV cache, paged-attention metadata, device
//! mapping, all stay encapsulated inside the target. That makes the
//! stepper trivially mockable (swap in a hand-built target that
//! returns canned logits + capture tensors for unit tests) and lets
//! the concrete scheduler wiring happen in its own commit without
//! this one growing an extra 300 lines of plumbing.

use candle_core::{Result, Tensor};
use candle_nn::Embedding;

use super::capture::FeatureCapture;

/// Trait the DFlash stepper consumes. Implementations must be
/// `Send + Sync` for integration into the async pipeline loop; our
/// concrete Qwen3.5 implementation satisfies this via its existing
/// `Arc<tokio::sync::Mutex<dyn Pipeline>>` wrapper.
pub trait DFlashTargetForward: Send + Sync {
    /// Run one target forward pass with the given `input_ids`, asking
    /// the text model to snapshot its post-layer hidden states at
    /// `capture.layer_ids`.
    ///
    /// Returns logits of shape `[batch=1, seq, vocab]`. The stepper
    /// consumes only the last `block_size` of those to compare with
    /// the draft's candidates.
    ///
    /// `past_kv_len` is the number of tokens the target has already
    /// processed (i.e. the position of the first element of
    /// `input_ids`). Implementations use this to set RoPE offsets and
    /// paged-attention slot mapping correctly.
    ///
    /// `capture` is reset by the implementation at the start of the
    /// call (same semantics as
    /// [`crate::vision_models::qwen3_5::Qwen3_5TextModel::forward_embeds_with_capture`]).
    fn forward_with_capture(
        &self,
        input_ids: &Tensor,
        past_kv_len: usize,
        capture: &mut FeatureCapture,
    ) -> Result<Tensor>;

    /// Return a reference to the target's token embedding layer.
    /// Shared with the draft for input embedding (the draft has no
    /// embedding of its own).
    fn embed_tokens(&self) -> &Embedding;

    /// Project `hidden` through the target's `lm_head`.
    ///
    /// Modelled as a method instead of an accessor because the target's
    /// live lm_head is typically an `Arc<dyn QuantMethod>` (Q4_K_M after
    /// ISQ), not a plain `Linear` — there's no uniform way to hand out
    /// a borrow that works for both. Input shape `[..., hidden_size]`,
    /// output `[..., vocab_size]`.
    fn apply_lm_head(&self, hidden: &Tensor) -> Result<Tensor>;
}
