//! Metal backend staging for Qwen3-Embedding-0.6B.
//!
//! The selected kernel seed is vendored under
//! `vendor/metal/kernels/ctox_qwen3_embedding_glue.metal`. It is not linked
//! yet; the next implementation slice ports the Rust command-encoder glue from
//! the existing Qwen3.5 Metal backend and removes generation-only entrypoints.

pub const KERNEL_MANIFEST: &[&str] = &[
    "token_embedding/get_rows",
    "rms_norm",
    "rope",
    "q4_k_or_bf16_matmul",
    "sdpa_or_attention",
    "silu",
    "last_token_pool",
    "l2_normalize",
];
