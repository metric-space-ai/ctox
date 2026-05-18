//! Native Rust reimplementation of the Qwen3.5 base modules that the
//! Python DFlash-MLX port pulls from `mlx_lm.models.qwen3` and
//! `mlx_lm.models.gated_delta`.
//!
//! # Scope
//!
//! Implements the core modules as plain-Rust structs whose `forward`
//! method encodes a sequence of Metal kernel dispatches into a caller-
//! provided `ComputeEncoder`:
//!
//!   * [`RmsNorm`]       — `mx.fast.rms_norm` equivalent.
//!   * [`Swiglu`]        — SiLU(gate) * up, used by the MLP block.
//!   * [`Mlp`]           — the Qwen3 two-linear-with-gate MLP.
//!   * [`Attention`]     — full-attention block with GQA and RoPE.
//!   * [`GatedDeltaNet`] — linear-attention block with SSM conv + tape.
//!   * [`Rope`]          — precomputed head_dim / rope_dim / theta.
//!   * [`KvCache`]       — rolling KV cache for the attention block.
//!
//! Each `forward` takes buffer handles + int shape args, never arrays.
//! Shape bookkeeping lives in the caller (the driver in `metal/driver.rs`
//! and the target/draft wiring in `metal/model.rs`).
//!
//! ref: `dflash_mlx/runtime.py`, `mlx_lm/models/qwen3.py`,
//!      `mlx_lm/models/gated_delta.py`, `mlx_lm/models/cache.py`.

use crate::common::errors::set_last_error;
use crate::metal::ffi::{Buffer, ComputeEncoder, Device};
use crate::metal::kernels;
use crate::metal::mlx_ops::{self, MlxDtype};
use crate::metal::verify::qmm;

// ─── RmsNorm ────────────────────────────────────────────────────────

/// Weight of an RMSNorm layer (a single [D] vector). Lives on the GPU
/// as a `Buffer` of `bfloat16`. The caller owns the `x`/`y` buffers.
pub struct RmsNorm {
    pub weight: Buffer,
    pub d: i32,
    pub eps: f32,
    pub weight_bias: f32,
}

impl RmsNorm {
    /// Allocate + upload an RmsNorm weight vector (`bfloat16`, length D).
    pub fn new(dev: &Device, weight_bf16: &[u16], d: i32, eps: f32) -> Option<Self> {
        let byte_len = weight_bf16.len() * std::mem::size_of::<u16>();
        let w = dev.new_buffer(byte_len)?;
        unsafe { w.write(0, weight_bf16) };
        Some(Self {
            weight: w,
            d,
            eps,
            weight_bias: 0.0,
        })
    }

    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        n_rows: usize,
    ) -> bool {
        kernels::rms_norm_bf16(
            enc,
            dev,
            x,
            &self.weight,
            y,
            self.d,
            self.eps,
            self.weight_bias,
            n_rows,
        )
    }
}

// ─── MLX 4-bit linear (packed weights) ──────────────────────────────

/// Packed-4bit linear layer matching `mx.quantized_matmul(transpose=True)`.
/// Fields come out of the MLX safetensors loader:
///   weights : `[out_features, in_features / 8]` uint32
///   scales  : `[out_features, in_features / GS]` bf16
///   biases  : `[out_features, in_features / GS]` bf16
pub struct Linear4Bit {
    pub w_q: Buffer,
    pub scales: Buffer,
    pub biases: Buffer,
    pub in_features: i32,
    pub out_features: i32,
}

impl Linear4Bit {
    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        m: i32,
    ) -> bool {
        self.forward_with_verify_scratch(enc, dev, x, y, m, None)
    }

    pub fn forward_mlx(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        m: i32,
    ) -> bool {
        if m == 1 && std::env::var_os("CTOX_METAL_LINEAR_M1_QMM").is_none() {
            return mlx_ops::op_qmv(
                enc,
                dev,
                MlxDtype::Bf16,
                &self.w_q,
                0,
                &self.scales,
                0,
                Some((&self.biases, 0)),
                x,
                0,
                y,
                0,
                1,
                self.out_features,
                self.in_features,
                64,
                4,
            );
        }
        mlx_ops::op_qmm_t(
            enc,
            dev,
            MlxDtype::Bf16,
            &self.w_q,
            0,
            &self.scales,
            0,
            Some((&self.biases, 0)),
            x,
            0,
            y,
            0,
            m,
            self.out_features,
            self.in_features,
            64,
            4,
        )
    }

    pub fn forward_qmm_t(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        m: i32,
    ) -> bool {
        mlx_ops::op_qmm_t(
            enc,
            dev,
            MlxDtype::Bf16,
            &self.w_q,
            0,
            &self.scales,
            0,
            Some((&self.biases, 0)),
            x,
            0,
            y,
            0,
            m,
            self.out_features,
            self.in_features,
            64,
            4,
        )
    }

    pub fn forward_from_row(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        x_row: i32,
        y: &Buffer,
        m: i32,
    ) -> bool {
        if x_row < 0 || m <= 0 {
            set_last_error(format!(
                "Linear4Bit::forward_from_row: invalid x_row={x_row} m={m}"
            ));
            return false;
        }
        let x_off = (x_row as usize) * (self.in_features as usize) * crate::metal::work::BF16;
        if m == 1 && std::env::var_os("CTOX_METAL_LINEAR_M1_QMM").is_none() {
            return mlx_ops::op_qmv(
                enc,
                dev,
                MlxDtype::Bf16,
                &self.w_q,
                0,
                &self.scales,
                0,
                Some((&self.biases, 0)),
                x,
                x_off,
                y,
                0,
                1,
                self.out_features,
                self.in_features,
                64,
                4,
            );
        }
        mlx_ops::op_qmm_t_nax(
            enc,
            dev,
            MlxDtype::Bf16,
            &self.w_q,
            0,
            &self.scales,
            0,
            Some((&self.biases, 0)),
            x,
            x_off,
            y,
            0,
            m,
            self.out_features,
            self.in_features,
            64,
            4,
        )
    }

    pub fn forward_with_verify_scratch(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        m: i32,
        verify_partials: Option<&Buffer>,
    ) -> bool {
        if m == 16
            && self.in_features % 32 == 0
            && self.out_features % 32 == 0
            && self.out_features < 100_000
        {
            return qmm::dispatch_verify(enc, dev, self, x, y, m, 64, 4, verify_partials);
        }
        kernels::quantized_matmul_mlx4bit_gs64_bf16(
            enc,
            dev,
            x,
            &self.w_q,
            &self.scales,
            &self.biases,
            y,
            m,
            self.in_features,
            self.out_features,
        )
    }

    pub fn forward_one_row(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        x_row: i32,
        y: &Buffer,
    ) -> bool {
        if x_row < 0 {
            set_last_error(format!("Linear4Bit::forward_one_row: invalid row {x_row}"));
            return false;
        }
        let x_off = (x_row as usize) * (self.in_features as usize) * crate::metal::work::BF16;
        if std::env::var_os("CTOX_METAL_LINEAR_M1_QMM").is_none() {
            return mlx_ops::op_qmv(
                enc,
                dev,
                MlxDtype::Bf16,
                &self.w_q,
                0,
                &self.scales,
                0,
                Some((&self.biases, 0)),
                x,
                x_off,
                y,
                0,
                1,
                self.out_features,
                self.in_features,
                64,
                4,
            );
        }
        mlx_ops::op_qmm_t_nax(
            enc,
            dev,
            MlxDtype::Bf16,
            &self.w_q,
            0,
            &self.scales,
            0,
            Some((&self.biases, 0)),
            x,
            x_off,
            y,
            0,
            1,
            self.out_features,
            self.in_features,
            64,
            4,
        )
    }
}

