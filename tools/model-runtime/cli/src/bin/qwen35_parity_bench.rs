Warning: Permanently added 'gpu1-a6000' (ED25519) to the list of known hosts.
//! Qwen3.5-27B **parity bench** — side-by-side FFI reference vs our
//! bare-metal stack across a prompt-length sweep.
//!
//! # What this is
//!
//! Honest, single-source-of-truth comparison harness for the
//! "reference library (`libdflash_run_lib.so` via `ctox-dflash-ffi`)
//! vs native `ctox-qwen35-27b` forward" delta. Produces the numbers
//! we need BEFORE starting any performance work on the bare-metal
//! stack. No kernel fixes, no silent skips. Whatever the harness
//! reports IS what we work from.
//!
//! # Methodology (non-negotiable)
//!
//! * **Same prompt**: the 9-token HumanEval pattern
//!   `[7734, 264, 6185, 36974, 883, 13094, 6326, 61369, 25]` repeated
//!   to fill `prompt_len`. Both stacks consume the identical byte
//!   sequence.
//! * **Same n_new**: 128 new tokens per run (matches the reference
//!   library's internal bench). Chain mode (no DDTree) so the two
//!   stacks are doing the same algorithmic work per step.
//! * **Warmup**: 3 forward passes at 128 tokens before the measured
//!   block (per stack).
//! * **Repeats**: 5 iterations per (stack, prompt_len); median
//!   reported.
//! * **Sync discipline**: `cudaDeviceSynchronize` before `t0` and
//!   before `t1` on every iteration of our stack. The FFI path runs
//!   synchronously inside the library (the library's own bench prints
//!   the post-sync wall time); we honor that.
//! * **Back-to-back on one process/GPU**: both stacks share the A6000
//!   device without a tear-down between them so thermal/kernel-cache
//!   noise cancels.
//! * **Model load cost**: reported separately, NOT folded into
//!   per-iter numbers.
//!
//! # Failure policy
//!
//! We expect the bare-metal side to fail at some prompt lengths right
//! now (OOM on `gdn_inter` for large prefills, possible NaN/Inf in
//! logits, possible shape mismatches). We **catch** those failures
//! and record them as a `FAIL: …` row — the sweep continues with the
//! next `prompt_len`. We do NOT swallow them, we do NOT "fix" them
//! here. The point of the harness is to surface them.
//!
//! # Output
//!
//! A single table at the end:
//!
//! ```text
//!   prompt_len |  ffi_prefill_ms  ffi_decode_tok/s | ours_prefill_ms  ours_decode_tok/s | prefill_ratio  decode_ratio | status
//!         1024 |      …               …            |      …               …             |      …              …      | ok
//!         4096 |      …               …            |      …               …             |      …              …      | ok
//!        16384 |      …               …            | FAIL: OOM gdn_inter_alloc …                                     | fail
//!   …
//! ```
//!
//! where `*_ratio = ours / ffi` (>1 means we're slower — smaller is
//! better). `FAIL` lines are printed literally.
//!
//! # Running on the A6000 host
//!
//! ```text
//! export PATH=$HOME/.cargo/bin:$PATH
//! cd /home/metricspace/ctox_fresh && \
//!   git fetch origin main && git reset --hard origin/main && \
//!   cd tools/model-runtime && \
//!   cargo build --release --bin qwen35-parity-bench --features cuda && \
//!   LD_LIBRARY_PATH=/home/metricspace/dflash-ref/dflash/build/deps/llama.cpp/ggml/src:\
//!     /home/metricspace/dflash-ref/dflash/build/deps/llama.cpp/ggml/src/ggml-cuda \
//!     ./target/release/qwen35-parity-bench
//! ```

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ctox_dflash_ffi::{DflashOpts, DflashRuntime};

#[cfg(feature = "cuda")]
use ctox_cuda_primitives::device::DeviceContext;
#[cfg(feature = "cuda")]
use ctox_cuda_primitives::kv_cache::KvCache;
#[cfg(feature = "cuda")]
use ctox_cuda_primitives::tensor::CudaTensor;
#[cfg(feature = "cuda")]
use ctox_qwen35_27b::{
    gguf_loader::{parse_qwen35_metadata, LoaderConfig},
    target::Qwen35Target,
    Qwen35Config,
};
#[cfg(feature = "cuda")]
use half::f16;

