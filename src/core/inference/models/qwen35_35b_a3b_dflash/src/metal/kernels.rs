//! Rust dispatch wrappers for the crate's metallib.
//!
//! # Current state — byte-exact source vendoring transition
//!
//! The metallib is built from two byte-exact upstream sources:
//!
//!   * `vendor/metal/shaders/ggml/ggml-metal.metal` (10,592 lines,
//!     pinned to llama.cpp commit `vendor/metal/ggml-metal.version`)
//!     → 111 ggml compute kernels.
//!   * `vendor/metal/shaders/dflash/*.metal` (6 files) byte-exact
//!     from `dflash_mlx/kernels.py` + `dflash_mlx/verify_qmm.py`.
//!
//! The **dispatch layer below** is being ported byte-exact from
//! `llama.cpp/ggml/src/ggml-metal/ggml-metal-ops.cpp` — each ggml op
//! gets a Rust function that builds the matching `ggml_metal_kargs_*`
//! struct, binds src/dst buffers, and launches with the right grid /
//! threadgroup dimensions.
//!
//! The earlier hand-written dispatch helpers (rms_norm_bf16,
//! sdpa_naive_bf16, quantized_matmul_mlx4bit_gs64_bf16, …) are **gone
//! together with the hand-written shaders they dispatched to**. Their
//! call sites in `qwen.rs` / `runtime.rs` now point at the `ggml_*`
//! functions below, which are pending the port.
//!
//! Until the port completes, any Rust call that tries to invoke a
//! `ggml_*` base op returns `false` + `set_last_error(...)` with a
//! pointer to the specific line range in `ggml-metal-ops.cpp` that
//! has to be ported.
//!
//! The dflash-specific dispatches at the bottom of this file are
//! unaffected — they still dispatch against the 6 byte-exact dflash
//! shaders as before.

use crate::common::errors::set_last_error;
use crate::metal::ffi::{Buffer, ComputeEncoder, Device};
use crate::metal::mlx_ops::{self, MlxDtype};
use crate::metal::moe::ExpertLinear4Bit;

fn dispatch_1d(enc: &ComputeEncoder, n: i32) {
    let tg = 256usize;
    let grid = (((n.max(0) as usize) + tg - 1) / tg, 1, 1);
    enc.dispatch_threadgroups(grid, (tg, 1, 1));
}

fn pending(fn_name: &str, ggml_fn: &str) -> bool {
    set_last_error(format!(
        "{fn_name}: pending byte-exact port of {ggml_fn} from \
         llama.cpp/ggml/src/ggml-metal/ggml-metal-ops.cpp \
         (commit pinned in vendor/metal/ggml-metal.version)"
    ));
    false
}

fn bind_pipeline(enc: &ComputeEncoder, dev: &Device, name: &str) -> bool {
    match dev.pipeline(name) {
        Some(pso) => {
            enc.set_pipeline(&pso);
            true
        }
        None => {
            set_last_error(format!("pipeline `{name}` missing from metallib"));
            false
        }
    }
}

fn bind_custom(enc: &ComputeEncoder, dev: &Device, name: &str) -> bool {
    bind_pipeline(enc, dev, name)
}

// ═══════════════════════════════════════════════════════════════════
//  ggml-metal base-op dispatch wrappers (pending byte-exact port)
// ═══════════════════════════════════════════════════════════════════
//
// The function signatures below match the call sites in `qwen.rs` +
// `runtime.rs`. Internally each one will call
// `bind_pipeline("kernel_<name>_<type>")` and set up the ggml kargs
// struct + buffer bindings + grid/threadgroup exactly as the
// corresponding `ggml_metal_op_<name>()` function in
// `ggml-metal-ops.cpp` does.
//
// Ordering below follows the order they appear in
// `ggml-metal-ops.cpp`.

/// ref: ggml-metal-ops.cpp::ggml_metal_op_cpy (line 643-683)
/// kernels: kernel_cpy_{f32,f16,bf16}_{f32,f16,bf16}
pub fn ggml_cpy_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _src: &Buffer,
    _dst: &Buffer,
    _n_bytes: usize,
) -> bool {
    pending("ggml_cpy_bf16", "ggml_metal_op_cpy")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_bin (line 681-...)
/// kernels: kernel_add_{row,row_c4}
pub fn ggml_add_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _a: &Buffer,
    _b: &Buffer,
    _y: &Buffer,
    _n: i32,
) -> bool {
    pending("ggml_add_bf16", "ggml_metal_op_bin (add)")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_bin
/// kernels: kernel_mul_{row,row_c4}
pub fn ggml_mul_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _a: &Buffer,
    _b: &Buffer,
    _y: &Buffer,
    _n: i32,
) -> bool {
    pending("ggml_mul_bf16", "ggml_metal_op_bin (mul)")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_unary (line 747-...)
/// kernels: kernel_silu, kernel_silu_4
pub fn ggml_silu_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _x: &Buffer,
    _y: &Buffer,
    _n: i32,
) -> bool {
    pending("ggml_silu_bf16", "ggml_metal_op_unary (silu)")
}

