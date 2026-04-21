//! Safetensors → MegakernelWeights loader for the Qwen3.5-0.8B
//! hybrid drafter.
//!
//! Walks through the HF checkpoint's 24 layers, resolving each
//! layer's weight tensors to the megakernel's expected BF16 layout
//! via a caller-supplied `VarBuilder`. Handles both the merged
//! Qwen3-next weight naming (`linear_attn.in_proj_qkvz.weight`,
//! `linear_attn.in_proj_ba.weight`) and the split convention
//! (`linear_attn.in_proj_qkv.weight` + `linear_attn.in_proj_z.weight`,
//! etc.) — the same fallback the main Qwen3.5 loader supports.
//!
//! Shape contract (all BF16):
//!   embed       [VOCAB, HIDDEN]
//!   final_norm  [HIDDEN]
//!   lm_head     [VOCAB, HIDDEN]
//!
//! Per DeltaNet layer:
//!   input_layernorm     [HIDDEN]
//!   qkv_proj            [DN_CONV_CHANNELS=6144, HIDDEN=1024]
//!   z_proj              [DN_V_SIZE=2048,        HIDDEN=1024]
//!   beta_proj           [DN_HEADS=16,           HIDDEN=1024]
//!   alpha_proj          [DN_HEADS=16,           HIDDEN=1024]
//!   conv1d              [DN_CONV_CHANNELS=6144, 1, DN_CONV_KERNEL=4]
//!   a_log               [DN_HEADS=16]
//!   dt_bias             [DN_HEADS=16]
//!   norm                [DN_VALUE_DIM=128]
//!   out_proj            [HIDDEN=1024, DN_V_SIZE=2048]
//!   post_attn_layernorm [HIDDEN]
//!   gate/up/down        (INTERMEDIATE=3584 ↔ HIDDEN=1024)
//!
//! Per FullAttention layer:
//!   input_layernorm     [HIDDEN]
//!   q_proj              [FA_QPROJ_SIZE=4096, HIDDEN=1024]   (q + gate concatenated, DFlash convention)
//!   k_proj              [FA_KV_SIZE=512,     HIDDEN=1024]
//!   v_proj              [FA_KV_SIZE=512,     HIDDEN=1024]
//!   q_norm              [FA_HEAD_DIM=256]
//!   k_norm              [FA_HEAD_DIM=256]
//!   o_proj              [HIDDEN=1024, FA_Q_SIZE=2048]
//!   post_attn_layernorm [HIDDEN]
//!   gate/up/down        (INTERMEDIATE=3584 ↔ HIDDEN=1024)

#![cfg(feature = "cuda")]

use candle_core::{DType, Device, Result, Tensor};
use candle_nn::VarBuilder;

use super::constants::*;
use super::weights::{
    DeltaNetLayer, FullAttentionLayer, LayerBundle, MegakernelWeights, QWEN35_0_8B_LAYER_PATTERN,
};

/// Load all megakernel weights from a BF16 CUDA `VarBuilder` rooted
/// at the model's top (either `model.` prefix present or not — the
/// caller positions the VarBuilder, we use straight child names
/// from there).
///
/// Expected layout under `vb`:
///   * `embed_tokens.weight`
///   * `layers.{i}.…` for i in 0..24
///   * `norm.weight`
///   * `lm_head.weight` (or shared with embed — we always load from
///     the path because Qwen3.5 checkpoints ship it separately).
pub fn load_megakernel_weights(vb: VarBuilder, device: Device) -> Result<MegakernelWeights> {
    if vb.dtype() != DType::BF16 {
        candle_core::bail!(
            "load_megakernel_weights: VarBuilder must be BF16 (got {:?})",
            vb.dtype()
        );
    }
    if !device.is_cuda() {
        candle_core::bail!(
            "load_megakernel_weights: device must be CUDA (got {:?})",
            device.location()
        );
    }

    let embed = vb.get((VOCAB_SIZE, HIDDEN_SIZE), "embed_tokens.weight")?;
    let final_norm = vb.get((HIDDEN_SIZE,), "norm.weight")?;
    // lm_head is stored separately in Qwen3.5 checkpoints.
    let lm_head = vb.get((VOCAB_SIZE, HIDDEN_SIZE), "lm_head.weight")?;

    let mut layers = Vec::with_capacity(NUM_LAYERS);
    for i in 0..NUM_LAYERS {
        let vb_l = vb.pp(format!("layers.{i}"));
        let layer = if QWEN35_0_8B_LAYER_PATTERN[i] == 1 {
            LayerBundle::FullAttention(load_fa_layer(vb_l)?)
        } else {
            LayerBundle::DeltaNet(load_dn_layer(vb_l)?)
        };
        layers.push(layer);
    }

    MegakernelWeights::new(device, embed, final_norm, lm_head, layers)
}

