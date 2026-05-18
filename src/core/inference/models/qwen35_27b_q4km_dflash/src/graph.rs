//! All ggml graph builders for the Qwen3.5-27B target and DFlash draft.
//!
//! Merged from three byte-exact ports of lucebox/dflash:
//!
//!   * `dflash/src/qwen35_target_graph.cpp`   — target KV cache, SSM
//!     snapshot/restore, and the qwen35 hybrid forward graph
//!   * `dflash/src/qwen3_dflash_graph.cpp`    — draft model forward
//!     graph (non-causal block diffusion over target features)
//!   * `dflash/src/delta_net_chunked.cpp`     — chunked GatedDeltaNet
//!     matmul variant used at `n_tokens > 1`
//!
//! The three files live together here because they share the same
//! backend context / tensor-type helpers and because the target graph
//! calls `build_delta_net_chunked` directly on every delta-net layer.
//! Keeping them in one translation unit mirrors the reference's
//! pattern of including `delta_net_chunked.h` from `qwen35_target_graph.cpp`
//! and eliminates a layer of `pub(crate)` plumbing.

use std::ffi::CString;
use std::os::raw::c_int;

use crate::ffi as sys;
use sys::{
    ggml_backend_t, ggml_cgraph, ggml_context, ggml_tensor, ggml_type,
    GGML_ROPE_TYPE_MROPE, GGML_ROPE_TYPE_NEOX, GGML_TRI_TYPE_LOWER,
    GGML_TRI_TYPE_LOWER_DIAG,
};

use crate::{
    set_last_error, DFLASH27B_DRAFT_BLOCK_SIZE, DFLASH27B_DRAFT_LAYERS,
    DFLASH27B_DRAFT_N_TARGET_LAYERS, DFLASH27B_RMS_EPS, DFLASH27B_ROPE_THETA,
    DFLASH27B_TARGET_HEAD_DIM, DFLASH27B_TARGET_HIDDEN, DFLASH27B_TARGET_N_HEADS,
    DFLASH27B_TARGET_N_KV_HEADS,
};
use crate::model::{
    DeltaNetCapture, DraftWeights, QwenGraphInputs, QwenGraphOutputs, TargetCache,
    TargetLayer, TargetWeights,
};

// ════════════════════════════════════════════════════════════════
//  TARGET GRAPH (qwen35_target_graph.cpp)
// ════════════════════════════════════════════════════════════════

// ─── q35 constants ───────────────────────────────────────────────
//
// ref: `qwen35_target_graph.cpp:46-67`
//
// Complement the DFLASH27B_* macros in constants.rs with
// qwen35-specific hparams that differ from the draft (which uses
// plain Qwen3 dims).

pub mod q35 {
    pub const N_HEAD: i32 = 24;
    pub const N_HEAD_KV: i32 = 4;
    pub const HEAD_DIM: i32 = 256; // key_length == value_length
    pub const Q_DIM: i32 = N_HEAD * HEAD_DIM; // 6144
    pub const KV_DIM: i32 = N_HEAD_KV * HEAD_DIM; // 1024
    pub const FFN_DIM: i32 = 17_408;

    pub const SSM_D_INNER: i32 = 6144;
    pub const SSM_D_STATE: i32 = 128;
    pub const SSM_DT_RANK: i32 = 48;
    pub const SSM_N_GROUP: i32 = 16;
    pub const SSM_CONV_KERN: i32 = 4;

    // Derived
    pub const HEAD_V_DIM: i32 = SSM_D_INNER / SSM_DT_RANK; // 128
    pub const HEAD_K_DIM: i32 = SSM_D_STATE; // 128
    pub const CONV_CHANNELS: i32 = SSM_D_INNER + 2 * SSM_N_GROUP * SSM_D_STATE; // 6144 + 4096 = 10240

    pub const EPS: f32 = 1e-6;
    pub const ROPE_THETA: f32 = 10_000_000.0;
}

// ─── create_target_cache ─────────────────────────────────────────
//
// ref: `qwen35_target_graph.cpp:71-225`

/// Allocate the backend-resident, pre-initialized state that
/// persists across forward calls.
///
/// `max_verify_tokens` controls the per-layer `ssm_intermediate` and
/// `conv_input_cache` sizes. Default is `DFLASH27B_DRAFT_BLOCK_SIZE`
/// (16) for chain verify. DDTree mode requires
/// `max(chain, 1 + tree_budget)` to hold the flat tree + root.
/// Pass 0 to use the default.
///
/// Returns `false` on failure and sets [`crate::last_error`].
///
/// ref: `qwen35_target_graph.cpp:71-225`
pub fn create_target_cache(
    w: &TargetWeights,
    max_ctx: c_int,
    max_verify_tokens_arg: c_int,
    backend: ggml_backend_t,
    out: &mut TargetCache,
) -> bool {
    out.backend = backend;
    out.max_ctx = max_ctx;
    out.cur_pos = 0;

    let mut max_verify_tokens = max_verify_tokens_arg;
    if max_verify_tokens <= 0 {
        max_verify_tokens = DFLASH27B_DRAFT_BLOCK_SIZE;
    }

    let n_full_attn = w.n_layer / w.full_attention_interval; // 16
    let n_delta = w.n_layer - n_full_attn; // 48

    out.attn_k = vec![std::ptr::null_mut(); n_full_attn as usize];
    out.attn_v = vec![std::ptr::null_mut(); n_full_attn as usize];
    out.ssm_state = vec![std::ptr::null_mut(); n_delta as usize];
    out.conv_state = vec![std::ptr::null_mut(); n_delta as usize];
    out.ssm_state_snap = vec![std::ptr::null_mut(); n_delta as usize];
    out.conv_state_snap = vec![std::ptr::null_mut(); n_delta as usize];
    out.ssm_intermediate = vec![std::ptr::null_mut(); n_delta as usize];
    out.conv_input_cache = vec![std::ptr::null_mut(); n_delta as usize];

    // Size the cache ggml context to hold all state tensors.
    //   per full-attn layer  : 2 (K, V)
    //   per delta-net layer  : 6 (ssm, conv, ssm_snap, conv_snap,
    //                             ssm_intermediate, conv_input_cache)
    //   top-level            : 1 (target_feat)
    //
    // ref: lines 96-104
    let n_tensors = 2 * n_full_attn + 6 * n_delta + 1;
    let ip = sys::ggml_init_params {
        mem_size: ((n_tensors + 32) as libc::size_t) * unsafe { sys::ggml_tensor_overhead() },
        mem_buffer: std::ptr::null_mut(),
        no_alloc: true,
    };
    out.ctx = unsafe { sys::ggml_init(ip) };
    if out.ctx.is_null() {
        set_last_error("cache ggml_init failed");
        return false;
    }

    // Create the KV cache tensors (one set per full-attn layer).
    //
    // Env overrides (checked in order; last wins):
    //   DFLASH27B_KV_F16=1  → f16 (regression baseline)
    //   DFLASH27B_KV_Q4=1   → Q4_0 (8× vs f16, required for 128K on
    //                              24 GB, ~3% AL hit)
    //
    // Default: Q8_0 — best quality/memory tradeoff at short context.
    //
    // ref: lines 108-122
    let mut kv_k_type = ggml_type::GGML_TYPE_Q8_0;
    let mut kv_v_type = ggml_type::GGML_TYPE_Q8_0;
    if let Ok(s) = std::env::var("DFLASH27B_KV_F16") {
        if s.parse::<i32>().unwrap_or(0) != 0 {
            kv_k_type = ggml_type::GGML_TYPE_F16;
            kv_v_type = ggml_type::GGML_TYPE_F16;
        }
    }
    if let Ok(s) = std::env::var("DFLASH27B_KV_Q4") {
        if s.parse::<i32>().unwrap_or(0) != 0 {
            kv_k_type = ggml_type::GGML_TYPE_Q4_0;
            kv_v_type = ggml_type::GGML_TYPE_Q4_0;
        }
    }

    // ref: lines 123-180
    let mut fa_idx: usize = 0;
    let mut dn_idx: usize = 0;
    for il in 0..w.n_layer {
        let is_attn = ((il + 1) % w.full_attention_interval) == 0;
        if is_attn {
            // [head_dim, max_ctx, n_head_kv]
            let k_t = unsafe {
                sys::ggml_new_tensor_3d(
                    out.ctx,
                    kv_k_type,
                    q35::HEAD_DIM as i64,
                    max_ctx as i64,
                    q35::N_HEAD_KV as i64,
                )
            };
            let v_t = unsafe {
                sys::ggml_new_tensor_3d(
                    out.ctx,
                    kv_v_type,
                    q35::HEAD_DIM as i64,
                    max_ctx as i64,
                    q35::N_HEAD_KV as i64,
                )
            };
            let nm_k = CString::new(format!("cache_k_{il}")).unwrap();
            let nm_v = CString::new(format!("cache_v_{il}")).unwrap();
            unsafe {
                sys::ggml_set_name(k_t, nm_k.as_ptr());
                sys::ggml_set_name(v_t, nm_v.as_ptr());
            }
            out.attn_k[fa_idx] = k_t;
            out.attn_v[fa_idx] = v_t;
            fa_idx += 1;
        } else {
            // ssm_state: [head_v_dim, head_v_dim, num_v_heads]
            let s_t = unsafe {
                sys::ggml_new_tensor_3d(
                    out.ctx,
                    ggml_type::GGML_TYPE_F32,
                    q35::HEAD_V_DIM as i64,
                    q35::HEAD_V_DIM as i64,
                    q35::SSM_DT_RANK as i64,
                )
            };
            let sn_t = unsafe {
                sys::ggml_new_tensor_3d(
                    out.ctx,
                    ggml_type::GGML_TYPE_F32,
                    q35::HEAD_V_DIM as i64,
                    q35::HEAD_V_DIM as i64,
                    q35::SSM_DT_RANK as i64,
                )
            };
            // conv_state: [kernel-1, conv_channels]
            let c_t = unsafe {
                sys::ggml_new_tensor_2d(
                    out.ctx,
                    ggml_type::GGML_TYPE_F32,
                    (q35::SSM_CONV_KERN - 1) as i64,
                    q35::CONV_CHANNELS as i64,
                )
            };
            let cn_t = unsafe {
                sys::ggml_new_tensor_2d(
                    out.ctx,
                    ggml_type::GGML_TYPE_F32,
                    (q35::SSM_CONV_KERN - 1) as i64,
                    q35::CONV_CHANNELS as i64,
                )
            };
            // ssm_intermediate: [S_v, S_v, H_v, max_verify_tokens] —
            // one SSM state per verify-block token. Sized to cover the
            // largest verify n_tokens we'll use (chain q_len=16 or
            // DDTree 1+budget). Stored in f16 to halve memory
            // (~3 MB → 1.5 MB per layer per slot), letting us fit
            // budgets up to ~50 on 24 GB. The gated_delta_net kernel
            // converts f32 ↔ f16 on write/read via
            // store/load_inter_state.
            let si_t = unsafe {
                sys::ggml_new_tensor_4d(
                    out.ctx,
                    ggml_type::GGML_TYPE_F16,
                    q35::HEAD_V_DIM as i64,
                    q35::HEAD_V_DIM as i64,
                    q35::SSM_DT_RANK as i64,
                    max_verify_tokens as i64,
                )
            };
            // conv_input_cache: [(K-1) + max_verify_tokens, conv_channels, 1]
            // — the full conv_input tensor captured during verify.
            let ci_t = unsafe {
                sys::ggml_new_tensor_3d(
                    out.ctx,
                    ggml_type::GGML_TYPE_F32,
                    ((q35::SSM_CONV_KERN - 1) + max_verify_tokens) as i64,
                    q35::CONV_CHANNELS as i64,
                    1,
                )
            };
            let nm_s = CString::new(format!("ssm_state_{il}")).unwrap();
            let nm_c = CString::new(format!("conv_state_{il}")).unwrap();
            let nm_sn = CString::new(format!("ssm_state_snap_{il}")).unwrap();
            let nm_cn = CString::new(format!("conv_state_snap_{il}")).unwrap();
            let nm_si = CString::new(format!("ssm_intermediate_{il}")).unwrap();
            let nm_ci = CString::new(format!("conv_input_cache_{il}")).unwrap();
            unsafe {
                sys::ggml_set_name(s_t, nm_s.as_ptr());
                sys::ggml_set_name(c_t, nm_c.as_ptr());
                sys::ggml_set_name(sn_t, nm_sn.as_ptr());
                sys::ggml_set_name(cn_t, nm_cn.as_ptr());
                sys::ggml_set_name(si_t, nm_si.as_ptr());
                sys::ggml_set_name(ci_t, nm_ci.as_ptr());
            }
            out.ssm_state[dn_idx] = s_t;
            out.conv_state[dn_idx] = c_t;
            out.ssm_state_snap[dn_idx] = sn_t;
            out.conv_state_snap[dn_idx] = cn_t;
            out.ssm_intermediate[dn_idx] = si_t;
            out.conv_input_cache[dn_idx] = ci_t;
            dn_idx += 1;
        }
    }

    // Rolling target_feat buffer: [5*hidden, target_feat_len] bf16.
    //
    // target_feat_len is capped (default 4096) instead of growing to
    // max_ctx, because the draft only ever reads the last
    // DRAFT_CTX_MAX=2048 positions (see test_dflash.cpp). Cap =
    // 2 * DRAFT_CTX_MAX to leave margin for prefill batching and
    // replay. Writes use `slot = kv_start % cap`; reads produce a
    // contiguous view of the last `draft_ctx` entries by handling the
    // wrap-around on the host side.
    //
    // At max_ctx=131072 this shrinks target_feat from 6.6 GB to
    // 0.2 GB — the difference that makes long context fit.
    //
    // ref: lines 182-199
    const TARGET_FEAT_CAP_DEFAULT: i32 = 4096;
    out.target_feat_cap = std::cmp::min(max_ctx, TARGET_FEAT_CAP_DEFAULT);
    {
        let fc_in = DFLASH27B_DRAFT_N_TARGET_LAYERS * w.n_embd; // 25600
        out.target_feat = unsafe {
            sys::ggml_new_tensor_2d(
                out.ctx,
                ggml_type::GGML_TYPE_BF16,
                fc_in as i64,
                out.target_feat_cap as i64,
            )
        };
        let nm = CString::new("target_feat").unwrap();
        unsafe { sys::ggml_set_name(out.target_feat, nm.as_ptr()) };
    }

    // ref: lines 201-207
    out.buf = unsafe { sys::ggml_backend_alloc_ctx_tensors(out.ctx, backend) };
    if out.buf.is_null() {
        set_last_error("ggml_backend_alloc_ctx_tensors failed for target cache");
        unsafe {
            sys::ggml_free(out.ctx);
        }
        out.ctx = std::ptr::null_mut();
        return false;
    }

    // Zero-initialize all state tensors. We need a scratch zero buffer
    // since `ggml_backend_tensor_memset` isn't always available. Use a
    // big-enough zero buffer and iterate.
    //
    // ref: lines 209-222
    let zeros: Vec<u8> = vec![0u8; 1 * 1024 * 1024];
    let mut t = unsafe { sys::ggml_get_first_tensor(out.ctx) };
    while !t.is_null() {
        let nb = unsafe { sys::ggml_nbytes(t) };
        let mut off: libc::size_t = 0;
        while off < nb {
            let chunk = std::cmp::min(nb - off, zeros.len() as libc::size_t);
            unsafe {
                sys::ggml_backend_tensor_set(
                    t,
                    zeros.as_ptr() as *const libc::c_void,
                    off,
                    chunk,
                );
            }
            off += chunk;
        }
        t = unsafe { sys::ggml_get_next_tensor(out.ctx, t) };
    }

    true
}

