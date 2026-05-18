//! Decoder building blocks: timing conditioning, KV cache and one-token layer step.

use crate::consts::*;
use crate::kernels::{AttentionSpec, KernelBackend};
use crate::{Error, Result};

pub fn compute_time_embedding(out: &mut [f32], delay_tokens: f32) -> Result<()> {
    if out.len() != VOX_DEC_DIM {
        return Err(Error::Shape("time embedding"));
    }
    let half = VOX_DEC_DIM / 2;
    let log_theta = 10_000.0f32.ln();
    for i in 0..half {
        let inv_freq = (-log_theta * i as f32 / half as f32).exp();
        let emb = delay_tokens * inv_freq;
        out[i] = emb.cos();
        out[i + half] = emb.sin();
    }
    Ok(())
}

pub fn compute_ada_scale<B: KernelBackend>(
    backend: &mut B,
    ada_scale_out: &mut [f32],
    t_cond: &[f32],
    ada_down: &[f32],
    ada_up: &[f32],
) -> Result<()> {
    if ada_scale_out.len() != VOX_DEC_DIM {
        return Err(Error::Shape("ada scale out"));
    }
    if t_cond.len() != VOX_DEC_DIM {
        return Err(Error::Shape("t_cond"));
    }
    if ada_down.len() != VOX_ADA_NORM_DIM * VOX_DEC_DIM {
        return Err(Error::Shape("ada_down"));
    }
    if ada_up.len() != VOX_DEC_DIM * VOX_ADA_NORM_DIM {
        return Err(Error::Shape("ada_up"));
    }
    let mut hidden = [0.0f32; VOX_ADA_NORM_DIM];
    backend.linear_f32(
        &mut hidden,
        t_cond,
        ada_down,
        None,
        1,
        VOX_DEC_DIM,
        VOX_ADA_NORM_DIM,
    )?;
    backend.gelu_inplace(&mut hidden)?;
    backend.linear_f32(
        ada_scale_out,
        &hidden,
        ada_up,
        None,
        1,
        VOX_ADA_NORM_DIM,
        VOX_DEC_DIM,
    )?;
    Ok(())
}

pub struct DecoderLayerBf16<'a> {
    pub wq: &'a [u16], // [4096,3072]
    pub wk: &'a [u16], // [1024,3072]
    pub wv: &'a [u16], // [1024,3072]
    pub wo: &'a [u16], // [3072,4096]
    pub w1: &'a [u16], // [9216,3072]
    pub w2: &'a [u16], // [3072,9216]
    pub w3: &'a [u16], // [9216,3072]
    pub attention_norm: &'a [f32],
    pub ffn_norm: &'a [f32],
    pub ada_scale: &'a [f32], // [3072]
}

pub struct KvCache {
    pub k: Vec<f32>,
    pub v: Vec<f32>,
    pub layers: usize,
    pub max_seq: usize,
    pub kv_dim: usize,
    pub len: usize,
    pub pos_offset: usize,
}

impl KvCache {
    pub fn new(layers: usize, max_seq: usize, kv_dim: usize) -> Self {
        let n = layers * max_seq * kv_dim;
        Self {
            k: vec![0.0; n],
            v: vec![0.0; n],
            layers,
            max_seq,
            kv_dim,
            len: 0,
            pos_offset: 0,
        }
    }

    pub fn write_layer_pos(
        &mut self,
        layer: usize,
        pos: usize,
        k_token: &[f32],
        v_token: &[f32],
    ) -> Result<()> {
        if layer >= self.layers || pos >= self.max_seq {
            return Err(Error::OutOfBounds("kv cache write"));
        }
        if k_token.len() != self.kv_dim || v_token.len() != self.kv_dim {
            return Err(Error::Shape("kv token dim"));
        }
        let off = (layer * self.max_seq + pos) * self.kv_dim;
        self.k[off..off + self.kv_dim].copy_from_slice(k_token);
        self.v[off..off + self.kv_dim].copy_from_slice(v_token);
        Ok(())
    }

    pub fn layer_k(&self, layer: usize, seq_k: usize) -> Result<&[f32]> {
        if layer >= self.layers || seq_k > self.max_seq {
            return Err(Error::OutOfBounds("kv layer_k"));
        }
        let off = layer * self.max_seq * self.kv_dim;
        Ok(&self.k[off..off + seq_k * self.kv_dim])
    }

    pub fn layer_v(&self, layer: usize, seq_k: usize) -> Result<&[f32]> {
        if layer >= self.layers || seq_k > self.max_seq {
            return Err(Error::OutOfBounds("kv layer_v"));
        }
        let off = layer * self.max_seq * self.kv_dim;
        Ok(&self.v[off..off + seq_k * self.kv_dim])
    }

    pub fn advance_one(&mut self) -> Result<()> {
        if self.len >= self.max_seq {
            return Err(Error::OutOfBounds(
                "kv cache full; compaction not implemented in seed",
            ));
        }
        self.len += 1;
        Ok(())
    }
}

