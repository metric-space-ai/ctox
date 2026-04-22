//! # ctox-engine-cuda
//!
//! Bare-metal CUDA primitives for the CTOX engine. This crate is the
//! **replacement** for candle's tensor + kernel layer inside the
//! engine's hot path — not a candle wrapper, not a transformers
//! re-implementation, just a thin typed layer over `cudarc` with a
//! kernel registry for explicit launches.
//!
//! ## Why not candle?
//!
//! Candle's per-op dispatch, dtype auto-cast, and sync-per-op default
//! cost us roughly 10× throughput vs the ggml-cuda reference on the
//! 27B decode hot path. The project's production scope — three to
//! five model families, NVIDIA-first, long-lived serving — means the
//! abstractions that candle provides (backend agnosticism, eager-mode
//! ergonomics, wide model coverage) are taxes without corresponding
//! benefits for us. This crate owns the replacement.
//!
//! ## Scope
//!
//! * `DeviceContext` — one CUDA context/device/stream, owned and
//!   passed by `Arc` to everything below.
//! * `CudaTensor<T>` — raw device buffer + shape + stride, typed by
//!   element. No ops on tensors — those go through kernel launches.
//! * `KvCache` — ring-buffer KV store keyed by (layer, head) with
//!   explicit pointer math (no candle `Tensor::cat`).
//! * `kernels::*` — per-op kernel wrappers, one Rust function per
//!   fused op. Each wrapper owns its launch config (grid/block) and
//!   does NOT implicitly sync the stream.
//!
//! ## What's not here
//!
//! * Model definitions (those live in `ctox-engine-core` as explicit
//!   sequences of kernel calls over `CudaTensor`).
//! * Graph construction, autograd, or lazy execution.
//! * CPU fallback (use a separate reference implementation for diffs,
//!   like our `draft_diff_bench`).
//!
//! ## Build gating
//!
//! The whole crate is behind the `cuda` feature. Without it, the crate
//! compiles to empty stubs so workspace builds on non-CUDA hosts (Macs
//! for dev, CI jobs without nvcc) succeed. Callers who need CUDA-only
//! types should also feature-gate.

#![allow(dead_code)]

// Tokenization is pure Rust and deliberately un-gated: consumers on
// non-CUDA hosts (Mac dev boxes, CI shapes without nvcc) still need
// to turn prompts into ids for bring-up and round-trip tests.
pub mod tokenizer;

#[cfg(feature = "cuda")]
pub mod device;

#[cfg(feature = "cuda")]
pub mod dtype;

#[cfg(feature = "cuda")]
pub mod tensor;

#[cfg(feature = "cuda")]
pub mod kv_cache;

#[cfg(feature = "cuda")]
pub mod kernels;

#[cfg(feature = "cuda")]
pub mod gguf;

#[cfg(feature = "cuda")]
pub mod models;

/// Re-exports for the common call path. Members gated on the `cuda`
/// feature — consumers that need them must also gate.
#[cfg(feature = "cuda")]
pub mod prelude {
    pub use crate::device::DeviceContext;
    pub use crate::dtype::{DType, DTypeTrait};
    pub use crate::kv_cache::KvCache;
    pub use crate::tensor::CudaTensor;
}

/// Stub sentinel visible on non-CUDA builds. Lets dependents reference
/// `ctox_engine_cuda::CUDA_ENABLED` without cfg-juggling at the call
/// site.
pub const CUDA_ENABLED: bool = cfg!(feature = "cuda");