// ─── free_target_cache ───────────────────────────────────────────
//
// ref: `qwen35_target_graph.cpp:227-240`

/// Idempotent counterpart to [`create_target_cache`].
pub fn free_target_cache(c: &mut TargetCache) {
    unsafe {
        if !c.buf.is_null() {
            sys::ggml_backend_buffer_free(c.buf);
            c.buf = std::ptr::null_mut();
        }
        if !c.ctx.is_null() {
            sys::ggml_free(c.ctx);
            c.ctx = std::ptr::null_mut();
        }
    }
    c.attn_k.clear();
    c.attn_v.clear();
    c.ssm_state.clear();
    c.conv_state.clear();
    c.ssm_state_snap.clear();
    c.conv_state_snap.clear();
    c.ssm_intermediate.clear();
    c.conv_input_cache.clear();
    c.target_feat = std::ptr::null_mut();
    c.cur_pos = 0;
}

// ─── snapshot_ssm_state / restore_ssm_state ─────────────────────
//
// ref: `qwen35_target_graph.cpp:244-256`

/// Snapshot SSM+conv state for speculative rollback. Uses device-side
/// tensor copy (`ggml_backend_tensor_copy`). Called outside of any
/// compute graph.
///
/// ref: `qwen35_target_graph.cpp:244-249`
pub fn snapshot_ssm_state(c: &mut TargetCache) {
    for i in 0..c.ssm_state.len() {
        unsafe {
            sys::ggml_backend_tensor_copy(c.ssm_state[i], c.ssm_state_snap[i]);
            sys::ggml_backend_tensor_copy(c.conv_state[i], c.conv_state_snap[i]);
        }
    }
}