/// HumanEval 9-token base pattern — same sequence the reference bench
/// and our microbench use.
const BASE_PROMPT_IDS: &[i32] = &[7734, 264, 6185, 36974, 883, 13094, 6326, 61369, 25];

#[derive(Parser, Debug)]
#[command(
    name = "qwen35-parity-bench",
    about = "Side-by-side FFI vs bare-metal Qwen3.5-27B sweep."
)]
struct Args {
    /// libdflash_run_lib.so (built from the hard-forked reference).
    #[arg(
        long,
        default_value = "/home/metricspace/dflash-ref/dflash/build/libdflash_run_lib.so"
    )]
    lib: PathBuf,

    /// Q4_K_M GGUF consumed by both stacks.
    #[arg(
        long,
        default_value = "/home/metricspace/dflash-ref/dflash/models/Qwen3.5-27B-Q4_K_M.gguf"
    )]
    target_gguf: PathBuf,

    /// Draft safetensors (only the FFI side consumes it; our bare-metal
    /// stack does not yet have spec decoding so this is unused on our
    /// side).
    #[arg(
        long,
        default_value = "/home/metricspace/dflash-ref/dflash/models/draft/model.safetensors"
    )]
    draft_st: PathBuf,

    /// Sweep list. Default matches the task brief:
    /// 1024, 4096, 16384, 32768, 65536, 131072.
    #[arg(
        long,
        default_value = "1024,4096,16384,32768,65536,131072"
    )]
    prompt_lens: String,

    /// Tokens to generate per measured iteration. Matches the
    /// reference's internal bench (n_new=128).
    #[arg(long, default_value_t = 128)]
    n_new: usize,

    /// Warmup iterations per stack (each at 128 prompt tokens /
    /// `n_new` generated).
    #[arg(long, default_value_t = 3)]
    warmup: usize,

    /// Measured iterations per (stack, prompt_len) — median of these.
    #[arg(long, default_value_t = 5)]
    repeats: usize,

    /// CUDA device ordinal (both stacks use the same device).
    #[arg(long, default_value_t = 0)]
    cuda_device: u32,

    /// Skip the bare-metal side entirely (useful for debugging the
    /// FFI run alone).
    #[arg(long)]
    ffi_only: bool,

    /// Skip the FFI side entirely (useful for debugging our stack
    /// alone). Mostly for local dev; production runs expect both.
    #[arg(long)]
    ours_only: bool,
}

/// Repeat BASE_PROMPT_IDS to fill `len`.
fn synth_prompt(len: usize) -> Vec<i32> {
    (0..len)
        .map(|i| BASE_PROMPT_IDS[i % BASE_PROMPT_IDS.len()])
        .collect()
}

/// Parse `--prompt-lens=1024,4096,...`.
fn parse_prompt_lens(s: &str) -> Result<Vec<usize>> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<usize>()
                .with_context(|| format!("bad prompt_lens entry {:?}", p))
        })
        .collect()
}

/// Median of a list of f64 (must be non-empty). Mutates for sort.
fn median(xs: &mut [f64]) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs[xs.len() / 2]
}

/// One measured result for (stack, prompt_len). `prefill_ms` is the
/// wall time to eat `prompt_len` tokens (excludes decode). `decode_tok_s`
/// is the pure decode rate over `n_new` generated tokens, excluding
/// prefill.
#[derive(Debug, Clone)]
struct StackRun {
    prefill_ms: Option<f64>,
    decode_tok_s: Option<f64>,
    status: Status,
}

#[derive(Debug, Clone)]
enum Status {
    Ok,
    /// Stack was never attempted (disabled via CLI flag).
    Skipped,
    /// Stack ran but hit an error at this prompt_len.
    Fail(String),
}

impl StackRun {
    fn ok(prefill_ms: f64, decode_tok_s: f64) -> Self {
        Self {
            prefill_ms: Some(prefill_ms),
            decode_tok_s: Some(decode_tok_s),
            status: Status::Ok,
        }
    }
    fn skipped() -> Self {
        Self {
            prefill_ms: None,
            decode_tok_s: None,
            status: Status::Skipped,
        }
    }
    fn fail(msg: impl Into<String>) -> Self {
        Self {
            prefill_ms: None,
            decode_tok_s: None,
            status: Status::Fail(msg.into()),
        }
    }
}

