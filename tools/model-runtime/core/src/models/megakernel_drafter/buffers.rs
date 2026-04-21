//! CUDA buffer allocation for the Qwen3.5-0.8B megakernel drafter.
//!
//! The megakernel has two entry points (`launch_prefill_bf16`,
//! `launch_decode`) each taking a fat tuple of raw device pointers
//! — ~14 pointers for prefill, ~20 for decode, plus the shared
//! KV / DN-state / conv-sliding-window buffers. This module owns
//! the allocation of all those buffers via candle's CUDA device
//! (so they live on the same device as the model weights) and
//! exposes them as raw device pointers on demand.
//!
//! Memory budget (all on-device, single drafter instance):
//!   * fa_k_cache / fa_v_cache : 2 × (6 FA × 2 KV-heads × 2048 × 256)
//!                               × 2 bytes = 12 MB each = 24 MB total.
//!   * dn_states               : 18 DN × 16 heads × 128 × 128 × 4 =
//!                               18 MB (f32).
//!   * conv_bufs               : 18 DN × 6144 × 4 × 4 = 1.7 MB (f32).
//!   * per-block scratch       : ~100 KB.
//! Totals ≈ 44 MB. Negligible alongside the 27B target.
//!
//! All buffers are allocated once in [`MegakernelBuffers::new`] and
//! reused across every prefill/decode call. `reset()` zeroes the
//! stateful buffers (KV, DN state, conv, barriers) without
//! re-allocating, so runs between user requests don't leak context.

#![cfg(feature = "cuda")]

use candle_core::{DType, Device, Result, Tensor};

use super::constants::*;

/// All device-resident scratch / state buffers the megakernel needs.
/// Exactly one allocation per kind; on decode the same buffers are
/// passed to `launch_decode`, on prefill a separate (larger) set of
/// scratch buffers is used because the prefill kernel processes `S`
/// tokens in parallel through cuBLAS BF16 GEMMs. Both kernels share
/// the same KV cache / DN state / conv-buf buffers so decode picks
/// up where prefill left off.
pub struct MegakernelBuffers {
    pub device: Device,

    // ── Shared state buffers (persist between prefill and decode) ──
    /// [n_fa, FA_KV_HEADS, MAX_SEQ_LEN, FA_HEAD_DIM] BF16
    pub fa_k_cache: Tensor,
    /// Same shape as `fa_k_cache`.
    pub fa_v_cache: Tensor,
    /// [n_dn, DN_HEADS, DN_KEY_DIM, DN_VALUE_DIM] F32
    pub dn_states: Tensor,
    /// [n_dn, DN_CONV_CHANNELS, DN_CONV_KERNEL] F32
    pub conv_bufs: Tensor,

    // ── Decode scratch (sized per-block, kernel.cu) ──
    pub hidden_buffer: Tensor,
    pub g_activations: Tensor,
    pub g_residual: Tensor,
    pub g_qkv_scratch: Tensor,
    pub g_kv_scratch: Tensor,
    pub g_attn_out: Tensor,
    pub g_mlp_inter: Tensor,
    pub g_z_scratch: Tensor,
    pub g_beta_scratch: Tensor,
    pub g_alpha_scratch: Tensor,
    pub g_normalized: Tensor,
    pub barrier_counter: Tensor, // u32
    pub barrier_generation: Tensor, // u32
    pub block_max_vals: Tensor,  // f32, size LM_NUM_BLOCKS
    pub block_max_idxs: Tensor,  // i32, size LM_NUM_BLOCKS
    pub lm_sync_counter: Tensor, // u32
    pub out_token: Tensor,       // i32, size 1 — holds decode/prefill output