pub fn ggml_sigmoid_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _x: &Buffer,
    _y: &Buffer,
    _n: i32,
) -> bool {
    pending("ggml_sigmoid_bf16", "ggml_metal_op_unary (sigmoid)")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_norm (line 3334-...)
/// kernel: kernel_rms_norm_f32 / _mul_f32 / _mul_add_f32
pub fn ggml_rms_norm_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _x: &Buffer,
    _weight: &Buffer,
    _y: &Buffer,
    _d: i32,
    _eps: f32,
    _n_rows: usize,
) -> bool {
    pending("ggml_rms_norm_bf16", "ggml_metal_op_norm")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_norm (l2 variant)
/// kernel: kernel_l2_norm_impl (l2_norm_f32)
pub fn ggml_l2_norm_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _x: &Buffer,
    _y: &Buffer,
    _d: i32,
    _eps: f32,
    _n_rows: usize,
) -> bool {
    pending("ggml_l2_norm_bf16", "ggml_metal_op_norm (l2_norm)")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_soft_max (line 1305-...)
/// kernel: kernel_soft_max / kernel_soft_max_4
pub fn ggml_soft_max_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _x: &Buffer,
    _y: &Buffer,
    _d: i32,
    _n_rows: usize,
) -> bool {
    pending("ggml_soft_max_bf16", "ggml_metal_op_soft_max")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_get_rows (line 1142-...)
/// kernel: kernel_get_rows_q4_K
pub fn ggml_get_rows_q4_k(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _ids: &Buffer,
    _src: &Buffer,
    _dst: &Buffer,
    _n_tokens: i32,
    _hidden: i32,
) -> bool {
    pending("ggml_get_rows_q4_k", "ggml_metal_op_get_rows (q4_K)")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_mul_mat (line 2122-...)
/// kernels: kernel_mul_mv_q4_K_f32 (decode), kernel_mul_mm (prefill)
pub fn ggml_mul_mat_q4_k(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _x: &Buffer,
    _w: &Buffer,
    _y: &Buffer,
    _m: i32,
    _k: i32,
    _n: i32,
) -> bool {
    pending("ggml_mul_mat_q4_k", "ggml_metal_op_mul_mat (q4_K)")
}

/// ref: ggml-metal-ops.cpp (rope dispatch path)
/// kernels: kernel_rope_multi, kernel_rope_norm, kernel_rope_neox
pub fn ggml_rope_multi_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _x: &Buffer,
    _positions: &Buffer,
    _y: &Buffer,
    _head_dim: i32,
    _rope_dim: i32,
    _n_heads: i32,
    _base: f32,
    _n_tokens: usize,
) -> bool {
    pending("ggml_rope_multi_bf16", "ggml_metal_op_rope")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_ssm_conv (line 1380-...)
/// kernel: kernel_ssm_conv_f32_f32
pub fn ggml_ssm_conv_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _conv_state: &Buffer,
    _x_new: &Buffer,
    _weight: &Buffer,
    _y: &Buffer,
    _t: i32,
    _channels: i32,
    _kernel_size: i32,
) -> bool {
    pending("ggml_ssm_conv_bf16", "ggml_metal_op_ssm_conv")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_gated_delta_net (line 1580-1660)
/// kernel: kernel_gated_delta_net (this is the UPSTREAM ggml variant;
///         the dflash-mlx tape-replay variant stays in the
///         `gated_delta_tape_*` dispatches below).
pub fn ggml_gated_delta_net_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _q: &Buffer,
    _k: &Buffer,
    _v: &Buffer,
    _g: &Buffer,
    _beta: &Buffer,
    _state: &Buffer,
    _dst: &Buffer,
) -> bool {
    pending("ggml_gated_delta_net_bf16", "ggml_metal_op_gated_delta_net")
}

/// ref: ggml-metal-ops.cpp::ggml_metal_op_argsort
/// kernel: kernel_argsort_f32_i32
pub fn ggml_argmax_last_bf16(
    _enc: &ComputeEncoder,
    _dev: &Device,
    _x: &Buffer,
    _y: &Buffer,
    _v: i32,
    _n_rows: usize,
) -> bool {
    pending("ggml_argmax_last_bf16", "ggml_metal_op_argsort")
}

// ═══════════════════════════════════════════════════════════════════
//  Back-compat shims for forwards still on the old call shape
// ═══════════════════════════════════════════════════════════════════
//
// qwen.rs + runtime.rs call these old names. Every shim redirects to
// `pending(...)` with a pointer at the ggml kernel that has to be
// dispatched instead. Keeps the build green; any attempt to run real
// inference surfaces the exact port gap.

pub fn rms_norm_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    x: &Buffer,
    weight: &Buffer,
    y: &Buffer,
    d: i32,
    eps: f32,
    weight_bias: f32,
    n_rows: usize,
) -> bool {
    if !bind_custom(enc, dev, "ctox_rms_norm_bf16") {
        return false;
    }
    let rows = n_rows as i32;
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, weight, 0);
    enc.set_buffer(2, y, 0);
    enc.set_bytes(3, &d);
    enc.set_bytes(4, &eps);
    enc.set_bytes(5, &weight_bias);
    enc.set_bytes(6, &rows);
    enc.dispatch_threadgroups((n_rows, 1, 1), (256, 1, 1));
    true
}

pub fn silu_bf16(e: &ComputeEncoder, d: &Device, x: &Buffer, y: &Buffer, n: i32) -> bool {
    if !bind_custom(e, d, "ctox_silu_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, y, 0);
    e.set_bytes(2, &n);
    dispatch_1d(e, n);
    true
}

pub fn sigmoid_bf16(e: &ComputeEncoder, d: &Device, x: &Buffer, y: &Buffer, n: i32) -> bool {
    if !bind_custom(e, d, "ctox_sigmoid_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, y, 0);
    e.set_bytes(2, &n);
    dispatch_1d(e, n);
    true
}

pub fn add_bf16(
    e: &ComputeEncoder,
    d: &Device,
    a: &Buffer,
    b: &Buffer,
    y: &Buffer,
    n: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_add_bf16") {
        return false;
    }
    e.set_buffer(0, a, 0);
    e.set_buffer(1, b, 0);
    e.set_buffer(2, y, 0);
    e.set_bytes(3, &n);
    dispatch_1d(e, n);
    true
}

pub fn mul_bf16(
    e: &ComputeEncoder,
    d: &Device,
    a: &Buffer,
    b: &Buffer,
    y: &Buffer,
    n: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_mul_bf16") {
        return false;
    }
    e.set_buffer(0, a, 0);
    e.set_buffer(1, b, 0);
    e.set_buffer(2, y, 0);
    e.set_bytes(3, &n);
    dispatch_1d(e, n);
    true
}

pub fn zero_bf16(e: &ComputeEncoder, d: &Device, y: &Buffer, offset: usize, n: i32) -> bool {
    if n <= 0 {
        return true;
    }
    if !bind_custom(e, d, "ctox_zero_bf16") {
        return false;
    }
    e.set_buffer(0, y, offset);
    e.set_bytes(1, &n);
    dispatch_1d(e, n);
    true
}

pub fn scale_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    y: &Buffer,
    scale: f32,
    n: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_scale_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, y, 0);
    e.set_bytes(2, &scale);
    e.set_bytes(3, &n);
    dispatch_1d(e, n);
    true
}

pub fn silu_mul_bf16(
    e: &ComputeEncoder,
    d: &Device,
    gate: &Buffer,
    up: &Buffer,
    y: &Buffer,
    n: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_silu_mul_bf16") {
        return false;
    }
    e.set_buffer(0, gate, 0);
    e.set_buffer(1, up, 0);
    e.set_buffer(2, y, 0);
    e.set_bytes(3, &n);
    dispatch_1d(e, n);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn rope_apply_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    positions: &Buffer,
    y: &Buffer,
    head_dim: i32,
    rope_dim: i32,
    n_heads: i32,
    base: f32,
    n_tokens: usize,
) -> bool {
    if !bind_custom(e, d, "ctox_rope_bf16") {
        return false;
    }
    let n_tokens_i = n_tokens as i32;
    e.set_buffer(0, x, 0);
    e.set_buffer(1, positions, 0);
    e.set_buffer(2, y, 0);
    e.set_bytes(3, &head_dim);
    e.set_bytes(4, &rope_dim);
    e.set_bytes(5, &n_heads);
    e.set_bytes(6, &base);
    e.set_bytes(7, &n_tokens_i);
    dispatch_1d(e, n_tokens_i * n_heads * head_dim);
    true
}

