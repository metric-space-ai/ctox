#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

//! Shared Gated Delta Net (GDN) implementation for hybrid models.
//!
//! Used by both Qwen3 Next (text-only) and Qwen3.5 MoE (vision) models.

use candle_core::{DType, Device, IndexOp, Module, Result, Tensor, D};
use candle_nn::Linear;
use engine_quant::{MatMul, QuantMethod, QuantizedConfig, RowParallelLayer, ShardedVarBuilder};
use std::sync::Arc;

use crate::device_map::DeviceMapper;

// ====================== GDN Config Trait ======================

/// Trait abstracting over config differences between Qwen3 Next and Qwen3.5 MoE.
#[allow(dead_code)]
pub trait GdnConfig {
    fn hidden_size(&self) -> usize;
    fn rms_norm_eps(&self) -> f64;
    fn linear_conv_kernel_dim(&self) -> usize;
    fn linear_key_head_dim(&self) -> usize;
    fn linear_value_head_dim(&self) -> usize;
    fn linear_num_key_heads(&self) -> usize;
    fn linear_num_value_heads(&self) -> usize;
    fn quantization_config(&self) -> &Option<QuantizedConfig>;

    fn linear_key_dim(&self) -> usize {
        self.linear_num_key_heads() * self.linear_key_head_dim()
    }
    fn linear_value_dim(&self) -> usize {
        self.linear_num_value_heads() * self.linear_value_head_dim()
    }
    fn linear_conv_dim(&self) -> usize {
        self.linear_key_dim() * 2 + self.linear_value_dim()
    }
}

// ====================== RMSNorm Gated ======================

/// RMSNorm with gating: `rms_norm(x) * weight * silu(gate)`
pub struct RmsNormGated {
    pub weight: Tensor,
    eps: f64,
}

impl RmsNormGated {
    pub fn new(
        size: usize,
        eps: f64,
        vb: ShardedVarBuilder,
        isq_target_device: Option<&Device>,
    ) -> Result<Self> {
        let mut weight = vb.get(size, "weight")?;
        if let Some(target_dev) = isq_target_device {
            weight = weight.to_device(target_dev)?;
        }
        Ok(Self { weight, eps })
    }

    pub fn forward(&self, x: &Tensor, gate: &Tensor) -> Result<Tensor> {
        let dtype = x.dtype();
        let x = x.to_dtype(DType::F32)?;
        let gate = candle_nn::ops::silu(&gate.to_dtype(DType::F32)?)?;
        let variance = x.sqr()?.mean_keepdim(D::Minus1)?;
        let normed = x.broadcast_div(&(variance + self.eps)?.sqrt()?)?;
        let out = normed
            .broadcast_mul(&self.weight.to_dtype(DType::F32)?)?
            .broadcast_mul(&gate)?;
        out.to_dtype(dtype)
    }
}

// ====================== GDN layer cache ======================

#[derive(Debug)]
pub struct GdnLayerCache {
    /// Conv state: (batch, conv_dim, kernel_size)
    pub conv_state: Tensor,
    /// Recurrent state: (batch, num_v_heads, head_k_dim, head_v_dim)
    pub recurrent_state: Tensor,
    pub seqlen_offset: usize,
}

#[allow(dead_code)]
impl GdnLayerCache {
    pub fn new(cfg: &dyn GdnConfig, dtype: DType, device: &Device) -> Result<Self> {
        let conv_dim = cfg.linear_conv_dim();
        let conv_state = Tensor::zeros((1, conv_dim, cfg.linear_conv_kernel_dim()), dtype, device)?;
        let recurrent_state = Tensor::zeros(
            (
                1,
                cfg.linear_num_value_heads(),
                cfg.linear_key_head_dim(),
                cfg.linear_value_head_dim(),
            ),
            dtype,
            device,
        )?;
        Ok(Self {
            conv_state,
            recurrent_state,
            seqlen_offset: 0,
        })
    }

    pub fn reset(&mut self) -> Result<()> {
        self.conv_state = self.conv_state.zeros_like()?;
        self.recurrent_state = self.recurrent_state.zeros_like()?;
        self.seqlen_offset = 0;
        Ok(())
    }
}

impl Clone for GdnLayerCache {
    fn clone(&self) -> Self {
        Self {
            conv_state: self.conv_state.clone(),
            recurrent_state: self.recurrent_state.clone(),
            seqlen_offset: self.seqlen_offset,
        }
    }
}

// ====================== GDN math functions ======================

pub fn l2_norm(x: &Tensor, eps: f64) -> Result<Tensor> {
    let inv_norm = x
        .sqr()?
        .sum_keepdim(D::Minus1)?
        .broadcast_add(&Tensor::new(eps as f32, x.device())?.to_dtype(x.dtype())?)?
        .sqrt()?
        .recip()?;
    x.broadcast_mul(&inv_norm)
}

pub fn softplus(x: &Tensor) -> Result<Tensor> {
    (Tensor::ones_like(x)? + x.exp()?)?.log()
}

