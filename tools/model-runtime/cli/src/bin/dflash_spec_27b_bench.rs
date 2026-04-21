//! End-to-end reference-matching speculative decode bench:
//! `z-lab/Qwen3.5-27B-DFlash` (block-diffusion draft, 3.46 GB BF16)
//! + `Qwen/Qwen3.5-27B` target (Q4_K_M ISQ), with DDTree tree verify
//! enabled via `DFLASH_USE_TREE_VERIFY=1` inside `run_greedy`.
//!
//! This is the config the reference implementation hits 130 tok/s
//! on HumanEval at AL 8.3 (`dflash/RESULTS.md`). The mirror-image on
//! our engine side uses the same draft architecture + same tree
//! verify + same budget — the only difference is the target matmul
//! path (candle Q4K ISQ vs ggml Q4_K_M hand-tuned), which is a
//! per-forward constant ~1.3-1.5× off the reference.
//!
//! Build:
//!   cargo build --release -p ctox-engine-cli --features cuda \
//!       --bin dflash-spec-27b-bench
//!
//! Run (chain verify):
//!   target/release/dflash-spec-27b-bench \
//!       --target-model-id Qwen/Qwen3.5-27B \
//!       --draft-path /mnt/hfcache/…/snapshots/<hash> \
//!       --prompt-ids 1,2,3 --n-tokens 256
//!
//! Run (DDTree tree verify, the 130-tok/s config):
//!   DFLASH_USE_TREE_VERIFY=1 DFLASH_DDTREE_BUDGET=22 \
//!   target/release/dflash-spec-27b-bench …
//!
//! NB: the DFlash draft conditions on the target's hidden states
//! captured at layers `[1, 16, 31, 46, 61]` (the 27B config). The
//! engine-side stepper manages the feature ring automatically; all
//! the bench has to do is construct the stepper and call `run_greedy`.

#![cfg(feature = "cuda")]

