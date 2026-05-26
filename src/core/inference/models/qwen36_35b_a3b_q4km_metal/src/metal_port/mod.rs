// Origin: CTOX
// License: AGPL-3.0-only

//! Per-op Metal Shading Language kernel ports for Qwen3.6-35B-A3B Q4_K_M.
//!
//! This module hosts **two** kernel sources, side by side, per
//! CLAUDE.md rule 4 + 5:
//!
//! - `metal_port::ops::*` — Rust dispatchers that call the vendored
//!   upstream kernels in `vendor/ggml-metal/`. These are the
//!   correctness baseline; their numbers are what every custom
//!   candidate has to beat to get promoted.
//! - `metal_port::kernels::*` — hand-authored MSL candidates that
//!   target this specific M5 + Qwen3.6-35B-A3B Q4_K_M shape contract.
//!   Each candidate ships with: a verifier against the vendored
//!   kernel within a documented tolerance, a feature-flag/runtime
//!   selector, and an isolated bench. Promotion to `accepted_profile`
//!   needs the full skill gate set.
//!
//! Stage 2 onwards — submodules appear as kernels are ported and as
//! candidates land. `// ref: vendor/ggml-metal/<file>:<line-range>`
//! anchors stay on every dispatcher; candidate kernels carry
//! `// ref: vendored kernel <name>` plus the perf hypothesis that
//! motivates them.
//!
//! Kernel families this crate will own (priority order for the "ohne
//! dflash" cut: full-attention layers + MoE FFN only):
//!
//! 1. `mul_mat_q4_k_m`               — quantized matmul, Q4_K_M weights × f16 acts
//! 2. `rms_norm`                     — RMSNorm with eps=1e-6 (stage-2 first kernel) ✅
//! 3. `rope_partial_mrope`           — M-RoPE on the leading 64 lanes
//! 4. `gqa_softmax_attn`             — exact softmax SDPA, GQA group=8
//! 5. `attn_output_gate`             — sigmoid-gated O projection input
//! 6. `moe_router`                   — top-8 of 256 experts + softmax
//! 7. `moe_expert_swiglu_q4_k_m`     — per-expert SwiGLU FFN
//! 8. `moe_shared_expert_swiglu_q4_k_m`
//! 9. `embed_get_rows`
//! 10. `lm_head_q4_k_m + sample`     — final projection + on-GPU sampling
//!
//! Explicitly deferred: `linear_attention_block`, `mtp_head`, vision tower.

#[cfg(feature = "metal")]
pub mod runtime;

pub mod ops;
