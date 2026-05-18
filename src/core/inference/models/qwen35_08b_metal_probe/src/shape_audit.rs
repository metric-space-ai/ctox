//! Shape audit for the current Metal kernels versus Qwen3.5 tensor contracts.

use crate::{LayerKind, MetalPack, PackLayout, TensorClass, QWEN35_08B};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeAuditStatus {
    Supported,
    KernelPlaceholder,
    Missing,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShapeAuditRow {
    pub layer: Option<usize>,
    pub class: TensorClass,
    pub expected_shapes: Vec<Vec<usize>>,
    pub kernel_shapes: Vec<Vec<usize>>,
    pub actual_shape: Option<Vec<usize>>,
    pub status: ShapeAuditStatus,
    pub note: String,
}

pub fn audit_shape_contract(pack: Option<&MetalPack>) -> Vec<ShapeAuditRow> {
    let mut rows = Vec::new();
    rows.push(global_vocab_row(TensorClass::TokenEmbedding, pack));
    rows.push(global_vocab_row(TensorClass::LmHead, pack));

    for layer in 0..QWEN35_08B.n_layers {
        rows.extend(ffn_rows(layer, pack));
        match QWEN35_08B.layer_kind(layer) {
            LayerKind::GatedDeltaNet => rows.extend(delta_rows(layer, pack)),
            LayerKind::FullAttention => rows.extend(attention_rows(layer, pack)),
        }
    }
    rows
}

fn global_vocab_row(class: TensorClass, pack: Option<&MetalPack>) -> ShapeAuditRow {
    let actual_shape = pack.and_then(|pack| {
        pack.find_first_class(class)
            .or_else(|| {
                if class == TensorClass::LmHead {
                    pack.find_first_class(TensorClass::TokenEmbedding)
                } else {
                    None
                }
            })
            .map(|entry| entry.source_shape.clone())
    });
    let expected_shapes = vec![vec![QWEN35_08B.vocab_size, QWEN35_08B.hidden_size]];
    let kernel_shapes = vec![vec![0, QWEN35_08B.hidden_size]];
    let status = match actual_shape.as_ref() {
        None => ShapeAuditStatus::Missing,
        Some(shape)
            if shape.len() == 2
                && shape[1] == QWEN35_08B.hidden_size
                && shape[0] <= QWEN35_08B.vocab_size =>
        {
            ShapeAuditStatus::Supported
        }
        Some(_) => ShapeAuditStatus::Unsupported,
    };
    ShapeAuditRow {
        layer: None,
        class,
        expected_shapes,
        kernel_shapes,
        actual_shape,
        status,
        note: match class {
            TensorClass::TokenEmbedding => {
                "embedding gather supports [rows, hidden] with tiled K=1024".to_owned()
            }
            TensorClass::LmHead => {
                "LM-head argmax supports tied embedding or [rows, hidden] with tiled K=1024"
                    .to_owned()
            }
            _ => unreachable!("global vocab row only supports embedding/lm_head"),
        },
    }
}

fn ffn_rows(layer: usize, pack: Option<&MetalPack>) -> Vec<ShapeAuditRow> {
    vec![
        layer_row(
            layer,
            TensorClass::MlpGate,
            vec![vec![QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size]],
            vec![vec![QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size]],
            pack,
            "FFN gate path is implemented as RMS+matvec K=1024",
        ),
        layer_row(
            layer,
            TensorClass::MlpUp,
            vec![vec![QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size]],
            vec![vec![QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size]],
            pack,
            "FFN up path is implemented as RMS+matvec K=1024",
        ),
        layer_row(
            layer,
            TensorClass::MlpDown,
            vec![vec![QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate]],
            vec![vec![QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate]],
            pack,
            "FFN down path is implemented as matvec K=3584",
        ),
    ]
}

fn delta_rows(layer: usize, pack: Option<&MetalPack>) -> Vec<ShapeAuditRow> {
    let width = QWEN35_08B.deltanet_width();
    let mut rows = vec![
        layer_row(
            layer,
            TensorClass::DeltaQkv,
            vec![vec![
                QWEN35_08B.deltanet_qkv_width(),
                QWEN35_08B.hidden_size,
            ]],
            vec![vec![
                QWEN35_08B.deltanet_qkv_width(),
                QWEN35_08B.hidden_size,
            ]],
            pack,
            "DeltaNet q/k/v split kernel assumes three 2048-wide vectors",
        ),
        layer_row(
            layer,
            TensorClass::DeltaZ,
            vec![vec![width, QWEN35_08B.hidden_size]],
            vec![vec![width, QWEN35_08B.hidden_size]],
            pack,
            "DeltaNet z gate path feeds the 2048-wide recurrent output",
        ),
        layer_row(
            layer,
            TensorClass::DeltaB,
            vec![vec![QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size]],
            vec![vec![QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size]],
            pack,
            "DeltaNet beta placeholder expects one scalar per V head",
        ),
        layer_row(
            layer,
            TensorClass::DeltaA,
            vec![vec![QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size]],
            vec![vec![QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size]],
            pack,
            "DeltaNet gate placeholder expects one scalar per V head",
        ),
        layer_row(
            layer,
            TensorClass::DeltaOut,
            vec![vec![QWEN35_08B.hidden_size, width]],
            vec![vec![QWEN35_08B.hidden_size, width]],
            pack,
            "DeltaNet out projection is implemented as matvec K=2048",
        ),
    ];
    rows.extend(delta_state_param_rows(layer, pack));
    rows
}

fn delta_state_param_rows(layer: usize, pack: Option<&MetalPack>) -> Vec<ShapeAuditRow> {
    let conv_channels = QWEN35_08B.deltanet_qkv_width();
    vec![
        delta_state_param_row(
            layer,
            "A_log",
            vec![vec![QWEN35_08B.deltanet_v_heads]],
            pack,
            true,
            "DeltaNet A_log decay parameter is consumed by the layered decode decay kernel",
        ),
        delta_state_param_row(
            layer,
            "dt_bias",
            vec![vec![QWEN35_08B.deltanet_v_heads]],
            pack,
            true,
            "DeltaNet dt_bias decay bias is consumed by the layered decode decay kernel",
        ),
        delta_state_param_row(
            layer,
            "conv1d.weight",
            vec![
                vec![conv_channels, 4],
                vec![conv_channels, 1, 4],
                vec![4, conv_channels],
            ],
            pack,
            true,
            "DeltaNet causal Conv1D weight is consumed by the layered decode Conv1D state kernel",
        ),
        delta_state_param_row(
            layer,
            "conv1d.bias",
            vec![vec![conv_channels]],
            pack,
            false,
            "DeltaNet causal Conv1D bias is optional; missing bias is treated as zero",
        ),
        delta_state_param_row(
            layer,
            "norm.weight",
            vec![vec![QWEN35_08B.deltanet_head_dim]],
            pack,
            true,
            "DeltaNet gated RMSNorm weight is consumed by the layered decode gated norm kernel",
        ),
    ]
}

fn attention_rows(layer: usize, pack: Option<&MetalPack>) -> Vec<ShapeAuditRow> {
    let q = QWEN35_08B.attention_q_width();
    let q_with_gate = QWEN35_08B.attention_q_with_head_gate_width();
    let kv = QWEN35_08B.attention_kv_width();
    vec![
        layer_row(
            layer,
            TensorClass::AttentionQ,
            vec![
                vec![q, QWEN35_08B.hidden_size],
                vec![q_with_gate, QWEN35_08B.hidden_size],
            ],
            vec![
                vec![q, QWEN35_08B.hidden_size],
                vec![q_with_gate, QWEN35_08B.hidden_size],
            ],
            pack,
            "attention projection supports Qwen GQA q width and optional per-dimension output gate",
        ),
        layer_row(
            layer,
            TensorClass::AttentionK,
            vec![vec![kv, QWEN35_08B.hidden_size]],
            vec![vec![kv, QWEN35_08B.hidden_size]],
            pack,
            "attention projection supports Qwen GQA kv width",
        ),
        layer_row(
            layer,
            TensorClass::AttentionV,
            vec![vec![kv, QWEN35_08B.hidden_size]],
            vec![vec![kv, QWEN35_08B.hidden_size]],
            pack,
            "attention projection supports Qwen GQA kv width",
        ),
        layer_row(
            layer,
            TensorClass::AttentionO,
            vec![vec![QWEN35_08B.hidden_size, q]],
            vec![vec![QWEN35_08B.hidden_size, q]],
            pack,
            "attention output projection is implemented as matvec K=2048",
        ),
    ]
}

fn layer_row(
    layer: usize,
    class: TensorClass,
    expected_shapes: Vec<Vec<usize>>,
    kernel_shapes: Vec<Vec<usize>>,
    pack: Option<&MetalPack>,
    note: &str,
) -> ShapeAuditRow {
    let actual_shape = pack
        .and_then(|pack| {
            pack.entries
                .iter()
                .find(|entry| entry.layer == Some(layer) && entry.class == class)
        })
        .map(|entry| entry.source_shape.clone());
    let status = status_for(&expected_shapes, &kernel_shapes, actual_shape.as_ref());
    ShapeAuditRow {
        layer: Some(layer),
        class,
        expected_shapes,
        kernel_shapes,
        actual_shape,
        status,
        note: note.to_owned(),
    }
}

fn delta_state_param_row(
    layer: usize,
    tensor_marker: &str,
    expected_shapes: Vec<Vec<usize>>,
    pack: Option<&MetalPack>,
    supported_by_kernel: bool,
    note: &str,
) -> ShapeAuditRow {
    let actual_shape = pack
        .and_then(|pack| {
            pack.entries.iter().find(|entry| {
                entry.layer == Some(layer)
                    && entry.class == TensorClass::DeltaStateParam
                    && entry.tensor.contains(tensor_marker)
            })
        })
        .map(|entry| entry.source_shape.clone());
    let status = match actual_shape.as_ref() {
        None => ShapeAuditStatus::Missing,
        Some(actual)
            if supported_by_kernel && expected_shapes.iter().any(|shape| shape == actual) =>
        {
            ShapeAuditStatus::Supported
        }
        Some(actual) if expected_shapes.iter().any(|shape| shape == actual) => {
            ShapeAuditStatus::KernelPlaceholder
        }
        Some(_) => ShapeAuditStatus::Unsupported,
    };
    ShapeAuditRow {
        layer: Some(layer),
        class: TensorClass::DeltaStateParam,
        expected_shapes,
        kernel_shapes: Vec::new(),
        actual_shape,
        status,
        note: format!("{tensor_marker}: {note}"),
    }
}

fn status_for(
    expected_shapes: &[Vec<usize>],
    kernel_shapes: &[Vec<usize>],
    actual_shape: Option<&Vec<usize>>,
) -> ShapeAuditStatus {
    if expected_shapes != kernel_shapes {
        if let Some(actual) = actual_shape {
            if !expected_shapes.iter().any(|shape| shape == actual) {
                return ShapeAuditStatus::Unsupported;
            }
        }
        return ShapeAuditStatus::KernelPlaceholder;
    }
    match actual_shape {
        None => ShapeAuditStatus::Missing,
        Some(actual) if expected_shapes.iter().any(|shape| shape == actual) => {
            ShapeAuditStatus::Supported
        }
        Some(_) => ShapeAuditStatus::Unsupported,
    }
}

pub fn is_entry_tiled_fp16(entry: &crate::MetalPackEntry) -> bool {
    entry.dtype == "F16" && entry.layout == PackLayout::Fp16RowTiled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_marks_missing_attention_without_pack() {
        let rows = audit_shape_contract(None);
        let attention_q = rows
            .iter()
            .find(|row| row.layer == Some(3) && row.class == TensorClass::AttentionQ)
            .unwrap();
        assert_eq!(attention_q.status, ShapeAuditStatus::Missing);
        let ffn_gate = rows
            .iter()
            .find(|row| row.layer == Some(0) && row.class == TensorClass::MlpGate)
            .unwrap();
        assert_eq!(ffn_gate.status, ShapeAuditStatus::Missing);
    }
}