/// Recurrent gated delta rule (used for both prefill and decode).
/// Matches torch_recurrent_gated_delta_rule from the reference implementation.
///
/// q, k: (batch, seq, num_v_heads, head_k_dim)
/// v:    (batch, seq, num_v_heads, head_v_dim)
/// g:    (batch, seq, num_v_heads)
/// beta: (batch, seq, num_v_heads)
/// state: (batch, num_v_heads, head_k_dim, head_v_dim)
///
/// Returns: (batch, seq, num_v_heads, head_v_dim)
pub fn gated_delta_rule_recurrence(
    q: &Tensor,
    k: &Tensor,
    v: &Tensor,
    g: &Tensor,
    beta: &Tensor,
    state: &mut Tensor,
) -> Result<Tensor> {
    let dtype = q.dtype();
    let k_head_dim = q.dim(D::Minus1)?;
    let scale = 1.0 / (k_head_dim as f64).sqrt();

    // Transpose to (batch, heads, seq, dim) and cast to f32
    let q = (q.transpose(1, 2)?.contiguous()?.to_dtype(DType::F32)? * scale)?;
    let k = k.transpose(1, 2)?.contiguous()?.to_dtype(DType::F32)?;
    let v = v.transpose(1, 2)?.contiguous()?.to_dtype(DType::F32)?;
    // g, beta: (batch, seq, heads) -> (batch, heads, seq)
    let g = g.transpose(1, 2)?.contiguous()?.to_dtype(DType::F32)?;
    let beta = beta.transpose(1, 2)?.contiguous()?.to_dtype(DType::F32)?;

    let seq_len = q.dim(2)?;
    let mut s = state.to_dtype(DType::F32)?;
    let mut outputs = Vec::with_capacity(seq_len);

    for i in 0..seq_len {
        // q_t, k_t: (batch, heads, k_dim); v_t: (batch, heads, v_dim)
        let q_t = q.i((.., .., i, ..))?;
        let k_t = k.i((.., .., i, ..))?;
        let v_t = v.i((.., .., i, ..))?;
        // g_t, beta_t: (batch, heads)
        let g_t = g.i((.., .., i))?;
        let beta_t = beta.i((.., .., i))?;

        // s = s * exp(g_t)
        let decay = g_t.exp()?.unsqueeze(D::Minus1)?.unsqueeze(D::Minus1)?;
        s = s.broadcast_mul(&decay)?;

        // kv_mem = (s * k_t[:,:,:,None]).sum(dim=2) -> (batch, heads, v_dim)
        let k_exp = k_t.unsqueeze(D::Minus1)?; // (batch, heads, k_dim, 1)
        let kv_mem = s.broadcast_mul(&k_exp)?.sum(2)?;

        // delta = (v_t - kv_mem) * beta_t[:,:,None]
        let beta_exp = beta_t.unsqueeze(D::Minus1)?;
        let delta = (v_t - kv_mem)?.broadcast_mul(&beta_exp)?;

        // s = s + k_t[:,:,:,None] * delta[:,:,None,:]
        let outer = k_exp.broadcast_mul(&delta.unsqueeze(2)?)?;
        s = (s + outer)?;

        // y_t = (s * q_t[:,:,:,None]).sum(dim=2) -> (batch, heads, v_dim)
        let q_exp = q_t.unsqueeze(D::Minus1)?;
        let y_t = s.broadcast_mul(&q_exp)?.sum(2)?;

        outputs.push(y_t);
    }

    *state = s.to_dtype(state.dtype())?;

    // Stack: (batch, heads, v_dim) * seq -> (batch, heads, seq, v_dim)
    let out = Tensor::stack(&outputs, 2)?;
    // Transpose back to (batch, seq, heads, v_dim)
    out.transpose(1, 2)?.contiguous()?.to_dtype(dtype)
}