/// Per-prompt-length comparison row.
#[derive(Debug, Clone)]
struct SweepRow {
    prompt_len: usize,
    ffi: StackRun,
    ours: StackRun,
}

// ─────────────────────────────────────────────────────────────────────
// FFI side
// ─────────────────────────────────────────────────────────────────────

/// Run `repeats` measured generate calls on the persistent FFI runtime
/// and return median (prefill_ms, decode_tok_s) for this prompt_len.
///
/// Prefill time is derived from the library's reported wall+decode:
///   `prefill_s = wall_s - n_generated / decode_tok_s`
/// which matches the `[dflash] generated X tokens in Y s → Z tok/s`
/// stderr accounting (decode_tok_s is measured on the generated tail
/// only; wall is the whole call).
fn run_ffi_at(
    rt: &mut DflashRuntime,
    prompt: &[i32],
    n_new: usize,
    repeats: usize,
) -> Result<(f64, f64)> {
    let mut prefill_ms_samples: Vec<f64> = Vec::with_capacity(repeats);
    let mut decode_tps_samples: Vec<f64> = Vec::with_capacity(repeats);
    for i in 0..repeats {
        let (_out, stats) = rt
            .generate(prompt, n_new)
            .with_context(|| format!("ffi generate rep={} prompt_len={}", i, prompt.len()))?;
        // Defensive: library must report both.
        if stats.decode_tok_s <= 0.0 {
            return Err(anyhow!(
                "ffi returned decode_tok_s={} (<=0) at prompt_len={}",
                stats.decode_tok_s,
                prompt.len()
            ));
        }
        if stats.n_generated <= 0 {
            return Err(anyhow!(
                "ffi returned n_generated={} (<=0) at prompt_len={}",
                stats.n_generated,
                prompt.len()
            ));
        }
        let decode_s = stats.n_generated as f64 / stats.decode_tok_s;
        let prefill_s = (stats.wall_s - decode_s).max(0.0);
        prefill_ms_samples.push(prefill_s * 1000.0);
        decode_tps_samples.push(stats.decode_tok_s);
        eprintln!(
            "  [ffi rep {}/{}] prompt_len={} wall={:.3}s decode={:.2} tok/s prefill={:.1} ms",
            i + 1,
            repeats,
            prompt.len(),
            stats.wall_s,
            stats.decode_tok_s,
            prefill_s * 1000.0
        );
    }
    Ok((
        median(&mut prefill_ms_samples),
        median(&mut decode_tps_samples),
    ))
}

