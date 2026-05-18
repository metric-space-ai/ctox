//! Port of `dflash_mlx/draft_backend.py`.
//!
//! The Python `EagerDraftBackend` wraps two things:
//!
//!   1. `make_cache(...)` — allocate one `ContextOnlyDraftKVCache`
//!      (sink + window rolling KV cache) per draft-layer.
//!   2. `draft_greedy(...)` — drive one block-diffusion draft pass.
//!      Concretely: form the block token IDs
//!      `[staged_first[0], mask_token, mask_token, … (block_len-1 of them)]`,
//!      embed them into noise embedding, run the draft forward with
//!      the target's captured hidden features, take the argmax at
//!      every position but the first, return the resulting IDs.
//!
//! The Rust port splits the two into separate functions but keeps
//! the shape contract identical so the driver translation is 1:1.
//!
//! ref: `dflash_mlx/draft_backend.py`, `dflash_mlx/model.py`

use crate::common::constants::{DFLASH35B_DRAFT_MASK_TOKEN_ID, DFLASH35B_TARGET_HIDDEN};
use crate::common::errors::set_last_error;
use crate::metal::ffi::{Buffer, CommandBuffer, Device};

/// Sink+window rolling KV cache for the draft's per-layer attention.
///
/// `sink_size` tokens at the start + most recent `window_size` tokens
/// are kept; anything between the sink and the window gets dropped on
/// `apply_window`. Matches `ContextOnlyDraftKVCache` in the reference.
pub struct DraftKvCache {
    pub sink_size: i32,
    pub window_size: i32,
    pub keys: Option<Buffer>,
    pub values: Option<Buffer>,
    pub offset: i32,
}

impl DraftKvCache {
    pub fn new(sink_size: i32, window_size: i32) -> Self {
        Self {
            sink_size,
            window_size,
            keys: None,
            values: None,
            offset: 0,
        }
    }

    pub fn cache_length(&self) -> i32 {
        if self.keys.is_none() {
            0
        } else {
            self.offset
        }
    }

    pub fn reset(&mut self) {
        self.keys = None;
        self.values = None;
        self.offset = 0;
    }
}

/// Allocate one cache per draft layer.
pub fn make_cache(n_draft_layers: usize, sink_size: i32, window_size: i32) -> Vec<DraftKvCache> {
    (0..n_draft_layers)
        .map(|_| DraftKvCache::new(sink_size, window_size))
        .collect()
}

/// Block of token IDs fed into the draft's noise-embedding path:
/// `[staged_first[0], MASK, MASK, … (block_len-1 masks)]`.
/// Returns the IDs as an owned `Vec<i32>`; caller uploads to a Buffer
/// before running the draft forward.
pub fn build_block_token_ids(staged_first: i32, block_len: i32) -> Vec<i32> {
    if block_len <= 1 {
        set_last_error("build_block_token_ids: block_len must be > 1");
        return vec![staged_first];
    }
    let mut out = Vec::with_capacity(block_len as usize);
    out.push(staged_first);
    for _ in 1..block_len {
        out.push(DFLASH35B_DRAFT_MASK_TOKEN_ID);
    }
    out
}

/// Shape sanity helper: draft hidden state should be sized
/// `[1, block_len, hidden]` before the lm_head projection. The target
/// hidden features fed into the draft's `fc` are `[1, T, 5 * hidden]`
/// (5 target-layer captures concatenated on the last dim).
pub fn draft_hidden_byte_len(block_len: i32) -> usize {
    (block_len as usize) * (DFLASH35B_TARGET_HIDDEN as usize) * std::mem::size_of::<u16>()
}

#[allow(dead_code)]
pub struct DraftGreedyInputs<'a> {
    pub staged_first: i32,
    pub target_hidden: &'a Buffer,
    pub block_len: i32,
    pub suppress_token_mask: Option<&'a Buffer>,
    pub async_launch: bool,
}

/// Placeholder for `draft_greedy`. The real body needs the draft
/// forward pass (Qwen module dispatches + RoPE + sink/window KV
/// cache pack) plus lm_head projection + argmax — all of which need
/// the full-model forward that lands in `driver.rs`.
///
/// Kept as a standalone function with its own signature now so the
/// driver can call into it once the forward is ready.
#[allow(unused_variables)]
pub fn draft_greedy(
    dev: &Device,
    cmd: &CommandBuffer,
    caches: &mut [DraftKvCache],
    inputs: DraftGreedyInputs<'_>,
    out_token_ids: &mut Vec<i32>,
) -> bool {
    set_last_error(
        "draft_greedy: pending — requires the full Qwen3.5 draft forward wired up \
         by the runtime port (see runtime.py)",
    );
    false
}
