// Origin: CTOX
// License: AGPL-3.0-only

//! Self-contained Qwen3.6-35B-A3B Q4_K_M Metal-only inference engine
//! for Apple Silicon (M-series). Stage-1 skeleton.
//!
//! See `docs/kernel-dev/MODEL_SHAPE.md` for the frozen kernel ABI and
//! `RESEARCH_LOG.md` for the chronological tuning record.
//!
//! Stage-1 deliberately ships no kernel code and no end-to-end forward
//! pass — the `Engine::run_turn` IPC entry point currently returns a
//! `not_ready` error. The point of this stage is to establish the
//! crate boundary, the kernel ABI freeze, and the hardware probe.

pub mod driver;
#[cfg(feature = "metal")]
pub mod driver_v2;
pub mod loader;
pub mod metal_port;
pub mod model;
pub mod server;
pub mod wire;

pub use model::{Qwen36MoeTextConfig, QWEN36_35B_A3B_TEXT_CONFIG};