/// Drive the FFI side across the whole sweep. Runtime is allocated
/// once, generate called N times. Per-length failures are caught and
/// recorded.
fn ffi_sweep(
    args: &Args,
    prompt_lens: &[usize],
) -> Result<(Vec<StackRun>, f64)> {
    // Max ctx must fit the largest prompt + n_new + a bit of headroom.
    // Chain-mode FFI (ddtree=false) doesn't need the DDTree budget.
    let max_prompt = *prompt_lens.iter().max().unwrap();
    let max_ctx = (max_prompt + args.n_new + 256) as u32;
    let opts = DflashOpts {
        target_gguf: args.target_gguf.clone(),
        draft_safetensors: args.draft_st.clone(),
        max_ctx,
        ddtree_mode: false,
        ddtree_budget: 22,
        ddtree_temp: 1.0,
        ddtree_chain_seed: true,
        fast_rollback: false,
        seq_verify: false,
        cuda_device: args.cuda_device,
        tbq_kv: false,
    };
    eprintln!(
        "[ffi] loading runtime: lib={} target={} max_ctx={}",
        args.lib.display(),
        args.target_gguf.display(),
        max_ctx
    );
    let t_load = Instant::now();
    let mut rt = DflashRuntime::new(&args.lib, &opts)
        .with_context(|| format!("init dflash runtime via {}", args.lib.display()))?;
    let load_s = t_load.elapsed().as_secs_f64();
    eprintln!("[ffi] runtime loaded in {:.2}s", load_s);

    // Warmup at a 128-token prompt.
    eprintln!("[ffi] warmup: {}× 128-tok generate", args.warmup);
    let warm = synth_prompt(128);
    for i in 0..args.warmup {
        match rt.generate(&warm, args.n_new) {
            Ok((_o, s)) => eprintln!(
                "  [ffi warmup {}/{}] decode={:.2} tok/s wall={:.2}s",
                i + 1,
                args.warmup,
                s.decode_tok_s,
                s.wall_s
            ),
            Err(e) => return Err(anyhow!("ffi warmup failed: {:?}", e)),
        }
    }

    let mut rows = Vec::with_capacity(prompt_lens.len());
    for &plen in prompt_lens {
        eprintln!(
            "\n[ffi] === sweep prompt_len={} ({}× measured) ===",
            plen, args.repeats
        );
        let prompt = synth_prompt(plen);
        let r = match run_ffi_at(&mut rt, &prompt, args.n_new, args.repeats) {
            Ok((p_ms, d_tps)) => StackRun::ok(p_ms, d_tps),
            Err(e) => {
                eprintln!("[ffi] FAIL at prompt_len={}: {}", plen, e);
                StackRun::fail(format!("{}", e))
            }
        };
        rows.push(r);
    }
    Ok((rows, load_s))
}

// ─────────────────────────────────────────────────────────────────────
// Our stack side
// ─────────────────────────────────────────────────────────────────────

#[cfg(feature = "cuda")]
struct OurState {
    device: Arc<DeviceContext>,
    config: Qwen35Config,
    target: Qwen35Target,
    /// Reused across prompt lengths — we reset + resize between runs.
    max_ctx: usize,
}

/// Build a fresh KV cache + GDN state vectors sized for a single call
/// of `n_call_tokens` (= prompt_len OR decode chunk size). The
/// gdn_inter buffer must accommodate the LARGEST single forward this
/// state will see; see the comment in `target.rs` bench.
#[cfg(feature = "cuda")]
fn setup_state(
    state: &OurState,
    n_inter: usize,
) -> Result<(
    KvCache,
    Vec<CudaTensor<f32>>,
    Vec<CudaTensor<f16>>,
    Vec<CudaTensor<f32>>,
)> {
    let kv = KvCache::new(
        state.device.clone(),
        state.target.n_full_attn,
        state.max_ctx,
        state.config.n_kv_heads,
        state.config.head_dim,
    )
    .map_err(|e| anyhow!("alloc kv cache: {:?}", e))?;
    let s_v = state.config.gdn_ssm_dim;
    let h = state.config.gdn_num_v_heads;
    let qkv_proj_dim = state.config.gdn_qkv_proj_dim();
    // ssm_conv1d kernel=4 → rolling state of K-1=3 rows per GDN layer.
    let conv_state_rows = 3usize;
    let mut gdn_states: Vec<CudaTensor<f32>> = Vec::with_capacity(state.target.n_gdn);
    let mut gdn_inter: Vec<CudaTensor<f16>> = Vec::with_capacity(state.target.n_gdn);
    let mut gdn_conv_states: Vec<CudaTensor<f32>> = Vec::with_capacity(state.target.n_gdn);
    for _ in 0..state.target.n_gdn {
        gdn_states
            .push(CudaTensor::<f32>::zeros(state.device.clone(), vec![s_v, s_v, h, 1]).map_err(
                |e| anyhow!("alloc gdn state: {:?}", e),
            )?);
        gdn_inter.push(
            CudaTensor::<f16>::zeros(state.device.clone(), vec![s_v, s_v, h, n_inter]).map_err(
                |e| anyhow!("alloc gdn inter n_inter={}: {:?}", n_inter, e),
            )?,
        );
        gdn_conv_states.push(
            CudaTensor::<f32>::zeros(
                state.device.clone(),
                vec![conv_state_rows, qkv_proj_dim],
            )
            .map_err(|e| anyhow!("alloc gdn conv state: {:?}", e))?,
        );
    }
    Ok((kv, gdn_states, gdn_inter, gdn_conv_states))
}

