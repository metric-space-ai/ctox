//! Metal helpers for the Qwen3.5-0.8B research probe.

pub mod bench;
pub mod ffi;
#[cfg(target_os = "macos")]
pub mod mps_sidecar;