pub struct DecoderScratch {
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

impl DecoderScratch {
    pub fn new() -> Self {
        Self {
            x_norm: vec![0.0; VOX_DEC_DIM],
            q: vec![0.0; VOX_DEC_HEADS * VOX_DEC_HEAD_DIM],
            k: vec![0.0; VOX_DEC_KV_HEADS * VOX_DEC_HEAD_DIM],
            v: vec![0.0; VOX_DEC_KV_HEADS * VOX_DEC_HEAD_DIM],
            attn_out: vec![0.0; VOX_DEC_HEADS * VOX_DEC_HEAD_DIM],
            proj_out: vec![0.0; VOX_DEC_DIM],
            gate: vec![0.0; VOX_DEC_HIDDEN],
            up: vec![0.0; VOX_DEC_HIDDEN],
            ffn_out: vec![0.0; VOX_DEC_DIM],
        }
    }
}

impl Default for DecoderScratch {
    fn default() -> Self {
        Self::new()
    }
}

pub fn decoder_layer_step_bf16<B: KernelBackend>(
    backend: &mut B,
    layer_idx: usize,
    layer: DecoderLayerBf16<'_>,
    h: &mut [f32],
    cache: &mut KvCache,
    scratch: &mut DecoderScratch,
    logical_pos: usize,
) -> Result<()> {
    if h.len() != VOX_DEC_DIM {
        return Err(Error::Shape("decoder h"));
    }
    let q_dim = VOX_DEC_HEADS * VOX_DEC_HEAD_DIM;
    let kv_dim = VOX_DEC_KV_HEADS * VOX_DEC_HEAD_DIM;
    if layer.wq.len() != q_dim * VOX_DEC_DIM
        || layer.wk.len() != kv_dim * VOX_DEC_DIM
        || layer.wv.len() != kv_dim * VOX_DEC_DIM
    {
        return Err(Error::Shape("decoder qkv weights"));
    }
    if layer.wo.len() != VOX_DEC_DIM * q_dim
        || layer.w1.len() != VOX_DEC_HIDDEN * VOX_DEC_DIM
        || layer.w2.len() != VOX_DEC_DIM * VOX_DEC_HIDDEN
        || layer.w3.len() != VOX_DEC_HIDDEN * VOX_DEC_DIM
    {
        return Err(Error::Shape("decoder projection/ffn weights"));
    }

    backend.rms_norm(
        &mut scratch.x_norm,
        h,
        layer.attention_norm,
        1,
        VOX_DEC_DIM,
        VOX_DEC_NORM_EPS,
    )?;
    backend.linear_bf16(
        &mut scratch.q,
        &scratch.x_norm,
        layer.wq,
        None,
        1,
        VOX_DEC_DIM,
        q_dim,
    )?;
    backend.linear_bf16(
        &mut scratch.k,
        &scratch.x_norm,
        layer.wk,
        None,
        1,
        VOX_DEC_DIM,
        kv_dim,
    )?;
    backend.linear_bf16(
        &mut scratch.v,
        &scratch.x_norm,
        layer.wv,
        None,
        1,
        VOX_DEC_DIM,
        kv_dim,
    )?;

    backend.rope_interleaved_inplace(
        &mut scratch.q,
        VOX_DEC_HEADS,
        VOX_DEC_HEAD_DIM,
        logical_pos,
        VOX_ROPE_THETA,
    )?;
    backend.rope_interleaved_inplace(
        &mut scratch.k,
        VOX_DEC_KV_HEADS,
        VOX_DEC_HEAD_DIM,
        logical_pos,
        VOX_ROPE_THETA,
    )?;

    let physical_pos = cache.len;
    cache.write_layer_pos(layer_idx, physical_pos, &scratch.k, &scratch.v)?;
    let seq_k = physical_pos + 1;
    let k_cache = cache.layer_k(layer_idx, seq_k)?;
    let v_cache = cache.layer_v(layer_idx, seq_k)?;
    backend.causal_attention(
        &mut scratch.attn_out,
        &scratch.q,
        k_cache,
        v_cache,
        AttentionSpec {
            seq_q: 1,
            seq_k,
            n_heads: VOX_DEC_HEADS,
            n_kv_heads: VOX_DEC_KV_HEADS,
            head_dim: VOX_DEC_HEAD_DIM,
            scale: 1.0 / (VOX_DEC_HEAD_DIM as f32).sqrt(),
            window_size: VOX_DEC_WINDOW,
            q_offset: logical_pos.saturating_sub(cache.pos_offset),
        },
    )?;

    backend.linear_bf16(
        &mut scratch.proj_out,
        &scratch.attn_out,
        layer.wo,
        None,
        1,
        q_dim,
        VOX_DEC_DIM,
    )?;
    backend.add_inplace(h, &scratch.proj_out)?;

    backend.rms_norm(
        &mut scratch.x_norm,
        h,
        layer.ffn_norm,
        1,
        VOX_DEC_DIM,
        VOX_DEC_NORM_EPS,
    )?;
    for i in 0..VOX_DEC_DIM {
        scratch.x_norm[i] *= 1.0 + layer.ada_scale[i];
    }
    backend.linear_bf16(
        &mut scratch.gate,
        &scratch.x_norm,
        layer.w1,
        None,
        1,
        VOX_DEC_DIM,
        VOX_DEC_HIDDEN,
    )?;
    backend.linear_bf16(
        &mut scratch.up,
        &scratch.x_norm,
        layer.w3,
        None,
        1,
        VOX_DEC_DIM,
        VOX_DEC_HIDDEN,
    )?;
    backend.silu_inplace(&mut scratch.gate)?;
    backend.mul_inplace(&mut scratch.gate, &scratch.up)?;
    backend.linear_bf16(
        &mut scratch.ffn_out,
        &scratch.gate,
        layer.w2,
        None,
        1,
        VOX_DEC_HIDDEN,
        VOX_DEC_DIM,
    )?;
    backend.add_inplace(h, &scratch.ffn_out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_embedding_shape() {
        let mut t = vec![0.0; VOX_DEC_DIM];
        compute_time_embedding(&mut t, DEFAULT_DELAY_TOKENS as f32).unwrap();
        assert!((t[0] - (DEFAULT_DELAY_TOKENS as f32).cos()).abs() < 1e-6);
        assert!(t[VOX_DEC_DIM / 2].abs() > 0.0);
    }
}
