//! Rust FFI bindings for the DFlash drafter megakernel.
//!
//! The CUDA source lives in `dflash_megakernel_decode.cu` (verbatim port
//! of the lucebox-hub reference `megakernel/kernel.cu`). It implements a
//! fused single-kernel decode of Qwen3.5-0.8B (drafter): all 24 layers,
//! hybrid DeltaNet + Full Attention, BF16 weights + activations, f32
//! DeltaNet state, one launch per generated token.
//!
//! Entry points:
//! * [`launch_decode`] — autoregressive decode of exactly one token.
//!
//! Scope of this file is *bindings only*. The driver-side concerns (pack
//! `LayerWeights`, allocate scratch/KV/state buffers on the target
//! device, map candle tensors to raw pointers without aliasing) are
//! handled by the caller — see the DFlash draft model wiring.

#![allow(non_camel_case_types)]

use std::os::raw::{c_int, c_uint, c_void};

/// Layout-compatible mirror of the C `LayerWeights` struct expected by
/// `launch_decode`. A flat `ptrs[14]` array is used at the ABI boundary;
/// the kernel reinterprets it as a `union { DeltaNetWeights dn;
/// FullAttnWeights fa; }` internally via `layer_type`:
///   * `layer_type = 0` → DeltaNet (14 pointers used).
///   * `layer_type = 1` → FullAttention (first 11 pointers used, 12..=13 ignored).
///
/// DeltaNet pointer order (matches `struct DeltaNetWeights` in the .cu):
///   `[input_layernorm, qkv_proj, z_proj, beta_proj, alpha_proj,
///     conv1d, a_log, dt_bias, norm, out_proj,
///     post_attn_layernorm, gate_proj, up_proj, down_proj]`
///
/// FullAttention pointer order (matches `struct FullAttnWeights`):
///   `[input_layernorm, q_proj, k_proj, v_proj, q_norm, k_norm,
///     o_proj, post_attn_layernorm, gate_proj, up_proj, down_proj,
///     _unused, _unused, _unused]`
///
/// All pointers target BF16 tensors on the CUDA device in row-major
/// layout with the shapes documented in `dflash_megakernel_decode.cu`
/// (FullAttnWeights / DeltaNetWeights struct fields).
#[repr(C)]
pub struct LayerWeights {
    pub layer_type: c_int,
    pub _pad: [c_int; 3],
    pub ptrs: [*const c_void; 14],
}

/// Layer-type constant for a DeltaNet layer.
pub const LAYER_TYPE_DELTANET: c_int = 0;
/// Layer-type constant for a FullAttention layer.
pub const LAYER_TYPE_FULL_ATTENTION: c_int = 1;

/// Static layer pattern for Qwen3.5-0.8B as encoded in
/// `__constant__ LAYER_TYPE` in `dflash_megakernel_decode.cu`:
/// `[0,0,0,1, 0,0,0,1, 0,0,0,1, 0,0,0,1, 0,0,0,1, 0,0,0,1]` — every
/// 4th layer is Full Attention, rest are DeltaNet. Exposed here so the
/// Rust driver can shape-check the packed weights against the kernel's
/// hard-coded layer pattern.
pub const LAYER_PATTERN: [c_int; 24] = [
    0, 0, 0, 1, //
    0, 0, 0, 1, //
    0, 0, 0, 1, //
    0, 0, 0, 1, //
    0, 0, 0, 1, //
    0, 0, 0, 1, //
];

// NOTE: `cudaStream_t` is an opaque handle (`struct CUstream_st *`) in
// the CUDA runtime API. We re-export it as `*mut c_void` to keep this
// file free of a bindgen-generated CUDA header dep; callers pass the
// raw stream pointer obtained from cudaforge / the engine's CUDA
// integration.
pub type CudaStreamPtr = *mut c_void;

/// Layout-compatible mirror of the C `PFLayerWeights` struct expected
/// by `launch_prefill_bf16`. Same packing as `LayerWeights` (the
/// kernel's decode-side struct), since both point at the same
/// weight tensors — 14 BF16 device pointers per layer, interpreted
/// as DeltaNet or FullAttention based on `layer_type`. The prefill
/// kernel doesn't carry an explicit `layer_type` field: it reads
/// the layer-type flag from the `__constant__ LAYER_TYPE[NUM_LAYERS]`
/// baked into the `.cu` TU, same as the decode kernel. To keep the
/// Rust side symmetric and easy to reason about, we reuse
/// `LayerWeights` for both entry points — the `_pad` slot is tolerated
/// by the prefill kernel.
pub type PrefillLayerWeights = LayerWeights;

