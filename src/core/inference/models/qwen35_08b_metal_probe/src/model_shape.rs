//! Fixed Qwen3.5-0.8B text-model shape contract.

pub const QWEN35_08B_CANONICAL_MODEL: &str = "Qwen/Qwen3.5-0.8B";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerKind {
    GatedDeltaNet,
    FullAttention,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelShape {
    pub model: &'static str,
    pub parameter_count: u64,
    pub hidden_size: usize,
    pub vocab_size: usize,
    pub n_layers: usize,
    pub ffn_intermediate: usize,
    pub attention_q_heads: usize,
    pub attention_kv_heads: usize,
    pub attention_head_dim: usize,
    pub attention_rope_dim: usize,
    pub deltanet_qk_heads: usize,
    pub deltanet_v_heads: usize,
    pub deltanet_head_dim: usize,
    pub native_context: usize,
}

impl ModelShape {
    pub const fn layer_kind(&self, layer: usize) -> LayerKind {
        match layer % 4 {
            3 => LayerKind::FullAttention,
            _ => LayerKind::GatedDeltaNet,
        }
    }

    pub const fn n_deltanet_layers(&self) -> usize {
        self.n_layers / 4 * 3
    }

    pub const fn n_full_attention_layers(&self) -> usize {
        self.n_layers / 4
    }

    pub const fn lm_head_fp16_bytes(&self) -> usize {
        self.hidden_size * self.vocab_size * 2
    }

    pub const fn approximate_fp16_weight_bytes(&self) -> u64 {
        self.parameter_count * 2
    }

    pub const fn attention_q_width(&self) -> usize {
        self.attention_q_heads * self.attention_head_dim
    }

    pub const fn attention_kv_width(&self) -> usize {
        self.attention_kv_heads * self.attention_head_dim
    }

    pub const fn attention_q_with_head_gate_width(&self) -> usize {
        self.attention_q_width() * 2
    }

    pub const fn deltanet_width(&self) -> usize {
        self.deltanet_v_heads * self.deltanet_head_dim
    }

    pub const fn deltanet_qkv_width(&self) -> usize {
        self.deltanet_width() * 3
    }
}

pub const QWEN35_08B: ModelShape = ModelShape {
    model: QWEN35_08B_CANONICAL_MODEL,
    parameter_count: 800_000_000,
    hidden_size: 1024,
    vocab_size: 248_320,
    n_layers: 24,
    ffn_intermediate: 3584,
    attention_q_heads: 8,
    attention_kv_heads: 2,
    attention_head_dim: 256,
    attention_rope_dim: 64,
    deltanet_qk_heads: 16,
    deltanet_v_heads: 16,
    deltanet_head_dim: 128,
    native_context: 262_144,
};

pub const QWEN35_08B_LAYER_PATTERN: [LayerKind; 24] = [
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::FullAttention,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::FullAttention,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::FullAttention,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::FullAttention,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::FullAttention,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::GatedDeltaNet,
    LayerKind::FullAttention,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_pattern_is_three_delta_one_attention() {
        for layer in 0..QWEN35_08B.n_layers {
            assert_eq!(
                QWEN35_08B.layer_kind(layer),
                QWEN35_08B_LAYER_PATTERN[layer]
            );
        }
        assert_eq!(QWEN35_08B.n_deltanet_layers(), 18);
        assert_eq!(QWEN35_08B.n_full_attention_layers(), 6);
    }

    #[test]
    fn lm_head_fp16_size_matches_expected_order_of_magnitude() {
        assert_eq!(QWEN35_08B.lm_head_fp16_bytes(), 508_559_360);
        assert_eq!(QWEN35_08B.approximate_fp16_weight_bytes(), 1_600_000_000);
    }

    #[test]
    fn projection_width_helpers_match_qwen35_08b_contract() {
        assert_eq!(QWEN35_08B.attention_q_width(), 2048);
        assert_eq!(QWEN35_08B.attention_kv_width(), 512);
        assert_eq!(QWEN35_08B.attention_q_with_head_gate_width(), 4096);
        assert_eq!(QWEN35_08B.deltanet_width(), 2048);
        assert_eq!(QWEN35_08B.deltanet_qkv_width(), 6144);
    }
}
