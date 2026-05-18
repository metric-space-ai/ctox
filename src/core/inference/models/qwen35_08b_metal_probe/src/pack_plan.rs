//! Metadata-only weight pack plan for the Qwen3.5-0.8B Metal path.

use std::collections::BTreeMap;

use crate::artifacts::TensorHeader;
use crate::model_shape::{LayerKind, ModelShape};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TensorClass {
    TokenEmbedding,
    LmHead,
    FinalNorm,
    LayerNorm,
    MlpGate,
    MlpUp,
    MlpDown,
    AttentionQ,
    AttentionK,
    AttentionV,
    AttentionO,
    DeltaQkv,
    DeltaZ,
    DeltaB,
    DeltaA,
    DeltaOut,
    DeltaStateParam,
    Other,
}

impl TensorClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TokenEmbedding => "token_embedding",
            Self::LmHead => "lm_head",
            Self::FinalNorm => "final_norm",
            Self::LayerNorm => "layer_norm",
            Self::MlpGate => "mlp_gate",
            Self::MlpUp => "mlp_up",
            Self::MlpDown => "mlp_down",
            Self::AttentionQ => "attention_q",
            Self::AttentionK => "attention_k",
            Self::AttentionV => "attention_v",
            Self::AttentionO => "attention_o",
            Self::DeltaQkv => "delta_qkv",
            Self::DeltaZ => "delta_z",
            Self::DeltaB => "delta_b",
            Self::DeltaA => "delta_a",
            Self::DeltaOut => "delta_out",
            Self::DeltaStateParam => "delta_state_param",
            Self::Other => "other",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "token_embedding" => Self::TokenEmbedding,
            "lm_head" => Self::LmHead,
            "final_norm" => Self::FinalNorm,
            "layer_norm" => Self::LayerNorm,
            "mlp_gate" => Self::MlpGate,
            "mlp_up" => Self::MlpUp,
            "mlp_down" => Self::MlpDown,
            "attention_q" => Self::AttentionQ,
            "attention_k" => Self::AttentionK,
            "attention_v" => Self::AttentionV,
            "attention_o" => Self::AttentionO,
            "delta_qkv" => Self::DeltaQkv,
            "delta_z" => Self::DeltaZ,
            "delta_b" => Self::DeltaB,
            "delta_a" => Self::DeltaA,
            "delta_out" => Self::DeltaOut,
            "delta_state_param" => Self::DeltaStateParam,
            "other" => Self::Other,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackLayout {
    Fp16RowTiled,
    Fp16Vector,
    Int8RowTiled,
    Int4GroupwiseRowTiled,
    RawState,
}