/// Raw bf16 linear layer used by the z-lab DFlash draft checkpoints.
/// Weight layout is `[out_features, in_features]` bf16.
pub struct Bf16Linear {
    pub weight: Buffer,
    pub bias: Option<Buffer>,
    pub in_features: i32,
    pub out_features: i32,
}

impl Bf16Linear {
    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        m: i32,
    ) -> bool {
        kernels::dense_matmul_bf16(
            enc,
            dev,
            x,
            &self.weight,
            self.bias.as_ref(),
            y,
            m,
            self.in_features,
            self.out_features,
        )
    }
}

// ─── SwiGLU + MLP ───────────────────────────────────────────────────

/// Qwen3 MLP: `down(silu(gate(x)) * up(x))`.
pub struct Mlp {
    pub gate: Linear4Bit,
    pub up: Linear4Bit,
    pub down: Linear4Bit,
    /// Size of the intermediate dim (= gate.out_features = up.out_features).
    pub intermediate: i32,
}

impl Mlp {
    /// Forward pass.
    ///
    /// Temporary buffers `tmp_gate`, `tmp_up`, `tmp_silu`, `tmp_prod`
    /// must each be sized for `M * intermediate * sizeof(bfloat16)`.
    /// Caller-owned to avoid per-call allocation on the hot path.
    #[allow(clippy::too_many_arguments)]
    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        m: i32,
        tmp_gate: &Buffer,
        tmp_up: &Buffer,
        _tmp_silu: &Buffer,
        tmp_prod: &Buffer,
        verify_partials: Option<&Buffer>,
    ) -> bool {
        let mlp_m1_qmm = m == 1 && std::env::var_os("CTOX_METAL_MLP_M1_QMM").is_some();
        let gate_ok = if mlp_m1_qmm {
            self.gate.forward_qmm_t(enc, dev, x, tmp_gate, m)
        } else {
            self.gate
                .forward_with_verify_scratch(enc, dev, x, tmp_gate, m, verify_partials)
        };
        if !gate_ok {
            return false;
        }
        let up_ok = if mlp_m1_qmm {
            self.up.forward_qmm_t(enc, dev, x, tmp_up, m)
        } else {
            self.up
                .forward_with_verify_scratch(enc, dev, x, tmp_up, m, verify_partials)
        };
        if !up_ok {
            return false;
        }
        let n_act = m * self.intermediate;
        if !kernels::silu_mul_bf16(enc, dev, tmp_gate, tmp_up, tmp_prod, n_act) {
            return false;
        }
        if mlp_m1_qmm {
            self.down.forward_qmm_t(enc, dev, tmp_prod, y, m)
        } else {
            self.down
                .forward_with_verify_scratch(enc, dev, tmp_prod, y, m, verify_partials)
        }
    }
}

pub struct Bf16Mlp {
    pub gate: Bf16Linear,
    pub up: Bf16Linear,
    pub down: Bf16Linear,
    pub intermediate: i32,
}

impl Bf16Mlp {
    #[allow(clippy::too_many_arguments)]
    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        m: i32,
        tmp_gate: &Buffer,
        tmp_up: &Buffer,
        _tmp_silu: &Buffer,
        tmp_prod: &Buffer,
    ) -> bool {
        if !self.gate.forward(enc, dev, x, tmp_gate, m) {
            return false;
        }
        if !self.up.forward(enc, dev, x, tmp_up, m) {
            return false;
        }
        let n_act = m * self.intermediate;
        if !kernels::silu_mul_bf16(enc, dev, tmp_gate, tmp_up, tmp_prod, n_act) {
            return false;
        }
        self.down.forward(enc, dev, tmp_prod, y, m)
    }
}

pub struct Bf16Attention {
    pub wq: Bf16Linear,
    pub wk: Bf16Linear,
    pub wv: Bf16Linear,
    pub wo: Bf16Linear,
    pub q_norm: Option<RmsNorm>,
    pub k_norm: Option<RmsNorm>,
    pub rope: Rope,
    pub n_heads: i32,
    pub n_kv_heads: i32,
    pub head_dim: i32,
}

