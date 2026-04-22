//! # ctox-cuda-primitives
//!
//! Thin, model-agnostic CUDA plumbing: device context, typed tensor
//! storage, KV-cache storage, dtype enum. No kernel math lives here —
//! that belongs to per-model crates under
//! `tools/model-runtime/models/<model>/`.
//!
//! ## Architectural rule
//!
//! Per-model crates (`ctox-qwen35-27b`, future `ctox-llama-70b`, etc.)
//! depend on this crate for the `CudaTensor<T>`, `DeviceContext`,
//! `KvCache`, and `DType` types so their kernel wrappers speak a
//! common storage vocabulary. They do **NOT** share kernel
//! implementations — every model brings its own `.cu` files tuned for
//! its architecture + target compute capability (sm_80 / sm_86 /
//! sm_89 / sm_90). Kernel duplication across models is intentional:
//! a Qwen3.5 head_dim=256 Q4K matmul kernel is not the same kernel as
//! a Llama-70B head_dim=128 Q4K matmul, even if the bit layout is
//! the same format.
//!
//! The generic serving / CTOX-integration layer
//! (`tools/model-runtime/engine/`) sits above the per-model crates
//! and only sees a trait-level `Model` abstraction.

#![allow(dead_code)]

#[cfg(feature = "cuda")]
pub mod device;

#[cfg(feature = "cuda")]
pub mod dtype;

#[cfg(feature = "cuda")]
pub mod tensor;

#[cfg(feature = "cuda")]
pub mod kv_cache;

#[cfg(feature = "cuda")]
pub mod prelude {
    pub use crate::device::DeviceContext;
    pub use crate::dtype::{DType, DTypeTrait};
    pub use crate::kv_cache::KvCache;
    pub use crate::tensor::{CudaTensor, TensorElem};
}

/// Sentinel const callers can reference without cfg-guarding.
pub const CUDA_ENABLED: bool = cfg!(feature = "cuda");
