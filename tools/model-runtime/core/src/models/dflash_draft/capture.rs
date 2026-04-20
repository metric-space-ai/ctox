//! Target-feature capture plan used by the DFlash draft.
//!
//! During a target forward the pipeline asks the text model to snapshot
//! the post-layer hidden states at a small set of layer indices. Those
//! snapshots become the `target_hidden_cat` input to the block-
//! diffusion draft one step later — see
//! `models/dflash_draft/model.rs::DFlashDraftModel::forward_hidden`.
//!
//! For Qwen3.5-27B + `z-lab/Qwen3.5-27B-DFlash` the canonical indices
//! are `[1, 16, 31, 46, 61]` (early, quarter, half, three-quarter and
//! pre-final). The draft is trained for exactly this set; changing it
//! would require retraining. Other target/draft pairs carry their own
//! indices in the draft's `config.json` under `dflash_config.target_layer_ids`.

use candle_core::Tensor;

/// Which layers to capture, and the captured tensors after a forward
/// pass.
///
/// Callers construct `FeatureCapture { layer_ids, captured: vec![] }`
/// with `layer_ids` pre-populated (typically from
/// [`crate::models::dflash_draft::DFlashDraftConfig::dflash::target_layer_ids`]),
/// pass `Some(&mut capture)` into the text model's `forward_embeds`,
/// and read `capture.captured` after the call. One tensor per entry in
/// `layer_ids`, pushed in the order the layers were visited (layer
/// loop iterates 0..num_hidden_layers ascending, and `layer_ids` is
/// expected to be sorted ascending too).
///
/// The captured tensors are raw per-layer hidden states
/// `[batch, seq_len, hidden_size]`, the exact shape the target's layer
/// stack passes from one layer to the next. The pipeline concatenates
/// them along the feature dim (not the sequence dim) before handing
/// them to the draft; that's why the draft's `fc` matrix has shape
/// `[5 × hidden, hidden]`.
#[derive(Debug, Default)]
pub struct FeatureCapture {
    /// Layer indices (0-based) to snapshot. Must be ascending and all
    /// within `[0, num_hidden_layers)` of the target model.
    pub layer_ids: Vec<usize>,

    /// Hidden states captured during the forward, one per entry in
    /// `layer_ids`. Cleared at the start of each forward invocation by
    /// the callee; reallocated here is cheap (small Vec of tensor
    /// handles) so we don't bother with `SmallVec`.
    pub captured: Vec<Tensor>,
}

impl FeatureCapture {
    /// Construct an empty capture plan. The caller is expected to set
    /// `layer_ids` before passing the value into a forward.
    pub fn new(layer_ids: Vec<usize>) -> Self {
        Self {
            layer_ids,
            captured: Vec::with_capacity(5),
        }
    }

    /// Return whether a given layer index is in the capture set.
    /// Linear scan — `layer_ids` is tiny (typically 5) so we don't
    /// need a set here.
    pub fn should_capture(&self, layer_idx: usize) -> bool {
        self.layer_ids.contains(&layer_idx)
    }

    /// Drop previously captured tensors. Called by the target model at
    /// the start of each forward pass so consecutive calls don't pile
    /// up stale state.
    pub fn reset(&mut self) {
        self.captured.clear();
    }

    /// Sanity-check the capture output. Called by the pipeline after a
    /// forward to fail loudly on a mismatch (layer_ids says 5 layers
    /// but we only got 4 — indicates an off-by-one or wrong model).
    pub fn validate(&self) -> Result<(), String> {
        if self.captured.len() != self.layer_ids.len() {
            return Err(format!(
                "FeatureCapture: captured {} tensors but layer_ids has {} entries",
                self.captured.len(),
                self.layer_ids.len()
            ));
        }
        Ok(())
    }
}