fn load_dn_layer(vb: VarBuilder) -> Result<DeltaNetLayer> {
    let input_layernorm = vb.get((HIDDEN_SIZE,), "input_layernorm.weight")?;
    let post_attn_layernorm = vb.get((HIDDEN_SIZE,), "post_attention_layernorm.weight")?;
    let vb_la = vb.pp("linear_attn");

    // qkv_proj: 6144 rows (Q + K + V interleaved). z_proj: 2048 rows.
    // Reference loader supports both merged qkvz and split qkv+z.
    let qkvz_size = DN_QK_SIZE + DN_QK_SIZE + DN_V_SIZE + DN_V_SIZE; // 8192 = qkv+z
    let (qkv_proj, z_proj) = if vb_la.contains_tensor("in_proj_qkvz.weight") {
        let qkvz = vb_la.get((qkvz_size, HIDDEN_SIZE), "in_proj_qkvz.weight")?;
        let qkv = qkvz
            .narrow(0, 0, DN_CONV_CHANNELS)?
            .contiguous()?;
        let z = qkvz
            .narrow(0, DN_CONV_CHANNELS, DN_V_SIZE)?
            .contiguous()?;
        (qkv, z)
    } else {
        let qkv = vb_la.get((DN_CONV_CHANNELS, HIDDEN_SIZE), "in_proj_qkv.weight")?;
        let z = vb_la.get((DN_V_SIZE, HIDDEN_SIZE), "in_proj_z.weight")?;
        (qkv, z)
    };

    let (beta_proj, alpha_proj) = if vb_la.contains_tensor("in_proj_ba.weight") {
        let ba = vb_la.get((DN_NUM_HEADS * 2, HIDDEN_SIZE), "in_proj_ba.weight")?;
        let beta = ba.narrow(0, 0, DN_NUM_HEADS)?.contiguous()?;
        let alpha = ba.narrow(0, DN_NUM_HEADS, DN_NUM_HEADS)?.contiguous()?;
        (beta, alpha)
    } else {
        let b = vb_la.get((DN_NUM_HEADS, HIDDEN_SIZE), "in_proj_b.weight")?;
        let a = vb_la.get((DN_NUM_HEADS, HIDDEN_SIZE), "in_proj_a.weight")?;
        (b, a)
    };

    let conv1d = vb_la.get(
        (DN_CONV_CHANNELS, 1, DN_CONV_KERNEL),
        "conv1d.weight",
    )?;
    let a_log = vb_la.get((DN_NUM_HEADS,), "A_log")?;
    let dt_bias = vb_la.get((DN_NUM_HEADS,), "dt_bias")?;
    let norm = vb_la.get((DN_VALUE_DIM,), "norm.weight")?;
    let out_proj = vb_la.get((HIDDEN_SIZE, DN_V_SIZE), "out_proj.weight")?;

    let vb_mlp = vb.pp("mlp");
    let gate_proj = vb_mlp.get((INTERMEDIATE_SIZE, HIDDEN_SIZE), "gate_proj.weight")?;
    let up_proj = vb_mlp.get((INTERMEDIATE_SIZE, HIDDEN_SIZE), "up_proj.weight")?;
    let down_proj = vb_mlp.get((HIDDEN_SIZE, INTERMEDIATE_SIZE), "down_proj.weight")?;

    Ok(DeltaNetLayer {
        input_layernorm: ensure_bf16_contig(input_layernorm)?,
        qkv_proj: ensure_bf16_contig(qkv_proj)?,
        z_proj: ensure_bf16_contig(z_proj)?,
        beta_proj: ensure_bf16_contig(beta_proj)?,
        alpha_proj: ensure_bf16_contig(alpha_proj)?,
        conv1d: ensure_bf16_contig(conv1d)?,
        a_log: ensure_bf16_contig(a_log)?,
        dt_bias: ensure_bf16_contig(dt_bias)?,
        norm: ensure_bf16_contig(norm)?,
        out_proj: ensure_bf16_contig(out_proj)?,
        post_attn_layernorm: ensure_bf16_contig(post_attn_layernorm)?,
        gate_proj: ensure_bf16_contig(gate_proj)?,
        up_proj: ensure_bf16_contig(up_proj)?,
        down_proj: ensure_bf16_contig(down_proj)?,
    })
}