/// Build token + MRoPE positions tensors for `n_tokens` starting at
/// absolute position `start`. Matches `target.rs::qwen35_prefill_decode_bench`.
#[cfg(feature = "cuda")]
fn build_input(
    device: &Arc<DeviceContext>,
    tokens_slice: &[i32],
    start: usize,
    n_tokens: usize,
) -> Result<(CudaTensor<i32>, CudaTensor<i32>)> {
    let tk = CudaTensor::<i32>::from_host(
        device.clone(),
        vec![n_tokens],
        &tokens_slice[..n_tokens],
    )
    .map_err(|e| anyhow!("upload tokens: {:?}", e))?;
    let mut pos = vec![0i32; 4 * n_tokens];
    for i in 0..n_tokens {
        let p = (start + i) as i32;
        pos[i] = p;
        pos[n_tokens + i] = p;
        pos[2 * n_tokens + i] = p;
        pos[3 * n_tokens + i] = 0;
    }
    let positions = CudaTensor::<i32>::from_host(device.clone(), vec![4, n_tokens], &pos)
        .map_err(|e| anyhow!("upload positions: {:?}", e))?;
    Ok((tk, positions))
}

/// Run one measured iteration of our stack at this prompt_len:
///   1. allocate fresh state (gdn_inter sized for max(prompt_len, n_new))
///   2. forward the prompt (prefill), time it
///   3. decode `n_new` tokens one at a time, time all of them together
/// Returns `(prefill_ms, decode_tok_s)`.
///
/// One-token decode currently exercises the small-M matmul path in
/// our kernel dispatch; if that path is missing or bugged, this
/// returns an error (not a panic — we propagate cleanly).
#[cfg(feature = "cuda")]
fn run_ours_once(
    state: &OurState,
    prompt: &[i32],
    n_new: usize,
) -> Result<(f64, f64)> {
    let prompt_len = prompt.len();
    // Decode-phase gdn_inter: the decode loop below issues 1-token
    // forwards, so n_inter for that path is just max(n_new, 1).
    // Prefill runs through `target.prefill()` which allocates its own
    // per-chunk gdn_inter sized for the ubatch (16 or 192), so we do
    // NOT need to oversize `gi` for prompt_len here. This is what
    // unblocks long-context prompts that previously OOMed at
    // `[128, 128, 48, prompt_len]`.
    let n_inter_decode = n_new.max(1);
    let (mut kv, mut gs, mut gi, mut gc) = setup_state(state, n_inter_decode)?;

    // ── Prefill ── chunked via target.prefill (ubatch=16/192 matching
    // dflash-ref's run_dflash_gen_loop).
    let prompt_tk = CudaTensor::<i32>::from_host(
        state.device.clone(),
        vec![prompt_len],
        prompt,
    )
    .map_err(|e| anyhow!("upload prompt tokens: {:?}", e))?;
    state
        .device
        .synchronize()
        .map_err(|e| anyhow!("pre-prefill sync: {:?}", e))?;
    let t0 = Instant::now();
    let logits = state
        .target
        .prefill(&prompt_tk, &mut kv, &mut gs, &mut gc)
        .map_err(|e| anyhow!("prefill: {:?}", e))?;
    state
        .device
        .synchronize()
        .map_err(|e| anyhow!("post-prefill sync: {:?}", e))?;
    let prefill_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // Logits sanity — prefill returns [1, vocab].
    {
        let shape = logits.shape().to_vec();
        if shape.len() != 2 || shape[0] != 1 {
            return Err(anyhow!(
                "prefill logits shape = {:?} expected [1, vocab]",
                shape
            ));
        }
    }

    // ── Decode: n_new 1-token forwards.
    //
    // Greedy: always feed the same base token — we don't want to pull
    // the argmax off the GPU each step, because the decode latency
    // measurement has to include only kernel launches, not host↔device
    // argmax round-trips. The reference measures the same way (its
    // n_new counter is independent of the chosen token IDs).
    let decode_token = BASE_PROMPT_IDS[0];
    let chunk_tokens = vec![decode_token; 1];

    // gdn_inter was sized for prompt_len; 1-token decode fits trivially.
    state
        .device
        .synchronize()
        .map_err(|e| anyhow!("pre-decode sync: {:?}", e))?;
    let t0 = Instant::now();
    let mut past = prompt_len;
    for _ in 0..n_new {
        let (tk, pos) = build_input(&state.device, &chunk_tokens, past, 1)?;
        let _logits = state
            .target
            .forward(&tk, &pos, &mut kv, &mut gs, &mut gi, &mut gc)
            .map_err(|e| anyhow!("decode forward past={}: {:?}", past, e))?;
        past += 1;
    }
    state
        .device
        .synchronize()
        .map_err(|e| anyhow!("post-decode sync: {:?}", e))?;
    let decode_s = t0.elapsed().as_secs_f64();
    let decode_tok_s = n_new as f64 / decode_s;
    Ok((prefill_ms, decode_tok_s))
}

