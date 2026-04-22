//! Minimal tok/s benchmark for the bare-metal speculative decoder.
//!
//! Loads the 27B target + DFlash draft, runs
//! `SpeculativeDecoder::generate` on a fixed HumanEval-shaped prompt,
//! reports wall time + per-step commit/propose stats so we can diff
//! against the FFI reference's ~100 tok/s.
//!
//! This bench is the counterpart to `qwen35-parity-bench`'s "ours"
//! sweep but drives the chain-verify loop instead of single-token
//! decode. Prompt pattern matches the parity bench so numbers are
//! directly comparable.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use engine_runtime::GenerativeModel;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use ctox_cuda_primitives::device::DeviceContext;
use ctox_qwen35_27b::draft::{DraftConfig, DraftModel};
use ctox_qwen35_27b::gguf_loader::{parse_qwen35_metadata, LoaderConfig};
use ctox_qwen35_27b::spec_decode::SpeculativeDecoder;
use ctox_qwen35_27b::{Qwen35Config, Qwen35Target, Qwen35Tokenizer};

#[derive(Parser, Debug)]
#[command(
    name = "qwen35-spec-bench",
    about = "Bare-metal speculative decoder throughput bench."
)]
struct Args {
    #[arg(long, default_value = "/home/metricspace/dflash-ref/dflash/models/Qwen3.5-27B-Q4_K_M.gguf")]
    target_gguf: PathBuf,
    #[arg(long, default_value = "/home/metricspace/dflash-ref/dflash/models/draft/model.safetensors")]
    draft_st: PathBuf,
    /// Path to a compatible `tokenizer.json`. The spec-decoder's
    /// `generate()` does not invoke encode/decode — we pass raw token
    /// ids — but the constructor requires an `Arc<Qwen35Tokenizer>`
    /// so we plumb any valid file through.
    #[arg(long, default_value = "/home/metricspace/myPokeDex/python-dev/out/qwen-web-inference/local-extension/Qwen3.5-0.8B-ONNX/tokenizer.json")]
    tokenizer_json: PathBuf,
    #[arg(long, default_value_t = 1024)]
    prompt_len: usize,
    #[arg(long, default_value_t = 128)]
    n_new: usize,
    #[arg(long, default_value_t = 2)]
    warmup: usize,
    #[arg(long, default_value_t = 3)]
    repeats: usize,
    #[arg(long, default_value_t = 0)]
    cuda_device: u32,
    /// max_ctx for the target's KV + feature buffer. Must be >= prompt_len + n_new + slack.
    #[arg(long, default_value_t = 4096)]
    max_ctx: usize,
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let args = Args::parse();

    let device = Arc::new(
        DeviceContext::new(args.cuda_device as usize)
            .with_context(|| format!("DeviceContext::new({})", args.cuda_device))?,
    );

    eprintln!("[spec] parse metadata...");
    let meta = parse_qwen35_metadata(&args.target_gguf)
        .with_context(|| "parse_qwen35_metadata")?;
    let config = Qwen35Config::from_metadata(&meta, 128);

    eprintln!("[spec] load target 27B (keep_packed=true)...");
    let t0 = Instant::now();
    let target = Qwen35Target::load_from_gguf_with_config(
        device.clone(),
        config.clone(),
        &args.target_gguf,
        LoaderConfig { keep_packed: true },
    )
    .with_context(|| "load target")?;
    eprintln!(
        "[spec] target loaded in {:.2}s (vocab={} n_fa={} n_gdn={})",
        t0.elapsed().as_secs_f64(),
        target.vocab_size,
        target.n_full_attn,
        target.n_gdn,
    );

    eprintln!("[spec] load draft safetensors...");
    let t0 = Instant::now();
    let draft =
        DraftModel::load_from_safetensors(device.clone(), &args.draft_st, DraftConfig::qwen35_27b())
            .with_context(|| "load draft")?;
    eprintln!(
        "[spec] draft loaded in {:.2}s ({} layers, hidden={}, block_size={})",
        t0.elapsed().as_secs_f64(),
        draft.config().n_layers,
        draft.config().hidden,
        draft.config().block_size,
    );

