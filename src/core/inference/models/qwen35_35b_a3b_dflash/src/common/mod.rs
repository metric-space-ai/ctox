//! Backend-agnostic building blocks shared between the CUDA path
//! (`src/cuda/`) and the Metal path (`src/metal/`).
//!
//! Nothing in this module may depend on `ggml`, `ggml_tensor`, `MTLDevice`,
//! or any other backend-specific type. Keep it 100 % Rust.

pub mod constants;
pub mod errors;