pub fn argmax_last_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    y: &Buffer,
    v: i32,
    n_rows: usize,
) -> bool {
    if !bind_custom(e, d, "ctox_argmax_bf16") {
        return false;
    }
    let rows = n_rows as i32;
    e.set_buffer(0, x, 0);
    e.set_buffer(1, y, 0);
    e.set_bytes(2, &v);
    e.set_bytes(3, &rows);
    e.dispatch_threadgroups((n_rows, 1, 1), (256, 1, 1));
    true
}

pub fn transpose_thd_to_htd_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    y: &Buffer,
    n_tokens: i32,
    n_heads: i32,
    head_dim: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_transpose_thd_to_htd_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, y, 0);
    e.set_bytes(2, &n_tokens);
    e.set_bytes(3, &n_heads);
    e.set_bytes(4, &head_dim);
    dispatch_1d(e, n_tokens * n_heads * head_dim);
    true
}

pub fn transpose_htd_to_thd_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    y: &Buffer,
    n_tokens: i32,
    n_heads: i32,
    head_dim: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_transpose_htd_to_thd_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, y, 0);
    e.set_bytes(2, &n_tokens);
    e.set_bytes(3, &n_heads);
    e.set_bytes(4, &head_dim);
    dispatch_1d(e, n_tokens * n_heads * head_dim);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn quantized_matmul_mlx4bit_gs64_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    w_q: &Buffer,
    s: &Buffer,
    b: &Buffer,
    y: &Buffer,
    m: i32,
    k: i32,
    n: i32,
) -> bool {
    if m <= 0 {
        set_last_error(format!("quantized_matmul_mlx4bit_gs64_bf16: invalid M={m}"));
        return false;
    }

    // MLX's high-performance dispatch is shape-specific: single-row decode
    // uses qmv/qmv_fast, while block prefill/verify uses qmm_t. Do not route
    // M>1 through row-wise qmv as a convenience fallback; that hides missing
    // high-performance kernels and invalidates throughput numbers.
    if m == 1 && std::env::var_os("CTOX_METAL_LINEAR_M1_QMM").is_none() {
        mlx_ops::op_qmv(
            e,
            d,
            MlxDtype::Bf16,
            w_q,
            0,
            s,
            0,
            Some((b, 0)),
            x,
            0,
            y,
            0,
            1,
            n,
            k,
            64,
            4,
        )
    } else {
        mlx_ops::op_qmm_t(
            e,
            d,
            MlxDtype::Bf16,
            w_q,
            0,
            s,
            0,
            Some((b, 0)),
            x,
            0,
            y,
            0,
            m,
            n,
            k,
            64,
            4,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn embedding_gather_mlx4bit_gs64_bf16(
    e: &ComputeEncoder,
    d: &Device,
    ids: &Buffer,
    w_q: &Buffer,
    s: &Buffer,
    b: &Buffer,
    out: &Buffer,
    n_tokens: i32,
    hidden: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_embedding_gather_mlx4_bf16") {
        return false;
    }
    e.set_buffer(0, ids, 0);
    e.set_buffer(1, w_q, 0);
    e.set_buffer(2, s, 0);
    e.set_buffer(3, b, 0);
    e.set_buffer(4, out, 0);
    e.set_bytes(5, &n_tokens);
    e.set_bytes(6, &hidden);
    dispatch_1d(e, n_tokens * hidden);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn dense_matmul_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    w: &Buffer,
    bias: Option<&Buffer>,
    y: &Buffer,
    m: i32,
    k: i32,
    n: i32,
) -> bool {
    if std::env::var_os("CTOX_METAL_DENSE_NAIVE").is_some() {
        if !bind_custom(e, d, "ctox_dense_matmul_bf16") {
            return false;
        }
        let has_bias = if bias.is_some() { 1i32 } else { 0i32 };
        e.set_buffer(0, x, 0);
        e.set_buffer(1, w, 0);
        e.set_buffer(2, bias.unwrap_or(w), 0);
        e.set_buffer(3, y, 0);
        e.set_bytes(4, &m);
        e.set_bytes(5, &k);
        e.set_bytes(6, &n);
        e.set_bytes(7, &has_bias);
        dispatch_1d(e, m * n);
        return true;
    }
    if m == 1 {
        return mlx_ops::op_gemv_bf16(e, d, x, 0, w, 0, bias.map(|b| (b, 0)), y, 0, k, n);
    }

    if !mlx_ops::op_steel_segmented_gemm_nt_bf16(e, d, x, 0, w, 0, y, 0, m, n, k) {
        return false;
    }
    if let Some(b) = bias {
        return add_bias_bf16(e, d, y, b, y, m, n);
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub fn kv_cache_append_bf16(
    e: &ComputeEncoder,
    d: &Device,
    src: &Buffer,
    cache: &Buffer,
    n_tokens: i32,
    n_kv_heads: i32,
    head_dim: i32,
    max_ctx: i32,
    write_offset: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_kv_cache_append_bf16") {
        return false;
    }
    e.set_buffer(0, src, 0);
    e.set_buffer(1, cache, 0);
    e.set_bytes(2, &n_tokens);
    e.set_bytes(3, &n_kv_heads);
    e.set_bytes(4, &head_dim);
    e.set_bytes(5, &max_ctx);
    e.set_bytes(6, &write_offset);
    dispatch_1d(e, n_tokens * n_kv_heads * head_dim);
    true
}

pub fn split_q_gate_bf16(
    e: &ComputeEncoder,
    d: &Device,
    raw: &Buffer,
    q: &Buffer,
    gate: &Buffer,
    n_tokens: i32,
    q_features: i32,
    head_dim: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_split_q_gate_bf16") {
        return false;
    }
    e.set_buffer(0, raw, 0);
    e.set_buffer(1, q, 0);
    e.set_buffer(2, gate, 0);
    e.set_bytes(3, &n_tokens);
    e.set_bytes(4, &q_features);
    e.set_bytes(5, &head_dim);
    dispatch_1d(e, n_tokens * q_features);
    true
}

pub fn apply_attention_gate_bf16(
    e: &ComputeEncoder,
    d: &Device,
    attn: &Buffer,
    gate: &Buffer,
    n: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_apply_attention_gate_bf16") {
        return false;
    }
    e.set_buffer(0, attn, 0);
    e.set_buffer(1, gate, 0);
    e.set_bytes(2, &n);
    dispatch_1d(e, n);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn sdpa_naive_bf16(
    e: &ComputeEncoder,
    d: &Device,
    q: &Buffer,
    k: &Buffer,
    v: &Buffer,
    mask: Option<&Buffer>,
    out: &Buffer,
    n_heads: i32,
    n_kv_heads: i32,
    q_len: i32,
    kv_len: i32,
    kv_stride: i32,
    head_dim: i32,
    scale: f32,
    causal: bool,
) -> bool {
    if mask.is_some() {
        set_last_error("sdpa_naive_bf16: mask path pending in CTOX glue kernel");
        return false;
    }
    if q_len == 1 && head_dim <= 256 {
        if !bind_custom(e, d, "ctox_sdpa_decode_vec_bf16") {
            return false;
        }
        e.set_buffer(0, q, 0);
        e.set_buffer(1, k, 0);
        e.set_buffer(2, v, 0);
        e.set_buffer(3, out, 0);
        e.set_bytes(4, &n_heads);
        e.set_bytes(5, &n_kv_heads);
        e.set_bytes(6, &kv_len);
        e.set_bytes(7, &kv_stride);
        e.set_bytes(8, &head_dim);
        e.set_bytes(9, &scale);
        e.dispatch_threadgroups((n_heads as usize, 1, 1), (256, 1, 1));
        return true;
    }
    if !bind_custom(e, d, "ctox_sdpa_naive_bf16") {
        return false;
    }
    let causal_i = if causal { 1i32 } else { 0i32 };
    e.set_buffer(0, q, 0);
    e.set_buffer(1, k, 0);
    e.set_buffer(2, v, 0);
    e.set_buffer(3, out, 0);
    e.set_bytes(4, &n_heads);
    e.set_bytes(5, &n_kv_heads);
    e.set_bytes(6, &q_len);
    e.set_bytes(7, &kv_len);
    e.set_bytes(8, &kv_stride);
    e.set_bytes(9, &head_dim);
    e.set_bytes(10, &scale);
    e.set_bytes(11, &causal_i);
    dispatch_1d(e, q_len * n_heads * head_dim);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn sdpa_vector_mlx_bf16(
    e: &ComputeEncoder,
    d: &Device,
    q_htd: &Buffer,
    k_htd: &Buffer,
    v_htd: &Buffer,
    out_htd: &Buffer,
    n_heads: i32,
    gqa_factor: i32,
    q_len: i32,
    kv_len: i32,
    kv_stride: i32,
    head_dim: i32,
    scale: f32,
    do_causal: bool,
) -> bool {
    if !(head_dim == 64 || head_dim == 96 || head_dim == 128 || head_dim == 256) {
        set_last_error(format!(
            "sdpa_vector_mlx_bf16: unsupported head_dim={head_dim}"
        ));
        return false;
    }
    let name = format!("sdpa_vector_bfloat16_t_{head_dim}_{head_dim}");
    let cache_key = format!(
        "{name}_nomask_qnt_{}_nosinks",
        if do_causal { "c" } else { "nc" }
    );
    let Some(pso) = d.pipeline_with_constants(&cache_key, &name, |cv| {
        crate::metal::ops::cv_set_bool(cv, false, 20);
        crate::metal::ops::cv_set_bool(cv, false, 21);
        crate::metal::ops::cv_set_bool(cv, do_causal, 22);
        crate::metal::ops::cv_set_bool(cv, false, 23);
        crate::metal::ops::cv_set_bool(cv, false, 24);
        crate::metal::ops::cv_set_bool(cv, false, 25);
    }) else {
        set_last_error(format!("sdpa_vector_mlx_bf16: pipeline `{name}` not found"));
        return false;
    };
    let k_head_stride = (kv_stride * head_dim) as usize;
    let k_seq_stride = head_dim as usize;
    let v_head_stride = k_head_stride;
    let v_seq_stride = k_seq_stride;
    e.set_pipeline(&pso);
    e.set_buffer(0, q_htd, 0);
    e.set_buffer(1, k_htd, 0);
    e.set_buffer(2, v_htd, 0);
    e.set_buffer(3, out_htd, 0);
    e.set_bytes(4, &gqa_factor);
    e.set_bytes(5, &kv_len);
    e.set_bytes(6, &k_head_stride);
    e.set_bytes(7, &k_seq_stride);
    e.set_bytes(8, &v_head_stride);
    e.set_bytes(9, &v_seq_stride);
    e.set_bytes(10, &scale);
    e.dispatch_threadgroups((n_heads as usize, q_len as usize, 1), (1024, 1, 1));
    true
}

#[allow(clippy::too_many_arguments)]
pub fn sdpa_vector_2pass_mlx_bf16(
    e: &ComputeEncoder,
    d: &Device,
    q_htd: &Buffer,
    k_htd: &Buffer,
    v_htd: &Buffer,
    partials: &Buffer,
    sums: &Buffer,
    maxs: &Buffer,
    out_htd: &Buffer,
    n_heads: i32,
    gqa_factor: i32,
    q_len: i32,
    kv_len: i32,
    kv_stride: i32,
    head_dim: i32,
    scale: f32,
    blocks: i32,
    _do_causal: bool,
) -> bool {
    if !sdpa_2pass_partials_bf16(
        e,
        d,
        false,
        q_htd,
        k_htd,
        v_htd,
        gqa_factor,
        kv_len,
        kv_stride * head_dim,
        head_dim,
        kv_stride * head_dim,
        head_dim,
        scale,
        blocks,
        None,
        partials,
        sums,
        maxs,
        n_heads as usize,
        1,
        q_len as usize,
    ) {
        return false;
    }
    sdpa_2pass_reduce_bf16(
        e,
        d,
        partials,
        sums,
        maxs,
        blocks,
        out_htd,
        n_heads as usize,
        q_len as usize,
        head_dim,
    )
}

pub fn softplus_bf16(e: &ComputeEncoder, d: &Device, x: &Buffer, y: &Buffer, n: i32) -> bool {
    if !bind_custom(e, d, "ctox_softplus_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, y, 0);
    e.set_bytes(2, &n);
    dispatch_1d(e, n);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn add_bias_bf16(
    e: &ComputeEncoder,
    d: &Device,
    a: &Buffer,
    bias: &Buffer,
    y: &Buffer,
    rows: i32,
    cols: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_add_bias_bf16") {
        return false;
    }
    e.set_buffer(0, a, 0);
    e.set_buffer(1, bias, 0);
    e.set_buffer(2, y, 0);
    e.set_bytes(3, &rows);
    e.set_bytes(4, &cols);
    dispatch_1d(e, rows * cols);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn neg_exp_mul_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    a_log: &Buffer,
    y: &Buffer,
    rows: i32,
    cols: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_neg_exp_mul_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, a_log, 0);
    e.set_buffer(2, y, 0);
    e.set_bytes(3, &rows);
    e.set_bytes(4, &cols);
    dispatch_1d(e, rows * cols);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn softplus_neg_exp_mul_bias_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    bias: &Buffer,
    a_log: &Buffer,
    y: &Buffer,
    rows: i32,
    cols: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_softplus_neg_exp_mul_bias_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, bias, 0);
    e.set_buffer(2, a_log, 0);
    e.set_buffer(3, y, 0);
    e.set_bytes(4, &rows);
    e.set_bytes(5, &cols);
    dispatch_1d(e, rows * cols);
    true
}

pub fn l2_norm_last_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    y: &Buffer,
    d_arg: i32,
    eps: f32,
    n_rows: usize,
) -> bool {
    if !bind_custom(e, d, "ctox_l2_norm_bf16") {
        return false;
    }
    let rows = n_rows as i32;
    e.set_buffer(0, x, 0);
    e.set_buffer(1, y, 0);
    e.set_bytes(2, &d_arg);
    e.set_bytes(3, &eps);
    e.set_bytes(4, &rows);
    e.dispatch_threadgroups((n_rows, 1, 1), (256, 1, 1));
    true
}

#[allow(clippy::too_many_arguments)]
pub fn conv_concat_bf16(
    e: &ComputeEncoder,
    d: &Device,
    conv_state: &Buffer,
    qkv_new: &Buffer,
    out: &Buffer,
    kernel_m1: i32,
    n_tokens: i32,
    conv_channels: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_conv_concat_bf16") {
        return false;
    }
    e.set_buffer(0, conv_state, 0);
    e.set_buffer(1, qkv_new, 0);
    e.set_buffer(2, out, 0);
    e.set_bytes(3, &kernel_m1);
    e.set_bytes(4, &n_tokens);
    e.set_bytes(5, &conv_channels);
    dispatch_1d(e, (kernel_m1 + n_tokens) * conv_channels);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn split_qkv_conv_bf16(
    e: &ComputeEncoder,
    d: &Device,
    conv_out: &Buffer,
    q: &Buffer,
    k: &Buffer,
    v: &Buffer,
    n_tokens: i32,
    q_size: i32,
    v_size: i32,
    conv_channels: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_split_qkv_conv_bf16") {
        return false;
    }
    e.set_buffer(0, conv_out, 0);
    e.set_buffer(1, q, 0);
    e.set_buffer(2, k, 0);
    e.set_buffer(3, v, 0);
    e.set_bytes(4, &n_tokens);
    e.set_bytes(5, &q_size);
    e.set_bytes(6, &v_size);
    e.set_bytes(7, &conv_channels);
    dispatch_1d(e, n_tokens * (2 * q_size + v_size));
    true
}

