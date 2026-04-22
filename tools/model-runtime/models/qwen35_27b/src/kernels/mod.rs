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

pub mod cast;
pub mod embedding;
pub mod flash_attn;
pub mod fused_ops;
pub mod gated_delta_net;
pub mod head_gather;
pub mod l2_norm;
pub mod matmul_bf16;
pub mod mmq_iq4_xs;
pub mod mmq_q4k;
pub mod mmq_q5k;
pub mod mmq_q6k;
pub mod mmq_q8_0;
pub mod quantize_q8_1;
pub mod residual;
pub mod rmsnorm;
pub mod rope;
pub mod silu_mul;
pub mod softmax;
pub mod ssm_conv1d;

pub use cast::{
    launch_cast_bf16_to_f32, launch_cast_f16_to_f32, launch_cast_f32_to_bf16,
    launch_cast_f32_to_f16,
};
pub use embedding::{launch_embedding_bf16, launch_embedding_f16, launch_embedding_f32};
pub use flash_attn::launch_flash_attn_bf16;
pub use fused_ops::{
    launch_scale_add_f32, launch_scale_add_with_bias_f32, launch_sigmoid_bf16,
    launch_sigmoid_mul_bf16, launch_transpose_2d_bf16,
};
pub use gated_delta_net::{
    launch_gated_delta_net_f32, GdnGateKind, GdnInterDtype, GdnLaunchInputs, GdnPersistInter,
    GdnRecurrence, GdnShape, GDN_TREE_ROOT_PARENT,
};
pub use head_gather::{
    launch_head_gather_bf16, launch_head_gather_slab_bf16, launch_head_scatter_bf16,
};
pub use l2_norm::launch_l2_norm_bf16;
pub use matmul_bf16::{launch_matmul_bf16_bf16, launch_matmul_bf16_f32};
pub use mmq_iq4_xs::{
    launch_mmvq_iq4_xs_f16, launch_mmvq_iq4_xs_f32, launch_mmvq_iq4_xs_q8_1_f16,
    launch_mmvq_iq4_xs_q8_1_f32, launch_mmvq_iq4_xs_q8_1_f32_view,
};
pub use mmq_q4k::{
    launch_mmvq_q4k_f16, launch_mmvq_q4k_f32, launch_mmvq_q4k_q8_1_f16, launch_mmvq_q4k_q8_1_f32,
    launch_mmvq_q4k_q8_1_f32_view,
};
pub use mmq_q5k::{launch_mmvq_q5k_f16, launch_mmvq_q5k_f32, launch_mmvq_q5k_f32_view};
pub use mmq_q6k::{launch_mmvq_q6k_f16, launch_mmvq_q6k_f32, launch_mmvq_q6k_f32_view};
pub use mmq_q8_0::{launch_mmvq_q8_0_f16, launch_mmvq_q8_0_f32, launch_mmvq_q8_0_f32_view};
pub use quantize_q8_1::{launch_quantize_q8_1_f32, q8_1_packed_bytes};
pub use residual::{launch_residual_add_bf16, launch_residual_add_f32};
pub use rmsnorm::launch_rmsnorm_f32;
pub use rope::launch_rope_mrope_bf16;
pub use silu_mul::{launch_silu_mul_bf16, launch_silu_mul_f32};
pub use softmax::launch_softmax_f32;
pub use ssm_conv1d::launch_ssm_conv1d_bf16;
