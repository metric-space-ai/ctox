//! Mixture-of-Experts (MoE) block used by every Qwen3.5-35B-A3B layer.
//!
//! Replaces the dense MLP of the 27B path:
//!
//!   router_logits = router(hidden_states)    // [T, num_experts]
//!   top_k_idx, top_k_weights = topk(softmax(router_logits), k=EXPERTS_PER_TOK)
//!   // For each token t, only `k` experts contribute:
//!   mlp_out[t] = sum over j in 0..k of
//!                  top_k_weights[t, j] * experts[top_k_idx[t, j]](hidden_states[t])
//!
//! Each expert is a standard SwiGLU MLP with the same shape trio
//! `(gate_proj, up_proj, down_proj)` as the dense case, at
//! `intermediate = MOE_INTERMEDIATE` (1408 for this model).
//!
//! # Status
//!
//! Skeleton. Populated once the `moe_route_topk` and `moe_expert_apply`
//! Metal shaders land. The dispatch shape and router gating formula
//! here match `mlx_lm.models.qwen3_moe` at commit pinned in
//! `vendor/metal/dflash-mlx.version`.
//!
//! ref: `mlx_lm/models/qwen3_moe.py` (upstream, not vendored)
//! ref: `dflash_mlx/runtime.py::_target_text_model` (detection only;
//!       the MoE block itself isn't touched by the DFlash runtime
//!       because every expert-routed linear in the model is already
//!       handled by `VerifyQuantizedLinear` at the nn-tree rewrite
//!       step).

use crate::metal::ffi::{Buffer, ComputeEncoder, Device};
use crate::metal::kernels;
use crate::metal::mlx_ops::{self, MlxDtype};
use crate::metal::qwen::{Linear4Bit, Mlp};

/// One stacked expert projection. The MLX checkpoint stores routed
/// experts as 3-D affine-4bit tensors:
/// `[num_experts, out_features, in_features / 8]`.
pub struct ExpertLinear4Bit {
    pub w_q: Buffer,
    pub scales: Buffer,
    pub biases: Buffer,
    pub num_experts: i32,
    pub in_features: i32,
    pub out_features: i32,
}

/// Routed expert set: gate/up/down are stacked by expert index.
pub struct ExpertSet {
    pub gate: ExpertLinear4Bit,
    pub up: ExpertLinear4Bit,
    pub down: ExpertLinear4Bit,
}

pub struct MoeBlock {
    /// Router projection: hidden → num_experts. In the MLX 4-bit export
    /// this is `mlp.gate`.
    pub router: Linear4Bit,
    /// Routed experts, stacked by expert index under `mlp.switch_mlp.*`.
    pub experts: ExpertSet,
    /// Shared dense expert.
    pub shared_expert: Mlp,
    /// Shared-expert scalar gate: hidden → 1.
    pub shared_expert_gate: Linear4Bit,
    /// Hyperparameters.
    pub hidden: i32,
    pub num_experts: i32,
    pub experts_per_tok: i32,
    pub moe_intermediate: i32,
}

