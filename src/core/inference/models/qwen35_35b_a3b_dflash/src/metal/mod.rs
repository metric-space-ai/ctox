//! Metal + Apple-Silicon implementation of the Qwen3.5-35B-A3B DFlash
//! model pair.
//!
//! Active on `cfg(target_os = "macos")`. Rust-native port of
//! [bstnxbt/dflash-mlx], with **zero MLX runtime dependency**:
//!
//!  * Every Metal compute shader used by the model (both dflash's custom
//!    kernels and the baseline ops that MLX normally ships)
//!    lives under `vendor/metal/shaders/` and is compiled into a single
//!    `ctox_qwen35_35b_a3b_dflash.metallib` by `build.rs` via `xcrun metal`
//!    + `xcrun metallib`.
//!  * The Rust runtime talks to Metal via the `objc2-metal` bindings:
//!    `MTLDevice`, `MTLCommandQueue`, `MTLBuffer`,
//!    `MTLComputePipelineState`. No Python, no libmlx on the link line.
//!
//! # Submodules
//!
//!   * [`ffi`]     — thin Metal device + command-queue + pipeline-cache
//!                   wrapper.
//!   * [`qwen`]    — native reimplementation of the Qwen3.5 base
//!                   modules that MLX' `mlx_lm.models.qwen3` supplies
//!                   (RMSNorm, MLP, Attention, GatedDeltaNet, RoPE, KV
//!                   cache). This is what the DFlash runtime calls into.
//!   * [`model`]   — TargetWeights / DraftWeights / TargetCache
//!                   (ref: `dflash_mlx/model.py`).
//!   * [`loader`]  — MLX-4bit safetensors loader (target) + z-lab
//!                   DFlash safetensors loader (draft).
//!   * [`cache`]   — `RecurrentRollbackCache` (ref:
//!                   `dflash_mlx/recurrent_rollback_cache.py`).
//!   * [`verify`]  — `verify_linear` + `verify_qmm` ports.
//!   * [`kernels`] — Rust-side wrappers around each compiled Metal
//!                   shader in the metallib (dispatch helpers).
//!   * [`driver`]  — spec-decode driver (ref: `dflash_mlx/runtime.py` +
//!                   `generate.py`).
//!
//! All submodules are currently `pub mod`-declared as skeletons while
//! the port is in progress. Each submodule carries `// ref: <file>:<line>`
//! annotations pointing at the Python reference in `vendor/metal/dflash-mlx-ref/`
//! so reviewers can diff line-by-line, same discipline as the CUDA
//! port against `vendor/cuda/` + `lucebox/dflash/`.
//!
//! [bstnxbt/dflash-mlx]: https://github.com/bstnxbt/dflash-mlx

pub mod cache;
pub mod draft_backend;
pub mod driver;
pub mod engine;
pub mod ffi;
pub mod forward;
pub mod gguf;
pub mod kargs;
pub mod kernels;
pub mod loader;
pub mod mlx_ops;
pub mod model;
pub mod moe;
pub mod ops;
pub mod qwen;
pub mod runtime;
pub mod tensor;
pub mod verify;
pub mod vision;
pub mod weights;
pub mod work;
