//! Encoder building blocks.
//!
//! The complete encoder graph is large; this module contains the ported shape
//! contracts and reusable layer primitives. The same `KernelBackend` trait is
//! used as for the decoder, so Metal/CUDA/WGSL kernels can replace the CPU path.

use crate::consts::*;
use crate::kernels::{AttentionSpec, KernelBackend};
use crate::{Error, Result};

pub struct EncoderLayerBf16<'a> {
    pub wq: &'a [u16], // [2048,1280]
    pub wk: &'a [u16], // [2048,1280]
    pub wv: &'a [u16], // [2048,1280]
    pub wo: &'a [u16], // [1280,2048]
    pub wq_bias: &'a [f32],
    pub wv_bias: &'a [f32],
    pub wo_bias: &'a [f32],
    pub attention_norm: &'a [f32],
    pub w1: &'a [u16], // [5120,1280]
    pub w2: &'a [u16], // [1280,5120]
    pub w2_bias: &'a [f32],
    pub w3: &'a [u16], // [5120,1280]
    pub ffn_norm: &'a [f32],
}

pub struct EncoderScratch {
    pub x_norm: Vec<f32>,
    pub q: Vec<f32>,
    pub k: Vec<f32>,
    pub v: Vec<f32>,
    pub attn_out: Vec<f32>,
    pub proj_out: Vec<f32>,
    pub gate: Vec<f32>,
    pub up: Vec<f32>,
    pub ffn_out: Vec<f32>,
}

impl EncoderScratch {
    pub fn new(seq_len: usize) -> Self {
        let q_dim = VOX_ENC_HEADS * VOX_ENC_HEAD_DIM;
        let kv_dim = VOX_ENC_KV_HEADS * VOX_ENC_HEAD_DIM;
        Self {
            x_norm: vec![0.0; seq_len * VOX_ENC_DIM],
            q: vec![0.0; seq_len * q_dim],
            k: vec![0.0; seq_len * kv_dim],
            v: vec![0.0; seq_len * kv_dim],
            attn_out: vec![0.0; seq_len * q_dim],
            proj_out: vec![0.0; seq_len * VOX_ENC_DIM],
            gate: vec![0.0; seq_len * VOX_ENC_HIDDEN],
            up: vec![0.0; seq_len * VOX_ENC_HIDDEN],
            ffn_out: vec![0.0; seq_len * VOX_ENC_DIM],
        }
    }
}

pub fn conv_stem<B: KernelBackend>(
    backend: &mut B,
    mel: &[f32],
    mel_frames: usize,
    conv0_weight: &[f32],
    conv0_bias: &[f32],
    conv1_weight: &[f32],
    conv1_bias: &[f32],
) -> Result<(Vec<f32>, usize)> {
    if mel.len() != VOX_MEL_BINS * mel_frames {
        return Err(Error::Shape("mel"));
    }
    let len0 = ((mel_frames + 2 - 3) / 1) + 1; // causal conv with padding kernel-stride=2 for k=3,stride=1
    let mut x0 = vec![0.0f32; VOX_ENC_DIM * len0];
    backend.causal_conv1d(
        &mut x0,
        mel,
        conv0_weight,
        Some(conv0_bias),
        VOX_MEL_BINS,
        VOX_ENC_DIM,
        mel_frames,
        3,
        1,
    )?;
    backend.gelu_inplace(&mut x0)?;

    let len1 = (((len0 as f32 - 3.0 + (3 - 2) as f32) / 2.0) + 1.0)
        .ceil()
        .max(0.0) as usize;
    let mut x1 = vec![0.0f32; VOX_ENC_DIM * len1];
    backend.causal_conv1d(
        &mut x1,
        &x0,
        conv1_weight,
        Some(conv1_bias),
        VOX_ENC_DIM,
        VOX_ENC_DIM,
        len0,
        3,
        2,
    )?;
    backend.gelu_inplace(&mut x1)?;
    Ok((x1, len1))
}

