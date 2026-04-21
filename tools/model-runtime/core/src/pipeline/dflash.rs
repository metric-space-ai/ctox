//! DFlash block-diffusion speculative decoding pipeline.
//!
//! Port of `dflash/src/qwen3_dflash_graph.cpp` + `qwen35_target_graph.cpp`
//! from <https://github.com/Luce-Org/lucebox-hub> into the CTOX engine,
//! reusing the candle-backed hybrid Qwen3.5 text model as the target
//! and a freshly-loaded `z-lab/Qwen3.5-27B-DFlash` as the draft.
//!
//! This file lands the pipeline struct, its Pipeline trait
//! implementation, and the delegation methods to the target
//! (PreProcessingMixin / IsqPipelineMixin / CacheManagerMixin /
//! MetadataMixin). The actual `step()` body that drives the
//! DFlash chain-verify loop lives in a follow-up commit — this
//! one only ships a compile-ready skeleton so the next commit's
//! diff is tightly scoped to the accept-loop logic.
//!
//! See `models/dflash_draft/` for the building blocks:
//!   - `DFlashDraftModel`        — the 5-layer block-diffusion draft
//!   - `TargetFeatureRing`       — sliding feature cache between steps
//!   - `DFlashChainStepper`      — chain-verify accept loop
//!   - `DFlashTargetForward`     — target abstraction
//!   - `Qwen35DFlashTarget`      — concrete Qwen3.5 impl