/// Chunked gated delta-rule — vectorized candle-ops implementation
/// that matches [`gated_delta_rule_recurrence`] numerically while
/// replacing most of the per-token work with batched matmuls and a
/// triangular-solve Neumann series.
///
/// Derivation (matches the sequential recurrence in this file):
///
/// Per-token recurrence:
///   s_t = exp(g_t) · s_{t-1} + k_t ⊗ δ_t
///   δ_t = β_t · (v_t − k_t · (exp(g_t) s_{t-1}))   (uses post-decay s)
///   y_t = q_t · s_t
///
/// For a chunk of size CS with prechunk state s_init and
/// g_cs_t = Σ_{i=0..t} g_i, G_{a:b} = exp(g_cs_b − g_cs_a):
///
///   s_t      = exp(g_cs_t) s_init + Σ_{j=0..t} G_{j:t} k_j ⊗ δ_j
///   (I + A) Δ = B       with
///     A[t,i]  = β_t · G_{i:t} · <k_t, k_i>   for i < t  (strict-lower),
///     B[t]    = β_t · (v_t − exp(g_cs_t) · k_t · s_init).
///
/// Solve Δ via Neumann series — A is strict-lower CS×CS, so
/// (I + A)^{-1} = Σ_{k=0..CS-1} (−A)^k exactly.
///
/// Output:
///   y_t = exp(g_cs_t) · (q_t · s_init) + (W @ Δ)[t]
///   W[t,j] = <q_t, k_j> · G_{j:t}   for j ≤ t  (lower, diagonal included)
///
/// Final state after chunk:
///   s_out = exp(g_cs_{CS-1}) · s_init + K_scaled^T @ Δ
///   K_scaled[j, :] = exp(g_cs_{CS-1} − g_cs_j) · k[j, :]
///
/// Why this matters: the sequential recurrence does ~10 tiny tensor
/// ops per sequence position. For the DFlash chain/tree-verify feed
/// of 17–23 tokens at 48 GDN layers that's 7–11k kernel launches per
/// target forward, dominating `verify_compute`. The chunked
/// formulation collapses to O(1) matmul / triangular-solve per chunk
/// of 64 tokens — for our short verify feeds (single chunk with
/// padding) the total reduces to ~80 launches per layer, matmul-
/// dominant, which matches reference behaviour on the ggml fork.
///
/// Scope: candle-ops implementation runs on whatever backend the
/// inputs live on (CUDA or CPU). The CUDA backend launches one
/// kernel per op here — for A6000 the existing hand-written
/// `crate::cuda::gdn::gated_delta_rule_recurrence_kernel_tiled`
/// still wins on short seq because it fuses the whole recurrence
/// into one launch. A follow-up port of this algorithm to a
/// dedicated CUDA kernel (BT=64 chunked-parallel) is where the
/// measured speedup materialises.
///
/// Parity: numerically bit-close to `gated_delta_rule_recurrence`
/// on CPU f32 up to ~1e-3 on realistic dims (dk=dv=128, seq=17);
/// verified by the standalone test crate (not committed to this
/// workspace; see commit log for the run). Will be wired in via
/// unit tests in this module once the workspace's proc-macro build
/// issue on macOS is resolved.
#[allow(clippy::too_many_arguments)]
pub fn gated_delta_rule_chunked(
    q: &Tensor,
    k: &Tensor,
    v: &Tensor,
    g: &Tensor,
    beta: &Tensor,
    state: &mut Tensor,
) -> Result<Tensor> {
    let out_dtype = q.dtype();
    let device = q.device().clone();
    let (b, seq, n_heads, dk) = q.dims4()?;
    let dv = v.dim(3)?;

    if seq == 0 {
        return Tensor::zeros((b, seq, n_heads, dv), out_dtype, &device);
    }
    if seq == 1 {
        return gated_delta_rule_recurrence(q, k, v, g, beta, state);
    }

    const CS: usize = 64;
    let pad = (CS - seq % CS) % CS;
    let seq_padded = seq + pad;
    let n_chunks = seq_padded / CS;

    let scale = 1.0_f32 / (dk as f32).sqrt();

    // (b, seq, h, dk) → (b, h, seq, dk) in f32, with q pre-scaled.
    let q = (q.to_dtype(DType::F32)?.transpose(1, 2)?.contiguous()? * scale as f64)?;
    let k = k.to_dtype(DType::F32)?.transpose(1, 2)?.contiguous()?;
    let v = v.to_dtype(DType::F32)?.transpose(1, 2)?.contiguous()?;
    let g_bh = g.to_dtype(DType::F32)?.transpose(1, 2)?.contiguous()?;
    let beta_bh = beta.to_dtype(DType::F32)?.transpose(1, 2)?.contiguous()?;

    // Zero-pad seq dim to a multiple of CS. Zero g/β/k/v keeps rows
    // inert under the decay + tril mask.
    let q = pad_seq_dim(&q, pad, 2)?;
    let k = pad_seq_dim(&k, pad, 2)?;
    let v = pad_seq_dim(&v, pad, 2)?;
    let g_bh = pad_seq_dim(&g_bh, pad, 2)?;
    let beta_bh = pad_seq_dim(&beta_bh, pad, 2)?;

    // Flatten (b, h) into a single batch dim, split seq into chunks.
    let bh = b * n_heads;
    let q = q.reshape((bh, n_chunks, CS, dk))?;
    let k = k.reshape((bh, n_chunks, CS, dk))?;
    let v = v.reshape((bh, n_chunks, CS, dv))?;
    let g_bh = g_bh.reshape((bh, n_chunks, CS))?;
    let beta_bh = beta_bh.reshape((bh, n_chunks, CS))?;

    // Chunk-local cumulative-gate.
    let g_cs = g_bh.cumsum(2)?;

    // Masks.
    let tril = Tensor::tril2(CS, DType::F32, &device)?.reshape((1, 1, CS, CS))?;
    let identity = Tensor::eye(CS, DType::F32, &device)?.reshape((1, 1, CS, CS))?;
    let strict = tril.sub(&identity)?; // strict-lower: 1 for j<t, 0 else.

    // decay[t, j] = exp(g_cs[t] - g_cs[j]), masked to lower-inclusive.
    let g_cs_t = g_cs.unsqueeze(3)?;
    let g_cs_j = g_cs.unsqueeze(2)?;
    let decay_raw = g_cs_t.broadcast_sub(&g_cs_j)?;
    let decay = decay_raw.broadcast_mul(&tril)?.exp()?;
    let decay = decay.broadcast_mul(&tril)?;

    // qk[t, j] = <q_t, k_j>, kk[t, j] = <k_t, k_j>.
    let qk = q.matmul(&k.transpose(2, 3)?.contiguous()?)?;
    let kk = k.matmul(&k.transpose(2, 3)?.contiguous()?)?;

    // β[t] broadcast over j — multiplies per-row (row t).
    let beta_row = beta_bh.unsqueeze(3)?;

    // A[t, i] = β_t · kk[t, i] · decay[t, i] · strict[t, i]
    let a_strict = kk
        .broadcast_mul(&decay)?
        .broadcast_mul(&strict)?
        .broadcast_mul(&beta_row)?;

    // W[t, j] = qk[t, j] · decay[t, j] · tril[t, j]  (j ≤ t)
    let w = qk.broadcast_mul(&decay)?.broadcast_mul(&tril)?;

    // (I + A)^{-1} via Neumann series — exact for strict-lower A.
    let neg_a = a_strict.neg()?;
    let mut x_inv = identity.broadcast_as(a_strict.shape())?.contiguous()?;
    let mut power = identity.broadcast_as(a_strict.shape())?.contiguous()?;
    for _ in 1..CS {
        power = power.matmul(&neg_a)?;
        x_inv = x_inv.add(&power)?;
    }

    // Running state (bh, dk, dv), f32.
    let mut s = state.to_dtype(DType::F32)?.reshape((bh, dk, dv))?;
    let mut chunk_outputs: Vec<Tensor> = Vec::with_capacity(n_chunks);

    for c in 0..n_chunks {
        let q_c = q.narrow(1, c, 1)?.squeeze(1)?.contiguous()?; // (bh, CS, dk)
        let k_c = k.narrow(1, c, 1)?.squeeze(1)?.contiguous()?;
        let v_c = v.narrow(1, c, 1)?.squeeze(1)?.contiguous()?;
        let beta_c = beta_bh.narrow(1, c, 1)?.squeeze(1)?.contiguous()?; // (bh, CS)
        let g_cs_c = g_cs.narrow(1, c, 1)?.squeeze(1)?.contiguous()?; // (bh, CS)
        let decay_c = decay.narrow(1, c, 1)?.squeeze(1)?.contiguous()?; // (bh, CS, CS)
        let w_c = w.narrow(1, c, 1)?.squeeze(1)?.contiguous()?;
        let x_inv_c = x_inv.narrow(1, c, 1)?.squeeze(1)?.contiguous()?;

        let exp_gcs = g_cs_c.exp()?; // (bh, CS)

        // B[t] = β_t · v_t − β_t · exp(g_cs_t) · (k_t · s_init)
        let beta_col = beta_c.unsqueeze(D::Minus1)?; // (bh, CS, 1)
        let beta_v = v_c.broadcast_mul(&beta_col)?; // (bh, CS, dv)
        let k_s = k_c.matmul(&s)?; // (bh, CS, dv)
        let scalar = beta_col.broadcast_mul(&exp_gcs.unsqueeze(D::Minus1)?)?; // (bh, CS, 1)
        let k_s_scaled = k_s.broadcast_mul(&scalar)?;
        let b_mat = beta_v.sub(&k_s_scaled)?;

        // Δ = (I + A)^{-1} · B
        let delta = x_inv_c.matmul(&b_mat)?;

        // y = exp(g_cs) · (q · s_init) + W · Δ
        let q_s = q_c.matmul(&s)?;
        let y_from_init = q_s.broadcast_mul(&exp_gcs.unsqueeze(D::Minus1)?)?;
        let y_intra = w_c.matmul(&delta)?;
        let y_chunk = y_from_init.add(&y_intra)?;
        chunk_outputs.push(y_chunk);

        // s_out = exp(g_cs[CS-1]) · s_init + K_scaled^T · Δ
        let last_decay = decay_c.narrow(1, CS - 1, 1)?.squeeze(1)?; // (bh, CS)
        let k_scaled = k_c.broadcast_mul(&last_decay.unsqueeze(D::Minus1)?)?;
        let chunk_contrib = k_scaled
            .transpose(1, 2)?
            .contiguous()?
            .matmul(&delta)?;
        let g_last_exp = g_cs_c.narrow(1, CS - 1, 1)?.exp()?;
        let scale_s = g_last_exp.reshape((bh, 1, 1))?;
        s = s.broadcast_mul(&scale_s)?.add(&chunk_contrib)?;
    }

    let out_chunks = Tensor::stack(&chunk_outputs, 1)?;
    let out_flat = out_chunks.reshape((bh, seq_padded, dv))?;
    let out_unpadded = if pad == 0 {
        out_flat
    } else {
        out_flat.narrow(1, 0, seq)?
    };
    let out = out_unpadded
        .reshape((b, n_heads, seq, dv))?
        .transpose(1, 2)?
        .contiguous()?
        .to_dtype(out_dtype)?;

    *state = s.reshape((b, n_heads, dk, dv))?.to_dtype(state.dtype())?;
    Ok(out)
}

