//! Work-buffer pool used by the forward pass.
//!
//! Every per-layer forward allocates a set of intermediate buffers
//! (post-norm, q-proj, k-proj, v-proj, attn-out, mlp-gate, mlp-up,
//! mlp-silu, mlp-prod) that can be shared across layers and across
//! time-steps because we consume them inside the layer and don't need
//! them afterwards. Pre-allocating them once per runtime saves a
//! huge amount of MTLBuffer churn in the decode loop.
//!
//! The pool is sized for the worst-case (max_tokens_per_step) plus
//! the intermediate dimension of the MLP and the concatenated QKV
//! width. It's reused for both prefill chunks and per-step decode.

use crate::metal::ffi::{Buffer, Device};

/// Bytes per bf16 element.
pub const BF16: usize = 2;
const SDPA_2PASS_MAX_BLOCKS: usize = 1024;
const SDPA_2PASS_MAX_Q: usize = 16;
pub const VERIFY_QMM_MAX_KPARTS: usize = 8;
pub const VERIFY_QMM_MAX_N: usize = 100_000;

pub struct WorkBuffers {
    pub max_tokens: i32,
    pub hidden: i32,
    pub intermediate: i32,
    pub n_q_features: i32,  // n_heads * head_dim
    pub n_kv_features: i32, // n_kv_heads * head_dim
    pub gdn_conv_channels: i32,
    pub gdn_d_inner: i32,
    pub gdn_dt_rank: i32,

    pub normed_x: Buffer,            // [max_tokens, hidden]   bf16
    pub q_proj_raw: Buffer,          // [max_tokens, 2 * n_q_features] for Qwen3.5 attention gate
    pub q_proj: Buffer,              // [max_tokens, n_q_features]
    pub q_tmp: Buffer,               // [max_tokens, n_q_features]
    pub q_gate: Buffer,              // [max_tokens, n_q_features]
    pub k_proj: Buffer,              // [max_tokens, n_kv_features]
    pub k_tmp: Buffer,               // [max_tokens, n_kv_features]
    pub v_proj: Buffer,              // [max_tokens, n_kv_features]
    pub attn_out: Buffer,            // [max_tokens, n_q_features]
    pub attn_2pass_partials: Buffer, // [heads, max_tokens, blocks, head_dim] bf16
    pub attn_2pass_sums: Buffer,     // [heads, max_tokens, blocks] f32
    pub attn_2pass_maxs: Buffer,     // [heads, max_tokens, blocks] f32
    pub mlp_gate: Buffer,            // [max_tokens, intermediate]
    pub mlp_up: Buffer,              // [max_tokens, intermediate]
    pub mlp_silu: Buffer,            // [max_tokens, intermediate]
    pub mlp_prod: Buffer,            // [max_tokens, intermediate]
    pub residual: Buffer,            // [max_tokens, hidden]
    pub verify_qmm_partials: Buffer, // [VERIFY_QMM_MAX_KPARTS, 16, VERIFY_QMM_MAX_N] f32

    // MoE-specific scratch. `moe_routed_prod` is [tokens, top_k, intermediate].
    pub moe_router_logits: Buffer, // [max_tokens, num_experts]
    pub moe_topk_ids: Buffer,      // [max_tokens, top_k] i32
    pub moe_topk_weights: Buffer,  // [max_tokens, top_k] bf16
    pub moe_lhs_token_ids: Buffer, // [max_tokens, top_k] i32
    pub moe_lhs_slot_ids: Buffer,  // [max_tokens, top_k] i32
    pub moe_gate: Buffer,          // [max_tokens, top_k, intermediate] bf16
    pub moe_up: Buffer,            // [max_tokens, top_k, intermediate] bf16
    pub moe_routed_prod: Buffer,   // [max_tokens, top_k, intermediate] bf16
    pub moe_down_slots: Buffer,    // [max_tokens, top_k, hidden] bf16
    pub moe_shared: Buffer,        // [max_tokens, hidden] bf16
    pub moe_shared_gate: Buffer,   // [max_tokens, 1] bf16

    // GDN-specific scratch.
    pub gdn_qkv_mixed: Buffer,  // [max_tokens, conv_channels]
    pub gdn_z: Buffer,          // [max_tokens, d_inner]
    pub gdn_beta: Buffer,       // [max_tokens, dt_rank]
    pub gdn_alpha: Buffer,      // [max_tokens, dt_rank]
    pub gdn_g: Buffer,          // [max_tokens, dt_rank]
    pub gdn_conv_input: Buffer, // [(kernel-1) + max_tokens, conv_channels]
    pub gdn_conv_out: Buffer,   // [max_tokens, conv_channels]
    pub gdn_q: Buffer,          // [max_tokens, Hk*Dk]  (before head repeat)
    pub gdn_k: Buffer,
    pub gdn_v: Buffer,     // [max_tokens, Hv*Dv]
    pub gdn_q_rep: Buffer, // [max_tokens, Hv*Dk]  (after repeat to Hv heads)
    pub gdn_k_rep: Buffer,
    pub gdn_delta_out: Buffer, // [max_tokens, Hv*Dv]
    pub gdn_state_tmp: Buffer, // [Hv, Dv, Dk] raw bf16 state scratch
    pub gdn_tape: Buffer,      // [max_tokens, Hv*Dv] f32
    pub gdn_output_n: Buffer,  // [max_tokens, Hv*Dv]
}