    // ── Prefill scratch (prefill.cu) ──
    /// bf16, size S × HIDDEN — filled by pf_embed, reused by rmsnorm.
    pub pf_hidden: Tensor,
    /// bf16, size S × HIDDEN — residual stream.
    pub pf_residual: Tensor,
    /// bf16, size S × HIDDEN — normalised stream.
    pub pf_normalized: Tensor,
    /// bf16, size S × max(FA_QPROJ_SIZE, DN_CONV_CHANNELS, INTERMEDIATE).
    pub pf_proj_buf: Tensor,
    /// Same size as proj_buf — second GEMM output slot.
    pub pf_proj_buf2: Tensor,
    /// bf16, size S × max(FA_Q_SIZE, DN_V_SIZE).
    pub pf_attn_buf: Tensor,
    /// bf16, size S × INTERMEDIATE.
    pub pf_mlp_buf: Tensor,
    /// bf16, size S × DN_V_SIZE.
    pub pf_dn_out_buf: Tensor,
    /// f32, size S × DN_HEADS.
    pub pf_beta_buf: Tensor,
    /// f32, size S × DN_HEADS.
    pub pf_alpha_buf: Tensor,
    /// bf16, size HIDDEN.
    pub pf_final_normed: Tensor,
    /// bf16, size HIDDEN — final-token hidden in bf16.
    pub pf_hidden_bf16_out: Tensor,
    /// f32, size LM_NUM_BLOCKS.
    pub pf_lm_bmv: Tensor,
    /// i32, size LM_NUM_BLOCKS.
    pub pf_lm_bmi: Tensor,

    /// Maximum prefill sequence length this buffer set was sized for.
    /// If a prefill call arrives with `seq_len > max_prefill_seq`,
    /// `MegakernelDrafter::prefill` will refuse.
    pub max_prefill_seq: usize,
}

