//! Model layer compositions — explicit sequences of kernel launches
//! over `CudaTensor` that implement a transformer block. Each family
//! lives in its own submodule; the crate's stance is "no runtime
//! graph", so these are plain Rust functions/structs that call
//! `kernels::launch_*` directly.
//!
//! A module per model family lets downstream crates pick only the
//! primitives they need; the CUDA primitives crate stays free of
//! transformer-shaped assumptions.

pub mod qwen35;
