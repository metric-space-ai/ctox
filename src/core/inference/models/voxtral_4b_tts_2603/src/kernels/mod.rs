//! Kernel abstraction.
//!
//! The Rust graph calls this trait. CPU implements it fully in Rust. GPU
//! backends should implement the same trait and dispatch to platform kernels.

pub mod cpu;

use crate::Result;

#[derive(Debug, Clone, Copy)]
pub struct AttentionSpec {
    pub seq_q: usize,
    pub seq_k: usize,
    pub n_heads: usize,
    pub n_kv_heads: usize,
    pub head_dim: usize,
    pub scale: f32,
    pub window_size: usize,
    pub q_offset: usize,
}

pub trait KernelBackend {
    fn name(&self) -> &'static str;

    fn add_inplace(&mut self, a: &mut [f32], b: &[f32]) -> Result<()>;
    fn mul_inplace(&mut self, a: &mut [f32], b: &[f32]) -> Result<()>;

    fn linear_f32(
        &mut self,
        y: &mut [f32],
        x: &[f32],
        w: &[f32],
        bias: Option<&[f32]>,
        seq_len: usize,
        in_dim: usize,
        out_dim: usize,
    ) -> Result<()>;

    fn linear_bf16(
        &mut self,
        y: &mut [f32],
        x: &[f32],
        w_bf16: &[u16],
        bias: Option<&[f32]>,
        seq_len: usize,
        in_dim: usize,
        out_dim: usize,
    ) -> Result<()>;

    fn matmul_t_bf16(
        &mut self,
        c: &mut [f32],
        a: &[f32],
        b_bf16: &[u16],
        m: usize,
        k: usize,
        n: usize,
    ) -> Result<()>;

    fn conv1d(
        &mut self,
        out: &mut [f32],
        input: &[f32],
        weight: &[f32],
        bias: Option<&[f32]>,
        channels_in: usize,
        channels_out: usize,
        length: usize,
        kernel_size: usize,
        stride: usize,
        padding: usize,
    ) -> Result<()>;

    fn causal_conv1d(
        &mut self,
        out: &mut [f32],
        input: &[f32],
        weight: &[f32],
        bias: Option<&[f32]>,
        channels_in: usize,
        channels_out: usize,
        length: usize,
        kernel_size: usize,
        stride: usize,
    ) -> Result<()>;

    fn rms_norm(
        &mut self,
        out: &mut [f32],
        x: &[f32],
        weight: &[f32],
        seq_len: usize,
        hidden: usize,
        eps: f32,
    ) -> Result<()>;
    fn silu_inplace(&mut self, x: &mut [f32]) -> Result<()>;
    fn gelu_inplace(&mut self, x: &mut [f32]) -> Result<()>;
    fn softmax_rows_inplace(&mut self, x: &mut [f32], rows: usize, cols: usize) -> Result<()>;

    fn causal_attention(
        &mut self,
        out: &mut [f32],
        q: &[f32],
        k: &[f32],
        v: &[f32],
        spec: AttentionSpec,
    ) -> Result<()>;

    fn rope_interleaved_inplace(
        &mut self,
        data: &mut [f32],
        n_heads: usize,
        head_dim: usize,
        position: usize,
        theta: f32,
    ) -> Result<()>;

    fn argmax(&mut self, x: &[f32]) -> usize;
}