use std::{
    any::Any,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result as anyhowResult;
use candle_core::{Device, Result, Tensor};
use engine_quant::IsqType;
use rand_isaac::Isaac64Rng;
use tokenizers::Tokenizer;

use crate::{
    device_map::DeviceMapper,
    get_mut_arcmutex,
    kv_cache::{CacheManager, HybridCacheManager, NormalCacheManager},
    models::dflash_draft::{
        DFlashChainStepper, DFlashDraftConfig, DFlashDraftModel, TargetFeatureRing,
        DEFAULT_RING_CAP,
    },
    pipeline::sampling::finish_or_add_toks_to_seq,
    prefix_cacher::PrefixCacheManagerV2,
    sampler::Logprobs,
    sequence::{Sequence, SequenceState},
    DeviceMapSetting, Loader, ModelKind, ModelPaths, PagedAttentionConfig, Pipeline, TokenSource,
    TryIntoDType,
};

use super::{
    chat_template::ChatTemplate, AnyMoePipelineMixin, CacheBackendMetadata, CacheInstruction,
    CacheManagerMixin, EitherCache, ForwardInputsResult, GeneralMetadata, IsqPipelineMixin,
    MetadataMixin, ModelCategory, PreProcessingMixin,
};

/// DFlash speculative decoding pipeline.
///
/// Wraps a target pipeline (Qwen3.5-27B hybrid dense+GDN; loaded via
/// the regular vision/qwen3_5 loader) and a standalone draft
/// (`DFlashDraftModel` loaded from safetensors) behind the engine's
/// `Pipeline` trait. Per-step state — last-committed token, feature
/// ring — lives in `state`.
///
/// Concurrency: the current implementation supports a single
/// sequence at a time. Multi-seq support would require one ring +
/// last-committed-token slot per seq; falling back to single-seq
/// keeps the initial port small and matches the reference
/// (`max_seqs = 1` in all of `bench_llm.py`'s invocations). The
/// Pipeline trait's `step` method splits batches of size > 1 into
/// serial calls so the engine-side scheduler is not restricted,
/// only the internal accept-loop.
pub struct DFlashPipeline {
    target: Arc<tokio::sync::Mutex<dyn Pipeline>>,
    target_cache: EitherCache,

    /// Draft model, held as an Arc so the stepper can borrow it
    /// across concurrent step calls without re-loading 3.46 GB from
    /// safetensors every time.
    draft: Arc<DFlashDraftModel>,

    /// Chain-verify driver. Owns the feature ring internally.
    stepper: Arc<DFlashChainStepper>,

    metadata: Arc<GeneralMetadata>,
    category: ModelCategory,

    /// DFlash draft config, kept for quick access to block_size /
    /// target_layer_ids in `step`.
    draft_cfg: DFlashDraftConfig,

    /// Per-sequence bookkeeping. Keyed by `Sequence::id()`.
    /// `None` = no step has run yet for this seq (first-call prefill
    /// still needed). `Some(tok)` = `tok` is the most recently
    /// committed token id for that seq; the next step will condition
    /// on it.
    last_committed: Mutex<std::collections::HashMap<usize, Option<u32>>>,

    /// Tracked separately from `seq.len()` because the target's
    /// hybrid KV cache grows by the full feed length per forward
    /// (block+1 per decode step, not just the accepted tokens — the
    /// first chain-verify port doesn't truncate the rejected tail).
    /// Keyed by `Sequence::id()`; set at prefill, incremented per
    /// decode step.
    past_kv_len: Mutex<std::collections::HashMap<usize, usize>>,
}

impl DFlashPipeline {
    /// Run ONE megakernel-drafter speculative-decode round against
    /// this pipeline's target. Parallel to the DFlash block-diffusion
    /// chain path (`DFlashChainStepper::step`) but using the
    /// Qwen3.5-0.8B megakernel drafter for autoregressive drafting.
    ///
    /// Locks the target pipeline's mutex for the duration of the
    /// target forward. Callers own the drafter mutably and the
    /// committed-token bookkeeping — this method is a primitive that
    /// a dedicated `MegakernelSpecDriver` consumes; the full
    /// generation loop (prompt prefill → repeated `run_megakernel_spec_round`
    /// → EOS check) lives one layer up in the bench / pipeline
    /// driver that wires the drafter prefill and position tracking.
    ///
    /// Returns on-device timings so a bench can attribute wall time
    /// to drafter vs target vs rollback replay.
    ///
    /// Requires the target to be a Qwen3.5 hybrid (gets rejected
    /// otherwise) — that's the architecture the megakernel assumes.
    #[cfg(feature = "cuda")]
    pub async fn run_megakernel_spec_round(
        &self,
        drafter: &mut crate::models::megakernel_drafter::MegakernelDrafter,
        last_committed_token: u32,
        past_kv_len: usize,
        opts: &crate::models::megakernel_drafter::MegakernelSpecOpts,
    ) -> Result<(
        crate::models::megakernel_drafter::SpecOutcome,
        crate::models::megakernel_drafter::SpecTimings,
    )> {
        use crate::models::dflash_draft::Qwen35DFlashTarget;
        use crate::models::megakernel_drafter::spec_round;
        let target_guard = self.target.lock().await;
        let text_model = target_guard.dflash_text_model().ok_or_else(|| {
            candle_core::Error::msg(
                "DFlashPipeline::run_megakernel_spec_round: target is not a \
                 Qwen3.5 vision pipeline; megakernel drafter only pairs with \
                 the Qwen3.5 hybrid family.",
            )
        })?;
        let target = Qwen35DFlashTarget::new(text_model);
        spec_round(&target, drafter, last_committed_token, past_kv_len, opts)
    }

    pub fn new(
        target: Arc<tokio::sync::Mutex<dyn Pipeline>>,
        draft: Arc<DFlashDraftModel>,
    ) -> Result<Self> {
        let metadata = get_mut_arcmutex!(target).get_metadata().clone();
        let category = get_mut_arcmutex!(target).category();
        let target_cache = get_mut_arcmutex!(target).cache().clone();
        let draft_cfg = draft.config().clone();

        // The ring lives on the same device as the target — the
        // captured features come from target forward and the draft
        // reads them as cross-attention KV, so they need to be co-
        // resident. The target's device is what the GeneralMetadata
        // records.
        let target_device = get_mut_arcmutex!(target).device();
        let fused_feature_dim = draft_cfg.fused_target_feature_dim();
        let ring = TargetFeatureRing::new(
            &target_device,
            DEFAULT_RING_CAP,
            fused_feature_dim,
            draft.dtype(),
        )?;
        let stepper = Arc::new(DFlashChainStepper::new(ring, draft_cfg.clone()));

        Ok(Self {
            target,
            target_cache,
            draft,
            stepper,
            metadata,
            category,
            draft_cfg,
            last_committed: Mutex::new(std::collections::HashMap::new()),
            past_kv_len: Mutex::new(std::collections::HashMap::new()),
        })
    }

}

impl PreProcessingMixin for DFlashPipeline {
    fn get_chat_template(&self) -> Option<Arc<ChatTemplate>> {
        get_mut_arcmutex!(self.target).get_chat_template()
    }
    fn get_input_processor_config(&self) -> Option<Arc<dyn Any>> {
        get_mut_arcmutex!(self.target).get_input_processor_config()
    }
    fn get_processor(&self) -> Arc<dyn super::Processor> {
        get_mut_arcmutex!(self.target).get_processor()
    }
}

impl IsqPipelineMixin for DFlashPipeline {
    fn re_isq_model(&mut self, dtype: IsqType) -> anyhow::Result<()> {
        // ISQ on the draft is unsupported — the 3.46 GB BF16 draft
        // stays unquantised. Just forward to target.
        get_mut_arcmutex!(self.target).re_isq_model(dtype)
    }
}

impl CacheManagerMixin for DFlashPipeline {
    fn clone_in_cache(&self, seqs: &mut [&mut Sequence]) {
        // Draft is stateless between steps (no KV cache, feature ring
        // is external state). Only the target's cache is sequence-
        // owned and needs clone-in / clone-out.
        let target = get_mut_arcmutex!(self.target);
        if matches!(target.cache(), EitherCache::Hybrid(_)) {
            HybridCacheManager.clone_in_cache(&*target, seqs, false);
        } else {
            NormalCacheManager.clone_in_cache(&*target, seqs, false);
        }
    }
    fn clone_out_cache(&self, seqs: &mut [&mut Sequence]) {
        let target = get_mut_arcmutex!(self.target);
        if matches!(target.cache(), EitherCache::Hybrid(_)) {
            HybridCacheManager.clone_out_cache(&*target, seqs, false);
        } else {
            NormalCacheManager.clone_out_cache(&*target, seqs, false);
        }
    }
    fn set_none_cache(
        &self,
        seqs: &mut [&mut Sequence],
        reset_non_granular: bool,
        modify_draft_cache: bool,
        load_preallocated_cache: bool,
    ) {
        let _ = modify_draft_cache;
        let target = get_mut_arcmutex!(self.target);
        if matches!(target.cache(), EitherCache::Hybrid(_)) {
            HybridCacheManager.set_none_cache(&*target, seqs, false, load_preallocated_cache);
        } else {
            NormalCacheManager.set_none_cache(&*target, seqs, false, load_preallocated_cache);
        }
        if reset_non_granular {
            self.reset_non_granular_state();
        }
        // Drop all per-seq DFlash bookkeeping AND reset the shared
        // feature ring. Otherwise the next request's prefill would
        // find the ring tail full of the previous generation's
        // features and the draft's cross-attention would produce
        // biased candidates — in practice each new prompt comes out
        // as a continuation of the previous topic.
        self.last_committed.lock().unwrap().clear();
        self.past_kv_len.lock().unwrap().clear();
        if let Ok(mut ring) = self.stepper.ring().lock() {
            ring.reset();
        }
    }
    fn cache(&self) -> &EitherCache {
        &self.target_cache
    }
    fn do_preallocated_cache(&self) -> bool {
        // We allocate the feature ring ourselves in `new`, and the
        // target's preallocated cache is handled by the target's own
        // flag. Return false so the engine doesn't try to double-
        // preallocate anything through this pipeline.
        false
    }
}

impl MetadataMixin for DFlashPipeline {
    fn device(&self) -> Device {
        get_mut_arcmutex!(self.target).device()
    }
    fn tokenizer(&self) -> Option<Arc<Tokenizer>> {
        get_mut_arcmutex!(self.target).tokenizer()
    }
    fn name(&self) -> String {
        format!(
            "DFlash: tgt = `{}`, draft_block_size = {}",
            get_mut_arcmutex!(self.target).name(),
            self.draft_cfg.block_size,
        )
    }
    fn reset_non_granular_state(&self) {
        get_mut_arcmutex!(self.target).reset_non_granular_state();
    }
    fn get_metadata(&self) -> Arc<GeneralMetadata> {
        self.metadata.clone()
    }
    fn device_mapper(&self) -> Option<&dyn DeviceMapper> {
        None
    }
}

#[async_trait::async_trait]
impl Pipeline for DFlashPipeline {
    fn forward_inputs(
        &mut self,
        _inputs: Box<dyn Any>,
        _return_raw_logits: bool,
    ) -> Result<ForwardInputsResult> {
        unreachable!()
    }

    async fn sample_causal_gen(
        &self,
        _seqs: &mut [&mut Sequence],
        _logits: Vec<Tensor>,
        _prefix_cacher: &mut PrefixCacheManagerV2,
        _disable_eos_stop: bool,
        _rng: Arc<std::sync::Mutex<Isaac64Rng>>,
    ) -> Result<()> {
        unreachable!()
    }

    async fn step(
        &mut self,
        input_seqs: &mut [&mut Sequence],
        is_prompt: bool,
        _return_raw_logits: bool,
        prefix_cacher: &mut PrefixCacheManagerV2,
        disable_eos_stop: bool,
        _rng: Arc<Mutex<Isaac64Rng>>,
        backend_metadata: CacheBackendMetadata,
    ) -> Result<Duration> {
        use crate::models::dflash_draft::{decode_step, prefill, StepperOpts};

        // Pre-op cache handling — mirror the SpeculativePipeline
        // behaviour so the engine scheduler's pre/post hooks stay
        // consistent.
        let _post_op = match backend_metadata {
            CacheBackendMetadata::DefaultInstructions { pre_op, post_op } => {
                match pre_op {
                    CacheInstruction::In => self.clone_in_cache(input_seqs),
                    CacheInstruction::Nothing => (),
                    CacheInstruction::Reset {
                        reset_non_granular,
                        load_preallocated_cache,
                    } => self.set_none_cache(
                        input_seqs,
                        reset_non_granular,
                        true,
                        load_preallocated_cache,
                    ),
                    _ => unreachable!("Unreachable PRE cache op."),
                }
                Some(post_op)
            }
            CacheBackendMetadata::PagedAttention { .. } => {
                self.clone_in_cache(input_seqs);
                None
            }
        };

        // Single-sequence only in this first port (`max_seqs = 1` is
        // also what the dflash reference benchmarks). Batching
        // across concurrent requests would need one feature ring +
        // last-token slot per seq; add when the first single-seq
        // results are in.
        if input_seqs.len() != 1 {
            candle_core::bail!(
                "DFlashPipeline::step: batch size {} > 1 is not supported yet",
                input_seqs.len()
            );
        }

        let t_start = std::time::Instant::now();

        // Decide up-front which input_seqs[0].id() we're operating on.
        // We run the target forward under `target_guard`, release the
        // guard, then fold accepted tokens into the sequence via
        // `finish_or_add_toks_to_seq`. The guard must be dropped before
        // that call — `finish_or_add_toks_to_seq` goes through
        // `this.get_metadata()`, which on DFlashPipeline re-locks
        // `self.target` and would deadlock against a held guard.
        let opts = StepperOpts::default();

        // ── Phase 1: run the target forward under the guard ───────────
        // Returns:
        //   * `outcome_opt`  — decode_step result (None on prefill path)
        //   * `eos_owned`    — the target's EOS tokens (pulled from
        //                      metadata before we drop the guard so the
        //                      Phase 2 accept loop can run an EOS check
        //                      without re-locking).
        //   * `first_tok`    — on prefill, the greedy-sampled first
        //                      output token (to be committed via
        //                      `finish_or_add_toks_to_seq` after the
        //                      guard is released). None on decode.
        let (outcome_opt, eos_owned, first_tok_on_prefill) = {
            let target_guard = self.target.lock().await;
            let target_text = target_guard.dflash_text_model().ok_or_else(|| {
                candle_core::Error::msg(
                    "DFlashPipeline::step: target is not a Qwen3.5 vision pipeline. \
                     DFlash requires the hybrid-Qwen3.5 target the reference draft \
                     was trained for (e.g. Qwen/Qwen3.5-27B).",
                )
            })?;
            let draft = self.draft.as_ref();
            let stepper = self.stepper.as_ref();
            let seq = &mut input_seqs[0];
            let eos_owned = target_guard.get_metadata().eos_tok.clone();

            if is_prompt {
                // Prefill path: feed prompt tokens through the target
                // with capture, seed the ring, greedy-sample the first
                // new token, stash it as last_committed + init
                // past_kv_len. We do NOT feed that first token into the
                // sequence here — the next decode-phase call will pick
                // it up through `last_committed` and re-feed it as the
                // leading token of the B+1 chain-verify batch. The
                // engine scheduler drives prompt→decode transitions.
                //
                // Defensive reset: the scheduler's pre-op cache
                // instruction doesn't always flow through as a
                // `Reset`, but a `is_prompt=true` step is by
                // definition the start of a fresh request. Wipe
                //   * the DFlash feature ring,
                //   * the per-seq last_committed / past_kv_len maps,
                //   * the target's hybrid KV cache (attention +
                //     recurrent) — so Gated-DeltaNet state from a
                //     previous request doesn't carry over and bias
                //     the new prompt toward the previous topic.
                //     Observed without this: three unrelated prompts
                //     all returning Fibonacci continuations of the
                //     first request.
                {
                    let mut ring = stepper.ring().lock().unwrap();
                    ring.reset();
                }
                self.last_committed.lock().unwrap().clear();
                self.past_kv_len.lock().unwrap().clear();
                target_text.dflash_reset_cache();
                let prompt_toks: Vec<u32> = seq.get_toks().to_vec();
                let prompt_len = prompt_toks.len();
                if prompt_len == 0 {
                    candle_core::bail!("DFlashPipeline::step: is_prompt=true but seq has no tokens");
                }
                let device = target_text.device().clone();
                let input_ids = Tensor::from_vec(prompt_toks, (1, prompt_len), &device)?;

                let (first_tok, kv_after_prefill) = {
                    let mut ring = stepper.ring().lock().unwrap();
                    prefill(target_text, &mut ring, stepper.config(), &input_ids)?
                };
                self.last_committed
                    .lock()
                    .unwrap()
                    .insert(*seq.id(), Some(first_tok));
                self.past_kv_len
                    .lock()
                    .unwrap()
                    .insert(*seq.id(), kv_after_prefill);
                (None, eos_owned, Some(first_tok))
            } else {
                let last_committed_token = {
                    let map = self.last_committed.lock().unwrap();
                    match map.get(seq.id()).copied().flatten() {
                        Some(t) => t,
                        None => candle_core::bail!(
                            "DFlashPipeline::step: decode without prior prefill for seq {}",
                            seq.id()
                        ),
                    }
                };
                let past_kv_len = {
                    let map = self.past_kv_len.lock().unwrap();
                    *map.get(seq.id()).ok_or_else(|| {
                        candle_core::Error::msg(format!(
                            "DFlashPipeline::step: no past_kv_len for seq {} — prefill missing?",
                            seq.id()
                        ))
                    })?
                };
                let outcome = decode_step(
                    target_text,
                    draft,
                    stepper,
                    last_committed_token,
                    past_kv_len,
                    &opts,
                )?;
                (Some(outcome), eos_owned, None)
            }
        }; // target_guard released here

        // ── Phase 2a: on prefill, commit the greedy-sampled first token
        // so the client's output stream includes it. Without this step
        // the first output token is silently dropped — the decode
        // chain-verify feeds `last_committed` as the *anchor* of the
        // B+1 batch, not as a new output, so only target_choices[0..]
        // would otherwise reach the sequence.
        let seq = &mut input_seqs[0];
        let eos_tok_slice = if disable_eos_stop || eos_owned.is_empty() {
            None
        } else {
            Some(eos_owned.as_slice())
        };
        if let Some(first_tok) = first_tok_on_prefill {
            let lp = Logprobs {
                token: first_tok,
                logprob: 0.0,
                bytes: None,
                top_logprobs: None,
            };
            finish_or_add_toks_to_seq(self, prefix_cacher, seq, lp, eos_tok_slice, true).await?;
            return Ok(t_start.elapsed());
        }

        let outcome = outcome_opt.expect("decode path sets outcome");

        // ── Phase 2b: commit accepted decode tokens ──────────────────
        // `finish_or_add_toks_to_seq` decodes each token to bytes via
        // the pipeline's tok_env, runs the EOS / stop-tok / max-len
        // check, writes `seq.completion_bytes`, and — on `is_done` —
        // flips `SequenceState::Done(reason)` AND dispatches a
        // `Response::Done(...)` through the seq's responder channel.
        // That responder side is the bit that was missing from the
        // first-smoke path: without it, the streamer sits on an empty
        // rx after `response.created` and the client never sees
        // `response.completed`.
        //
        // Greedy argmax verify ⇒ logprob is unknown; we emit 0.0 as a
        // placeholder. DDTree verify (follow-up) can wire real
        // logprobs through `DraftStepOutput.top_k_logprobs`.
        let mut new_last = {
            let map = self.last_committed.lock().unwrap();
            map.get(seq.id()).copied().flatten().unwrap_or(0)
        };
        let n_accepted = outcome.accepted.len();
        for tok in outcome.accepted.iter().copied() {
            let lp = Logprobs {
                token: tok,
                logprob: 0.0,
                bytes: None,
                top_logprobs: None,
            };
            finish_or_add_toks_to_seq(self, prefix_cacher, seq, lp, eos_tok_slice, true).await?;
            new_last = tok;
            // Stop as soon as finish_or_add flipped the seq to Done —
            // no point over-committing rejected-after-stop draft tokens.
            if matches!(seq.getstate(), SequenceState::Done(_)) {
                break;
            }
        }

        self.last_committed
            .lock()
            .unwrap()
            .insert(*seq.id(), Some(new_last));

        // Advance the target KV tracker by `draft_accepted + 1` — the
        // number of feed positions the commit replay actually wrote
        // into the cache (last_tok + draft_accepted matched drafts).
        // The committed boundary / bonus token (target_choices[K] on
        // partial, target_choices[B] on full) has no KV entry yet;
        // it picks up its KV next step as feed[0] of that step's
        // verify forward. See the stepper's commit-replay comment.
        let commit_len = outcome.draft_accepted + 1;
        self.past_kv_len
            .lock()
            .unwrap()
            .entry(*seq.id())
            .and_modify(|v| *v += commit_len)
            .or_insert(commit_len);

        // Telemetry: rolling accept-rate + per-step tok/s, same
        // format as `SpeculativePipeline`'s so `grep accept-rate`
        // works across both.
        let elapsed = t_start.elapsed();
        let step_tps = if elapsed.as_secs_f64() > 0.0 {
            n_accepted as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        tracing::info!(
            "dflash step: accepted={} draft_accepted={} step_time={:?} step_tok/s={:.1}",
            n_accepted,
            outcome.draft_accepted,
            elapsed,
            step_tps
        );

        Ok(elapsed)
    }

    fn category(&self) -> ModelCategory {
        self.category.clone()
    }
}

impl AnyMoePipelineMixin for DFlashPipeline {}

/// Loader for a DFlash pipeline.
///
/// Wraps an existing target `Box<dyn Loader>` with the draft
/// safetensors + config paths. At `load_model_from_hf` time:
///   1. delegates to the target loader to produce the `Pipeline`
///      for the Qwen3.5 target,
///   2. parses `draft_config.json` into a [`DFlashDraftConfig`],
///   3. mmap-loads the draft safetensors into a
///      [`DFlashDraftModel`] on the target's device,
///   4. wraps both in a `DFlashPipeline` and returns it as an
///      `Arc<Mutex<dyn Pipeline>>`.
///
/// Parallel shape to [`super::speculative::SpeculativeLoader`], but
/// the draft is loaded directly from a path instead of delegating to
/// a second `Box<dyn Loader>` — the DFlash draft isn't a standard
/// Qwen3 checkpoint and has no matching `NormalLoaderType`.
pub struct DFlashLoader {
    pub target: Box<dyn Loader>,
    pub draft_safetensors: PathBuf,
    pub draft_config: PathBuf,
}

impl DFlashLoader {
    fn load_draft(&self, device: &Device) -> anyhowResult<Arc<DFlashDraftModel>> {
        use candle_nn::VarBuilder;

        // Parse the DFlash draft config.
        let cfg_text = std::fs::read_to_string(&self.draft_config).map_err(|e| {
            anyhow::anyhow!(
                "DFlashLoader: cannot read draft config at {}: {e}",
                self.draft_config.display()
            )
        })?;
        let cfg: DFlashDraftConfig = serde_json::from_str(&cfg_text).map_err(|e| {
            anyhow::anyhow!(
                "DFlashLoader: parse draft config {}: {e}",
                self.draft_config.display()
            )
        })?;

        // mmap-load the safetensors. The draft is 3.46 GB BF16 on
        // Qwen3.5-27B-DFlash; mmap means we don't pay the eager-copy
        // tax — only the pages the draft touches during forward are
        // resident.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[self.draft_safetensors.clone()],
                candle_core::DType::BF16,
                device,
            )
            .map_err(|e| {
                anyhow::anyhow!(
                    "DFlashLoader: mmap draft safetensors {}: {e}",
                    self.draft_safetensors.display()
                )
            })?
        };

        let draft = DFlashDraftModel::load(vb, cfg).map_err(|e| {
            anyhow::anyhow!("DFlashLoader: build DFlashDraftModel: {e}")
        })?;
        Ok(Arc::new(draft))
    }
}

