//! ctox-qwen35-35b-a3b-dflash — self-contained Qwen3.5-35B-A3B (MoE)
//! DFlash speculative-decoding crate.
//!
//! Target architecture: Qwen3.5 hybrid text stack with 40 layers,
//! `linear_attention` except every fourth `full_attention` layer, and
//! MoE MLP blocks. Each text layer routes over 256 experts with
//! top-8 activation; the "A3B" name reflects that only ~3 B of the
//! 35 B parameters are active per token. The DFlash draft is the
//! 8-layer block-diffusion variant from `z-lab/Qwen3.5-35B-A3B-DFlash`
//! and consumes five captured target-layer states.
//!
//! # Backends
//!
//!   * `cfg(target_os = "macos")`  → Metal path in `src/metal/`.
//!     Port of [bstnxbt/dflash-mlx] adapted for MoE. All kernels and
//!     the `libmlx`-replacement base ops live under
//!     `vendor/metal/shaders/` — no MLX runtime dependency.
//!
//!   * `cfg(target_os = "linux")` → fail-fast inference stub with the
//!     first owned CUDA glue kernels vendored under `vendor/cuda/`.
//!     No ggml runtime, tensor runtime, or external inference library is
//!     linked; full CUDA text inference still has to be ported.
//!
//! # No cross-model code sharing
//!
//! Per the curated-model rule in the root development guide, this crate vendors its own
//! copies of every shader and every FFI shim — no code or kernels are
//! shared with `qwen35_27b_dflash` or any other model crate. The
//! directory trees look similar because the generic ops (RMSNorm,
//! RoPE, quantized matmul, …) do the same thing for every Qwen3-family
//! model; they are textually identical copies for now. If one crate
//! later tunes a shader for its specific shape, the other does not
//! inherit that tuning.
//!
//! [bstnxbt/dflash-mlx]: https://github.com/bstnxbt/dflash-mlx
//! [lucebox/dflash]:     https://github.com/lucebox/dflash

pub mod common;

// ─── Re-export shared model constants + error slot at the crate root ─

pub use common::constants::*;
pub use common::errors::{last_error, set_last_error};

// ─── Backend modules ────────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub mod metal;

#[cfg(target_os = "linux")]
pub mod cuda;
