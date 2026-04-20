//! Draft-side of a DFlash speculative step.
//!
//! Given:
//!   - the most recently committed target token,
//!   - the current [`TargetFeatureRing`] context window,
//!   - the target's token embedding layer (shared with the draft),
//!   - the target's `lm_head` (shared with the draft),
//!
//! [`DFlashDraftRunner`] produces `block_size` candidate tokens in a
//! single forward pass of the draft. It does **not** talk to the
//! target's transformer — the assumption is that the caller (the
//! pipeline on the other end) has already populated the ring via
//! [`TargetFeatureRing::append`] from the previous verify step's
//! capture, and will hand the resulting candidate tokens to the
//! target for verification next.
//!
//! Split this way on purpose: the draft run is stateless between
//! steps (no KV cache, no recurrent state), so the call is pure — you
//! can unit-test it with any set of mock tensors for the target
//! embed/lm_head. The target-side of the step, which has all the
//! PagedAttention + hybrid-cache bookkeeping, lives in a follow-up
//! commit.

use candle_core::{DType, IndexOp, Result, Tensor, D};
use candle_nn::Embedding;

use super::config::DFlashDraftConfig;
use super::model::DFlashDraftModel;
use super::ring::TargetFeatureRing;

/// Output of one draft step.
#[derive(Debug, Clone)]
pub struct DraftStepOutput {
    /// The `block_size` candidate tokens (greedy argmax per position).
    pub candidates: Vec<u32>,

    /// Top-K token ids per position, `[block_size, k]`. For chain
    /// verify (commit 5) only `candidates` is consumed; DDTree
    /// (commit 6) uses the per-position top-K to build its tree.
    pub top_k_ids: Vec<Vec<u32>>,

    /// Top-K log-probabilities per position, same shape as `top_k_ids`.
    pub top_k_logprobs: Vec<Vec<f32>>,
}

/// Configuration for [`DFlashDraftRunner::step`]. Kept separate from
/// [`DFlashDraftConfig`] so the runner can be parametrised per-call
/// (e.g. `top_k` tweaks for DDTree mode) without editing the model
/// config.
#[derive(Debug, Clone)]
pub struct DraftStepOpts {
    /// Number of top tokens per position to return. Used by DDTree
    /// (commit 6) — chain-verify only needs k=1. A modest default of
    /// 8 keeps the heap manageable without tuning.
    pub top_k: usize,

    /// How many recently-committed target tokens to use as cross-
    /// attention context. Must be ≥ 1 and ≤ the ring's current valid
    /// length. The caller sizes this to the ring's available history;
    /// typical values are 16–64 for interactive chat, up to the full
    /// ring capacity for long-context decode.
    pub ctx_len: usize,
}

impl Default for DraftStepOpts {
    fn default() -> Self {
        Self { top_k: 8, ctx_len: 64 }
    }
}

/// Pure draft runner. Holds nothing beyond references — lightweight,
/// fully reusable, no interior mutability. One instance per loaded
/// draft+target pair, shared across pipeline instances.
///
/// The `lm_head` projection is represented as a closure passed into
/// [`Self::step`] so the runner doesn't need to know whether the
/// target's lm_head is a plain `Linear` (prefill path) or a
/// `QuantMethod` (post-ISQ path). The caller wraps the right op as a
/// `Fn(&Tensor) -> Result<Tensor>`.
pub struct DFlashDraftRunner<'a> {
    draft: &'a DFlashDraftModel,
    target_embed: &'a Embedding,
}

impl<'a> DFlashDraftRunner<'a> {
    pub fn new(draft: &'a DFlashDraftModel, target_embed: &'a Embedding) -> Self {
        Self {
            draft,
            target_embed,
        }
    }

