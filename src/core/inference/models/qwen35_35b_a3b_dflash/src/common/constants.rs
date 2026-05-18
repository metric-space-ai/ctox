//! Qwen3.5-35B-A3B (MoE) + z-lab DFlash draft model constants.
//!
//! "A3B" = roughly 3 B active parameters per forward step out of ~35 B
//! total, via an MoE router that picks top-K experts per token. The
//! text stack has 40 layers with a 3:1 linear-attention/full-attention
//! pattern. The HF bundle also contains a Qwen3.5 vision tower.
//!
//! ref: `mlx-community/Qwen3.5-35B-A3B-4bit` HF `config.json`
//!      `dflash_mlx/runtime.py` (the same runtime handles MoE targets
//!       via the generic hybrid detection path).

// ─── Target model constants ─────────────────────────────────────────

pub const DFLASH35B_TARGET_HIDDEN: i32 = 2048;
pub const DFLASH35B_TARGET_LAYERS: i32 = 40;

/// Dense-attention shape from the Qwen3.5-35B-A3B text config.
pub const DFLASH35B_TARGET_N_HEADS: i32 = 16;
pub const DFLASH35B_TARGET_N_KV_HEADS: i32 = 2;
pub const DFLASH35B_TARGET_HEAD_DIM: i32 = 256;

/// MoE hyperparameters (per expert, not the fused MLP block's dim).
///   * `NUM_EXPERTS`        total experts routed over per-layer
///   * `EXPERTS_PER_TOK`    top-K routed per token
///   * `MOE_INTERMEDIATE`   per-expert MLP intermediate dim
pub const DFLASH35B_NUM_EXPERTS: i32 = 256;
pub const DFLASH35B_EXPERTS_PER_TOK: i32 = 8;
pub const DFLASH35B_MOE_INTERMEDIATE: i32 = 512;

/// Non-MoE intermediate (used by the shared experts path, when the
/// model has any; 0 for "pure" A3B).
pub const DFLASH35B_SHARED_INTERMEDIATE: i32 = 512;

pub const DFLASH35B_TARGET_VOCAB: i32 = 248320;
pub const DFLASH35B_ROPE_THETA: f32 = 10_000_000.0;
pub const DFLASH35B_RMS_EPS: f32 = 1e-6;

// ─── Draft model constants ──────────────────────────────────────────
//
// The z-lab DFlash draft for Qwen3.5-35B-A3B ships under
// `z-lab/Qwen3.5-35B-A3B-DFlash`. Same block-diffusion protocol as the
// 27B draft, but the draft transformer itself has 8 layers. It consumes
// 5 captured target layers through the fc projection below.

pub const DFLASH35B_DRAFT_LAYERS: i32 = 8;
pub const DFLASH35B_DRAFT_BLOCK_SIZE: i32 = 16;
pub const DFLASH35B_DRAFT_N_TARGET_LAYERS: i32 = 5;
pub const DFLASH35B_DRAFT_MASK_TOKEN_ID: i32 = 248_070;

/// target_layer_ids from `z-lab/Qwen3.5-35B-A3B-DFlash/config.json`.
pub const DFLASH35B_DRAFT_TARGET_LAYER_IDS: [i32; 5] = [1, 10, 19, 28, 37];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_byte_match() {
        assert_eq!(DFLASH35B_TARGET_HIDDEN, 2048);
        assert_eq!(DFLASH35B_TARGET_LAYERS, 40);
        assert_eq!(DFLASH35B_TARGET_N_HEADS, 16);
        assert_eq!(DFLASH35B_TARGET_N_KV_HEADS, 2);
        assert_eq!(DFLASH35B_TARGET_HEAD_DIM, 256);
        assert_eq!(DFLASH35B_NUM_EXPERTS, 256);
        assert_eq!(DFLASH35B_EXPERTS_PER_TOK, 8);
        assert_eq!(DFLASH35B_MOE_INTERMEDIATE, 512);
        assert_eq!(DFLASH35B_TARGET_VOCAB, 248_320);
        assert!((DFLASH35B_ROPE_THETA - 10_000_000.0).abs() < 1e-3);
        assert!((DFLASH35B_RMS_EPS - 1e-6).abs() < 1e-12);
        assert_eq!(DFLASH35B_DRAFT_LAYERS, 8);
        assert_eq!(DFLASH35B_DRAFT_BLOCK_SIZE, 16);
        assert_eq!(DFLASH35B_DRAFT_N_TARGET_LAYERS, 5);
        assert_eq!(DFLASH35B_DRAFT_MASK_TOKEN_ID, 248_070);
        assert_eq!(DFLASH35B_DRAFT_TARGET_LAYER_IDS, [1, 10, 19, 28, 37]);
    }
}