#[allow(clippy::too_many_arguments)]
pub fn ssm_conv1d_bf16(
    e: &ComputeEncoder,
    d: &Device,
    conv_state_in: &Buffer,
    x_new: &Buffer,
    weight: &Buffer,
    bias: &Buffer,
    y: &Buffer,
    conv_state_out: &Buffer,
    t: i32,
    channels: i32,
    kernel_size: i32,
    has_bias: bool,
) -> bool {
    if !bind_custom(e, d, "ctox_ssm_conv1d_bf16") {
        return false;
    }
    let has_bias_i = if has_bias { 1i32 } else { 0i32 };
    e.set_buffer(0, conv_state_in, 0);
    e.set_buffer(1, x_new, 0);
    e.set_buffer(2, weight, 0);
    e.set_buffer(3, bias, 0);
    e.set_buffer(4, y, 0);
    e.set_bytes(5, &t);
    e.set_bytes(6, &channels);
    e.set_bytes(7, &kernel_size);
    e.set_bytes(8, &has_bias_i);
    dispatch_1d(e, t * channels);

    if !bind_custom(e, d, "ctox_ssm_conv_state_update_bf16") {
        return false;
    }
    e.set_buffer(0, conv_state_in, 0);
    e.set_buffer(1, x_new, 0);
    e.set_buffer(2, conv_state_out, 0);
    e.set_bytes(3, &t);
    e.set_bytes(4, &channels);
    e.set_bytes(5, &kernel_size);
    dispatch_1d(e, channels);
    true
}