impl Bf16Attention {
    #[allow(clippy::too_many_arguments)]
    pub fn forward_cross(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        noise_hidden: &Buffer,
        projected_target_hidden: &Buffer,
        target_positions: &Buffer,
        noise_positions: &Buffer,
        y: &Buffer,
        kv_cache: &mut KvCache,
        work: &crate::metal::work::WorkBuffers,
        block_len: i32,
        ctx_len: i32,
    ) -> bool {
        if !self
            .wq
            .forward(enc, dev, noise_hidden, &work.q_proj, block_len)
        {
            return false;
        }
        let q_after_norm = if let Some(qn) = &self.q_norm {
            let rows = (block_len as usize) * (self.n_heads as usize);
            if !qn.forward(enc, dev, &work.q_proj, &work.q_tmp, rows) {
                return false;
            }
            &work.q_tmp
        } else {
            &work.q_proj
        };
        let q_rope_out = if std::ptr::eq(q_after_norm, &work.q_proj) {
            &work.q_tmp
        } else {
            &work.q_proj
        };
        if !self.rope.apply(
            enc,
            dev,
            q_after_norm,
            noise_positions,
            q_rope_out,
            self.n_heads,
            block_len as usize,
        ) {
            return false;
        }

        if !self
            .wk
            .forward(enc, dev, projected_target_hidden, &work.k_proj, ctx_len)
        {
            return false;
        }
        let ctx_k_after_norm = if let Some(kn) = &self.k_norm {
            let rows = (ctx_len as usize) * (self.n_kv_heads as usize);
            if !kn.forward(enc, dev, &work.k_proj, &work.k_tmp, rows) {
                return false;
            }
            &work.k_tmp
        } else {
            &work.k_proj
        };
        let ctx_k_rope_out = if std::ptr::eq(ctx_k_after_norm, &work.k_proj) {
            &work.k_tmp
        } else {
            &work.k_proj
        };
        if !self.rope.apply(
            enc,
            dev,
            ctx_k_after_norm,
            target_positions,
            ctx_k_rope_out,
            self.n_kv_heads,
            ctx_len as usize,
        ) {
            return false;
        }
        let ctx_write_off = kv_cache.offset;
        if !crate::metal::kernels::kv_cache_append_bf16(
            enc,
            dev,
            ctx_k_rope_out,
            &kv_cache.keys,
            ctx_len,
            self.n_kv_heads,
            self.head_dim,
            kv_cache.max_ctx,
            ctx_write_off,
        ) {
            return false;
        }
        if !self
            .wv
            .forward(enc, dev, projected_target_hidden, &work.v_proj, ctx_len)
        {
            return false;
        }
        if !crate::metal::kernels::kv_cache_append_bf16(
            enc,
            dev,
            &work.v_proj,
            &kv_cache.values,
            ctx_len,
            self.n_kv_heads,
            self.head_dim,
            kv_cache.max_ctx,
            ctx_write_off,
        ) {
            return false;
        }
        let persistent_ctx_len = ctx_write_off + ctx_len;

        if !self
            .wk
            .forward(enc, dev, noise_hidden, &work.k_proj, block_len)
        {
            return false;
        }
        let noise_k_after_norm = if let Some(kn) = &self.k_norm {
            let rows = (block_len as usize) * (self.n_kv_heads as usize);
            if !kn.forward(enc, dev, &work.k_proj, &work.k_tmp, rows) {
                return false;
            }
            &work.k_tmp
        } else {
            &work.k_proj
        };
        let noise_k_rope_out = if std::ptr::eq(noise_k_after_norm, &work.k_proj) {
            &work.k_tmp
        } else {
            &work.k_proj
        };
        if !self.rope.apply(
            enc,
            dev,
            noise_k_after_norm,
            noise_positions,
            noise_k_rope_out,
            self.n_kv_heads,
            block_len as usize,
        ) {
            return false;
        }
        // Match DFlash's ContextOnlyDraftKVCache: context K/V persist across
        // draft cycles, but noise K/V are only appended transiently for this
        // attention call and must not advance the persistent cache offset.
        let noise_write_off = persistent_ctx_len;
        if !crate::metal::kernels::kv_cache_append_bf16(
            enc,
            dev,
            noise_k_rope_out,
            &kv_cache.keys,
            block_len,
            self.n_kv_heads,
            self.head_dim,
            kv_cache.max_ctx,
            noise_write_off,
        ) {
            return false;
        }
        if !self
            .wv
            .forward(enc, dev, noise_hidden, &work.v_proj, block_len)
        {
            return false;
        }
        if !crate::metal::kernels::kv_cache_append_bf16(
            enc,
            dev,
            &work.v_proj,
            &kv_cache.values,
            block_len,
            self.n_kv_heads,
            self.head_dim,
            kv_cache.max_ctx,
            noise_write_off,
        ) {
            return false;
        }

        let scale = 1.0f32 / (self.head_dim as f32).sqrt();
        if std::env::var_os("CTOX_METAL_DRAFT_NAIVE_SDPA").is_some() {
            if !crate::metal::kernels::sdpa_naive_bf16(
                enc,
                dev,
                q_rope_out,
                &kv_cache.keys,
                &kv_cache.values,
                None,
                &work.attn_out,
                self.n_heads,
                self.n_kv_heads,
                block_len,
                persistent_ctx_len + block_len,
                kv_cache.max_ctx,
                self.head_dim,
                scale,
                false,
            ) {
                return false;
            }
        } else if block_len > 0
            && block_len <= 16
            && (self.head_dim == 64
                || self.head_dim == 96
                || self.head_dim == 128
                || self.head_dim == 256)
        {
            if !crate::metal::kernels::transpose_thd_to_htd_bf16(
                enc,
                dev,
                q_rope_out,
                &work.q_proj_raw,
                block_len,
                self.n_heads,
                self.head_dim,
            ) {
                return false;
            }
            if !crate::metal::kernels::sdpa_vector_mlx_bf16(
                enc,
                dev,
                &work.q_proj_raw,
                &kv_cache.keys,
                &kv_cache.values,
                &work.q_tmp,
                self.n_heads,
                self.n_heads / self.n_kv_heads,
                block_len,
                persistent_ctx_len + block_len,
                kv_cache.max_ctx,
                self.head_dim,
                scale,
                false,
            ) {
                return false;
            }
            if !crate::metal::kernels::transpose_htd_to_thd_bf16(
                enc,
                dev,
                &work.q_tmp,
                &work.attn_out,
                block_len,
                self.n_heads,
                self.head_dim,
            ) {
                return false;
            }
        } else {
            set_last_error(format!(
                "Bf16Attention.forward_cross: no high-performance SDPA kernel wired for \
                 block_len={block_len}, head_dim={}, n_heads={}, n_kv_heads={}",
                self.head_dim, self.n_heads, self.n_kv_heads
            ));
            return false;
        }
        kv_cache.offset = persistent_ctx_len;

        self.wo.forward(enc, dev, &work.attn_out, y, block_len)
    }
}

