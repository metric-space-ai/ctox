// Origin: CTOX
// License: Apache-2.0
//
// ref: vendor/upstream-config/Qwen3.6-35B-A3B.config.json (2026-05-08)
//
// Frozen kernel ABI for the Qwen3.6-35B-A3B text decoder. These constants
// are the canonical shape contract every Metal kernel in this crate is
// allowed to specialize on — change them only when the vendored config
// snapshot is refreshed in the same commit.

use std::fmt;

/// Per-layer attention kind. Order in `LAYER_TYPES` matches the
/// `text_config.layer_types` array in the vendored config snapshot.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// Standard softmax attention with M-RoPE, GQA, and a gated O
    /// projection. ~1 of every 4 layers (`full_attention_interval=4`).
    /// This is the "non-dflash" path stage 1 targets.
    FullAttention,
    /// Linear-attention block with a short causal conv1d (kernel=4),
    /// fp32 SSM state, and a separate K/V head pair (16/32 heads,
    /// head_dim=128). This is the "dflash" path — explicitly deferred
    /// out of stage 1 per user contract.
    LinearAttention,
}

/// Frozen kernel-ABI struct for the text decoder. Numeric fields are
/// `usize` so they compose directly into kernel launch dims; `f32` is
/// used for floating-point hyperparameters that flow into kernel
/// constants.
#[derive(Clone, Debug)]
pub struct Qwen36MoeTextConfig {
    /// `text_config.hidden_size` — embedding/residual width.
    pub hidden_size: usize,
    /// `text_config.num_hidden_layers` — total decoder layers
    /// (full + linear interleaved).
    pub num_hidden_layers: usize,
    /// `text_config.layer_types` — per-layer attention kind.
    pub layer_types: &'static [LayerKind],
    /// Length-`num_hidden_layers` vector. `Some(idx)` means the layer is
    /// the `idx`-th full-attention layer; `None` means it is a
    /// linear-attention layer. Computed once at compile time so kernels
    /// can index full-only buffers without scanning the type array.
    pub full_attention_layer_index: &'static [Option<usize>],
    /// `text_config.full_attention_interval` — repeat period of the
    /// hybrid layer pattern.
    pub full_attention_interval: usize,
    /// `text_config.num_attention_heads` — Q heads in a full-attention
    /// layer.
    pub num_attention_heads: usize,
    /// `text_config.num_key_value_heads` — KV heads in a full-attention
    /// layer (GQA group size = num_attention_heads / num_key_value_heads).
    pub num_key_value_heads: usize,
    /// `text_config.head_dim` — per-head projection dim. Same for Q, K,
    /// V in the full-attention layer; the linear-attention layer uses
    /// its own head dims (see linear_* fields).
    pub head_dim: usize,
    /// `text_config.attn_output_gate` — when true, the attention output
    /// is multiplied by a learned sigmoid gate before the O projection.
    /// Adds a fused gate kernel to the attention block.
    pub attn_output_gate: bool,
    /// `text_config.partial_rotary_factor` — fraction of `head_dim`
    /// covered by RoPE. The remaining suffix is left untouched.
    pub partial_rotary_factor: f32,
    /// `text_config.rope_parameters.rope_theta` — RoPE base.
    pub rope_theta: f64,
    /// `text_config.rope_parameters.mrope_interleaved` — when true the
    /// M-RoPE position axes are interleaved across the rotated head dim
    /// rather than concatenated.
    pub mrope_interleaved: bool,
    /// `text_config.rope_parameters.mrope_section` — split of the
    /// rotated head dim into [text, spatial-x, spatial-y(/temporal)].
    pub mrope_section: [usize; 3],
    /// `text_config.rms_norm_eps`.
    pub rms_norm_eps: f32,
    /// `text_config.linear_num_key_heads` (for the deferred dflash path).
    pub linear_num_key_heads: usize,
    /// `text_config.linear_num_value_heads` (for the deferred dflash path).
    pub linear_num_value_heads: usize,
    /// `text_config.linear_key_head_dim` (for the deferred dflash path).
    pub linear_key_head_dim: usize,
    /// `text_config.linear_value_head_dim` (for the deferred dflash path).
    pub linear_value_head_dim: usize,
    /// `text_config.linear_conv_kernel_dim` (for the deferred dflash path).
    pub linear_conv_kernel_dim: usize,
    /// `text_config.num_experts` — total routed experts per layer.
    pub num_experts: usize,
    /// `text_config.num_experts_per_tok` — top-k experts selected per token.
    pub num_experts_per_tok: usize,
    /// `text_config.moe_intermediate_size` — per-expert SwiGLU intermediate.
    pub moe_intermediate_size: usize,
    /// `text_config.shared_expert_intermediate_size` — width of the
    /// shared (always-on) expert that runs alongside the routed top-k.
    pub shared_expert_intermediate_size: usize,
    /// `text_config.vocab_size`.
    pub vocab_size: usize,
    /// `text_config.tie_word_embeddings` — when false the LM head has
    /// its own weight tensor (Qwen3.6-35B-A3B = false).
    pub tie_word_embeddings: bool,
    /// `text_config.max_position_embeddings`.
    pub max_position_embeddings: usize,
    /// `text_config.bos_token_id`.
    pub bos_token_id: u32,
    /// `text_config.eos_token_id`.
    pub eos_token_id: u32,
    /// `text_config.mtp_num_hidden_layers` — multi-token-prediction
    /// head depth. Stage 1 ignores the MTP head; recorded here so the
    /// loader can validate weight presence.
    pub mtp_num_hidden_layers: usize,
}