pub fn copy_raw_bf16(e: &ComputeEncoder, d: &Device, src: &Buffer, dst: &Buffer, n: i32) -> bool {
    if !bind_custom(e, d, "ctox_copy_bf16") {
        return false;
    }
    e.set_buffer(0, src, 0);
    e.set_buffer(1, dst, 0);
    e.set_bytes(2, &n);
    dispatch_1d(e, n);
    true
}

pub fn copy_raw_bf16_offset(
    e: &ComputeEncoder,
    d: &Device,
    src: &Buffer,
    src_offset: usize,
    dst: &Buffer,
    dst_offset: usize,
    n: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_copy_bf16") {
        return false;
    }
    e.set_buffer(0, src, src_offset);
    e.set_buffer(1, dst, dst_offset);
    e.set_bytes(2, &n);
    dispatch_1d(e, n);
    true
}

pub fn copy_raw_f32(e: &ComputeEncoder, d: &Device, src: &Buffer, dst: &Buffer, n: i32) -> bool {
    if !bind_custom(e, d, "ctox_copy_f32") {
        return false;
    }
    e.set_buffer(0, src, 0);
    e.set_buffer(1, dst, 0);
    e.set_bytes(2, &n);
    dispatch_1d(e, n);
    true
}

pub fn copy_raw_u32(e: &ComputeEncoder, d: &Device, src: &Buffer, dst: &Buffer, n: i32) -> bool {
    if !bind_custom(e, d, "ctox_copy_u32") {
        return false;
    }
    e.set_buffer(0, src, 0);
    e.set_buffer(1, dst, 0);
    e.set_bytes(2, &n);
    dispatch_1d(e, n);
    true
}

pub fn repeat_hidden5_bf16(
    e: &ComputeEncoder,
    d: &Device,
    src: &Buffer,
    dst: &Buffer,
    src_row_offset: i32,
    rows: i32,
    hidden: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_repeat_hidden5_bf16") {
        return false;
    }
    let byte_offset = (src_row_offset.max(0) as usize) * (hidden as usize) * 2;
    e.set_buffer(0, src, byte_offset);
    e.set_buffer(1, dst, 0);
    e.set_bytes(2, &rows);
    e.set_bytes(3, &hidden);
    dispatch_1d(e, rows * hidden * 5);
    true
}

