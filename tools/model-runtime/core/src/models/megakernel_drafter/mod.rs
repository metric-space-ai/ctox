//! Megakernel-driven Qwen3.5-0.8B drafter for speculative decoding.
//!
//! Direct CTOX driver for the reference Lucebox megakernel
//! (`megakernel/kernel.cu` + `megakernel/prefill.cu`, compiled in via
//! `core/src/cuda/dflash_megakernel_{decode,prefill}.cu`). Runs the
//! entire 24-layer hybrid DeltaNet + FullAttention forward of
//! Qwen3.5-0.8B in TWO CUDA launches per token:
//!   * prefill(N): seeds KV cache + DN state from N prompt tokens,
//!     returns the first generated token's argmax.
//!   * step(token): single fused decode launch per subsequent token.
//!
//! Reference decode throughput: ~413 tok/s on RTX 3090 (sm_86) with
//! the model running standalone. Used here as the cheap drafter in a
//! classical speculative-decoding loop against a Qwen3.5-27B Q4_K_M
//! target that verifies via DDTree tree-verify (budget 22) — same
//! macro topology as the DFlash reference but with the heavier
//! diffusion-style block draft replaced by this one-token-at-a-time
//! fast autoregressive drafter.
//!
//! Contract:
//!   * Weights come in as candle BF16 tensors on CUDA (loaded via
//!     the caller's VarBuilder).
//!   * All scratch / KV / DN state buffers live on the same CUDA
//!     device as the weights — allocated once at `new()` and reused
//!     across every prefill/step.
//!   * The drafter is single-sequence only; reset() wipes all state
//!     (position, KV, DN) between requests.
//!
//! Scope of this module: weight packing, buffer allocation, FFI
//! dispatch. The pipeline wiring (feeding target → drafter → target
//! with tree verify) lives in the upstream `pipeline::megakernel_spec`
//! module (forthcoming commit).

pub mod buffers;
pub mod constants;
pub mod driver;
pub mod loader;
pub mod weights;

pub use buffers::{MegakernelBuffers, MegakernelStateSnapshot};
pub use driver::MegakernelDrafter;
pub use loader::load_megakernel_weights;
pub use weights::{MegakernelWeights, QWEN35_0_8B_LAYER_PATTERN};
