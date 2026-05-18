//! Byte-exact port of `lucebox/dflash/src/internal.h`.
//!
//! ref: `dflash/src/internal.h:1-289`
//!
//! Shared type definitions — every struct here maps 1:1 to the C++
//! source. Lifecycle ops (`load_*`, `free_*`, `create_target_cache`,
//! `build_qwen35_graph`) live in their own files to mirror the
//! reference's translation-unit split.
//!
//! # Ownership semantics
//!
//! The reference manages `ggml_context *` / `ggml_backend_t` /
//! `ggml_backend_buffer_t` by hand with `free_*` functions. The Rust
//! port keeps the same explicit ownership discipline: each struct
//! holds raw pointers and the matching `Drop` impl in the matching
//! module calls the correct ggml free function.

use crate::ffi::{ggml_backend_buffer_t, ggml_backend_t, ggml_context, ggml_tensor, ggml_type};
use libc::c_int;

// ─── TargetLayer ─────────────────────────────────────────────────
//
// ref: `internal.h:21-64`
//
// Qwen3.5 uses two kinds of blocks interleaved:
//   - FULL ATTENTION block  (every `full_attention_interval`-th layer, =4):
//       attn_norm, wq, wk, wv, wo, q_norm, k_norm + FFN tensors
//       (M-RoPE applied with rope_sections [11,11,10,0] — rope dims=64 of head_dim=256)
//   - GATED DELTANET block (all other layers, ~3 out of every 4):
//       attn_norm, wqkv (fused), wqkv_gate (the "z" projection),
//       delta-net per-head parameters (beta, gate, conv), plus FFN tensors.
//
// We keep ONE struct with all possible fields and leave unused ones null.
// Actual tensor names in unsloth's GGUF are read via gguf_find_tensor() in
// the loader.

#[derive(Default, Debug)]
pub struct TargetLayer {
    // Shared
    pub attn_norm: *mut ggml_tensor,      // [hidden]
    pub attn_post_norm: *mut ggml_tensor, // [hidden]  (post-block norm before FFN)
    pub ffn_norm: *mut ggml_tensor,       // [hidden]
    pub w_gate: *mut ggml_tensor,         // [hidden, intermediate]
    pub w_up: *mut ggml_tensor,           // [hidden, intermediate]
    pub w_down: *mut ggml_tensor,         // [intermediate, hidden]

    // Full-attention block (non-null for layers where (il+1) % 4 == 0)
    pub wq: *mut ggml_tensor,     // [hidden, q_dim]
    pub wk: *mut ggml_tensor,     // [hidden, kv_dim]
    pub wv: *mut ggml_tensor,     // [hidden, kv_dim]
    pub wo: *mut ggml_tensor,     // [q_dim, hidden]
    pub q_norm: *mut ggml_tensor, // [head_dim]
    pub k_norm: *mut ggml_tensor, // [head_dim]

    // Gated DeltaNet block (non-null for the other ~3/4 of layers)
    pub wqkv: *mut ggml_tensor,        // fused Q/K/V projection
    pub wqkv_gate: *mut ggml_tensor,   // the "z" projection
    pub ssm_conv1d: *mut ggml_tensor,  // [kernel, dim]  depthwise causal conv
    pub ssm_beta: *mut ggml_tensor,    // per-token beta input projection
    pub ssm_alpha: *mut ggml_tensor,   // per-token alpha input projection
    pub ssm_a: *mut ggml_tensor,       // [dt_rank] per-head -A parameter
    pub ssm_dt_bias: *mut ggml_tensor, // [dt_rank] per-head alpha bias
    pub ssm_norm: *mut ggml_tensor,    // [head_v_dim]
    pub ssm_out: *mut ggml_tensor,     // output projection after delta-net
}

// ─── CpuEmbedder ─────────────────────────────────────────────────
//
// ref: `internal.h:66-85`
//
// Keeps a mmap of the GGUF alive and knows how to dequantize individual
// rows of the quantized tok_embd tensor on demand. Matches llama.cpp's
// behavior of running embedding get_rows on CPU (because CUDA's
// get_rows doesn't support k-quants), so we never need to upload the
// 682 MiB token embedding to VRAM.

pub struct CpuEmbedder {
    pub mmap_addr: *mut libc::c_void,
    pub mmap_len: libc::size_t,
    pub mmap_fd: c_int,
    pub tok_embd_bytes: *const u8, // into the mmap region
    pub tok_embd_type: ggml_type,
    pub n_embd: i64,
    pub n_vocab: i64,
    pub row_bytes: libc::size_t, // bytes per row in the quant format
}

impl Default for CpuEmbedder {
    fn default() -> Self {
        Self {
            mmap_addr: std::ptr::null_mut(),
            mmap_len: 0,
            mmap_fd: -1,
            tok_embd_bytes: std::ptr::null(),
            tok_embd_type: ggml_type::GGML_TYPE_COUNT,
            n_embd: 0,
            n_vocab: 0,
            row_bytes: 0,
        }
    }
}

