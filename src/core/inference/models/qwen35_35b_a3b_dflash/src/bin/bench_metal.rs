//! `qwen35-35b-a3b-dflash-bench-metal` — macOS + Apple-Silicon bench for
//! the Qwen3.5-35B-A3B DFlash port.
//!
//! CLI mirrors `bench_cuda.rs` so output comparison between Linux and
//! macOS runs on an equivalent prompt / n_gen is a plain `cmp(1)` on
//! the `i32`-LE binary output files.
//!
//! On non-macOS hosts this file compiles to a stub that exits 2 —
//! matches the `bench_cuda.rs` / non-Linux behaviour so both binaries
//! are cfg-gated symmetrically.

#![cfg_attr(not(target_os = "macos"), allow(unused))]

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!(
        "qwen35-35b-a3b-dflash-bench-metal: this binary is only available on \
         macOS + Apple Silicon builds."
    );
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use anyhow::{anyhow, Context, Result};
#[cfg(target_os = "macos")]
use clap::Parser;

#[cfg(target_os = "macos")]
use ctox_qwen35_35b_a3b_dflash as dflash;
#[cfg(target_os = "macos")]
use dflash::metal::driver::run_dflash_gen_loop;
#[cfg(target_os = "macos")]
use dflash::metal::ffi::global_device;
#[cfg(target_os = "macos")]
use dflash::metal::loader::{load_draft_safetensors, load_target_mlx4bit};
#[cfg(target_os = "macos")]
use dflash::metal::runtime::{DFlashRuntime, GenConfig, RunStats};

#[cfg(target_os = "macos")]
#[derive(Parser, Debug)]
#[command(
    name = "qwen35-35b-a3b-dflash-bench-metal",
    about = "Byte-for-byte parity bench of the Metal port against the bstnxbt/dflash-mlx Python reference. \
             Output file is plain i32 little-endian tokens (prompt + generated), identical format \
             to the CUDA bench so `cmp(1)` across runs is trivial."
)]
struct Args {
    /// Target model directory (mlx-community/Qwen3.5-35B-A3B-4bit export).
    /// Must contain `config.json` + `model-*.safetensors` shards.
    target_dir: PathBuf,
    /// Draft safetensors path (z-lab/Qwen3.5-35B-A3B-DFlash, bf16).
    draft_st: PathBuf,
    /// Prompt token IDs, int32 LE binary file (same format as CUDA bench).
    prompt_bin: PathBuf,
    /// Number of new tokens to generate.
    n_gen: i32,
    /// Output file — prompt + generated tokens, int32 LE.
    out_bin: PathBuf,

    /// Draft block size (default 16).
    #[arg(long, default_value_t = 16)]
    block_size: i32,
    /// Draft sink KV cache size.
    #[arg(long, default_value_t = 64)]
    sink_size: i32,
    /// Draft window KV cache size.
    #[arg(long, default_value_t = 1024)]
    window_size: i32,
    /// `dflash-max-ctx` threshold — stock MLX fallback above this.
    #[arg(long, default_value_t = 8192)]
    dflash_max_ctx: i32,
    /// Enable the verify-specialized qmm kernel (`DFLASH_VERIFY_QMM=1`).
    /// Defaults off to match the Python reference's default.
    #[arg(long)]
    verify_qmm: bool,
    /// Profile timing.
    #[arg(long)]
    profile: bool,
    /// Disable the default Metal compute-pipeline warmup before timed runs.
    #[arg(long)]
    no_pipeline_warmup: bool,
    /// Run target-only AR decode without loading the BF16 DFlash draft.
    /// This is a bring-up mode for the Metal target path, not a DFlash
    /// speculative-decoding benchmark.
    #[arg(long)]
    target_only: bool,
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let prompt = read_int32_file(&args.prompt_bin)
        .with_context(|| format!("read prompt file {}", args.prompt_bin.display()))?;
    if prompt.is_empty() {
        return Err(anyhow!("empty prompt file"));
    }

    let dev = global_device().ok_or_else(|| anyhow!("failed to acquire default Metal device"))?;

    // Load target (MLX 4-bit safetensors directory).
    let target = load_target_mlx4bit(dev, &args.target_dir)
        .with_context(|| format!("load_target_mlx4bit({})", args.target_dir.display()))?;

    if args.verify_qmm {
        std::env::set_var("DFLASH_VERIFY_QMM", "1");
    }