impl MoeBlock {
    #[allow(clippy::too_many_arguments)]
    fn gather_expert_linear(
        enc: &ComputeEncoder,
        dev: &Device,
        linear: &ExpertLinear4Bit,
        x: &Buffer,
        x_row_stride: i64,
        lhs_indices: &Buffer,
        rhs_indices: &Buffer,
        y: &Buffer,
        batch: i32,
    ) -> bool {
        let k = linear.in_features;
        let n = linear.out_features;
        let w_expert_stride = i64::from(n) * i64::from(k / 8);
        let sb_expert_stride = i64::from(n) * i64::from(k / 64);
        mlx_ops::op_gather_qmv(
            enc,
            dev,
            MlxDtype::Bf16,
            &linear.w_q,
            w_expert_stride,
            &linear.scales,
            sb_expert_stride,
            &linear.biases,
            sb_expert_stride,
            x,
            x_row_stride,
            lhs_indices,
            rhs_indices,
            y,
            batch,
            n,
            k,
            64,
            4,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        normed_x: &Buffer,
        y: &Buffer,
        n_tokens: i32,
        work: &crate::metal::work::WorkBuffers,
    ) -> bool {
        let moe_m1_qmm = n_tokens == 1 && std::env::var_os("CTOX_METAL_MOE_M1_QMM").is_some();
        let router_ok = if moe_m1_qmm {
            self.router
                .forward_qmm_t(enc, dev, normed_x, &work.moe_router_logits, n_tokens)
        } else {
            self.router.forward_with_verify_scratch(
                enc,
                dev,
                normed_x,
                &work.moe_router_logits,
                n_tokens,
                Some(&work.verify_qmm_partials),
            )
        };
        if !router_ok {
            return false;
        }
        if !kernels::moe_route_topk_bf16(
            enc,
            dev,
            &work.moe_router_logits,
            &work.moe_topk_ids,
            &work.moe_topk_weights,
            n_tokens,
            self.num_experts,
            self.experts_per_tok,
        ) {
            return false;
        }

        let routed_batch = n_tokens * self.experts_per_tok;
        if !kernels::moe_fill_gather_indices_i32(
            enc,
            dev,
            &work.moe_lhs_token_ids,
            &work.moe_lhs_slot_ids,
            n_tokens,
            self.experts_per_tok,
        ) {
            return false;
        }

        if std::env::var_os("CTOX_METAL_MOE_CUSTOM").is_some() {
            if !kernels::moe_expert_gate_up_bf16(
                enc,
                dev,
                normed_x,
                &self.experts.gate,
                &self.experts.up,
                &work.moe_topk_ids,
                &work.moe_routed_prod,
                n_tokens,
                self.experts_per_tok,
                self.hidden,
                self.moe_intermediate,
            ) {
                return false;
            }
            if !kernels::moe_expert_down_accum_bf16(
                enc,
                dev,
                &work.moe_routed_prod,
                &self.experts.down,
                &work.moe_topk_ids,
                &work.moe_topk_weights,
                y,
                n_tokens,
                self.experts_per_tok,
                self.hidden,
                self.moe_intermediate,
            ) {
                return false;
            }
            if !self.shared_expert.forward(
                enc,
                dev,
                normed_x,
                &work.moe_shared,
                n_tokens,
                &work.mlp_gate,
                &work.mlp_up,
                &work.mlp_silu,
                &work.mlp_prod,
                Some(&work.verify_qmm_partials),
            ) {
                return false;
            }
            let shared_gate_ok = if moe_m1_qmm {
                self.shared_expert_gate.forward_qmm_t(
                    enc,
                    dev,
                    normed_x,
                    &work.moe_shared_gate,
                    n_tokens,
                )
            } else {
                self.shared_expert_gate.forward_with_verify_scratch(
                    enc,
                    dev,
                    normed_x,
                    &work.moe_shared_gate,
                    n_tokens,
                    Some(&work.verify_qmm_partials),
                )
            };
            if !shared_gate_ok {
                return false;
            }
            return kernels::moe_add_shared_bf16(
                enc,
                dev,
                y,
                &work.moe_shared,
                &work.moe_shared_gate,
                n_tokens,
                self.hidden,
            );
        }

        if !Self::gather_expert_linear(
            enc,
            dev,
            &self.experts.gate,
            normed_x,
            i64::from(self.hidden),
            &work.moe_lhs_token_ids,
            &work.moe_topk_ids,
            &work.moe_gate,
            routed_batch,
        ) {
            return false;
        }
        if !Self::gather_expert_linear(
            enc,
            dev,
            &self.experts.up,
            normed_x,
            i64::from(self.hidden),
            &work.moe_lhs_token_ids,
            &work.moe_topk_ids,
            &work.moe_up,
            routed_batch,
        ) {
            return false;
        }
        if !kernels::silu_mul_bf16(
            enc,
            dev,
            &work.moe_gate,
            &work.moe_up,
            &work.moe_routed_prod,
            routed_batch * self.moe_intermediate,
        ) {
            return false;
        }
        if !Self::gather_expert_linear(
            enc,
            dev,
            &self.experts.down,
            &work.moe_routed_prod,
            i64::from(self.moe_intermediate),
            &work.moe_lhs_slot_ids,
            &work.moe_topk_ids,
            &work.moe_down_slots,
            routed_batch,
        ) {
            return false;
        }
        if !kernels::moe_accum_weighted_bf16(
            enc,
            dev,
            &work.moe_down_slots,
            &work.moe_topk_weights,
            y,
            n_tokens,
            self.experts_per_tok,
            self.hidden,
        ) {
            return false;
        }
        if std::env::var_os("CTOX_METAL_MOE_DISABLE_SHARED").is_some() {
            return true;
        }

        if !self.shared_expert.forward(
            enc,
            dev,
            normed_x,
            &work.moe_shared,
            n_tokens,
            &work.mlp_gate,
            &work.mlp_up,
            &work.mlp_silu,
            &work.mlp_prod,
            Some(&work.verify_qmm_partials),
        ) {
            return false;
        }
        let shared_gate_ok = if moe_m1_qmm {
            self.shared_expert_gate.forward_qmm_t(
                enc,
                dev,
                normed_x,
                &work.moe_shared_gate,
                n_tokens,
            )
        } else {
            self.shared_expert_gate.forward_with_verify_scratch(
                enc,
                dev,
                normed_x,
                &work.moe_shared_gate,
                n_tokens,
                Some(&work.verify_qmm_partials),
            )
        };
        if !shared_gate_ok {
            return false;
        }
        kernels::moe_add_shared_bf16(
            enc,
            dev,
            y,
            &work.moe_shared,
            &work.moe_shared_gate,
            n_tokens,
            self.hidden,
        )
    }
}