pub fn encoder_layer_full_bf16<B: KernelBackend>(
    backend: &mut B,
    layer: EncoderLayerBf16<'_>,
    h: &mut [f32],
    seq_len: usize,
    scratch: &mut EncoderScratch,
    pos_offset: usize,
) -> Result<()> {
    if h.len() != seq_len * VOX_ENC_DIM {
        return Err(Error::Shape("encoder h"));
    }
    let q_dim = VOX_ENC_HEADS * VOX_ENC_HEAD_DIM;
    let kv_dim = VOX_ENC_KV_HEADS * VOX_ENC_HEAD_DIM;

    backend.rms_norm(
        &mut scratch.x_norm,
        h,
        layer.attention_norm,
        seq_len,
        VOX_ENC_DIM,
        VOX_ENC_NORM_EPS,
    )?;
    backend.linear_bf16(
        &mut scratch.q,
        &scratch.x_norm,
        layer.wq,
        Some(layer.wq_bias),
        seq_len,
        VOX_ENC_DIM,
        q_dim,
    )?;
    backend.linear_bf16(
        &mut scratch.k,
        &scratch.x_norm,
        layer.wk,
        None,
        seq_len,
        VOX_ENC_DIM,
        kv_dim,
    )?;
    backend.linear_bf16(
        &mut scratch.v,
        &scratch.x_norm,
        layer.wv,
        Some(layer.wv_bias),
        seq_len,
        VOX_ENC_DIM,
        kv_dim,
    )?;

    for t in 0..seq_len {
        let q_row = &mut scratch.q[t * q_dim..(t + 1) * q_dim];
        let k_row = &mut scratch.k[t * kv_dim..(t + 1) * kv_dim];
        backend.rope_interleaved_inplace(
            q_row,
            VOX_ENC_HEADS,
            VOX_ENC_HEAD_DIM,
            pos_offset + t,
            VOX_ROPE_THETA,
        )?;
        backend.rope_interleaved_inplace(
            k_row,
            VOX_ENC_KV_HEADS,
            VOX_ENC_HEAD_DIM,
            pos_offset + t,
            VOX_ROPE_THETA,
        )?;
    }

    backend.causal_attention(
        &mut scratch.attn_out,
        &scratch.q,
        &scratch.k,
        &scratch.v,
        AttentionSpec {
            seq_q: seq_len,
            seq_k: seq_len,
            n_heads: VOX_ENC_HEADS,
            n_kv_heads: VOX_ENC_KV_HEADS,
            head_dim: VOX_ENC_HEAD_DIM,
            scale: 1.0 / (VOX_ENC_HEAD_DIM as f32).sqrt(),
            window_size: VOX_ENC_WINDOW,
            q_offset: 0,
        },
    )?;
    backend.linear_bf16(
        &mut scratch.proj_out,
        &scratch.attn_out,
        layer.wo,
        Some(layer.wo_bias),
        seq_len,
        q_dim,
        VOX_ENC_DIM,
    )?;
    backend.add_inplace(h, &scratch.proj_out)?;

    backend.rms_norm(
        &mut scratch.x_norm,
        h,
        layer.ffn_norm,
        seq_len,
        VOX_ENC_DIM,
        VOX_ENC_NORM_EPS,
    )?;
    backend.linear_bf16(
        &mut scratch.gate,
        &scratch.x_norm,
        layer.w1,
        None,
        seq_len,
        VOX_ENC_DIM,
        VOX_ENC_HIDDEN,
    )?;
    backend.linear_bf16(
        &mut scratch.up,
        &scratch.x_norm,
        layer.w3,
        None,
        seq_len,
        VOX_ENC_DIM,
        VOX_ENC_HIDDEN,
    )?;
    backend.silu_inplace(&mut scratch.gate)?;
    backend.mul_inplace(&mut scratch.gate, &scratch.up)?;
    backend.linear_bf16(
        &mut scratch.ffn_out,
        &scratch.gate,
        layer.w2,
        Some(layer.w2_bias),
        seq_len,
        VOX_ENC_HIDDEN,
        VOX_ENC_DIM,
    )?;
    backend.add_inplace(h, &scratch.ffn_out)?;
    Ok(())
}
