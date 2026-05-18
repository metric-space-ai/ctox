//! `qwen35-27b-q4km-dflash-bench` — drives the in-crate Rust port of
//! `lucebox/dflash/test/test_dflash.cpp` end-to-end.
//!
//! CLI mirrors the reference binary's positional arguments so bit-exact
//! comparison is trivial:
//!
//! ```text
//! qwen35-27b-q4km-dflash-bench <target.gguf> <draft.safetensors> \
//!                              <prompt_ids.bin> <n_gen> <out_ids.bin>
//! ```
//!
//! The prompt file is a flat `int32` little-endian token stream; the
//! output file is the same format, containing prompt + generated tokens
//! (matches the reference's `write_int32_file`). `cmp` between the
//! two confirms byte-for-byte parity.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Parser;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::driver::{run_dflash_gen_loop, GenConfig, RunStats};
use dflash::ffi as sys;
use dflash::graph::{create_target_cache, free_target_cache};
use dflash::loader::{
    free_draft_weights, free_target_weights, load_draft_safetensors, load_target_gguf,
};
use dflash::model::{DraftWeights, TargetCache, TargetWeights};

#[derive(Parser, Debug)]
#[command(
    name = "qwen35-27b-q4km-dflash-bench",
    about = "Rust port of the lucebox/dflash reference binary (target = Qwen3.5-27B Q4_K_M, draft = z-lab DFlash)."
)]
struct Args {
    /// Target GGUF path (Qwen3.5-27B Q4_K_M).
    target_gguf: PathBuf,
    /// Draft safetensors path (z-lab/Qwen3.5-27B-DFlash, bf16).
    draft_st: PathBuf,
    /// Prompt token IDs, int32 LE binary file.
    prompt_bin: PathBuf,
    /// Number of new tokens to generate.
    n_gen: i32,
    /// Output file — prompt + generated tokens, int32 LE.
    out_bin: PathBuf,
    /// Target cache `max_ctx` (default 4096; reference's
    /// `DFLASH27B_MAX_CTX_OVERRIDE` equivalent).
    #[arg(long, default_value_t = 4096)]
    max_ctx: i32,
    /// `max_verify_tokens` — pass 0 to use the default
    /// `DFLASH27B_DRAFT_BLOCK_SIZE = 16`.
    #[arg(long, default_value_t = 0)]
    max_verify_tokens: i32,
    /// CUDA device index.
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    /// `--fast-rollback`: skip snapshot + replay-forward pair, roll
    /// back SSM + conv via the per-step captured intermediates.
    /// Matches reference `--fast-rollback` exactly. ~20 % faster at
    /// the same output stream.
    #[arg(long)]
    fast_rollback: bool,
    /// `--ddtree`: tree-structured verify on top of fast-rollback.
    /// Matches reference `--ddtree` exactly. The reference's peak
    /// config on RTX 3090 (130 tok/s mean on HumanEval) uses
    /// `--ddtree --ddtree-budget=22`.
    #[arg(long)]
    ddtree: bool,
    /// DDTree budget (max non-root tree nodes). Default 64.
    #[arg(long, default_value_t = 64)]
    ddtree_budget: i32,
    /// DDTree softmax temperature for top-K extract. `< 1` sharpens
    /// the draft distribution.
    #[arg(long, default_value_t = 1.0)]
    ddtree_temp: f32,
    /// Disable the `chain_seed` defensive prefix in the DDTree
    /// builder (matches reference `--ddtree-no-chain-seed`).
    #[arg(long)]
    ddtree_no_chain_seed: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let prompt = read_int32_file(&args.prompt_bin)
        .with_context(|| format!("read prompt file {}", args.prompt_bin.display()))?;
    if prompt.is_empty() {
        return Err(anyhow!("empty prompt file"));
    }