pub fn copy_hidden_slot_bf16(
    e: &ComputeEncoder,
    d: &Device,
    src: &Buffer,
    dst: &Buffer,
    src_row: i32,
    hidden: i32,
    slot: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_copy_hidden_slot_bf16") {
        return false;
    }
    e.set_buffer(0, src, 0);
    e.set_buffer(1, dst, 0);
    e.set_bytes(2, &src_row);
    e.set_bytes(3, &hidden);
    e.set_bytes(4, &slot);
    dispatch_1d(e, hidden);
    true
}

pub fn copy_hidden_slot_rows_bf16(
    e: &ComputeEncoder,
    d: &Device,
    src: &Buffer,
    dst: &Buffer,
    rows: i32,
    hidden: i32,
    slot: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_copy_hidden_slot_rows_bf16") {
        return false;
    }
    e.set_buffer(0, src, 0);
    e.set_buffer(1, dst, 0);
    e.set_bytes(2, &rows);
    e.set_bytes(3, &hidden);
    e.set_bytes(4, &slot);
    dispatch_1d(e, rows * hidden);
    true
}

pub fn copy_hidden_slot_rows_bf16_offset(
    e: &ComputeEncoder,
    d: &Device,
    src: &Buffer,
    dst: &Buffer,
    dst_row_offset: i32,
    rows: i32,
    hidden: i32,
    slot: i32,
    n_slots: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_copy_hidden_slot_rows_bf16") {
        return false;
    }
    let byte_offset =
        (dst_row_offset.max(0) as usize) * (hidden as usize) * (n_slots as usize) * 2;
    e.set_buffer(0, src, 0);
    e.set_buffer(1, dst, byte_offset);
    e.set_bytes(2, &rows);
    e.set_bytes(3, &hidden);
    e.set_bytes(4, &slot);
    dispatch_1d(e, rows * hidden);
    true
}

pub fn gdn_conv_state_replay_bf16(
    e: &ComputeEncoder,
    d: &Device,
    state_in: &Buffer,
    qkv: &Buffer,
    state_out: &Buffer,
    accepted: i32,
    state_rows: i32,
    channels: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_gdn_conv_state_replay_bf16") {
        return false;
    }
    e.set_buffer(0, state_in, 0);
    e.set_buffer(1, qkv, 0);
    e.set_buffer(2, state_out, 0);
    e.set_bytes(3, &accepted);
    e.set_bytes(4, &state_rows);
    e.set_bytes(5, &channels);
    dispatch_1d(e, state_rows * channels);
    true
}

pub fn moe_route_topk_bf16(
    e: &ComputeEncoder,
    d: &Device,
    router_logits: &Buffer,
    topk_ids: &Buffer,
    topk_weights: &Buffer,
    n_tokens: i32,
    num_experts: i32,
    top_k: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_moe_route_topk_bf16") {
        return false;
    }
    e.set_buffer(0, router_logits, 0);
    e.set_buffer(1, topk_ids, 0);
    e.set_buffer(2, topk_weights, 0);
    e.set_bytes(3, &n_tokens);
    e.set_bytes(4, &num_experts);
    e.set_bytes(5, &top_k);
    e.dispatch_threadgroups((n_tokens.max(0) as usize, 1, 1), (1, 1, 1));
    true
}

pub fn moe_fill_gather_indices_i32(
    e: &ComputeEncoder,
    d: &Device,
    token_lhs: &Buffer,
    slot_lhs: &Buffer,
    n_tokens: i32,
    top_k: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_moe_fill_gather_indices_i32") {
        return false;
    }
    e.set_buffer(0, token_lhs, 0);
    e.set_buffer(1, slot_lhs, 0);
    e.set_bytes(2, &n_tokens);
    e.set_bytes(3, &top_k);
    dispatch_1d(e, n_tokens * top_k);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn moe_expert_gate_up_bf16(
    e: &ComputeEncoder,
    d: &Device,
    x: &Buffer,
    gate: &ExpertLinear4Bit,
    up: &ExpertLinear4Bit,
    topk_ids: &Buffer,
    prod: &Buffer,
    n_tokens: i32,
    top_k: i32,
    hidden: i32,
    intermediate: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_moe_expert_gate_up_bf16") {
        return false;
    }
    e.set_buffer(0, x, 0);
    e.set_buffer(1, &gate.w_q, 0);
    e.set_buffer(2, &gate.scales, 0);
    e.set_buffer(3, &gate.biases, 0);
    e.set_buffer(4, &up.w_q, 0);
    e.set_buffer(5, &up.scales, 0);
    e.set_buffer(6, &up.biases, 0);
    e.set_buffer(7, topk_ids, 0);
    e.set_buffer(8, prod, 0);
    e.set_bytes(9, &n_tokens);
    e.set_bytes(10, &top_k);
    e.set_bytes(11, &hidden);
    e.set_bytes(12, &intermediate);
    dispatch_1d(e, n_tokens * top_k * intermediate);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn moe_expert_down_accum_bf16(
    e: &ComputeEncoder,
    d: &Device,
    prod: &Buffer,
    down: &ExpertLinear4Bit,
    topk_ids: &Buffer,
    topk_weights: &Buffer,
    y: &Buffer,
    n_tokens: i32,
    top_k: i32,
    hidden: i32,
    intermediate: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_moe_expert_down_accum_bf16") {
        return false;
    }
    e.set_buffer(0, prod, 0);
    e.set_buffer(1, &down.w_q, 0);
    e.set_buffer(2, &down.scales, 0);
    e.set_buffer(3, &down.biases, 0);
    e.set_buffer(4, topk_ids, 0);
    e.set_buffer(5, topk_weights, 0);
    e.set_buffer(6, y, 0);
    e.set_bytes(7, &n_tokens);
    e.set_bytes(8, &top_k);
    e.set_bytes(9, &hidden);
    e.set_bytes(10, &intermediate);
    dispatch_1d(e, n_tokens * hidden);
    true
}

pub fn moe_accum_weighted_bf16(
    e: &ComputeEncoder,
    d: &Device,
    down_slots: &Buffer,
    topk_weights: &Buffer,
    y: &Buffer,
    n_tokens: i32,
    top_k: i32,
    hidden: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_moe_accum_weighted_bf16") {
        return false;
    }
    e.set_buffer(0, down_slots, 0);
    e.set_buffer(1, topk_weights, 0);
    e.set_buffer(2, y, 0);
    e.set_bytes(3, &n_tokens);
    e.set_bytes(4, &top_k);
    e.set_bytes(5, &hidden);
    dispatch_1d(e, n_tokens * hidden);
    true
}

