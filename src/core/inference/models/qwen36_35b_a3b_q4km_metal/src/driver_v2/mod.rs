// Origin: CTOX
// License: AGPL-3.0-only

//! Stage-4 forward-pass driver.
//!
//! Composes the per-op kernels from `metal_port::ops::*` into the
//! Qwen3.6-35B-A3B forward pass:
//!
//! ```text
//!   token_id ──► get_rows_q4_K ──► residual
//!                      │
//!     for layer in 0..40 (per `LAYER_TYPES`):
//!         if layer is LinearAttention:
//!             linear_attention_block(residual, layer.weights)
//!         else:
//!             full_attention_block(residual, layer.weights)
//!         moe_ffn_block(residual, layer.moe_weights)
//!         residual += block_outputs
//!                      │
//!   final_norm ──► lm_head_q4_K_or_q6_K ──► sample ──► next token
//! ```
//!
//! Every kernel runs against the persistent `BufferPool` so per-call
//! buffer alloc is paid ONCE per session, not per dispatch (Stage-4.0
//! decision: 3.4× win on dense matvecs, 1.7× on narrow).
//!
//! This module is the orchestration code; all the math kernels are
//! already wired and verified in `metal_port::ops`. See
//! [docs/kernel-dev/OPTIMIZATION_PLAN.md] §6 for the layer-block
//! sequence the integrated bench will measure.

#![cfg(feature = "metal")]

pub mod block;
pub mod session;