/// Run `repeats` measured iterations of our stack at `prompt_len`,
/// return medians. Each iteration wraps the forward in `catch_unwind`
/// so a panic in our kernel code turns into a `FAIL: …` row instead
/// of aborting the whole sweep.
#[cfg(feature = "cuda")]
fn run_ours_at(
    state: &OurState,
    prompt: &[i32],
    n_new: usize,
    repeats: usize,
) -> std::result::Result<(f64, f64), String> {
    let mut prefill_ms_samples: Vec<f64> = Vec::with_capacity(repeats);
    let mut decode_tps_samples: Vec<f64> = Vec::with_capacity(repeats);
    for i in 0..repeats {
        let caught = catch_unwind(AssertUnwindSafe(|| run_ours_once(state, prompt, n_new)));
        match caught {
            Ok(Ok((p_ms, d_tps))) => {
                eprintln!(
                    "  [ours rep {}/{}] prompt_len={} prefill={:.1}ms decode={:.2} tok/s",
                    i + 1,
                    repeats,
                    prompt.len(),
                    p_ms,
                    d_tps
                );
                prefill_ms_samples.push(p_ms);
                decode_tps_samples.push(d_tps);
            }
            Ok(Err(e)) => {
                return Err(format!("{}", e));
            }
            Err(panic_err) => {
                // `catch_unwind` swallowed a panic. Try to recover a
                // string from it.
                let msg = if let Some(s) = panic_err.downcast_ref::<&'static str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "<opaque panic>".to_string()
                };
                return Err(format!("panic: {}", msg));
            }
        }
    }
    Ok((
        median(&mut prefill_ms_samples),
        median(&mut decode_tps_samples),
    ))
}

#[cfg(feature = "cuda")]
fn ours_sweep(
    args: &Args,
    prompt_lens: &[usize],
) -> Result<(Vec<StackRun>, f64)> {
    eprintln!("[ours] cuda init");
    let device = Arc::new(
        DeviceContext::new(args.cuda_device as usize)
            .with_context(|| format!("DeviceContext::new({})", args.cuda_device))?,
    );
    let meta =
        parse_qwen35_metadata(&args.target_gguf).with_context(|| "parse_qwen35_metadata")?;
    // Use the 27B gdn_ssm_dim=128 baked value (also what target.rs::bench uses).
    let config = Qwen35Config::from_metadata(&meta, 128);

    eprintln!("[ours] loading 27B (keep_packed=true)...");
    let t_load = Instant::now();
    let target = Qwen35Target::load_from_gguf_with_config(
        device.clone(),
        config.clone(),
        &args.target_gguf,
        LoaderConfig { keep_packed: true },
    )
    .with_context(|| "Qwen35Target::load_from_gguf_with_config")?;
    let load_s = t_load.elapsed().as_secs_f64();
    eprintln!(
        "[ours] loaded in {:.2}s (vocab={} layers={} n_full_attn={} n_gdn={})",
        load_s,
        target.vocab_size,
        target.layers.len(),
        target.n_full_attn,
        target.n_gdn
    );

    // Biggest prompt we'll face sets max_ctx. Add n_new headroom so
    // decode KV advances don't overrun.
    let max_prompt = *prompt_lens.iter().max().unwrap();
    let max_ctx = max_prompt + args.n_new + 256;
    let state = OurState {
        device: device.clone(),
        config,
        target,
        max_ctx,
    };

    // Warmup: 3× 128-token forward (matches target.rs::bench).
    eprintln!("[ours] warmup: {}× 128-tok prefill", args.warmup);
    let warm_prompt = synth_prompt(128);
    for i in 0..args.warmup {
        match catch_unwind(AssertUnwindSafe(|| run_ours_once(&state, &warm_prompt, 8))) {
            Ok(Ok((p_ms, d_tps))) => eprintln!(
                "  [ours warmup {}/{}] prefill={:.1}ms decode={:.2} tok/s",
                i + 1,
                args.warmup,
                p_ms,
                d_tps
            ),
            Ok(Err(e)) => {
                return Err(anyhow!("ours warmup failed: {}", e));
            }
            Err(panic_err) => {
                let msg = if let Some(s) = panic_err.downcast_ref::<&'static str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "<opaque panic>".to_string()
                };
                return Err(anyhow!("ours warmup panic: {}", msg));
            }
        }
    }

    let mut rows = Vec::with_capacity(prompt_lens.len());
    for &plen in prompt_lens {
        eprintln!(
            "\n[ours] === sweep prompt_len={} ({}× measured) ===",
            plen, args.repeats
        );
        let prompt = synth_prompt(plen);
        let r = match run_ours_at(&state, &prompt, args.n_new, args.repeats) {
            Ok((p_ms, d_tps)) => StackRun::ok(p_ms, d_tps),
            Err(e) => {
                eprintln!("[ours] FAIL at prompt_len={}: {}", plen, e);
                StackRun::fail(e)
            }
        };
        rows.push(r);
    }

    Ok((rows, load_s))
}