    let tokenizer = Arc::new(
        Qwen35Tokenizer::from_file(&args.tokenizer_json)
            .with_context(|| format!("load tokenizer {:?}", args.tokenizer_json))?,
    );

    eprintln!("[spec] build SpeculativeDecoder (max_ctx={})...", args.max_ctx);
    let mut spec = SpeculativeDecoder::new(target, draft, tokenizer.clone(), args.max_ctx)
        .with_context(|| "SpeculativeDecoder::new")?;

    // HumanEval pattern repeated to prompt_len — identical to the
    // parity bench so decode tok/s is directly comparable.
    let base: [i32; 9] = [7734, 264, 6185, 36974, 883, 13094, 6326, 61369, 25];
    let prompt: Vec<i32> = (0..args.prompt_len).map(|i| base[i % 9]).collect();

    // Warmup (short generate — the first call loads PTX modules, grows
    // KV pool, JITs mmvq kernels).
    for i in 0..args.warmup {
        eprintln!("[spec] warmup {}/{} (prompt_len={}, n_new=32)...", i + 1, args.warmup, args.prompt_len);
        let t0 = Instant::now();
        let (_toks, stats) = spec
            .generate(&prompt[..args.prompt_len.min(128)], 32)
            .with_context(|| "warmup generate")?;
        eprintln!(
            "  warmup {}: {:.2}s decode={:.2} tok/s n_accepted={} n_proposed={} steps={}",
            i + 1,
            t0.elapsed().as_secs_f64(),
            stats.decode_tok_s,
            stats.n_accepted,
            stats.n_proposed,
            stats.n_draft_steps,
        );
    }

    eprintln!(
        "[spec] measured sweep: prompt_len={} n_new={} repeats={}",
        args.prompt_len, args.n_new, args.repeats
    );
    let mut samples: Vec<(f64, f64, usize, usize, usize)> = Vec::new();
    for r in 0..args.repeats {
        let t0 = Instant::now();
        let (tokens, stats) = spec
            .generate(&prompt, args.n_new)
            .with_context(|| "measured generate")?;
        let wall_s = t0.elapsed().as_secs_f64();
        let gen_toks = tokens.len().saturating_sub(prompt.len());
        // DIAGNOSTIC: dump the first 32 generated token IDs — used to
        // cross-diff against the reference's output.bin. If these are
        // not the HumanEval pattern continuation, target.forward has
        // a correctness bug regardless of speculative decoding.
        let gen_start = prompt.len();
        let gen_end = (gen_start + 32).min(tokens.len());
        eprintln!("  DIAG generated: {:?}", &tokens[gen_start..gen_end]);
        eprintln!(
            "  rep {}/{}: wall={:.3}s gen={} decode={:.2} tok/s steps={} accepted={}/{} ({:.1}%)",
            r + 1,
            args.repeats,
            wall_s,
            gen_toks,
            stats.decode_tok_s,
            stats.n_draft_steps,
            stats.n_accepted,
            stats.n_proposed,
            if stats.n_proposed > 0 {
                stats.n_accepted as f64 / stats.n_proposed as f64 * 100.0
            } else {
                0.0
            },
        );
        samples.push((
            wall_s,
            stats.decode_tok_s,
            stats.n_accepted,
            stats.n_proposed,
            stats.n_draft_steps,
        ));
    }

    // Median decode tok/s.
    let mut tps: Vec<f64> = samples.iter().map(|s| s.1).collect();
    tps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = tps[tps.len() / 2];
    eprintln!(
        "\n=== SPEC DECODE BENCH: prompt_len={} n_new={} (median of {}) ===",
        args.prompt_len, args.n_new, args.repeats
    );
    eprintln!("  median decode tok/s: {:.2}", median);
    if let Some((_, _, a, p, s)) = samples.last() {
        eprintln!(
            "  last-rep acceptance: {}/{} ({:.1}% per-step commit/step={:.2})",
            a,
            p,
            if *p > 0 { *a as f64 / *p as f64 * 100.0 } else { 0.0 },
            if *s > 0 { *a as f64 / *s as f64 } else { 0.0 },
        );
    }

    Ok(())
}
