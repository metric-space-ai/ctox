//! # ctox-engine-cuda (legacy — superseded by split crates)
//!
//! This crate formerly hosted every CUDA primitive, kernel wrapper,
//! GGUF loader, and Qwen3.5 layer composition in one bucket. That
//! has been split into:
//!
//!   * `ctox-cuda-primitives` — `DeviceContext`, `CudaTensor<T>`,
//!     `KvCache`, `DType`. Shared across every per-model crate.
//!   * `ctox-qwen35-27b` — Qwen3.5-specific kernels (moved to
//!     `models/qwen35_27b/kernels/sm_XX/`), layer composition,
//!     GGUF loader, tokenizer.
//!   * `ctox-engine-runtime` — the generic `Model` trait.
//!
//! This crate is kept around as a compile-time fallback while the
//! new crates bed in; its modules are byte-identical duplicates of
//! the primitives that were copied over to `cuda-primitives`. Once
//! the driving conversation retires this crate, delete it entirely.
//!
//! Note: `kernels::*`, `gguf`, `tokenizer`, and `models::*` all
//! migrated to `ctox-qwen35-27b`.

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
    pub use crate::tensor::CudaTensor;
}

pub const CUDA_ENABLED: bool = cfg!(feature = "cuda");
