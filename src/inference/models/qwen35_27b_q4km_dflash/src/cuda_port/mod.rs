//! Bare-metal Rust port of llama.cpp's ggml-cuda host-side dispatcher.
//!
//! See CLAUDE.md "Inference-Engine Architecture (HARD rules)" for the
//! contract: each model crate ports the C++ dispatcher byte-for-byte
//! into Rust inside its own tree. This module is **the port for the
//! qwen35_27b_q4km_dflash crate only** — sibling model crates will
//! have their own port trees, never imports from here.
//!
//! # Pattern
//!
//! The upstream files under `vendor/ggml-cuda/` come in two layers:
//!
//! * **Kernel layer** (`__global__` / `__device__` functions): these are
//!   left unmodified and compiled by `build.rs` into PTX. Example:
//!   `rms_norm_f32<block_size>` template at `norm.cu:153-191`.
//!
//! * **Host-launcher layer** (`static void <op>_<dtype>_cuda(...)` and
//!   `void ggml_cuda_op_<op>(...)`): these are what this module ports.
//!   Each launcher becomes a Rust function with a `// ref:` doc anchor
//!   pointing at the upstream line range. Variable names are preserved.
//!
//! The giant switch in `ggml-cuda.cu` that dispatches ops ends up in
//! [`dispatch`] below, with the subset of ops this model actually uses.
//!
//! # Status
//!
//! Scaffolding only. The FFI `graph.rs` path (via `libggml-cuda.so`)
//! remains the default runtime; this module is built alongside it and
//! will take over op-by-op as each port lands + is bit-exact-verified.

pub mod driver;
pub mod module;
pub mod ops;
pub mod ptx;
