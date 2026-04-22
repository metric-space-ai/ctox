//! Qwen3.5 hybrid model building blocks.
//!
//! 27B uses a 1:3 full-attention / gated-delta-net ratio (16 full
//! attention layers, 48 GDN layers). This module owns the CUDA-side
//! layer composition for the full-attention half. GDN lives alongside
//! in a sibling module owned by Agent J.

pub mod config;
pub mod full_attention;

pub use config::Qwen35Config;
pub use full_attention::Qwen35FullAttention;