    let cfg = GenConfig {
        block_size: args.block_size,
        sink_size: args.sink_size,
        window_size: args.window_size,
        dflash_max_ctx: args.dflash_max_ctx,
        verify_qmm_opt_in: args.verify_qmm,
        profile: args.profile,
        pipeline_warmup: !args.no_pipeline_warmup,
    };

    let mut out: Vec<i32> = Vec::new();
    let stats: RunStats = if args.target_only {
        let draft = placeholder_draft(dev)?;
        let mut rt = DFlashRuntime::new(dev, target, draft, cfg)
            .with_context(|| "DFlashRuntime::new target-only failed")?;
        rt.generate(dev, &prompt, args.n_gen, &mut out)
            .with_context(|| "target-only generate failed")?
    } else {
        let draft = load_draft_safetensors(dev, &args.draft_st)
            .with_context(|| format!("load_draft_safetensors({})", args.draft_st.display()))?;
        run_dflash_gen_loop(dev, target, draft, &prompt, args.n_gen, &mut out, cfg)
            .with_context(|| "run_dflash_gen_loop failed")?
    };

    write_int32_file(&args.out_bin, &out)
        .with_context(|| format!("write out_bin {}", args.out_bin.display()))?;

    let accept_total = stats.n_draft_tokens_attempted as f64;
    let accept_pct = if stats.n_draft_tokens_attempted > 0 {
        100.0 * (stats.n_accept_sum as f64) / accept_total
    } else {
        0.0
    };
    let avg_commit = if stats.n_draft_steps > 0 {
        (stats.n_generated as f64) / (stats.n_draft_steps as f64)
    } else {
        0.0
    };
    let prefill_tok_s = (prompt.len() as f64) / stats.prefill_s.max(1e-9);
    println!(
        "\n[dflash35b-a3b-metal] prompt {} tokens, generated {} tokens in {:.3} s \
         (prefill {:.3} s, {:.2} tok/s) -> {:.2} tok/s decode",
        prompt.len(),
        stats.n_generated,
        stats.wall_s,
        stats.prefill_s,
        prefill_tok_s,
        stats.decode_tok_s
    );
    println!(
        "[dflash35b-a3b-metal] {} draft steps, accepted={}/{} ({:.1}% per step), avg commit/step={:.2}",
        stats.n_draft_steps,
        stats.n_accept_sum,
        stats.n_draft_tokens_attempted,
        accept_pct,
        avg_commit,
    );
    if stats.pipeline_warmup_n > 0 {
        println!(
            "[dflash35b-a3b-metal] pipeline warmup {:.3} s for {} cached/compiled pipelines \
             (excluded from wall time)",
            stats.pipeline_warmup_s, stats.pipeline_warmup_n
        );
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn placeholder_draft(
    dev: &dflash::metal::ffi::Device,
) -> Result<dflash::metal::model::DraftWeights> {
    use dflash::metal::model::DraftWeights;
    use dflash::metal::qwen::{Bf16Linear, RmsNorm};

    let lin = || -> Result<Bf16Linear> {
        Ok(Bf16Linear {
            weight: dev
                .new_buffer(std::mem::size_of::<u16>())
                .ok_or_else(|| anyhow!("placeholder draft bf16 linear alloc failed"))?,
            bias: None,
            in_features: 1,
            out_features: 1,
        })
    };
    let norm = || -> Result<RmsNorm> {
        Ok(RmsNorm {
            weight: dev
                .new_buffer(std::mem::size_of::<u16>())
                .ok_or_else(|| anyhow!("placeholder draft bf16 norm alloc failed"))?,
            d: 1,
            eps: 1e-6,
            weight_bias: 0.0,
        })
    };

    Ok(DraftWeights {
        fc: lin()?,
        hidden_norm: norm()?,
        layers: Vec::new(),
        out_norm: norm()?,
    })
}

#[cfg(target_os = "macos")]
fn read_int32_file(p: &std::path::Path) -> Result<Vec<i32>> {
    let bytes = std::fs::read(p)?;
    if !bytes.len().is_multiple_of(4) {
        return Err(anyhow!("file {} is not a multiple of 4 bytes", p.display()));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
fn write_int32_file(p: &std::path::Path, ids: &[i32]) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(p)?;
    for v in ids {
        f.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}
