#![forbid(unsafe_op_in_unsafe_fn)]

//! Native text-to-audio graph for CTOX's `engineai/Voxtral-4B-TTS-2603` alias.
//!
//! The production candidate uses the pinned model-local MIT C graph and native
//! CUDA kernels, with Rust artifact validation and serialized warm sessions.
//! Earlier Rust reference modules remain correctness/scaffold material; Metal
//! and WGSL graph requests fail closed. See README.md for actual limitations.

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
mod native_graph;
pub mod speech;
pub mod stream;
pub mod tokenizer;

pub use error::{Error, Result};
pub use speech::{
    SpeechRequest, SpeechResponse, VoxtralTtsArtifactInspection, VoxtralTtsBackend,
    VoxtralTtsConfig, VoxtralTtsModel, VOXTRAL_4B_TTS_2603_CANONICAL_MODEL,
};
