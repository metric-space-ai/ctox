use crate::common::{EmbeddingError, EmbeddingResult, PoolingMode};

pub fn rms_norm_rows(
    input: &[f32],
    rows: usize,
    dim: usize,
    weight: &[f32],
    eps: f32,
) -> EmbeddingResult<Vec<f32>> {
    if rows == 0 || dim == 0 || input.len() != rows * dim || weight.len() != dim {
        return Err(EmbeddingError::InvalidShape {
            detail: format!(
                "rms_norm shape mismatch: input={}, rows={rows}, dim={dim}, weight={}",
                input.len(),
                weight.len()
            ),
        });
    }
    let mut out = vec![0.0_f32; input.len()];
    for row in 0..rows {
        let offset = row * dim;
        let sum_sq = input[offset..offset + dim]
            .iter()
            .map(|value| value * value)
            .sum::<f32>();
        let inv = 1.0_f32 / (sum_sq / dim as f32 + eps).sqrt();
        for col in 0..dim {
            out[offset + col] = input[offset + col] * inv * weight[col];
        }
    }
    Ok(out)
}

pub fn linear_no_bias(
    input: &[f32],
    rows: usize,
    in_dim: usize,
    weight: &[f32],
    out_dim: usize,
) -> EmbeddingResult<Vec<f32>> {
    if rows == 0
        || in_dim == 0
        || out_dim == 0
        || input.len() != rows * in_dim
        || weight.len() != out_dim * in_dim
    {
        return Err(EmbeddingError::InvalidShape {
            detail: format!(
                "linear shape mismatch: input={}, rows={rows}, in_dim={in_dim}, weight={}, out_dim={out_dim}",
                input.len(),
                weight.len()
            ),
        });
    }
    let mut out = vec![0.0_f32; rows * out_dim];
    for row in 0..rows {
        for out_col in 0..out_dim {
            let mut acc = 0.0_f32;
            for in_col in 0..in_dim {
                acc += input[row * in_dim + in_col] * weight[out_col * in_dim + in_col];
            }
            out[row * out_dim + out_col] = acc;
        }
    }
    Ok(out)
}

pub fn silu_in_place(values: &mut [f32]) {
    for value in values {
        *value /= 1.0_f32 + (-*value).exp();
    }
}

pub fn mul_elementwise(a: &[f32], b: &[f32]) -> EmbeddingResult<Vec<f32>> {
    if a.len() != b.len() {
        return Err(EmbeddingError::InvalidShape {
            detail: format!("elementwise mul shape mismatch: {} vs {}", a.len(), b.len()),
        });
    }
    Ok(a.iter().zip(b).map(|(lhs, rhs)| lhs * rhs).collect())
}

pub fn softmax_in_place(values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0_f32;
    for value in values.iter_mut() {
        *value = (*value - max).exp();
        sum += *value;
    }
    if sum > 0.0 {
        for value in values {
            *value /= sum;
        }
    }
}

pub fn apply_rope_in_place(
    values: &mut [f32],
    positions: &[usize],
    heads: usize,
    head_dim: usize,
    theta: f32,
) -> EmbeddingResult<()> {
    if heads == 0 || head_dim == 0 || head_dim % 2 != 0 {
        return Err(EmbeddingError::InvalidShape {
            detail: "heads must be positive and head_dim must be positive/even".to_string(),
        });
    }
    let tokens = positions.len();
    if values.len() != tokens * heads * head_dim {
        return Err(EmbeddingError::InvalidShape {
            detail: format!(
                "rope shape mismatch: values={}, tokens={tokens}, heads={heads}, head_dim={head_dim}",
                values.len()
            ),
        });
    }
    for (token, position) in positions.iter().copied().enumerate() {
        for head in 0..heads {
            let base = (token * heads + head) * head_dim;
            for pair in 0..head_dim / 2 {
                let even = base + pair;
                let odd = base + pair + head_dim / 2;
                let freq = theta.powf(-2.0_f32 * pair as f32 / head_dim as f32);
                let angle = position as f32 * freq;
                let (sin, cos) = angle.sin_cos();
                let x0 = values[even];
                let x1 = values[odd];
                values[even] = x0 * cos - x1 * sin;
                values[odd] = x0 * sin + x1 * cos;
            }
        }
    }
    Ok(())
}