use anyhow::{anyhow, Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use clap::Parser;
use engine_core::{
    run_greedy, DFlashChainStepper, DFlashDraftConfig, DFlashDraftModel, DeviceMapSetting,
    StepperOpts, TargetFeatureRing, TokenSource, DEFAULT_RING_CAP,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(about = "DFlash block-diffusion + DDTree end-to-end spec-decode bench (reference config)")]
struct Args {
    /// HF model id for the target (resolved via HF_HOME cache).
    #[arg(long, default_value = "Qwen/Qwen3.5-27B")]
    target_model_id: String,

    /// Local snapshot dir for z-lab/Qwen3.5-27B-DFlash.
    #[arg(long)]
    draft_path: PathBuf,

    /// Prompt token ids (comma-separated).
    #[arg(long, default_value = "1,2,3,4,5")]
    prompt_ids: String,

    /// Number of new tokens to generate (including the first token
    /// from target prefill).
    #[arg(long, default_value_t = 128)]
    n_tokens: usize,

    /// How many recently-committed target tokens to use as the
    /// draft's cross-attention context (ring window).
    #[arg(long, default_value_t = 64)]
    ctx_len: usize,

    /// Top-K per draft position — forwarded to the DDTree builder
    /// when DFLASH_USE_TREE_VERIFY=1. Chain mode ignores this.
    #[arg(long, default_value_t = 8)]
    draft_top_k: usize,

    /// CUDA device ordinal.
    #[arg(long, default_value_t = 0)]
    device: usize,

    /// Apply Q4_K_M ISQ on the target. Default on — BF16 27B
    /// (~54 GB) does not fit on a 48 GB A6000.
    #[arg(long, default_value_t = true)]
    q4k: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .compact()
        .init();

    let args = Args::parse();
    let device = Device::new_cuda(args.device)
        .with_context(|| format!("open CUDA device {}", args.device))?;
    eprintln!("opened CUDA:{}", args.device);

    // ── Target: 27B Qwen3.5 via engine vision loader.
    eprintln!(
        "building target loader for {} (Q4_K_M ISQ = {})",
        args.target_model_id, args.q4k
    );
    let target_loader = engine_core::VisionLoaderBuilder::new(
        engine_core::VisionSpecificConfig::default(),
        None,
        None,
        Some(args.target_model_id.clone()),
        None,
    )
    .build(Some(engine_core::VisionLoaderType::Qwen3_5));
    let isq = if args.q4k {
        Some(engine_core::IsqType::Q4K)
    } else {
        None
    };
    let t_target_load = Instant::now();
    let pipeline = target_loader
        .load_model_from_hf(
            None,
            TokenSource::None,
            &candle_core::DType::BF16,
            &device,
            false,
            DeviceMapSetting::dummy(),
            isq,
            None,
        )
        .map_err(|e| anyhow!("load target: {e}"))?;
    eprintln!(
        "target loaded in {:.2}s",
        t_target_load.elapsed().as_secs_f64()
    );

    // ── Draft: z-lab/Qwen3.5-27B-DFlash block-diffusion draft.
    eprintln!("loading DFlash block-diffusion draft from {:?}", args.draft_path);
    let t_draft = Instant::now();
    let draft_cfg_path = args.draft_path.join("config.json");
    let draft_cfg_text = std::fs::read_to_string(&draft_cfg_path)
        .with_context(|| format!("read draft config {:?}", draft_cfg_path))?;
    let draft_cfg: DFlashDraftConfig =
        serde_json::from_str(&draft_cfg_text).context("parse draft config")?;
    let draft_shards: Vec<PathBuf> = std::fs::read_dir(&args.draft_path)
        .with_context(|| format!("read draft dir {:?}", args.draft_path))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("safetensors"))
        .collect();
    if draft_shards.is_empty() {
        return Err(anyhow!("no safetensors under {:?}", args.draft_path));
    }
    let draft_vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&draft_shards, DType::BF16, &device)
            .context("mmap draft safetensors")?
    };
    let draft = Arc::new(
        DFlashDraftModel::load(draft_vb, draft_cfg.clone()).context("build DFlashDraftModel")?,
    );
    eprintln!(
        "draft loaded in {:.2}s",
        t_draft.elapsed().as_secs_f64()
    );

    // ── Stepper + ring (single sequence).
    let fused_dim = draft_cfg.fused_target_feature_dim();
    let ring = TargetFeatureRing::new(&device, DEFAULT_RING_CAP, fused_dim, DType::BF16)
        .context("alloc target feature ring")?;
    let stepper = DFlashChainStepper::new(ring, draft_cfg.clone());

    // ── Acquire target text-model reference.
    let pipeline_guard = pipeline.lock().await;
    let text_model = pipeline_guard
        .dflash_text_model()
        .ok_or_else(|| anyhow!("target is not a Qwen3.5 vision pipeline"))?;
    // Wipe any state the loader / warmup may have left in the
    // hybrid cache (GDN recurrences are non-invertible so a stale
    // prefix poisons every subsequent draft prediction), then seed
    // the recurrent-state slot index so the linear-attention layers
    // know which pool slot to read/write.
    text_model.dflash_reset_cache();
    text_model
        .dflash_set_state_indices(&[0])
        .context("dflash_set_state_indices")?;

    // ── Parse prompt.
    let prompt_ids: Vec<u32> = args
        .prompt_ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("parse --prompt-ids")?;
    if prompt_ids.is_empty() {
        return Err(anyhow!("--prompt-ids must be non-empty"));
    }
    let prompt_len = prompt_ids.len();
    let prompt_tensor = Tensor::from_vec(prompt_ids, (1, prompt_len), &device)?;

    let opts = StepperOpts {
        ctx_len: args.ctx_len,
        draft_top_k: args.draft_top_k,
    };
    let eos_ids: Vec<u32> = pipeline_guard.get_metadata().eos_tok.clone();
    let use_tree = std::env::var("DFLASH_USE_TREE_VERIFY").ok().as_deref()
        == Some("1")
        || std::env::var("DFLASH_USE_TREE_VERIFY").ok().as_deref() == Some("true");
    eprintln!(
        "prompt_len={} n_tokens={} ctx_len={} draft_top_k={} tree_verify={}",
        prompt_len, args.n_tokens, args.ctx_len, args.draft_top_k, use_tree
    );

    // ── Run.
    let t_gen = Instant::now();
    let outcome = run_greedy(
        text_model,
        draft.as_ref(),
        &stepper,
        &prompt_tensor,
        args.n_tokens,
        &eos_ids,
        &opts,
    )
    .context("run_greedy")?;
    let elapsed = t_gen.elapsed();

    let n_new = outcome.generated_tokens.len().saturating_sub(1);
    let al = if outcome.draft_steps > 0 {
        outcome.draft_accepted_total as f64 / outcome.draft_steps as f64
    } else {
        0.0
    };
    println!("\n=== RESULT ===");
    println!("mode:        {}", if use_tree { "DDTree tree verify" } else { "chain verify" });
    println!("total wall:  {:.2}s", elapsed.as_secs_f64());
    println!("new tokens:  {n_new}");
    println!(
        "decode tok/s (engine-reported): {:.1}",
        outcome.decode_tok_per_s
    );
    println!(
        "decode tok/s (wall):            {:.1}",
        n_new as f64 / elapsed.as_secs_f64()
    );
    println!("spec rounds: {}", outcome.draft_steps);
    println!(
        "acceptance length (AL): {:.2}  ({} accepted / {} rounds)",
        al, outcome.draft_accepted_total, outcome.draft_steps
    );
    println!("\ngenerated ids:");
    for (i, tid) in outcome.generated_tokens.iter().enumerate() {
        print!("{tid}");
        if i + 1 < outcome.generated_tokens.len() {
            print!(",");
        }
    }
    println!();

    Ok(())
}
