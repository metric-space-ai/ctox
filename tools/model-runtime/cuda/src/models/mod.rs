//! Model layer compositions — explicit sequences of kernel launches
//! over `CudaTensor` that implement a transformer block.
//!
//! A model module composes primitives from `kernels::*` into a
//! layer-level `forward` that's ultimately wired together into the
//! full decoder by a higher-level stepper (lives outside this crate,
//! in `ctox-engine-core`).
//!
//! Per-architecture split:
//!   * `qwen35` — Qwen3.5-27B hybrid. 48 GDN (linear-attention) +
//!     16 FullAttention layers, single tied embedding + LM head.

pub mod qwen35;