impl PackLayout {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fp16RowTiled => "fp16_row_tiled",
            Self::Fp16Vector => "fp16_vector",
            Self::Int8RowTiled => "int8_row_tiled",
            Self::Int4GroupwiseRowTiled => "int4_groupwise_row_tiled",
            Self::RawState => "raw_state",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "fp16_row_tiled" => Self::Fp16RowTiled,
            "fp16_vector" => Self::Fp16Vector,
            "int8_row_tiled" => Self::Int8RowTiled,
            "int4_groupwise_row_tiled" => Self::Int4GroupwiseRowTiled,
            "raw_state" => Self::RawState,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuantScheme {
    None,
    Int8Symmetric,
    Int4GroupwiseSymmetric,
}

impl QuantScheme {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Int8Symmetric => "int8_symmetric",
            Self::Int4GroupwiseSymmetric => "int4_groupwise_symmetric",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "int8_symmetric" => Self::Int8Symmetric,
            "int4_groupwise_symmetric" => Self::Int4GroupwiseSymmetric,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackPlan {
    pub entries: Vec<PackEntry>,
    pub class_summary: Vec<ClassSummary>,
    pub warnings: Vec<PackPlanWarning>,
    pub total_tensor_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackEntry {
    pub tensor: String,
    pub class: TensorClass,
    pub layer: Option<usize>,
    pub dtype: String,
    pub shape: Vec<usize>,
    pub bytes: u64,
    pub layout: PackLayout,
    pub row_tile: usize,
    pub col_tile: usize,
    pub quant_scheme: QuantScheme,
    pub quant_group_size: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassSummary {
    pub class: TensorClass,
    pub count: usize,
    pub bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackPlanWarning {
    pub blocking: bool,
    pub message: String,
}

impl PackPlan {
    pub fn from_tensors(shape: &ModelShape, tensors: &[TensorHeader]) -> Self {
        let mut entries = tensors
            .iter()
            .map(|tensor| {
                let class = classify_tensor(&tensor.name);
                let bytes = tensor.data_offsets[1].saturating_sub(tensor.data_offsets[0]);
                PackEntry {
                    tensor: tensor.name.clone(),
                    class,
                    layer: parse_layer_id(&tensor.name),
                    dtype: tensor.dtype.clone(),
                    shape: tensor.shape.clone(),
                    bytes,
                    layout: layout_for(class, &tensor.shape),
                    row_tile: row_tile_for(class),
                    col_tile: col_tile_for(class, &tensor.shape),
                    quant_scheme: quant_scheme_for(class),
                    quant_group_size: quant_group_size_for(class),
                }
            })
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| {
            a.layer
                .cmp(&b.layer)
                .then(a.class.cmp(&b.class))
                .then(a.tensor.cmp(&b.tensor))
        });

        let mut by_class = BTreeMap::<TensorClass, (usize, u64)>::new();
        let mut total_tensor_bytes = 0;
        for entry in &entries {
            total_tensor_bytes += entry.bytes;
            let summary = by_class.entry(entry.class).or_default();
            summary.0 += 1;
            summary.1 += entry.bytes;
        }
        let class_summary = by_class
            .into_iter()
            .map(|(class, (count, bytes))| ClassSummary {
                class,
                count,
                bytes,
            })
            .collect();
        let warnings = validate_plan(shape, &entries);

        Self {
            entries,
            class_summary,
            warnings,
            total_tensor_bytes,
        }
    }
}

pub fn classify_tensor(name: &str) -> TensorClass {
    if name.starts_with("mtp.") || name.starts_with("model.visual.") {
        return TensorClass::Other;
    }
    if name == "model.embed_tokens.weight" || name.ends_with(".embed_tokens.weight") {
        return TensorClass::TokenEmbedding;
    }
    if name == "lm_head.weight" || name.ends_with(".lm_head.weight") {
        return TensorClass::LmHead;
    }
    if name == "model.norm.weight"
        || name == "model.language_model.norm.weight"
        || name.ends_with(".final_layernorm.weight")
    {
        return TensorClass::FinalNorm;
    }
    if name.contains(".mlp.gate_proj.") {
        return TensorClass::MlpGate;
    }
    if name.contains(".mlp.up_proj.") {
        return TensorClass::MlpUp;
    }
    if name.contains(".mlp.down_proj.") {
        return TensorClass::MlpDown;
    }
    if name.contains(".self_attn.q_proj.") {
        return TensorClass::AttentionQ;
    }
    if name.contains(".self_attn.k_proj.") {
        return TensorClass::AttentionK;
    }
    if name.contains(".self_attn.v_proj.") {
        return TensorClass::AttentionV;
    }
    if name.contains(".self_attn.o_proj.") {
        return TensorClass::AttentionO;
    }
    if name.contains(".in_proj_qkv.") {
        return TensorClass::DeltaQkv;
    }
    if name.contains(".in_proj_z.") {
        return TensorClass::DeltaZ;
    }
    if name.contains(".in_proj_b.") {
        return TensorClass::DeltaB;
    }
    if name.contains(".in_proj_a.") {
        return TensorClass::DeltaA;
    }
    if name.contains(".out_proj.") && !name.contains(".self_attn.") {
        return TensorClass::DeltaOut;
    }
    if name.ends_with(".A_log")
        || name.ends_with(".dt_bias")
        || name.contains(".conv1d.")
        || name.contains(".mixer.norm.weight")
        || name.contains(".linear_attn.norm.weight")
        || name.contains(".deltanet.")
    {
        return TensorClass::DeltaStateParam;
    }
    if name.ends_with("layernorm.weight")
        || name.ends_with("layer_norm.weight")
        || name.ends_with(".norm.weight")
        || name.ends_with(".q_norm.weight")
        || name.ends_with(".k_norm.weight")
    {
        return TensorClass::LayerNorm;
    }
    TensorClass::Other
}

pub fn parse_layer_id(name: &str) -> Option<usize> {
    let marker = ".layers.";
    let start = name.find(marker)? + marker.len();
    let digits = name[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn layout_for(class: TensorClass, shape: &[usize]) -> PackLayout {
    if shape.len() == 2 {
        match quant_scheme_for(class) {
            QuantScheme::Int8Symmetric => return PackLayout::Int8RowTiled,
            QuantScheme::Int4GroupwiseSymmetric => return PackLayout::Int4GroupwiseRowTiled,
            QuantScheme::None => {}
        }
    }
    match class {
        TensorClass::DeltaStateParam => PackLayout::RawState,
        _ if shape.len() == 2 => PackLayout::Fp16RowTiled,
        _ => PackLayout::Fp16Vector,
    }
}

fn quant_scheme_for(class: TensorClass) -> QuantScheme {
    let Some(value) = std::env::var("CTOX_QWEN35_PACK_QUANT").ok() else {
        return QuantScheme::None;
    };
    let scheme = match value.as_str() {
        "int8" | "int8_symmetric" => QuantScheme::Int8Symmetric,
        "int4" | "q4" | "int4_groupwise_symmetric" => QuantScheme::Int4GroupwiseSymmetric,
        _ => QuantScheme::None,
    };
    if quantizable_class(class) {
        scheme
    } else {
        QuantScheme::None
    }
}

const fn quantizable_class(class: TensorClass) -> bool {
    matches!(
        class,
        TensorClass::LmHead
            | TensorClass::TokenEmbedding
            | TensorClass::MlpGate
            | TensorClass::MlpUp
            | TensorClass::MlpDown
            | TensorClass::AttentionQ
            | TensorClass::AttentionK
            | TensorClass::AttentionV
            | TensorClass::AttentionO
            | TensorClass::DeltaQkv
            | TensorClass::DeltaZ
            | TensorClass::DeltaB
            | TensorClass::DeltaA
            | TensorClass::DeltaOut
    )
}

fn quant_group_size_for(class: TensorClass) -> usize {
    if !quantizable_class(class) {
        return 0;
    }
    match quant_scheme_for(class) {
        QuantScheme::None => 0,
        QuantScheme::Int8Symmetric => std::env::var("CTOX_QWEN35_PACK_QUANT_GROUP")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| matches!(*value, 32 | 64 | 128 | 256))
            .unwrap_or(256),
        QuantScheme::Int4GroupwiseSymmetric => std::env::var("CTOX_QWEN35_PACK_QUANT_GROUP")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| matches!(*value, 32 | 64 | 128))
            .unwrap_or(64),
    }
}

fn row_tile_for(class: TensorClass) -> usize {
    if let Some(tile) = env_usize("CTOX_QWEN35_PACK_ROW_TILE") {
        return tile.clamp(1, 8);
    }
    match class {
        TensorClass::LmHead | TensorClass::TokenEmbedding => 8,
        TensorClass::MlpGate | TensorClass::MlpUp | TensorClass::MlpDown => 8,
        TensorClass::AttentionQ
        | TensorClass::AttentionK
        | TensorClass::AttentionV
        | TensorClass::AttentionO
        | TensorClass::DeltaQkv
        | TensorClass::DeltaZ
        | TensorClass::DeltaB
        | TensorClass::DeltaA
        | TensorClass::DeltaOut => 8,
        _ => 8,
    }
}

fn col_tile_for(class: TensorClass, shape: &[usize]) -> usize {
    if shape.len() != 2 {
        return 0;
    }
    if let Some(tile) = env_usize("CTOX_QWEN35_PACK_COL_TILE") {
        return tile.max(1);
    }
    match class {
        TensorClass::LmHead | TensorClass::TokenEmbedding => 256,
        TensorClass::MlpGate | TensorClass::MlpUp | TensorClass::MlpDown => 256,
        TensorClass::AttentionQ
        | TensorClass::AttentionK
        | TensorClass::AttentionV
        | TensorClass::AttentionO
        | TensorClass::DeltaQkv
        | TensorClass::DeltaZ
        | TensorClass::DeltaB
        | TensorClass::DeltaA
        | TensorClass::DeltaOut => 256,
        _ => 256,
    }
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
}

fn validate_plan(shape: &ModelShape, entries: &[PackEntry]) -> Vec<PackPlanWarning> {
    let mut warnings = Vec::new();
    require_global(
        &mut warnings,
        entries,
        TensorClass::TokenEmbedding,
        "missing token embedding tensor",
        true,
    );
    if !has_class(entries, TensorClass::LmHead) {
        warnings.push(PackPlanWarning {
            blocking: false,
            message: "missing lm_head.weight; assuming tied embedding is allowed".to_owned(),
        });
    }
    require_global(
        &mut warnings,
        entries,
        TensorClass::FinalNorm,
        "missing final norm tensor",
        true,
    );

    for layer in 0..shape.n_layers {
        for marker in ["input_layernorm.weight", "post_attention_layernorm.weight"] {
            require_layer_norm(&mut warnings, entries, layer, marker, true);
        }
        require_layer(
            &mut warnings,
            entries,
            layer,
            TensorClass::MlpGate,
            "missing MLP gate projection",
            true,
        );
        require_layer(
            &mut warnings,
            entries,
            layer,
            TensorClass::MlpUp,
            "missing MLP up projection",
            true,
        );
        require_layer(
            &mut warnings,
            entries,
            layer,
            TensorClass::MlpDown,
            "missing MLP down projection",
            true,
        );

        match shape.layer_kind(layer) {
            LayerKind::FullAttention => {
                for class in [
                    TensorClass::AttentionQ,
                    TensorClass::AttentionK,
                    TensorClass::AttentionV,
                    TensorClass::AttentionO,
                ] {
                    require_layer(
                        &mut warnings,
                        entries,
                        layer,
                        class,
                        "missing attention projection",
                        true,
                    );
                }
            }
            LayerKind::GatedDeltaNet => {
                for class in [
                    TensorClass::DeltaQkv,
                    TensorClass::DeltaZ,
                    TensorClass::DeltaB,
                    TensorClass::DeltaA,
                    TensorClass::DeltaOut,
                ] {
                    require_layer(
                        &mut warnings,
                        entries,
                        layer,
                        class,
                        "missing DeltaNet projection",
                        true,
                    );
                }
                for marker in ["A_log", "dt_bias", "conv1d.weight", "norm.weight"] {
                    require_delta_state_param(&mut warnings, entries, layer, marker, true);
                }
                require_delta_state_param(&mut warnings, entries, layer, "conv1d.bias", false);
            }
        }
    }

    warnings
}

fn require_global(
    warnings: &mut Vec<PackPlanWarning>,
    entries: &[PackEntry],
    class: TensorClass,
    message: &str,
    blocking: bool,
) {
    if !has_class(entries, class) {
        warnings.push(PackPlanWarning {
            blocking,
            message: message.to_owned(),
        });
    }
}

fn require_layer(
    warnings: &mut Vec<PackPlanWarning>,
    entries: &[PackEntry],
    layer: usize,
    class: TensorClass,
    message: &str,
    blocking: bool,
) {
    if !entries
        .iter()
        .any(|entry| entry.layer == Some(layer) && entry.class == class)
    {
        warnings.push(PackPlanWarning {
            blocking,
            message: format!("layer {layer}: {message} ({})", class.as_str()),
        });
    }
}

fn require_delta_state_param(
    warnings: &mut Vec<PackPlanWarning>,
    entries: &[PackEntry],
    layer: usize,
    marker: &str,
    blocking: bool,
) {
    if !entries.iter().any(|entry| {
        entry.layer == Some(layer)
            && entry.class == TensorClass::DeltaStateParam
            && entry.tensor.contains(marker)
    }) {
        warnings.push(PackPlanWarning {
            blocking,
            message: format!("layer {layer}: missing DeltaNet state param `{marker}`"),
        });
    }
}

fn require_layer_norm(
    warnings: &mut Vec<PackPlanWarning>,
    entries: &[PackEntry],
    layer: usize,
    marker: &str,
    blocking: bool,
) {
    if !entries.iter().any(|entry| {
        entry.layer == Some(layer)
            && entry.class == TensorClass::LayerNorm
            && entry.tensor.contains(marker)
    }) {
        warnings.push(PackPlanWarning {
            blocking,
            message: format!("layer {layer}: missing layer norm `{marker}`"),
        });
    }
}

fn has_class(entries: &[PackEntry], class: TensorClass) -> bool {
    entries.iter().any(|entry| entry.class == class)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_qwen_tensor_names() {
        assert_eq!(
            classify_tensor("model.embed_tokens.weight"),
            TensorClass::TokenEmbedding
        );
        assert_eq!(
            classify_tensor("model.layers.3.self_attn.q_proj.weight"),
            TensorClass::AttentionQ
        );
        assert_eq!(
            classify_tensor("model.layers.0.mlp.down_proj.weight"),
            TensorClass::MlpDown
        );
        assert_eq!(
            classify_tensor("model.layers.0.in_proj_qkv.weight"),
            TensorClass::DeltaQkv
        );
        assert_eq!(
            classify_tensor("model.layers.0.linear_attn.norm.weight"),
            TensorClass::DeltaStateParam
        );
        assert_eq!(
            classify_tensor("model.language_model.norm.weight"),
            TensorClass::FinalNorm
        );
        assert_eq!(
            classify_tensor("mtp.layers.0.mlp.down_proj.weight"),
            TensorClass::Other
        );
        assert_eq!(
            parse_layer_id("model.layers.23.mlp.up_proj.weight"),
            Some(23)
        );
    }

    #[test]
    fn parses_static_quant_layout_names() {
        assert_eq!(
            PackLayout::from_str("int8_row_tiled"),
            Some(PackLayout::Int8RowTiled)
        );
        assert_eq!(
            PackLayout::from_str("int4_groupwise_row_tiled"),
            Some(PackLayout::Int4GroupwiseRowTiled)
        );
        assert_eq!(
            QuantScheme::from_str("int8_symmetric"),
            Some(QuantScheme::Int8Symmetric)
        );
        assert_eq!(
            QuantScheme::from_str("int4_groupwise_symmetric"),
            Some(QuantScheme::Int4GroupwiseSymmetric)
        );
    }
}
