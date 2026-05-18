//! Audio-language adapter: downsample-4 + Linear(5120->3072) + GELU + Linear(3072->3072).

use crate::consts::{VOX_DEC_DIM, VOX_DOWNSAMPLE, VOX_ENC_DIM};
use crate::kernels::KernelBackend;
use crate::{Error, Result};

pub struct AdapterWeights<'a> {
    /// [3072, 5120] BF16
    pub linear0_weight_bf16: &'a [u16],
    /// [3072, 3072] BF16
    pub linear1_weight_bf16: &'a [u16],
}

pub fn adapter_forward_bf16<B: KernelBackend>(
    backend: &mut B,
    weights: AdapterWeights<'_>,
    enc_out: &[f32],
    enc_seq_len: usize,
) -> Result<Vec<f32>> {
    if enc_out.len() != enc_seq_len * VOX_ENC_DIM {
        return Err(Error::Shape("adapter enc_out"));
    }
    let out_seq = enc_seq_len / VOX_DOWNSAMPLE;
    let concat_dim = VOX_ENC_DIM * VOX_DOWNSAMPLE;
    if weights.linear0_weight_bf16.len() != VOX_DEC_DIM * concat_dim {
        return Err(Error::Shape("adapter linear0"));
    }
    if weights.linear1_weight_bf16.len() != VOX_DEC_DIM * VOX_DEC_DIM {
        return Err(Error::Shape("adapter linear1"));
    }

    let mut down = vec![0.0f32; out_seq * concat_dim];
    for t in 0..out_seq {
        for j in 0..VOX_DOWNSAMPLE {
            let src = (t * VOX_DOWNSAMPLE + j) * VOX_ENC_DIM;
            let dst = t * concat_dim + j * VOX_ENC_DIM;
            down[dst..dst + VOX_ENC_DIM].copy_from_slice(&enc_out[src..src + VOX_ENC_DIM]);
        }
    }

    let mut hidden = vec![0.0f32; out_seq * VOX_DEC_DIM];
    backend.linear_bf16(
        &mut hidden,
        &down,
        weights.linear0_weight_bf16,
        None,
        out_seq,
        concat_dim,
        VOX_DEC_DIM,
    )?;
    backend.gelu_inplace(&mut hidden)?;

    let mut out = vec![0.0f32; out_seq * VOX_DEC_DIM];
    backend.linear_bf16(
        &mut out,
        &hidden,
        weights.linear1_weight_bf16,
        None,
        out_seq,
        VOX_DEC_DIM,
        VOX_DEC_DIM,
    )?;
    Ok(out)
}
