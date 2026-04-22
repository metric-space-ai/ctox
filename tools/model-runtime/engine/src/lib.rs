//! # ctox-engine-runtime
//!
//! Generic, model-agnostic serving layer. Per-model crates
//! (`ctox-qwen35-27b`, future `ctox-llama-70b`, etc.) implement the
//! [`Model`] trait defined here. CTOX's upstream infra
//! (`src/execution/...` in the main CTOX crate) talks to this
//! trait only; it never sees model-specific types.
//!
//! ## Relationship to the per-model and primitive tiers
//!
//! Three-layer stack:
//!
//! ```text
//!   ┌──────────────────────────────────┐
//!   │ ctox-engine-runtime (THIS crate) │  ← model-agnostic, trait-based
//!   │  • trait Model                    │
//!   │  • serving loop, streaming        │
//!   │  • KV pool orchestration          │
//!   │  • tokenizer bridge               │
//!   └───────────────┬──────────────────┘
//!                   │  impl Model
//!   ┌───────────────┴──────────────────┐
//!   │ ctox-qwen35-27b  (per model)     │  ← model-specific
//!   │  • 64-layer decoder composition   │
//!   │  • GGUF loader (Qwen3.5 naming)   │
//!   │  • sm_XX kernel dispatch          │
//!   │  • tokenizer adapter              │
//!   └───────────────┬──────────────────┘
//!                   │  uses
//!   ┌───────────────┴──────────────────┐
//!   │ ctox-cuda-primitives (shared)    │  ← plumbing only
//!   │  • DeviceContext, CudaTensor     │
//!   │  • KvCache, DType                │
//!   │  • (no kernels, no math)         │
//!   └──────────────────────────────────┘
//! ```
//!
//! Per-model kernels (`models/<model>/kernels/sm_XX/*.cu`) are
//! vendored from upstream (llama.cpp ggml-cuda + dflash), not
//! reimplemented. See `models/qwen35_27b/vendor/README.md`.

#![allow(dead_code)]

pub mod model;

pub use model::{Model, ModelInput, ModelOutput};