impl Qwen36MoeTextConfig {
    /// Q hidden width in the full-attention layer (Q heads × head_dim).
    pub const fn full_attn_q_hidden(&self) -> usize {
        self.num_attention_heads * self.head_dim
    }

    /// KV hidden width in the full-attention layer (KV heads × head_dim).
    pub const fn full_attn_kv_hidden(&self) -> usize {
        self.num_key_value_heads * self.head_dim
    }

    /// GQA group size: how many Q heads share one KV head.
    pub const fn full_attn_gqa_group(&self) -> usize {
        self.num_attention_heads / self.num_key_value_heads
    }

    /// Number of head_dim lanes covered by RoPE rotation.
    pub fn rope_rotated_dim(&self) -> usize {
        ((self.head_dim as f32) * self.partial_rotary_factor) as usize
    }

    /// Count of `FullAttention` entries in `layer_types`.
    pub const fn num_full_attention_layers(&self) -> usize {
        let mut i = 0;
        let mut count = 0;
        while i < self.layer_types.len() {
            if matches!(self.layer_types[i], LayerKind::FullAttention) {
                count += 1;
            }
            i += 1;
        }
        count
    }

    /// Count of `LinearAttention` entries in `layer_types`.
    pub const fn num_linear_attention_layers(&self) -> usize {
        self.num_hidden_layers - self.num_full_attention_layers()
    }
}

impl fmt::Display for Qwen36MoeTextConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Qwen3.6-35B-A3B text decoder: {} layers ({} full + {} linear), \
             hidden={}, full-attn heads={}/{} (head_dim={}), \
             experts={} top-{} (moe_intermediate={}, shared={}), vocab={}",
            self.num_hidden_layers,
            self.num_full_attention_layers(),
            self.num_linear_attention_layers(),
            self.hidden_size,
            self.num_attention_heads,
            self.num_key_value_heads,
            self.head_dim,
            self.num_experts,
            self.num_experts_per_tok,
            self.moe_intermediate_size,
            self.shared_expert_intermediate_size,
            self.vocab_size,
        )
    }
}

const fn build_layer_types() -> [LayerKind; 40] {
    // ref: vendor/upstream-config/Qwen3.6-35B-A3B.config.json
    //      text_config.layer_types — 40 entries, repeating
    //      [linear, linear, linear, full] × 10.
    let mut out = [LayerKind::LinearAttention; 40];
    let mut i = 0;
    while i < 40 {
        if (i + 1) % 4 == 0 {
            out[i] = LayerKind::FullAttention;
        }
        i += 1;
    }
    out
}

const LAYER_TYPES: [LayerKind; 40] = build_layer_types();

