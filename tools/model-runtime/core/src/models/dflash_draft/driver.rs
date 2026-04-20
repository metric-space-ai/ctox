//! End-to-end DFlash speculative decoding driver.
//!
//! Pulls the prefill + chain-verify decode loop together as a pure
//! function over concrete types (`Qwen3_5TextModel` for the target,
//! `DFlashDraftModel` for the draft). Intentionally **not** hidden
//! behind the `Pipeline` trait: the abstraction the scheduler wants
//! (step per async call, per sequence, with cache-manager hooks)
//! doesn't mesh naturally with dflash's stateful ring + last-token
//! bookkeeping. The trait-level wiring that calls this driver from
//! `DFlashPipeline::step` lands in the next commit; this one is
//! fully unit-testable in isolation.
//!
//! Flow per user request:
//!
//!   1. `prefill(target, ring, cfg, input_ids) -> (sampled_tok,
//!      new_past_kv_len)` — runs one target forward over the entire
//!      prompt with feature capture at `target_layer_ids`, pushes the
//!      captured rows into the ring, and greedy-samples the first
//!      new token from the last-position logit row.
//!   2. Per subsequent decode turn, `decode_step(target, draft,
//!      stepper, last_committed_token, past_kv_len, opts)` invokes
//!      `DFlashChainStepper::step` against the concrete target.
//!      Returns `StepOutcome { accepted, draft_accepted }`.

use candle_core::{DType, IndexOp, Result, Tensor};

use super::capture::FeatureCapture;
use super::config::DFlashDraftConfig;
use super::model::DFlashDraftModel;
use super::qwen35_target::Qwen35DFlashTarget;
use super::ring::TargetFeatureRing;
use super::stepper::{fuse_captured_features, DFlashChainStepper, StepOutcome, StepperOpts};
use crate::vision_models::qwen3_5::Qwen3_5TextModel;

/// Run the initial prefill forward: feed the prompt through the
/// target with feature capture, push the captures into the `ring`,
/// and greedy-sample the first new token.
///
/// `ring` is mutated in place — one row per prompt token gets
/// appended. Returns `(sampled_token, new_past_kv_len)` where
/// `new_past_kv_len == input_ids.dim(1)` on a clean start.
pub fn prefill(
    target: &Qwen3_5TextModel,
    ring: &mut TargetFeatureRing,
    cfg: &DFlashDraftConfig,
    input_ids: &Tensor,
) -> Result<(u32, usize)> {
    let prompt_len = input_ids.dim(1)?;
    if prompt_len == 0 {
        candle_core::bail!("prefill: empty prompt");
    }

    let mut capture = FeatureCapture::new(cfg.dflash.target_layer_ids.clone());
    let logits = target.forward_with_dflash_capture(input_ids, 0, Some(&mut capture))?;
    capture.validate().map_err(candle_core::Error::msg)?;

    // Seed the ring with one feature row per prompt token. The
    // capture tensors are `[1, prompt_len, hidden]` each; fuse
    // across the layer dim → `[prompt_len, fused_dim]`.
    let seeded = fuse_captured_features(&capture, 0, prompt_len)?;
    ring.append(&seeded)?;

    // Greedy-sample the first new token from position prompt_len-1
    // of the logits tensor.
    let last_logits = logits.i((0, prompt_len - 1))?;
    let token = argmax_u32(&last_logits)?;
    Ok((token, prompt_len))
}

/// Run one chain-verify decode step. Thin wrapper around
/// [`DFlashChainStepper::step`] with the concrete Qwen3.5 target
/// resolved to its trait impl.
pub fn decode_step(
    target_text: &Qwen3_5TextModel,
    draft: &DFlashDraftModel,
    stepper: &DFlashChainStepper,
    last_committed_token: u32,
    past_kv_len: usize,
    opts: &StepperOpts,
) -> Result<StepOutcome> {
    let target = Qwen35DFlashTarget::new(target_text);
    stepper.step(&target, draft, last_committed_token, past_kv_len, opts)
}

