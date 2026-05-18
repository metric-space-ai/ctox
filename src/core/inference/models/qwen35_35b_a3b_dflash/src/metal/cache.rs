//! `RecurrentRollbackCache` — tape-replay rollback for GatedDeltaNet.
//!
//! Records an innovation tape during the verify forward and, on a
//! speculative-decode rejection, rewinds the recurrent state by only
//! replaying the accepted prefix through the `tape_replay` Metal
//! kernel. Skips the full re-forward the naive rollback would need.
//!
//! ref: `dflash_mlx/recurrent_rollback_cache.py`

use crate::common::errors::set_last_error;
use crate::metal::ffi::{Buffer, CommandBuffer, Device};
use crate::metal::kernels;

/// One layer's slot inside the rollback cache. For a full-attention
/// layer we just hold the (K, V) pair; for a GatedDeltaNet layer we
/// hold (conv_state, ssm_state). The discriminant matches what the
/// Python side stored under `cache.cache[0]` and `cache.cache[1]`.
pub enum LayerSlot {
    /// Full-attention: `(keys, values)`. `offset` mirrors
    /// `KvCache.offset` so the rollback can truncate cleanly.
    Attn {
        keys: Buffer,
        values: Buffer,
        offset: i32,
    },
    /// GatedDeltaNet: `(conv_state, ssm_state)`.
    Delta {
        conv_state: Buffer,
        ssm_state: Buffer,
    },
    /// Layer hasn't been prefilled yet.
    Empty,
}

/// Shape info for one delta-net layer — used by `rollback` to size
/// the `tape_replay_kernel` dispatch. Populated by the driver on
/// prefill.
#[derive(Clone, Copy, Default)]
pub struct DeltaLayerShape {
    pub b: usize,
    pub hv: usize,
    pub dv: usize,
    pub dk: usize,
    pub hk: usize,
    pub t: i32,
    /// True if `g` is the vectorized [B,T,Hv,Dk] form; false for
    /// [B,T,Hv] scalar gating. Passed through to the shader variant
    /// pick.
    pub vectorized: bool,
}

pub struct TapeBundle {
    pub tape: Buffer,
    pub tape_k: Buffer,
    pub tape_g: Buffer,
    pub tape_qkv: Buffer,
    pub shape: DeltaLayerShape,
}

/// Rollback-capable cache.
///
/// `slots` has one entry per layer (order matches `TargetWeights.layers`).
/// `tapes` mirrors that layout but only the delta-net layers record a
/// tape during verify; for others the entry is `None`.
pub struct RecurrentRollbackCache {
    pub slots: Vec<LayerSlot>,
    pub snapshot: Option<Vec<LayerSlot>>, // populated by arm_rollback
    pub tapes: Vec<Option<TapeBundle>>,
    pub armed: bool,
    pub conv_kernel_size: i32,
}

impl RecurrentRollbackCache {
    pub fn new(n_layers: usize, conv_kernel_size: i32) -> Self {
        let mut slots = Vec::with_capacity(n_layers);
        let mut tapes = Vec::with_capacity(n_layers);
        for _ in 0..n_layers {
            slots.push(LayerSlot::Empty);
            tapes.push(None);
        }
        Self {
            slots,
            snapshot: None,
            tapes,
            armed: false,
            conv_kernel_size,
        }
    }

    /// Clear transients between verify cycles. Same name + role as
    /// the Python side.
    pub fn clear_transients(&mut self) {
        self.armed = false;
        self.snapshot = None;
        for t in self.tapes.iter_mut() {
            *t = None;
        }
    }

    /// Mirrors `arm_rollback` — snapshots every slot so a later
    /// `rollback(n_accepted)` can restore it.
    ///
    /// NOTE: snapshotting here moves slot buffers into the snapshot
    /// vector and leaves the live cache in `Empty`. The Python version
    /// uses MLX lazy copy-on-write so a real snapshot is cheap; on
    /// Metal we either (a) duplicate the MTLBuffer contents via a
    /// blit encoder, or (b) swap pointers and recreate the live
    /// buffers on rollback. This is a skeleton — the full snapshot
    /// machinery lands alongside the driver port (runtime.py), which
    /// is what decides when arming happens.
    pub fn arm_rollback(&mut self, _prefix_len: i32) {
        self.armed = true;
        // Snapshot is built lazily by the driver — it owns the
        // BlitEncoder needed to memcpy live cache into snapshot
        // buffers.
        self.snapshot = None;
        for t in self.tapes.iter_mut() {
            *t = None;
        }
    }

    /// Store the tape tensors produced by one verify forward step
    /// for a specific delta-net layer.
    pub fn record_tape(&mut self, layer_idx: usize, bundle: TapeBundle) {
        if layer_idx >= self.tapes.len() {
            set_last_error(format!(
                "record_tape: layer_idx={layer_idx} out of range (n_layers={})",
                self.tapes.len()
            ));
            return;
        }
        self.tapes[layer_idx] = Some(bundle);
    }

    /// Replay `n_accepted` committed tape steps into the snapshot
    /// SSM state and write it back into `slots[layer_idx]`.
    pub fn rollback_layer(
        &mut self,
        dev: &Device,
        cmd: &CommandBuffer,
        layer_idx: usize,
        n_accepted: i32,
    ) -> bool {
        let Some(bundle) = self.tapes[layer_idx].as_ref() else {
            return true; // no tape → nothing to roll back
        };
        // n_accepted matches Python's `accepted_steps = int(n_accepted) + 1`
        let accepted_steps = n_accepted + 1;
        let shape = bundle.shape;

        // The current slot holds the *new* state; the snapshot (if
        // any) holds the pre-verify SSM state. Replay accepted_steps
        // of tape over the snapshot to produce the committed state.
        let Some(snapshot) = self.snapshot.as_mut() else {
            // No snapshot → verify didn't arm; nothing to do.
            return true;
        };

        let state_in = match &snapshot[layer_idx] {
            LayerSlot::Delta { ssm_state, .. } => ssm_state,
            _ => return true,
        };
        let state_out = match &mut self.slots[layer_idx] {
            LayerSlot::Delta { ssm_state, .. } => ssm_state,
            _ => return true,
        };

        let Some(enc) = cmd.compute() else {
            set_last_error("rollback_layer: no compute encoder available");
            return false;
        };
        let ok = kernels::tape_replay_bf16(
            &enc,
            dev,
            false,
            shape.vectorized,
            &bundle.tape,
            &bundle.tape_k,
            &bundle.tape_g,
            state_in,
            accepted_steps,
            None,
            state_out,
            shape.b,
            shape.hk,
            shape.hv,
            shape.dk,
            shape.dv,
        );
        enc.end();
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sizes_slot_vec() {
        let c = RecurrentRollbackCache::new(64, 4);
        assert_eq!(c.slots.len(), 64);
        assert_eq!(c.tapes.len(), 64);
        assert!(!c.armed);
        assert_eq!(c.conv_kernel_size, 4);
    }

    #[test]
    fn clear_transients_is_idempotent() {
        let mut c = RecurrentRollbackCache::new(4, 4);
        c.arm_rollback(0);
        assert!(c.armed);
        c.clear_transients();
        assert!(!c.armed);
        c.clear_transients();
        assert!(!c.armed);
    }
}