    // CUDA backend.
    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!(
            "ggml_backend_cuda_init failed for device {}",
            args.cuda_device
        ));
    }

    // Load target.
    let mut w = TargetWeights::default();
    if !load_target_gguf(&args.target_gguf, backend, &mut w) {
        unsafe { sys::ggml_backend_free(backend) };
        return Err(anyhow!(
            "load_target_gguf failed: {}",
            dflash::last_error()
        ));
    }
    eprintln!("[target] {}", dflash::last_error());

    // Load draft.
    let mut dw = DraftWeights::default();
    if !load_draft_safetensors(&args.draft_st, backend, &mut dw) {
        free_target_weights(&mut w);
        unsafe { sys::ggml_backend_free(backend) };
        return Err(anyhow!(
            "load_draft_safetensors failed: {}",
            dflash::last_error()
        ));
    }
    eprintln!("[draft] loaded");

    // Build cache. For DDTree we need room for the flat tree
    // (1 + budget) nodes; for chain verify q_len=16 is enough.
    // Matches test_dflash.cpp:801-805.
    const Q_LEN: i32 = 16;
    let max_verify_tokens_eff = if args.max_verify_tokens > 0 {
        args.max_verify_tokens
    } else if args.ddtree {
        std::cmp::max(Q_LEN, args.ddtree_budget + 1)
    } else {
        Q_LEN
    };
    let mut cache = TargetCache::default();
    if !create_target_cache(&w, args.max_ctx, max_verify_tokens_eff, backend, &mut cache) {
        free_draft_weights(&mut dw);
        free_target_weights(&mut w);
        unsafe { sys::ggml_backend_free(backend) };
        return Err(anyhow!(
            "create_target_cache failed: {}",
            dflash::last_error()
        ));
    }

    // Run gen loop.
    let mut out_all: Vec<i32> = Vec::new();
    let cfg = GenConfig {
        fast_rollback: args.fast_rollback,
        ddtree: args.ddtree,
        ddtree_budget: args.ddtree_budget,
        ddtree_temp: args.ddtree_temp,
        ddtree_chain_seed: !args.ddtree_no_chain_seed,
    };
    let stats: RunStats = match run_dflash_gen_loop(
        &w,
        &dw,
        &mut cache,
        backend,
        &prompt,
        args.n_gen,
        &mut out_all,
        cfg,
    ) {
        Ok(s) => s,
        Err(e) => {
            free_target_cache(&mut cache);
            free_draft_weights(&mut dw);
            free_target_weights(&mut w);
            unsafe { sys::ggml_backend_free(backend) };
            return Err(anyhow!("run_dflash_gen_loop: {e}"));
        }
    };

    write_int32_file(&args.out_bin, &out_all)
        .with_context(|| format!("write out_bin {}", args.out_bin.display()))?;

    // Match the reference's final printout shape.
    let accept_total = (stats.n_draft_steps * 16) as f64; // q_len = 16
    let accept_pct = if stats.n_draft_steps > 0 {
        100.0 * (stats.n_accept_sum as f64) / accept_total
    } else {
        0.0
    };
    let avg_commit = if stats.n_draft_steps > 0 {
        (stats.n_generated as f64) / (stats.n_draft_steps as f64)
    } else {
        0.0
    };
    println!(
        "\n[dflash27b] generated {} tokens in {:.3} s (prefill {:.3} s)  →  {:.2} tok/s decode",
        stats.n_generated, stats.wall_s, stats.prefill_s, stats.decode_tok_s
    );
    println!(
        "[dflash27b] {} draft steps, accepted={}/{} ({:.1}% per step), avg commit/step={:.2}",
        stats.n_draft_steps,
        stats.n_accept_sum,
        stats.n_draft_steps * 16,
        accept_pct,
        avg_commit
    );
    let tail_start = out_all.len().saturating_sub(20);
    print!("[dflash27b] output tail: ");
    for t in &out_all[tail_start..] {
        print!("{t} ");
    }
    println!();

    free_target_cache(&mut cache);
    free_draft_weights(&mut dw);
    free_target_weights(&mut w);
    unsafe { sys::ggml_backend_free(backend) };

    Ok(())
}

// ─── I/O helpers (mirror test_dflash.cpp read/write_int32_file) ──

fn read_int32_file(p: &std::path::Path) -> Result<Vec<i32>> {
    let bytes = std::fs::read(p)?;
    if !bytes.len().is_multiple_of(4) {
        return Err(anyhow!("file {} is not a multiple of 4 bytes", p.display()));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let v = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        out.push(v);
    }
    Ok(out)
}

fn write_int32_file(p: &std::path::Path, ids: &[i32]) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(p)?;
    for v in ids {
        f.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}
