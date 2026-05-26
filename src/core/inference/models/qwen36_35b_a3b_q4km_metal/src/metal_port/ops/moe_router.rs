// Origin: CTOX
// License: AGPL-3.0-only

//! MoE top-k router. Pure Rust because the routing logic itself is
//! a softmax + top-k pick over a 256-wide vector — totally CPU-friendly
//! at decode batch=1, GPU-friendly only via a custom kernel which we
//! don't need yet.
//!
//! The expensive part — the per-expert FFN matmul — uses the vendored
//! `kernel_mul_mv_id_q4_K_f32` (or `kernel_mul_mm_id_q4_K_f32` for
//! prefill batches), which takes the `ids` buffer this module produces.

use std::cmp::Ordering;

/// Softmax over `n_experts` logits + top-k pick. Returns
/// `(top_k_indices: [k], top_k_weights: [k])` where the weights are
/// renormalised so they sum to 1 — that's the convention Qwen3.6
/// uses (`norm_topk_prob = true` per the frozen kernel ABI in
/// [crate::model::QWEN36_35B_A3B_TEXT_CONFIG]).
///
/// `out_idx` and `out_w` must have length ≥ `k`.
pub fn router_softmax_top_k(
    logits: &[f32],
    k: usize,
    out_idx: &mut [u32],
    out_w: &mut [f32],
) {
    debug_assert!(out_idx.len() >= k);
    debug_assert!(out_w.len() >= k);
    debug_assert!(k <= logits.len());

    // Numerically stable softmax: subtract max before exp.
    let mut max = f32::NEG_INFINITY;
    for &v in logits {
        if v > max {
            max = v;
        }
    }
    let mut probs: Vec<f32> = logits.iter().map(|&v| (v - max).exp()).collect();
    let sum: f32 = probs.iter().sum();
    if sum > 0.0 {
        for v in probs.iter_mut() {
            *v /= sum;
        }
    }

    // Top-k by partial sort (small k, so a partial selection is fine).
    let mut idx: Vec<u32> = (0..probs.len() as u32).collect();
    idx.sort_unstable_by(|&a, &b| {
        probs[b as usize]
            .partial_cmp(&probs[a as usize])
            .unwrap_or(Ordering::Equal)
    });

    // Renormalise the top-k weights so they sum to 1 (Qwen3.6 convention).
    let mut topk_sum = 0.0f32;
    for i in 0..k {
        let e = idx[i] as usize;
        topk_sum += probs[e];
    }
    let inv = if topk_sum > 0.0 { 1.0 / topk_sum } else { 0.0 };

    for i in 0..k {
        let e = idx[i];
        out_idx[i] = e;
        out_w[i] = probs[e as usize] * inv;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renormalised_weights_sum_to_one() {
        let logits = [1.0, 2.0, 0.5, 3.0, -1.0, 0.0, 4.0, 2.5];
        let mut idx = [0u32; 4];
        let mut w = [0.0f32; 4];
        router_softmax_top_k(&logits, 4, &mut idx, &mut w);
        let sum: f32 = w.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        // Top index should be the argmax (4.0 at position 6).
        assert_eq!(idx[0], 6);
    }

    #[test]
    fn picks_correct_top_k_order() {
        let logits = [10.0, 1.0, 5.0, 2.0, 8.0];
        let mut idx = [0u32; 3];
        let mut w = [0.0f32; 3];
        router_softmax_top_k(&logits, 3, &mut idx, &mut w);
        assert_eq!(idx[0], 0);
        assert_eq!(idx[1], 4);
        assert_eq!(idx[2], 2);
    }
}