pub fn moe_add_shared_bf16(
    e: &ComputeEncoder,
    d: &Device,
    y: &Buffer,
    shared: &Buffer,
    shared_gate: &Buffer,
    n_tokens: i32,
    hidden: i32,
) -> bool {
    if !bind_custom(e, d, "ctox_moe_add_shared_bf16") {
        return false;
    }
    e.set_buffer(0, y, 0);
    e.set_buffer(1, shared, 0);
    e.set_buffer(2, shared_gate, 0);
    e.set_bytes(3, &n_tokens);
    e.set_bytes(4, &hidden);
    dispatch_1d(e, n_tokens * hidden);
    true
}

// ═══════════════════════════════════════════════════════════════════
//  dflash-specific dispatches — byte-exact kernels, no port pending
// ═══════════════════════════════════════════════════════════════════
//
// These dispatch against the 6 vendored dflash shaders under
// `vendor/metal/shaders/dflash/`, which are themselves byte-exact
// extractions from the Python reference's `kernels.py` +
// `verify_qmm.py` (pinned in `vendor/metal/dflash-mlx.version`).
// No ggml-metal port affects them.

#[allow(clippy::too_many_arguments)]
pub fn gated_delta_tape_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    has_mask: bool,
    vectorized: bool,
    q: &Buffer,
    k: &Buffer,
    v: &Buffer,
    g: &Buffer,
    beta: &Buffer,
    state_in: &Buffer,
    t: i32,
    mask: Option<&Buffer>,
    y: &Buffer,
    state_out: &Buffer,
    innovation_tape: &Buffer,
    _state_roundtrip: bool,
    b: usize,
    hk: usize,
    hv: usize,
    dk: usize,
    dv: usize,
) -> bool {
    let name = match (vectorized, has_mask) {
        (false, false) => "gated_delta_tape",
        (false, true) => "gated_delta_tape_mask",
        (true, false) => "gated_delta_tape_vec",
        (true, true) => "gated_delta_tape_vec_mask",
    };
    let cache_key = format!("{name}_dk={dk}_dv={dv}_hk={hk}_hv={hv}");
    let Some(pso) = dev.pipeline_with_constants(&cache_key, name, |cv| {
        mlx_ops::cv_set_int32(cv, dk as i32, 0);
        mlx_ops::cv_set_int32(cv, dv as i32, 1);
        mlx_ops::cv_set_int32(cv, hk as i32, 2);
        mlx_ops::cv_set_int32(cv, hv as i32, 3);
    }) else {
        set_last_error(format!("gated_delta_tape: pipeline `{name}` not found"));
        return false;
    };
    enc.set_pipeline(&pso);
    enc.set_buffer(0, q, 0);
    enc.set_buffer(1, k, 0);
    enc.set_buffer(2, v, 0);
    enc.set_buffer(3, g, 0);
    enc.set_buffer(4, beta, 0);
    enc.set_buffer(5, state_in, 0);
    enc.set_bytes(6, &t);
    if has_mask {
        if let Some(m) = mask {
            enc.set_buffer(7, m, 0);
        } else {
            set_last_error("gated_delta_tape: has_mask=true but mask buffer is None");
            return false;
        }
    }
    enc.set_buffer(8, y, 0);
    enc.set_buffer(9, state_out, 0);
    enc.set_buffer(10, innovation_tape, 0);
    enc.dispatch((32, dv, b * hv), (32, 4, 1));
    true
}

#[allow(clippy::too_many_arguments)]
pub fn gated_delta_f32_state_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    has_mask: bool,
    vectorized: bool,
    q: &Buffer,
    k: &Buffer,
    v: &Buffer,
    g: &Buffer,
    beta: &Buffer,
    state_in: &Buffer,
    t: i32,
    mask: Option<&Buffer>,
    y: &Buffer,
    state_out: &Buffer,
    b: usize,
    hk: usize,
    hv: usize,
    dk: usize,
    dv: usize,
) -> bool {
    let name = match (vectorized, has_mask) {
        (false, false) => "gated_delta_f32_state",
        (false, true) => "gated_delta_f32_state_mask",
        (true, false) => "gated_delta_f32_state_vec",
        (true, true) => "gated_delta_f32_state_vec_mask",
    };
    let cache_key = format!("{name}_dk={dk}_dv={dv}_hk={hk}_hv={hv}");
    let Some(pso) = dev.pipeline_with_constants(&cache_key, name, |cv| {
        mlx_ops::cv_set_int32(cv, dk as i32, 0);
        mlx_ops::cv_set_int32(cv, dv as i32, 1);
        mlx_ops::cv_set_int32(cv, hk as i32, 2);
        mlx_ops::cv_set_int32(cv, hv as i32, 3);
    }) else {
        set_last_error(format!("gated_delta_f32_state: pipeline `{name}` not found"));
        return false;
    };
    enc.set_pipeline(&pso);
    enc.set_buffer(0, q, 0);
    enc.set_buffer(1, k, 0);
    enc.set_buffer(2, v, 0);
    enc.set_buffer(3, g, 0);
    enc.set_buffer(4, beta, 0);
    enc.set_buffer(5, state_in, 0);
    enc.set_bytes(6, &t);
    if has_mask {
        if let Some(m) = mask {
            enc.set_buffer(7, m, 0);
        } else {
            set_last_error("gated_delta_f32_state: has_mask=true but mask buffer is None");
            return false;
        }
    }
    enc.set_buffer(8, y, 0);
    enc.set_buffer(9, state_out, 0);
    enc.dispatch((32, dv, b * hv), (32, 4, 1));
    true
}

#[allow(clippy::too_many_arguments)]
pub fn tape_replay_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    has_mask: bool,
    vectorized: bool,
    tape: &Buffer,
    k: &Buffer,
    g: &Buffer,
    state_in: &Buffer,
    t: i32,
    mask: Option<&Buffer>,
    state_out: &Buffer,
    b: usize,
    hk: usize,
    hv: usize,
    dk: usize,
    dv: usize,
) -> bool {
    let name = match (vectorized, has_mask) {
        (false, false) => "tape_replay",
        (false, true) => "tape_replay_mask",
        (true, false) => "tape_replay_vec",
        (true, true) => "tape_replay_vec_mask",
    };
    let cache_key = format!("{name}_dk={dk}_dv={dv}_hk={hk}_hv={hv}");
    let Some(pso) = dev.pipeline_with_constants(&cache_key, name, |cv| {
        mlx_ops::cv_set_int32(cv, dk as i32, 0);
        mlx_ops::cv_set_int32(cv, dv as i32, 1);
        mlx_ops::cv_set_int32(cv, hk as i32, 2);
        mlx_ops::cv_set_int32(cv, hv as i32, 3);
    }) else {
        set_last_error(format!("tape_replay: pipeline `{name}` not found"));
        return false;
    };
    enc.set_pipeline(&pso);
    enc.set_buffer(0, tape, 0);
    enc.set_buffer(1, k, 0);
    enc.set_buffer(2, g, 0);
    enc.set_buffer(3, state_in, 0);
    enc.set_bytes(4, &t);
    if has_mask {
        if let Some(m) = mask {
            enc.set_buffer(5, m, 0);
        } else {
            set_last_error("tape_replay: has_mask=true but mask buffer is None");
            return false;
        }
    }
    enc.set_buffer(6, state_out, 0);
    enc.dispatch((32, dv, b * hv), (32, 4, 1));
    true
}

