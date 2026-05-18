//! DFlash runtime — port of `dflash_mlx/runtime.py`.
//!
//! Owns the end-to-end orchestration: prefill, per-step draft
//! forward, target verify, greedy acceptance, rollback, commit,
//! decode loop.
//!
//! # Status
//!
//! Control-flow + prefill + one-cycle scaffolding in place. Target
//! forward now runs end-to-end through embedding + N target layers
//! + out_norm + lm_head via the vendored Metal kernels. The draft
//! forward + spec-decode commit path ties into these in `one_cycle`.
//!
//! Known limits on this first pass (all documented at call sites
//! + covered by pending todos):
//!
//!   * `GatedDeltaNet::forward` is a scaffolded stub — prefill over
//!     the hybrid target will fail on a GDN layer until the GDN
//!     forward lands.
//!   * The `one_cycle` body drives draft forward + verify through
//!     the naive single-token path; it does not yet route through
//!     the `gated_delta_tape` + `tape_replay` rollback machinery.
//!
//! ref: `dflash_mlx/runtime.py`

use std::time::Instant;

use anyhow::{anyhow, Result};

use crate::common::constants::{
    DFLASH35B_DRAFT_BLOCK_SIZE, DFLASH35B_DRAFT_MASK_TOKEN_ID, DFLASH35B_DRAFT_N_TARGET_LAYERS,
    DFLASH35B_DRAFT_TARGET_LAYER_IDS,
};
use crate::metal::cache::RecurrentRollbackCache;
use crate::metal::engine::{detect_engine, Engine};
use crate::metal::ffi::{BlitEncoder, Buffer, CommandBuffer, Device};
use crate::metal::kernels;
use crate::metal::model::{DraftWeights, TargetLayer, TargetWeights};
use crate::metal::qwen::KvCache;
use crate::metal::work::WorkBuffers;

// ─── Public config + stats (mirror the CUDA side's `GenConfig`/`RunStats`) ──