// ─── RoPE ───────────────────────────────────────────────────────────

/// Shape-only RoPE descriptor. The actual rotation is applied in the
/// `rope_apply_bf16` kernel by the attention block — we hold the dims
/// + base here for re-use across layers. No precomputed cos/sin tables
/// since the kernel computes them on the fly per position.
pub struct Rope {
    pub head_dim: i32,
    pub rope_dim: i32,
    pub base: f32,
}

impl Rope {
    pub fn apply(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        positions: &Buffer,
        y: &Buffer,
        n_heads: i32,
        n_tokens: usize,
    ) -> bool {
        kernels::rope_apply_bf16(
            enc,
            dev,
            x,
            positions,
            y,
            self.head_dim,
            self.rope_dim,
            n_heads,
            self.base,
            n_tokens,
        )
    }
}

// ─── KV Cache ───────────────────────────────────────────────────────
//
// A rolling `MTLBuffer` of keys and values shaped [layers][head_dim *
// max_ctx * n_kv_heads]. The attention block appends `n_tokens` new
// entries per forward; `offset` tracks the next write position.

pub struct KvCache {
    pub keys: Buffer,
    pub values: Buffer,
    pub max_ctx: i32,
    pub n_kv_heads: i32,
    pub head_dim: i32,
    pub offset: i32,
}

impl KvCache {
    /// Allocate a fresh KV cache sized for `max_ctx` tokens.
    pub fn new(dev: &Device, max_ctx: i32, n_kv_heads: i32, head_dim: i32) -> Option<Self> {
        let bytes_per_elt = 2; // bf16
        let per_slice = (head_dim as usize) * (n_kv_heads as usize) * (max_ctx as usize);
        let k = dev.new_buffer(per_slice * bytes_per_elt)?;
        let v = dev.new_buffer(per_slice * bytes_per_elt)?;
        Some(Self {
            keys: k,
            values: v,
            max_ctx,
            n_kv_heads,
            head_dim,
            offset: 0,
        })
    }

    /// Reset the cache for a fresh generation.
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// Rewind the cache by `n` tokens after a speculative-decode
    /// rejection. The actual K/V data in those slots stays as-is but
    /// is no longer read on the next forward.
    pub fn rewind(&mut self, n: i32) {
        self.offset = (self.offset - n).max(0);
    }
}

// ─── Attention (full-attention block) ───────────────────────────────

pub struct Attention {
    pub wq: Linear4Bit,
    pub wk: Linear4Bit,
    pub wv: Linear4Bit,
    pub wo: Linear4Bit,
    pub q_norm: Option<RmsNorm>,
    pub k_norm: Option<RmsNorm>,
    pub rope: Rope,
    pub n_heads: i32,
    pub n_kv_heads: i32,
    pub head_dim: i32,
}

impl Attention {
    pub fn gqa_factor(&self) -> i32 {
        if self.n_kv_heads == 0 {
            return 1;
        }
        self.n_heads / self.n_kv_heads
    }

    /// Shape of the Q projection output (tokens × heads × head_dim).
    pub fn q_proj_out_dim(&self) -> i32 {
        self.n_heads * self.head_dim
    }

    pub fn kv_proj_out_dim(&self) -> i32 {
        self.n_kv_heads * self.head_dim
    }