/// Stub used when built without the `cuda` feature so the binary at
/// least links. In CI / on the A6000 we always build with
/// `--features cuda` and this is dead code. The non-cuda build path
/// exists only so `cargo check` on a laptop doesn't require CUDA.
#[cfg(not(feature = "cuda"))]
fn ours_sweep(
    _args: &Args,
    prompt_lens: &[usize],
) -> Result<(Vec<StackRun>, f64)> {
    let rows = prompt_lens
        .iter()
        .map(|_| StackRun::fail("built without --features cuda".into()))
        .collect();
    Ok((rows, 0.0))
}

// ─────────────────────────────────────────────────────────────────────
// Table
// ─────────────────────────────────────────────────────────────────────

fn status_str(s: &Status) -> &'static str {
    match s {
        Status::Ok => "ok",
        Status::Skipped => "skip",
        Status::Fail(_) => "FAIL",
    }
}

fn format_f(x: Option<f64>) -> String {
    match x {
        Some(v) => format!("{:.2}", v),
        None => "-".to_string(),
    }
}

fn print_table(rows: &[SweepRow]) {
    println!("\n=== QWEN3.5-27B PARITY BENCH (FFI reference vs bare-metal) ===");
    println!(
        "{:>10}  {:>14}  {:>17}  {:>14}  {:>17}  {:>13}  {:>12}  {:<6}  {:<60}",
        "prompt_len",
        "ffi_prefill_ms",
        "ffi_decode_tok/s",
        "ours_prefill_ms",
        "ours_decode_tok/s",
        "prefill_ratio",
        "decode_ratio",
        "status",
        "failure_reason"
    );
    println!("{}", "-".repeat(170));
    for r in rows {
        let (prefill_ratio, decode_ratio) =
            match (r.ffi.prefill_ms, r.ffi.decode_tok_s, r.ours.prefill_ms, r.ours.decode_tok_s) {
                (Some(fp), Some(fd), Some(op), Some(od)) => {
                    let pr = if fp > 0.0 { op / fp } else { f64::NAN };
                    // "decode ratio" is defined so >1 means we are
                    // SLOWER than the reference (worse). Since decode
                    // is in tok/s (higher = better), we invert:
                    let dr = if od > 0.0 { fd / od } else { f64::NAN };
                    (Some(pr), Some(dr))
                }
                _ => (None, None),
            };
        let (combined_status, failure) = match (&r.ffi.status, &r.ours.status) {
            (Status::Ok, Status::Ok) => ("ok", String::new()),
            (Status::Fail(e), Status::Ok) => ("FAIL", format!("ffi: {}", e)),
            (Status::Ok, Status::Fail(e)) => ("FAIL", format!("ours: {}", e)),
            (Status::Fail(a), Status::Fail(b)) => ("FAIL", format!("ffi: {} ; ours: {}", a, b)),
            (Status::Skipped, Status::Ok) => ("ffi_skip", String::new()),
            (Status::Ok, Status::Skipped) => ("ours_skip", String::new()),
            _ => ("skip", String::new()),
        };
        let trunc: String = failure.chars().take(60).collect();
        println!(
            "{:>10}  {:>14}  {:>17}  {:>14}  {:>17}  {:>13}  {:>12}  {:<6}  {:<60}",
            r.prompt_len,
            format_f(r.ffi.prefill_ms),
            format_f(r.ffi.decode_tok_s),
            format_f(r.ours.prefill_ms),
            format_f(r.ours.decode_tok_s),
            format_f(prefill_ratio),
            format_f(decode_ratio),
            combined_status,
            trunc
        );
        // Emit the full failure line too (we truncate inside the table so
        // terminal widths don't split mid-message).
        if !failure.is_empty() && failure.len() > 60 {
            println!("    └── {}", failure);
        }
        let _ = status_str; // silence dead-code-if-unused warning
    }
}

