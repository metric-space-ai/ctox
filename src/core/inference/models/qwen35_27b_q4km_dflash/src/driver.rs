//! Byte-exact port of the driver code in
//! `lucebox/dflash/test/test_dflash.cpp`.
//!
//! ref: `dflash/test/test_dflash.cpp:1-1807`
//!
//! # Scope in this first-pass port
//!
//! The reference driver bundles several decode strategies behind the
//! same CLI (`chain-verify-replay` / `chain-verify-fast-rollback` /
//! `DDTree` / `--seq-verify`). This Rust module ports the
//! **chain-verify legacy replay** path only — the one we've been
//! measuring against in earlier sessions. The DDTree tree-build
//! (`build_tree` + `build_target_step_tree` + `follow_verified_tree`,
//! ~400 lines) and fast-rollback (using captured per-step SSM
//! intermediates to skip the replay forward, ~150 lines) are kept as
//! declared follow-up items. Every skipped section is flagged with a
//! `// FOLLOWUP(...)` comment pointing at the reference line range,
//! mirroring the `// TODO` discipline in the C++ source.
//!
//! # Port structure
//!
//!   * [`StepGraph`]                ← test_dflash.cpp lines 377-409
//!   * [`step_graph_free`]          ← lines 411-427
//!   * [`build_target_step`]        ← lines 501-564
//!   * [`build_draft_step`]         ← lines 637-696
//!   * [`run_dflash_gen_loop`]      ← the chain-only subset of main
//!
//! The DDTree path (`build_tree`, `build_target_step_tree`,
//! `follow_verified_tree`) is absent by design here — it sits in a
//! follow-up crate feature `ddtree` when we revive it.

use std::os::raw::{c_char, c_int};

use crate::ffi as sys;
use sys::{
    ggml_backend_t, ggml_cgraph, ggml_context, ggml_gallocr, ggml_status, ggml_tensor,
    ggml_type,
};

use crate::{
    set_last_error, DFLASH27B_DRAFT_BLOCK_SIZE, DFLASH27B_DRAFT_MASK_TOKEN_ID,
    DFLASH27B_TARGET_HIDDEN, DFLASH27B_TARGET_VOCAB,
};
use crate::model::{
    DeltaNetCapture, QwenGraphInputs, TargetCache, TargetWeights,
};
use crate::graph::{
    build_qwen35_graph, restore_ssm_state, snapshot_ssm_state,
};

/// Draft-side `ctx_len` cap — matches the reference.
pub const DRAFT_CTX_MAX: c_int = 2048;

/// Owned `ggml_context` + `ggml_gallocr` + input tensor handles for
/// one target/draft graph build pass. Corresponds 1:1 to the
/// `StepGraph` struct in the reference driver.
///
/// ref: test_dflash.cpp:377-409
pub struct StepGraph {
    pub ctx: *mut ggml_context,
    pub alloc: *mut ggml_gallocr,
    pub gf: *mut ggml_cgraph,

    // Per-call input tensors (owned by `ctx`, freed when `ctx` is).
    pub inp_embed: *mut ggml_tensor,
    pub positions: *mut ggml_tensor,
    pub positions_k: *mut ggml_tensor, // draft only
    pub attn_mask: *mut ggml_tensor,
    pub parent_ids: *mut ggml_tensor,  // DDTree only (unused in this subset)
    pub target_hidden_cat: *mut ggml_tensor, // draft only

    // Graph-resident outputs pointing into the compute buffer after
    // `ggml_backend_graph_compute` runs.
    pub logits: *mut ggml_tensor,
    pub delta_captures: Vec<DeltaNetCapture>,
}

impl Default for StepGraph {
    fn default() -> Self {
        Self {
            ctx: std::ptr::null_mut(),
            alloc: std::ptr::null_mut(),
            gf: std::ptr::null_mut(),
            inp_embed: std::ptr::null_mut(),
            positions: std::ptr::null_mut(),
            positions_k: std::ptr::null_mut(),
            attn_mask: std::ptr::null_mut(),
            parent_ids: std::ptr::null_mut(),
            target_hidden_cat: std::ptr::null_mut(),
            logits: std::ptr::null_mut(),
            delta_captures: Vec::new(),
        }
    }
}

/// Tear down all state held by the previous graph build. Keeps
/// `sg.alloc` alive across calls — the allocator is reused to amortize
/// the setup cost.
///
/// ref: test_dflash.cpp:411-427
pub fn step_graph_free(sg: &mut StepGraph) {
    unsafe {
        if !sg.ctx.is_null() {
            sys::ggml_free(sg.ctx);
            sg.ctx = std::ptr::null_mut();
        }
    }
    sg.gf = std::ptr::null_mut();
    sg.inp_embed = std::ptr::null_mut();
    sg.positions = std::ptr::null_mut();
    sg.positions_k = std::ptr::null_mut();
    sg.attn_mask = std::ptr::null_mut();
    sg.parent_ids = std::ptr::null_mut();
    sg.target_hidden_cat = std::ptr::null_mut();
    sg.logits = std::ptr::null_mut();
    sg.delta_captures.clear();
}

impl Drop for StepGraph {
    fn drop(&mut self) {
        step_graph_free(self);
        unsafe {
            if !self.alloc.is_null() {
                sys::ggml_gallocr_free(self.alloc);
                self.alloc = std::ptr::null_mut();
            }
        }
    }
}

// KQ-mask alignment. The reference uses a runtime `g_kq_stride_pad`
// that's set based on the FA kernel kind (TBQ=256, others=1). We port
// only the f16/Q* common path here which is stride 1. Caller-code
// that wants TBQ can flip this behind a feature flag later.
const KQ_MASK_PAD: c_int = 64; // from fattn-common.cuh
const G_KQ_STRIDE_PAD: c_int = 1;

// Static C-strings for tensor names. CString::new allocates each call;
// the reference uses string literals so we match that by keeping them
// as `b"...\0"` byte slices indexed by `.as_ptr()`.
const NAME_INP_EMBED: &[u8] = b"inp_embed\0";
const NAME_POSITIONS: &[u8] = b"positions\0";
const NAME_ATTN_MASK: &[u8] = b"attn_mask\0";
const NAME_PARENT_IDS: &[u8] = b"parent_ids\0";
const NAME_TARGET_HIDDEN_CAT: &[u8] = b"target_hidden_cat\0";
const NAME_POSITIONS_Q: &[u8] = b"positions_q\0";
const NAME_POSITIONS_K: &[u8] = b"positions_k\0";

#[inline]
fn align_up(n: c_int, k: c_int) -> c_int {
    ((n + k - 1) / k) * k
}

// ─── build_target_step ───────────────────────────────────────────
//
// ref: test_dflash.cpp:501-564