extern "C" {
    /// Single-token autoregressive decode of the DFlash drafter
    /// (Qwen3.5-0.8B). Reads the current KV / DeltaNet state, runs all
    /// 24 hybrid layers fused, samples greedy argmax on the LM head,
    /// and writes the next token id to `output_token_id`.
    ///
    /// All pointer args alias device memory allocated by the caller
    /// on the same CUDA context/device as `stream`. Pointer contracts:
    ///
    /// * `embed_weight`   — BF16 `[vocab=248320, hidden=1024]`.
    /// * `layer_weights`  — packed `[NUM_LAYERS=24] LayerWeights`.
    /// * `final_norm_weight` — BF16 `[1024]`.
    /// * `lm_head_weight`    — BF16 `[vocab=248320, hidden=1024]`.
    /// * `fa_k_cache`/`fa_v_cache` — BF16 ring buffers sized for
    ///                         `max_seq_len` per Full-Attn layer × 6.
    /// * `dn_states`      — f32 `[6, num_heads=16, key=128, val=128]`.
    /// * `conv_bufs`      — BF16 depthwise-conv sliding window state.
    /// * Remaining `g_*` tensors are per-block scratch, allocated once
    ///   and reused across decode calls (sizes come from the .cu file).
    /// * `barrier_counter` / `barrier_generation` — initialised to 0
    ///   on first call, kernel uses them for persistent grid sync.
    /// * `block_max_vals`, `block_max_idxs` — scratch for the LM-head
    ///   reduction producing the argmax into `output_token_id`.
    /// * `lm_sync_counter` — LM-head completion barrier.
    /// * `position` — 0-based index of the token being generated.
    /// * `max_seq_len` — ring capacity of the KV cache.
    /// * `stream` — CUDA stream on which the kernel is enqueued.
    pub fn launch_decode(
        input_token_id: c_int,
        output_token_id: *mut c_int,
        embed_weight: *const c_void,
        layer_weights: *const LayerWeights,
        final_norm_weight: *const c_void,
        lm_head_weight: *const c_void,
        fa_k_cache: *mut c_void,
        fa_v_cache: *mut c_void,
        dn_states: *mut c_void,
        conv_bufs: *mut c_void,
        hidden_buffer: *mut c_void,
        g_activations: *mut c_void,
        g_residual: *mut c_void,
        g_qkv_scratch: *mut c_void,
        g_kv_scratch: *mut c_void,
        g_attn_out: *mut c_void,
        g_mlp_inter: *mut c_void,
        g_z_scratch: *mut c_void,
        g_beta_scratch: *mut c_void,
        g_alpha_scratch: *mut c_void,
        g_normalized: *mut c_void,
        barrier_counter: *mut c_uint,
        barrier_generation: *mut c_uint,
        block_max_vals: *mut f32,
        block_max_idxs: *mut c_int,
        lm_sync_counter: *mut c_uint,
        position: c_int,
        max_seq_len: c_int,
        stream: CudaStreamPtr,
    );

    /// Prefill path for the DFlash drafter. Runs all `seq_len` prompt
    /// tokens through the 24 hybrid layers using cuBLAS BF16 GEMMs +
    /// a standalone DeltaNet recurrence kernel, populates the per-
    /// layer KV caches / DN states / conv sliding windows the decode
    /// kernel depends on, and writes the argmax of the final-position
    /// LM head into `output_token` — the first generated token.
    ///
    /// Caller owns all pointer buffers; contracts match
    /// `launch_decode` for the shared tensors (`embed_weight`,
    /// `layer_weights`, `final_norm_w`, `lm_head_w`, `fa_k_cache`,
    /// `fa_v_cache`, `dn_states`, `conv_bufs`). The scratch tensors
    /// (`hidden`, `residual`, `normalized`, two `proj_buf`s, `attn_buf`,
    /// `mlp_buf`, `dn_out_buf`, `beta_buf`, `alpha_buf`, `final_normed`,
    /// `hidden_bf16_out`, `lm_bmv`, `lm_bmi`) are sized per the
    /// constants baked into `dflash_megakernel_prefill.cu` and are
    /// prefill-specific (separate buffers from the decode scratch).
    pub fn launch_prefill_bf16(
        token_ids: *const c_int,
        seq_len: c_int,
        output_token: *mut c_int,
        embed_weight: *const c_void,
        layers: *const PrefillLayerWeights,
        final_norm_w: *const c_void,
        lm_head_w: *const c_void,
        fa_k_cache: *mut c_void,
        fa_v_cache: *mut c_void,
        dn_states: *mut f32,
        conv_bufs: *mut f32,
        hidden: *mut c_void,
        residual: *mut c_void,
        normalized: *mut c_void,
        proj_buf: *mut c_void,
        proj_buf2: *mut c_void,
        attn_buf: *mut c_void,
        mlp_buf: *mut c_void,
        dn_out_buf: *mut c_void,
        beta_buf: *mut f32,
        alpha_buf: *mut f32,
        final_normed: *mut c_void,
        hidden_bf16_out: *mut c_void,
        lm_bmv: *mut f32,
        lm_bmi: *mut c_int,
        stream: CudaStreamPtr,
    );
}