    /// Run one draft forward.
    ///
    /// - `last_committed_token`: id of the token the target most
    ///   recently accepted. The draft conditions its first mask on
    ///   this token.
    /// - `ring`: the current target-feature history. The runner
    ///   borrows `ctx_len` rows starting from the tail.
    /// - `opts`: per-call knobs — see [`DraftStepOpts`].
    /// - `apply_lm_head`: projection onto the vocabulary; see
    ///   [`crate::models::dflash_draft::DFlashTargetForward::apply_lm_head`].
    ///
    /// Returns the `block_size` candidate tokens plus their per-
    /// position top-K distributions.
    pub fn step<F>(
        &self,
        last_committed_token: u32,
        ring: &TargetFeatureRing,
        opts: &DraftStepOpts,
        apply_lm_head: F,
    ) -> Result<DraftStepOutput>
    where
        F: FnOnce(&Tensor) -> Result<Tensor>,
    {
        let cfg: &DFlashDraftConfig = self.draft.config();
        if ring.is_empty() {
            candle_core::bail!(
                "DFlashDraftRunner::step requires a non-empty feature ring. \
                 Run target prefill first so the ring has at least one row."
            );
        }

        let ctx_len = opts.ctx_len.clamp(1, ring.len());
        let device = self.draft.device();
        let dtype = self.draft.dtype();

        // ── Build input_ids = [last_tok, MASK × (block_size - 1)]
        //    The exact shape the trained draft expects: position 0 is
        //    the last real target token, positions 1..block_size are
        //    filled with the mask id and get denoised in the forward.
        let block = cfg.block_size;
        let mask_id = cfg.dflash.mask_token_id;
        let mut input_buf = Vec::with_capacity(block);
        input_buf.push(last_committed_token);
        for _ in 1..block {
            input_buf.push(mask_id);
        }
        let input_ids = Tensor::from_vec(input_buf, (1, block), device)?;

        // ── Fetch the context window from the ring and reshape to
        //    `[B=1, ctx_len, fused_feature_dim]`. The ring stores rows
        //    in `[ctx_len, fused_feature_dim]`, contiguous.
        let ctx = ring.window(ctx_len)?;
        let ctx = ctx.unsqueeze(0)?; // [1, ctx_len, fused_feature_dim]
        // Match dtype with the draft (both should be BF16 in practice;
        // this guards against a CPU-test path where the ring was F32).
        let ctx = if ctx.dtype() == dtype {
            ctx
        } else {
            ctx.to_dtype(dtype)?
        };

        // ── Draft forward → logits `[1, block_size, vocab]`.
        let logits = self.draft.forward_with_lm_head(
            &input_ids,
            &ctx,
            self.target_embed,
            apply_lm_head,
        )?;

        // ── Extract top-K per block position.
        //    Work in F32 for numerical stability of log_softmax.
        let logits_f32 = logits.to_dtype(DType::F32)?;
        let log_probs = candle_nn::ops::log_softmax(&logits_f32, D::Minus1)?;
        // log_probs: [1, block_size, vocab]

        let top_k = opts.top_k.max(1);
        let mut candidates = Vec::with_capacity(block);
        let mut top_k_ids = Vec::with_capacity(block);
        let mut top_k_lp = Vec::with_capacity(block);

        for i in 0..block {
            let row = log_probs.i((0, i))?; // [vocab]
            let (ids, lp) = top_k_from_row(&row, top_k)?;
            candidates.push(ids[0]);
            top_k_ids.push(ids);
            top_k_lp.push(lp);
        }

        Ok(DraftStepOutput {
            candidates,
            top_k_ids,
            top_k_logprobs: top_k_lp,
        })
    }
}

/// Return the top-`k` (id, log_prob) entries of a 1-D log-probability
/// tensor, sorted descending by log-prob. CPU-bound, host-side sort —
/// the draft's vocab is ~248K; O(V log V) is tolerable for k=8 calls
/// at 16 positions per step (≈60 µs total on a typical CPU, which is
/// far below the GPU cost of a single layer's matmul).
fn top_k_from_row(row: &Tensor, k: usize) -> Result<(Vec<u32>, Vec<f32>)> {
    let host: Vec<f32> = row.to_dtype(DType::F32)?.to_vec1()?;
    let mut idx: Vec<u32> = (0..host.len() as u32).collect();
    let k = k.min(host.len());
    // Partial select for k: sort descending by host value, take k.
    idx.sort_by(|&a, &b| {
        host[b as usize]
            .partial_cmp(&host[a as usize])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let ids: Vec<u32> = idx.into_iter().take(k).collect();
    let lp: Vec<f32> = ids.iter().map(|&i| host[i as usize]).collect();
    Ok((ids, lp))
}