/// Build one target forward graph. Reuses `sg.alloc` across calls.
/// After return, the caller pushes inputs via
/// `ggml_backend_tensor_set(sg.inp_embed, ...)` etc. and runs
/// `ggml_backend_graph_compute(backend, sg.gf)`.
///
/// Returns `false` on any allocation / graph-build failure and sets
/// `errors::last_error`.
///
/// ref: test_dflash.cpp:501-564
#[allow(clippy::too_many_arguments)]
pub fn build_target_step(
    sg: &mut StepGraph,
    w: &TargetWeights,
    cache: &mut TargetCache,
    backend: ggml_backend_t,
    kv_start: c_int,
    n_tokens: c_int,
    with_mask: bool,
    capture: bool,
    capture_delta_intermediate: bool,
) -> bool {
    step_graph_free(sg);

    unsafe {
        let ip = sys::ggml_init_params {
            // ctx arena holds tensor *descriptors* only (no_alloc=true).
            // 512 MB covers target graph even with
            // capture_delta_intermediate enabled.
            mem_size: 512 * 1024 * 1024,
            mem_buffer: std::ptr::null_mut(),
            no_alloc: true,
        };
        sg.ctx = sys::ggml_init(ip);
        if sg.ctx.is_null() {
            set_last_error("build_target_step: ggml_init failed");
            return false;
        }

        let hidden = DFLASH27B_TARGET_HIDDEN;
        sg.inp_embed = sys::ggml_new_tensor_3d(
            sg.ctx,
            ggml_type::GGML_TYPE_F32,
            hidden as i64,
            n_tokens as i64,
            1,
        );
        sys::ggml_set_name(sg.inp_embed, NAME_INP_EMBED.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.inp_embed);

        sg.positions = sys::ggml_new_tensor_1d(
            sg.ctx,
            ggml_type::GGML_TYPE_I32,
            (4 * n_tokens) as i64,
        );
        sys::ggml_set_name(sg.positions, NAME_POSITIONS.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.positions);

        if with_mask {
            let kv_len = kv_start + n_tokens;
            let kv_pad = align_up(kv_len, G_KQ_STRIDE_PAD);
            let q_pad = align_up(n_tokens, KQ_MASK_PAD);
            sg.attn_mask = sys::ggml_new_tensor_2d(
                sg.ctx,
                ggml_type::GGML_TYPE_F16,
                kv_pad as i64,
                q_pad as i64,
            );
            sys::ggml_set_name(sg.attn_mask, NAME_ATTN_MASK.as_ptr() as *const c_char);
            sys::ggml_set_input(sg.attn_mask);
        }

        sg.gf = sys::ggml_new_graph_custom(sg.ctx, 16384, false);

        let gi = QwenGraphInputs {
            inp_embed: sg.inp_embed,
            positions: sg.positions,
            attn_mask: sg.attn_mask,
            n_tokens,
            kv_start,
            capture_layers: capture,
            capture_delta_intermediate,
            parent_ids: std::ptr::null_mut(),
        };

        let go = build_qwen35_graph(sg.ctx, sg.gf, w, cache, &gi);
        if go.logits.is_null() {
            set_last_error("build_target_step: build_qwen35_graph returned null logits");
            return false;
        }
        sg.logits = go.logits;
        sg.delta_captures = go.delta_captures;
        sys::ggml_set_output(sg.logits);
        sys::ggml_build_forward_expand(sg.gf, sg.logits);

        if sg.alloc.is_null() {
            sg.alloc =
                sys::ggml_gallocr_new(sys::ggml_backend_get_default_buffer_type(backend));
            if sg.alloc.is_null() {
                set_last_error("build_target_step: ggml_gallocr_new failed");
                return false;
            }
        }
        sys::ggml_gallocr_alloc_graph(sg.alloc, sg.gf)
    }
}

// ─── build_target_step_tree ──────────────────────────────────────
//
// ref: test_dflash.cpp:574-635

/// DDTree-variant of [`build_target_step`]. Same shape except:
///   * `n_tokens = 1 + tree.n_nodes` (root + flat DFS tree nodes)
///   * `with_mask` is always true; caller fills ancestor-only mask
///     via `build_tree_mask()`.
///   * Adds a fresh `parent_ids[n_tokens]` i32 input tensor wired
///     into `QwenGraphInputs` so `build_delta_net_block` can call
///     `ggml_gated_delta_net_tree` (kernel handles DeltaNet/SSM
///     tree recurrence via parent_ids).
///   * `capture_layers=true`, `capture_delta_intermediate=true` —
///     the spec loop relies on per-step intermediates for rollback.
///
/// ref: test_dflash.cpp:574-635
pub fn build_target_step_tree(
    sg: &mut StepGraph,
    w: &TargetWeights,
    cache: &mut TargetCache,
    backend: ggml_backend_t,
    kv_start: c_int,
    n_tokens: c_int,
) -> bool {
    step_graph_free(sg);

    unsafe {
        let ip = sys::ggml_init_params {
            mem_size: 512 * 1024 * 1024,
            mem_buffer: std::ptr::null_mut(),
            no_alloc: true,
        };
        sg.ctx = sys::ggml_init(ip);
        if sg.ctx.is_null() {
            set_last_error("build_target_step_tree: ggml_init failed");
            return false;
        }

        let hidden = DFLASH27B_TARGET_HIDDEN;
        sg.inp_embed = sys::ggml_new_tensor_3d(
            sg.ctx,
            ggml_type::GGML_TYPE_F32,
            hidden as i64,
            n_tokens as i64,
            1,
        );
        sys::ggml_set_name(sg.inp_embed, NAME_INP_EMBED.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.inp_embed);

        sg.positions = sys::ggml_new_tensor_1d(
            sg.ctx,
            ggml_type::GGML_TYPE_I32,
            (4 * n_tokens) as i64,
        );
        sys::ggml_set_name(sg.positions, NAME_POSITIONS.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.positions);

        let kv_len = kv_start + n_tokens;
        let kv_pad = align_up(kv_len, G_KQ_STRIDE_PAD);
        let q_pad = align_up(n_tokens, KQ_MASK_PAD);
        sg.attn_mask = sys::ggml_new_tensor_2d(
            sg.ctx,
            ggml_type::GGML_TYPE_F16,
            kv_pad as i64,
            q_pad as i64,
        );
        sys::ggml_set_name(sg.attn_mask, NAME_ATTN_MASK.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.attn_mask);

        // parent_ids[n_tokens] i32 — tree-mode DeltaNet input.
        //   -1    : reload from pre-block state (root, t==0)
        //    k    : reload from intermediate[k]   (tree siblings)
        //    t-1  : sequential                    (hot path)
        sg.parent_ids = sys::ggml_new_tensor_1d(
            sg.ctx,
            ggml_type::GGML_TYPE_I32,
            n_tokens as i64,
        );
        sys::ggml_set_name(sg.parent_ids, NAME_PARENT_IDS.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.parent_ids);

        sg.gf = sys::ggml_new_graph_custom(sg.ctx, 16384, false);

        let gi = QwenGraphInputs {
            inp_embed: sg.inp_embed,
            positions: sg.positions,
            attn_mask: sg.attn_mask,
            n_tokens,
            kv_start,
            capture_layers: true,
            capture_delta_intermediate: true,
            parent_ids: sg.parent_ids,
        };

        let go = build_qwen35_graph(sg.ctx, sg.gf, w, cache, &gi);
        if go.logits.is_null() {
            set_last_error("build_target_step_tree: null logits");
            return false;
        }
        sg.logits = go.logits;
        sg.delta_captures = go.delta_captures;
        sys::ggml_set_output(sg.logits);
        sys::ggml_build_forward_expand(sg.gf, sg.logits);

        if sg.alloc.is_null() {
            sg.alloc =
                sys::ggml_gallocr_new(sys::ggml_backend_get_default_buffer_type(backend));
            if sg.alloc.is_null() {
                set_last_error("build_target_step_tree: gallocr_new failed");
                return false;
            }
        }
        sys::ggml_gallocr_alloc_graph(sg.alloc, sg.gf)
    }
}

// ─── build_draft_step ────────────────────────────────────────────
//
// ref: test_dflash.cpp:637-696