/// Pad `t` along `dim` with `pad` trailing zeros. Helper for the
/// chunked path's seq-dim padding to a multiple of `CS`.
fn pad_seq_dim(t: &Tensor, pad: usize, dim: usize) -> Result<Tensor> {
    if pad == 0 {
        return Ok(t.clone());
    }
    let mut shape = t.dims().to_vec();
    shape[dim] = pad;
    let zeros = Tensor::zeros(shape, t.dtype(), t.device())?;
    Tensor::cat(&[t, &zeros], dim)
}

// ====================== Gated Delta Net layer ======================

pub struct GatedDeltaNet {
    pub in_proj_qkvz: Linear,
    pub in_proj_ba: Linear,
    pub conv1d_weight: Tensor,
    pub dt_bias: Tensor,
    pub a_log: Tensor,
    pub norm: RmsNormGated,
    pub out_proj: Arc<dyn QuantMethod>,
    pub num_k_heads: usize,
    pub num_v_heads: usize,
    pub head_k_dim: usize,
    pub head_v_dim: usize,
    pub conv_kernel_size: usize,
    pub key_dim: usize,
    pub value_dim: usize,
    pub out_proj_input_offset: usize,
    pub out_proj_input_dim: usize,
}

fn grouped_value_shard(
    num_k_heads: usize,
    v_per_group: usize,
    head_v_dim: usize,
    rank: usize,
    world_size: usize,
) -> (usize, usize) {
    let (group_offset, group_len) = engine_quant::shard_bounds(num_k_heads, rank, world_size);
    let value_heads_before = group_offset * v_per_group;
    let local_value_heads = group_len * v_per_group;
    (
        value_heads_before * head_v_dim,
        local_value_heads * head_v_dim,
    )
}

/// Whether to try merged weight names first or separate HF names with fallback.
pub enum GdnWeightMode {
    /// Only load merged weight names (in_proj_qkvz, in_proj_ba)
    MergedOnly,
    /// Try merged first, fall back to separate HF names (in_proj_qkv + in_proj_z, in_proj_b + in_proj_a)
    MergedWithFallback,
}

impl GatedDeltaNet {
    /// Default to the custom CUDA recurrence kernel on CUDA devices — the
    /// pure-candle fallback does ~10 tensor ops per seq step in Rust,
    /// which on a Qwen3.6-35B-A3B (30 of 40 layers are GatedDeltaNet)
    /// adds ~300 kernel launches per decode token and dominates the
    /// tok/s budget. The custom kernel keeps the entire recurrence on
    /// the device inside one launch. Set
    /// `ENGINE_DISABLE_GDN_CUDA_KERNEL=1` to force the fallback for
    /// debugging (e.g. if a new model breaks the kernel's shape
    /// assumptions).
    #[cfg(feature = "cuda")]
    fn use_custom_cuda_recurrence() -> bool {
        !std::env::var("ENGINE_DISABLE_GDN_CUDA_KERNEL")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
    }