impl MegakernelBuffers {
    /// Allocate all buffers on `device` sized for up to
    /// `max_prefill_seq` prompt tokens on a single prefill call.
    /// `max_prefill_seq` only bounds the prefill scratch; the KV
    /// ring capacity is [`MAX_SEQ_LEN`] regardless.
    pub fn new(device: &Device, max_prefill_seq: usize) -> Result<Self> {
        if !device.is_cuda() {
            candle_core::bail!(
                "MegakernelBuffers::new: device must be CUDA (got {:?})",
                device.location()
            );
        }
        if max_prefill_seq == 0 {
            candle_core::bail!("MegakernelBuffers::new: max_prefill_seq must be > 0");
        }
        if max_prefill_seq > MAX_SEQ_LEN {
            candle_core::bail!(
                "MegakernelBuffers::new: max_prefill_seq {} exceeds KV ring cap {}",
                max_prefill_seq,
                MAX_SEQ_LEN
            );
        }

        // ── Shared state.
        let fa_k_cache = Tensor::zeros(
            (N_FA_LAYERS, FA_NUM_KV_HEADS, MAX_SEQ_LEN, FA_HEAD_DIM),
            DType::BF16,
            device,
        )?;
        let fa_v_cache = Tensor::zeros(
            (N_FA_LAYERS, FA_NUM_KV_HEADS, MAX_SEQ_LEN, FA_HEAD_DIM),
            DType::BF16,
            device,
        )?;
        let dn_states = Tensor::zeros(
            (N_DN_LAYERS, DN_NUM_HEADS, DN_KEY_DIM, DN_VALUE_DIM),
            DType::F32,
            device,
        )?;
        let conv_bufs = Tensor::zeros(
            (N_DN_LAYERS, DN_CONV_CHANNELS, DN_CONV_KERNEL),
            DType::F32,
            device,
        )?;

        // ── Decode scratch: per-token, sized per the kernel.cu
        //    constants. Python reference uses f32 for most of these
        //    and bf16 for a couple — we follow the same dtype choices
        //    so the kernel's pointer casts (`(float*)`, `(__nv_bfloat16
        //    *)`) agree with the buffer dtype. A dtype mismatch here
        //    manifests as silent memory scrambling inside the kernel.
        let max_scratch = FA_QPROJ_SIZE
            .max(DN_CONV_CHANNELS)
            .max(HIDDEN_SIZE * 8 + INTERMEDIATE_SIZE);
        let hidden_buffer = Tensor::zeros((HIDDEN_SIZE,), DType::BF16, device)?;
        let g_activations = Tensor::zeros((max_scratch,), DType::F32, device)?;
        let g_residual = Tensor::zeros((HIDDEN_SIZE,), DType::BF16, device)?;
        let g_qkv_scratch = Tensor::zeros(
            (FA_QPROJ_SIZE.max(DN_CONV_CHANNELS),),
            DType::F32,
            device,
        )?;
        let g_kv_scratch = Tensor::zeros((FA_KV_SIZE * 2,), DType::F32, device)?;
        let g_attn_out = Tensor::zeros((FA_Q_SIZE.max(DN_V_SIZE),), DType::F32, device)?;
        let g_mlp_inter = Tensor::zeros((INTERMEDIATE_SIZE,), DType::F32, device)?;
        let g_z_scratch = Tensor::zeros((DN_V_SIZE,), DType::F32, device)?;
        let g_beta_scratch = Tensor::zeros((DN_NUM_HEADS,), DType::F32, device)?;
        let g_alpha_scratch = Tensor::zeros((DN_NUM_HEADS,), DType::F32, device)?;
        let g_normalized = Tensor::zeros((HIDDEN_SIZE,), DType::F32, device)?;
        let barrier_counter = Tensor::zeros((1,), DType::U32, device)?;
        let barrier_generation = Tensor::zeros((1,), DType::U32, device)?;
        let block_max_vals = Tensor::zeros((1024,), DType::F32, device)?;
        // Kernel writes int[1024] here — dtype MUST be I32 (4-byte
        // elements). I64 would stride-mismatch by 2×.
        let block_max_idxs = Tensor::zeros((1024,), DType::I32, device)?;
        let lm_sync_counter = Tensor::zeros((1,), DType::U32, device)?;
        // Kernel writes a single int (4 bytes) into out_token.
        let out_token = Tensor::zeros((1,), DType::I32, device)?;

        // ── Prefill scratch: per-prompt-token (S-dependent), sized
        //    for `max_prefill_seq`. These are separate allocations
        //    from the decode scratch because the decode kernel
        //    expects per-block fixed-size arrays.
        let s = max_prefill_seq;
        let pf_hidden = Tensor::zeros((s, HIDDEN_SIZE), DType::BF16, device)?;
        let pf_residual = Tensor::zeros((s, HIDDEN_SIZE), DType::BF16, device)?;
        let pf_normalized = Tensor::zeros((s, HIDDEN_SIZE), DType::BF16, device)?;
        let pf_proj_max = FA_QPROJ_SIZE
            .max(DN_CONV_CHANNELS)
            .max(INTERMEDIATE_SIZE);
        let pf_proj_buf = Tensor::zeros((s, pf_proj_max), DType::BF16, device)?;
        let pf_proj_buf2 = Tensor::zeros((s, pf_proj_max), DType::BF16, device)?;
        let pf_attn_buf = Tensor::zeros((s, FA_Q_SIZE.max(DN_V_SIZE)), DType::BF16, device)?;
        let pf_mlp_buf = Tensor::zeros((s, INTERMEDIATE_SIZE), DType::BF16, device)?;
        let pf_dn_out_buf = Tensor::zeros((s, DN_V_SIZE), DType::BF16, device)?;
        let pf_beta_buf = Tensor::zeros((s, DN_NUM_HEADS), DType::F32, device)?;
        let pf_alpha_buf = Tensor::zeros((s, DN_NUM_HEADS), DType::F32, device)?;
        let pf_final_normed = Tensor::zeros((HIDDEN_SIZE,), DType::BF16, device)?;
        let pf_hidden_bf16_out = Tensor::zeros((HIDDEN_SIZE,), DType::BF16, device)?;
        let pf_lm_bmv = Tensor::zeros((1024,), DType::F32, device)?;
        // Matches the prefill kernel's `int *lm_bmi` — 4-byte indices.
        let pf_lm_bmi = Tensor::zeros((1024,), DType::I32, device)?;

        Ok(Self {
            device: device.clone(),
            fa_k_cache,
            fa_v_cache,
            dn_states,
            conv_bufs,
            hidden_buffer,
            g_activations,
            g_residual,
            g_qkv_scratch,
            g_kv_scratch,
            g_attn_out,
            g_mlp_inter,
            g_z_scratch,
            g_beta_scratch,
            g_alpha_scratch,
            g_normalized,
            barrier_counter,
            barrier_generation,
            block_max_vals,
            block_max_idxs,
            lm_sync_counter,
            out_token,
            pf_hidden,
            pf_residual,
            pf_normalized,
            pf_proj_buf,
            pf_proj_buf2,
            pf_attn_buf,
            pf_mlp_buf,
            pf_dn_out_buf,
            pf_beta_buf,
            pf_alpha_buf,
            pf_final_normed,
            pf_hidden_bf16_out,
            pf_lm_bmv,
            pf_lm_bmi,
            max_prefill_seq,
        })
    }