// Drop impl lives in `loader` (where the mmap is created),
// to keep this file structure-only.

// ─── TargetWeights ───────────────────────────────────────────────
//
// ref: `internal.h:87-115`

pub struct TargetWeights {
    pub ctx: *mut ggml_context,
    pub backend: ggml_backend_t,
    pub buf: ggml_backend_buffer_t,

    // CPU-side embedding table (zero GPU cost).
    pub embedder: CpuEmbedder,

    pub tok_embd: *mut ggml_tensor, // [hidden, vocab] (metadata only; data NOT on GPU)
    pub layers: Vec<TargetLayer>,   // size = 64
    pub out_norm: *mut ggml_tensor, // [hidden]
    pub output: *mut ggml_tensor,   // [hidden, vocab]  (lm_head)

    // Metadata from GGUF (validated at load time)
    pub full_attention_interval: c_int,
    pub rope_sections: [c_int; 4],
    pub n_embd_head_k: c_int, // key_length
    pub n_embd_head_v: c_int, // value_length
    pub n_head: c_int,
    pub n_head_kv: c_int,
    pub n_layer: c_int,
    pub n_embd: c_int,
    pub n_ff: c_int,
    pub ssm_d_conv: c_int,
    pub ssm_d_inner: c_int,
    pub ssm_d_state: c_int,
    pub ssm_dt_rank: c_int,
    pub ssm_n_group: c_int,
}

impl Default for TargetWeights {
    fn default() -> Self {
        Self {
            ctx: std::ptr::null_mut(),
            backend: std::ptr::null_mut(),
            buf: std::ptr::null_mut(),
            embedder: CpuEmbedder::default(),
            tok_embd: std::ptr::null_mut(),
            layers: Vec::new(),
            out_norm: std::ptr::null_mut(),
            output: std::ptr::null_mut(),
            full_attention_interval: 4,
            rope_sections: [11, 11, 10, 0],
            n_embd_head_k: 256,
            n_embd_head_v: 256,
            n_head: 24,
            n_head_kv: 4,
            n_layer: 64,
            n_embd: 5120,
            n_ff: 17_408,
            ssm_d_conv: 4,
            ssm_d_inner: 6144,
            ssm_d_state: 128,
            ssm_dt_rank: 48,
            ssm_n_group: 16,
        }
    }
}

// ─── DraftLayer / DraftWeights ───────────────────────────────────
//
// ref: `internal.h:127-150`

#[derive(Default, Debug)]
pub struct DraftLayer {
    pub attn_norm: *mut ggml_tensor,
    pub ffn_norm: *mut ggml_tensor,
    pub wq: *mut ggml_tensor,
    pub wk: *mut ggml_tensor,
    pub wv: *mut ggml_tensor,
    pub wo: *mut ggml_tensor,
    pub q_norm: *mut ggml_tensor,
    pub k_norm: *mut ggml_tensor,
    pub w_gate: *mut ggml_tensor,
    pub w_up: *mut ggml_tensor,
    pub w_down: *mut ggml_tensor,
}

pub struct DraftWeights {
    pub ctx: *mut ggml_context,
    pub backend: ggml_backend_t,
    pub buf: ggml_backend_buffer_t,

    pub fc: *mut ggml_tensor,          // [5*hidden, hidden]
    pub hidden_norm: *mut ggml_tensor, // [hidden]
    pub layers: Vec<DraftLayer>,       // size = 5
    pub out_norm: *mut ggml_tensor,    // [hidden]
}

impl Default for DraftWeights {
    fn default() -> Self {
        Self {
            ctx: std::ptr::null_mut(),
            backend: std::ptr::null_mut(),
            buf: std::ptr::null_mut(),
            fc: std::ptr::null_mut(),
            hidden_norm: std::ptr::null_mut(),
            layers: Vec::new(),
            out_norm: std::ptr::null_mut(),
        }
    }
}

// ─── TargetCache ─────────────────────────────────────────────────
//
// ref: `internal.h:158-215`
//
// Pre-allocated, backend-resident state that persists across decode
// steps. Created once via `create_target_cache()` (see
// [`crate::graph`]) and threaded through every
// `build_qwen35_graph()` call.

pub struct TargetCache {
    pub ctx: *mut ggml_context,
    pub backend: ggml_backend_t,
    pub buf: ggml_backend_buffer_t,

    pub max_ctx: c_int, // max tokens in the KV cache
    pub cur_pos: c_int, // number of tokens already committed

    // Full-attention KV cache: one K and one V per full-attention layer.
    // Layout: [head_dim, max_ctx, n_head_kv] f16, contiguous per layer.
    pub attn_k: Vec<*mut ggml_tensor>, // size = n_full_attn_layers (16)
    pub attn_v: Vec<*mut ggml_tensor>,

