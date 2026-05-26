// Origin: CTOX
// License: AGPL-3.0-only

//! Engine driver — orchestrates loading, KV-cache layout, prefill,
//! decode, and sampling. Stage-1 is a typed placeholder so the server
//! and bench binaries compile and report a clean "not_ready" instead of
//! pretending to do inference.

use anyhow::Result;
use thiserror::Error;

use crate::model::{Qwen36MoeTextConfig, QWEN36_35B_A3B_TEXT_CONFIG};

/// Owned engine handle. Stage-1 only carries the frozen kernel ABI.
pub struct Engine {
    pub config: Qwen36MoeTextConfig,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(
        "qwen36-35b-a3b-q4km-metal engine is in stage-1 skeleton: \
         no Q4_K_M GGUF loader, no MSL kernels, no forward pass yet. \
         Use the qwen36_35b_a3b_ggml shim for actual inference until \
         stage 2 of the local-llm-inference-optimization skill lands."
    )]
    NotReady,
}

impl Engine {
    /// Build the stage-1 engine. Always succeeds; it just gives callers
    /// a typed handle they can route IPC traffic at.
    pub fn new() -> Self {
        Self {
            config: QWEN36_35B_A3B_TEXT_CONFIG.clone(),
        }
    }

    /// Run one Responses turn. Stage-1: always errors with `NotReady`.
    pub fn run_turn(&self) -> Result<(), EngineError> {
        Err(EngineError::NotReady)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