fn load_fa_layer(vb: VarBuilder) -> Result<FullAttentionLayer> {
    let input_layernorm = vb.get((HIDDEN_SIZE,), "input_layernorm.weight")?;
    let post_attn_layernorm = vb.get((HIDDEN_SIZE,), "post_attention_layernorm.weight")?;
    let vb_sa = vb.pp("self_attn");

    let q_proj = vb_sa.get((FA_QPROJ_SIZE, HIDDEN_SIZE), "q_proj.weight")?;
    let k_proj = vb_sa.get((FA_KV_SIZE, HIDDEN_SIZE), "k_proj.weight")?;
    let v_proj = vb_sa.get((FA_KV_SIZE, HIDDEN_SIZE), "v_proj.weight")?;
    let q_norm = vb_sa.get((FA_HEAD_DIM,), "q_norm.weight")?;
    let k_norm = vb_sa.get((FA_HEAD_DIM,), "k_norm.weight")?;
    let o_proj = vb_sa.get((HIDDEN_SIZE, FA_Q_SIZE), "o_proj.weight")?;

    let vb_mlp = vb.pp("mlp");
    let gate_proj = vb_mlp.get((INTERMEDIATE_SIZE, HIDDEN_SIZE), "gate_proj.weight")?;
    let up_proj = vb_mlp.get((INTERMEDIATE_SIZE, HIDDEN_SIZE), "up_proj.weight")?;
    let down_proj = vb_mlp.get((HIDDEN_SIZE, INTERMEDIATE_SIZE), "down_proj.weight")?;

    Ok(FullAttentionLayer {
        input_layernorm: ensure_bf16_contig(input_layernorm)?,
        q_proj: ensure_bf16_contig(q_proj)?,
        k_proj: ensure_bf16_contig(k_proj)?,
        v_proj: ensure_bf16_contig(v_proj)?,
        q_norm: ensure_bf16_contig(q_norm)?,
        k_norm: ensure_bf16_contig(k_norm)?,
        o_proj: ensure_bf16_contig(o_proj)?,
        post_attn_layernorm: ensure_bf16_contig(post_attn_layernorm)?,
        gate_proj: ensure_bf16_contig(gate_proj)?,
        up_proj: ensure_bf16_contig(up_proj)?,
        down_proj: ensure_bf16_contig(down_proj)?,
    })
}

/// Enforce BF16 dtype + contiguity (the kernel's pointer cast
/// assumes both). Casts via `to_dtype` if needed; errors if the
/// underlying storage isn't BF16-capable (e.g. quantised).
fn ensure_bf16_contig(t: Tensor) -> Result<Tensor> {
    let t = if t.dtype() == DType::BF16 {
        t
    } else {
        t.to_dtype(DType::BF16)?
    };
    t.contiguous()
}
