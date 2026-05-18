//! Pure Rust CPU kernels. No BLAS, no SIMD intrinsics by default.
//! These are correctness/reference kernels; platform-specific implementations
//! should override the same trait methods with fused kernels.

use crate::bf16::bf16_to_f32;
use crate::kernels::{AttentionSpec, KernelBackend};
use crate::{Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct CpuBackend;

impl CpuBackend {
    #[inline]
    fn check_len(name: &'static str, got: usize, expected: usize) -> Result<()> {
        if got != expected {
            return Err(Error::Shape(name));
        }
        Ok(())
    }
}

impl KernelBackend for CpuBackend {
    fn name(&self) -> &'static str {
        "cpu-rust-reference"
    }

    fn add_inplace(&mut self, a: &mut [f32], b: &[f32]) -> Result<()> {
        Self::check_len("add_inplace", a.len(), b.len())?;
        for (aa, bb) in a.iter_mut().zip(b.iter().copied()) {
            *aa += bb;
        }
        Ok(())
    }

    fn mul_inplace(&mut self, a: &mut [f32], b: &[f32]) -> Result<()> {
        Self::check_len("mul_inplace", a.len(), b.len())?;
        for (aa, bb) in a.iter_mut().zip(b.iter().copied()) {
            *aa *= bb;
        }
        Ok(())
    }

    fn linear_f32(
        &mut self,
        y: &mut [f32],
        x: &[f32],
        w: &[f32],
        bias: Option<&[f32]>,
        seq_len: usize,
        in_dim: usize,
        out_dim: usize,
    ) -> Result<()> {
        Self::check_len("linear_f32 y", y.len(), seq_len * out_dim)?;
        Self::check_len("linear_f32 x", x.len(), seq_len * in_dim)?;
        Self::check_len("linear_f32 w", w.len(), out_dim * in_dim)?;
        if let Some(b) = bias {
            Self::check_len("linear_f32 bias", b.len(), out_dim)?;
        }

        for s in 0..seq_len {
            let x_row = &x[s * in_dim..(s + 1) * in_dim];
            let y_row = &mut y[s * out_dim..(s + 1) * out_dim];
            for o in 0..out_dim {
                let w_row = &w[o * in_dim..(o + 1) * in_dim];
                let mut sum = bias.map_or(0.0, |b| b[o]);
                for i in 0..in_dim {
                    sum += x_row[i] * w_row[i];
                }
                y_row[o] = sum;
            }
        }
        Ok(())
    }

    fn linear_bf16(
        &mut self,
        y: &mut [f32],
        x: &[f32],
        w_bf16: &[u16],
        bias: Option<&[f32]>,
        seq_len: usize,
        in_dim: usize,
        out_dim: usize,
    ) -> Result<()> {
        Self::check_len("linear_bf16 y", y.len(), seq_len * out_dim)?;
        Self::check_len("linear_bf16 x", x.len(), seq_len * in_dim)?;
        Self::check_len("linear_bf16 w", w_bf16.len(), out_dim * in_dim)?;
        if let Some(b) = bias {
            Self::check_len("linear_bf16 bias", b.len(), out_dim)?;
        }

        for s in 0..seq_len {
            let x_row = &x[s * in_dim..(s + 1) * in_dim];
            let y_row = &mut y[s * out_dim..(s + 1) * out_dim];
            for o in 0..out_dim {
                let w_row = &w_bf16[o * in_dim..(o + 1) * in_dim];
                let mut sum = bias.map_or(0.0, |b| b[o]);
                let mut i = 0;
                // Manual 4-way unroll. LLVM will vectorize on native targets.
                while i + 4 <= in_dim {
                    sum += bf16_to_f32(w_row[i]) * x_row[i]
                        + bf16_to_f32(w_row[i + 1]) * x_row[i + 1]
                        + bf16_to_f32(w_row[i + 2]) * x_row[i + 2]
                        + bf16_to_f32(w_row[i + 3]) * x_row[i + 3];
                    i += 4;
                }
                while i < in_dim {
                    sum += bf16_to_f32(w_row[i]) * x_row[i];
                    i += 1;
                }
                y_row[o] = sum;
            }
        }
        Ok(())
    }

    fn matmul_t_bf16(
        &mut self,
        c: &mut [f32],
        a: &[f32],
        b_bf16: &[u16],
        m: usize,
        k: usize,
        n: usize,
    ) -> Result<()> {
        Self::check_len("matmul_t_bf16 c", c.len(), m * n)?;
        Self::check_len("matmul_t_bf16 a", a.len(), m * k)?;
        Self::check_len("matmul_t_bf16 b", b_bf16.len(), n * k)?;
        for row in 0..m {
            let a_row = &a[row * k..(row + 1) * k];
            for col in 0..n {
                let b_row = &b_bf16[col * k..(col + 1) * k];
                let mut sum = 0.0f32;
                for i in 0..k {
                    sum += a_row[i] * bf16_to_f32(b_row[i]);
                }
                c[row * n + col] = sum;
            }
        }
        Ok(())
    }

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
    ) -> Result<()> {
        let out_length = (length + 2 * padding - kernel_size) / stride + 1;
        Self::check_len("conv1d input", input.len(), channels_in * length)?;
        Self::check_len(
            "conv1d weight",
            weight.len(),
            channels_out * channels_in * kernel_size,
        )?;
        Self::check_len("conv1d out", out.len(), channels_out * out_length)?;
        if let Some(b) = bias {
            Self::check_len("conv1d bias", b.len(), channels_out)?;
        }

        for oc in 0..channels_out {
            let b = bias.map_or(0.0, |bb| bb[oc]);
            for ol in 0..out_length {
                let mut sum = b;
                for ic in 0..channels_in {
                    for kk in 0..kernel_size {
                        let il_signed =
                            ol as isize * stride as isize - padding as isize + kk as isize;
                        if il_signed >= 0 && (il_signed as usize) < length {
                            let il = il_signed as usize;
                            let w_idx = oc * channels_in * kernel_size + ic * kernel_size + kk;
                            sum += input[ic * length + il] * weight[w_idx];
                        }
                    }
                }
                out[oc * out_length + ol] = sum;
            }
        }
        Ok(())
    }

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
    ) -> Result<()> {
        let padding_total = kernel_size - stride;
        let n_frames =
            (length as f32 - kernel_size as f32 + padding_total as f32) / stride as f32 + 1.0;
        let out_length = n_frames.ceil().max(0.0) as usize;
        Self::check_len("causal_conv1d out", out.len(), channels_out * out_length)?;
        let left_pad = padding_total as isize;
        for oc in 0..channels_out {
            let b = bias.map_or(0.0, |bb| bb[oc]);
            for ol in 0..out_length {
                let mut sum = b;
                for ic in 0..channels_in {
                    for kk in 0..kernel_size {
                        let il_signed = ol as isize * stride as isize - left_pad + kk as isize;
                        if il_signed >= 0 && (il_signed as usize) < length {
                            let il = il_signed as usize;
                            let w_idx = oc * channels_in * kernel_size + ic * kernel_size + kk;
                            sum += input[ic * length + il] * weight[w_idx];
                        }
                    }
                }
                out[oc * out_length + ol] = sum;
            }
        }
        Ok(())
    }

    fn rms_norm(
        &mut self,
        out: &mut [f32],
        x: &[f32],
        weight: &[f32],
        seq_len: usize,
        hidden: usize,
        eps: f32,
    ) -> Result<()> {
        Self::check_len("rms_norm out", out.len(), seq_len * hidden)?;
        Self::check_len("rms_norm x", x.len(), seq_len * hidden)?;
        Self::check_len("rms_norm weight", weight.len(), hidden)?;
        for s in 0..seq_len {
            let xr = &x[s * hidden..(s + 1) * hidden];
            let yr = &mut out[s * hidden..(s + 1) * hidden];
            let mut sum_sq = 0.0f32;
            for &v in xr {
                sum_sq += v * v;
            }
            let inv = 1.0 / (sum_sq / hidden as f32 + eps).sqrt();
            for i in 0..hidden {
                yr[i] = xr[i] * inv * weight[i];
            }
        }
        Ok(())
    }

    fn silu_inplace(&mut self, x: &mut [f32]) -> Result<()> {
        for v in x {
            *v = *v / (1.0 + (-*v).exp());
        }
        Ok(())
    }

    fn gelu_inplace(&mut self, x: &mut [f32]) -> Result<()> {
        for v in x {
            let x3 = *v * *v * *v;
            let inner = 0.797_884_6 * (*v + 0.044_715 * x3);
            *v = 0.5 * *v * (1.0 + inner.tanh());
        }
        Ok(())
    }

    fn softmax_rows_inplace(&mut self, x: &mut [f32], rows: usize, cols: usize) -> Result<()> {
        Self::check_len("softmax", x.len(), rows * cols)?;
        for r in 0..rows {
            let row = &mut x[r * cols..(r + 1) * cols];
            let mut max_v = f32::NEG_INFINITY;
            for &v in row.iter() {
                max_v = max_v.max(v);
            }
            let mut sum = 0.0;
            for v in row.iter_mut() {
                *v = (*v - max_v).exp();
                sum += *v;
            }
            let inv = 1.0 / sum;
            for v in row.iter_mut() {
                *v *= inv;
            }
        }
        Ok(())
    }

    fn causal_attention(
        &mut self,
        out: &mut [f32],
        q: &[f32],
        k: &[f32],
        v: &[f32],
        spec: AttentionSpec,
    ) -> Result<()> {
        let q_hidden = spec.n_heads * spec.head_dim;
        let kv_hidden = spec.n_kv_heads * spec.head_dim;
        Self::check_len("attention q", q.len(), spec.seq_q * q_hidden)?;
        Self::check_len("attention k", k.len(), spec.seq_k * kv_hidden)?;
        Self::check_len("attention v", v.len(), spec.seq_k * kv_hidden)?;
        Self::check_len("attention out", out.len(), spec.seq_q * q_hidden)?;
        if spec.n_heads % spec.n_kv_heads != 0 {
            return Err(Error::Shape("attention GQA ratio"));
        }
        let heads_per_kv = spec.n_heads / spec.n_kv_heads;

        for h in 0..spec.n_heads {
            let kv_h = h / heads_per_kv;
            for i in 0..spec.seq_q {
                let q_row =
                    &q[i * q_hidden + h * spec.head_dim..i * q_hidden + (h + 1) * spec.head_dim];
                let out_row = &mut out
                    [i * q_hidden + h * spec.head_dim..i * q_hidden + (h + 1) * spec.head_dim];
                out_row.fill(0.0);

                let global_pos = spec.q_offset + i;
                let k_start = if spec.window_size > 0 {
                    global_pos.saturating_sub(spec.window_size - 1)
                } else {
                    0
                };
                let mut k_end = global_pos + 1;
                if k_end > spec.seq_k {
                    k_end = spec.seq_k;
                }

                let mut max_score = f32::NEG_INFINITY;
                let mut sum_exp = 0.0f32;
                for j in k_start..k_end {
                    let k_row = &k[j * kv_hidden + kv_h * spec.head_dim
                        ..j * kv_hidden + (kv_h + 1) * spec.head_dim];
                    let v_row = &v[j * kv_hidden + kv_h * spec.head_dim
                        ..j * kv_hidden + (kv_h + 1) * spec.head_dim];
                    let mut score = 0.0f32;
                    for d in 0..spec.head_dim {
                        score += q_row[d] * k_row[d];
                    }
                    score *= spec.scale;

                    if score > max_score {
                        let correction = (max_score - score).exp();
                        sum_exp = sum_exp * correction + 1.0;
                        for d in 0..spec.head_dim {
                            out_row[d] = out_row[d] * correction + v_row[d];
                        }
                        max_score = score;
                    } else {
                        let weight = (score - max_score).exp();
                        sum_exp += weight;
                        for d in 0..spec.head_dim {
                            out_row[d] += weight * v_row[d];
                        }
                    }
                }
                if sum_exp > 0.0 {
                    let inv = 1.0 / sum_exp;
                    for d in 0..spec.head_dim {
                        out_row[d] *= inv;
                    }
                }
            }
        }
        Ok(())
    }

    fn rope_interleaved_inplace(
        &mut self,
        data: &mut [f32],
        n_heads: usize,
        head_dim: usize,
        position: usize,
        theta: f32,
    ) -> Result<()> {
        Self::check_len("rope data", data.len(), n_heads * head_dim)?;
        let half = head_dim / 2;
        for h in 0..n_heads {
            let base = h * head_dim;
            for i in 0..half {
                let inv_freq = theta.powf(-(i as f32) / (half as f32));
                let angle = position as f32 * inv_freq;
                let (sin, cos) = angle.sin_cos();
                let a = data[base + 2 * i];
                let b = data[base + 2 * i + 1];
                data[base + 2 * i] = a * cos - b * sin;
                data[base + 2 * i + 1] = a * sin + b * cos;
            }
        }
        Ok(())
    }

    fn argmax(&mut self, x: &[f32]) -> usize {
        let mut best_i = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for (i, &v) in x.iter().enumerate() {
            if v > best_v {
                best_v = v;
                best_i = i;
            }
        }
        best_i
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bf16::f32_to_bf16_round_nearest_even;

    #[test]
    fn linear_bf16_small() {
        let mut cpu = CpuBackend;
        let x = [1.0, 2.0, 3.0];
        let w_f = [1.0, 0.0, 0.5, -1.0, 2.0, 1.0]; // [2,3]
        let w: Vec<u16> = w_f
            .iter()
            .map(|&v| f32_to_bf16_round_nearest_even(v))
            .collect();
        let mut y = [0.0; 2];
        cpu.linear_bf16(&mut y, &x, &w, Some(&[0.25, -0.25]), 1, 3, 2)
            .unwrap();
        assert!((y[0] - 2.75).abs() < 1e-6);
        assert!((y[1] - 5.75).abs() < 1e-6);
    }

    #[test]
    fn rope_preserves_norm() {
        let mut cpu = CpuBackend;
        let mut x = vec![1.0, 2.0, 3.0, 4.0];
        let before: f32 = x.iter().map(|v| v * v).sum();
        cpu.rope_interleaved_inplace(&mut x, 1, 4, 7, 1_000_000.0)
            .unwrap();
        let after: f32 = x.iter().map(|v| v * v).sum();
        assert!((before - after).abs() < 1e-4);
    }

    #[test]
    fn attention_one_hot_value() {
        let mut cpu = CpuBackend;
        let q = [1.0, 0.0];
        let k = [10.0, 0.0];
        let v = [3.0, 4.0];
        let mut out = [0.0; 2];
        cpu.causal_attention(
            &mut out,
            &q,
            &k,
            &v,
            AttentionSpec {
                seq_q: 1,
                seq_k: 1,
                n_heads: 1,
                n_kv_heads: 1,
                head_dim: 2,
                scale: 1.0,
                window_size: 0,
                q_offset: 0,
            },
        )
        .unwrap();
        assert!((out[0] - 3.0).abs() < 1e-6);
        assert!((out[1] - 4.0).abs() < 1e-6);
    }
}
