#![forbid(unsafe_op_in_unsafe_fn)]

//! Bare-metal CTOX native port surface for `engineai/Voxtral-Mini-4B-Realtime-2602`.
//!
//! The CTOX model boundary is Rust orchestration, model-local artifact parsing,
//! vendored ggml platform kernels, and no external inference process.

pub mod audio;
pub mod consts;
pub mod error;
#[cfg(not(ctox_ggml_unavailable))]
pub(crate) mod ffi;
#[cfg(not(ctox_ggml_unavailable))]
mod ggml_runtime;
pub mod gguf;
pub mod kernels;
pub mod stt;

pub use consts::VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL;
pub use error::{Error, Result};
pub use kernels::{CpuBackend, KernelBackend, VoxtralSttBackend};
pub use stt::{
    inspect_gguf, shape_contract, TranscriptionRequest, TranscriptionResponse,
    VoxtralSttArtifactInspection, VoxtralSttConfig, VoxtralSttModel,
};

pub const GGML_BLAS_ENABLED: bool = cfg!(ctox_ggml_blas);
pub const GGML_CPU_ENABLED: bool = !cfg!(ctox_ggml_unavailable);
pub const GGML_METAL_ENABLED: bool = !cfg!(ctox_ggml_unavailable) && cfg!(target_os = "macos");