/// Build one draft forward graph. `ctx_len` is the number of committed
/// target positions the draft cross-attends over (already <= DRAFT_CTX_MAX).
///
/// ref: test_dflash.cpp:637-696
pub fn build_draft_step(
    sg: &mut StepGraph,
    dw: &crate::model::DraftWeights,
    tw: &TargetWeights,
    backend: ggml_backend_t,
    ctx_len: c_int,
) -> bool {
    use crate::graph::{build_draft_graph, DraftGraphInputs};
    use crate::DFLASH27B_DRAFT_N_TARGET_LAYERS;

    step_graph_free(sg);

    unsafe {
        let ip = sys::ggml_init_params {
            mem_size: 256 * 1024 * 1024,
            mem_buffer: std::ptr::null_mut(),
            no_alloc: true,
        };
        sg.ctx = sys::ggml_init(ip);
        if sg.ctx.is_null() {
            set_last_error("build_draft_step: ggml_init failed");
            return false;
        }

        let hidden = DFLASH27B_TARGET_HIDDEN;
        let q_len = DFLASH27B_DRAFT_BLOCK_SIZE;
        let total_k = ctx_len + q_len;
        let fc_in = DFLASH27B_DRAFT_N_TARGET_LAYERS * hidden;

        // Inputs: pre-embedded noise block + target-hidden-cat + two
        // position tensors.
        sg.inp_embed = sys::ggml_new_tensor_3d(
            sg.ctx,
            ggml_type::GGML_TYPE_F32,
            hidden as i64,
            q_len as i64,
            1,
        );
        sys::ggml_set_name(sg.inp_embed, NAME_INP_EMBED.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.inp_embed);

        sg.target_hidden_cat = sys::ggml_new_tensor_3d(
            sg.ctx,
            ggml_type::GGML_TYPE_F32,
            fc_in as i64,
            ctx_len as i64,
            1,
        );
        sys::ggml_set_name(
            sg.target_hidden_cat,
            NAME_TARGET_HIDDEN_CAT.as_ptr() as *const c_char,
        );
        sys::ggml_set_input(sg.target_hidden_cat);

        sg.positions = sys::ggml_new_tensor_1d(
            sg.ctx,
            ggml_type::GGML_TYPE_I32,
            q_len as i64,
        );
        sys::ggml_set_name(sg.positions, NAME_POSITIONS_Q.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.positions);

        sg.positions_k = sys::ggml_new_tensor_1d(
            sg.ctx,
            ggml_type::GGML_TYPE_I32,
            total_k as i64,
        );
        sys::ggml_set_name(sg.positions_k, NAME_POSITIONS_K.as_ptr() as *const c_char);
        sys::ggml_set_input(sg.positions_k);

        sg.gf = sys::ggml_new_graph_custom(sg.ctx, 4096, false);

        let di = DraftGraphInputs {
            ctx_len,
            noise_embed: sg.inp_embed,
            target_hidden_cat: sg.target_hidden_cat,
            positions_q: sg.positions,
            positions_k: sg.positions_k,
            lm_head: tw.output, // share target lm_head
        };

        let og = build_draft_graph(sg.ctx, dw, &di);
        if og.logits.is_null() {
            set_last_error("build_draft_step: build_draft_graph returned null logits");
            return false;
        }
        sg.logits = og.logits;
        sys::ggml_set_output(sg.logits);
        sys::ggml_build_forward_expand(sg.gf, sg.logits);

        if sg.alloc.is_null() {
            sg.alloc =
                sys::ggml_gallocr_new(sys::ggml_backend_get_default_buffer_type(backend));
            if sg.alloc.is_null() {
                set_last_error("build_draft_step: ggml_gallocr_new failed");
                return false;
            }
        }
        sys::ggml_gallocr_alloc_graph(sg.alloc, sg.gf)
    }
}

// ─── Helpers (ported from test_dflash.cpp top-of-file) ──────────

/// ggml f16 encoding of -inf (attention mask pad value).
///
/// ref: test_dflash.cpp top (constant `F16_NEG_INF`)
const F16_NEG_INF: u16 = 0xFC00;
/// ggml f16 encoding of +0.0f (attention mask attendable value).
const F16_ZERO: u16 = 0x0000;

/// Host-side argmax over a `vocab`-slice.
///
/// ref: `test_dflash.cpp::argmax_f32` (top of file)
fn argmax_f32(row: &[f32]) -> i32 {
    let mut best_idx: i32 = 0;
    let mut best_val: f32 = f32::NEG_INFINITY;
    for (i, &v) in row.iter().enumerate() {
        if v > best_val {
            best_val = v;
            best_idx = i as i32;
        }
    }
    best_idx
}

/// Build an f16 causal mask of shape `[kv_len_padded, n_tokens]`
/// (column-major — positions `[q, k]` at index `q*kv_pad + k`).
///
/// ref: `test_dflash.cpp::build_causal_mask` (top of file)
fn build_causal_mask(
    buf: &mut Vec<u16>,
    kv_len: c_int,
    n_tokens: c_int,
    kv_start: c_int,
) {
    let kv_pad = align_up(kv_len, G_KQ_STRIDE_PAD);
    let q_pad = align_up(n_tokens, KQ_MASK_PAD);
    buf.clear();
    buf.resize((kv_pad as usize) * (q_pad as usize), F16_NEG_INF);
    // Row q attends to keys [0, kv_start + q].
    for q in 0..n_tokens as usize {
        let up_to = kv_start as usize + q + 1; // inclusive upper bound
        for k in 0..up_to {
            buf[q * (kv_pad as usize) + k] = F16_ZERO;
        }
    }
}

// ─── run_dflash_gen_loop ─────────────────────────────────────────
//
// ref: test_dflash.cpp:870-1758 (the non-DDTree, non-fast-rollback,
// batched-verify subset of main)

/// Stats returned by [`run_dflash_gen_loop`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RunStats {
    pub n_generated: c_int,
    pub n_draft_steps: c_int,
    pub n_accept_sum: c_int,
    pub wall_s: f64,
    pub prefill_s: f64,
    pub decode_tok_s: f64,
}

/// Run-mode switch for [`run_dflash_gen_loop`] — mirrors the CLI
/// flags on the reference `test_dflash` binary.
///
/// ref: test_dflash.cpp:729-761 (argv parsing)
#[derive(Debug, Clone, Copy)]
pub struct GenConfig {
    /// `--fast-rollback`: skip the snapshot + replay-forward pair,
    /// roll back SSM + conv state from the per-step intermediates
    /// captured during verify. ref: test_dflash.cpp:719-722 + 1603-1694
    pub fast_rollback: bool,
    /// `--ddtree`: tree-structured verify on top of fast-rollback.
    /// When true, `fast_rollback` is implicitly forced on (matches
    /// reference behavior at test_dflash.cpp:740).
    /// ref: test_dflash.cpp:723-728 + 1164-1462
    pub ddtree: bool,
    /// `--ddtree-budget` — max non-root tree nodes. Default 64, the
    /// README's peak-config uses 22.
    pub ddtree_budget: i32,
    /// `--ddtree-temp` — softmax temperature for the top-K extract.
    /// `<1` sharpens; compensates for Q4_K_M flat-softmax.
    pub ddtree_temp: f32,
    /// `--ddtree-no-chain-seed` flips this to false. Pre-seeds the
    /// top-1 chain into the tree builder (defensive default, matches
    /// reference).
    pub ddtree_chain_seed: bool,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self {
            fast_rollback: false,
            ddtree: false,
            ddtree_budget: 64,
            ddtree_temp: 1.0,
            ddtree_chain_seed: true,
        }
    }
}

