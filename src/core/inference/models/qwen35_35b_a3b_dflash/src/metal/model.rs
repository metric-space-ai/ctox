//! `TargetWeights` / `DraftWeights` / `TargetCache` for the Metal
//! backend. Holds every `MTLBuffer` handle the forward pass needs,
//! built once by the loader and then threaded through the driver.
//!
//! Role-analogous to `cuda::model` on the Linux side — same struct
//! names, same field semantics, only the buffer type differs
//! (`Buffer` instead of `*mut ggml_tensor`).
//!
//! ref: `dflash_mlx/model.py`, `dflash_mlx/runtime.py` §§1-600,
//!      `mlx_lm/models/qwen3.py`.

use crate::metal::ffi::Buffer;
use crate::metal::moe::MoeBlock;
use crate::metal::qwen::{
    Attention, Bf16Attention, Bf16Linear, Bf16Mlp, GatedDeltaNet, KvCache, Linear4Bit, RmsNorm,
    Rope,
};
use crate::metal::vision::VisionWeights;

// ─── TargetLayer ────────────────────────────────────────────────────
//
// Qwen3.5-35B-A3B hybrid: each of the 64 layers is either a full-attention
// block (every 4th layer, starting at (il+1) % 4 == 0) or a
// GatedDeltaNet block. The two variants carry different sub-modules,
// so we use an enum.

pub enum TargetLayer {
    FullAttention {
        attn_norm: RmsNorm,
        attn_post_norm: RmsNorm,
        ffn_norm: RmsNorm,
        attention: Attention,
        mlp: MoeBlock,
    },
    GatedDelta {
        attn_norm: RmsNorm,
        attn_post_norm: RmsNorm,
        ffn_norm: RmsNorm,
        delta: GatedDeltaNet,
        mlp: MoeBlock,
    },
}

// ─── TargetWeights ──────────────────────────────────────────────────

pub struct TargetWeights {
    pub tok_embed: Linear4Bit, // packed-4bit embedding table
    pub vision: Option<VisionWeights>,
    pub layers: Vec<TargetLayer>,
    pub out_norm: RmsNorm,
    pub output: Linear4Bit, // lm_head, also packed 4-bit for the 35B-A3B-4bit variant

    // Architecture constants pulled from HF config.json during load.
    pub full_attention_interval: i32,
    pub rope_sections: [i32; 4],
    pub n_embd_head_k: i32,
    pub n_embd_head_v: i32,
    pub n_head: i32,
    pub n_head_kv: i32,
    pub n_layer: i32,
    pub n_embd: i32,
    pub n_ff: i32,
    pub ssm_d_conv: i32,
    pub ssm_d_inner: i32,
    pub ssm_d_state: i32,
    pub ssm_dt_rank: i32,
    pub ssm_n_group: i32,

    pub rope: Rope,
}

// ─── DraftLayer / DraftWeights ──────────────────────────────────────

pub struct DraftLayer {
    pub attn_norm: RmsNorm,
    pub ffn_norm: RmsNorm,
    pub attention: Bf16Attention,
    pub mlp: Bf16Mlp,
}

pub struct DraftWeights {
    pub fc: Bf16Linear,
    pub hidden_norm: RmsNorm,
    pub layers: Vec<DraftLayer>,
    pub out_norm: RmsNorm,
}

// ─── TargetCache ────────────────────────────────────────────────────
//
// Per-layer state that persists across decode steps:
//
//   * KV cache for every full-attention layer (10 of 40).
//   * SSM state + conv state for every linear-attention layer (30 of 40).
//   * Snapshot buffers for fast-rollback.
//   * Rolling target_feat buffer — the bf16 stack of the 5 captured
//     layer outputs, consumed by the draft's `fc` projection.

pub struct TargetCache {
    pub max_ctx: i32,
    pub cur_pos: i32,

    /// KV caches — one slot per FULL-attention layer, in layer-index order.
    pub attn_kv: Vec<KvCache>,

    /// GatedDeltaNet recurrent state — one slot per delta-net layer.
    ///   ssm_state: [S_v, S_v, H_v] bf16 per element
    ///   conv_state: [(kernel-1), conv_channels] bf16 per element
    pub ssm_state: Vec<Buffer>,
    pub conv_state: Vec<Buffer>,

    /// Snapshot buffers for rollback (same sizes as above).
    pub ssm_state_snap: Vec<Buffer>,
    pub conv_state_snap: Vec<Buffer>,

    /// Per-step innovation tape + conv input captured during verify.
    pub ssm_intermediate: Vec<Buffer>,
    pub conv_input_cache: Vec<Buffer>,

    /// Rolling target-feature buffer fed into the draft's `fc` projection.
    /// Shape: [5 * hidden, target_feat_cap] bf16.
    pub target_feat: Buffer,
    pub target_feat_cap: i32,
}
