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
//!
//! ## Module layout
//!
//! ```text
//!   src/
//!     lib.rs                   — this file
//!     config.rs                — Qwen35Config (hoisted from layer dir)
//!     target.rs                — Qwen35Target (full-model forward)
//!     gguf_loader.rs           — Qwen3.5-naming GGUF parser
//!     tokenizer.rs             — Qwen35Tokenizer (HF wrapper)
//!     layers/
//!       mod.rs, ffn.rs, full_attention.rs, gdn.rs
//!     kernels/
//!       mod.rs, rmsnorm.rs, softmax.rs, rope.rs,
//!       flash_attn.rs, mmq_*.rs, ...
//!   kernels/                   — raw .cu source, nvcc-compiled in build.rs
//!     sm_80/ (empty, TODO),  sm_86/ (populated),
//!     sm_89/ (empty, TODO),  sm_90/ (empty, TODO)
//! ```

#![allow(dead_code)]

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
    pub use crate::layers::{Qwen35FFN, Qwen35FullAttention, Qwen35GDN};
    pub use crate::target::{Qwen35Layer, Qwen35Target};
    pub use crate::tokenizer::Qwen35Tokenizer;
}

// Top-level re-exports for external consumers (matches the old
// `cuda/src/models/qwen35/mod.rs` public surface).
#[cfg(feature = "cuda")]
pub use config::Qwen35Config;
#[cfg(feature = "cuda")]
pub use layers::{Qwen35FFN, Qwen35FullAttention, Qwen35GDN};
#[cfg(feature = "cuda")]
pub use target::{Qwen35Layer, Qwen35Target};
#[cfg(feature = "cuda")]
pub use tokenizer::Qwen35Tokenizer;

pub const CRATE_NAME: &str = "ctox-qwen35-27b";
pub const CUDA_ENABLED: bool = cfg!(feature = "cuda");