/// Run prefill + chain-verify spec-decode on the given target/draft
/// models. Emits the full token stream (prompt + `n_gen` generated)
/// into `out_all` and returns aggregate stats.
///
/// Modes supported:
///   * chain-verify + legacy-replay   (cfg.fast_rollback == false)
///   * chain-verify + fast-rollback   (cfg.fast_rollback == true)
///
/// The reference's `--seq-verify` and `--ddtree` are separate
/// functions (not yet ported for the latter). Prefill ubatch
/// defaults to 16 (short-prompt-friendly) and honors
/// `DFLASH27B_PREFILL_UBATCH` env override exactly like the reference.
///
/// Returns `Ok(stats)` on success; `Err` with an error message on any
/// failure (also pushes the message into [`crate::last_error`]).
///
/// ref: test_dflash.cpp:870-1758
#[allow(clippy::too_many_arguments)]
pub fn run_dflash_gen_loop(
    w: &TargetWeights,
    dw: &crate::model::DraftWeights,
    cache: &mut TargetCache,
    backend: ggml_backend_t,
    prompt_ids: &[i32],
    n_gen: c_int,
    out_all: &mut Vec<i32>,
    cfg: GenConfig,
) -> Result<RunStats, String> {
    let _ = DFLASH27B_DRAFT_MASK_TOKEN_ID; // used below via `mask_tok`
    let _ = DFLASH27B_TARGET_VOCAB;

    // ref: test_dflash.cpp:876-879
    let q_len = DFLASH27B_DRAFT_BLOCK_SIZE;
    let hidden = DFLASH27B_TARGET_HIDDEN;
    let vocab = DFLASH27B_TARGET_VOCAB;
    let mask_tok = DFLASH27B_DRAFT_MASK_TOKEN_ID;

    let prompt_len: c_int = prompt_ids.len() as c_int;
    if prompt_len == 0 {
        return fail("empty prompt");
    }
    // ref: test_dflash.cpp:881-884
    if prompt_len + n_gen + q_len > cache.max_ctx {
        return fail(&format!(
            "prompt+gen+block exceeds max_ctx: {} + {} + {} > {}",
            prompt_len, n_gen, q_len, cache.max_ctx
        ));
    }

    // DDTree implicitly turns on fast-rollback (ref: test_dflash.cpp:740).
    let fast_rollback = cfg.fast_rollback || cfg.ddtree;

    *out_all = prompt_ids.to_vec();
    let mut committed: c_int = 0;
    let mut last_tok: i32 = -1;

    let mut sg = StepGraph::default();
    let mut pf_mask_buf: Vec<u16> = Vec::new();

    // Prefill ubatch selection. ref: test_dflash.cpp:906-912
    let prefill_ubatch: c_int = std::env::var("DFLASH27B_PREFILL_UBATCH")
        .ok()
        .and_then(|s| s.parse::<c_int>().ok())
        .map(|v| v.max(1))
        .unwrap_or(if prompt_len > 2048 { 192 } else { 16 });

    let t_pf0 = std::time::Instant::now();

    // Prefill hot-path scratch (allocate once, reuse per chunk).
    // ref: test_dflash.cpp:914-917
    let mut pf_embed_buf: Vec<f32> = Vec::new();
    let mut pf_pos_buf: Vec<i32> = Vec::new();
    let mut pf_logits_buf: Vec<f32> = vec![0.0_f32; vocab as usize];

    // ─── Prefill loop. ref: test_dflash.cpp:919-971 ─────────────
    let mut start: c_int = 0;
    while start < prompt_len {
        let n_tokens = std::cmp::min(prefill_ubatch, prompt_len - start);
        let kv_len = start + n_tokens;
        // ref: line 926
        let pf_with_mask = (G_KQ_STRIDE_PAD > KQ_MASK_PAD) || (n_tokens > 1);

        if !build_target_step(
            &mut sg,
            w,
            cache,
            backend,
            start,
            n_tokens,
            pf_with_mask,
            /*capture=*/ true,
            /*capture_delta_intermediate=*/ false,
        ) {
            return fail(&format!(
                "prefill build @{start}: {}",
                crate::last_error()
            ));
        }

        // CPU-side token embedding dequant (bf16 → f32 for the graph).
        // ref: line 933-934
        let pf_embed_elems = (hidden as usize) * (n_tokens as usize);
        pf_embed_buf.resize(pf_embed_elems, 0.0_f32);
        if !w.embedder.embed(
            &prompt_ids[start as usize..(start + n_tokens) as usize],
            &mut pf_embed_buf[..pf_embed_elems],
        ) {
            return fail("prefill: embedder failed");
        }
        unsafe {
            sys::ggml_backend_tensor_set(
                sg.inp_embed,
                pf_embed_buf.as_ptr() as *const libc::c_void,
                0,
                (pf_embed_elems * std::mem::size_of::<f32>()) as libc::size_t,
            );
        }

        // M-RoPE 4D text layout — axis0..axis2 = absolute position, axis3 = 0.
        // ref: lines 941-950
        let pf_pos_elems = 4 * (n_tokens as usize);
        pf_pos_buf.resize(pf_pos_elems, 0);
        for i in 0..n_tokens as usize {
            let p = start + (i as c_int);
            pf_pos_buf[0 * (n_tokens as usize) + i] = p;
            pf_pos_buf[1 * (n_tokens as usize) + i] = p;
            pf_pos_buf[2 * (n_tokens as usize) + i] = p;
            pf_pos_buf[3 * (n_tokens as usize) + i] = 0;
        }
        unsafe {
            sys::ggml_backend_tensor_set(
                sg.positions,
                pf_pos_buf.as_ptr() as *const libc::c_void,
                0,
                (pf_pos_elems * std::mem::size_of::<i32>()) as libc::size_t,
            );
        }

        // ref: lines 955-959
        if pf_with_mask {
            build_causal_mask(&mut pf_mask_buf, kv_len, n_tokens, /*kv_start=*/ start);
            unsafe {
                sys::ggml_backend_tensor_set(
                    sg.attn_mask,
                    pf_mask_buf.as_ptr() as *const libc::c_void,
                    0,
                    (pf_mask_buf.len() * std::mem::size_of::<u16>()) as libc::size_t,
                );
            }
        }

        // ref: lines 961-962
        let st = unsafe { sys::ggml_backend_graph_compute(backend, sg.gf) };
        if st != ggml_status::GGML_STATUS_SUCCESS {
            return fail(&format!(
                "prefill compute @{start}: status {:?}",
                st
            ));
        }

        // ref: lines 964-969
        let last_row_off: libc::size_t =
            (n_tokens as libc::size_t - 1) * (vocab as libc::size_t) * std::mem::size_of::<f32>();
        unsafe {
            sys::ggml_backend_tensor_get(
                sg.logits,
                pf_logits_buf.as_mut_ptr() as *mut libc::c_void,
                last_row_off,
                (vocab as libc::size_t) * std::mem::size_of::<f32>() as libc::size_t,
            );
        }
        last_tok = argmax_f32(&pf_logits_buf);
        committed = start + n_tokens;

        start += n_tokens;
    }

    let t_pf1 = std::time::Instant::now();
    let prefill_s = t_pf1.duration_since(t_pf0).as_secs_f64();
    // (prefill timing available in the returned RunStats; no hot-path
    // tracing call to avoid format-arg materialization cost)

    // ─── DFlash decode loop. ref: test_dflash.cpp:979-1757 ───────
    //
    // All per-step scratch Vecs are allocated here ONCE — the decode
    // loop reuses their capacity via `resize` / `fill` / direct index
    // assignment. Matches the C++ reference's pattern of declaring
    // these vectors above the `while` and reusing storage across
    // iterations. Allocating + zeroing a fresh 16 MB logit buffer
    // every step otherwise costs ~18 % wall-clock at DDTree budget=22.
    let mut n_draft_steps: c_int = 0;
    let mut n_accept_sum: c_int = 0;
    let mut n_generated: c_int = 0;

    // Hot-path scratch (size FIXED over decode). ref: lines 980-989
    let mut noise_ids: Vec<i32> = vec![0; q_len as usize];
    let mut draft_tok: Vec<i32> = vec![0; q_len as usize];
    let mut target_tok: Vec<i32> = vec![0; q_len as usize];
    let mut mask_buf: Vec<u16> = Vec::new();
    let mut pos_q_buf: Vec<i32> = vec![0; q_len as usize];
    let mut pos_k_buf: Vec<i32> = vec![0; (cache.max_ctx + q_len) as usize];
    let mut pos4_buf: Vec<i32> = vec![0; 4 * (q_len as usize)];
    let mut noise_embed_buf = vec![0.0_f32; (hidden as usize) * (q_len as usize)];

    // Hot-path scratch for verify / replay / DDTree (size depends on
    // mode; `verify_max_tokens` covers worst case). The DDTree path
    // needs `1 + budget` flat slots while chain-verify needs `q_len`.
    let verify_max_tokens: c_int = if cfg.ddtree {
        std::cmp::max(q_len, cfg.ddtree_budget + 1)
    } else {
        q_len
    };
    let mut draft_logits_buf: Vec<f32> =
        vec![0.0_f32; (vocab as usize) * (q_len as usize)];
    let mut verify_logits_buf: Vec<f32> =
        vec![0.0_f32; (vocab as usize) * (verify_max_tokens as usize)];
    // verify_embed is reused for chain-verify AND replay AND DDTree
    // flat-token embed. Worst-case size = hidden * verify_max_tokens.
    let mut verify_embed: Vec<f32> =
        vec![0.0_f32; (hidden as usize) * (verify_max_tokens as usize)];
    let mut replay_pos: Vec<i32> = vec![0; 4 * (q_len as usize)];
    let mut last_logits: Vec<f32> = vec![0.0_f32; vocab as usize];
    // DDTree-only scratch — sizes based on `ddtree_budget`.
    let mut flat_tokens: Vec<i32> = Vec::with_capacity(verify_max_tokens as usize);
    let mut posterior: Vec<i32> = vec![0; verify_max_tokens as usize];
    let mut tree_pos4: Vec<i32> = vec![0; 4 * (verify_max_tokens as usize)];
    let mut tree_parent_ids: Vec<i32> = vec![0; verify_max_tokens as usize];
    // top-K scratch for extract_draft_topk (K ∈ {1, 8}; worst-case L*8).
    let ddtree_k: usize = if cfg.ddtree && cfg.ddtree_budget > (q_len - 1) {
        8
    } else {
        1
    };
    let mut top_logp: Vec<f32> = vec![0.0_f32; ((q_len - 1) as usize) * ddtree_k];
    let mut top_ids: Vec<i32> = vec![0_i32; ((q_len - 1) as usize) * ddtree_k];

    let t_gen0 = std::time::Instant::now();

    while n_generated < n_gen {
        let need_commit_budget = n_gen - n_generated;

        // 1) Noise block [last_tok, MASK*15]. ref: lines 1011-1014
        noise_ids[0] = last_tok;
        for i in 1..q_len as usize {
            noise_ids[i] = mask_tok;
        }
        if !w.embedder.embed(&noise_ids, &mut noise_embed_buf) {
            return fail("embedder failed for noise block");
        }

        // Draft target-attention window. ref: lines 1016-1024
        let draft_ctx = std::cmp::min(committed, DRAFT_CTX_MAX);
        let draft_start = committed - draft_ctx;

        // 2) Build + run the draft forward. ref: lines 1026-1073
        if !build_draft_step(&mut sg, dw, w, backend, draft_ctx) {
            return fail(&format!(
                "draft build failed: {}",
                crate::last_error()
            ));
        }

        unsafe {
            sys::ggml_backend_tensor_set(
                sg.inp_embed,
                noise_embed_buf.as_ptr() as *const libc::c_void,
                0,
                (noise_embed_buf.len() * std::mem::size_of::<f32>()) as libc::size_t,
            );
        }

        // target_hidden_cat widen: device→device bf16→f32 copy from the
        // ring buffer. Use the vendored f16_convert kernel (same as
        // reference). ref: lines 1041-1059
        {
            let fc_in: libc::size_t = 5 * (hidden as libc::size_t);
            let cap_len = cache.target_feat_cap;
            let elt_feat = unsafe { sys::ggml_element_size(cache.target_feat) };
            let slot0 = draft_start % cap_len;
            let pre_n = std::cmp::min(draft_ctx, cap_len - slot0);
            let post_n = draft_ctx - pre_n;

            unsafe {
                let tf_data = (*cache.target_feat).data as *const u8;
                let thc_data = (*sg.target_hidden_cat).data as *mut u8;
                // Pre-slice.
                sys::dflash27b_launch_bf16_to_f32(
                    tf_data.add((slot0 as libc::size_t) * elt_feat * fc_in)
                        as *const libc::c_void,
                    thc_data as *mut libc::c_void,
                    (pre_n as libc::size_t) * fc_in,
                    std::ptr::null_mut(),
                );
                if post_n > 0 {
                    sys::dflash27b_launch_bf16_to_f32(
                        tf_data as *const libc::c_void,
                        thc_data.add(
                            (pre_n as libc::size_t) * fc_in * std::mem::size_of::<f32>(),
                        ) as *mut libc::c_void,
                        (post_n as libc::size_t) * fc_in,
                        std::ptr::null_mut(),
                    );
                }
            }
        }

        // Positions. ref: lines 1063-1066
        for i in 0..q_len as usize {
            pos_q_buf[i] = draft_ctx + (i as c_int);
        }
        for i in 0..(draft_ctx + q_len) as usize {
            pos_k_buf[i] = i as i32;
        }
        unsafe {
            sys::ggml_backend_tensor_set(
                sg.positions,
                pos_q_buf.as_ptr() as *const libc::c_void,
                0,
                (q_len as libc::size_t) * std::mem::size_of::<i32>() as libc::size_t,
            );
            sys::ggml_backend_tensor_set(
                sg.positions_k,
                pos_k_buf.as_ptr() as *const libc::c_void,
                0,
                ((draft_ctx + q_len) as libc::size_t) * std::mem::size_of::<i32>() as libc::size_t,
            );
        }

        // Compute draft forward. ref: line 1070
        let st = unsafe { sys::ggml_backend_graph_compute(backend, sg.gf) };
        if st != ggml_status::GGML_STATUS_SUCCESS {
            return fail(&format!("draft compute: status {:?}", st));
        }

        // Download draft logits and argmax per position. ref: lines 1075-1083
        unsafe {
            sys::ggml_backend_tensor_get(
                sg.logits,
                draft_logits_buf.as_mut_ptr() as *mut libc::c_void,
                0,
                ((vocab * q_len) as libc::size_t) * std::mem::size_of::<f32>() as libc::size_t,
            );
        }
        for i in 0..q_len as usize {
            let slice = &draft_logits_buf[i * (vocab as usize)..(i + 1) * (vocab as usize)];
            draft_tok[i] = argmax_f32(slice);
        }
        // Pin draft_tok[0] = last_tok so verify + replay see the
        // correct prefix. ref: line 1083
        draft_tok[0] = last_tok;

        // 3) Snapshot SSM + conv state — only needed for legacy
        //    replay. fast_rollback / ddtree both use the per-step
        //    intermediates captured during verify.
        //
        //    ref: test_dflash.cpp:1120-1125
        if !fast_rollback {
            snapshot_ssm_state(cache);
        }

        // ── DDTree path: tree-build + tree-verify + walk + rollback.
        //     ref: test_dflash.cpp:1164-1462
        if cfg.ddtree {
            // Top-K extract from draft logits (skipping slot 0 which
            // is the fixed root/last_tok). ref: lines 1085-1116
            let ll = (q_len - 1) as usize;
            if ddtree_k == 1 {
                for i in 0..ll {
                    top_logp[i] = 0.0;
                    top_ids[i] = draft_tok[i + 1];
                }
            } else {
                // Skip the leading vocab slice (position 0 = root) to
                // match the reference pointer offset at line 1110.
                let logits_tail = &draft_logits_buf[(vocab as usize)..];
                crate::ddtree::extract_draft_topk(
                    logits_tail,
                    ll,
                    vocab as usize,
                    ddtree_k,
                    &mut top_logp,
                    &mut top_ids,
                    cfg.ddtree_temp,
                );
            }

            // Build flat tree. ref: lines 1166-1171
            let tree = crate::ddtree::build_ddtree(
                &top_logp,
                &top_ids,
                ll as i32,
                ddtree_k as i32,
                cfg.ddtree_budget,
                cfg.ddtree_chain_seed,
            );
            let n_nodes = tree.n_nodes;
            let n_flat: c_int = 1 + n_nodes; // slot 0 = root

            if !build_target_step_tree(&mut sg, w, cache, backend, committed, n_flat) {
                return fail(&format!(
                    "ddtree verify build: {}",
                    crate::last_error()
                ));
            }

            // Flat embed sequence: [last_tok, tree.token_ids...].
            // Reuse pre-allocated `flat_tokens` + `verify_embed`.
            // ref: lines 1181-1189
            flat_tokens.clear();
            flat_tokens.push(last_tok);
            flat_tokens.extend_from_slice(&tree.token_ids);

            let embed_elems = (hidden as usize) * (n_flat as usize);
            if !w
                .embedder
                .embed(&flat_tokens, &mut verify_embed[..embed_elems])
            {
                return fail("ddtree: embedder failed");
            }
            unsafe {
                sys::ggml_backend_tensor_set(
                    sg.inp_embed,
                    verify_embed.as_ptr() as *const libc::c_void,
                    0,
                    (embed_elems * std::mem::size_of::<f32>()) as libc::size_t,
                );
            }

            // M-RoPE positions: committed + depth_of_node.
            // Slot 0 = root = depth 0 → position `committed`.
            // ref: lines 1191-1201
            let n_us = n_flat as usize;
            let pos_elems = 4 * n_us;
            let tp = &mut tree_pos4[..pos_elems];
            for i in 0..n_us {
                let p = committed
                    + if i == 0 {
                        0
                    } else {
                        tree.depths[i - 1]
                    };
                tp[0 * n_us + i] = p;
                tp[1 * n_us + i] = p;
                tp[2 * n_us + i] = p;
                tp[3 * n_us + i] = 0;
            }
            unsafe {
                sys::ggml_backend_tensor_set(
                    sg.positions,
                    tree_pos4.as_ptr() as *const libc::c_void,
                    0,
                    (pos_elems * std::mem::size_of::<i32>()) as libc::size_t,
                );
            }

            // Ancestor-only attention mask. ref: lines 1203-1206
            crate::ddtree::build_tree_mask(&tree, committed, &mut mask_buf);
            unsafe {
                sys::ggml_backend_tensor_set(
                    sg.attn_mask,
                    mask_buf.as_ptr() as *const libc::c_void,
                    0,
                    (mask_buf.len() * std::mem::size_of::<u16>()) as libc::size_t,
                );
            }

            // parent_ids for tree-mode DeltaNet kernel.
            //   Slot 0 (root): -1 (reload initial state).
            //   Slots 1..N-1:  tree.parents[i] (flat-tree index).
            // ref: lines 1208-1215
            let pi = &mut tree_parent_ids[..n_us];
            pi[0] = -1;
            for i in 1..n_us {
                pi[i] = tree.parents[i];
            }
            unsafe {
                sys::ggml_backend_tensor_set(
                    sg.parent_ids,
                    tree_parent_ids.as_ptr() as *const libc::c_void,
                    0,
                    (n_us * std::mem::size_of::<i32>()) as libc::size_t,
                );
            }

            let st = unsafe { sys::ggml_backend_graph_compute(backend, sg.gf) };
            if st != ggml_status::GGML_STATUS_SUCCESS {
                return fail(&format!("ddtree verify compute: {:?}", st));
            }

            // Read N verify logits, compute posterior argmax per slot.
            let verify_elems = (vocab as usize) * n_us;
            unsafe {
                sys::ggml_backend_tensor_get(
                    sg.logits,
                    verify_logits_buf.as_mut_ptr() as *mut libc::c_void,
                    0,
                    (verify_elems * std::mem::size_of::<f32>()) as libc::size_t,
                );
            }
            let post = &mut posterior[..n_us];
            for i in 0..n_us {
                let slice = &verify_logits_buf[i * (vocab as usize)..(i + 1) * (vocab as usize)];
                post[i] = argmax_f32(slice);
            }

            // Walk tree → accepted DFS indices + next bonus token.
            // ref: lines 1235-1263
            let (accepted, next_token) = crate::ddtree::follow_verified_tree(&tree, post);
            let accept_depth = accepted.len() as c_int;

            // Commit count = accept_depth (includes root).
            // ref: lines 1265-1271
            let mut commit_n: c_int = accept_depth;
            if commit_n > need_commit_budget {
                commit_n = need_commit_budget;
            }

            // Emit tokens along the accepted path. slot 0 is last_tok
            // (the pending token from previous iter). next slots pull
            // from tree.token_ids. ref: lines 1273-1284
            for i in 0..commit_n as usize {
                let dfs_idx = accepted[i];
                let tok = if dfs_idx == 0 {
                    last_tok
                } else {
                    tree.token_ids[(dfs_idx - 1) as usize]
                };
                out_all.push(tok);
            }
            last_tok = next_token;

            // Rollback: SSM + conv + target_feat + KV compaction.
            // ref: lines 1290-1455
            let rollback_dfs: i32 = if commit_n > 0 {
                accepted[(commit_n - 1) as usize]
            } else {
                0
            };
            // Fast-path: pure-chain walk → accepted[i] == i ∀ i.
            let mut walked_sibling = false;
            for i in 0..commit_n as usize {
                if accepted[i] != i as i32 {
                    walked_sibling = true;
                    break;
                }
            }

            let n_delta = sg.delta_captures.len();
            const K_CONV: libc::size_t = 4;
            for il in 0..n_delta {
                let cap = sg.delta_captures[il];
                if cap.ssm_intermediate_states.is_null() || cap.conv_input.is_null() {
                    return fail(&format!(
                        "ddtree rollback: missing capture layer {il}"
                    ));
                }

                unsafe {
                    // SSM rollback at DFS slot `rollback_dfs`.
                    // ref: lines 1318-1335
                    let ssm_dst = cache.ssm_state[il];
                    let ssm_dst_ne = (*ssm_dst).ne;
                    let ssm_elems = (ssm_dst_ne[0] as libc::size_t)
                        * (ssm_dst_ne[1] as libc::size_t)
                        * (ssm_dst_ne[2] as libc::size_t);
                    let ssm_src_offset = (rollback_dfs as libc::size_t)
                        * (*cap.ssm_intermediate_states).nb[3];
                    let ssm_src = ((*cap.ssm_intermediate_states).data as *const u8)
                        .add(ssm_src_offset);
                    sys::dflash27b_launch_f16_to_f32(
                        ssm_src as *const libc::c_void,
                        (*ssm_dst).data,
                        ssm_elems,
                        std::ptr::null_mut(),
                    );

                    // Conv rollback — two paths.
                    // ref: lines 1336-1387
                    let conv_state_dst = cache.conv_state[il];
                    let row_cnt = (*cap.conv_input).ne[1] as libc::size_t;
                    let elt = sys::ggml_element_size(cap.conv_input);
                    let dpitch = (K_CONV - 1) * elt;
                    let spitch = (*cap.conv_input).nb[1];

                    if !walked_sibling {
                        // Hot path: 3 contiguous slots ending at
                        // rollback_dfs. conv_input has K-1 history
                        // rows prepended, so slot `rollback_dfs + 1`
                        // is where the source window starts.
                        let conv_off = (rollback_dfs as libc::size_t) + 1;
                        let conv_src = ((*cap.conv_input).data as *const u8)
                            .add(conv_off * elt);
                        let ce = sys::cudaMemcpy2DAsync(
                            (*conv_state_dst).data,
                            dpitch,
                            conv_src as *const libc::c_void,
                            spitch,
                            (K_CONV - 1) * elt,
                            row_cnt,
                            sys::CUDA_MEMCPY_DEVICE_TO_DEVICE,
                            std::ptr::null_mut(),
                        );
                        if ce != 0 {
                            return fail(&format!(
                                "ddtree conv fast il={il} ce={ce}"
                            ));
                        }
                    } else {
                        // Sibling path: K-1 separate column copies
                        // along the ancestry chain.
                        let mut virt: [i32; 3] = [0; 3];
                        virt[(K_CONV - 2) as usize] = rollback_dfs;
                        // Safe because K_CONV-1 is known 3.
                        for k in (0..(K_CONV as i32 - 2)).rev() {
                            let prev = virt[(k + 1) as usize];
                            virt[k as usize] = if prev >= 0 {
                                tree.parents[prev as usize]
                            } else {
                                prev - 1
                            };
                        }
                        for k in 0..(K_CONV - 1) {
                            let sx_slot = (K_CONV - 1) as i64 + virt[k as usize] as i64;
                            let src_col = ((*cap.conv_input).data as *const u8)
                                .add((sx_slot as libc::size_t) * elt);
                            let dst_col = ((*conv_state_dst).data as *mut u8)
                                .add((k as libc::size_t) * elt);
                            let ce = sys::cudaMemcpy2DAsync(
                                dst_col as *mut libc::c_void,
                                dpitch,
                                src_col as *const libc::c_void,
                                spitch,
                                elt,
                                row_cnt,
                                sys::CUDA_MEMCPY_DEVICE_TO_DEVICE,
                                std::ptr::null_mut(),
                            );
                            if ce != 0 {
                                return fail(&format!(
                                    "ddtree conv col il={il} k={k} ce={ce}"
                                ));
                            }
                        }
                    }
                }
            }

            // target_feat compaction. ref: lines 1389-1413
            if !cache.target_feat.is_null() {
                unsafe {
                    let elt = sys::ggml_element_size(cache.target_feat);
                    let fc_in = (*cache.target_feat).ne[0] as libc::size_t; // 5*hidden
                    let col_stride = (*cache.target_feat).nb[1];
                    let tcap = cache.target_feat_cap;
                    for d in 1..commit_n {
                        let src_dfs = accepted[d as usize];
                        if src_dfs == d {
                            continue;
                        }
                        let src_slot = (committed + src_dfs) % tcap;
                        let dst_slot = (committed + d) % tcap;
                        let src_off = (src_slot as libc::size_t) * col_stride;
                        let dst_off = (dst_slot as libc::size_t) * col_stride;
                        let ce = sys::cudaMemcpyAsync(
                            ((*cache.target_feat).data as *mut u8).add(dst_off)
                                as *mut libc::c_void,
                            ((*cache.target_feat).data as *const u8).add(src_off)
                                as *const libc::c_void,
                            fc_in * elt,
                            sys::CUDA_MEMCPY_DEVICE_TO_DEVICE,
                            std::ptr::null_mut(),
                        );
                        if ce != 0 {
                            return fail(&format!(
                                "ddtree target_feat compact d={d} ce={ce}"
                            ));
                        }
                    }
                }
            }

            // Full-attention KV compaction. ref: lines 1415-1451
            let n_full_attn = cache.attn_k.len();
            for d in 0..commit_n {
                let src_dfs = accepted[d as usize];
                let dst_slot = d;
                if src_dfs == dst_slot {
                    continue;
                }
                for l in 0..n_full_attn {
                    unsafe {
                        let ck = cache.attn_k[l];
                        let cv = cache.attn_v[l];
                        let slot_bytes = (*ck).nb[1];
                        let src_off = ((committed + src_dfs) as libc::size_t) * slot_bytes;
                        let dst_off = ((committed + dst_slot) as libc::size_t) * slot_bytes;
                        let n_kv = (*ck).ne[2] as usize;
                        for h in 0..n_kv {
                            let head_src = src_off + (h as libc::size_t) * (*ck).nb[2];
                            let head_dst = dst_off + (h as libc::size_t) * (*ck).nb[2];
                            let ce_k = sys::cudaMemcpyAsync(
                                ((*ck).data as *mut u8).add(head_dst) as *mut libc::c_void,
                                ((*ck).data as *const u8).add(head_src)
                                    as *const libc::c_void,
                                slot_bytes,
                                sys::CUDA_MEMCPY_DEVICE_TO_DEVICE,
                                std::ptr::null_mut(),
                            );
                            let ce_v = sys::cudaMemcpyAsync(
                                ((*cv).data as *mut u8).add(head_dst) as *mut libc::c_void,
                                ((*cv).data as *const u8).add(head_src)
                                    as *const libc::c_void,
                                slot_bytes,
                                sys::CUDA_MEMCPY_DEVICE_TO_DEVICE,
                                std::ptr::null_mut(),
                            );
                            if ce_k != 0 || ce_v != 0 {
                                return fail(&format!(
                                    "ddtree KV compact l={l} h={h} d={d} ce_k={ce_k} ce_v={ce_v}"
                                ));
                            }
                        }
                    }
                }
            }

            // ref: lines 1458-1462 — commit stats, skip the rest of
            // the loop body.
            committed += commit_n;
            n_generated += commit_n;
            n_accept_sum += commit_n;
            n_draft_steps += 1;
            continue;
        }

        // 4) Target verify (batched, with causal mask).
        //    `capture_delta_intermediate` tells the target graph to
        //    populate `sg.delta_captures[*].ssm_intermediate_states`
        //    and `.conv_input` — required for the fast_rollback path.
        //
        //    ref: lines 1375-1505 (chain-verify non-seq branch)
        if !build_target_step(
            &mut sg,
            w,
            cache,
            backend,
            /*kv_start=*/ committed,
            /*n_tokens=*/ q_len,
            /*with_mask=*/ true,
            /*capture=*/ true,
            /*capture_delta_intermediate=*/ fast_rollback,
        ) {
            return fail(&format!(
                "verify build: {}",
                crate::last_error()
            ));
        }

        // Embedding for verify tokens (reuse pre-allocated buffer).
        // ref: lines 1475-1478
        let verify_embed_elems = (hidden as usize) * (q_len as usize);
        if !w
            .embedder
            .embed(&draft_tok, &mut verify_embed[..verify_embed_elems])
        {
            return fail("verify: embedder failed");
        }
        unsafe {
            sys::ggml_backend_tensor_set(
                sg.inp_embed,
                verify_embed.as_ptr() as *const libc::c_void,
                0,
                (verify_embed_elems * std::mem::size_of::<f32>()) as libc::size_t,
            );
        }

        // Positions (same 4-axis pattern as prefill). ref: lines 1482-1489
        for i in 0..q_len as usize {
            let p = committed + (i as c_int);
            pos4_buf[0 * (q_len as usize) + i] = p;
            pos4_buf[1 * (q_len as usize) + i] = p;
            pos4_buf[2 * (q_len as usize) + i] = p;
            pos4_buf[3 * (q_len as usize) + i] = 0;
        }
        unsafe {
            sys::ggml_backend_tensor_set(
                sg.positions,
                pos4_buf.as_ptr() as *const libc::c_void,
                0,
                (pos4_buf.len() * std::mem::size_of::<i32>()) as libc::size_t,
            );
        }

        // Causal mask. ref: lines 1491-1492
        build_causal_mask(&mut mask_buf, committed + q_len, q_len, committed);
        unsafe {
            sys::ggml_backend_tensor_set(
                sg.attn_mask,
                mask_buf.as_ptr() as *const libc::c_void,
                0,
                (mask_buf.len() * std::mem::size_of::<u16>()) as libc::size_t,
            );
        }

        let st = unsafe { sys::ggml_backend_graph_compute(backend, sg.gf) };
        if st != ggml_status::GGML_STATUS_SUCCESS {
            return fail(&format!("verify compute: status {:?}", st));
        }

        // Download verify logits and argmax per position. ref: lines 1501-1505
        unsafe {
            sys::ggml_backend_tensor_get(
                sg.logits,
                verify_logits_buf.as_mut_ptr() as *mut libc::c_void,
                0,
                ((vocab * q_len) as libc::size_t) * std::mem::size_of::<f32>() as libc::size_t,
            );
        }
        for i in 0..q_len as usize {
            let slice =
                &verify_logits_buf[i * (vocab as usize)..(i + 1) * (vocab as usize)];
            target_tok[i] = argmax_f32(slice);
        }

        // 5) Greedy longest-prefix accept. ref: lines 1554-1558
        let mut accept_n: c_int = 1; // draft_tok[0] assumed = last_tok
        for i in 0..(q_len - 1) as usize {
            if draft_tok[i + 1] == target_tok[i] {
                accept_n += 1;
            } else {
                break;
            }
        }

        // Commit-strategy branch. ref: lines 1568-1586
        //
        //   * fast_rollback: commit_n = accept_n (no explicit bonus —
        //     the "bonus" becomes the next iter's pinned draft[0]).
        //   * legacy:        commit_n = accept_n + bonus (bonus token
        //     is target's correction for the first mismatched draft).
        let mut bonus_tok: i32 = -1;
        let mut commit_n: c_int;
        if fast_rollback {
            commit_n = accept_n;
        } else {
            if accept_n < q_len {
                bonus_tok = target_tok[(accept_n - 1) as usize];
            }
            commit_n = accept_n + if bonus_tok >= 0 { 1 } else { 0 };
        }
        if commit_n > need_commit_budget {
            commit_n = need_commit_budget;
            if commit_n <= accept_n {
                bonus_tok = -1;
            }
        }

        // (per-step tracing disabled in hot path — args would
        // materialize even when the filter rejects the event)

        // 6) Rollback + commit.
        if fast_rollback {
            // ── Fast-rollback path. ref: test_dflash.cpp:1603-1695
            //
            // Rollback SSM + conv unless we fully accepted. When
            // commit_n == q_len the state after processing all q_len
            // tokens IS what we want (KV is correct, SSM state is at
            // position committed + q_len).
            if commit_n < q_len {
                let rollback_idx = commit_n - 1; // 0-based intermediate index

                let n_delta = sg.delta_captures.len();
                for il in 0..n_delta {
                    let cap = sg.delta_captures[il];
                    if cap.ssm_intermediate_states.is_null() || cap.conv_input.is_null() {
                        return fail(&format!(
                            "rollback: missing capture at layer {il}"
                        ));
                    }

                    unsafe {
                        // ── SSM rollback: copy f16 intermediate[rollback_idx]
                        //    → cache.ssm_state[il] (f32 destination, so we
                        //    widen on copy via the vendored kernel).
                        //
                        //    ref: test_dflash.cpp:1628-1646
                        let ssm_dst = cache.ssm_state[il];
                        let ssm_dst_ne = (*ssm_dst).ne;
                        let ssm_elems = (ssm_dst_ne[0] as libc::size_t)
                            * (ssm_dst_ne[1] as libc::size_t)
                            * (ssm_dst_ne[2] as libc::size_t);
                        let ssm_src_offset = (rollback_idx as libc::size_t)
                            * (*cap.ssm_intermediate_states).nb[3];
                        let ssm_src =
                            ((*cap.ssm_intermediate_states).data as *const u8)
                                .add(ssm_src_offset);
                        sys::dflash27b_launch_f16_to_f32(
                            ssm_src as *const libc::c_void,
                            (*ssm_dst).data,
                            ssm_elems,
                            std::ptr::null_mut(),
                        );

                        // ── Conv rollback: copy
                        //    `conv_input[commit_n..commit_n+K-2, :, :]`
                        //    into `cache.conv_state[il]`.
                        //
                        //    conv_input shape: [kernel-1 + n_tokens, conv_channels, 1]
                        //       nb[0] = elt, nb[1] = (kernel-1+n_tokens)*elt
                        //    conv_state shape: [kernel-1, conv_channels, 1]
                        //       nb[0] = elt, nb[1] = (kernel-1)*elt
                        //
                        //    Need cudaMemcpy2D because the source has a larger
                        //    row stride than the dest.
                        //
                        //    ref: test_dflash.cpp:1649-1675
                        const K_CONV: libc::size_t = 4;
                        let conv_state_dst = cache.conv_state[il];
                        let row_cnt = (*cap.conv_input).ne[1] as libc::size_t;
                        let elt = sys::ggml_element_size(cap.conv_input);
                        let dpitch = (K_CONV - 1) * elt; // 12 bytes
                        let spitch = (*cap.conv_input).nb[1];
                        let width = (K_CONV - 1) * elt; // 3 floats per row
                        let conv_src = ((*cap.conv_input).data as *const u8)
                            .add((commit_n as libc::size_t) * elt);

                        let ce = sys::cudaMemcpy2DAsync(
                            (*conv_state_dst).data,
                            dpitch,
                            conv_src as *const libc::c_void,
                            spitch,
                            width,
                            row_cnt,
                            sys::CUDA_MEMCPY_DEVICE_TO_DEVICE,
                            std::ptr::null_mut(),
                        );
                        if ce != 0 {
                            return fail(&format!(
                                "cudaMemcpy2DAsync conv rollback il={il} ce={ce}"
                            ));
                        }
                    }
                }
            }

            // Next last_tok: target's prediction at position
            // committed+accept_n given the accepted prefix. Both
            // commit_n == q_len (fully accepted) and commit_n < q_len
            // (partial) reduce to `target_tok[commit_n - 1]`.
            //
            // ref: test_dflash.cpp:1592-1597
            last_tok = target_tok[(commit_n - 1) as usize];

            // Commit: push accepted draft tokens. No bonus — next iter
            // picks it up as last_tok. ref: line 1605
            for i in 0..commit_n as usize {
                out_all.push(draft_tok[i]);
            }
        } else {
            // ── Legacy replay path. ref: test_dflash.cpp:1696-1751
            restore_ssm_state(cache);

            let mut replay_tok: Vec<i32> = Vec::with_capacity(commit_n as usize);
            for i in 0..commit_n {
                let i_us = i as usize;
                if i < accept_n && i_us < draft_tok.len() {
                    replay_tok.push(draft_tok[i_us]);
                } else {
                    replay_tok.push(bonus_tok);
                }
            }

            let replay_with_mask = commit_n > 1;
            if !build_target_step(
                &mut sg,
                w,
                cache,
                backend,
                committed,
                commit_n,
                replay_with_mask,
                /*capture=*/ true,
                /*capture_delta_intermediate=*/ false,
            ) {
                return fail(&format!(
                    "replay build: {}",
                    crate::last_error()
                ));
            }

            // Reuse `verify_embed` scratch for the replay embed.
            let replay_embed_elems = (hidden as usize) * (commit_n as usize);
            if !w
                .embedder
                .embed(&replay_tok, &mut verify_embed[..replay_embed_elems])
            {
                return fail("replay: embedder failed");
            }
            unsafe {
                sys::ggml_backend_tensor_set(
                    sg.inp_embed,
                    verify_embed.as_ptr() as *const libc::c_void,
                    0,
                    (replay_embed_elems * std::mem::size_of::<f32>()) as libc::size_t,
                );
            }

            let replay_pos_elems = 4 * (commit_n as usize);
            let rp = &mut replay_pos[..replay_pos_elems];
            for i in 0..commit_n as usize {
                let p = committed + (i as c_int);
                rp[0 * (commit_n as usize) + i] = p;
                rp[1 * (commit_n as usize) + i] = p;
                rp[2 * (commit_n as usize) + i] = p;
                rp[3 * (commit_n as usize) + i] = 0;
            }
            unsafe {
                sys::ggml_backend_tensor_set(
                    sg.positions,
                    replay_pos.as_ptr() as *const libc::c_void,
                    0,
                    (replay_pos_elems * std::mem::size_of::<i32>()) as libc::size_t,
                );
            }

            if replay_with_mask {
                build_causal_mask(
                    &mut mask_buf,
                    committed + commit_n,
                    commit_n,
                    committed,
                );
                unsafe {
                    sys::ggml_backend_tensor_set(
                        sg.attn_mask,
                        mask_buf.as_ptr() as *const libc::c_void,
                        0,
                        (mask_buf.len() * std::mem::size_of::<u16>()) as libc::size_t,
                    );
                }
            }

            let st = unsafe { sys::ggml_backend_graph_compute(backend, sg.gf) };
            if st != ggml_status::GGML_STATUS_SUCCESS {
                return fail(&format!("replay compute: status {:?}", st));
            }

            let last_off: libc::size_t = (vocab as libc::size_t)
                * (commit_n as libc::size_t - 1)
                * std::mem::size_of::<f32>() as libc::size_t;
            unsafe {
                sys::ggml_backend_tensor_get(
                    sg.logits,
                    last_logits.as_mut_ptr() as *mut libc::c_void,
                    last_off,
                    (vocab as libc::size_t) * std::mem::size_of::<f32>() as libc::size_t,
                );
            }
            last_tok = argmax_f32(&last_logits);

            for &t in &replay_tok {
                out_all.push(t);
            }
        }

        committed += commit_n;
        n_generated += commit_n;
        n_accept_sum += accept_n;
        n_draft_steps += 1;
    }

    let t_gen1 = std::time::Instant::now();
    let wall_s = t_gen1.duration_since(t_pf0).as_secs_f64();
    let gen_s = t_gen1.duration_since(t_gen0).as_secs_f64();
    let decode_tok_s = if gen_s > 0.0 {
        (n_generated as f64) / gen_s
    } else {
        0.0
    };

    Ok(RunStats {
        n_generated,
        n_draft_steps,
        n_accept_sum,
        wall_s,
        prefill_s,
        decode_tok_s,
    })
}

#[inline]
fn fail<T>(msg: &str) -> Result<T, String> {
    set_last_error(msg);
    Err(msg.to_string())
}
