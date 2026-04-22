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

pub mod embedding;
pub mod gated_delta_net;
pub mod matmul_bf16;
pub mod mmq_q4k;
pub mod rmsnorm;
pub mod rope;
pub mod silu_mul;
pub mod softmax;

pub use embedding::{launch_embedding_bf16, launch_embedding_f16, launch_embedding_f32};
pub use gated_delta_net::{
    launch_gated_delta_net_f32, GdnGateKind, GdnInterDtype, GdnLaunchInputs, GdnPersistInter,
    GdnRecurrence, GdnShape, GDN_TREE_ROOT_PARENT,
};
pub use matmul_bf16::{launch_matmul_bf16_bf16, launch_matmul_bf16_f32};
pub use mmq_q4k::{launch_mmvq_q4k_f16, launch_mmvq_q4k_f32};
pub use rmsnorm::launch_rmsnorm_f32;
pub use rope::launch_rope_mrope_bf16;
pub use silu_mul::{launch_silu_mul_bf16, launch_silu_mul_f32};
pub use softmax::launch_softmax_f32;
