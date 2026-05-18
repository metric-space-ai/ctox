#![forbid(unsafe_op_in_unsafe_fn)]

//! Bare-metal CTOX native port scaffold for `engineai/Voxtral-4B-TTS-2603`.
//!
//! Scope of this seed:
//! - no external crates;
//! - pure Rust CPU reference kernels;
//! - raw mmap/safetensors header support;
//! - kernel source layout for Metal/CUDA/WGSL backends;
//! - model constants and shape contracts matching Voxtral Realtime 4B.
//!
//! The full model graph is intentionally split into small modules so each C file
//! can be ported one-for-one while keeping platform-specific kernels isolated.

pub mod audio;
pub mod bf16;
pub mod consts;
pub mod error;
pub mod mmap;
pub mod safetensors;
pub mod tensor;

pub mod adapter;
pub mod decoder;
pub mod encoder;
pub mod kernels;
pub mod model;
pub mod speech;
pub mod stream;
pub mod tokenizer;

pub use error::{Error, Result};
pub use speech::{
    SpeechRequest, SpeechResponse, VoxtralTtsArtifactInspection, VoxtralTtsBackend,
    VoxtralTtsConfig, VoxtralTtsModel, VOXTRAL_4B_TTS_2603_CANONICAL_MODEL,
};