    /// Full-attention-layer forward. Encodes dispatches into `enc`;
    /// caller commits the surrounding `CommandBuffer`.
    ///
    /// Arguments:
    ///   * `normed_x`    : input activation already passed through
    ///                     `attn_norm`, shape `[n_tokens, hidden]`.
    ///   * `positions`   : `[n_tokens]` i32 token positions for RoPE.
    ///   * `y`           : destination `[n_tokens, hidden]`.
    ///   * `kv_cache`    : per-layer KV cache (updated in-place).
    ///   * `work`        : pre-allocated per-runtime work buffers
    ///                     (q/k/v_proj, attn_out).
    ///   * `n_tokens`    : number of tokens in this chunk.
    #[allow(clippy::too_many_arguments)]
    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        normed_x: &Buffer,
        positions: &Buffer,
        y: &Buffer,
        kv_cache: &mut KvCache,
        work: &crate::metal::work::WorkBuffers,
        n_tokens: i32,
    ) -> bool {
        // 1. Q / K / V projections. Qwen3.5-MoE full attention uses
        // an output gate packed into q_proj: [q, gate].
        let q_features = self.q_proj_out_dim();
        let has_output_gate = self.wq.out_features == q_features * 2;
        let attn_m1_qmm =
            n_tokens == 1 && std::env::var_os("CTOX_METAL_ATTN_M1_QMM").is_some();
        if has_output_gate {
            let ok = if attn_m1_qmm {
                self.wq
                    .forward_qmm_t(enc, dev, normed_x, &work.q_proj_raw, n_tokens)
            } else {
                self.wq.forward_with_verify_scratch(
                    enc,
                    dev,
                    normed_x,
                    &work.q_proj_raw,
                    n_tokens,
                    Some(&work.verify_qmm_partials),
                )
            };
            if !ok {
                return false;
            }
            if !kernels::split_q_gate_bf16(
                enc,
                dev,
                &work.q_proj_raw,
                &work.q_proj,
                &work.q_gate,
                n_tokens,
                q_features,
                self.head_dim,
            ) {
                return false;
            }
        } else {
            let ok = if attn_m1_qmm {
                self.wq
                    .forward_qmm_t(enc, dev, normed_x, &work.q_proj, n_tokens)
            } else {
                self.wq.forward_with_verify_scratch(
                    enc,
                    dev,
                    normed_x,
                    &work.q_proj,
                    n_tokens,
                    Some(&work.verify_qmm_partials),
                )
            };
            if !ok {
                return false;
            }
        }
        let wk_ok = if attn_m1_qmm {
            self.wk
                .forward_qmm_t(enc, dev, normed_x, &work.k_proj, n_tokens)
        } else {
            self.wk.forward_with_verify_scratch(
                enc,
                dev,
                normed_x,
                &work.k_proj,
                n_tokens,
                Some(&work.verify_qmm_partials),
            )
        };
        if !wk_ok {
            return false;
        }
        let wv_ok = if attn_m1_qmm {
            self.wv
                .forward_qmm_t(enc, dev, normed_x, &work.v_proj, n_tokens)
        } else {
            self.wv.forward_with_verify_scratch(
                enc,
                dev,
                normed_x,
                &work.v_proj,
                n_tokens,
                Some(&work.verify_qmm_partials),
            )
        };
        if !wv_ok {
            return false;
        }

        // 2. Optional per-head RMSNorm (Qwen3-specific q_norm / k_norm).
        //    Applied over head_dim for each (token, head) row. Do not
        //    run this in-place: each row is reduced by many threads.
        let q_after_norm = if let Some(qn) = &self.q_norm {
            let rows = (n_tokens as usize) * (self.n_heads as usize);
            if !qn.forward(enc, dev, &work.q_proj, &work.q_tmp, rows) {
                return false;
            }
            &work.q_tmp
        } else {
            &work.q_proj
        };
        let k_after_norm = if let Some(kn) = &self.k_norm {
            let rows = (n_tokens as usize) * (self.n_kv_heads as usize);
            if !kn.forward(enc, dev, &work.k_proj, &work.k_tmp, rows) {
                return false;
            }
            &work.k_tmp
        } else {
            &work.k_proj
        };

        // 3. RoPE on q and k. Also avoid in-place writes because each
        // pair rotation reads a sibling value.
        let q_rope_out = if std::ptr::eq(q_after_norm, &work.q_proj) {
            &work.q_tmp
        } else {
            &work.q_proj
        };
        if !self.rope.apply(
            enc,
            dev,
            q_after_norm,
            positions,
            q_rope_out,
            self.n_heads,
            n_tokens as usize,
        ) {
            return false;
        }
        let k_rope_out = if std::ptr::eq(k_after_norm, &work.k_proj) {
            &work.k_tmp
        } else {
            &work.k_proj
        };
        if !self.rope.apply(
            enc,
            dev,
            k_after_norm,
            positions,
            k_rope_out,
            self.n_kv_heads,
            n_tokens as usize,
        ) {
            return false;
        }

        // 4. Append K and V to the rolling cache.
        let write_off = kv_cache.offset;
        if !crate::metal::kernels::kv_cache_append_bf16(
            enc,
            dev,
            k_rope_out,
            &kv_cache.keys,
            n_tokens,
            self.n_kv_heads,
            self.head_dim,
            kv_cache.max_ctx,
            write_off,
        ) {
            return false;
        }
        if !crate::metal::kernels::kv_cache_append_bf16(
            enc,
            dev,
            &work.v_proj,
            &kv_cache.values,
            n_tokens,
            self.n_kv_heads,
            self.head_dim,
            kv_cache.max_ctx,
            write_off,
        ) {
            return false;
        }
        kv_cache.offset += n_tokens;

        // 5. SDPA. Only explicitly wired high-performance kernels are
        // allowed in this model path.
        let scale = 1.0f32 / (self.head_dim as f32).sqrt();
        if n_tokens == 1
            && (self.head_dim == 64
                || self.head_dim == 96
                || self.head_dim == 128
                || self.head_dim == 256)
            && kv_cache.offset >= 1024
        {
            let blocks = 64i32;
            if !crate::metal::kernels::transpose_thd_to_htd_bf16(
                enc,
                dev,
                q_rope_out,
                &work.q_proj_raw,
                n_tokens,
                self.n_heads,
                self.head_dim,
            ) {
                return false;
            }
            if !crate::metal::kernels::sdpa_vector_2pass_mlx_bf16(
                enc,
                dev,
                &work.q_proj_raw,
                &kv_cache.keys,
                &kv_cache.values,
                &work.attn_2pass_partials,
                &work.attn_2pass_sums,
                &work.attn_2pass_maxs,
                &work.q_tmp,
                self.n_heads,
                self.gqa_factor(),
                n_tokens,
                kv_cache.offset,
                kv_cache.max_ctx,
                self.head_dim,
                scale,
                blocks,
                true,
            ) {
                return false;
            }
            if !crate::metal::kernels::transpose_htd_to_thd_bf16(
                enc,
                dev,
                &work.q_tmp,
                &work.attn_out,
                n_tokens,
                self.n_heads,
                self.head_dim,
            ) {
                return false;
            }
        } else if n_tokens > 0
            && n_tokens <= 16
            && (self.head_dim == 64
                || self.head_dim == 96
                || self.head_dim == 128
                || self.head_dim == 256)
            && !(n_tokens == 16 && self.head_dim == 256 && kv_cache.offset >= 1024)
        {
            if !crate::metal::kernels::transpose_thd_to_htd_bf16(
                enc,
                dev,
                q_rope_out,
                &work.q_proj_raw,
                n_tokens,
                self.n_heads,
                self.head_dim,
            ) {
                return false;
            }
            if !crate::metal::kernels::sdpa_vector_mlx_bf16(
                enc,
                dev,
                &work.q_proj_raw,
                &kv_cache.keys,
                &kv_cache.values,
                &work.q_tmp,
                self.n_heads,
                self.gqa_factor(),
                n_tokens,
                kv_cache.offset,
                kv_cache.max_ctx,
                self.head_dim,
                scale,
                true,
            ) {
                return false;
            }
            if !crate::metal::kernels::transpose_htd_to_thd_bf16(
                enc,
                dev,
                &work.q_tmp,
                &work.attn_out,
                n_tokens,
                self.n_heads,
                self.head_dim,
            ) {
                return false;
            }
        } else if n_tokens == 16 && self.head_dim == 256 && kv_cache.offset >= 1024 {
            let blocks = 64i32;
            if !crate::metal::kernels::transpose_thd_to_htd_bf16(
                enc,
                dev,
                q_rope_out,
                &work.q_proj_raw,
                n_tokens,
                self.n_heads,
                self.head_dim,
            ) {
                return false;
            }
            if !crate::metal::kernels::sdpa_2pass_partials_bf16(
                enc,
                dev,
                false,
                &work.q_proj_raw,
                &kv_cache.keys,
                &kv_cache.values,
                self.gqa_factor(),
                kv_cache.offset,
                kv_cache.max_ctx * self.head_dim,
                self.head_dim,
                kv_cache.max_ctx * self.head_dim,
                self.head_dim,
                scale,
                blocks,
                None,
                &work.attn_2pass_partials,
                &work.attn_2pass_sums,
                &work.attn_2pass_maxs,
                self.n_heads as usize,
                1,
                n_tokens as usize,
            ) {
                return false;
            }
            if !crate::metal::kernels::sdpa_2pass_reduce_bf16(
                enc,
                dev,
                &work.attn_2pass_partials,
                &work.attn_2pass_sums,
                &work.attn_2pass_maxs,
                blocks,
                &work.q_tmp,
                self.n_heads as usize,
                n_tokens as usize,
                self.head_dim,
            ) {
                return false;
            }
            if !crate::metal::kernels::transpose_htd_to_thd_bf16(
                enc,
                dev,
                &work.q_tmp,
                &work.attn_out,
                n_tokens,
                self.n_heads,
                self.head_dim,
            ) {
                return false;
            }
        } else {
            set_last_error(format!(
                "Attention.forward: no high-performance SDPA kernel wired for \
                 n_tokens={n_tokens}, head_dim={}, n_heads={}, n_kv_heads={}; \
                 naive SDPA fallback is not allowed",
                self.head_dim, self.n_heads, self.n_kv_heads
            ));
            return false;
        }
        if has_output_gate
            && !crate::metal::kernels::apply_attention_gate_bf16(
                enc,
                dev,
                &work.attn_out,
                &work.q_gate,
                n_tokens * q_features,
            )
        {
            return false;
        }

        // 6. Output projection (`wo`) into y.
        if attn_m1_qmm {
            self.wo
                .forward_qmm_t(enc, dev, &work.attn_out, y, n_tokens)
        } else {
            self.wo.forward_with_verify_scratch(
                enc,
                dev,
                &work.attn_out,
                y,
                n_tokens,
                Some(&work.verify_qmm_partials),
            )
        }
    }
}