impl WorkBuffers {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dev: &Device,
        max_tokens: i32,
        hidden: i32,
        intermediate: i32,
        n_q_features: i32,
        n_kv_features: i32,
        gdn_conv_channels: i32,
        gdn_d_inner: i32,
        gdn_dt_rank: i32,
        _gdn_d_state: i32, // head_v_dim — reserved for future SSM-state sizing
        gdn_kernel_size: i32,
    ) -> Option<Self> {
        let mt = max_tokens as usize;
        let alloc = |n_cols: i32| -> Option<Buffer> {
            dev.new_buffer(mt * (n_cols as usize).max(1) * BF16)
        };
        let alloc_fixed = |bytes: usize| -> Option<Buffer> { dev.new_buffer(bytes.max(16)) };

        let conv_concat_rows = (gdn_kernel_size - 1).max(0) + max_tokens;
        let sdpa_head_dim = if n_q_features % 256 == 0 {
            256usize
        } else if n_q_features % 128 == 0 {
            128usize
        } else {
            (n_q_features as usize).max(1)
        };
        let sdpa_heads = (n_q_features as usize / sdpa_head_dim).max(1);

        Some(Self {
            max_tokens,
            hidden,
            intermediate,
            n_q_features,
            n_kv_features,
            gdn_conv_channels,
            gdn_d_inner,
            gdn_dt_rank,
            normed_x: alloc(hidden)?,
            q_proj_raw: alloc(n_q_features * 2)?,
            q_proj: alloc(n_q_features)?,
            q_tmp: alloc(n_q_features)?,
            q_gate: alloc(n_q_features)?,
            k_proj: alloc(n_kv_features)?,
            k_tmp: alloc(n_kv_features)?,
            v_proj: alloc(n_kv_features)?,
            attn_out: alloc(n_q_features)?,
            attn_2pass_partials: alloc_fixed(
                sdpa_heads * SDPA_2PASS_MAX_Q * SDPA_2PASS_MAX_BLOCKS * sdpa_head_dim * BF16,
            )?,
            attn_2pass_sums: alloc_fixed(
                sdpa_heads * SDPA_2PASS_MAX_Q * SDPA_2PASS_MAX_BLOCKS * std::mem::size_of::<f32>(),
            )?,
            attn_2pass_maxs: alloc_fixed(
                sdpa_heads * SDPA_2PASS_MAX_Q * SDPA_2PASS_MAX_BLOCKS * std::mem::size_of::<f32>(),
            )?,
            mlp_gate: alloc(intermediate)?,
            mlp_up: alloc(intermediate)?,
            mlp_silu: alloc(intermediate)?,
            mlp_prod: alloc(intermediate)?,
            residual: alloc(hidden)?,
            verify_qmm_partials: alloc_fixed(
                VERIFY_QMM_MAX_KPARTS * 16 * VERIFY_QMM_MAX_N * std::mem::size_of::<f32>(),
            )?,
            moe_router_logits: alloc(crate::common::constants::DFLASH35B_NUM_EXPERTS)?,
            moe_topk_ids: dev.new_buffer(
                mt * (crate::common::constants::DFLASH35B_EXPERTS_PER_TOK as usize)
                    * std::mem::size_of::<i32>(),
            )?,
            moe_topk_weights: alloc(crate::common::constants::DFLASH35B_EXPERTS_PER_TOK)?,
            moe_lhs_token_ids: dev.new_buffer(
                mt * (crate::common::constants::DFLASH35B_EXPERTS_PER_TOK as usize)
                    * std::mem::size_of::<i32>(),
            )?,
            moe_lhs_slot_ids: dev.new_buffer(
                mt * (crate::common::constants::DFLASH35B_EXPERTS_PER_TOK as usize)
                    * std::mem::size_of::<i32>(),
            )?,
            moe_gate: alloc(intermediate * crate::common::constants::DFLASH35B_EXPERTS_PER_TOK)?,
            moe_up: alloc(intermediate * crate::common::constants::DFLASH35B_EXPERTS_PER_TOK)?,
            moe_routed_prod: alloc(
                intermediate * crate::common::constants::DFLASH35B_EXPERTS_PER_TOK,
            )?,
            moe_down_slots: alloc(hidden * crate::common::constants::DFLASH35B_EXPERTS_PER_TOK)?,
            moe_shared: alloc(hidden)?,
            moe_shared_gate: alloc(1)?,

            gdn_qkv_mixed: alloc(gdn_conv_channels)?,
            gdn_z: alloc(gdn_d_inner)?,
            gdn_beta: alloc(gdn_dt_rank)?,
            gdn_alpha: alloc(gdn_dt_rank)?,
            gdn_g: alloc(gdn_dt_rank)?,
            gdn_conv_input: alloc_fixed(
                (conv_concat_rows as usize) * (gdn_conv_channels as usize) * BF16,
            )?,
            gdn_conv_out: alloc(gdn_conv_channels)?,
            gdn_q: alloc(gdn_d_inner / 2)?,
            gdn_k: alloc(gdn_d_inner / 2)?,
            gdn_v: alloc(gdn_d_inner)?,
            gdn_q_rep: alloc(gdn_d_inner)?,
            gdn_k_rep: alloc(gdn_d_inner)?,
            gdn_delta_out: alloc(gdn_d_inner)?,
            gdn_state_tmp: alloc_fixed(
                (gdn_d_inner as usize).max(1) * 128 * std::mem::size_of::<f32>(),
            )?,
            gdn_tape: alloc_fixed((mt) * (gdn_d_inner as usize) * std::mem::size_of::<f32>())?,
            gdn_output_n: alloc(gdn_d_inner)?,
        })
    }
}