const fn build_full_attention_layer_index() -> [Option<usize>; 40] {
    let mut out = [None; 40];
    let mut i = 0;
    let mut full_idx = 0;
    while i < 40 {
        if matches!(LAYER_TYPES[i], LayerKind::FullAttention) {
            out[i] = Some(full_idx);
            full_idx += 1;
        }
        i += 1;
    }
    out
}

const FULL_ATTENTION_LAYER_INDEX: [Option<usize>; 40] = build_full_attention_layer_index();

/// Frozen kernel ABI for the official `Qwen/Qwen3.6-35B-A3B` revision
/// captured in `vendor/upstream-config/`.
pub const QWEN36_35B_A3B_TEXT_CONFIG: Qwen36MoeTextConfig = Qwen36MoeTextConfig {
    hidden_size: 2048,
    num_hidden_layers: 40,
    layer_types: &LAYER_TYPES,
    full_attention_layer_index: &FULL_ATTENTION_LAYER_INDEX,
    full_attention_interval: 4,
    num_attention_heads: 16,
    num_key_value_heads: 2,
    head_dim: 256,
    attn_output_gate: true,
    partial_rotary_factor: 0.25,
    rope_theta: 10_000_000.0,
    mrope_interleaved: true,
    mrope_section: [11, 11, 10],
    rms_norm_eps: 1e-6,
    linear_num_key_heads: 16,
    linear_num_value_heads: 32,
    linear_key_head_dim: 128,
    linear_value_head_dim: 128,
    linear_conv_kernel_dim: 4,
    num_experts: 256,
    num_experts_per_tok: 8,
    moe_intermediate_size: 512,
    shared_expert_intermediate_size: 512,
    vocab_size: 248_320,
    tie_word_embeddings: false,
    max_position_embeddings: 262_144,
    bos_token_id: 248_044,
    eos_token_id: 248_044,
    mtp_num_hidden_layers: 1,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_config_is_internally_consistent() {
        let c = &QWEN36_35B_A3B_TEXT_CONFIG;
        assert_eq!(c.layer_types.len(), c.num_hidden_layers);
        assert_eq!(c.full_attention_layer_index.len(), c.num_hidden_layers);
        assert_eq!(c.num_full_attention_layers(), 10);
        assert_eq!(c.num_linear_attention_layers(), 30);
        assert_eq!(c.full_attn_q_hidden(), 16 * 256);
        assert_eq!(c.full_attn_kv_hidden(), 2 * 256);
        assert_eq!(c.full_attn_gqa_group(), 8);
        // partial_rotary_factor=0.25 of head_dim=256 gives 64 rotated lanes.
        assert_eq!(c.rope_rotated_dim(), 64);
        // The 3 M-RoPE sections cover the rotated half (rope_rotated_dim/2);
        // the full rotated lanes pair (cos, sin) so the sum × 2 equals
        // rope_rotated_dim. Here 11+11+10 = 32 → 64 rotated lanes.
        assert_eq!(c.mrope_section.iter().sum::<usize>() * 2, c.rope_rotated_dim());
    }

    #[test]
    fn full_attention_layers_match_interval_pattern() {
        let c = &QWEN36_35B_A3B_TEXT_CONFIG;
        for (i, kind) in c.layer_types.iter().enumerate() {
            let want_full = (i + 1) % c.full_attention_interval == 0;
            assert_eq!(
                matches!(kind, LayerKind::FullAttention),
                want_full,
                "layer {i} kind"
            );
        }
    }

    #[test]
    fn full_attention_layer_index_is_compact_and_dense() {
        let c = &QWEN36_35B_A3B_TEXT_CONFIG;
        let mut seen = 0usize;
        for (i, slot) in c.full_attention_layer_index.iter().enumerate() {
            match (c.layer_types[i], slot) {
                (LayerKind::FullAttention, Some(idx)) => {
                    assert_eq!(*idx, seen, "full layer {i} maps to compact idx {seen}");
                    seen += 1;
                }
                (LayerKind::LinearAttention, None) => {}
                other => panic!("inconsistent layer index entry at {i}: {other:?}"),
            }
        }
        assert_eq!(seen, c.num_full_attention_layers());
    }
}
