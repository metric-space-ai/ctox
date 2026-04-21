//! End-to-end driver for the Qwen3.5-0.8B megakernel drafter.
//!
//! Owns a [`MegakernelBuffers`] + [`MegakernelWeights`] pair and
//! exposes two hot-path methods:
//!   * [`MegakernelDrafter::prefill`] — feed a full prompt, get the
//!     first generated token back. Must be called before any step().
//!   * [`MegakernelDrafter::step`] — one fused-kernel decode for the
//!     next token. Reads state from the shared buffers, mutates in
//!     place, returns the sampled argmax.
//!
//! Thread-safety: single-sequence only. The kernel uses persistent
//! per-device state (barrier counters, KV cache, DN state) that is
//! specific to this drafter instance; callers must not share it
//! across concurrent requests. For multiple inflight sequences,
//! allocate multiple drafters.

#![cfg(feature = "cuda")]

use candle_core::{Device, Result, Tensor};
use std::ffi::{c_int, c_uint, c_void};

use super::buffers::MegakernelBuffers;
use super::constants::*;
use super::weights::MegakernelWeights;
use crate::cuda::dflash_megakernel::{launch_decode, launch_prefill_bf16, CudaStreamPtr};

pub struct MegakernelDrafter {
    pub weights: MegakernelWeights,
    pub buffers: MegakernelBuffers,
    /// Monotonic decode position — advanced by 1 per step() after
    /// prefill has set it to `prompt_len`. Matches the `position`
    /// arg the kernel uses to index RoPE + the KV ring.
    position: i32,
}

impl MegakernelDrafter {
    /// Assemble a new drafter from loaded weights. Allocates
    /// on-device scratch + state buffers sized for up to
    /// `max_prefill_seq` prompt tokens. Call [`Self::reset`] before
    /// each new request to wipe KV / DN state.
    pub fn new(weights: MegakernelWeights, max_prefill_seq: usize) -> Result<Self> {
        let device = weights.device.clone();
        let buffers = MegakernelBuffers::new(&device, max_prefill_seq)?;
        let mut drafter = Self {
            weights,
            buffers,
            position: 0,
        };
        // Pack the weight pointer table once up-front.
        drafter.weights.pack()?;
        Ok(drafter)
    }

    pub fn device(&self) -> &Device {
        &self.weights.device
    }

    /// Current decode position. After prefill with N prompt tokens
    /// this is N; after each step() it increments by 1.
    pub fn position(&self) -> i32 {
        self.position
    }

    /// Wipe all stateful buffers (KV cache, DN states, conv windows,
    /// barrier counters, LM sync counter) and reset the position to
    /// 0. Must be called between independent requests.
    pub fn reset(&mut self) -> Result<()> {
        self.buffers.reset()?;
        self.position = 0;
        Ok(())
    }

