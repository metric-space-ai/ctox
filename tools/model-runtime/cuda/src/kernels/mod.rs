//! Kernel registry — one Rust wrapper module per fused CUDA op.
//!
//! Each module provides:
//!   * A PTX blob (compiled at build time from `kernels/*.cu` via
//!     `nvcc`, embedded by `build.rs`).
//!   * A `launch_<kernel>_<dtype>()` Rust wrapper that validates
//!     shapes, caches the loaded `CudaFunction`, and launches without
//!     synchronizing the stream.
//!
//! See `rmsnorm` as the canonical template when adding a new kernel.
//!
//! PTX blobs (`<STEM>_PTX: &[u8]`) live in the auto-generated
//! `ptx_registry.rs` emitted by `build.rs`. Included here once so all
//! kernel modules reference the same constants via `super::…_PTX`.

#![allow(clippy::missing_safety_doc)]

// AUTO-GENERATED PTX blobs: RMSNORM_PTX: &[u8], PtxBlob struct,
// PTX_BLOBS: &[PtxBlob]. One entry per kernels/*.cu compiled.
include!(concat!(env!("OUT_DIR"), "/ptx_registry.rs"));

pub mod mmq_q4k;
pub mod rmsnorm;

pub use mmq_q4k::{launch_mmvq_q4k_f16, launch_mmvq_q4k_f32};
pub use rmsnorm::launch_rmsnorm_f32;