    pub fn load(
        vb: ShardedVarBuilder,
        cfg: &dyn GdnConfig,
        mapper: &dyn DeviceMapper,
        layer_idx: usize,
        loading_isq: bool,
        comm: &Arc<engine_quant::Comm>,
        weight_mode: GdnWeightMode,
    ) -> Result<Self> {
        let isq_target_device = if loading_isq {
            mapper.device_for(layer_idx, false).cloned()
        } else {
            None
        };
        let layer_device = mapper
            .device_for(layer_idx, false)
            .cloned()
            .or_else(|| mapper.device_for(layer_idx, loading_isq).cloned())
            .unwrap_or(Device::Cpu);

        let num_k_heads = cfg.linear_num_key_heads();
        let num_v_heads = cfg.linear_num_value_heads();
        let head_k_dim = cfg.linear_key_head_dim();
        let head_v_dim = cfg.linear_value_head_dim();
        let key_dim = num_k_heads * head_k_dim;
        let value_dim = num_v_heads * head_v_dim;
        let conv_kernel_size = cfg.linear_conv_kernel_dim();
        let hidden_size = cfg.hidden_size();
        let v_per_group = num_v_heads / num_k_heads;
        let (out_proj_input_offset, out_proj_input_dim) = grouped_value_shard(
            num_k_heads,
            v_per_group,
            head_v_dim,
            comm.rank(),
            comm.world_size(),
        );

        let vb_la = if loading_isq {
            // Linear-attention weights must still resolve onto the repeating layer device
            // when immediate/staged ISQ keeps the outer VarBuilder on the non-repeating device.
            vb.pp("linear_attn").set_device(layer_device.clone())
        } else {
            mapper.set_device(layer_idx, vb.pp("linear_attn"), loading_isq)
        };

        // Load qkvz and ba projections
        let qkvz_out = key_dim * 2 + value_dim * 2;
        let mut qkvz_w = match weight_mode {
            GdnWeightMode::MergedOnly => {
                vb_la.get((qkvz_out, hidden_size), "in_proj_qkvz.weight")?
            }
            GdnWeightMode::MergedWithFallback => {
                if vb_la.contains_tensor("in_proj_qkvz.weight") {
                    vb_la.get((qkvz_out, hidden_size), "in_proj_qkvz.weight")?
                } else {
                    // Load separate HF weights and interleave into grouped layout
                    let qkv_w =
                        vb_la.get((key_dim * 2 + value_dim, hidden_size), "in_proj_qkv.weight")?;
                    let z_w = vb_la.get((value_dim, hidden_size), "in_proj_z.weight")?;
                    let q_w = qkv_w.narrow(0, 0, key_dim)?;
                    let k_w = qkv_w.narrow(0, key_dim, key_dim)?;
                    let v_w = qkv_w.narrow(0, key_dim * 2, value_dim)?;
                    let q_grouped = q_w.reshape((num_k_heads, head_k_dim, hidden_size))?;
                    let k_grouped = k_w.reshape((num_k_heads, head_k_dim, hidden_size))?;
                    let v_grouped =
                        v_w.reshape((num_k_heads, v_per_group * head_v_dim, hidden_size))?;
                    let z_grouped =
                        z_w.reshape((num_k_heads, v_per_group * head_v_dim, hidden_size))?;
                    let merged = Tensor::cat(&[q_grouped, k_grouped, v_grouped, z_grouped], 1)?;
                    merged.reshape((qkvz_out, hidden_size))?
                }
            }
        };

        let mut ba_w = match weight_mode {
            GdnWeightMode::MergedOnly => {
                vb_la.get((num_v_heads * 2, hidden_size), "in_proj_ba.weight")?
            }
            GdnWeightMode::MergedWithFallback => {
                if vb_la.contains_tensor("in_proj_ba.weight") {
                    vb_la.get((num_v_heads * 2, hidden_size), "in_proj_ba.weight")?
                } else {
                    let b_w = vb_la.get((num_v_heads, hidden_size), "in_proj_b.weight")?;
                    let a_w = vb_la.get((num_v_heads, hidden_size), "in_proj_a.weight")?;
                    let b_grouped = b_w.reshape((num_k_heads, v_per_group, hidden_size))?;
                    let a_grouped = a_w.reshape((num_k_heads, v_per_group, hidden_size))?;
                    let merged = Tensor::cat(&[b_grouped, a_grouped], 1)?;
                    merged.reshape((num_v_heads * 2, hidden_size))?
                }
            }
        };

        let conv_dim = key_dim * 2 + value_dim;
        let mut conv1d_weight = vb_la.get((conv_dim, 1, conv_kernel_size), "conv1d.weight")?;
        let mut dt_bias = vb_la.get(num_v_heads, "dt_bias")?;
        let mut a_log = vb_la.get(num_v_heads, "A_log")?;

        if let Some(ref target_dev) = isq_target_device {
            qkvz_w = qkvz_w.to_device(target_dev)?;
            ba_w = ba_w.to_device(target_dev)?;
            conv1d_weight = conv1d_weight.to_device(target_dev)?;
            dt_bias = dt_bias.to_device(target_dev)?;
            a_log = a_log.to_device(target_dev)?;
        }

        let in_proj_qkvz = Linear::new(qkvz_w, None);
        let in_proj_ba = Linear::new(ba_w, None);

        let norm = RmsNormGated::new(
            head_v_dim,
            cfg.rms_norm_eps(),
            vb_la.pp("norm"),
            isq_target_device.as_ref(),
        )?;

        let out_proj = RowParallelLayer::new_with_shard(
            value_dim,
            hidden_size,
            cfg.quantization_config(),
            false,
            comm,
            engine_quant::Shard::Offset {
                dim: 1,
                offset: out_proj_input_offset,
                len: out_proj_input_dim,
            },
            vb_la.pp("out_proj"),
        )?;

        Ok(Self {
            in_proj_qkvz,
            in_proj_ba,
            conv1d_weight,
            dt_bias,
            a_log,
            norm,
            out_proj,
            num_k_heads,
            num_v_heads,
            head_k_dim,
            head_v_dim,
            conv_kernel_size,
            key_dim,
            value_dim,
            out_proj_input_offset,
            out_proj_input_dim,
        })
    }