/// Summary of a single greedy run.
pub struct GreedyRunOutcome {
    /// Tokens produced by the model (does not include the prompt).
    pub generated_tokens: Vec<u32>,
    /// Decode tok/s measured from the end of prefill to the last
    /// committed token.
    pub decode_tok_per_s: f64,
    /// How many draft candidates the target validated across the
    /// whole run. Divided by `draft_steps` gives mean acceptance
    /// length — the core metric the dflash paper cites.
    pub draft_accepted_total: usize,
    /// Number of verify steps invoked. One call to
    /// [`decode_step`] == one draft+verify round.
    pub draft_steps: usize,
}

/// Convenience greedy generator. Prefill + decode loop until either
/// `max_new_tokens` is reached or the model emits a token in
/// `eos_ids`. Synchronous, single-sequence. Exists so a standalone
/// smoke-bench binary can call one function and get end-to-end
/// numbers without reimplementing prefill + decode-loop plumbing.
pub fn run_greedy(
    target_text: &Qwen3_5TextModel,
    draft: &DFlashDraftModel,
    stepper: &DFlashChainStepper,
    prompt_ids: &Tensor,
    max_new_tokens: usize,
    eos_ids: &[u32],
    opts: &StepperOpts,
) -> Result<GreedyRunOutcome> {
    // Prefill: seed the ring and pick the first new token. The
    // stepper owns the ring behind a Mutex; lock for the append.
    let (first_tok, mut past_kv_len) = {
        let mut ring = stepper.ring().lock().unwrap();
        prefill(target_text, &mut ring, stepper.config(), prompt_ids)?
    };
    let mut generated: Vec<u32> = Vec::with_capacity(max_new_tokens);
    generated.push(first_tok);
    if eos_ids.contains(&first_tok) || generated.len() >= max_new_tokens {
        return Ok(finalize(&generated, None, 0, 0));
    }

    let t_start = std::time::Instant::now();
    let mut last_tok = first_tok;
    let mut draft_accepted_total = 0usize;
    let mut draft_steps = 0usize;

    while generated.len() < max_new_tokens {
        let outcome = decode_step(
            target_text,
            draft,
            stepper,
            last_tok,
            past_kv_len,
            opts,
        )?;
        draft_steps += 1;
        draft_accepted_total += outcome.draft_accepted;

        let n_accepted = outcome.accepted.len();
        for (i, tok) in outcome.accepted.iter().copied().enumerate() {
            generated.push(tok);
            if eos_ids.contains(&tok) {
                let _ = i; // no further use
                return Ok(finalize(
                    &generated,
                    Some(t_start),
                    draft_accepted_total,
                    draft_steps,
                ));
            }
            if generated.len() >= max_new_tokens {
                return Ok(finalize(
                    &generated,
                    Some(t_start),
                    draft_accepted_total,
                    draft_steps,
                ));
            }
        }
        if let Some(&last) = outcome.accepted.last() {
            last_tok = last;
        }
        past_kv_len += n_accepted;
    }

    Ok(finalize(
        &generated,
        Some(t_start),
        draft_accepted_total,
        draft_steps,
    ))
}

fn finalize(
    generated: &[u32],
    t_start: Option<std::time::Instant>,
    draft_accepted_total: usize,
    draft_steps: usize,
) -> GreedyRunOutcome {
    let decode_secs = t_start.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
    let new_tokens = generated.len().saturating_sub(1);
    let tps = if decode_secs > 0.0 {
        new_tokens as f64 / decode_secs
    } else {
        0.0
    };
    GreedyRunOutcome {
        generated_tokens: generated.to_vec(),
        decode_tok_per_s: tps,
        draft_accepted_total,
        draft_steps,
    }
}

/// Host-side argmax over a vocab row.
fn argmax_u32(row: &Tensor) -> Result<u32> {
    let host: Vec<f32> = row.to_dtype(DType::F32)?.to_vec1()?;
    let mut best_i = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in host.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best_i = i;
        }
    }
    Ok(best_i as u32)
}