// ─── GatedDeltaNet (linear-attention + SSM conv block) ──────────────

pub struct GatedDeltaNet {
    pub wqkv: Linear4Bit,
    pub wqkv_gate: Linear4Bit,
    pub ssm_conv_weight: Buffer,
    pub ssm_conv_bias: Option<Buffer>,
    pub ssm_beta: Linear4Bit,
    pub ssm_alpha: Linear4Bit,
    pub ssm_a: Buffer,
    pub ssm_dt_bias: Buffer,
    pub ssm_norm: RmsNorm,
    pub ssm_out: Linear4Bit,
    pub d_conv: i32,
    pub d_inner: i32,
    pub d_state: i32,
    pub n_group: i32,
}

impl GatedDeltaNet {
    pub fn conv_channels(&self) -> i32 {
        self.d_inner + 2 * self.n_group * self.d_state
    }

    /// GatedDeltaNet-layer forward. Writes the layer output into `y`,
    /// records the per-step innovation tape (bundle) for later
    /// rollback, updates the conv_state + ssm_state slots in-place.
    ///
    /// # Contract
    ///
    ///   * `normed_x`   : `[n_tokens, hidden]` post-attn_norm input.
    ///   * `y`          : `[n_tokens, hidden]` destination.
    ///   * `conv_state` : `[kernel-1, conv_channels]` — updated in place.
    ///   * `ssm_state`  : `[Hv, Dv, Dk]` raw bf16 state buffer matching
    ///                    the reference rollback/cache copy contract.
    ///   * `work`       : pre-allocated bf16 scratch buffers; we
    ///                    reuse the MLP slots for the QKV fused path
    ///                    and the GDN-specific ones come from
    ///                    `gdn_scratch`.
    ///
    /// Full-forward. Dispatches every Metal kernel the block needs
    /// into `enc`; caller commits. ref: cuda::graph::build_delta_net_block
    /// (which is itself byte-exact against the C++ reference). Shape
    /// semantics mirror that port step-for-step.
    ///
    /// Arguments:
    ///   * `normed_x`   : `[n_tokens, hidden]` post-attn_norm input.
    ///   * `y`          : `[n_tokens, hidden]` destination.
    ///   * `conv_state` : `[kernel-1, conv_channels]` — updated in place.
    ///   * `ssm_state`  : `[Hv, Dv, Dk]` raw bf16 state buffer.
    ///   * `work`       : pre-allocated scratch buffers.
    #[allow(clippy::too_many_arguments)]
    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        normed_x: &Buffer,
        y: &Buffer,
        conv_state: &Buffer,
        ssm_state: &Buffer,
        work: &crate::metal::work::WorkBuffers,
        n_tokens: i32,
        layer_idx: usize,
        state_roundtrip: bool,
    ) -> bool {
        use crate::metal::kernels as K;
        let debug_stop_gdn_layer = std::env::var("CTOX_METAL_STOP_GDN_LAYER")
            .ok()
            .and_then(|s| s.parse::<usize>().ok());
        let debug_stop_gdn_stage = std::env::var("CTOX_METAL_STOP_GDN_STAGE").ok();
        let stop_after = |stage: &str| {
            debug_stop_gdn_layer == Some(layer_idx)
                && debug_stop_gdn_stage.as_deref() == Some(stage)
        };

        // Model-wide GDN dimensions for Qwen3.5-35B-A3B (matches
        // cuda::graph::q35 constants).
        let head_k_dim: i32 = 128;
        let head_v_dim: i32 = self.d_state;
        let num_k_heads: i32 = self.n_group.max(1);
        let num_v_heads: i32 = (self.d_inner / head_v_dim).max(1);
        let kernel_m1: i32 = (self.d_conv - 1).max(0);
        let conv_channels = self.conv_channels();
        let exact_small_proj_rows = n_tokens.max(16);

        // 1. qkv_mixed = wqkv @ normed_x    → [n_tokens, conv_channels]
        let gdn_qmm = std::env::var_os("CTOX_METAL_GDN_QMM").is_some()
            || (n_tokens == 1 && std::env::var_os("CTOX_METAL_GDN_M1_QMM").is_some());
        let qkv_ok = if gdn_qmm {
            self.wqkv
                .forward_qmm_t(enc, dev, normed_x, &work.gdn_qkv_mixed, n_tokens)
        } else {
            self.wqkv.forward_with_verify_scratch(
                enc,
                dev,
                normed_x,
                &work.gdn_qkv_mixed,
                n_tokens,
                Some(&work.verify_qmm_partials),
            )
        };
        if !qkv_ok {
            set_last_error("GDN.forward: wqkv projection failed");
            return false;
        }
        if stop_after("wqkv") {
            return true;
        }

        // 2. z = wqkv_gate @ normed_x       → [n_tokens, d_inner]
        let z_ok = if gdn_qmm {
            self.wqkv_gate
                .forward_qmm_t(enc, dev, normed_x, &work.gdn_z, n_tokens)
        } else {
            self.wqkv_gate.forward_with_verify_scratch(
                enc,
                dev,
                normed_x,
                &work.gdn_z,
                n_tokens,
                Some(&work.verify_qmm_partials),
            )
        };
        if !z_ok {
            set_last_error("GDN.forward: wqkv_gate projection failed");
            return false;
        }
        if stop_after("z") {
            return true;
        }

        // 3. beta = sigmoid(ssm_beta @ normed_x)   → [n_tokens, dt_rank]
        //
        if exact_small_proj_rows > n_tokens {
            let hidden = self.wqkv.in_features;
            let tail_offset =
                (n_tokens as usize) * (hidden as usize) * crate::metal::work::BF16;
            let tail_n = (exact_small_proj_rows - n_tokens) * hidden;
            if !K::zero_bf16(enc, dev, normed_x, tail_offset, tail_n) {
                set_last_error("GDN.forward: exact small projection pad failed");
                return false;
            }
        }

        // dflash-mlx wraps in_proj_b/in_proj_a with _ExactSmallProjPad:
        // for S<16 it pads the input rows to M=16 but still uses stock MLX
        // quantized_matmul, not verify_qmm. These 32-column projections are
        // numerically sensitive for recurrent-state coherence.
        if !self.ssm_beta.forward_mlx(
            enc,
            dev,
            normed_x,
            &work.gdn_beta,
            exact_small_proj_rows,
        ) {
            set_last_error("GDN.forward: beta projection failed");
            return false;
        }
        if !K::sigmoid_bf16(
            enc,
            dev,
            &work.gdn_beta,
            &work.gdn_beta,
            n_tokens * num_v_heads,
        ) {
            return false;
        }
        if stop_after("beta") {
            return true;
        }

        // 4. alpha = softplus((ssm_alpha @ normed_x) + ssm_dt_bias)
        //    g     = alpha * (-exp(ssm_a))     ref: cuda::graph lines 749-759
        if !self.ssm_alpha.forward_mlx(
            enc,
            dev,
            normed_x,
            &work.gdn_alpha,
            exact_small_proj_rows,
        ) {
            set_last_error("GDN.forward: alpha projection failed");
            return false;
        }
        if !K::softplus_neg_exp_mul_bias_bf16(
            enc,
            dev,
            &work.gdn_alpha,
            &self.ssm_dt_bias,
            &self.ssm_a,
            &work.gdn_g,
            n_tokens,
            num_v_heads,
        ) {
            return false;
        }
        if stop_after("alpha") {
            return true;
        }

        // 5. conv_input = concat(conv_state, qkv_mixed) on row axis
        if !K::conv_concat_bf16(
            enc,
            dev,
            conv_state,
            &work.gdn_qkv_mixed,
            &work.gdn_conv_input,
            kernel_m1,
            n_tokens,
            conv_channels,
        ) {
            return false;
        }

        // 6. Update conv_state in-place with the trailing (kernel-1)
        //    rows of conv_input. Using a straight copy kernel.
        let tail_bytes = (kernel_m1 as usize) * (conv_channels as usize) * crate::metal::work::BF16;
        if tail_bytes > 0 {
            // Source offset into conv_input: row = n_tokens.
            // copy_raw_bf16 takes whole-buffer src→dst; we use the
            // ssm_conv1d kernel's conv_state_out side when available,
            // else fall through — the runtime owns the conv_state_out
            // slot and does the final write.
            //
            // For now we assume the runtime-level caller (driver.rs)
            // copies the tail explicitly from conv_input after this
            // call returns. This keeps the in-kernel forward clean.
        }

        // 7. conv_out = silu(ssm_conv(conv_input, ssm_conv1d))
        if !K::ssm_conv1d_bf16(
            enc,
            dev,
            conv_state,          // conv_state_in (for boundary handling)
            &work.gdn_qkv_mixed, // x_new
            &self.ssm_conv_weight,
            self.ssm_conv_bias.as_ref().unwrap_or(&self.ssm_conv_weight),
            &work.gdn_conv_out,
            conv_state, // conv_state_out (in-place tail update)
            n_tokens,
            conv_channels,
            self.d_conv,
            self.ssm_conv_bias.is_some(),
        ) {
            set_last_error("GDN.forward: ssm_conv1d failed");
            return false;
        }
        if !K::silu_bf16(
            enc,
            dev,
            &work.gdn_conv_out,
            &work.gdn_conv_out,
            n_tokens * conv_channels,
        ) {
            return false;
        }
        if stop_after("conv") {
            return true;
        }

        // 8. Split conv_out into q/k/v by channel offset.
        if !K::split_qkv_conv_bf16(
            enc,
            dev,
            &work.gdn_conv_out,
            &work.gdn_q,
            &work.gdn_k,
            &work.gdn_v,
            n_tokens,
            num_k_heads * head_k_dim,
            num_v_heads * head_v_dim,
            conv_channels,
        ) {
            return false;
        }

        // 9. L2-norm q and k (per-head-dim row).
        if !K::l2_norm_last_bf16(
            enc,
            dev,
            &work.gdn_q,
            &work.gdn_q,
            head_k_dim,
            1e-6,
            (n_tokens as usize) * (num_k_heads as usize),
        ) {
            return false;
        }
        // MLX uses RMS norm here. ctox_l2_norm_bf16 returns x/sqrt(sum(x^2)),
        // so multiplying q by 1/sqrt(D) matches inv_scale**2 * rms_norm(q).
        // k needs no extra scale because inv_scale * rms_norm(k) equals L2 norm.
        let q_scale = (head_k_dim as f32).sqrt().recip();
        if !K::scale_bf16(
            enc,
            dev,
            &work.gdn_q,
            &work.gdn_q,
            q_scale,
            n_tokens * num_k_heads * head_k_dim,
        ) {
            return false;
        }
        if !K::l2_norm_last_bf16(
            enc,
            dev,
            &work.gdn_k,
            &work.gdn_k,
            head_k_dim,
            1e-6,
            (n_tokens as usize) * (num_k_heads as usize),
        ) {
            return false;
        }
        if stop_after("qk_norm") {
            return true;
        }

        // 10. Normal target execution keeps the SSM state in f32, matching
        // mlx_lm.gated_delta_update. The tape kernel is only for speculative
        // rollback recording.
        let state_is_f32 = !state_roundtrip;
        let gdn_ok = if state_is_f32 {
            K::gated_delta_f32_state_bf16(
                enc,
                dev,
                /* has_mask: */ false,
                /* vectorized: */ false,
                &work.gdn_q,
                &work.gdn_k,
                &work.gdn_v,
                &work.gdn_g,
                &work.gdn_beta,
                ssm_state,
                n_tokens,
                None,
                &work.gdn_delta_out,
                &work.gdn_state_tmp,
                1,
                num_k_heads as usize,
                num_v_heads as usize,
                head_k_dim as usize,
                head_v_dim as usize,
            )
        } else {
            K::gated_delta_tape_bf16(
                enc,
                dev,
                /* has_mask: */ false,
                /* vectorized: */ false,
                &work.gdn_q,
                &work.gdn_k,
                &work.gdn_v,
                &work.gdn_g,
                &work.gdn_beta,
                ssm_state,
                n_tokens,
                None,
                &work.gdn_delta_out,
                &work.gdn_state_tmp,
                &work.gdn_tape,
                state_roundtrip,
                1,
                num_k_heads as usize,
                num_v_heads as usize,
                head_k_dim as usize,
                head_v_dim as usize,
            )
        };
        if !gdn_ok {
            set_last_error("GDN.forward: gated_delta kernel failed");
            return false;
        }
        if stop_after("tape") {
            return true;
        }
        let state_elems = (num_v_heads as usize)
            * (head_v_dim as usize)
            * (head_k_dim as usize);
        let state_bytes = state_elems
            * if state_is_f32 {
                std::mem::size_of::<f32>()
            } else {
                std::mem::size_of::<half::bf16>()
            };
        let state_u32_words = ((state_bytes + std::mem::size_of::<u32>() - 1)
            / std::mem::size_of::<u32>()) as i32;
        if !K::copy_raw_u32(
            enc,
            dev,
            &work.gdn_state_tmp,
            ssm_state,
            state_u32_words,
        ) {
            set_last_error("GDN.forward: state copy-back failed");
            return false;
        }
        if stop_after("state_copy") {
            return true;
        }

        // 11. output_n = rms_norm(output) * ssm_norm_weight
        if !self.ssm_norm.forward(
            enc,
            dev,
            &work.gdn_delta_out,
            &work.gdn_output_n,
            (n_tokens as usize) * (num_v_heads as usize),
        ) {
            return false;
        }
        if stop_after("ssm_norm") {
            return true;
        }

        // 13. output_n *= silu(z)
        if !K::silu_mul_bf16(
            enc,
            dev,
            &work.gdn_z,
            &work.gdn_output_n,
            &work.gdn_output_n,
            n_tokens * self.d_inner,
        ) {
            return false;
        }
        if stop_after("silu_mul") {
            return true;
        }

        // 14. out = ssm_out @ output_n_flat   → [n_tokens, hidden]
        let ok = if gdn_qmm {
            self.ssm_out
                .forward_qmm_t(enc, dev, &work.gdn_output_n, y, n_tokens)
        } else {
            self.ssm_out.forward_with_verify_scratch(
                enc,
                dev,
                &work.gdn_output_n,
                y,
                n_tokens,
                Some(&work.verify_qmm_partials),
            )
        };
        if ok && stop_after("ssm_out") {
            return true;
        }
        ok
    }
}