    /// Zero the stateful buffers (KV, DN state, conv, barriers) so
    /// the next request sees a clean slate. Scratch buffers are
    /// overwritten by each call anyway; no need to touch them.
    pub fn reset(&mut self) -> Result<()> {
        self.fa_k_cache = self.fa_k_cache.zeros_like()?;
        self.fa_v_cache = self.fa_v_cache.zeros_like()?;
        self.dn_states = self.dn_states.zeros_like()?;
        self.conv_bufs = self.conv_bufs.zeros_like()?;
        self.barrier_counter = self.barrier_counter.zeros_like()?;
        self.barrier_generation = self.barrier_generation.zeros_like()?;
        self.lm_sync_counter = self.lm_sync_counter.zeros_like()?;
        Ok(())
    }
}

/// Snapshot of the stateful buffers at a specific decode position.
/// Owned — not a reference — so callers can safely drop the
/// [`MegakernelBuffers`] while holding a snapshot for replay.
///
/// Memory footprint equals the stateful subset of `MegakernelBuffers`:
/// ~18 MB F32 for `dn_states` + 1.7 MB F32 for `conv_bufs`. The FA
/// KV cache is NOT snapshotted — rolling back the position counter
/// is enough because the kernel will overwrite ring slots on the
/// next write, and the tree/chain mask only reads `[0..position)`
/// entries. (This is the same shortcut the reference DFlash chain-
/// stepper takes for the FA KV — see commit log for qwen35_target.)
///
/// DN state + conv sliding window MUST be snapshotted because their
/// recurrences (exp-decay gating + conv window) are non-invertible.
pub struct MegakernelStateSnapshot {
    pub position: i32,
    pub dn_states: Tensor,
    pub conv_bufs: Tensor,
}

impl MegakernelBuffers {
    /// Capture the current stateful buffers into a snapshot. The
    /// snapshot is a device-to-device copy; caller can `restore`
    /// it later to roll back drafter state on a verify reject.
    pub fn snapshot_state(&self, position: i32) -> Result<MegakernelStateSnapshot> {
        Ok(MegakernelStateSnapshot {
            position,
            dn_states: self.dn_states.copy()?,
            conv_bufs: self.conv_bufs.copy()?,
        })
    }

    /// Restore DN state + conv sliding window from a snapshot. The
    /// FA KV cache is deliberately NOT restored — the caller's
    /// position counter tells the kernel where to write next, and
    /// stale ring entries past that position are overwritten on
    /// next step. Caller must also reset barrier_counter /
    /// barrier_generation / lm_sync_counter; those are persistent-
    /// kernel sync state, not decode state.
    pub fn restore_state(&mut self, snap: &MegakernelStateSnapshot) -> Result<()> {
        self.dn_states = snap.dn_states.copy()?;
        self.conv_bufs = snap.conv_bufs.copy()?;
        Ok(())
    }
}