#[allow(clippy::too_many_arguments)]
pub fn sdpa_2pass_partials_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    has_mask: bool,
    queries: &Buffer,
    keys: &Buffer,
    values: &Buffer,
    gqa_factor: i32,
    n_kv: i32,
    k_head_stride: i32,
    k_seq_stride: i32,
    v_head_stride: i32,
    v_seq_stride: i32,
    scale: f32,
    blocks: i32,
    mask: Option<&Buffer>,
    partials: &Buffer,
    sums: &Buffer,
    maxs: &Buffer,
    h_q: usize,
    b: usize,
    q_len: usize,
) -> bool {
    let name = if has_mask {
        "batched_sdpa_2pass_partials_mask"
    } else {
        "batched_sdpa_2pass_partials"
    };
    let d = k_seq_stride;
    let v = v_seq_stride;
    let hk = (h_q as i32) / gqa_factor.max(1);
    let m_fixed = q_len as i32;
    let cache_key = format!("{name}_d={d}_v={v}_hk={hk}_m={m_fixed}");
    let Some(pso) = dev.pipeline_with_constants(&cache_key, name, |cv| {
        mlx_ops::cv_set_int32(cv, hk, 2);
        mlx_ops::cv_set_int32(cv, d, 4);
        mlx_ops::cv_set_int32(cv, v, 5);
        mlx_ops::cv_set_int32(cv, m_fixed, 6);
    }) else {
        set_last_error(format!("sdpa_2pass_partials: pipeline `{name}` not found"));
        return false;
    };
    enc.set_pipeline(&pso);
    enc.set_buffer(0, queries, 0);
    enc.set_buffer(1, keys, 0);
    enc.set_buffer(2, values, 0);
    enc.set_bytes(3, &gqa_factor);
    enc.set_bytes(4, &n_kv);
    enc.set_bytes(5, &k_head_stride);
    enc.set_bytes(6, &k_seq_stride);
    enc.set_bytes(7, &v_head_stride);
    enc.set_bytes(8, &v_seq_stride);
    enc.set_bytes(9, &scale);
    enc.set_bytes(10, &blocks);
    if has_mask {
        if let Some(m) = mask {
            enc.set_buffer(11, m, 0);
        } else {
            set_last_error("sdpa_2pass_partials: has_mask=true but mask buffer is None");
            return false;
        }
    }
    enc.set_buffer(12, partials, 0);
    enc.set_buffer(13, sums, 0);
    enc.set_buffer(14, maxs, 0);
    enc.dispatch((h_q * 32, b, (blocks as usize) * q_len), (32, 1, q_len));
    true
}

pub fn sdpa_2pass_reduce_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    partials: &Buffer,
    sums: &Buffer,
    maxs: &Buffer,
    blocks: i32,
    out: &Buffer,
    bh: usize,
    q_len: usize,
    vdim: i32,
) -> bool {
    let v = vdim;
    let m_fixed = q_len as i32;
    let cache_key = format!("batched_sdpa_2pass_reduce_v={v}_m={m_fixed}");
    let Some(pso) = dev.pipeline_with_constants(&cache_key, "batched_sdpa_2pass_reduce", |cv| {
        mlx_ops::cv_set_int32(cv, v, 5);
        mlx_ops::cv_set_int32(cv, m_fixed, 6);
    }) else {
        set_last_error("sdpa_2pass_reduce: pipeline `batched_sdpa_2pass_reduce` not found");
        return false;
    };
    enc.set_pipeline(&pso);
    enc.set_buffer(0, partials, 0);
    enc.set_buffer(1, sums, 0);
    enc.set_buffer(2, maxs, 0);
    enc.set_bytes(3, &blocks);
    enc.set_buffer(4, out, 0);
    enc.dispatch((bh * 1024, q_len, 1), (1024, 1, 1));
    true
}

#[allow(clippy::too_many_arguments)]
pub fn verify_qmm_mma2big_gs64_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    x: &Buffer,
    w_q: &Buffer,
    scales: &Buffer,
    biases: &Buffer,
    y: &Buffer,
    m: i32,
    k: i32,
    n: i32,
) -> bool {
    if !bind_pipeline(enc, dev, "verify_mma2big_gs64_bf16") {
        return false;
    }
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w_q, 0);
    enc.set_buffer(2, scales, 0);
    enc.set_buffer(3, biases, 0);
    enc.set_bytes(4, &m);
    enc.set_bytes(5, &k);
    enc.set_bytes(6, &n);
    enc.set_buffer(7, y, 0);
    let tg_count_n = ((n + 31) / 32) as usize;
    enc.dispatch_threadgroups((1, tg_count_n, 1), (64, 1, 1));
    true
}

#[allow(clippy::too_many_arguments)]
pub fn verify_qmm_mma2big_pipe_gs64_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    x: &Buffer,
    w_q: &Buffer,
    scales: &Buffer,
    biases: &Buffer,
    partials: &Buffer,
    m: i32,
    k: i32,
    n: i32,
    k_parts: i32,
) -> bool {
    if !bind_pipeline(enc, dev, "verify_mma2big_pipe_gs64_bf16") {
        return false;
    }
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w_q, 0);
    enc.set_buffer(2, scales, 0);
    enc.set_buffer(3, biases, 0);
    enc.set_bytes(4, &m);
    enc.set_bytes(5, &k);
    enc.set_bytes(6, &n);
    enc.set_bytes(7, &k_parts);
    enc.set_buffer(8, partials, 0);
    let tg_count_n = ((n + 31) / 32) as usize;
    enc.dispatch_threadgroups((1, tg_count_n, k_parts as usize), (64, 1, 1));
    true
}

pub fn verify_qmm_reduce_partials_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    partials: &Buffer,
    y: &Buffer,
    k_parts: i32,
    n: i32,
) -> bool {
    if !bind_custom(enc, dev, "ctox_verify_qmm_reduce_partials_bf16") {
        return false;
    }
    enc.set_buffer(0, partials, 0);
    enc.set_buffer(1, y, 0);
    enc.set_bytes(2, &k_parts);
    enc.set_bytes(3, &n);
    dispatch_1d(enc, 16 * n);
    true
}
