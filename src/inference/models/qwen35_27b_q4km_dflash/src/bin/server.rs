//! `qwen35-27b-q4km-dflash-server` — local inference socket server.
//!
//! Loads the Qwen3.5-27B Q4_K_M target weights + the DFlash draft,
//! binds a Unix domain socket, and serves CTOX's line-delimited JSON
//! IPC (matching `src/harness/core/src/client.rs::LocalIpcRequest`).
//! Emits OpenAI Responses API stream events.
//!
//! No HTTP, no TCP, no TLS. Peer UID is checked against the server
//! UID on each accept (Linux, via SO_PEERCRED).
//!
//! CLI:
//!
//! ```text
//! qwen35-27b-q4km-dflash-server \
//!     --target    <target.gguf> \
//!     --draft     <draft.safetensors> \
//!     --tokenizer <tokenizer.json> \
//!     --socket    <path> \
//!     [--cuda-device 0] \
//!     [--max-ctx 4096] \
//!     [--max-verify-tokens 0]
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use tokio::sync::Mutex;

use ctox_qwen35_27b_q4km_dflash as dflash;
use dflash::ffi as sys;
use dflash::graph::create_target_cache;
use dflash::loader::{load_draft_safetensors, load_target_gguf};
use dflash::model::{DraftWeights, TargetCache, TargetWeights};
use dflash::server::{serve, Engine, ServeConfig};
use dflash::tokenizer::Tokenizer;

#[derive(Parser, Debug)]
#[command(
    name = "qwen35-27b-q4km-dflash-server",
    about = "Local inference server (Unix-socket IPC, no HTTP) for Qwen3.5-27B Q4_K_M + DFlash draft"
)]
struct Args {
    /// Qwen3.5-27B Q4_K_M GGUF path.
    #[arg(long)]
    target: PathBuf,
    /// DFlash draft safetensors path (z-lab/Qwen3.5-27B-DFlash).
    #[arg(long)]
    draft: PathBuf,
    /// tokenizer.json (from the HuggingFace Qwen3.5-27B snapshot).
    /// If omitted the server tries the default HF-cache location.
    #[arg(long)]
    tokenizer: Option<PathBuf>,
    /// Unix-domain-socket path to bind (mode 0600, parent 0700).
    #[arg(long)]
    socket: PathBuf,
    /// CUDA device index.
    #[arg(long, default_value_t = 0)]
    cuda_device: i32,
    /// Target cache `max_ctx`.
    #[arg(long, default_value_t = 4096)]
    max_ctx: i32,
    /// `max_verify_tokens` — pass 0 to use default DRAFT_BLOCK_SIZE=16.
    #[arg(long, default_value_t = 0)]
    max_verify_tokens: i32,
    /// Model ID reported in health probes + Responses envelopes.
    #[arg(long, default_value = "qwen35-27b-q4km-dflash")]
    model_id: String,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();

    // 1. CUDA backend.
    let backend = unsafe { sys::ggml_backend_cuda_init(args.cuda_device) };
    if backend.is_null() {
        return Err(anyhow!(
            "ggml_backend_cuda_init failed for device {}",
            args.cuda_device
        ));
    }

    // 2. Load target.
    let mut target = TargetWeights::default();
    if !load_target_gguf(&args.target, backend, &mut target) {
        unsafe { sys::ggml_backend_free(backend) };
        return Err(anyhow!(
            "load_target_gguf failed: {}",
            dflash::last_error()
        ));
    }
    tracing::info!(target = ?args.target, "target loaded");

    // 3. Load draft.
    let mut draft = DraftWeights::default();
    if !load_draft_safetensors(&args.draft, backend, &mut draft) {
        unsafe { sys::ggml_backend_free(backend) };
        return Err(anyhow!(
            "load_draft_safetensors failed: {}",
            dflash::last_error()
        ));
    }
    tracing::info!("draft loaded");

    // 4. Build target cache. Default `max_verify_tokens` = 16 (draft
    //    block size). If DDTree opens up later we bump the cap here.
    let mvt_eff = if args.max_verify_tokens > 0 {
        args.max_verify_tokens
    } else {
        16
    };
    let mut cache = TargetCache::default();
    if !create_target_cache(&target, args.max_ctx, mvt_eff, backend, &mut cache) {
        unsafe { sys::ggml_backend_free(backend) };
        return Err(anyhow!(
            "create_target_cache failed: {}",
            dflash::last_error()
        ));
    }
    tracing::info!(max_ctx = args.max_ctx, "target cache ready");

    // 5. Tokenizer.
    let tok_path = match args.tokenizer {
        Some(p) => p,
        None => Tokenizer::resolve_default()
            .context("no --tokenizer given and no HF-cache tokenizer.json found")?,
    };
    let tokenizer = Tokenizer::from_file(&tok_path)?;
    tracing::info!(tokenizer = ?tok_path, "tokenizer ready");

    // 6. Engine handle + async runtime.
    let engine = Engine {
        target,
        draft,
        cache,
        backend,
        tokenizer,
        model_id: args.model_id,
    };
    let shared = Arc::new(Mutex::new(engine));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .context("tokio runtime")?;
    rt.block_on(async move {
        serve(
            shared,
            ServeConfig {
                socket_path: args.socket,
            },
        )
        .await
    })?;

    Ok(())
}
