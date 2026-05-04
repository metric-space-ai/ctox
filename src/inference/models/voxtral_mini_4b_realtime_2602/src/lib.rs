#![forbid(unsafe_op_in_unsafe_fn)]

//! Bare-metal CTOX native port surface for `engineai/Voxtral-Mini-4B-Realtime-2602`.
//!
//! This crate is seeded from andrijdavid/voxtral.cpp but keeps the CTOX model
//! boundary: Rust orchestration, model-local artifact parsing, model-local
//! platform kernel slots, and no external inference process.

pub mod audio;
pub mod consts;
pub mod error;
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