// ─────────────────────────────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    // Quiet default: we WANT stderr from the reference library (its
    // [dflash] lines are data); we do not spam info-level tracing ourselves.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let args = Args::parse();
    let prompt_lens = parse_prompt_lens(&args.prompt_lens)?;
    if prompt_lens.is_empty() {
        return Err(anyhow!("--prompt-lens empty"));
    }
    if args.ffi_only && args.ours_only {
        return Err(anyhow!("--ffi-only and --ours-only are mutually exclusive"));
    }

    eprintln!(
        "\nparity bench config:\n  prompt_lens={:?}\n  n_new={}\n  warmup={}\n  repeats={}\n  cuda_device={}\n  ffi_only={}\n  ours_only={}\n",
        prompt_lens, args.n_new, args.warmup, args.repeats, args.cuda_device, args.ffi_only, args.ours_only
    );

    // Run FFI side first — it's the reference and we want its model load
    // to warm up the GPU before ours starts. (The order doesn't affect
    // tok/s once both stacks are past their own warmup, but running
    // the reference first also validates the GPU is healthy before we
    // invest the ~15 s it takes our loader to upload 27B.)
    let (ffi_rows, ffi_load_s) = if args.ours_only {
        (
            prompt_lens.iter().map(|_| StackRun::skipped()).collect(),
            0.0,
        )
    } else {
        eprintln!("── FFI reference stack ──");
        ffi_sweep(&args, &prompt_lens)?
    };

    let (ours_rows, ours_load_s) = if args.ffi_only {
        (
            prompt_lens.iter().map(|_| StackRun::skipped()).collect(),
            0.0,
        )
    } else {
        eprintln!("\n── bare-metal (ctox-qwen35-27b) stack ──");
        ours_sweep(&args, &prompt_lens)?
    };

    let rows: Vec<SweepRow> = prompt_lens
        .iter()
        .enumerate()
        .map(|(i, &plen)| SweepRow {
            prompt_len: plen,
            ffi: ffi_rows[i].clone(),
            ours: ours_rows[i].clone(),
        })
        .collect();

    print_table(&rows);
    println!(
        "\nModel-load cost (not in per-iter numbers):\n  ffi  : {:.2}s\n  ours : {:.2}s\n",
        ffi_load_s, ours_load_s
    );
    println!(
        "Notes:\n  * n_new={} per measured iter, repeats={}, warmup={} per stack.\n  * prefill_ratio, decode_ratio are ours/ffi for prefill (ms, lower=better) and ffi/ours for decode (tok/s, lower=better; >1 means we are slower than reference).\n  * Prompt: 9-token HumanEval pattern repeated to prompt_len.\n  * ffi prefill_ms = wall_s - n_generated/decode_tok_s (decode_tok_s from `[dflash] generated …` line).\n",
        args.n_new, args.repeats, args.warmup
    );

    Ok(())
}