    // Gated DeltaNet recurrent state: one per delta-net layer.
    // ssm_state: [S_v, S_v, H_v] f32    (head_v_dim^2 × num_v_heads)
    // conv_state: [(kernel-1), conv_channels] f32
    // where conv_channels = d_inner + 2 * n_group * d_state
    pub ssm_state: Vec<*mut ggml_tensor>, // size = n_delta_layers (48)
    pub conv_state: Vec<*mut ggml_tensor>,

    // Snapshot buffers for speculative decoding rollback. Sized
    // identically to ssm_state/conv_state above. Populated by
    // snapshot_ssm_state() and restored by restore_ssm_state().
    pub ssm_state_snap: Vec<*mut ggml_tensor>,
    pub conv_state_snap: Vec<*mut ggml_tensor>,

    // Per-step SSM + conv inputs captured during a verify forward when
    // QwenGraphInputs::capture_delta_intermediate is true.
    //
    //   ssm_intermediate: [S_v, S_v, H_v, max_q_len] f32, one per
    //     delta layer. Element t on axis 3 holds the DeltaNet recurrent
    //     state after processing verify token t. Spec decode commits
    //     t = commit_n - 1.
    //   conv_input_cache: [(kernel-1) + max_q_len, conv_channels] f32,
    //     one per delta layer. Holds the full concat(old_conv_state,
    //     qkv_new_tokens) that was fed to ggml_ssm_conv. Spec decode
    //     slices [commit_n..commit_n+kernel-2] along dim 0 for conv
    //     state rollback.
    pub ssm_intermediate: Vec<*mut ggml_tensor>, // size = n_delta (48)
    pub conv_input_cache: Vec<*mut ggml_tensor>,

    // Rolling target layer features captured during target forward
    // passes. Shape [5 * hidden, target_feat_cap] bf16.
    // target_feat_cap is typically << max_ctx (e.g. 4096) so the
    // buffer stays small at 128K context. The graph writes to slot
    // `(kv_start + i) % target_feat_cap` so positions beyond the cap
    // wrap and overwrite older entries. Readers (draft) only need
    // the last DRAFT_CTX_MAX positions, so wrap is invisible in
    // practice. Fed into the draft graph's fc projection after a
    // bf16→f32 cast (dflash27b_launch_bf16_to_f32).
    pub target_feat: *mut ggml_tensor,
    pub target_feat_cap: c_int,
}

impl Default for TargetCache {
    fn default() -> Self {
        Self {
            ctx: std::ptr::null_mut(),
            backend: std::ptr::null_mut(),
            buf: std::ptr::null_mut(),
            max_ctx: 0,
            cur_pos: 0,
            attn_k: Vec::new(),
            attn_v: Vec::new(),
            ssm_state: Vec::new(),
            conv_state: Vec::new(),
            ssm_state_snap: Vec::new(),
            conv_state_snap: Vec::new(),
            ssm_intermediate: Vec::new(),
            conv_input_cache: Vec::new(),
            target_feat: std::ptr::null_mut(),
            target_feat_cap: 0,
        }
    }
}

// ─── Graph I/O structs ───────────────────────────────────────────
//
// ref: `internal.h:251-279`

#[derive(Default, Debug, Copy, Clone)]
pub struct DeltaNetCapture {
    pub ssm_intermediate_states: *mut ggml_tensor,
    pub conv_input: *mut ggml_tensor,
}

pub struct QwenGraphInputs {
    pub inp_embed: *mut ggml_tensor, // [hidden, n_tokens, 1] f32 — pre-embedded by the caller
    pub positions: *mut ggml_tensor, // [4 * n_tokens] i32 (M-RoPE needs 4 per token)
    pub attn_mask: *mut ggml_tensor, // optional [kv_len, n_tokens_padded] f32 (causal); null for n_tokens==1
    pub n_tokens: c_int,
    pub kv_start: c_int,
    pub capture_layers: bool,
    pub capture_delta_intermediate: bool,
    pub parent_ids: *mut ggml_tensor, // DDTree extension; null for chain mode
}

impl Default for QwenGraphInputs {
    fn default() -> Self {
        Self {
            inp_embed: std::ptr::null_mut(),
            positions: std::ptr::null_mut(),
            attn_mask: std::ptr::null_mut(),
            n_tokens: 0,
            kv_start: 0,
            capture_layers: false,
            capture_delta_intermediate: false,
            parent_ids: std::ptr::null_mut(),
        }
    }
}

pub struct QwenGraphOutputs {
    pub logits: *mut ggml_tensor, // [vocab, n_tokens] f32
    /// One entry per delta-net layer (48 for qwen35-27b). Only
    /// populated when `QwenGraphInputs::capture_delta_intermediate`
    /// is true.
    pub delta_captures: Vec<DeltaNetCapture>,
}

impl Default for QwenGraphOutputs {
    fn default() -> Self {
        Self {
            logits: std::ptr::null_mut(),
            delta_captures: Vec::new(),
        }
    }
}