#[derive(Clone, Copy, Debug)]
pub struct GenConfig {
    pub block_size: i32,
    pub sink_size: i32,
    pub window_size: i32,
    pub dflash_max_ctx: i32,
    pub verify_qmm_opt_in: bool,
    pub profile: bool,
    pub pipeline_warmup: bool,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self {
            block_size: DFLASH35B_DRAFT_BLOCK_SIZE,
            sink_size: 64,
            window_size: 1024,
            // runtime.py `_resolve_dflash_max_ctx` defaults to 8192.
            dflash_max_ctx: 8192,
            verify_qmm_opt_in: false,
            profile: false,
            pipeline_warmup: true,
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct RunStats {
    pub n_generated: i64,
    pub n_draft_steps: i64,
    pub n_accept_sum: i64,
    pub n_draft_tokens_attempted: i64,
    pub prefill_s: f64,
    pub pipeline_warmup_s: f64,
    pub pipeline_warmup_n: i64,
    pub wall_s: f64,
    pub decode_tok_s: f64,
}

const PROFILE_PREFIX: &str = "dflash35b-a3b-metal/profile";

const COMMON_METAL_PIPELINES: &[&str] = &[
    "ctox_rms_norm_bf16",
    "ctox_silu_bf16",
    "ctox_sigmoid_bf16",
    "ctox_add_bf16",
    "ctox_mul_bf16",
    "ctox_scale_bf16",
    "ctox_silu_mul_bf16",
    "ctox_rope_bf16",
    "ctox_argmax_bf16",
    "ctox_embedding_gather_mlx4_bf16",
    "ctox_dense_matmul_bf16",
    "ctox_kv_cache_append_bf16",
    "ctox_split_q_gate_bf16",
    "ctox_apply_attention_gate_bf16",
    "ctox_sdpa_naive_bf16",
    "ctox_sdpa_decode_vec_bf16",
    "ctox_softplus_bf16",
    "ctox_add_bias_bf16",
    "ctox_neg_exp_mul_bf16",
    "ctox_softplus_neg_exp_mul_bias_bf16",
    "ctox_l2_norm_bf16",
    "ctox_conv_concat_bf16",
    "ctox_split_qkv_conv_bf16",
    "ctox_ssm_conv1d_bf16",
    "ctox_ssm_conv_state_update_bf16",
    "ctox_copy_bf16",
    "ctox_copy_u32",
    "ctox_repeat_hidden5_bf16",
    "ctox_copy_hidden_slot_bf16",
    "ctox_moe_route_topk_bf16",
    "ctox_moe_fill_gather_indices_i32",
    "ctox_moe_expert_gate_up_bf16",
    "ctox_moe_expert_down_accum_bf16",
    "ctox_moe_accum_weighted_bf16",
    "ctox_moe_add_shared_bf16",
    "affine_qmv_fast_bfloat16_t_gs_64_b_4_batch_0",
    "affine_gather_qmv_fast_bfloat16_t_gs_64_b_4",
    "affine_gather_qmv_bfloat16_t_gs_64_b_4",
    "affine_qmv_bfloat16_t_gs_64_b_4_batch_0",
    "affine_qmm_t_bfloat16_t_gs_64_b_4_alN_false_batch_0",
    "gemv_bfloat16_bm1_bn8_sm1_sn32_tm4_tn4_nc0_axpby0",
    "gemv_bfloat16_bm1_bn8_sm1_sn32_tm4_tn4_nc0_axpby1",
    "gemv_bfloat16_bm1_bn8_sm1_sn32_tm1_tn4_nc0_axpby0",
    "gemv_bfloat16_bm1_bn8_sm1_sn32_tm1_tn4_nc0_axpby1",
    "gemv_bfloat16_bm1_bn1_sm8_sn4_tm4_tn4_nc0_axpby0",
    "gemv_bfloat16_bm1_bn1_sm8_sn4_tm4_tn4_nc0_axpby1",
    "gemv_bfloat16_bm1_bn1_sm8_sn4_tm1_tn4_nc0_axpby0",
    "gemv_bfloat16_bm1_bn1_sm8_sn4_tm1_tn4_nc0_axpby1",
    "gemv_bfloat16_bm4_bn1_sm1_sn32_tm1_tn4_nc0_axpby0",
    "gemv_bfloat16_bm4_bn1_sm1_sn32_tm1_tn4_nc0_axpby1",
    "gemv_bfloat16_bm4_bn1_sm1_sn32_tm4_tn4_nc0_axpby0",
    "gemv_bfloat16_bm4_bn1_sm1_sn32_tm4_tn4_nc0_axpby1",
    "gemv_bfloat16_bm8_bn1_sm1_sn32_tm4_tn4_nc0_axpby0",
    "gemv_bfloat16_bm8_bn1_sm1_sn32_tm4_tn4_nc0_axpby1",
    "v_Sigmoidbfloat16bfloat16",
    "vv_Addbfloat16",
    "vv_Multiplybfloat16",
    "verify_mma2big_gs64_bf16",
    "verify_mma2big_pipe_gs64_bf16",
];

struct BufferSnapshot {
    buf: Buffer,
    len: usize,
}

impl BufferSnapshot {
    fn len(&self) -> usize {
        self.len
    }
}

struct KvSnapshot {
    offset: i32,
}

struct RuntimeSnapshot {
    kv: Vec<KvSnapshot>,
    conv: Vec<Option<BufferSnapshot>>,
    ssm: Vec<Option<BufferSnapshot>>,
}

impl RuntimeSnapshot {
    fn byte_len(&self) -> usize {
        let conv_bytes = self
            .conv
            .iter()
            .filter_map(|snap| snap.as_ref())
            .map(BufferSnapshot::len)
            .sum::<usize>();
        let ssm_bytes = self
            .ssm
            .iter()
            .filter_map(|snap| snap.as_ref())
            .map(BufferSnapshot::len)
            .sum::<usize>();
        conv_bytes + ssm_bytes
    }
}

struct VerifyGdnTapeBuffers {
    tape: Buffer,
    k: Buffer,
    g: Buffer,
    qkv: Buffer,
    conv_rows: i32,
    conv_channels: i32,
    hk: usize,
    hv: usize,
    dk: usize,
    dv: usize,
}

struct CycleOutcome {
    committed: Vec<i32>,
    next_staged: i32,
    accepted_from_draft: i32,
    attempted_from_draft: i32,
}

// ─── DFlashRuntime ─────────────────────────────────────────────────

pub struct DFlashRuntime {
    pub target: TargetWeights,
    pub draft: DraftWeights,
    pub cfg: GenConfig,
    pub engine: Engine,
    pub target_rollback: RecurrentRollbackCache,

    /// Per-full-attention-layer KV cache. Indexed in layer order — but
    /// only layers where `TargetLayer::FullAttention` lives get a
    /// populated slot; GDN layers hold a zero-capacity placeholder.
    pub layer_kv: Vec<KvCache>,

    /// Per-GDN-layer conv_state + ssm_state buffers. Parallel vec to
    /// `layer_kv`; unused slots are placeholders.
    pub layer_conv_state: Vec<Option<Buffer>>,
    pub layer_ssm_state: Vec<Option<Buffer>>,
    verify_gdn_tapes: Vec<Option<VerifyGdnTapeBuffers>>,

    /// Pre-allocated per-runtime activation scratch. Sized for the
    /// worst-case prefill chunk width.
    pub work: WorkBuffers,

    /// A running hidden buffer [max_tokens, hidden] used across layers.
    pub hidden_a: Buffer,
    pub hidden_b: Buffer,

    /// Positions buffer + logits buffer (reused across steps).
    pub positions_buf: Buffer,
    pub logits_buf: Buffer,
    pub last_target_rows: i32,

    pub draft_target_base: Buffer,
    pub draft_target_concat: Buffer,
    pub draft_context_rows: i32,
    pub draft_context_token_offset: i32,
    pub draft_projected_hidden: Buffer,
    pub draft_hidden_a: Buffer,
    pub draft_hidden_b: Buffer,
    pub draft_arg_buf: Buffer,
    pub draft_positions_ctx: Buffer,
    pub draft_positions_noise: Buffer,
    pub draft_layer_kv: Vec<KvCache>,
}

impl DFlashRuntime {
    fn bf16_to_f32(v: u16) -> f32 {
        f32::from_bits(u32::from(v) << 16)
    }

    fn maybe_dump_moe_topk(&self, tag: &str) {
        if std::env::var_os("CTOX_METAL_DEBUG_MOE_TOPK").is_none() {
            return;
        }
        let k = crate::common::constants::DFLASH35B_EXPERTS_PER_TOK as usize;
        let mut ids = vec![0i32; k];
        let mut weights_raw = vec![0u16; k];
        unsafe {
            self.work.moe_topk_ids.read(0, &mut ids);
            self.work.moe_topk_weights.read(0, &mut weights_raw);
        }
        let weights: Vec<f32> = weights_raw.into_iter().map(Self::bf16_to_f32).collect();
        eprintln!("[{PROFILE_PREFIX}] {tag} moe_topk ids={ids:?} weights={weights:?}");
    }

    fn maybe_dump_moe_shared(&self, tag: &str) {
        if std::env::var_os("CTOX_METAL_DEBUG_MOE_SHARED").is_none() {
            return;
        }
        let hidden = self.target.n_embd as usize;
        let mut gate_raw = [0u16; 1];
        let mut shared_raw = vec![0u16; hidden];
        let mut residual_raw = vec![0u16; hidden];
        let mut final_hidden_raw = vec![0u16; hidden];
        unsafe {
            self.work.moe_shared_gate.read(0, &mut gate_raw);
            self.work.moe_shared.read(0, &mut shared_raw);
            self.work.residual.read(0, &mut residual_raw);
            self.draft_target_base.read(0, &mut final_hidden_raw);
        }
        let gate = Self::bf16_to_f32(gate_raw[0]);
        let mut sum = 0.0f32;
        let mut max_abs = 0.0f32;
        for v in shared_raw {
            let x = Self::bf16_to_f32(v);
            sum += x;
            max_abs = max_abs.max(x.abs());
        }
        let mut residual_sum = 0.0f32;
        let mut residual_max_abs = 0.0f32;
        for v in residual_raw {
            let x = Self::bf16_to_f32(v);
            residual_sum += x;
            residual_max_abs = residual_max_abs.max(x.abs());
        }
        let mut final_hidden_sum = 0.0f32;
        let mut final_hidden_max_abs = 0.0f32;
        for v in final_hidden_raw {
            let x = Self::bf16_to_f32(v);
            final_hidden_sum += x;
            final_hidden_max_abs = final_hidden_max_abs.max(x.abs());
        }
        eprintln!(
            "[{PROFILE_PREFIX}] {tag} moe_shared gate={gate} sum={sum} max_abs={max_abs} \
             residual_sum={residual_sum} residual_max_abs={residual_max_abs} \
             final_hidden_sum={final_hidden_sum} final_hidden_max_abs={final_hidden_max_abs}"
        );
    }

    fn prewarm_common_pipelines(dev: &Device) -> usize {
        COMMON_METAL_PIPELINES
            .iter()
            .filter(|&&name| dev.pipeline(name).is_some())
            .count()
    }

    fn warm_first_target_forward(&mut self, dev: &Device, token_id: i32) -> Result<f64> {
        let snapshot = self.snapshot_runtime_state(dev)?;
        let ids_buf = dev
            .new_buffer_from_slice(bytemuck::cast_slice(&[token_id]))
            .ok_or_else(|| anyhow!("warm_first_target_forward: upload token failed"))?;
        let t = Instant::now();
        let cmd = dev
            .new_command_buffer()
            .ok_or_else(|| anyhow!("warm_first_target_forward: no command buffer"))?;
        self.target_forward(dev, &cmd, &ids_buf, 1, 0, false, 0, false)?;
        if self.cfg.profile {
            eprintln!("[{PROFILE_PREFIX}] warm_first_target_forward commit begin");
        }
        cmd.commit_and_wait()
            .map_err(|e| anyhow!("warm_first_target_forward command buffer: {e}"))?;
        if self.cfg.profile {
            eprintln!("[{PROFILE_PREFIX}] warm_first_target_forward commit end");
        }
        let warm_s = t.elapsed().as_secs_f64();
        self.restore_runtime_state(dev, &snapshot)?;
        Ok(warm_s)
    }

    pub fn new(
        dev: &Device,
        target: TargetWeights,
        draft: DraftWeights,
        cfg: GenConfig,
    ) -> Result<Self> {
        let n_target_layers = target.n_layer as usize;
        let _ = draft.layers.len();
        let engine = detect_engine(&target);
        let target_rollback = RecurrentRollbackCache::new(n_target_layers, target.ssm_d_conv);

        // Max tokens per forward chunk. Prefill uses full
        // `dflash_max_ctx` for a single pass if the prompt fits;
        // verify/AR decode uses block_size or 1. Pick the max of the
        // two so work buffers fit everything.
        let max_tokens = cfg.dflash_max_ctx.max(cfg.block_size).max(1);

        let hidden = target.n_embd;
        let intermediate = target.n_ff.max(
            draft
                .layers
                .first()
                .map(|l| l.mlp.intermediate)
                .unwrap_or(0),
        );
        let n_q_features = target.n_head * target.n_embd_head_k;
        let n_kv_features = target.n_head_kv * target.n_embd_head_k;

        // GDN sizing — use the target's SSM config. For a pure-attn
        // target these are the defaults, the scratch just sits unused.
        let conv_channels = target.ssm_d_inner + 2 * target.ssm_n_group * target.ssm_d_state;
        let work = WorkBuffers::new(
            dev,
            max_tokens,
            hidden,
            intermediate,
            n_q_features,
            n_kv_features,
            conv_channels,
            target.ssm_d_inner,
            target.ssm_dt_rank,
            target.ssm_d_state,
            target.ssm_d_conv,
        )
        .ok_or_else(|| anyhow!("WorkBuffers::new: device alloc failed"))?;

        let bf16 = crate::metal::work::BF16;
        let hidden_bytes = (max_tokens as usize) * (hidden as usize) * bf16;
        let hidden_a = dev
            .new_buffer(hidden_bytes)
            .ok_or_else(|| anyhow!("hidden_a alloc failed"))?;
        let hidden_b = dev
            .new_buffer(hidden_bytes)
            .ok_or_else(|| anyhow!("hidden_b alloc failed"))?;

        let positions_buf = dev
            .new_buffer((max_tokens as usize) * std::mem::size_of::<i32>())
            .ok_or_else(|| anyhow!("positions alloc failed"))?;
        let logits_bytes = (max_tokens as usize) * (target.output.out_features as usize) * bf16;
        let logits_buf = dev
            .new_buffer(logits_bytes)
            .ok_or_else(|| anyhow!("logits alloc failed"))?;

        let draft_hidden = draft.fc.out_features.max(1);
        let draft_max_ctx = cfg.sink_size.max(0) + cfg.window_size.max(0) + cfg.block_size.max(1);
        let draft_target_base = dev
            .new_buffer((max_tokens as usize) * (hidden as usize) * bf16)
            .ok_or_else(|| anyhow!("draft_target_base alloc failed"))?;
        let draft_target_concat = dev
            .new_buffer(
                (max_tokens as usize)
                    * (hidden as usize)
                    * (DFLASH35B_DRAFT_N_TARGET_LAYERS as usize)
                    * bf16,
            )
            .ok_or_else(|| anyhow!("draft_target_concat alloc failed"))?;
        let draft_projected_hidden = dev
            .new_buffer((max_tokens as usize) * (draft_hidden as usize) * bf16)
            .ok_or_else(|| anyhow!("draft_projected_hidden alloc failed"))?;
        let draft_hidden_a = dev
            .new_buffer((cfg.block_size.max(1) as usize) * (draft_hidden as usize) * bf16)
            .ok_or_else(|| anyhow!("draft_hidden_a alloc failed"))?;
        let draft_hidden_b = dev
            .new_buffer((cfg.block_size.max(1) as usize) * (draft_hidden as usize) * bf16)
            .ok_or_else(|| anyhow!("draft_hidden_b alloc failed"))?;
        let draft_arg_buf = dev
            .new_buffer((cfg.block_size.max(1) as usize) * std::mem::size_of::<i32>())
            .ok_or_else(|| anyhow!("draft_arg_buf alloc failed"))?;
        let draft_positions_ctx = dev
            .new_buffer((max_tokens as usize) * std::mem::size_of::<i32>())
            .ok_or_else(|| anyhow!("draft_positions_ctx alloc failed"))?;
        let draft_positions_noise = dev
            .new_buffer((cfg.block_size.max(1) as usize) * std::mem::size_of::<i32>())
            .ok_or_else(|| anyhow!("draft_positions_noise alloc failed"))?;

        let mut draft_layer_kv = Vec::with_capacity(draft.layers.len());
        for layer in &draft.layers {
            draft_layer_kv.push(
                KvCache::new(
                    dev,
                    draft_max_ctx.max(cfg.block_size).max(2),
                    layer.attention.n_kv_heads,
                    layer.attention.head_dim,
                )
                .ok_or_else(|| anyhow!("draft layer KV alloc failed"))?,
            );
        }

        // Allocate per-layer KV + SSM caches up front.
        let mut layer_kv = Vec::with_capacity(n_target_layers);
        let mut layer_conv_state: Vec<Option<Buffer>> = Vec::with_capacity(n_target_layers);
        let mut layer_ssm_state: Vec<Option<Buffer>> = Vec::with_capacity(n_target_layers);
        let mut verify_gdn_tapes: Vec<Option<VerifyGdnTapeBuffers>> =
            Vec::with_capacity(n_target_layers);

        for layer in &target.layers {
            match layer {
                TargetLayer::FullAttention { attention, .. } => {
                    layer_kv.push(
                        KvCache::new(
                            dev,
                            cfg.dflash_max_ctx,
                            attention.n_kv_heads,
                            attention.head_dim,
                        )
                        .ok_or_else(|| anyhow!("layer KV alloc failed"))?,
                    );
                    layer_conv_state.push(None);
                    layer_ssm_state.push(None);
                    verify_gdn_tapes.push(None);
                }
                TargetLayer::GatedDelta { delta, .. } => {
                    // Placeholder KV (not used on GDN layers).
                    layer_kv.push(
                        KvCache::new(dev, 1, 1, 1)
                            .ok_or_else(|| anyhow!("placeholder KV alloc failed"))?,
                    );
                    let conv_bytes = ((delta.d_conv - 1).max(0) as usize)
                        * (delta.conv_channels() as usize)
                        * bf16;
                    let num_v_heads = (delta.d_inner / delta.d_state).max(1) as usize;
                    let head_k_dim = 128usize;
                    let ssm_bytes = num_v_heads
                        * (delta.d_state as usize)
                        * head_k_dim
                        * std::mem::size_of::<f32>();
                    let conv_state = dev.new_buffer(conv_bytes.max(16));
                    if let Some(buf) = conv_state.as_ref() {
                        unsafe { std::ptr::write_bytes(buf.as_ptr(), 0, buf.len()) };
                    }
                    let ssm_state = dev.new_buffer(ssm_bytes.max(16));
                    if let Some(buf) = ssm_state.as_ref() {
                        unsafe { std::ptr::write_bytes(buf.as_ptr(), 0, buf.len()) };
                    }
                    let verify_tokens = cfg.block_size.max(1) as usize;
                    let conv_channels = delta.conv_channels() as usize;
                    let num_k_heads = delta.n_group.max(1) as usize;
                    let head_v_dim = delta.d_state.max(1) as usize;
                    let tape = dev
                        .new_buffer(
                            verify_tokens
                                * num_v_heads
                                * head_v_dim
                                * std::mem::size_of::<f32>(),
                        )
                        .ok_or_else(|| anyhow!("verify GDN tape alloc failed"))?;
                    let k = dev
                        .new_buffer(verify_tokens * num_k_heads * head_k_dim * bf16)
                        .ok_or_else(|| anyhow!("verify GDN k alloc failed"))?;
                    let g = dev
                        .new_buffer(verify_tokens * num_v_heads * std::mem::size_of::<f32>())
                        .ok_or_else(|| anyhow!("verify GDN g alloc failed"))?;
                    let qkv = dev
                        .new_buffer(verify_tokens * conv_channels * bf16)
                        .ok_or_else(|| anyhow!("verify GDN qkv alloc failed"))?;
                    layer_conv_state.push(conv_state);
                    layer_ssm_state.push(ssm_state);
                    verify_gdn_tapes.push(Some(VerifyGdnTapeBuffers {
                        tape,
                        k,
                        g,
                        qkv,
                        conv_rows: (delta.d_conv - 1).max(0),
                        conv_channels: conv_channels as i32,
                        hk: num_k_heads,
                        hv: num_v_heads,
                        dk: head_k_dim,
                        dv: head_v_dim,
                    }));
                }
            }
        }

        Ok(Self {
            target,
            draft,
            cfg,
            engine,
            target_rollback,
            layer_kv,
            layer_conv_state,
            layer_ssm_state,
            verify_gdn_tapes,
            work,
            hidden_a,
            hidden_b,
            positions_buf,
            logits_buf,
            last_target_rows: 0,
            draft_target_base,
            draft_target_concat,
            draft_context_rows: 0,
            draft_context_token_offset: 0,
            draft_projected_hidden,
            draft_hidden_a,
            draft_hidden_b,
            draft_arg_buf,
            draft_positions_ctx,
            draft_positions_noise,
            draft_layer_kv,
        })
    }

    /// One forward pass of the target model over `n_tokens` at
    /// `token_offset` absolute positions. Token IDs must already be
    /// uploaded to `token_ids`. Final logits land in `self.logits_buf`.
    ///
    /// ref: runtime.py::target_forward_with_hidden_states
    fn target_forward(
        &mut self,
        dev: &Device,
        cmd: &CommandBuffer,
        token_ids: &Buffer,
        n_tokens: i32,
        token_offset: i32,
        capture_rows: bool,
        capture_row_offset: i32,
        capture_verify_tapes: bool,
    ) -> Result<()> {
        self.last_target_rows = n_tokens;
        // Upload positions.
        let pos: Vec<i32> = (token_offset..token_offset + n_tokens).collect();
        unsafe { self.positions_buf.write(0, &pos) };

        let Some(enc) = cmd.compute() else {
            return Err(anyhow!("target_forward: no compute encoder"));
        };

        // Embedding lookup.
        if !kernels::embedding_gather_mlx4bit_gs64_bf16(
            &enc,
            dev,
            token_ids,
            &self.target.tok_embed.w_q,
            &self.target.tok_embed.scales,
            &self.target.tok_embed.biases,
            &self.hidden_a,
            n_tokens,
            self.target.n_embd,
        ) {
            enc.end();
            return Err(anyhow!("embedding gather failed"));
        }

        // Per-layer forward. alternate hidden_a ↔ hidden_b as
        // in / out to avoid in-place hazards across layers.
        let mut input_buf = &self.hidden_a;
        let output_buf = &self.hidden_b;
        let rms_eps = self.target.rope.base; // unused but silences borrowck
        let _ = rms_eps;
        let debug_max_layers = std::env::var("CTOX_METAL_MAX_LAYERS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok());
        let debug_stop_after_attn_layer = std::env::var("CTOX_METAL_STOP_AFTER_ATTN_LAYER")
            .ok()
            .and_then(|s| s.parse::<usize>().ok());
        let trace_layers = std::env::var_os("CTOX_METAL_TRACE_LAYERS").is_some();

        for (layer_idx, layer) in self.target.layers.iter().enumerate() {
            if debug_max_layers.is_some_and(|max| layer_idx >= max) {
                if self.cfg.profile || trace_layers {
                    eprintln!(
                        "[{PROFILE_PREFIX}] target_forward debug stop before layer {layer_idx}"
                    );
                }
                break;
            }
            if trace_layers {
                eprintln!("[{PROFILE_PREFIX}] target_forward encode layer {layer_idx}");
            }
            match layer {
                TargetLayer::FullAttention {
                    attn_norm,
                    ffn_norm,
                    attention,
                    mlp,
                    ..
                } => {
                    // input → normed_x via attn_norm.
                    if !attn_norm.forward(
                        &enc,
                        dev,
                        input_buf,
                        &self.work.normed_x,
                        n_tokens as usize,
                    ) {
                        enc.end();
                        return Err(anyhow!("layer {layer_idx}: attn_norm failed"));
                    }
                    // attention → residual into work.residual
                    if !attention.forward(
                        &enc,
                        dev,
                        &self.work.normed_x,
                        &self.positions_buf,
                        &self.work.residual,
                        &mut self.layer_kv[layer_idx],
                        &self.work,
                        n_tokens,
                    ) {
                        enc.end();
                        return Err(anyhow!(
                            "layer {layer_idx}: attention forward failed — {}",
                            crate::common::errors::last_error()
                        ));
                    }
                    // residual add: out = input + attention_out
                    let n_elt = n_tokens * self.target.n_embd;
                    if !kernels::add_bf16(
                        &enc,
                        dev,
                        input_buf,
                        &self.work.residual,
                        output_buf,
                        n_elt,
                    ) {
                        enc.end();
                        return Err(anyhow!("layer {layer_idx}: residual add failed"));
                    }
                    if debug_stop_after_attn_layer == Some(layer_idx) {
                        if self.cfg.profile || trace_layers {
                            eprintln!(
                                "[{PROFILE_PREFIX}] target_forward debug stop after attention layer {layer_idx}"
                            );
                        }
                        input_buf = output_buf;
                        break;
                    }
                    // ffn_norm on output_buf → normed_x.
                    if !ffn_norm.forward(
                        &enc,
                        dev,
                        output_buf,
                        &self.work.normed_x,
                        n_tokens as usize,
                    ) {
                        enc.end();
                        return Err(anyhow!("layer {layer_idx}: ffn_norm failed"));
                    }
                    // MLP → residual.
                    if !mlp.forward(
                        &enc,
                        dev,
                        &self.work.normed_x,
                        &self.work.residual,
                        n_tokens,
                        &self.work,
                    ) {
                        enc.end();
                        return Err(anyhow!("layer {layer_idx}: mlp forward failed"));
                    }
                    // residual add over output_buf.
                    if !kernels::add_bf16(
                        &enc,
                        dev,
                        output_buf,
                        &self.work.residual,
                        input_buf, // reuse the previous input slot as the next hidden state
                        n_elt,
                    ) {
                        enc.end();
                        return Err(anyhow!("layer {layer_idx}: mlp residual add failed"));
                    }
                    // `input_buf` now holds the next hidden state; `output_buf`
                    // remains scratch for the next layer.
                }
                TargetLayer::GatedDelta {
                    attn_norm,
                    ffn_norm,
                    delta,
                    mlp,
                    ..
                } => {
                    if !attn_norm.forward(
                        &enc,
                        dev,
                        input_buf,
                        &self.work.normed_x,
                        n_tokens as usize,
                    ) {
                        enc.end();
                        return Err(anyhow!("GDN layer {layer_idx}: attn_norm failed"));
                    }
                    let Some(conv_state) = self.layer_conv_state[layer_idx].as_ref() else {
                        enc.end();
                        return Err(anyhow!("GDN layer {layer_idx}: conv_state buffer missing"));
                    };
                    let Some(ssm_state) = self.layer_ssm_state[layer_idx].as_ref() else {
                        enc.end();
                        return Err(anyhow!("GDN layer {layer_idx}: ssm_state buffer missing"));
                    };
                    if !delta.forward(
                        &enc,
                        dev,
                        &self.work.normed_x,
                        &self.work.residual,
                        conv_state,
                        ssm_state,
                        &self.work,
                        n_tokens,
                        layer_idx,
                        capture_verify_tapes,
                    ) {
                        enc.end();
                        return Err(anyhow!(
                            "GDN layer {layer_idx}: GatedDeltaNet forward failed — {}",
                            crate::common::errors::last_error()
                        ));
                    }
                    if capture_verify_tapes {
                        let tape = self.verify_gdn_tapes[layer_idx]
                            .as_ref()
                            .ok_or_else(|| anyhow!("GDN layer {layer_idx}: verify tape missing"))?;
                        let n = n_tokens as usize;
                        if !kernels::copy_raw_f32(
                            &enc,
                            dev,
                            &self.work.gdn_tape,
                            &tape.tape,
                            (n * tape.hv * tape.dv) as i32,
                        ) || !kernels::copy_raw_bf16(
                            &enc,
                            dev,
                            &self.work.gdn_k,
                            &tape.k,
                            (n * tape.hk * tape.dk) as i32,
                        ) || !kernels::copy_raw_f32(
                            &enc,
                            dev,
                            &self.work.gdn_g,
                            &tape.g,
                            (n * tape.hv) as i32,
                        ) || !kernels::copy_raw_bf16(
                            &enc,
                            dev,
                            &self.work.gdn_qkv_mixed,
                            &tape.qkv,
                            (n * tape.conv_channels as usize) as i32,
                        ) {
                            enc.end();
                            return Err(anyhow!("GDN layer {layer_idx}: verify tape capture failed"));
                        }
                    }
                    let n_elt = n_tokens * self.target.n_embd;
                    if !kernels::add_bf16(
                        &enc,
                        dev,
                        input_buf,
                        &self.work.residual,
                        output_buf,
                        n_elt,
                    ) {
                        enc.end();
                        return Err(anyhow!("GDN layer {layer_idx}: residual add failed"));
                    }
                    if debug_stop_after_attn_layer == Some(layer_idx) {
                        if self.cfg.profile || trace_layers {
                            eprintln!(
                                "[{PROFILE_PREFIX}] target_forward debug stop after GDN layer {layer_idx}"
                            );
                        }
                        input_buf = output_buf;
                        break;
                    }
                    if !ffn_norm.forward(
                        &enc,
                        dev,
                        output_buf,
                        &self.work.normed_x,
                        n_tokens as usize,
                    ) {
                        enc.end();
                        return Err(anyhow!("GDN layer {layer_idx}: ffn_norm failed"));
                    }
                    if !mlp.forward(
                        &enc,
                        dev,
                        &self.work.normed_x,
                        &self.work.residual,
                        n_tokens,
                        &self.work,
                    ) {
                        enc.end();
                        return Err(anyhow!(
                            "GDN layer {layer_idx}: mlp forward failed — {}",
                            crate::common::errors::last_error()
                        ));
                    }
                    if !kernels::add_bf16(
                        &enc,
                        dev,
                        output_buf,
                        &self.work.residual,
                        input_buf,
                        n_elt,
                    ) {
                        enc.end();
                        return Err(anyhow!("GDN layer {layer_idx}: mlp residual add failed"));
                    }
                    // `input_buf` now holds the next hidden state; `output_buf`
                    // remains scratch for the next layer.
                }
            }

            if let Some(slot) = DFLASH35B_DRAFT_TARGET_LAYER_IDS
                .iter()
                .position(|&id| id as usize == layer_idx)
            {
                let ok = if capture_rows {
                    kernels::copy_hidden_slot_rows_bf16_offset(
                        &enc,
                        dev,
                        input_buf,
                        &self.draft_target_concat,
                        capture_row_offset,
                        n_tokens,
                        self.target.n_embd,
                        slot as i32,
                        DFLASH35B_DRAFT_N_TARGET_LAYERS,
                    )
                } else {
                    let src_row = (n_tokens - 1).max(0);
                    kernels::copy_hidden_slot_bf16(
                        &enc,
                        dev,
                        input_buf,
                        &self.draft_target_concat,
                        src_row,
                        self.target.n_embd,
                        slot as i32,
                    )
                };
                if !ok {
                    enc.end();
                    return Err(anyhow!("layer {layer_idx}: target hidden capture failed"));
                }
            }
        }

        // Final norm + lm_head.
        if !kernels::copy_raw_bf16(
            &enc,
            dev,
            input_buf,
            &self.draft_target_base,
            n_tokens * self.target.n_embd,
        ) {
            enc.end();
            return Err(anyhow!("target hidden capture failed"));
        }
        if !self.target.out_norm.forward(
            &enc,
            dev,
            input_buf,
            &self.work.normed_x,
            n_tokens as usize,
        ) {
            enc.end();
            return Err(anyhow!("out_norm forward failed"));
        }
        let lm_head_ok = if n_tokens == 1 && std::env::var_os("CTOX_METAL_LM_HEAD_M1_QMM").is_some() {
            self.target.output.forward_qmm_t(
                &enc,
                dev,
                &self.work.normed_x,
                &self.logits_buf,
                n_tokens,
            )
        } else {
            self.target
                .output
                .forward(&enc, dev, &self.work.normed_x, &self.logits_buf, n_tokens)
        };
        if !lm_head_ok {
            enc.end();
            return Err(anyhow!("lm_head forward failed"));
        }
        enc.end();
        Ok(())
    }

    // ref: runtime.py:1000-1220 (`_prefill_target`)
    fn prefill(&mut self, dev: &Device, prompt_ids: &[i32]) -> Result<i32> {
        if prompt_ids.is_empty() {
            return Err(anyhow!("prefill: empty prompt"));
        }
        let n = prompt_ids.len() as i32;
        if n > self.cfg.dflash_max_ctx {
            return Err(anyhow!(
                "prefill: prompt_len={n} > dflash_max_ctx={}",
                self.cfg.dflash_max_ctx
            ));
        }

        let t_total = Instant::now();
        let mut upload_s = 0.0f64;
        let t_forward = Instant::now();
        let mut pos = 0i32;
        let prefill_chunk = self.cfg.block_size.min(16).max(1);
        while pos < n {
            let rows = (n - pos).min(prefill_chunk);
            let lo = pos as usize;
            let hi = (pos + rows) as usize;
            let t_upload = Instant::now();
            let ids_buf = dev
                .new_buffer_from_slice(bytemuck::cast_slice(&prompt_ids[lo..hi]))
                .ok_or_else(|| anyhow!("prefill: upload prompt ids chunk failed"))?;
            upload_s += t_upload.elapsed().as_secs_f64();

            let cmd = dev
                .new_command_buffer()
                .ok_or_else(|| anyhow!("prefill: no command buffer"))?;
            self.target_forward(dev, &cmd, &ids_buf, rows, pos, true, pos, false)?;
            if self.cfg.profile {
                eprintln!("[{PROFILE_PREFIX}] prefill chunk pos={pos} rows={rows} commit begin");
            }
            cmd.commit_and_wait()
                .map_err(|e| anyhow!("prefill chunk pos={pos} rows={rows} command buffer: {e}"))?;
            if self.cfg.profile {
                eprintln!("[{PROFILE_PREFIX}] prefill chunk pos={pos} rows={rows} commit end");
            }
            pos += rows;
        }
        self.draft_context_rows = n;
        self.draft_context_token_offset = 0;
        self.maybe_dump_moe_topk("prefill");
        self.maybe_dump_moe_shared("prefill");
        let forward_s = t_forward.elapsed().as_secs_f64();

        let t_argmax = Instant::now();
        let last_rows = n - ((n - 1) / prefill_chunk) * prefill_chunk;
        let posterior = self.argmax_logits(dev, last_rows)?;
        let argmax_s = t_argmax.elapsed().as_secs_f64();
        if self.cfg.profile {
            eprintln!(
                "[{PROFILE_PREFIX}] prefill rows={n} upload_s={upload_s:.6} \
                 target_forward_s={forward_s:.6} argmax_s={argmax_s:.6} total_s={:.6}",
                t_total.elapsed().as_secs_f64()
            );
        }
        posterior
            .last()
            .copied()
            .ok_or_else(|| anyhow!("prefill: empty posterior"))
    }

    fn snapshot_buffer(dev: &Device, blit: &BlitEncoder, buf: &Buffer) -> Result<BufferSnapshot> {
        let len = buf.len();
        let snapshot = dev
            .new_buffer(len)
            .ok_or_else(|| anyhow!("snapshot_buffer: alloc {len} bytes failed"))?;
        blit.copy_buffer(buf, 0, &snapshot, 0, len);
        Ok(BufferSnapshot { buf: snapshot, len })
    }

    fn restore_buffer(blit: &BlitEncoder, buf: &Buffer, snapshot: &BufferSnapshot) {
        blit.copy_buffer(&snapshot.buf, 0, buf, 0, snapshot.len);
    }

    fn snapshot_runtime_state(&self, dev: &Device) -> Result<RuntimeSnapshot> {
        let cmd = dev
            .new_command_buffer()
            .ok_or_else(|| anyhow!("snapshot_runtime_state: no command buffer"))?;
        let blit = cmd
            .blit()
            .ok_or_else(|| anyhow!("snapshot_runtime_state: no blit encoder"))?;
        let kv = self
            .layer_kv
            .iter()
            .map(|cache| KvSnapshot {
                offset: cache.offset,
            })
            .collect();
        let conv = self
            .layer_conv_state
            .iter()
            .map(|b| {
                b.as_ref()
                    .map(|buf| Self::snapshot_buffer(dev, &blit, buf))
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        let ssm = self
            .layer_ssm_state
            .iter()
            .map(|b| {
                b.as_ref()
                    .map(|buf| Self::snapshot_buffer(dev, &blit, buf))
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        blit.end();
        cmd.commit_and_wait()
            .map_err(|e| anyhow!("snapshot_runtime_state blit: {e}"))?;
        Ok(RuntimeSnapshot { kv, conv, ssm })
    }

    fn restore_runtime_state(&mut self, dev: &Device, snapshot: &RuntimeSnapshot) -> Result<()> {
        let cmd = dev
            .new_command_buffer()
            .ok_or_else(|| anyhow!("restore_runtime_state: no command buffer"))?;
        let blit = cmd
            .blit()
            .ok_or_else(|| anyhow!("restore_runtime_state: no blit encoder"))?;
        for (cache, snap) in self.layer_kv.iter_mut().zip(snapshot.kv.iter()) {
            cache.offset = snap.offset;
        }
        for (live, snap) in self.layer_conv_state.iter().zip(snapshot.conv.iter()) {
            if let (Some(buf), Some(snap)) = (live.as_ref(), snap.as_ref()) {
                Self::restore_buffer(&blit, buf, snap);
            }
        }
        for (live, snap) in self.layer_ssm_state.iter().zip(snapshot.ssm.iter()) {
            if let (Some(buf), Some(snap)) = (live.as_ref(), snap.as_ref()) {
                Self::restore_buffer(&blit, buf, snap);
            }
        }
        blit.end();
        cmd.commit_and_wait()
            .map_err(|e| anyhow!("restore_runtime_state blit: {e}"))?;
        Ok(())
    }

    fn rollback_runtime_state_to_accepted(
        &mut self,
        dev: &Device,
        snapshot: &RuntimeSnapshot,
        accepted_steps: i32,
        block_len: i32,
        token_offset: i32,
    ) -> Result<()> {
        let cmd = dev
            .new_command_buffer()
            .ok_or_else(|| anyhow!("rollback_runtime_state_to_accepted: no command buffer"))?;
        let Some(enc) = cmd.compute() else {
            return Err(anyhow!(
                "rollback_runtime_state_to_accepted: no compute encoder"
            ));
        };
        if accepted_steps < block_len {
            for (cache, snap) in self.layer_kv.iter_mut().zip(snapshot.kv.iter()) {
                cache.offset = snap.offset + accepted_steps;
            }
            for layer_idx in 0..self.target.layers.len() {
                let Some(tape) = self.verify_gdn_tapes[layer_idx].as_ref() else {
                    continue;
                };
                let Some(Some(conv_snapshot)) = snapshot.conv.get(layer_idx) else {
                    enc.end();
                    return Err(anyhow!("rollback layer {layer_idx}: conv snapshot missing"));
                };
                let Some(Some(ssm_snapshot)) = snapshot.ssm.get(layer_idx) else {
                    enc.end();
                    return Err(anyhow!("rollback layer {layer_idx}: ssm snapshot missing"));
                };
                let Some(conv_live) = self.layer_conv_state[layer_idx].as_ref() else {
                    enc.end();
                    return Err(anyhow!("rollback layer {layer_idx}: live conv state missing"));
                };
                let Some(ssm_live) = self.layer_ssm_state[layer_idx].as_ref() else {
                    enc.end();
                    return Err(anyhow!("rollback layer {layer_idx}: live ssm state missing"));
                };
                if !kernels::gdn_conv_state_replay_bf16(
                    &enc,
                    dev,
                    &conv_snapshot.buf,
                    &tape.qkv,
                    conv_live,
                    accepted_steps,
                    tape.conv_rows,
                    tape.conv_channels,
                ) {
                    enc.end();
                    return Err(anyhow!("rollback layer {layer_idx}: conv replay failed"));
                }
                if !kernels::tape_replay_bf16(
                    &enc,
                    dev,
                    false,
                    false,
                    &tape.tape,
                    &tape.k,
                    &tape.g,
                    &ssm_snapshot.buf,
                    accepted_steps,
                    None,
                    ssm_live,
                    1,
                    tape.hk,
                    tape.hv,
                    tape.dk,
                    tape.dv,
                ) {
                    enc.end();
                    return Err(anyhow!("rollback layer {layer_idx}: tape replay failed"));
                }
            }
        }
        enc.end();
        cmd.commit_and_wait()
            .map_err(|e| anyhow!("rollback_runtime_state_to_accepted command buffer: {e}"))?;
        self.draft_context_rows = accepted_steps;
        self.draft_context_token_offset = token_offset;
        Ok(())
    }

    fn argmax_logits(&self, dev: &Device, rows: i32) -> Result<Vec<i32>> {
        let vocab = self.target.output.out_features;
        let out_buf = dev
            .new_buffer((rows as usize) * std::mem::size_of::<i32>())
            .ok_or_else(|| anyhow!("argmax_logits: output alloc failed"))?;
        let cmd = dev
            .new_command_buffer()
            .ok_or_else(|| anyhow!("argmax_logits: no command buffer"))?;
        let Some(enc) = cmd.compute() else {
            return Err(anyhow!("argmax_logits: no compute encoder"));
        };
        if !kernels::argmax_last_bf16(&enc, dev, &self.logits_buf, &out_buf, vocab, rows as usize) {
            enc.end();
            return Err(anyhow!("argmax_logits: argmax_last_bf16 failed"));
        }
        enc.end();
        cmd.commit_and_wait()
            .map_err(|e| anyhow!("argmax_logits command buffer: {e}"))?;

        let mut out = vec![0i32; rows as usize];
        unsafe { out_buf.read(0, &mut out) };
        Ok(out)
    }

    fn draft_forward_greedy(
        &mut self,
        dev: &Device,
        staged_first: i32,
        block_len: i32,
    ) -> Result<Vec<i32>> {
        if self.draft.layers.is_empty() || block_len <= 1 {
            return Ok(Vec::new());
        }

        let hidden = self.target.n_embd;
        if self.draft.fc.in_features != hidden * DFLASH35B_DRAFT_N_TARGET_LAYERS {
            return Err(anyhow!(
                "draft fc shape mismatch: in_features={} expected {}",
                self.draft.fc.in_features,
                hidden * DFLASH35B_DRAFT_N_TARGET_LAYERS
            ));
        }
        if self.draft.fc.out_features != hidden {
            return Err(anyhow!(
                "draft hidden mismatch: fc.out_features={} target hidden={hidden}",
                self.draft.fc.out_features
            ));
        }

        let mut block_ids = Vec::with_capacity(block_len as usize);
        block_ids.push(staged_first);
        block_ids.extend(std::iter::repeat_n(
            DFLASH35B_DRAFT_MASK_TOKEN_ID,
            (block_len - 1) as usize,
        ));
        let ids_buf = dev
            .new_buffer_from_slice(bytemuck::cast_slice(&block_ids))
            .ok_or_else(|| anyhow!("draft_forward_greedy: upload block ids failed"))?;

        let context_rows = self.draft_context_rows.max(1);
        if context_rows > self.cfg.dflash_max_ctx {
            return Err(anyhow!(
                "draft_forward_greedy: context_rows={context_rows} exceeds dflash_max_ctx={}",
                self.cfg.dflash_max_ctx
            ));
        }
        let ctx_pos0 = self.draft_context_token_offset.max(0);
        let ctx_pos: Vec<i32> = (ctx_pos0..ctx_pos0 + context_rows).collect();
        let noise_pos0 = ctx_pos0 + context_rows;
        let noise_pos: Vec<i32> = (noise_pos0..noise_pos0 + block_len).collect();
        unsafe {
            self.draft_positions_ctx.write(0, &ctx_pos);
            self.draft_positions_noise.write(0, &noise_pos);
        }

        let cmd = dev
            .new_command_buffer()
            .ok_or_else(|| anyhow!("draft_forward_greedy: no command buffer"))?;
        let Some(enc) = cmd.compute() else {
            return Err(anyhow!("draft_forward_greedy: no compute encoder"));
        };

        if !self.draft.fc.forward(
            &enc,
            dev,
            &self.draft_target_concat,
            &self.draft_projected_hidden,
            context_rows,
        ) {
            enc.end();
            return Err(anyhow!("draft fc projection failed"));
        }
        if !self.draft.hidden_norm.forward(
            &enc,
            dev,
            &self.draft_projected_hidden,
            &self.draft_target_concat,
            context_rows as usize,
        ) {
            enc.end();
            return Err(anyhow!("draft hidden_norm failed"));
        }

        if !kernels::embedding_gather_mlx4bit_gs64_bf16(
            &enc,
            dev,
            &ids_buf,
            &self.target.tok_embed.w_q,
            &self.target.tok_embed.scales,
            &self.target.tok_embed.biases,
            &self.draft_hidden_a,
            block_len,
            hidden,
        ) {
            enc.end();
            return Err(anyhow!("draft noise embedding gather failed"));
        }

        let cur = self.draft_hidden_a.clone();
        let next = self.draft_hidden_b.clone();
        for (layer_idx, layer) in self.draft.layers.iter().enumerate() {
            if !layer
                .attn_norm
                .forward(&enc, dev, &cur, &self.work.normed_x, block_len as usize)
            {
                enc.end();
                return Err(anyhow!("draft layer {layer_idx}: attn_norm failed"));
            }
            if !layer.attention.forward_cross(
                &enc,
                dev,
                &self.work.normed_x,
                &self.draft_target_concat,
                &self.draft_positions_ctx,
                &self.draft_positions_noise,
                &self.work.residual,
                &mut self.draft_layer_kv[layer_idx],
                &self.work,
                block_len,
                context_rows,
            ) {
                enc.end();
                return Err(anyhow!("draft layer {layer_idx}: cross attention failed"));
            }
            let n_elt = block_len * hidden;
            if !kernels::add_bf16(&enc, dev, &cur, &self.work.residual, &next, n_elt) {
                enc.end();
                return Err(anyhow!(
                    "draft layer {layer_idx}: attention residual add failed"
                ));
            }
            if !layer
                .ffn_norm
                .forward(&enc, dev, &next, &self.work.normed_x, block_len as usize)
            {
                enc.end();
                return Err(anyhow!("draft layer {layer_idx}: ffn_norm failed"));
            }
            if !layer.mlp.forward(
                &enc,
                dev,
                &self.work.normed_x,
                &self.work.residual,
                block_len,
                &self.work.mlp_gate,
                &self.work.mlp_up,
                &self.work.mlp_silu,
                &self.work.mlp_prod,
            ) {
                enc.end();
                return Err(anyhow!("draft layer {layer_idx}: mlp failed"));
            }
            if !kernels::add_bf16(&enc, dev, &next, &self.work.residual, &cur, n_elt) {
                enc.end();
                return Err(anyhow!("draft layer {layer_idx}: mlp residual add failed"));
            }
        }

        if !self
            .draft
            .out_norm
            .forward(&enc, dev, &cur, &self.work.normed_x, block_len as usize)
        {
            enc.end();
            return Err(anyhow!("draft out_norm failed"));
        }
        let draft_rows = block_len - 1;
        if !self.target.output.forward_from_row(
            &enc,
            dev,
            &self.work.normed_x,
            1,
            &self.logits_buf,
            draft_rows,
        ) {
            enc.end();
            return Err(anyhow!("draft lm_head failed"));
        }
        if !kernels::argmax_last_bf16(
            &enc,
            dev,
            &self.logits_buf,
            &self.draft_arg_buf,
            self.target.output.out_features,
            draft_rows as usize,
        ) {
            enc.end();
            return Err(anyhow!("draft argmax failed"));
        }
        enc.end();
        cmd.commit_and_wait()
            .map_err(|e| anyhow!("draft_forward_greedy command buffer: {e}"))?;

        let mut ids = vec![0i32; draft_rows as usize];
        unsafe { self.draft_arg_buf.read(0, &mut ids) };
        Ok(ids)
    }

    /// One speculative-decode cycle.
    ///
    /// ref: runtime.py::generate_dflash_once main loop (lines 1290-1493).
    ///
    /// This implementation keeps the committed output on the AR target
    /// path while the draft model is computed as a sidecar. That is an
    /// intentional staging step: the next port step is target verify +
    /// acceptance + rollback before draft tokens are committed.
    fn target_commit(
        &mut self,
        dev: &Device,
        ids: &[i32],
        capture_verify_rows: bool,
    ) -> Result<Vec<i32>> {
        let t_total = Instant::now();
        let abs_pos = self.abs_position();
        let t_upload = Instant::now();
        let ids_buf = dev
            .new_buffer_from_slice(bytemuck::cast_slice(ids))
            .ok_or_else(|| anyhow!("target_commit: upload ids failed"))?;
        let upload_s = t_upload.elapsed().as_secs_f64();
        let t_forward = Instant::now();
        let cmd = dev
            .new_command_buffer()
            .ok_or_else(|| anyhow!("target_commit: no command buffer"))?;
        self.target_forward(
            dev,
            &cmd,
            &ids_buf,
            ids.len() as i32,
            abs_pos,
            capture_verify_rows,
            0,
            capture_verify_rows,
        )?;
        cmd.commit_and_wait()
            .map_err(|e| anyhow!("target_commit target_forward: {e}"))?;
        self.maybe_dump_moe_topk("target_commit");
        self.maybe_dump_moe_shared("target_commit");
        let forward_s = t_forward.elapsed().as_secs_f64();
        let t_argmax = Instant::now();
        let posterior = self.argmax_logits(dev, ids.len() as i32)?;
        let argmax_s = t_argmax.elapsed().as_secs_f64();
        if self.cfg.profile {
            eprintln!(
                "[{PROFILE_PREFIX}] target_commit rows={} abs_pos={abs_pos} \
                 upload_s={upload_s:.6} target_forward_s={forward_s:.6} \
                 argmax_s={argmax_s:.6} total_s={:.6}",
                ids.len(),
                t_total.elapsed().as_secs_f64()
            );
        }
        Ok(posterior)
    }

    fn count_acceptance(verify_ids: &[i32], posterior: &[i32]) -> i32 {
        let mut accepted = 0i32;
        let max_cmp = verify_ids
            .len()
            .saturating_sub(1)
            .min(posterior.len().saturating_sub(1));
        for i in 0..max_cmp {
            if verify_ids[i + 1] == posterior[i] {
                accepted += 1;
            } else {
                break;
            }
        }
        accepted
    }

    fn one_cycle(
        &mut self,
        dev: &Device,
        staged_first: i32,
        max_commit: i32,
    ) -> Result<CycleOutcome> {
        let t_cycle = Instant::now();
        let block_len = self.cfg.block_size.max(1).min(max_commit.max(1));
        let verify_token_offset = self.abs_position();
        if self.draft.layers.is_empty() || block_len <= 1 {
            let t_ar = Instant::now();
            let posterior = self.target_commit(dev, &[staged_first], false)?;
            let ar_s = t_ar.elapsed().as_secs_f64();
            let next_staged = posterior
                .first()
                .copied()
                .ok_or_else(|| anyhow!("one_cycle: empty AR posterior"))?;
            if self.cfg.profile {
                eprintln!(
                    "[{PROFILE_PREFIX}] cycle block_len={block_len} target_only=true \
                     target_commit_s={ar_s:.6} total_s={:.6}",
                    t_cycle.elapsed().as_secs_f64()
                );
            }
            return Ok(CycleOutcome {
                committed: vec![staged_first],
                next_staged,
                accepted_from_draft: 0,
                attempted_from_draft: 0,
            });
        }

        let t_draft = Instant::now();
        let drafted = self.draft_forward_greedy(dev, staged_first, block_len)?;
        let draft_s = t_draft.elapsed().as_secs_f64();
        let mut verify_ids = Vec::with_capacity(block_len as usize);
        verify_ids.push(staged_first);
        verify_ids.extend(drafted.into_iter().take((block_len - 1) as usize));
        if verify_ids.len() < block_len as usize {
            verify_ids.resize(block_len as usize, DFLASH35B_DRAFT_MASK_TOKEN_ID);
        }

        let t_snapshot = Instant::now();
        let snapshot = self.snapshot_runtime_state(dev)?;
        let snapshot_s = t_snapshot.elapsed().as_secs_f64();
        let snapshot_bytes = snapshot.byte_len();
        let t_verify = Instant::now();
        let posterior = self.target_commit(dev, &verify_ids, true)?;
        let verify_s = t_verify.elapsed().as_secs_f64();
        let accepted_from_draft = Self::count_acceptance(&verify_ids, &posterior);
        let commit_count = 1 + accepted_from_draft;
        let next_staged = posterior
            .get(accepted_from_draft as usize)
            .copied()
            .ok_or_else(|| anyhow!("one_cycle: posterior missing next staged token"))?;

        let committed = verify_ids[..commit_count as usize].to_vec();
        let t_rollback = Instant::now();
        if std::env::var_os("CTOX_METAL_RECOMMIT_ROLLBACK").is_some() {
            self.restore_runtime_state(dev, &snapshot)?;
            let ids_buf = dev
                .new_buffer_from_slice(bytemuck::cast_slice(&committed))
                .ok_or_else(|| anyhow!("one_cycle recommit: upload committed ids failed"))?;
            let cmd = dev
                .new_command_buffer()
                .ok_or_else(|| anyhow!("one_cycle recommit: no command buffer"))?;
            self.target_forward(
                dev,
                &cmd,
                &ids_buf,
                commit_count,
                verify_token_offset,
                true,
                0,
                false,
            )?;
            cmd.commit_and_wait()
                .map_err(|e| anyhow!("one_cycle recommit target_forward: {e}"))?;
            self.draft_context_rows = commit_count;
            self.draft_context_token_offset = verify_token_offset;
        } else {
            self.rollback_runtime_state_to_accepted(
                dev,
                &snapshot,
                commit_count,
                block_len,
                verify_token_offset,
            )?;
        }
        let rollback_s = t_rollback.elapsed().as_secs_f64();
        if self.cfg.profile {
            eprintln!(
                "[{PROFILE_PREFIX}] cycle block_len={block_len} commit_count={commit_count} \
                 accepted={accepted_from_draft}/{} draft_s={draft_s:.6} \
                 snapshot_s={snapshot_s:.6} snapshot_mb={:.2} verify_s={verify_s:.6} \
                 rollback_s={rollback_s:.6} total_s={:.6}",
                block_len - 1,
                snapshot_bytes as f64 / (1024.0 * 1024.0),
                t_cycle.elapsed().as_secs_f64()
            );
            if block_len <= 8 {
                eprintln!(
                    "[{PROFILE_PREFIX}] cycle tokens verify_ids={verify_ids:?} posterior={posterior:?}"
                );
            }
        }
        if std::env::var_os("CTOX_METAL_TRACE_CYCLE_TOKENS").is_some() {
            eprintln!(
                "[{PROFILE_PREFIX}] cycle_tokens pos={verify_token_offset} block_len={block_len} \
                 commit_count={commit_count} accepted={accepted_from_draft} \
                 committed={committed:?} next={next_staged} verify_ids={verify_ids:?} \
                 posterior={posterior:?}"
            );
        }

        Ok(CycleOutcome {
            committed,
            next_staged,
            accepted_from_draft,
            attempted_from_draft: block_len - 1,
        })
    }

    /// Current absolute position counter, derived from the first
    /// full-attention layer's KV cache offset. Prefill sets
    /// `offset = prompt_len`; every AR step nudges it by +1.
    fn abs_position(&self) -> i32 {
        // Walk layers to find the first full-attn layer and report
        // its cache offset. GDN layers don't maintain a positional
        // offset in the attention-style sense.
        for (i, layer) in self.target.layers.iter().enumerate() {
            if matches!(layer, TargetLayer::FullAttention { .. }) {
                return self.layer_kv[i].offset;
            }
        }
        0
    }

    /// Top-level generate loop. Mirrors the CUDA side's
    /// `run_dflash_gen_loop` shape so both backends can be driven by
    /// the same bench binary.
    pub fn generate(
        &mut self,
        dev: &Device,
        prompt_ids: &[i32],
        n_gen: i32,
        out: &mut Vec<i32>,
    ) -> Result<RunStats> {
        let (pipeline_warmup_s, pipeline_warmup_n) = if self.cfg.pipeline_warmup {
            let t = Instant::now();
            let n = Self::prewarm_common_pipelines(dev);
            let pso_s = t.elapsed().as_secs_f64();
            let warm_forward_s = self.warm_first_target_forward(dev, prompt_ids[0])?;
            let s = pso_s + warm_forward_s;
            if self.cfg.profile {
                eprintln!(
                    "[{PROFILE_PREFIX}] pipeline_warmup compiled_or_cached={n}/{} \
                     pso_s={pso_s:.6} warm_target_forward_s={warm_forward_s:.6} \
                     warmup_s={s:.6}",
                    COMMON_METAL_PIPELINES.len()
                );
            }
            (s, n as i64)
        } else {
            (0.0, 0)
        };
        let t_total = Instant::now();

        // Prefill.
        let t_prefill = Instant::now();
        let mut next = self.prefill(dev, prompt_ids)?;
        let prefill_s = t_prefill.elapsed().as_secs_f64();

        out.clear();
        out.extend_from_slice(prompt_ids);

        // Decode.
        let mut stats = RunStats {
            prefill_s,
            pipeline_warmup_s,
            pipeline_warmup_n,
            ..RunStats::default()
        };
        let mut remaining = n_gen;

        while remaining > 0 {
            let cycle = self.one_cycle(dev, next, remaining)?;
            if cycle.committed.is_empty() {
                break;
            }
            let take = (cycle.committed.len() as i32).min(remaining);
            for &t in &cycle.committed[..take as usize] {
                out.push(t);
            }
            stats.n_generated += take as i64;
            stats.n_draft_steps += 1;
            stats.n_accept_sum += i64::from(cycle.accepted_from_draft.min(take.saturating_sub(1)));
            stats.n_draft_tokens_attempted += i64::from(cycle.attempted_from_draft);
            next = cycle.next_staged;
            remaining -= take;
        }

        let wall = t_total.elapsed().as_secs_f64();
        let decode_s = (wall - prefill_s).max(1e-9);
        stats.wall_s = wall;
        stats.decode_tok_s = (stats.n_generated as f64) / decode_s;
        Ok(stats)
    }
}
