//! # ctox-qwen35-27b
//!
//! Bare-metal CUDA implementation of Qwen3.5-27B hybrid (the
//! 16-FullAttention + 48-GatedDeltaNet variant from the z-lab /
//! dflash line). Kernels are vendored verbatim under `kernels/sm_XX/`
//! from `ggml-cuda` (llama.cpp) and `dflash` — this crate does NOT
//! write its own CUDA math. The Rust side is the glue layer that:
//!
//!   * Loads GGUF weights (Qwen3.5-specific tensor naming).
//!   * Composes the 64-layer decoder (FA and GDN layers interleaved).
//!   * Dispatches kernel launches to the arch-specific variant
//!     (sm_80 / sm_86 / sm_89 / sm_90) at runtime.
//!
//! ## Why duplicate kernels across models instead of sharing?
//!
//! Each model's kernels are tuned for that model's shapes + target
//! compute generation. A Qwen3.5 head_dim=256 attention kernel is
//! not interchangeable with a Llama-70B head_dim=128 one even if
//! the format is nominally the same — tile sizes, register pressure,
//! and shared-memory allocation differ. Keeping a per-model copy
//! means each model stays independently optimizable without
//! regression risk elsewhere. See the top-level architecture doc in
//! `tools/model-runtime/README.md`.

#![allow(dead_code)]

#[cfg(feature = "cuda")]
pub mod arch_dispatch;

#[cfg(feature = "cuda")]
pub mod kernels;

#[cfg(feature = "cuda")]
pub mod layers;

#[cfg(feature = "cuda")]
pub mod config;

#[cfg(feature = "cuda")]
pub mod gguf_loader;

#[cfg(feature = "cuda")]
pub mod target;

#[cfg(feature = "cuda")]
pub mod tokenizer;

#[cfg(feature = "cuda")]
pub mod prelude {
    pub use crate::config::Qwen35Config;
    pub use crate::target::Qwen35Target;
    pub use crate::tokenizer::Qwen35Tokenizer;
}

pub const CRATE_NAME: &str = "ctox-qwen35-27b";
