//! Concrete `DFlashTargetForward` implementation for the hybrid
//! Qwen3.5 text model (Qwen3.5-27B Gated-DeltaNet target, 64 layers,
//! same arch the reference DFlash implementation targets).
//!
//! Two of the three trait methods are thin accessors — [`embed_tokens`]
//! and [`apply_lm_head`] forward straight to
//! [`crate::vision_models::qwen3_5::Qwen3_5TextModel::embed_tokens_layer`]
//! and
//! [`crate::vision_models::qwen3_5::Qwen3_5TextModel::apply_lm_head`].
//!
//! The third, [`forward_with_capture`], is the one that needs to
//! assemble:
//!   - position_ids (MRoPE, text-only branch, positions
//!     `[past_kv_len..past_kv_len + seq_len]`),
//!   - causal `attention_mask`,
//!   - `FlashParams` + `context_lens`,
//!   - paged-attention metadata (or `None` in the simple non-paged
//!     case),
//! then call
//! `Qwen3_5TextModel::forward_embeds_with_capture`.
//!
//! Commit 6 ships only the accessors and the struct skeleton; the
//! forward body is a `todo!` placeholder. Commit 7 fills it in with
//! the full metadata-assembly logic and the first end-to-end smoke
//! test will follow immediately after. Splitting it this way keeps
//! each commit small enough to review and means the trait-impl
//! structure — where the target lives, how it's constructed, how the
//! struct threads the existing text model reference — is settled
//! before the ~200 lines of forward-metadata plumbing lands.

use std::sync::Arc;

use candle_core::{Result, Tensor};
use candle_nn::Embedding;

use super::capture::FeatureCapture;
use super::target::DFlashTargetForward;
use crate::vision_models::qwen3_5::Qwen3_5TextModel;

/// Owns a shared handle to the Qwen3.5 text model. Created alongside
/// the target pipeline so target and stepper see exactly the same
/// weights and KV cache.
///
/// `Arc<Qwen3_5TextModel>` keeps the struct cheap to clone when the
/// async pipeline needs the stepper on one thread and the scheduler
/// on another.
pub struct Qwen35DFlashTarget {
    text: Arc<Qwen3_5TextModel>,
}

impl Qwen35DFlashTarget {
    pub fn new(text: Arc<Qwen3_5TextModel>) -> Self {
        Self { text }
    }

    pub fn text(&self) -> &Qwen3_5TextModel {
        &self.text
    }
}

impl DFlashTargetForward for Qwen35DFlashTarget {
    fn forward_with_capture(
        &self,
        _input_ids: &Tensor,
        _past_kv_len: usize,
        _capture: &mut FeatureCapture,
    ) -> Result<Tensor> {
        // Lands in commit 7. Keeping the unimplemented marker here
        // rather than at compile time so the trait surface is already
        // usable for the scheduler-wiring commit (commit 7) and for
        // unit tests that mock the target without touching forward.
        candle_core::bail!(
            "Qwen35DFlashTarget::forward_with_capture is not implemented yet — \
             wait for the next dflash commit which lands the position_ids / \
             FlashParams / paged-attn assembly."
        )
    }

    fn embed_tokens(&self) -> &Embedding {
        self.text.embed_tokens_layer()
    }

    fn apply_lm_head(&self, hidden: &Tensor) -> Result<Tensor> {
        self.text.apply_lm_head(hidden)
    }
}