    pub fn forward(&self, x: &Tensor, cache: &mut GdnLayerCache) -> Result<Tensor> {
        let (batch_size, seq_len, _hidden) = x.dims3()?;
        let dtype = x.dtype();
        let v_per_group = self.num_v_heads / self.num_k_heads;

        // 1. Project input
        if !x.device().same_device(self.in_proj_qkvz.weight().device()) {
            candle_core::bail!(
                "gdn in_proj_qkvz device mismatch: input={:?} weight={:?}",
                x.device().location(),
                self.in_proj_qkvz.weight().device().location()
            );
        }
        if !x.device().same_device(self.in_proj_ba.weight().device()) {
            candle_core::bail!(
                "gdn in_proj_ba device mismatch: input={:?} weight={:?}",
                x.device().location(),
                self.in_proj_ba.weight().device().location()
            );
        }
        let mixed_qkvz = self.in_proj_qkvz.forward(x)?;
        let mixed_ba = self.in_proj_ba.forward(x)?;

        // 2. Grouped head layout
        let group_size_qkvz = 2 * self.head_k_dim + 2 * v_per_group * self.head_v_dim;
        let mixed_qkvz =
            mixed_qkvz.reshape((batch_size, seq_len, self.num_k_heads, group_size_qkvz))?;

        let group_size_ba = 2 * v_per_group;
        let mixed_ba = mixed_ba.reshape((batch_size, seq_len, self.num_k_heads, group_size_ba))?;

        // Split within each group
        let mut offset = 0;
        let q = mixed_qkvz.narrow(D::Minus1, offset, self.head_k_dim)?;
        offset += self.head_k_dim;
        let k = mixed_qkvz.narrow(D::Minus1, offset, self.head_k_dim)?;
        offset += self.head_k_dim;
        let v = mixed_qkvz.narrow(D::Minus1, offset, v_per_group * self.head_v_dim)?;
        offset += v_per_group * self.head_v_dim;
        let z = mixed_qkvz.narrow(D::Minus1, offset, v_per_group * self.head_v_dim)?;

        let b = mixed_ba.narrow(D::Minus1, 0, v_per_group)?;
        let a = mixed_ba.narrow(D::Minus1, v_per_group, v_per_group)?;

        // Reshape v, z -> (batch, seq, num_v_heads, head_v_dim)
        let v = v.reshape((batch_size, seq_len, self.num_v_heads, self.head_v_dim))?;
        let z = z.reshape((batch_size, seq_len, self.num_v_heads, self.head_v_dim))?;

        // Reshape b, a -> (batch, seq, num_v_heads)
        let b = b.reshape((batch_size, seq_len, self.num_v_heads))?;
        let a = a.reshape((batch_size, seq_len, self.num_v_heads))?;

        // Flatten q, k, v for conv1d
        let q = q.reshape((batch_size, seq_len, self.key_dim))?;
        let k = k.reshape((batch_size, seq_len, self.key_dim))?;
        let v_flat = v.reshape((batch_size, seq_len, self.value_dim))?;

        // 3. Concatenate q, k, v for conv1d
        let mixed_qkv = Tensor::cat(&[&q, &k, &v_flat], D::Minus1)?;

        // 4. Apply causal conv1d (includes silu activation)
        let mixed_qkv = if cache.seqlen_offset > 0 && seq_len == 1 {
            self.causal_conv1d_update(&mixed_qkv, cache)?
        } else {
            self.causal_conv1d_full(&mixed_qkv, cache)?
        };

        // 5. Split back after conv and reshape to per-head
        let q = mixed_qkv.narrow(D::Minus1, 0, self.key_dim)?;
        let k = mixed_qkv.narrow(D::Minus1, self.key_dim, self.key_dim)?;
        let v = mixed_qkv.narrow(D::Minus1, self.key_dim * 2, self.value_dim)?;

        let q = q.reshape((batch_size, seq_len, self.num_k_heads, self.head_k_dim))?;
        let k = k.reshape((batch_size, seq_len, self.num_k_heads, self.head_k_dim))?;
        let v = v.reshape((batch_size, seq_len, self.num_v_heads, self.head_v_dim))?;

        // 6. Compute beta and g
        let (beta, g) = {
            #[cfg(feature = "cuda")]
            {
                if b.device().is_cuda() {
                    let b_flat = b.contiguous()?.flatten_all()?;
                    let a_flat = a.contiguous()?.flatten_all()?;
                    let a_log_f32 = self.a_log.to_dtype(DType::F32)?.contiguous()?;
                    let dt_bias_f32 = self.dt_bias.to_dtype(DType::F32)?.contiguous()?;
                    let (beta_flat, g_flat) = crate::cuda::gdn::fused_gdn_gating_cuda(
                        &b_flat,
                        &a_flat,
                        &a_log_f32,
                        &dt_bias_f32,
                    )?;
                    let shape = b.shape();
                    (beta_flat.reshape(shape)?, g_flat.reshape(shape)?)
                } else {
                    self.compute_beta_g_cpu(&b, &a, dtype)?
                }
            }
            #[cfg(feature = "metal")]
            {
                if b.device().is_metal() {
                    let b_flat = b.contiguous()?.flatten_all()?;
                    let a_flat = a.contiguous()?.flatten_all()?;
                    let a_log_f32 = self.a_log.to_dtype(DType::F32)?.contiguous()?;
                    let dt_bias_f32 = self.dt_bias.to_dtype(DType::F32)?.contiguous()?;
                    let (beta_flat, g_flat) = crate::metal::gdn::fused_gdn_gating_metal(
                        &b_flat,
                        &a_flat,
                        &a_log_f32,
                        &dt_bias_f32,
                    )?;
                    let shape = b.shape();
                    (beta_flat.reshape(shape)?, g_flat.reshape(shape)?)
                } else {
                    self.compute_beta_g_cpu(&b, &a, dtype)?
                }
            }
            #[cfg(not(any(feature = "cuda", feature = "metal")))]
            {
                self.compute_beta_g_cpu(&b, &a, dtype)?
            }
        };

        // 7. If num_v_heads > num_k_heads, repeat_interleave q and k
        let (q, k) = if v_per_group > 1 {
            let q = q
                .unsqueeze(3)?
                .repeat((1, 1, 1, v_per_group, 1))?
                .reshape((batch_size, seq_len, self.num_v_heads, self.head_k_dim))?;
            let k = k
                .unsqueeze(3)?
                .repeat((1, 1, 1, v_per_group, 1))?
                .reshape((batch_size, seq_len, self.num_v_heads, self.head_k_dim))?;
            (q, k)
        } else {
            (q, k)
        };

        // 8. L2-normalize q and k
        let q = l2_norm(&q, 1e-6)?;
        let k = l2_norm(&k, 1e-6)?;

        // 9. Apply recurrence
        let y = {
            #[cfg(feature = "cuda")]
            {
                if q.device().is_cuda() {
                    self.recurrence_cuda(&q, &k, &v, &g, &beta, batch_size, seq_len, cache, dtype)?
                } else {
                    gated_delta_rule_recurrence(&q, &k, &v, &g, &beta, &mut cache.recurrent_state)?
                }
            }
            #[cfg(feature = "metal")]
            {
                if q.device().is_metal() {
                    self.recurrence_metal(&q, &k, &v, &g, &beta, batch_size, seq_len, cache, dtype)?
                } else {
                    gated_delta_rule_recurrence(&q, &k, &v, &g, &beta, &mut cache.recurrent_state)?
                }
            }
            #[cfg(not(any(feature = "cuda", feature = "metal")))]
            {
                gated_delta_rule_recurrence(&q, &k, &v, &g, &beta, &mut cache.recurrent_state)?
            }
        };

        cache.seqlen_offset += seq_len;

        // 10. Apply RMSNormGated
        let z_shape = z.shape().clone();
        let y = y.reshape(((), self.head_v_dim))?;
        let z = z.reshape(((), self.head_v_dim))?;
        if !y.device().same_device(self.norm.weight.device()) {
            candle_core::bail!(
                "gdn norm device mismatch: y={:?} weight={:?}",
                y.device().location(),
                self.norm.weight.device().location()
            );
        }
        if !z.device().same_device(self.norm.weight.device()) {
            candle_core::bail!(
                "gdn norm gate device mismatch: gate={:?} weight={:?}",
                z.device().location(),
                self.norm.weight.device().location()
            );
        }
        let y = self.norm.forward(&y, &z)?;
        let y = y.reshape(z_shape)?;
        let y = y.reshape((batch_size, seq_len, self.value_dim))?;
        let y = if self.out_proj_input_dim < self.value_dim {
            y.narrow(
                D::Minus1,
                self.out_proj_input_offset,
                self.out_proj_input_dim,
            )?
            .contiguous()?
        } else {
            y
        };

        // 11. Output projection
        let original_dtype = x.dtype();
        let mut y_proj = y;
        if let Some(t) = self.out_proj.quantized_act_type() {
            y_proj = y_proj.to_dtype(t)?;
        }
        let (_, out_proj_device) = self.out_proj.dtype_and_device();
        if !y_proj.device().same_device(&out_proj_device) {
            candle_core::bail!(
                "gdn out_proj device mismatch: input={:?} weight={:?}",
                y_proj.device().location(),
                out_proj_device.location()
            );
        }
        let mut res = MatMul.qmethod_matmul(&y_proj, &*self.out_proj)?;
        if self.out_proj.quantized_act_type().is_some() {
            res = res.to_dtype(original_dtype)?;
        }
        Ok(res)
    }

