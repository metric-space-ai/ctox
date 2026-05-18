// Origin: CTOX
// License: Apache-2.0

//! Per-layer block driver.
//!
//! Three block types live here:
//! - `forward_full_attention_block` — RMSNorm → Q/K/V proj → M-RoPE
//!   → softmax-SDPA → attn_output_gate → O proj → residual add
//! - `forward_linear_attention_block` — RMSNorm → Q/K/V/gate/beta proj
//!   → ssm_conv on Q/K/V → gated_delta_net → O proj → residual add
//! - `forward_moe_ffn_block` — RMSNorm → router (Rust) → 8 indexed
//!   gate/up/down matmuls → SwiGLU → weighted-sum → residual add
//!
//! All three commit ONE `MTLCommandBuffer` per call, chaining
//! kernel dispatches without per-kernel waitUntilCompleted. This is
//! where the 3.4× persistent-buffer win compounds across the 5-9
//! kernels per layer-block.

#![cfg(feature = "metal")]

use anyhow::Result;

use super::session::Session;

/// One full-attention layer (10 of these per Qwen3.6 forward pass).
///
/// Stage-4 wakeup #2 fills this in. Inputs:
/// - `residual`: name in `Session::weights` of the current residual buffer
/// - `layer_idx`: 0..40 (used to locate weights via `weights.buf("blk.{n}.attn_norm.weight")` etc.)
/// - `pos`: KV-cache position to write into
///
/// Currently a sketch of the dispatch order.
pub fn forward_full_attention_block(
    _session: &Session,
    _layer_idx: usize,
    _pos: usize,
) -> Result<()> {
    // 1. RMSNorm pre-attn:
    //    enc.dispatch(rms_norm,  residual → norm_temp,  weight=blk.{l}.attn_norm.weight)
    // 2. Q proj:  enc.dispatch(mul_mv,  norm_temp × Q_w → q_buf)   (ne0=4096, ne00=2048)
    // 3. K proj:  enc.dispatch(mul_mv,  norm_temp × K_w → k_buf)   (ne0=512,  ne00=2048)
    // 4. V proj:  enc.dispatch(mul_mv,  norm_temp × V_w → v_buf)   (ne0=512,  ne00=2048)
    // 5. M-RoPE on q_buf, k_buf (kernel_rope_*; need a Rust dispatcher
    //    similar to ssm_conv — vendored kernel exists at
    //    vendor/ggml-metal/ggml-metal.metal — wire in next wakeup)
    // 6. Append k_buf, v_buf to KV cache at `pos`
    // 7. Softmax SDPA: kernel_mul_mm or flash_attn_ext
    //                  (sweep showed -fa hurts under llama.cpp routing;
    //                  isolated mul_mm path may differ — bench both)
    // 8. attn_output_gate (Qwen3.6 has attn_output_gate=true):
    //    sigmoid(gate_buf) * attn_out (need one-shot fused kernel —
    //    available as kernel_mul + kernel_unary_silu chain or fused)
    // 9. O proj:  enc.dispatch(mul_mv, gated_attn × O_w → o_buf)
    // 10. residual += o_buf  (kernel_add or fused)
    //
    // Then the COMMAND BUFFER commits ONCE.
    todo!("Stage-4 wakeup #2: implement full-attention block dispatch chain")
}

/// One linear-attention layer (30 of these per forward pass).
///
/// Stage-4 wakeup #3 fills this in.
pub fn forward_linear_attention_block(
    _session: &Session,
    _layer_idx: usize,
    _pos: usize,
) -> Result<()> {
    // 1. RMSNorm pre-attn
    // 2. Q proj  (ne0 = num_q_heads * S_v   = 16 * 128 = 2048)
    // 3. K proj  (ne0 = num_k_heads * S_v   = 16 * 128 = 2048)
    // 4. V proj  (ne0 = num_v_heads * S_v   = 32 * 128 = 4096)
    // 5. gate proj (ne0 = num_v_heads * G   = 32 * 1   = 32)
    // 6. beta proj (ne0 = num_v_heads       = 32)
    // 7. ssm_conv on Q/K/V (kernel_dim=4, vec4 path)
    //    → keep last-4-tokens window per row in recurrent_state
    // 8. gated_delta_net dispatch (553 µs measured for this shape)
    //    → reads/writes recurrent_state slot for this layer
    //    → produces attention output [n_v_heads, S_v]
    // 9. O proj   (ne0 = hidden = 2048)
    // 10. residual += o_buf
    todo!("Stage-4 wakeup #3: implement linear-attention block dispatch chain")
}

/// Per-layer MoE FFN (after either attention block).
///
/// Stage-4 wakeup #4 fills this in.
pub fn forward_moe_ffn_block(_session: &Session, _layer_idx: usize) -> Result<()> {
    // 1. RMSNorm pre-FFN
    // 2. Router proj: input × router_w → [n_experts=256] f32
    // 3. CPU softmax + top-8 → expert_ids [8] int32, expert_weights [8] f32
    //    (router_softmax_top_k from metal_port::ops::moe_router)
    // 4. mul_mv_id × 3 (gate, up, down):
    //    a. gate_out [8, intermediate=512] = mul_mv_id(gate_w, input, ids)
    //    b. up_out   [8, intermediate=512] = mul_mv_id(up_w,   input, ids)
    //    c. activated[8, intermediate=512] = silu(gate_out) * up_out  (SwiGLU)
    //    d. down_out [8, hidden=2048]      = mul_mv_id(down_w, activated, ids)
    // 5. Shared-expert path: same as 4a-d but no routing (single expert)
    // 6. Weighted-sum across the 8 routed slots + add shared:
    //    residual += Σ_slot weights[slot] * down_out[slot] + shared_out
    todo!("Stage-4 wakeup #4: implement MoE FFN block dispatch chain")
}

/// One full token forward pass: 40 layers + LM head + sample.
///
/// Stage-4 wakeup #5 wires the layers + LM head + sampling.
pub fn forward_token(_session: &Session, _input_token_id: u32, _pos: usize) -> Result<u32> {
    // 1. embedding lookup: get_rows_q4_K(token_embedding, [token_id]) → residual [hidden]
    // 2. for layer in 0..40:
    //      if LAYER_TYPES[layer] == FullAttention:
    //          forward_full_attention_block(session, layer, pos)
    //      else:
    //          forward_linear_attention_block(session, layer, pos)
    //      forward_moe_ffn_block(session, layer)
    // 3. final RMSNorm
    // 4. LM head: mul_mv (or mul_mm at prefill) → logits [vocab=248320]
    // 5. on-GPU sample (kernel_argmax for greedy — vendored)
    // 6. return next_token_id
    todo!("Stage-4 wakeup #5: wire the full token loop")
}