impl Loader for DFlashLoader {
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn load_model_from_hf(
        &self,
        revision: Option<String>,
        token_source: TokenSource,
        dtype: &dyn TryIntoDType,
        device: &Device,
        silent: bool,
        mapper: DeviceMapSetting,
        in_situ_quant: Option<IsqType>,
        paged_attn_config: Option<PagedAttentionConfig>,
    ) -> anyhowResult<Arc<tokio::sync::Mutex<dyn Pipeline + Send + Sync>>> {
        let target = self.target.load_model_from_hf(
            revision,
            token_source,
            dtype,
            device,
            silent,
            mapper,
            in_situ_quant,
            paged_attn_config,
        )?;
        let draft = self.load_draft(device)?;
        let pipeline = DFlashPipeline::new(target, draft)?;
        Ok(Arc::new(tokio::sync::Mutex::new(pipeline)))
    }

    #[allow(clippy::type_complexity, clippy::too_many_arguments, clippy::borrowed_box)]
    fn load_model_from_path(
        &self,
        paths: &Box<dyn ModelPaths>,
        dtype: &dyn TryIntoDType,
        device: &Device,
        silent: bool,
        mapper: DeviceMapSetting,
        in_situ_quant: Option<IsqType>,
        paged_attn_config: Option<PagedAttentionConfig>,
    ) -> anyhowResult<Arc<tokio::sync::Mutex<dyn Pipeline + Send + Sync>>> {
        let target = self.target.load_model_from_path(
            paths,
            dtype,
            device,
            silent,
            mapper,
            in_situ_quant,
            paged_attn_config,
        )?;
        let draft = self.load_draft(device)?;
        let pipeline = DFlashPipeline::new(target, draft)?;
        Ok(Arc::new(tokio::sync::Mutex::new(pipeline)))
    }

    fn get_id(&self) -> String {
        format!(
            "DFlash: tgt = `{}`, draft = `{}`",
            self.target.get_id(),
            self.draft_safetensors.display()
        )
    }

    fn get_kind(&self) -> ModelKind {
        // DFlash isn't enumerated in ModelKind yet — report the
        // target's kind so downstream diagnostics (logging, engine-
        // info) still produce sensible output. If/when a dedicated
        // `ModelKind::DFlash` variant is added this flips over.
        self.target.get_kind()
    }
}