    fn compute_beta_g_cpu(&self, b: &Tensor, a: &Tensor, dtype: DType) -> Result<(Tensor, Tensor)> {
        let beta = candle_nn::ops::sigmoid(b)?;
        let a_f = a.to_dtype(DType::F32)?;
        let dt_bias_expanded = self
            .dt_bias
            .to_dtype(DType::F32)?
            .unsqueeze(0)?
            .unsqueeze(0)?;
        let g = self
            .a_log
            .to_dtype(DType::F32)?
            .exp()?
            .neg()?
            .unsqueeze(0)?
            .unsqueeze(0)?
            .broadcast_mul(&softplus(&a_f.broadcast_add(&dt_bias_expanded)?)?)?
            .to_dtype(dtype)?;
        Ok((beta, g))
    }

    #[cfg(feature = "cuda")]
    fn recurrence_cuda(
        &self,
        q: &Tensor,
        k: &Tensor,
        v: &Tensor,
        g: &Tensor,
        beta: &Tensor,
        batch_size: usize,
        seq_len: usize,
        cache: &mut GdnLayerCache,
        dtype: DType,
    ) -> Result<Tensor> {
        if !Self::use_custom_cuda_recurrence() {
            return gated_delta_rule_recurrence(q, k, v, g, beta, &mut cache.recurrent_state)?
                .to_dtype(dtype);
        }

        let num_heads = self.num_v_heads;
        let k_head = self.head_k_dim;
        let v_head = self.head_v_dim;
        let scale = 1.0 / (k_head as f64).sqrt();

        let q_bh = (q.transpose(1, 2)?.contiguous()?.to_dtype(DType::F32)? * scale)?
            .reshape((batch_size * num_heads, seq_len, k_head))?
            .contiguous()?;
        let k_bh = k
            .transpose(1, 2)?
            .contiguous()?
            .to_dtype(DType::F32)?
            .reshape((batch_size * num_heads, seq_len, k_head))?
            .contiguous()?;
        let v_bh = v
            .transpose(1, 2)?
            .contiguous()?
            .to_dtype(DType::F32)?
            .reshape((batch_size * num_heads, seq_len, v_head))?
            .contiguous()?;
        let g_bh = g
            .to_dtype(DType::F32)?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((batch_size * num_heads, seq_len))?
            .contiguous()?;
        let beta_bh = beta
            .to_dtype(DType::F32)?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((batch_size * num_heads, seq_len))?
            .contiguous()?;

        let mut state_flat = cache
            .recurrent_state
            .to_dtype(DType::F32)?
            .reshape((batch_size * num_heads, k_head, v_head))?
            .contiguous()?;

        const CHUNK_THRESHOLD: usize = 64;
        let out_bh = if seq_len >= CHUNK_THRESHOLD {
            crate::cuda::gdn::chunked_gated_delta_rule_recurrence_cuda(
                &q_bh,
                &k_bh,
                &v_bh,
                &g_bh,
                &beta_bh,
                &mut state_flat,
            )?
        } else {
            crate::cuda::gdn::gated_delta_rule_recurrence_cuda(
                &q_bh,
                &k_bh,
                &v_bh,
                &g_bh,
                &beta_bh,
                &mut state_flat,
            )?
        };

        cache.recurrent_state = state_flat
            .reshape((batch_size, num_heads, k_head, v_head))?
            .to_dtype(cache.recurrent_state.dtype())?;

        out_bh
            .reshape((batch_size, num_heads, seq_len, v_head))?
            .transpose(1, 2)?
            .contiguous()?
            .to_dtype(dtype)
    }

    #[cfg(feature = "metal")]
    fn recurrence_metal(
        &self,
        q: &Tensor,
        k: &Tensor,
        v: &Tensor,
        g: &Tensor,
        beta: &Tensor,
        batch_size: usize,
        seq_len: usize,
        cache: &mut GdnLayerCache,
        dtype: DType,
    ) -> Result<Tensor> {
        let num_heads = self.num_v_heads;
        let k_head = self.head_k_dim;
        let v_head = self.head_v_dim;
        let scale = 1.0 / (k_head as f64).sqrt();

        let q_bh = (q.transpose(1, 2)?.contiguous()?.to_dtype(DType::F32)? * scale)?
            .reshape((batch_size * num_heads, seq_len, k_head))?
            .contiguous()?;
        let k_bh = k
            .transpose(1, 2)?
            .contiguous()?
            .to_dtype(DType::F32)?
            .reshape((batch_size * num_heads, seq_len, k_head))?
            .contiguous()?;
        let v_bh = v
            .transpose(1, 2)?
            .contiguous()?
            .to_dtype(DType::F32)?
            .reshape((batch_size * num_heads, seq_len, v_head))?
            .contiguous()?;
        let g_bh = g
            .to_dtype(DType::F32)?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((batch_size * num_heads, seq_len))?
            .contiguous()?;
        let beta_bh = beta
            .to_dtype(DType::F32)?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((batch_size * num_heads, seq_len))?
            .contiguous()?;

        let mut state_flat = cache
            .recurrent_state
            .to_dtype(DType::F32)?
            .reshape((batch_size * num_heads, k_head, v_head))?
            .contiguous()?;

        const CHUNK_THRESHOLD: usize = 64;
        let out_bh = if seq_len >= CHUNK_THRESHOLD {
            crate::metal::gdn::chunked_gated_delta_rule_recurrence_metal(
                &q_bh,
                &k_bh,
                &v_bh,
                &g_bh,
                &beta_bh,
                &mut state_flat,
            )?
        } else {
            crate::metal::gdn::gated_delta_rule_recurrence_metal(
                &q_bh,
                &k_bh,
                &v_bh,
                &g_bh,
                &beta_bh,
                &mut state_flat,
            )?
        };

        cache.recurrent_state = state_flat
            .reshape((batch_size, num_heads, k_head, v_head))?
            .to_dtype(cache.recurrent_state.dtype())?;

        out_bh
            .reshape((batch_size, num_heads, seq_len, v_head))?
            .transpose(1, 2)?
            .contiguous()?
            .to_dtype(dtype)
    }