/// Counterpart to [`snapshot_ssm_state`] — copies the snapshot
/// tensors back into the live `ssm_state` / `conv_state` slots.
///
/// ref: `qwen35_target_graph.cpp:251-256`
pub fn restore_ssm_state(c: &mut TargetCache) {
    for i in 0..c.ssm_state.len() {
        unsafe {
            sys::ggml_backend_tensor_copy(c.ssm_state_snap[i], c.ssm_state[i]);
            sys::ggml_backend_tensor_copy(c.conv_state_snap[i], c.conv_state[i]);
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────
//
// ref: `qwen35_target_graph.cpp:258-273`

/// `rms_norm(x) * weight` — 2-op helper used all over the model.
///
/// ref: `qwen35_target_graph.cpp:260-264`
#[inline]
fn rms_norm_mul(
    ctx: *mut ggml_context,
    x: *mut ggml_tensor,
    weight: *mut ggml_tensor,
    eps: f32,
) -> *mut ggml_tensor {
    unsafe {
        let n = sys::ggml_rms_norm(ctx, x, eps);
        sys::ggml_mul(ctx, n, weight)
    }
}

/// SwiGLU FFN: `w_down * (silu(w_gate*cur) * (w_up*cur))`.
///
/// ref: `qwen35_target_graph.cpp:266-273`
fn build_swiglu_ffn(
    ctx: *mut ggml_context,
    cur: *mut ggml_tensor,
    l: &TargetLayer,
) -> *mut ggml_tensor {
    unsafe {
        let gate0 = sys::ggml_mul_mat(ctx, l.w_gate, cur); // [inter, n_tokens]
        let gate = sys::ggml_silu(ctx, gate0);
        let up = sys::ggml_mul_mat(ctx, l.w_up, cur);
        let gu = sys::ggml_mul(ctx, gate, up);
        sys::ggml_mul_mat(ctx, l.w_down, gu) // [hidden, n_tokens]
    }
}

// ─── build_full_attn_block ───────────────────────────────────────
//
// ref: `qwen35_target_graph.cpp:275-406`
//
// Matches llama.cpp's `build_layer_attn` for qwen35.
//
// `cache_k` / `cache_v` are the persistent KV buffers for this layer
// (shape `[head_dim, max_ctx, n_head_kv]` f16). We write the new K/V
// for `n_tokens` new positions starting at `kv_start`, then run
// causal attention over `[0..kv_start + n_tokens)`.

#[allow(clippy::too_many_arguments)]
fn build_full_attn_block(
    ctx: *mut ggml_context,
    gf: *mut ggml_cgraph,
    l: &TargetLayer,
    cur: *mut ggml_tensor,           // [hidden, n_tokens]
    positions: *mut ggml_tensor,     // [n_tokens] i32
    rope_sections: &[c_int; 4],
    cache_k: *mut ggml_tensor,       // [head_dim, max_ctx, n_head_kv]
    cache_v: *mut ggml_tensor,       // [head_dim, max_ctx, n_head_kv]
    attn_mask: *mut ggml_tensor,     // [kv_len, n_tokens] f32 or null
    kv_start: c_int,
    n_tokens: c_int,
) -> *mut ggml_tensor {
    unsafe {
        // ── Q projection (packed Q || gate), shape [2*q_dim, n_tokens]
        //     ref: lines 294-305
        let qg_mul = sys::ggml_mul_mat(ctx, l.wq, cur);
        // Reshape to [head_dim*2, n_head, n_tokens] so we can view the
        // Q and gate halves.
        let qg = sys::ggml_reshape_3d(
            ctx,
            qg_mul,
            (q35::HEAD_DIM * 2) as i64,
            q35::N_HEAD as i64,
            n_tokens as i64,
        );

        // Q half: view at offset 0, stride head_dim*2.
        // Layout: [head_dim, n_head, n_tokens].
        let elt = sys::ggml_element_size(qg);
        let mut q_view = sys::ggml_view_3d(
            ctx,
            qg,
            q35::HEAD_DIM as i64,
            q35::N_HEAD as i64,
            n_tokens as i64,
            elt * ((q35::HEAD_DIM * 2) as libc::size_t), // nb1: stride over n_head
            elt * ((q35::HEAD_DIM * 2 * q35::N_HEAD) as libc::size_t), // nb2: stride over n_tokens
            0,
        );
        q_view = rms_norm_mul(ctx, q_view, l.q_norm, q35::EPS);

        // Gate half: view at offset head_dim. ref: lines 308-314
        let gate_view = sys::ggml_view_3d(
            ctx,
            qg,
            q35::HEAD_DIM as i64,
            q35::N_HEAD as i64,
            n_tokens as i64,
            elt * ((q35::HEAD_DIM * 2) as libc::size_t),
            elt * ((q35::HEAD_DIM * 2 * q35::N_HEAD) as libc::size_t),
            elt * (q35::HEAD_DIM as libc::size_t),
        );
        // [q_dim, n_tokens]
        let gate = sys::ggml_cont_2d(
            ctx,
            gate_view,
            (q35::HEAD_DIM * q35::N_HEAD) as i64,
            n_tokens as i64,
        );

        // ── K and V projections. ref: lines 316-322
        let mut kcur = sys::ggml_mul_mat(ctx, l.wk, cur); // [kv_dim, n_tokens]
        let vcur = sys::ggml_mul_mat(ctx, l.wv, cur); // [kv_dim, n_tokens]

        kcur = sys::ggml_reshape_3d(
            ctx,
            kcur,
            q35::HEAD_DIM as i64,
            q35::N_HEAD_KV as i64,
            n_tokens as i64,
        );
        kcur = rms_norm_mul(ctx, kcur, l.k_norm, q35::EPS);
        let vcur = sys::ggml_reshape_3d(
            ctx,
            vcur,
            q35::HEAD_DIM as i64,
            q35::N_HEAD_KV as i64,
            n_tokens as i64,
        );

        // ── M-RoPE (multi-axis rotary). `n_rot` = the number of dims
        //     to rotate; for qwen35 that's `rope.dimension_count = 64`
        //     (out of `head_dim = 256`).
        //
        //     ref: lines 324-338
        let n_rot: c_int = 64;
        let sections: [c_int; 4] = *rope_sections;

        let q_out = sys::ggml_rope_multi(
            ctx,
            q_view,
            positions,
            std::ptr::null_mut(),
            n_rot,
            sections.as_ptr(),
            GGML_ROPE_TYPE_MROPE,
            0,
            q35::ROPE_THETA,
            1.0,
            0.0,
            1.0,
            0.0,
            0.0,
        );
        let kcur = sys::ggml_rope_multi(
            ctx,
            kcur,
            positions,
            std::ptr::null_mut(),
            n_rot,
            sections.as_ptr(),
            GGML_ROPE_TYPE_MROPE,
            0,
            q35::ROPE_THETA,
            1.0,
            0.0,
            1.0,
            0.0,
            0.0,
        );

        // ── Write K/V into the persistent cache at slot
        //    [kv_start..kv_start+n_tokens). ref: lines 340-361
        //
        // cache_k is [head_dim, max_ctx, n_head_kv]. We want to copy
        // kcur [head_dim, n_head_kv, n_tokens] into
        // cache_k[:, kv_start:kv_start+n_tokens, :].
        //
        // Easiest: transpose kcur to [head_dim, n_tokens, n_head_kv]
        // so its axes line up with cache_k's
        // [head_dim, max_ctx, n_head_kv], then view a slice of cache_k
        // and copy.
        let kcur_t = sys::ggml_permute(ctx, kcur, 0, 2, 1, 3); // [head_dim, n_tokens, n_head_kv]
        let vcur_t = sys::ggml_permute(ctx, vcur, 0, 2, 1, 3); // [head_dim, n_tokens, n_head_kv]

        let cache_k_nb1 = (*cache_k).nb[1];
        let cache_k_nb2 = (*cache_k).nb[2];
        let cache_v_nb1 = (*cache_v).nb[1];
        let cache_v_nb2 = (*cache_v).nb[2];

        let k_slot = sys::ggml_view_3d(
            ctx,
            cache_k,
            q35::HEAD_DIM as i64,
            n_tokens as i64,
            q35::N_HEAD_KV as i64,
            cache_k_nb1,
            cache_k_nb2,
            cache_k_nb1 * (kv_start as libc::size_t),
        );
        let v_slot = sys::ggml_view_3d(
            ctx,
            cache_v,
            q35::HEAD_DIM as i64,
            n_tokens as i64,
            q35::N_HEAD_KV as i64,
            cache_v_nb1,
            cache_v_nb2,
            cache_v_nb1 * (kv_start as libc::size_t),
        );

        sys::ggml_build_forward_expand(gf, sys::ggml_cpy(ctx, kcur_t, k_slot));
        sys::ggml_build_forward_expand(gf, sys::ggml_cpy(ctx, vcur_t, v_slot));

        // ── Flash attention over the valid slice [0, kv_start + n_tokens).
        //     ref: lines 363-395
        let kv_len = kv_start + n_tokens;

        // FA kernel alignment requirements for the kv view length
        // (f16/Q* paths are stride 1; future TurboQuant paths would
        // need 256 alignment, kept behind a compile-time constant here
        // for drop-in extension). FATTN_KQ_STRIDE=256 (see
        // fattn.cu:get_best_fattn_kernel). Round up for TBQ cache
        // types; the caller's attn_mask is built with the same padded
        // length so positions beyond the real kv_len get -inf.
        const FATTN_STRIDE: c_int = 1;
        let kv_len_padded = ((kv_len + FATTN_STRIDE - 1) / FATTN_STRIDE) * FATTN_STRIDE;

        // Q needs to be [head_dim, n_tokens, n_head] for flash_attn_ext.
        let qfa_perm = sys::ggml_permute(ctx, q_out, 0, 2, 1, 3);
        let qfa = sys::ggml_cont(ctx, qfa_perm);

        // K and V from cache: a view into the first kv_len_padded
        // slots. For non-TBQ paths `kv_len_padded == kv_len` so this
        // is identical to the old behaviour.
        let kfa = sys::ggml_view_3d(
            ctx,
            cache_k,
            q35::HEAD_DIM as i64,
            kv_len_padded as i64,
            q35::N_HEAD_KV as i64,
            cache_k_nb1,
            cache_k_nb2,
            0,
        );
        let vfa = sys::ggml_view_3d(
            ctx,
            cache_v,
            q35::HEAD_DIM as i64,
            kv_len_padded as i64,
            q35::N_HEAD_KV as i64,
            cache_v_nb1,
            cache_v_nb2,
            0,
        );

        // Causal mask: for `n_tokens==1` we don't need one (a single
        // query attending to all keys is trivially causal). For
        // `n_tokens>1` the caller must provide a mask shaped
        // [kv_len, n_tokens] with 0 for attendable positions and -inf
        // for positions beyond the causal boundary.
        let kq_scale = 1.0_f32 / (q35::HEAD_DIM as f32).sqrt();
        let attn0 =
            sys::ggml_flash_attn_ext(ctx, qfa, kfa, vfa, attn_mask, kq_scale, 0.0, 0.0);
        // attn: [head_dim, n_head, n_tokens] (permuted)
        let attn1 = sys::ggml_reshape_2d(ctx, attn0, q35::Q_DIM as i64, n_tokens as i64);

        // ── Apply the sigmoid gate from the packed Q. ref: lines 399-401
        let gate_sig = sys::ggml_sigmoid(ctx, gate);
        let attn2 = sys::ggml_mul(ctx, attn1, gate_sig);

        // ── Output projection. ref: lines 403-405
        sys::ggml_mul_mat(ctx, l.wo, attn2) // [hidden, n_tokens]
    }
}

// ─── build_delta_net_block ───────────────────────────────────────
//
// ref: `qwen35_target_graph.cpp:408-664`
//
// Gated DeltaNet block using the fused `ggml_gated_delta_net`
// primitive. Matches the semantics of llama.cpp's
// `build_layer_attn_linear + build_delta_net_fused`. Updates
// `conv_state` and `ssm_state` in place.
//
// When `cap` is non-null, the function populates
// `cap.ssm_intermediate_states` with a view into the
// gated_delta_net result's per-step recurrent states and
// `cap.conv_input` with the concatenated conv input (old state +
// new tokens), both of which are persisted via in-graph `ggml_cpy`
// into the cache so the spec-decode loop can rollback SSM + conv
// state to any intermediate step without a replay forward pass.

#[allow(clippy::too_many_arguments)]
fn build_delta_net_block(
    ctx: *mut ggml_context,
    gf: *mut ggml_cgraph,
    l: &TargetLayer,
    cur: *mut ggml_tensor,           // [hidden, n_tokens]
    conv_state: *mut ggml_tensor,    // [kernel-1, conv_channels] persistent
    ssm_state: *mut ggml_tensor,     // [head_v_dim, head_v_dim, num_v_heads] persistent
    n_tokens: c_int,
    cap: Option<&mut DeltaNetCapture>,
    parent_ids: *mut ggml_tensor,    // optional [n_tokens] i32; tree mode when non-null
) -> *mut ggml_tensor {
    // Constants (mirrors lines 429-435).
    let head_k_dim = q35::HEAD_K_DIM; // 128
    let num_k_heads = q35::SSM_N_GROUP; // 16
    let num_v_heads = q35::SSM_DT_RANK; // 48
    let head_v_dim = q35::HEAD_V_DIM; // 128
    let n_seqs: i64 = 1;
    let n_seq_tokens: i64 = n_tokens as i64;

    unsafe {
        // ── qkv_mixed = wqkv @ cur  [10240, n_tokens]. ref: lines 437-439
        let qkv_mixed0 = sys::ggml_mul_mat(ctx, l.wqkv, cur);
        let qkv_mixed = sys::ggml_reshape_3d(
            ctx,
            qkv_mixed0,
            q35::CONV_CHANNELS as i64,
            n_seq_tokens,
            n_seqs,
        );

        // ── z = wqkv_gate @ cur  [inner, n_tokens]. ref: line 442
        let z = sys::ggml_mul_mat(ctx, l.wqkv_gate, cur);

        // ── beta = ssm_beta @ cur  [dt_rank, n_tokens]. ref: lines 444-447
        let beta0 = sys::ggml_mul_mat(ctx, l.ssm_beta, cur);
        let beta1 = sys::ggml_reshape_4d(ctx, beta0, 1, num_v_heads as i64, n_seq_tokens, n_seqs);
        let beta = sys::ggml_sigmoid(ctx, beta1);

        // ── alpha = ssm_alpha @ cur  [dt_rank, n_tokens]
        //    alpha = alpha + ssm_dt_bias  (per-head bias)
        //    alpha = softplus(alpha)
        //    g     = alpha * ssm_a  (-A_log.exp() * softplus)
        //
        //    ref: lines 449-458
        let alpha0 = sys::ggml_mul_mat(ctx, l.ssm_alpha, cur);
        let alpha1 = sys::ggml_reshape_3d(ctx, alpha0, num_v_heads as i64, n_seq_tokens, n_seqs);
        let alpha2 = sys::ggml_add(ctx, alpha1, l.ssm_dt_bias);
        let alpha3 = sys::ggml_softplus(ctx, alpha2);
        let g0 = sys::ggml_mul(ctx, alpha3, l.ssm_a);
        let g_tensor =
            sys::ggml_reshape_4d(ctx, g0, 1, num_v_heads as i64, n_seq_tokens, n_seqs);

        // ── Fetch conv state [kernel-1, conv_channels] and prepend to
        //    qkv_mixed along the token axis to form the convolution
        //    input. ref: lines 460-470
        let conv_states_r = sys::ggml_reshape_3d(
            ctx,
            conv_state,
            (q35::SSM_CONV_KERN - 1) as i64,
            q35::CONV_CHANNELS as i64,
            n_seqs,
        );

        // qkv_mixed is currently [conv_channels, n_tokens, n_seqs]; we
        // need [n_tokens, conv_channels, n_seqs] to concat on dim 0.
        let qkv_t = sys::ggml_transpose(ctx, qkv_mixed);

        let conv_input = sys::ggml_concat(ctx, conv_states_r, qkv_t, 0);
        // conv_input: [kernel-1 + n_tokens, conv_channels, n_seqs]

        // For spec-decode rollback: copy the full conv_input into the
        // persistent cache buffer via an in-graph ggml_cpy. This
        // avoids marking conv_input as a graph output (which would
        // force the gallocr to preserve its memory past graph_compute).
        // After graph_compute, the cache buffer's data is always valid;
        // the rollback code slices it at commit_n.
        //
        // ref: lines 472-479
        if let Some(c) = cap.as_deref() {
            if !c.conv_input.is_null() {
                sys::ggml_build_forward_expand(gf, sys::ggml_cpy(ctx, conv_input, c.conv_input));
            }
        }

        // ── Save the last (kernel-1) steps back to conv_state.
        //     ref: lines 481-486
        let conv_input_ne0 = (*conv_input).ne[0];
        let conv_input_nb1 = (*conv_input).nb[1];
        let conv_input_nb2 = (*conv_input).nb[2];
        let conv_input_elt = sys::ggml_element_size(conv_input);
        let last_conv = sys::ggml_view_3d(
            ctx,
            conv_input,
            (q35::SSM_CONV_KERN - 1) as i64,
            q35::CONV_CHANNELS as i64,
            n_seqs,
            conv_input_nb1,
            conv_input_nb2,
            ((conv_input_ne0 - (q35::SSM_CONV_KERN - 1) as i64) as libc::size_t) * conv_input_elt,
        );
        sys::ggml_build_forward_expand(gf, sys::ggml_cpy(ctx, last_conv, conv_state));

        // ── 1D conv + silu
        //    Tree mode: use the parent-chain-aware variant so sibling
        //    nodes gather their conv window from their actual tree
        //    parent instead of the DFS predecessor. Without this,
        //    siblings get garbage logits (the conv output would mix
        //    unrelated branches).
        //
        //    ref: lines 488-496
        let conv_out0 = if !parent_ids.is_null() {
            sys::ggml_ssm_conv_tree(ctx, conv_input, l.ssm_conv1d, parent_ids)
        } else {
            sys::ggml_ssm_conv(ctx, conv_input, l.ssm_conv1d)
        };
        let conv_out = sys::ggml_silu(ctx, conv_out0);

        // conv_out: [conv_channels, n_tokens, n_seqs]. ref: lines 498-523
        let q_offset: i64 = 0;
        let k_offset: i64 = (num_k_heads * head_k_dim) as i64;
        let v_offset: i64 = 2 * (num_k_heads * head_k_dim) as i64;

        let elt = sys::ggml_element_size(conv_out);
        let row_size = (q35::CONV_CHANNELS as libc::size_t) * elt;

        let mut q_c = sys::ggml_view_4d(
            ctx,
            conv_out,
            head_k_dim as i64,
            num_k_heads as i64,
            n_seq_tokens,
            n_seqs,
            (head_k_dim as libc::size_t) * elt,
            row_size,
            row_size * (n_seq_tokens as libc::size_t),
            (q_offset as libc::size_t) * elt,
        );
        let mut k_c = sys::ggml_view_4d(
            ctx,
            conv_out,
            head_k_dim as i64,
            num_k_heads as i64,
            n_seq_tokens,
            n_seqs,
            (head_k_dim as libc::size_t) * elt,
            row_size,
            row_size * (n_seq_tokens as libc::size_t),
            (k_offset as libc::size_t) * elt,
        );
        let v_c = sys::ggml_view_4d(
            ctx,
            conv_out,
            head_v_dim as i64,
            num_v_heads as i64,
            n_seq_tokens,
            n_seqs,
            (head_v_dim as libc::size_t) * elt,
            row_size,
            row_size * (n_seq_tokens as libc::size_t),
            (v_offset as libc::size_t) * elt,
        );

        // L2 norm on Q and K. ref: lines 525-527
        q_c = sys::ggml_l2_norm(ctx, q_c, q35::EPS);
        k_c = sys::ggml_l2_norm(ctx, k_c, q35::EPS);

        // Repeat Q and K from num_k_heads to num_v_heads so they match
        // V's layout (only needed if not using the fused op's
        // broadcast support).
        //
        // ref: lines 529-534
        if num_k_heads != num_v_heads {
            q_c = sys::ggml_repeat_4d(
                ctx,
                q_c,
                head_k_dim as i64,
                num_v_heads as i64,
                n_seq_tokens,
                n_seqs,
            );
            k_c = sys::ggml_repeat_4d(
                ctx,
                k_c,
                head_k_dim as i64,
                num_v_heads as i64,
                n_seq_tokens,
                n_seqs,
            );
        }

        // ── SSM state (recurrent): reshape to [S_v, S_v, H_v, n_seqs].
        //     ref: lines 536-538
        let s = sys::ggml_reshape_4d(
            ctx,
            ssm_state,
            head_v_dim as i64,
            head_v_dim as i64,
            num_v_heads as i64,
            n_seqs,
        );

        // ── Fused Gated DeltaNet op — returns packed
        //    (output | new_state [| intermediates]). ref: lines 540-583
        //
        //    When `cap.ssm_intermediate_states` is present AND we are
        //    in tree mode, use the `_tree_persist` variant: the kernel
        //    writes per-token intermediate states DIRECTLY into the
        //    persistent cache buffer, eliminating the downstream
        //    `ggml_cpy` that would otherwise copy them. Saves ~5-10 ms
        //    per verify step (memory-bandwidth bound) on 27B.
        let persist_inter: *mut ggml_tensor = match (parent_ids.is_null(), cap.as_deref()) {
            (false, Some(c)) if !c.ssm_intermediate_states.is_null() => c.ssm_intermediate_states,
            _ => std::ptr::null_mut(),
        };

        // Chunked delta-net path: chain-only (no parent_ids), no
        // per-token capture (no cap). Ported from llama.cpp
        // `src/models/delta-net-base.cpp::build_delta_net_chunking`.
        //
        // Currently OFF by default in the Rust port — the reference
        // itself says "port produces correct shape but slightly wrong
        // final state, causing AL degradation and loopy output. Set
        // DFLASH27B_CHUNKED=1 to opt in for A/B testing while
        // debugging." We don't implement the chunked path in Rust
        // yet; the env hook is honored only by panicking on opt-in.
        //
        // ref: lines 552-565
        let use_chunked = parent_ids.is_null()
            && cap.is_none()
            && n_seq_tokens > 1
            && std::env::var("DFLASH27B_CHUNKED")
                .ok()
                .and_then(|s| s.parse::<i32>().ok())
                .map(|v| v != 0)
                .unwrap_or(false);

        if use_chunked {
            // Chunked path writes output + new_state directly; persist
            // new_state to the ssm_state slot. ref: lines 570-646 of
            // qwen35_target_graph.cpp + delta_net_chunked.cpp:223-233
            let r = build_delta_net_chunked(ctx, q_c, k_c, v_c, g_tensor, beta, s);
            let output = r.output;
            let new_state = r.new_state;
            sys::ggml_build_forward_expand(gf, sys::ggml_cpy(ctx, new_state, s));

            // Gated output norm and final projection — shared between
            // chunked and sequential paths. ref: lines 649-663
            let z_4d = sys::ggml_reshape_4d(
                ctx,
                z,
                head_v_dim as i64,
                num_v_heads as i64,
                n_seq_tokens,
                n_seqs,
            );
            let output_n0 = sys::ggml_rms_norm(ctx, output, q35::EPS);
            let output_n1 = sys::ggml_mul(ctx, output_n0, l.ssm_norm);
            let z_silu = sys::ggml_silu(ctx, z_4d);
            let output_n = sys::ggml_mul(ctx, output_n1, z_silu);
            let flat = sys::ggml_reshape_3d(
                ctx,
                output_n,
                (head_v_dim * num_v_heads) as i64,
                n_seq_tokens,
                n_seqs,
            );
            let out_mul = sys::ggml_mul_mat(ctx, l.ssm_out, flat);
            return sys::ggml_reshape_2d(
                ctx,
                out_mul,
                DFLASH27B_TARGET_HIDDEN as i64,
                n_seq_tokens * n_seqs,
            );
        }

        // ref: lines 577-583
        let result = if !persist_inter.is_null() {
            sys::ggml_gated_delta_net_tree_persist(
                ctx,
                q_c,
                k_c,
                v_c,
                g_tensor,
                beta,
                s,
                parent_ids,
                persist_inter,
            )
        } else if !parent_ids.is_null() {
            sys::ggml_gated_delta_net_tree(
                ctx, q_c, k_c, v_c, g_tensor, beta, s, parent_ids,
            )
        } else {
            sys::ggml_gated_delta_net(ctx, q_c, k_c, v_c, g_tensor, beta, s)
        };

        // Slice output and new_state out of the packed result.
        // ref: lines 586-601
        let s_v: i64 = head_v_dim as i64;
        let h_v: i64 = num_v_heads as i64;
        let r_elt = sys::ggml_element_size(result);
        let output = sys::ggml_view_4d(
            ctx,
            result,
            s_v,
            h_v,
            n_seq_tokens,
            n_seqs,
            (s_v as libc::size_t) * r_elt,
            ((s_v * h_v) as libc::size_t) * r_elt,
            ((s_v * h_v * n_seq_tokens) as libc::size_t) * r_elt,
            0,
        );
        let new_state = sys::ggml_view_4d(
            ctx,
            result,
            s_v,
            s_v,
            h_v,
            n_seqs,
            (s_v as libc::size_t) * r_elt,
            ((s_v * s_v) as libc::size_t) * r_elt,
            ((s_v * s_v * h_v) as libc::size_t) * r_elt,
            ((s_v * h_v * n_seq_tokens * n_seqs) as libc::size_t) * r_elt,
        );

        // Persist new_state back to cache. ref: line 604
        sys::ggml_build_forward_expand(gf, sys::ggml_cpy(ctx, new_state, ssm_state));

        // Expose per-step intermediate states for spec-decode rollback.
        // ref: lines 606-636
        //
        // The patched `ggml_gated_delta_net` kernel appends an
        // intermediate-states region to the result tensor after the
        // final-state slot. Layout in result->data:
        //   [ attn_out:           S_v*H_v*n_seq_tokens*n_seqs floats
        //   | final_state:        S_v*S_v*H_v*n_seqs          floats
        //   | intermediate_states: S_v*S_v*H_v*n_seq_tokens*n_seqs floats ]
        //
        // Instead of marking the whole `result` tensor as a graph
        // output (which forces gallocr to preserve ~50 MB per layer ×
        // 48 layers of otherwise transient memory and inflates
        // graph_build by ~35 ms), we create a VIEW into the
        // intermediate region and ggml_cpy it into the persistent
        // cache buffer cap->ssm_intermediate_states. The gallocr is
        // unaware of the persistent cache, so verify_build stays
        // cheap. Matches SGLang's mamba_caches.intermediate_ssm
        // pattern.
        if let Some(c) = cap {
            if !c.ssm_intermediate_states.is_null() && persist_inter.is_null() {
                // Legacy cpy path: only used when the kernel wrote
                // intermediates into its own result region (i.e. when
                // we did NOT use _tree_persist). The _tree_persist
                // variant writes directly to the cache buffer and
                // this cpy becomes redundant, saving ~5-10 ms per
                // verify step.
                let inter_offset = ((s_v * h_v * n_seq_tokens * n_seqs) as libc::size_t) * r_elt
                    + ((s_v * s_v * h_v * n_seqs) as libc::size_t) * r_elt;
                let inter_view = sys::ggml_view_4d(
                    ctx,
                    result,
                    s_v,
                    s_v,
                    h_v,
                    n_seq_tokens,
                    (s_v as libc::size_t) * r_elt,
                    ((s_v * s_v) as libc::size_t) * r_elt,
                    ((s_v * s_v * h_v) as libc::size_t) * r_elt,
                    inter_offset,
                );
                sys::ggml_build_forward_expand(
                    gf,
                    sys::ggml_cpy(ctx, inter_view, c.ssm_intermediate_states),
                );
            }
        }

        // ── Gated output norm: rms_norm(output) * silu(z_4d).
        //     ref: lines 649-654
        let z_4d = sys::ggml_reshape_4d(
            ctx,
            z,
            head_v_dim as i64,
            num_v_heads as i64,
            n_seq_tokens,
            n_seqs,
        );
        let output_n0 = sys::ggml_rms_norm(ctx, output, q35::EPS);
        let output_n1 = sys::ggml_mul(ctx, output_n0, l.ssm_norm);
        let z_silu = sys::ggml_silu(ctx, z_4d);
        let output_n = sys::ggml_mul(ctx, output_n1, z_silu);

        // Reshape to [d_inner, n_tokens]. ref: lines 656-658
        let flat = sys::ggml_reshape_3d(
            ctx,
            output_n,
            (head_v_dim * num_v_heads) as i64,
            n_seq_tokens,
            n_seqs,
        );

        // Output projection. ref: lines 660-662
        let out_mul = sys::ggml_mul_mat(ctx, l.ssm_out, flat);
        sys::ggml_reshape_2d(
            ctx,
            out_mul,
            DFLASH27B_TARGET_HIDDEN as i64,
            n_seq_tokens * n_seqs,
        )
    }
}

// ─── build_qwen35_graph ──────────────────────────────────────────
//
// ref: `qwen35_target_graph.cpp:668-804`
//
// Main graph entry point. Caller owns `ctx` + `gf` + the input
// tensors. Output pointers belong to `ctx`; reading their `->data`
// after `ggml_backend_graph_compute` is valid.

/// Build the full target forward compute graph.
///
/// ref: `qwen35_target_graph.cpp:668-804`
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn build_qwen35_graph(
    ctx: *mut ggml_context,
    gf: *mut ggml_cgraph,
    w: &TargetWeights,
    cache: &mut TargetCache,
    in_: &QwenGraphInputs,
) -> QwenGraphOutputs {
    let n_tokens = in_.n_tokens;

    // 1. Caller supplies pre-embedded inputs via in.inp_embed (CPU
    //    lookup done ahead of time, zero GPU cost for the embedding
    //    table). ref: lines 675-679
    let mut inp_l = in_.inp_embed;

    let mut fa_idx: usize = 0;
    let mut dn_idx: usize = 0;

    // If the caller requested capture, size the output list to the
    // total delta-net layer count so we can index by `dn_idx` as we
    // iterate the layers. ref: lines 683-690
    let mut og: QwenGraphOutputs = QwenGraphOutputs::default();
    if in_.capture_delta_intermediate {
        let n_full_attn = w.n_layer / w.full_attention_interval;
        let n_delta = w.n_layer - n_full_attn;
        og.delta_captures = vec![DeltaNetCapture::default(); n_delta as usize];
    }

    // DFlash target layer IDs for feature capture: {1, 16, 31, 46, 61}
    // HF hidden_states[lid+1] convention — capture AFTER layer 'lid'
    // runs. ref: lines 692-695
    const CAPTURE_LAYERS: [c_int; DFLASH27B_DRAFT_N_TARGET_LAYERS as usize] =
        [1, 16, 31, 46, 61];

    let hidden = w.n_embd;
    let eps = q35::EPS;

    unsafe {
        for il in 0..w.n_layer {
            let layer = &w.layers[il as usize];
            let is_attn = ((il + 1) % w.full_attention_interval) == 0;

            let inp_sa = inp_l;

            // Pre-attention norm. ref: line 707
            let cur_norm = rms_norm_mul(ctx, inp_l, layer.attn_norm, eps);

            let cur_attn = if is_attn {
                let r = build_full_attn_block(
                    ctx,
                    gf,
                    layer,
                    cur_norm,
                    in_.positions,
                    &w.rope_sections,
                    cache.attn_k[fa_idx],
                    cache.attn_v[fa_idx],
                    in_.attn_mask,
                    in_.kv_start,
                    n_tokens,
                );
                fa_idx += 1;
                r
            } else {
                // ref: lines 714-730
                let (cur_out, captured) = if in_.capture_delta_intermediate {
                    // Point at the persistent per-layer cache buffers
                    // so `build_delta_net_block` can ggml_cpy into
                    // them during graph execution. The caller reads
                    // from these tensors post-compute; their ->data
                    // pointers are always valid because they're
                    // cache-resident, not gallocr-managed.
                    let mut local_cap = DeltaNetCapture {
                        ssm_intermediate_states: cache.ssm_intermediate[dn_idx],
                        conv_input: cache.conv_input_cache[dn_idx],
                    };
                    let r = build_delta_net_block(
                        ctx,
                        gf,
                        layer,
                        cur_norm,
                        cache.conv_state[dn_idx],
                        cache.ssm_state[dn_idx],
                        n_tokens,
                        Some(&mut local_cap),
                        in_.parent_ids,
                    );
                    (r, Some(local_cap))
                } else {
                    let r = build_delta_net_block(
                        ctx,
                        gf,
                        layer,
                        cur_norm,
                        cache.conv_state[dn_idx],
                        cache.ssm_state[dn_idx],
                        n_tokens,
                        None,
                        in_.parent_ids,
                    );
                    (r, None)
                };
                if let Some(c) = captured {
                    og.delta_captures[dn_idx] = c;
                }
                dn_idx += 1;
                cur_out
            };

            // Residual. ref: line 733
            let cur1 = sys::ggml_add(ctx, cur_attn, inp_sa);

            // Post-attention norm (before FFN). ref: lines 735-737
            let ffn_residual = cur1;
            let post = rms_norm_mul(ctx, cur1, layer.attn_post_norm, eps);

            // SwiGLU FFN. ref: lines 739-741
            let ffn = build_swiglu_ffn(ctx, post, layer);
            let cur2 = sys::ggml_add(ctx, ffn, ffn_residual);

            // ── DFlash layer feature capture ──
            // Write `cur` into the rolling target_feat buffer. The
            // buffer is a ring of `target_feat_cap` slots; position P
            // maps to slot `P % cap`. Within a single build call we
            // may straddle the wrap boundary, so we split the copy
            // into up to two contiguous ggml_cpy ops.
            //
            // ref: lines 743-787
            if in_.capture_layers && !cache.target_feat.is_null() {
                let mut capture_idx: i32 = -1;
                for k in 0..DFLASH27B_DRAFT_N_TARGET_LAYERS {
                    if CAPTURE_LAYERS[k as usize] == il {
                        capture_idx = k;
                        break;
                    }
                }
                if capture_idx >= 0 {
                    let elt = sys::ggml_element_size(cache.target_feat);
                    let col_stride = (*cache.target_feat).nb[1];
                    let cap_len = cache.target_feat_cap;
                    let slot_start = in_.kv_start % cap_len;
                    let pre_n = std::cmp::min(n_tokens, cap_len - slot_start);
                    let post_n = n_tokens - pre_n;

                    let cur_2d = sys::ggml_reshape_2d(ctx, cur2, hidden as i64, n_tokens as i64);

                    // First slice: [slot_start..slot_start+pre_n) in ring.
                    {
                        let offset = (slot_start as libc::size_t) * col_stride
                            + (capture_idx as libc::size_t)
                                * (hidden as libc::size_t)
                                * elt;
                        let slot = sys::ggml_view_2d(
                            ctx,
                            cache.target_feat,
                            hidden as i64,
                            pre_n as i64,
                            col_stride,
                            offset,
                        );
                        let src = sys::ggml_view_2d(
                            ctx,
                            cur_2d,
                            hidden as i64,
                            pre_n as i64,
                            (*cur_2d).nb[1],
                            0,
                        );
                        sys::ggml_build_forward_expand(gf, sys::ggml_cpy(ctx, src, slot));
                    }

                    // Second slice: wrap-around at [0..post_n) if needed.
                    if post_n > 0 {
                        let offset = (capture_idx as libc::size_t)
                            * (hidden as libc::size_t)
                            * elt;
                        let slot = sys::ggml_view_2d(
                            ctx,
                            cache.target_feat,
                            hidden as i64,
                            post_n as i64,
                            col_stride,
                            offset,
                        );
                        let src = sys::ggml_view_2d(
                            ctx,
                            cur_2d,
                            hidden as i64,
                            post_n as i64,
                            (*cur_2d).nb[1],
                            (pre_n as libc::size_t) * (*cur_2d).nb[1],
                        );
                        sys::ggml_build_forward_expand(gf, sys::ggml_cpy(ctx, src, slot));
                    }
                }
            }

            inp_l = cur2;
        }

        // 2. Final norm. ref: line 793
        let out = rms_norm_mul(ctx, inp_l, w.out_norm, q35::EPS);

        // 3. LM head. ref: lines 795-799
        let logits = sys::ggml_mul_mat(ctx, w.output, out);
        let logits_name = CString::new("logits").unwrap();
        sys::ggml_set_name(logits, logits_name.as_ptr());

        sys::ggml_build_forward_expand(gf, logits);

        og.logits = logits;
        og
    }
}


// ════════════════════════════════════════════════════════════════
//  DRAFT GRAPH (qwen3_dflash_graph.cpp)
// ════════════════════════════════════════════════════════════════

// ─── Input / output structs ──────────────────────────────────────
//
// ref: `dflash_graph.h:10-26`

pub struct DraftGraphInputs {
    /// length of `target_hidden_cat` along `ne[1]`
    pub ctx_len: i32,
    /// `[hidden, q_len=16, 1]` f32
    pub noise_embed: *mut ggml_tensor,
    /// `[5*hidden, ctx_len, 1]` f32
    pub target_hidden_cat: *mut ggml_tensor,
    /// `[q_len]` i32 — values `[ctx_len..ctx_len+q_len-1]`
    pub positions_q: *mut ggml_tensor,
    /// `[ctx_len+q_len]` i32 — values `[0..ctx_len+q_len-1]`
    pub positions_k: *mut ggml_tensor,
    /// Optional: if non-null, the graph projects final hidden states
    /// through this LM head (shape `[hidden, vocab]`) and returns
    /// logits alongside the hidden states. Used for DFlash integration
    /// where the draft shares the target's lm_head.
    pub lm_head: *mut ggml_tensor,
}

impl Default for DraftGraphInputs {
    fn default() -> Self {
        Self {
            ctx_len: 0,
            noise_embed: std::ptr::null_mut(),
            target_hidden_cat: std::ptr::null_mut(),
            positions_q: std::ptr::null_mut(),
            positions_k: std::ptr::null_mut(),
            lm_head: std::ptr::null_mut(),
        }
    }
}

pub struct DraftGraphOutputs {
    /// `[hidden, q_len, 1]` (always set)
    pub hidden_states: *mut ggml_tensor,
    /// `[vocab, q_len, 1]` — non-null iff `lm_head` was provided
    pub logits: *mut ggml_tensor,
}

impl Default for DraftGraphOutputs {
    fn default() -> Self {
        Self {
            hidden_states: std::ptr::null_mut(),
            logits: std::ptr::null_mut(),
        }
    }
}

// ─── build_draft_graph ───────────────────────────────────────────
//
// ref: `qwen3_dflash_graph.cpp:38-166`

/// Build the draft graph. Caller owns `ctx` + both `DraftWeights` and
/// the input tensors. The returned [`DraftGraphOutputs`] pointers are
/// graph nodes inside `ctx` — their data lives inside the backend
/// buffer assigned to `ctx` once `ggml_backend_graph_compute` runs.
///
/// # Safety
///
/// All pointer fields on `w` and `in_` must be valid ggml tensors
/// belonging to `ctx` (or allocated into a buffer `ctx` can read).
///
/// ref: `qwen3_dflash_graph.cpp:38-166`
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn build_draft_graph(
    ctx: *mut ggml_context,
    w: &DraftWeights,
    in_: &DraftGraphInputs,
) -> DraftGraphOutputs {
    let q_len = DFLASH27B_DRAFT_BLOCK_SIZE;
    let ctx_len = in_.ctx_len;
    let total_k = ctx_len + q_len;
    let n_head = DFLASH27B_TARGET_N_HEADS; // 32
    let n_kv = DFLASH27B_TARGET_N_KV_HEADS; // 8
    let head_dim = DFLASH27B_TARGET_HEAD_DIM; // 128
    let eps = DFLASH27B_RMS_EPS;
    let rope_base = DFLASH27B_ROPE_THETA;

    unsafe {
        // ── 1. Feature fusion:
        //   target_feat = rms_norm(fc @ target_hidden_cat, hidden_norm)
        //
        //   fc:                [5*hidden, hidden]  (ne[0]=5*hidden, ne[1]=hidden)
        //   target_hidden_cat: [5*hidden, ctx_len, 1]
        //   Result:            [hidden,   ctx_len, 1]
        //
        //   ref: lines 53-60
        let mut target_feat = sys::ggml_mul_mat(ctx, w.fc, in_.target_hidden_cat);
        target_feat = sys::ggml_rms_norm(ctx, target_feat, eps);
        target_feat = sys::ggml_mul(ctx, target_feat, w.hidden_norm);
        sys::ggml_set_name(target_feat, b"target_feat\0".as_ptr().cast());

        // ── 2. Decoder layers. ref: lines 62-148
        let mut h = in_.noise_embed; // [hidden, q_len, 1]

        for il in 0..(DFLASH27B_DRAFT_LAYERS as usize) {
            let l = &w.layers[il];

            // ── 2a. Attention pre-norm. ref: lines 68-70
            let mut hn = sys::ggml_rms_norm(ctx, h, eps);
            hn = sys::ggml_mul(ctx, hn, l.attn_norm);

            // ── 2b. Q from noise only, then per-head RMSNorm.
            //     wq: [hidden, q_dim=4096]. ref: lines 72-77
            let mut q = sys::ggml_mul_mat(ctx, l.wq, hn); // [q_dim, q_len, 1]
            q = sys::ggml_reshape_3d(ctx, q, head_dim as i64, n_head as i64, q_len as i64);
            q = sys::ggml_rms_norm(ctx, q, eps); // normalize along head_dim
            q = sys::ggml_mul(ctx, q, l.q_norm); // broadcast [head_dim]

            // ── 2c. K and V from target_feat AND noise, then concat
            //     along sequence.
            //     wk, wv: [hidden, kv_dim=1024]. ref: lines 79-95
            let kctx = sys::ggml_mul_mat(ctx, l.wk, target_feat); // [kv_dim, ctx_len, 1]
            let kn = sys::ggml_mul_mat(ctx, l.wk, hn); // [kv_dim, q_len,   1]
            let vctx = sys::ggml_mul_mat(ctx, l.wv, target_feat);
            let vn = sys::ggml_mul_mat(ctx, l.wv, hn);

            // concat along ne[1] (sequence) — ggml_concat second arg dim=1
            let mut k = sys::ggml_concat(ctx, kctx, kn, 1); // [kv_dim, total_k, 1]
            let mut v = sys::ggml_concat(ctx, vctx, vn, 1);

            // Per-head k_norm
            k = sys::ggml_reshape_3d(ctx, k, head_dim as i64, n_kv as i64, total_k as i64);
            k = sys::ggml_rms_norm(ctx, k, eps);
            k = sys::ggml_mul(ctx, k, l.k_norm);

            v = sys::ggml_reshape_3d(ctx, v, head_dim as i64, n_kv as i64, total_k as i64);

            // ── 2d. RoPE (NEOX, theta=10M).
            //   Q: positions_q  [q_len]   values [ctx_len..ctx_len+q_len-1]
            //   K: positions_k  [total_k] values [0..total_k-1]
            //
            //   ref: lines 100-107
            q = sys::ggml_rope_ext(
                ctx,
                q,
                in_.positions_q,
                std::ptr::null_mut(),
                head_dim,
                GGML_ROPE_TYPE_NEOX,
                0,
                rope_base,
                1.0,
                0.0,
                1.0,
                0.0,
                0.0,
            );
            k = sys::ggml_rope_ext(
                ctx,
                k,
                in_.positions_k,
                std::ptr::null_mut(),
                head_dim,
                GGML_ROPE_TYPE_NEOX,
                0,
                rope_base,
                1.0,
                0.0,
                1.0,
                0.0,
                0.0,
            );

            // ── 2e. Permute into the layout flash_attn_ext wants.
            //   q: [n_embd_k=head_dim, n_batch=q_len, n_head,   ne3]
            //   k: [n_embd_k=head_dim, n_kv=total_k, n_head_kv, ne3]
            //   v: [n_embd_v=head_dim, n_kv=total_k, n_head_kv, ne3]
            //
            //   ref: lines 109-118
            q = sys::ggml_permute(ctx, q, 0, 2, 1, 3); // [head_dim, q_len,   n_head, 1]
            q = sys::ggml_cont(ctx, q);
            k = sys::ggml_permute(ctx, k, 0, 2, 1, 3); // [head_dim, total_k, n_kv,   1]
            k = sys::ggml_cont(ctx, k);
            v = sys::ggml_permute(ctx, v, 0, 2, 1, 3); // [head_dim, total_k, n_kv,   1]
            v = sys::ggml_cont(ctx, v);

            // ── 2f. Non-causal flash attention; GQA broadcast handled
            //     internally. ref: lines 120-127
            let scale = 1.0_f32 / (head_dim as f32).sqrt();
            let mut attn = sys::ggml_flash_attn_ext(
                ctx,
                q,
                k,
                v,
                std::ptr::null_mut(),
                scale,
                0.0,
                0.0,
            );
            // attn result: [n_embd_v=head_dim, n_head, n_batch=q_len, 1]
            attn = sys::ggml_reshape_2d(ctx, attn, (head_dim * n_head) as i64, q_len as i64);
            // attn: [q_dim, q_len]

            // ── 2g. Output projection + residual.
            //     wo: [q_dim, hidden]  (ne[0]=q_dim, ne[1]=hidden)
            //
            //     ref: lines 129-132
            let attn_out = sys::ggml_mul_mat(ctx, l.wo, attn); // [hidden, q_len]
            h = sys::ggml_add(ctx, h, attn_out);

            // ── 2h. FFN pre-norm. ref: lines 134-136
            let mut hf = sys::ggml_rms_norm(ctx, h, eps);
            hf = sys::ggml_mul(ctx, hf, l.ffn_norm);

            // ── 2i. SwiGLU: down(silu(gate(x)) * up(x)).
            //     w_gate, w_up: [hidden, intermediate]
            //     w_down:       [intermediate, hidden]
            //
            //     ref: lines 138-147
            let mut g = sys::ggml_mul_mat(ctx, l.w_gate, hf); // [inter, q_len]
            g = sys::ggml_silu(ctx, g);
            let u = sys::ggml_mul_mat(ctx, l.w_up, hf); // [inter, q_len]
            let gu = sys::ggml_mul(ctx, g, u);
            let ffn_out = sys::ggml_mul_mat(ctx, l.w_down, gu); // [hidden, q_len]

            h = sys::ggml_add(ctx, h, ffn_out);
        }

        // ── 3. Final norm. ref: lines 150-153
        let mut out = sys::ggml_rms_norm(ctx, h, eps);
        out = sys::ggml_mul(ctx, out, w.out_norm);
        sys::ggml_set_name(out, b"draft_hidden_out\0".as_ptr().cast());

        let mut og = DraftGraphOutputs {
            hidden_states: out,
            logits: std::ptr::null_mut(),
        };

        // ── 4. Optional: project through target's lm_head to emit
        //     vocab logits. ref: lines 159-164
        if !in_.lm_head.is_null() {
            let logits = sys::ggml_mul_mat(ctx, in_.lm_head, out);
            sys::ggml_set_name(logits, b"draft_logits\0".as_ptr().cast());
            og.logits = logits;
        }
        og
    }
}


// ════════════════════════════════════════════════════════════════
//  DELTA-NET CHUNKED (delta_net_chunked.cpp)
// ════════════════════════════════════════════════════════════════

/// Output of [`build_delta_net_chunked`] — mirrors the reference
/// `DeltaNetChunkedResult` struct.
///
/// ref: `delta_net_chunked.h:9-12`
pub struct DeltaNetChunkedResult {
    /// `[S_v, H_v, n_tokens, n_seqs]`
    pub output: *mut ggml_tensor,
    /// `[S_v, S_v, H_v, n_seqs]`
    pub new_state: *mut ggml_tensor,
}

/// 2-D slice helper — carves out chunk index `c` on the `ne[2]` axis.
///
/// ref: `delta_net_chunked.cpp:24-27`
#[inline]
unsafe fn get_slice_2d(ctx0: *mut ggml_context, t: *mut ggml_tensor, c: i64) -> *mut ggml_tensor {
    let t_ref = &*t;
    sys::ggml_view_4d(
        ctx0,
        t,
        t_ref.ne[0],
        t_ref.ne[1],
        1,
        t_ref.ne[3],
        t_ref.nb[1],
        t_ref.nb[2],
        t_ref.nb[3],
        t_ref.nb[2] * (c as libc::size_t),
    )
}

/// Chain-only, no-capture, no-tree variant. Caller passes q/k/v/g/b/s
/// in the same shape as `ggml_gated_delta_net` expects. Returns the
/// per-token output and the final recurrent state as two separate
/// tensors (unlike the fused kernel which packs them into one dst
/// tensor).
///
/// ref: `delta_net_chunked.cpp:29-235`
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn build_delta_net_chunked(
    ctx0: *mut ggml_context,
    q: *mut ggml_tensor,
    k: *mut ggml_tensor,
    v: *mut ggml_tensor,
    g: *mut ggml_tensor,
    b: *mut ggml_tensor,
    s: *mut ggml_tensor,
) -> DeltaNetChunkedResult {
    unsafe {
        // ref: lines 37-40
        let q_ne = (*q).ne;
        let v_ne = (*v).ne;
        let s_k: i64 = q_ne[0];
        let h_k: i64 = q_ne[1];
        let n_tokens: i64 = q_ne[2];
        let n_seqs: i64 = q_ne[3];

        let s_v: i64 = v_ne[0];
        let h_v: i64 = v_ne[1];

        // GDA only in our port — Qwen3.5 delta-net uses gate scalar
        // per head (`g->ne[0] == 1`). The KDA branch below is kept
        // structurally identical to llama.cpp but never taken in
        // practice. ref: lines 44-47
        let g_ne = (*g).ne;
        let kda = g_ne[0] == s_k && g_ne[1] == h_k;

        debug_assert_eq!(s_k, s_v);
        debug_assert!(h_v % h_k == 0);

        // ref: lines 61-63
        let scale = 1.0_f32 / (s_k as f32).sqrt();
        let q = sys::ggml_scale(ctx0, q, scale);

        // ref: lines 65-69
        let q = sys::ggml_permute(ctx0, q, 0, 2, 1, 3);
        let k = sys::ggml_permute(ctx0, k, 0, 2, 1, 3);
        let v = sys::ggml_permute(ctx0, v, 0, 2, 1, 3);
        let g = sys::ggml_permute(ctx0, g, 0, 2, 1, 3);
        let b = sys::ggml_permute(ctx0, b, 0, 2, 1, 3);

        // ref: lines 71-74
        let cs: i32 = if kda { 16 } else { 64 };
        let n_tokens_i32 = n_tokens as i32;
        let pad: i32 = (cs - n_tokens_i32 % cs) % cs;
        let n_chunks: i32 = (n_tokens_i32 + pad) / cs;

        // ref: lines 76-80
        let q = sys::ggml_pad(ctx0, q, 0, pad, 0, 0);
        let k = sys::ggml_pad(ctx0, k, 0, pad, 0, 0);
        let v = sys::ggml_pad(ctx0, v, 0, pad, 0, 0);
        let g = sys::ggml_pad(ctx0, g, 0, pad, 0, 0);
        let b = sys::ggml_pad(ctx0, b, 0, pad, 0, 0);

        // ref: lines 82-83
        let v_b = sys::ggml_mul(ctx0, v, b);
        let k_b = sys::ggml_mul(ctx0, k, b);

        // ref: lines 85-92
        let q = sys::ggml_reshape_4d(ctx0, q, s_k, cs as i64, n_chunks as i64, h_k * n_seqs);
        let k = sys::ggml_reshape_4d(ctx0, k, s_k, cs as i64, n_chunks as i64, h_k * n_seqs);
        let k_b = sys::ggml_reshape_4d(
            ctx0,
            k_b,
            s_k,
            cs as i64,
            n_chunks as i64,
            h_v * n_seqs,
        );
        let _v = sys::ggml_reshape_4d(ctx0, v, s_v, cs as i64, n_chunks as i64, h_v * n_seqs);
        let v_b = sys::ggml_reshape_4d(
            ctx0,
            v_b,
            s_v,
            cs as i64,
            n_chunks as i64,
            h_v * n_seqs,
        );

        let g_ne0 = (*g).ne[0];
        let g = sys::ggml_reshape_4d(ctx0, g, g_ne0, cs as i64, n_chunks as i64, h_v * n_seqs);
        let _b = sys::ggml_reshape_4d(ctx0, b, 1, cs as i64, n_chunks as i64, h_v * n_seqs);

        // ref: line 94
        let g_t = sys::ggml_transpose(ctx0, g);
        let g_t_cont = sys::ggml_cont(ctx0, g_t);
        let g_cs = sys::ggml_cumsum(ctx0, g_t_cont);

        // ── decay mask + kb/kq tables ────────────────────────────
        //     ref: lines 96-141
        let kb: *mut ggml_tensor;
        let kq: *mut ggml_tensor;
        if kda {
            let chb: i64 = (n_chunks as i64) * h_k * n_seqs;

            let g_cs_i = sys::ggml_reshape_4d(ctx0, g_cs, cs as i64, 1, s_k, chb);
            let mut g_cs_j = sys::ggml_reshape_4d(ctx0, g_cs, 1, cs as i64, s_k, chb);

            g_cs_j = sys::ggml_repeat_4d(ctx0, g_cs_j, cs as i64, cs as i64, s_k, chb);

            let decay_sub = sys::ggml_sub(ctx0, g_cs_j, g_cs_i);
            let decay_tri = sys::ggml_tri(ctx0, decay_sub, GGML_TRI_TYPE_LOWER_DIAG);
            let decay_exp = sys::ggml_exp(ctx0, decay_tri);
            let decay_perm = sys::ggml_permute(ctx0, decay_exp, 2, 1, 0, 3);
            let decay_mask =
                sys::ggml_cont_4d(ctx0, decay_perm, s_k, cs as i64, cs as i64, chb);

            let k_b_i = sys::ggml_reshape_4d(ctx0, k_b, s_k, cs as i64, 1, chb);
            let k_j = sys::ggml_reshape_4d(ctx0, k, s_k, 1, cs as i64, chb);
            let q_i = sys::ggml_reshape_4d(ctx0, q, s_k, cs as i64, 1, chb);

            let decay_k_b_i = sys::ggml_mul(ctx0, decay_mask, k_b_i);
            let decay_q_i = sys::ggml_mul(ctx0, decay_mask, q_i);

            let kb_m = sys::ggml_mul_mat(ctx0, decay_k_b_i, k_j);
            let kq_m = sys::ggml_mul_mat(ctx0, decay_q_i, k_j);

            let kb_r =
                sys::ggml_reshape_4d(ctx0, kb_m, cs as i64, cs as i64, n_chunks as i64, h_v * n_seqs);
            let kq_r =
                sys::ggml_reshape_4d(ctx0, kq_m, cs as i64, cs as i64, n_chunks as i64, h_v * n_seqs);
            let kb_t = sys::ggml_transpose(ctx0, kb_r);
            let kq_t = sys::ggml_transpose(ctx0, kq_r);
            kb = sys::ggml_cont(ctx0, kb_t);
            kq = sys::ggml_cont(ctx0, kq_t);
        } else {
            let g_cs_i = g_cs;
            let mut g_cs_j =
                sys::ggml_reshape_4d(ctx0, g_cs, 1, cs as i64, n_chunks as i64, h_v * n_seqs);

            g_cs_j =
                sys::ggml_repeat_4d(ctx0, g_cs_j, cs as i64, cs as i64, n_chunks as i64, h_v * n_seqs);

            let decay_sub = sys::ggml_sub(ctx0, g_cs_j, g_cs_i);
            let decay_tri = sys::ggml_tri(ctx0, decay_sub, GGML_TRI_TYPE_LOWER_DIAG);
            let decay_mask = sys::ggml_exp(ctx0, decay_tri);

            let kb_mm = sys::ggml_mul_mat(ctx0, k, k_b);
            kb = sys::ggml_mul(ctx0, kb_mm, decay_mask);

            let kq_mm = sys::ggml_mul_mat(ctx0, k, q);
            kq = sys::ggml_mul(ctx0, kq_mm, decay_mask);
        }

        // ref: line 143
        let kq = sys::ggml_tri(ctx0, kq, GGML_TRI_TYPE_LOWER_DIAG);

        // ref: lines 145-158
        let attn0 = sys::ggml_tri(ctx0, kb, GGML_TRI_TYPE_LOWER);
        let identity0 = sys::ggml_view_1d(ctx0, attn0, cs as i64, 0);
        let identity1 = sys::ggml_fill(ctx0, identity0, 1.0);
        let identity = sys::ggml_diag(ctx0, identity1);

        let lhs = sys::ggml_add(ctx0, attn0, identity);
        let attn_neg = sys::ggml_neg(ctx0, attn0);
        let lin_solve = sys::ggml_solve_tri(ctx0, lhs, attn_neg, true, true, false);
        let attn = sys::ggml_add(ctx0, lin_solve, identity);

        // ref: line 160
        let v_b_t = sys::ggml_transpose(ctx0, v_b);
        let v_b_t_cont = sys::ggml_cont(ctx0, v_b_t);
        let mut v = sys::ggml_mul_mat(ctx0, v_b_t_cont, attn);

        // ref: lines 162-168
        let g_exp = sys::ggml_exp(ctx0, g_cs);
        let k_b_t = sys::ggml_transpose(ctx0, k_b);
        let k_b_cont = sys::ggml_cont(ctx0, k_b_t);
        let kbg = sys::ggml_mul(ctx0, k_b_cont, g_exp);
        let k_cd = sys::ggml_mul_mat(ctx0, kbg, attn);

        // ref: lines 170-171
        let g_exp_t0 = sys::ggml_transpose(ctx0, g_exp);
        let g_exp_t = sys::ggml_cont(ctx0, g_exp_t0);
        let q_g_exp = sys::ggml_mul(ctx0, q, g_exp_t);

        // ref: lines 173-181
        let g_cs_ne = (*g_cs).ne;
        let g_cs_nb = (*g_cs).nb;
        let g_cs_type = (*g_cs).type_;
        let g_last0 = sys::ggml_view_4d(
            ctx0,
            g_cs,
            1,
            g_cs_ne[1],
            g_cs_ne[2],
            g_cs_ne[3],
            g_cs_nb[1],
            g_cs_nb[2],
            g_cs_nb[3],
            sys::ggml_row_size(g_cs_type, g_cs_ne[0] - 1),
        );
        let g_last = sys::ggml_cont(ctx0, g_last0);
        let g_last_exp = sys::ggml_exp(ctx0, g_last);
        let g_last_exp_t = sys::ggml_transpose(ctx0, g_last_exp);

        // ref: lines 183-185
        let g_diff_sub = sys::ggml_sub(ctx0, g_cs, g_last);
        let g_diff = sys::ggml_neg(ctx0, g_diff_sub);
        let g_diff_exp = sys::ggml_exp(ctx0, g_diff);
        let g_diff_exp_t0 = sys::ggml_transpose(ctx0, g_diff_exp);
        let g_diff_exp_t = sys::ggml_cont(ctx0, g_diff_exp_t0);

        // ref: lines 187-189
        let kg = sys::ggml_mul(ctx0, k, g_diff_exp_t);
        let kg_t_raw = sys::ggml_transpose(ctx0, kg);
        let kg_t = sys::ggml_cont(ctx0, kg_t_raw);

        // ref: line 191
        let mut s_state = sys::ggml_reshape_4d(ctx0, s, s_v, s_v, 1, h_v * n_seqs);

        // ref: line 193
        let v_t_raw = sys::ggml_transpose(ctx0, v);
        let v_t = sys::ggml_cont(ctx0, v_t_raw);

        // ref: lines 195-220 — main chunk loop
        for chunk in 0..(n_chunks as i64) {
            let ch_k_cd = get_slice_2d(ctx0, k_cd, chunk);
            let ch_v_t = get_slice_2d(ctx0, v_t, chunk);
            let ch_kq = get_slice_2d(ctx0, kq, chunk);
            let ch_q_g_exp = get_slice_2d(ctx0, q_g_exp, chunk);
            let ch_kg_t = get_slice_2d(ctx0, kg_t, chunk);

            let v_t_p = sys::ggml_mul_mat(ctx0, ch_k_cd, s_state);
            let v_t_new = sys::ggml_sub(ctx0, ch_v_t, v_t_p);

            let v_attn = sys::ggml_mul_mat(ctx0, v_t_new, ch_kq);
            let attn_inter = sys::ggml_mul_mat(ctx0, s_state, ch_q_g_exp);
            let o_ch = sys::ggml_add(ctx0, attn_inter, v_attn);

            let v_nb = (*v).nb;
            v = sys::ggml_set_inplace(
                ctx0,
                v,
                o_ch,
                v_nb[1],
                v_nb[2],
                v_nb[3],
                (chunk as libc::size_t) * v_nb[2],
            );

            let kgv = sys::ggml_mul_mat(ctx0, ch_kg_t, v_t_new);
            let ch_g_last_exp_t = get_slice_2d(ctx0, g_last_exp_t, chunk);

            s_state = sys::ggml_mul(ctx0, s_state, ch_g_last_exp_t);
            s_state = sys::ggml_add(ctx0, s_state, kgv);
        }

        // truncate padded tokens back to n_tokens. ref: lines 223-227
        let v_type = (*v).type_;
        let o_view = sys::ggml_view_4d(
            ctx0,
            v,
            s_v,
            n_tokens,
            h_v,
            n_seqs,
            sys::ggml_row_size(v_type, s_v),
            sys::ggml_row_size(v_type, s_v * cs as i64 * n_chunks as i64),
            sys::ggml_row_size(v_type, s_v * cs as i64 * n_chunks as i64 * h_v),
            0,
        );
        // ref: line 228 — [S_v, H_v, n_tokens, n_seqs]
        let o = sys::ggml_permute(ctx0, o_view, 0, 2, 1, 3);

        // ref: line 229
        let s_state = sys::ggml_reshape_4d(ctx0, s_state, s_v, s_v, h_v, n_seqs);

        DeltaNetChunkedResult {
            output: o,
            new_state: s_state,
        }
    }
}