    /// Run prefill over a prompt of `token_ids.len()` tokens.
    /// Populates the KV cache + DN state, writes the first generated
    /// token's argmax into `buffers.out_token`, and advances the
    /// position counter by `prompt_len`.
    ///
    /// Returns the first generated token id (`out_token[0]`).
    pub fn prefill(&mut self, token_ids: &[i32]) -> Result<i32> {
        let seq_len = token_ids.len();
        if seq_len == 0 {
            candle_core::bail!("MegakernelDrafter::prefill: empty token_ids");
        }
        if seq_len > self.buffers.max_prefill_seq {
            candle_core::bail!(
                "MegakernelDrafter::prefill: seq_len {seq_len} exceeds \
                 buffers.max_prefill_seq={}",
                self.buffers.max_prefill_seq
            );
        }
        if seq_len > MAX_SEQ_LEN {
            candle_core::bail!(
                "MegakernelDrafter::prefill: seq_len {seq_len} exceeds kernel cap {MAX_SEQ_LEN}"
            );
        }

        // Upload the prompt tokens to device as i32.
        let device = self.device().clone();
        let token_tensor = Tensor::from_vec(token_ids.to_vec(), (seq_len,), &device)?;

        // Pull raw device pointers.
        let stream = cuda_stream(&device)?;
        let packed = self
            .weights
            .packed_ptr()
            .ok_or_else(|| candle_core::Error::msg("MegakernelDrafter: weights not packed"))?;

        // NB: `token_tensor` was built as I64 via `Tensor::from_vec`
        // taking `Vec<i32>` which candle widens to I64 by default.
        // The kernel reads 32-bit ints; on little-endian x86_64 the
        // low 32 bits of each I64 element = the original i32 value.
        let token_addr = addr_i64(&token_tensor)?;
        let out_addr = addr_i64(&self.buffers.out_token)?;
        let fa_k = addr_bf16(&self.buffers.fa_k_cache)?;
        let fa_v = addr_bf16(&self.buffers.fa_v_cache)?;
        let dn_states = addr_f32(&self.buffers.dn_states)?;
        let conv_bufs = addr_f32(&self.buffers.conv_bufs)?;
        let pf_hidden = addr_bf16(&self.buffers.pf_hidden)?;
        let pf_residual = addr_bf16(&self.buffers.pf_residual)?;
        let pf_normalized = addr_bf16(&self.buffers.pf_normalized)?;
        let pf_proj = addr_bf16(&self.buffers.pf_proj_buf)?;
        let pf_proj2 = addr_bf16(&self.buffers.pf_proj_buf2)?;
        let pf_attn = addr_bf16(&self.buffers.pf_attn_buf)?;
        let pf_mlp = addr_bf16(&self.buffers.pf_mlp_buf)?;
        let pf_dn_out = addr_bf16(&self.buffers.pf_dn_out_buf)?;
        let pf_beta = addr_f32(&self.buffers.pf_beta_buf)?;
        let pf_alpha = addr_f32(&self.buffers.pf_alpha_buf)?;
        let pf_final_normed = addr_bf16(&self.buffers.pf_final_normed)?;
        let pf_hidden_out = addr_bf16(&self.buffers.pf_hidden_bf16_out)?;
        let pf_lm_bmv = addr_f32(&self.buffers.pf_lm_bmv)?;
        let pf_lm_bmi = addr_i64(&self.buffers.pf_lm_bmi)?;

        unsafe {
            launch_prefill_bf16(
                token_addr as *const c_int,
                seq_len as c_int,
                out_addr as *mut c_int,
                self.weights.embed_ptr(),
                packed as *const _,
                self.weights.final_norm_ptr(),
                self.weights.lm_head_ptr(),
                fa_k as *mut c_void,
                fa_v as *mut c_void,
                dn_states as *mut f32,
                conv_bufs as *mut f32,
                pf_hidden as *mut c_void,
                pf_residual as *mut c_void,
                pf_normalized as *mut c_void,
                pf_proj as *mut c_void,
                pf_proj2 as *mut c_void,
                pf_attn as *mut c_void,
                pf_mlp as *mut c_void,
                pf_dn_out as *mut c_void,
                pf_beta as *mut f32,
                pf_alpha as *mut f32,
                pf_final_normed as *mut c_void,
                pf_hidden_out as *mut c_void,
                pf_lm_bmv as *mut f32,
                pf_lm_bmi as *mut c_int,
                stream,
            );
        }

        self.position = seq_len as i32;
        read_out_token(&self.buffers.out_token)
    }

    /// Run one decode step. Reads `input_token` + current state,
    /// writes the next token's argmax into `buffers.out_token`,
    /// advances position by 1, returns the new token id.
    pub fn step(&mut self, input_token: i32) -> Result<i32> {
        if self.position as usize >= MAX_SEQ_LEN {
            candle_core::bail!(
                "MegakernelDrafter::step: position {} exceeds kernel cap {MAX_SEQ_LEN}",
                self.position
            );
        }
        let device = self.device().clone();
        let stream = cuda_stream(&device)?;
        let packed = self
            .weights
            .packed_ptr()
            .ok_or_else(|| candle_core::Error::msg("MegakernelDrafter: weights not packed"))?;

        let out_addr = addr_i64(&self.buffers.out_token)?;
        let fa_k = addr_bf16(&self.buffers.fa_k_cache)?;
        let fa_v = addr_bf16(&self.buffers.fa_v_cache)?;
        let dn_states = addr_f32(&self.buffers.dn_states)?;
        let conv_bufs = addr_f32(&self.buffers.conv_bufs)?;
        let hidden = addr_bf16(&self.buffers.hidden_buffer)?;
        let activations = addr_f32(&self.buffers.g_activations)?;
        let residual = addr_bf16(&self.buffers.g_residual)?;
        let qkv = addr_f32(&self.buffers.g_qkv_scratch)?;
        let kv_sc = addr_f32(&self.buffers.g_kv_scratch)?;
        let attn_out = addr_f32(&self.buffers.g_attn_out)?;
        let mlp = addr_f32(&self.buffers.g_mlp_inter)?;
        let z_sc = addr_f32(&self.buffers.g_z_scratch)?;
        let beta_sc = addr_f32(&self.buffers.g_beta_scratch)?;
        let alpha_sc = addr_f32(&self.buffers.g_alpha_scratch)?;
        let normalized = addr_f32(&self.buffers.g_normalized)?;
        let bar_c = addr_u32(&self.buffers.barrier_counter)?;
        let bar_g = addr_u32(&self.buffers.barrier_generation)?;
        let bmv = addr_f32(&self.buffers.block_max_vals)?;
        let bmi = addr_i64(&self.buffers.block_max_idxs)?;
        let lm_sync = addr_u32(&self.buffers.lm_sync_counter)?;

        unsafe {
            launch_decode(
                input_token as c_int,
                out_addr as *mut c_int,
                self.weights.embed_ptr(),
                packed as *const _,
                self.weights.final_norm_ptr(),
                self.weights.lm_head_ptr(),
                fa_k as *mut c_void,
                fa_v as *mut c_void,
                dn_states as *mut c_void,
                conv_bufs as *mut c_void,
                hidden as *mut c_void,
                activations as *mut c_void,
                residual as *mut c_void,
                qkv as *mut c_void,
                kv_sc as *mut c_void,
                attn_out as *mut c_void,
                mlp as *mut c_void,
                z_sc as *mut c_void,
                beta_sc as *mut c_void,
                alpha_sc as *mut c_void,
                normalized as *mut c_void,
                bar_c as *mut c_uint,
                bar_g as *mut c_uint,
                bmv as *mut f32,
                bmi as *mut c_int,
                lm_sync as *mut c_uint,
                self.position,
                MAX_SEQ_LEN as c_int,
                stream,
            );
        }

        self.position += 1;
        read_out_token(&self.buffers.out_token)
    }
}