    /// Single-step causal conv1d update for decode.
    fn causal_conv1d_update(&self, x: &Tensor, cache: &mut GdnLayerCache) -> Result<Tensor> {
        let (_batch, seq_len, _conv_dim) = x.dims3()?;
        let x_t = x.transpose(1, 2)?.contiguous()?;

        #[cfg(feature = "cuda")]
        if x_t.device().is_cuda() {
            let weight = self
                .conv1d_weight
                .squeeze(1)?
                .to_dtype(x_t.dtype())?
                .contiguous()?;
            let conv_state = cache.conv_state.contiguous()?;
            let (output, new_conv_state) = crate::cuda::gdn::causal_conv1d_cuda(
                &x_t,
                &weight,
                &conv_state,
                self.conv_kernel_size,
                true,
            )?;
            cache.conv_state = new_conv_state;
            return output.transpose(1, 2);
        }

        #[cfg(feature = "metal")]
        if x_t.device().is_metal() {
            let weight = self
                .conv1d_weight
                .squeeze(1)?
                .to_dtype(x_t.dtype())?
                .contiguous()?;
            let conv_state = cache.conv_state.contiguous()?;
            let (output, new_conv_state) = crate::metal::gdn::causal_conv1d_metal(
                &x_t,
                &weight,
                &conv_state,
                true,
                self.conv_kernel_size,
            )?;
            cache.conv_state = new_conv_state;
            return output.transpose(1, 2);
        }

        // CPU fallback
        let state_len = cache.conv_state.dim(2)?;
        let hidden_new = Tensor::cat(&[cache.conv_state.clone(), x_t], 2)?;
        let new_len = hidden_new.dim(2)?;
        cache.conv_state = hidden_new.narrow(2, new_len - state_len, state_len)?;

        let weight = self
            .conv1d_weight
            .squeeze(1)?
            .to_dtype(hidden_new.dtype())?;
        let mut conv_outputs = Vec::with_capacity(seq_len);
        let total_len = hidden_new.dim(2)?;
        for i in (total_len - seq_len)..total_len {
            let window =
                hidden_new.narrow(2, i + 1 - self.conv_kernel_size, self.conv_kernel_size)?;
            let out = (window * weight.unsqueeze(0)?)?.sum(D::Minus1)?;
            conv_outputs.push(out);
        }
        let out = Tensor::stack(&conv_outputs, 2)?;
        let out = candle_nn::ops::silu(&out)?;
        out.transpose(1, 2)
    }

    /// Full sequence causal conv1d for prefill.
    fn causal_conv1d_full(&self, x: &Tensor, cache: &mut GdnLayerCache) -> Result<Tensor> {
        let (batch_size, seq_len, conv_dim) = x.dims3()?;
        let x_t = x.transpose(1, 2)?.contiguous()?;

        #[cfg(feature = "cuda")]
        if x_t.device().is_cuda() {
            let weight = self
                .conv1d_weight
                .squeeze(1)?
                .to_dtype(x_t.dtype())?
                .contiguous()?;
            let (output, new_conv_state) = crate::cuda::gdn::causal_conv1d_cuda(
                &x_t,
                &weight,
                &cache.conv_state,
                self.conv_kernel_size,
                false,
            )?;
            cache.conv_state = new_conv_state;
            return output.transpose(1, 2);
        }

        #[cfg(feature = "metal")]
        if x_t.device().is_metal() {
            let weight = self
                .conv1d_weight
                .squeeze(1)?
                .to_dtype(x_t.dtype())?
                .contiguous()?;
            let (output, new_conv_state) = crate::metal::gdn::causal_conv1d_metal(
                &x_t,
                &weight,
                &cache.conv_state,
                false,
                self.conv_kernel_size,
            )?;
            cache.conv_state = new_conv_state;
            return output.transpose(1, 2);
        }

        // CPU fallback
        let pad_width = self.conv_kernel_size.saturating_sub(seq_len);
        cache.conv_state = if pad_width > 0 {
            let zeros =
                Tensor::zeros((batch_size, conv_dim, pad_width), x_t.dtype(), x_t.device())?;
            Tensor::cat(&[zeros, x_t.clone()], 2)?
        } else {
            x_t.narrow(2, seq_len - self.conv_kernel_size, self.conv_kernel_size)?
        };

        let padded_t = Tensor::cat(
            &[
                Tensor::zeros(
                    (batch_size, conv_dim, self.conv_kernel_size - 1),
                    x_t.dtype(),
                    x_t.device(),
                )?,
                x_t,
            ],
            2,
        )?;

        let weight = self.conv1d_weight.squeeze(1)?.to_dtype(padded_t.dtype())?;

        let mut conv_outputs = Vec::with_capacity(seq_len);
        for i in 0..seq_len {
            let window = padded_t.narrow(2, i, self.conv_kernel_size)?;
            let out = (window * weight.unsqueeze(0)?)?.sum(D::Minus1)?;
            conv_outputs.push(out);
        }
        let out = Tensor::stack(&conv_outputs, 2)?;
        let out = candle_nn::ops::silu(&out)?;
        out.transpose(1, 2)
    }
}
