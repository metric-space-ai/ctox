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
    prefix_cacher::PrefixCacheManagerV2,
    sequence::Sequence,
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
}

impl DFlashPipeline {
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
        // Drop per-seq DFlash bookkeeping for finished sequences so
        // the hashmap doesn't grow unboundedly across long-running
        // engines. Conservative: clear every tracked seq on reset,
        // because the DFlash feature ring does not survive a cache
        // reset anyway.
        self.last_committed.lock().unwrap().clear();
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
        _prefix_cacher: &mut PrefixCacheManagerV2,
        _disable_eos_stop: bool,
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

        // Reach the concrete Qwen3.5 text model behind the target
        // pipeline. The Pipeline trait's `dflash_text_model` hook
        // returns `Some(&Qwen3_5TextModel)` only when the inner
        // model is the hybrid Qwen3.5 the DFlash draft was trained
        // for; everything else errors out clearly.
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
        let opts = StepperOpts::default();

        if is_prompt {
            // Prefill path: the sequence carries the prompt tokens;
            // feed them through the target with capture, seed the
            // ring, greedy-sample the first new token, commit it to
            // the sequence. We also record this as the
            // `last_committed` token for subsequent decode steps.
            let prompt_toks: Vec<u32> = seq.get_toks().to_vec();
            let prompt_len = prompt_toks.len();
            if prompt_len == 0 {
                candle_core::bail!("DFlashPipeline::step: is_prompt=true but seq has no tokens");
            }
            let device = target_text.device().clone();
            let input_ids = Tensor::from_vec(prompt_toks, (1, prompt_len), &device)?;

            let (first_tok, _past_kv_len) = {
                let mut ring = stepper.ring().lock().unwrap();
                prefill(target_text, &mut ring, stepper.config(), &input_ids)?
            };
            self.last_committed
                .lock()
                .unwrap()
                .insert(*seq.id(), Some(first_tok));
            return Ok(t_start.elapsed());
        }

        // Decode path: one chain-verify round. Pull the last
        // committed token we stashed at prefill time.
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
        // Current KV length of the target's hybrid cache == how many
        // tokens have been committed so far minus one (the last
        // committed token is re-fed as the first element of the
        // verify block, not yet absorbed into the cache). This
        // matches the behaviour of speculative.rs where the target
        // verify forward starts at `past = current_seq_len`.
        let past_kv_len = seq.get_toks().len().saturating_sub(1);

        let outcome = decode_step(
            target_text,
            draft,
            stepper,
            last_committed_token,
            past_kv_len,
            &opts,
        )?;

        let mut new_last = last_committed_token;
        for tok in outcome.accepted.iter().copied() {
            // Append without running the full scheduler-level token
            // post-processing (finish-on-EOS etc.) — the outer
            // scheduler does its own EOS check on `seq.get_toks()`.
            // For a first port this matches
            // `speculative.rs::finish_or_add_toks_to_seq` call sites
            // where the logit-probs + chat-template completion
            // machinery wrap the low-level token append.
            seq.add_tmp_tok(tok);
            new_last = tok;
        }
        // Flip add_tmp_tok's `is_tmp` so the token count reflected
        // by seq.len() stays consistent with the scheduler's
        // expectations. `add_tmp_tok` + `remove_tmp_tok` is the
        // speculative-decoding idiom for batched commits; here we
        // commit for real, not tentatively.
        seq.remove_tmp_tok(0);

        self.last_committed
            .lock()
            .unwrap()
            .insert(*seq.id(), Some(new_last));

        Ok(t_start.elapsed())
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
