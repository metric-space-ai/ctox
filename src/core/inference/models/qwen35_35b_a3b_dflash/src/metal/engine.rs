//! Verify + rollback engine stubs — direct port of
//! `dflash_mlx/engine.py` + `dflash_mlx/adapter.py`.
//!
//! The Python version has two subclasses of the same base
//! (`FullAttentionEngine` for pure-attention targets, `HybridGDNEngine`
//! for GDN-hybrid targets); both forward to `runtime._verify_target_block`
//! and `runtime._restore_target_cache_after_acceptance`. Identical
//! interface split exists here so future models (pure-Qwen3, etc.)
//! can opt into the `FullAttention` variant without touching the
//! runtime.
//!
//! ref: `dflash_mlx/engine.py`, `dflash_mlx/adapter.py`

use crate::metal::cache::RecurrentRollbackCache;
use crate::metal::ffi::{CommandBuffer, Device};
use crate::metal::model::TargetWeights;

/// Engine variants — the hybrid GDN variant is the only one the
/// 35B-A3B-4bit target uses, but carrying the discriminant from day one
/// lets us share this module with future pure-attention target models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineKind {
    FullAttention,
    HybridGdn,
}

/// Engine handle. Zero-sized plus discriminant — the actual work is
/// done by the verify + rollback functions that take the usual
/// (target, cache, device) tuple.
pub struct Engine {
    pub kind: EngineKind,
}

impl Engine {
    /// Full-attention target.
    pub fn full_attention() -> Self {
        Self {
            kind: EngineKind::FullAttention,
        }
    }

    /// Hybrid GDN target (this is what Qwen3.5-35B-A3B-4bit uses).
    pub fn hybrid_gdn() -> Self {
        Self {
            kind: EngineKind::HybridGdn,
        }
    }

    pub fn arm_rollback(&self, cache: &mut RecurrentRollbackCache, prefix_len: i32) {
        cache.arm_rollback(prefix_len);
    }

    /// Roll each delta-net layer's SSM state back to the committed
    /// prefix via `tape_replay_kernel`, then forget the tapes.
    pub fn rollback(
        &self,
        dev: &Device,
        cmd: &CommandBuffer,
        cache: &mut RecurrentRollbackCache,
        n_accepted: i32,
    ) -> bool {
        // Walk every layer slot; only the GDN layers actually have
        // tapes, the rest are no-ops.
        let n_layers = cache.slots.len();
        for layer_idx in 0..n_layers {
            if !cache.rollback_layer(dev, cmd, layer_idx, n_accepted) {
                return false;
            }
        }
        cache.clear_transients();
        true
    }
}

/// ref: `adapter.py::detect_engine`
///
/// On this crate the model identity is fixed — Qwen3.5-35B-A3B is a
/// hybrid GDN target — so we short-circuit the detection. Kept as a
/// function so the public API mirrors the reference's discoverability
/// contract: callers ask "what engine should I use for this target?"
/// and get an answer.
pub fn detect_engine(_target: &TargetWeights) -> Engine {
    // Qwen3.5-35B-A3B → hybrid GDN (attention layers interleaved with
    // GatedDeltaNet). See full_attention_interval on TargetWeights.
    Engine::hybrid_gdn()
}