pub fn pool_hidden_states(
    hidden_states: &[f32],
    batch: usize,
    seq_len: usize,
    dim: usize,
    mode: PoolingMode,
) -> EmbeddingResult<Vec<Vec<f32>>> {
    if batch == 0 || seq_len == 0 || dim == 0 {
        return Err(EmbeddingError::InvalidShape {
            detail: "batch, seq_len, and dim must be positive".to_string(),
        });
    }
    let expected = batch
        .checked_mul(seq_len)
        .and_then(|value| value.checked_mul(dim))
        .ok_or_else(|| EmbeddingError::InvalidShape {
            detail: "hidden-state shape overflows usize".to_string(),
        })?;
    if hidden_states.len() != expected {
        return Err(EmbeddingError::InvalidShape {
            detail: format!("expected {expected} values, got {}", hidden_states.len()),
        });
    }

    let mut pooled = Vec::with_capacity(batch);
    for batch_index in 0..batch {
        let batch_offset = batch_index * seq_len * dim;
        let mut vector = vec![0.0_f32; dim];
        match mode {
            PoolingMode::LastToken => {
                let start = batch_offset + (seq_len - 1) * dim;
                vector.copy_from_slice(&hidden_states[start..start + dim]);
            }
            PoolingMode::Mean => {
                for token_index in 0..seq_len {
                    let start = batch_offset + token_index * dim;
                    for dim_index in 0..dim {
                        vector[dim_index] += hidden_states[start + dim_index];
                    }
                }
                let scale = 1.0_f32 / seq_len as f32;
                for value in &mut vector {
                    *value *= scale;
                }
            }
        }
        pooled.push(vector);
    }
    Ok(pooled)
}

pub fn l2_normalize_batch(vectors: &mut [Vec<f32>]) {
    for vector in vectors {
        let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in vector {
                *value /= norm;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pools_last_token() {
        let hidden = vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];
        let pooled = pool_hidden_states(&hidden, 2, 2, 3, PoolingMode::LastToken).unwrap();
        assert_eq!(pooled, vec![vec![4.0, 5.0, 6.0], vec![10.0, 11.0, 12.0]]);
    }

    #[test]
    fn pools_mean() {
        let hidden = vec![1.0, 3.0, 5.0, 7.0];
        let pooled = pool_hidden_states(&hidden, 1, 2, 2, PoolingMode::Mean).unwrap();
        assert_eq!(pooled, vec![vec![3.0, 5.0]]);
    }

    #[test]
    fn normalizes_vectors() {
        let mut vectors = vec![vec![3.0, 4.0], vec![0.0, 0.0]];
        l2_normalize_batch(&mut vectors);
        assert!((vectors[0][0] - 0.6).abs() < 1e-6);
        assert!((vectors[0][1] - 0.8).abs() < 1e-6);
        assert_eq!(vectors[1], vec![0.0, 0.0]);
    }

    #[test]
    fn transformer_primitives_match_reference_shapes() {
        let out = rms_norm_rows(&[3.0, 4.0], 1, 2, &[1.0, 2.0], 0.0).unwrap();
        let inv = 1.0_f32 / ((9.0_f32 + 16.0_f32) / 2.0_f32).sqrt();
        assert!((out[0] - 3.0 * inv).abs() < 1e-6);
        assert!((out[1] - 4.0 * inv * 2.0).abs() < 1e-6);

        let linear =
            linear_no_bias(&[1.0, 2.0, 3.0, 4.0], 2, 2, &[10.0, 1.0, 1.0, 10.0], 2).unwrap();
        assert_eq!(linear, vec![12.0, 21.0, 34.0, 43.0]);

        let mut activation = vec![0.0, 1.0];
        silu_in_place(&mut activation);
        assert_eq!(activation[0], 0.0);
        assert!((activation[1] - 0.7310586).abs() < 1e-6);
        assert_eq!(
            mul_elementwise(&[2.0, 3.0], &[4.0, 5.0]).unwrap(),
            vec![8.0, 15.0]
        );

        let mut logits = vec![1000.0, 1000.0];
        softmax_in_place(&mut logits);
        assert!((logits[0] - 0.5).abs() < 1e-6);
        assert!((logits[1] - 0.5).abs() < 1e-6);

        let mut rope = vec![1.0, 2.0, 3.0, 4.0];
        apply_rope_in_place(&mut rope, &[0], 1, 4, 1_000_000.0).unwrap();
        assert_eq!(rope, vec![1.0, 2.0, 3.0, 4.0]);
    }
}