// ── Raw device-pointer helpers ──
//
// These use candle's cudarc bindings to extract the base pointer of a
// tensor. They require the tensor to be contiguous (no view strides)
// and of the expected dtype. All returned pointers share the tensor's
// lifetime; the caller must keep the tensor alive for at least as
// long as the kernel launch consuming the pointer.

fn cuda_stream(device: &Device) -> Result<CudaStreamPtr> {
    let dev = device.as_cuda_device()?;
    let stream = dev.cuda_stream();
    // `cu_stream()` returns a `cudaStream_t`-compatible handle.
    Ok(stream.cu_stream() as CudaStreamPtr)
}

// All device-pointer helpers return a bare `u64` — the raw CUDA
// device address of the tensor's first live element. The caller
// casts to `*mut T` at the call site. This structure keeps the
// lifetime narrow: the `MutexGuard` protecting `Storage` is held
// only inside the helper, released on return, and the returned
// `u64` has no borrow attached. The underlying device allocation
// remains valid for as long as the source `Tensor` is alive, so
// the pointer is stable for the duration of a kernel launch that
// references the tensor.

fn addr_bf16(t: &Tensor) -> Result<u64> {
    use candle_core::cuda_backend::cudarc::driver::DevicePtr;
    if t.dtype() != candle_core::DType::BF16 {
        candle_core::bail!("expected BF16 tensor, got {:?}", t.dtype());
    }
    let (storage, layout) = t.storage_and_layout();
    match &*storage {
        candle_core::Storage::Cuda(c) => {
            let s = c.as_cuda_slice::<half::bf16>()?;
            let (addr, _g) = s.slice(layout.start_offset()..).device_ptr(s.stream());
            Ok(addr)
        }
        _ => candle_core::bail!("non-CUDA tensor"),
    }
}

fn addr_f32(t: &Tensor) -> Result<u64> {
    use candle_core::cuda_backend::cudarc::driver::DevicePtr;
    if t.dtype() != candle_core::DType::F32 {
        candle_core::bail!("expected F32 tensor, got {:?}", t.dtype());
    }
    let (storage, layout) = t.storage_and_layout();
    match &*storage {
        candle_core::Storage::Cuda(c) => {
            let s = c.as_cuda_slice::<f32>()?;
            let (addr, _g) = s.slice(layout.start_offset()..).device_ptr(s.stream());
            Ok(addr)
        }
        _ => candle_core::bail!("non-CUDA tensor"),
    }
}

fn addr_i64(t: &Tensor) -> Result<u64> {
    // I64-allocated scratch tensor reinterpreted as i32 on the kernel
    // side (little-endian only; the kernel only reads element 0
    // anyway for `out_token` / `lm_bmi`).
    use candle_core::cuda_backend::cudarc::driver::DevicePtr;
    if t.dtype() != candle_core::DType::I64 {
        candle_core::bail!("expected I64 tensor, got {:?}", t.dtype());
    }
    let (storage, layout) = t.storage_and_layout();
    match &*storage {
        candle_core::Storage::Cuda(c) => {
            let s = c.as_cuda_slice::<i64>()?;
            let (addr, _g) = s.slice(layout.start_offset()..).device_ptr(s.stream());
            Ok(addr)
        }
        _ => candle_core::bail!("non-CUDA tensor"),
    }
}

fn addr_u32(t: &Tensor) -> Result<u64> {
    use candle_core::cuda_backend::cudarc::driver::DevicePtr;
    if t.dtype() != candle_core::DType::U32 {
        candle_core::bail!("expected U32 tensor, got {:?}", t.dtype());
    }
    let (storage, layout) = t.storage_and_layout();
    match &*storage {
        candle_core::Storage::Cuda(c) => {
            let s = c.as_cuda_slice::<u32>()?;
            let (addr, _g) = s.slice(layout.start_offset()..).device_ptr(s.stream());
            Ok(addr)
        }
        _ => candle_core::bail!("non-CUDA tensor"),
    }
}

fn read_out_token(out: &Tensor) -> Result<i32> {
    // `out_token` was allocated as I64 (single element). Copy to host
    // and take the low 32 bits — consistent with the kernel writing a
    // 32-bit `int` into the first element.
    let v: Vec<i64> = out.to_dtype(candle_core::DType::I64)?.to_vec1()?;
    Ok(v.first().copied().unwrap_or(0) as i32)
}