// ─── Attention-free mini tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gqa_factor_basic() {
        let attn = Attention {
            wq: unreachable_linear(),
            wk: unreachable_linear(),
            wv: unreachable_linear(),
            wo: unreachable_linear(),
            q_norm: None,
            k_norm: None,
            rope: Rope {
                head_dim: 128,
                rope_dim: 64,
                base: 10_000_000.0,
            },
            n_heads: 24,
            n_kv_heads: 4,
            head_dim: 128,
        };
        assert_eq!(attn.gqa_factor(), 6);
    }

    // The tests below touch no actual GPU — they just exercise the
    // shape-arithmetic helpers. A `Device` would be needed for any
    // real forward test, which we defer to the Metal driver-level
    // integration tests.
    fn unreachable_linear() -> Linear4Bit {
        Linear4Bit {
            w_q: dummy_buffer(),
            scales: dummy_buffer(),
            biases: dummy_buffer(),
            in_features: 1,
            out_features: 1,
        }
    }

    fn dummy_buffer() -> Buffer {
        // Cheat: tests that need a real buffer should build a Device
        // first; the shape-arithmetic tests don't, so we use a tiny
        // buffer from the system device if available, else panic
        // with a clear message. Falling back to a panic here means
        // `cargo test` on a non-Metal host correctly skips rather
        // than silently compiling in a placeholder.
        let dev =
            crate::metal::ffi::global_device().expect("metal device required even for smoke tests");
        dev.new_buffer(16).expect("1-byte alloc should never fail")
    }
}
