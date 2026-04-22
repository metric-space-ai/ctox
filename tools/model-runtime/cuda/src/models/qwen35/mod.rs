//! Qwen3.5-27B hybrid model composition.
//!
//! Two layer variants:
//!   * [`Qwen35FullAttention`] — standard multi-head attention with
//!     MRoPE and GQA (40 Q heads / 8 KV heads); 16 of 64 layers.
//!   * [`Qwen35GDN`] — Gated DeltaNet (linear-attention); 48 of 64
//!     layers.
//!
//! This module hosts per-layer forward passes only — embedding lookup,
//! the decoder loop over layers, and the LM-head projection live one
//! level up.

pub mod config;
pub mod full_attention;
pub mod gdn;
pub mod target;

pub use config::Qwen35Config;
pub use full_attention::Qwen35FullAttention;
pub use gdn::Qwen35GDN;
pub use target::{Qwen35Layer, Qwen35Target};
