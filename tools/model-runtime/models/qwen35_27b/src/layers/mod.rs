//! Per-layer composition for Qwen3.5-27B.
//!
//! The 64-layer decoder interleaves two block types (16 FullAttention
//! + 48 GatedDeltaNet), both followed by the same SwiGLU FFN. Layer
//! forward passes here are pure compositions over the kernel
//! primitives in [`crate::kernels`]; the full decoder stitch-up
//! (embed → layer loop → lm head) lives in [`crate::target`].

pub mod ffn;
pub mod full_attention;
pub mod gdn;

pub use ffn::Qwen35FFN;
pub use full_attention::Qwen35FullAttention;
pub use gdn::Qwen35GDN;
