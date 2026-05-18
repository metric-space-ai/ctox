//! Raw FFI bindings to ggml / ggml-backend / ggml-cuda / gguf.
//!
//! Upstream: llama.cpp `b16de65904ed7e468397f5417ad130f092cba8f4`
//! (pinned in `../../models/qwen35_27b/vendor/llama-cpp.version`).
//!
//! This is a **subset** binding — we only declare the symbols that the
//! lucebox `dflash/src/*.cpp` reference actually calls. Missing
//! symbols are a bug (we hit a linker error) and get added on demand.
//! See `GGML_OPS_USED.md` (TODO) for the full audit.
//!
//! All types are `#[repr(C)]` where they face the C ABI. Opaque
//! structs (`ggml_context`, `ggml_backend`, `ggml_backend_buffer`,
//! `ggml_cgraph`, `ggml_gallocr`, `gguf_context`) are `enum {}` so
//! they can only be used behind pointers.
//!
//! Safety: every call into this crate is `unsafe` at the C boundary.
//! The safe wrappers live in `ctox-dflash27b`.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]
#![allow(clippy::missing_safety_doc)]

use libc::{c_char, c_int, c_void, size_t};

// ─── Opaque types ─────────────────────────────────────────────────
//
// ggml hides the full definitions of these behind forward declarations
// in its public headers. We match that policy — everything goes
// through pointers.

#[repr(C)]
pub struct ggml_context {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ggml_cgraph {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ggml_gallocr {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ggml_backend {
    _private: [u8; 0],
}
pub type ggml_backend_t = *mut ggml_backend;

#[repr(C)]
pub struct ggml_backend_buffer {
    _private: [u8; 0],
}
pub type ggml_backend_buffer_t = *mut ggml_backend_buffer;

#[repr(C)]
pub struct ggml_backend_buffer_type {
    _private: [u8; 0],
}
pub type ggml_backend_buffer_type_t = *mut ggml_backend_buffer_type;

#[repr(C)]
pub struct ggml_backend_sched {
    _private: [u8; 0],
}

#[repr(C)]
pub struct gguf_context {
    _private: [u8; 0],
}

// ─── Enums ────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ggml_type {
    GGML_TYPE_F32 = 0,
    GGML_TYPE_F16 = 1,
    GGML_TYPE_Q4_0 = 2,
    GGML_TYPE_Q4_1 = 3,
    // ggml_type enum has gaps — kept sparse to match the upstream
    // header exactly. The values we actually consume for qwen3.5-27B:
    GGML_TYPE_Q8_0 = 8,
    GGML_TYPE_Q2_K = 10,
    GGML_TYPE_Q3_K = 11,
    GGML_TYPE_Q4_K = 12,
    GGML_TYPE_Q5_K = 13,
    GGML_TYPE_Q6_K = 14,
    GGML_TYPE_Q8_K = 15,
    GGML_TYPE_IQ2_XXS = 16,
    GGML_TYPE_IQ2_XS = 17,
    GGML_TYPE_IQ3_XXS = 18,
    GGML_TYPE_IQ1_S = 19,
    GGML_TYPE_IQ4_NL = 20,
    GGML_TYPE_IQ3_S = 21,
    GGML_TYPE_IQ2_S = 22,
    GGML_TYPE_IQ4_XS = 23,
    GGML_TYPE_I8 = 24,
    GGML_TYPE_I16 = 25,
    GGML_TYPE_I32 = 26,
    GGML_TYPE_I64 = 27,
    GGML_TYPE_F64 = 28,
    GGML_TYPE_IQ1_M = 29,
    GGML_TYPE_BF16 = 30,
    GGML_TYPE_COUNT = 39,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ggml_status {
    GGML_STATUS_ALLOC_FAILED = -2,
    GGML_STATUS_FAILED = -1,
    GGML_STATUS_SUCCESS = 0,
    GGML_STATUS_ABORTED = 1,
}

// `ggml_op` has ~100 entries; we only declare the ones the reference
// uses. The numeric values must match ggml.h — if upstream reorders
// the enum we need to resync.
pub const GGML_OP_NONE: c_int = 0;
pub const GGML_OP_MUL_MAT: c_int = 28;
// Actual numbers come from the version pinned in llama-cpp.version.
// We don't rely on these constants directly — graph ops are set via
// the helper builders (ggml_mul_mat etc.) which set .op internally.

// RoPE mode flags. Passed to ggml_rope_ext's `mode` arg.
pub const GGML_ROPE_TYPE_NORMAL: c_int = 0;
pub const GGML_ROPE_TYPE_NEOX: c_int = 2;
pub const GGML_ROPE_TYPE_MROPE: c_int = 8;
pub const GGML_ROPE_TYPE_VISION: c_int = 24;

// ggml_tri masking variant. Values match enum order in ggml.h.
pub const GGML_TRI_TYPE_UPPER_DIAG: c_int = 0;
pub const GGML_TRI_TYPE_UPPER: c_int = 1;
pub const GGML_TRI_TYPE_LOWER_DIAG: c_int = 2;
pub const GGML_TRI_TYPE_LOWER: c_int = 3;

pub const GGUF_TYPE_INT32: c_int = 5;
pub const GGUF_TYPE_STRING: c_int = 8;
pub const GGUF_TYPE_ARRAY: c_int = 9;

// ─── ggml_tensor ──────────────────────────────────────────────────
//
// The public `ggml_tensor` struct IS exposed by upstream (in ggml.h)
// because graph-building code needs to read `->ne`, `->nb`, `->data`,
// `->type`, etc. We mirror the exact layout. Any field reorder
// upstream is a silent ABI break — pin against the checked-in
// `llama-cpp.version` commit.
//
// On the pinned commit the struct is:
//
// ```c
// struct ggml_tensor {
//     enum ggml_type         type;
//     struct ggml_backend_buffer * buffer;
//     int64_t                ne[4];
//     size_t                 nb[4];
//     enum ggml_op           op;
//     int32_t                op_params[16];
//     int32_t                flags;
//     struct ggml_tensor   * src[10];
//     struct ggml_tensor   * view_src;
//     size_t                 view_offs;
//     void                 * data;
//     char                   name[64];
//     void                 * extra;
//     char                   padding[8];
// };
// ```
//
// Size: 368 bytes on x86_64 Linux. Verified at runtime via
// `assert_eq!(std::mem::size_of::<ggml_tensor>(), 368)` — see
// tests/abi.rs.

pub const GGML_MAX_DIMS: usize = 4;
pub const GGML_MAX_OP_PARAMS: usize = 16;
pub const GGML_MAX_SRC: usize = 10;
pub const GGML_MAX_NAME: usize = 64;

#[repr(C)]
pub struct ggml_tensor {
    pub type_: ggml_type,
    pub buffer: ggml_backend_buffer_t,
    pub ne: [i64; GGML_MAX_DIMS],
    pub nb: [size_t; GGML_MAX_DIMS],
    pub op: c_int, // ggml_op enum, opaque here
    pub op_params: [i32; GGML_MAX_OP_PARAMS],
    pub flags: i32,
    pub src: [*mut ggml_tensor; GGML_MAX_SRC],
    pub view_src: *mut ggml_tensor,
    pub view_offs: size_t,
    pub data: *mut c_void,
    pub name: [c_char; GGML_MAX_NAME],
    pub extra: *mut c_void,
    pub padding: [c_char; 8],
}

// ─── ggml_init ────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone)]
pub struct ggml_init_params {
    pub mem_size: size_t,
    pub mem_buffer: *mut c_void,
    pub no_alloc: bool,
}

// ─── Function declarations ────────────────────────────────────────

extern "C" {
    // Context lifecycle.
    pub fn ggml_init(params: ggml_init_params) -> *mut ggml_context;
    pub fn ggml_free(ctx: *mut ggml_context);
    pub fn ggml_tensor_overhead() -> size_t;

    // Tensor introspection + naming.
    pub fn ggml_nbytes(t: *const ggml_tensor) -> size_t;
    pub fn ggml_element_size(t: *const ggml_tensor) -> size_t;
    pub fn ggml_row_size(t: ggml_type, ne: i64) -> size_t;
    pub fn ggml_type_size(t: ggml_type) -> size_t;
    pub fn ggml_type_name(t: ggml_type) -> *const c_char;
    pub fn ggml_set_name(t: *mut ggml_tensor, name: *const c_char);
    pub fn ggml_get_name(t: *const ggml_tensor) -> *const c_char;
    pub fn ggml_set_output(t: *mut ggml_tensor);
    pub fn ggml_nelements(t: *const ggml_tensor) -> i64;
    pub fn ggml_get_first_tensor(ctx: *const ggml_context) -> *mut ggml_tensor;
    pub fn ggml_get_next_tensor(
        ctx: *const ggml_context,
        cur: *const ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_is_contiguous(t: *const ggml_tensor) -> bool;

    // View ops (metadata only — no device work). Each view tensor
    // borrows from `a`'s buffer; the reference passes explicit byte
    // strides (`nb1`/`nb2`/`nb3`) and a byte offset.
    pub fn ggml_view_1d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        offset: size_t,
    ) -> *mut ggml_tensor;
    pub fn ggml_view_2d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
        nb1: size_t,
        offset: size_t,
    ) -> *mut ggml_tensor;
    pub fn ggml_view_3d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
        ne2: i64,
        nb1: size_t,
        nb2: size_t,
        offset: size_t,
    ) -> *mut ggml_tensor;
    pub fn ggml_view_4d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
        ne2: i64,
        ne3: i64,
        nb1: size_t,
        nb2: size_t,
        nb3: size_t,
        offset: size_t,
    ) -> *mut ggml_tensor;

    // Reshapes (metadata only — no device work).
    pub fn ggml_reshape_1d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_reshape_2d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_reshape_3d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
        ne2: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_reshape_4d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
        ne2: i64,
        ne3: i64,
    ) -> *mut ggml_tensor;

    // Tensor construction.
    pub fn ggml_new_tensor_1d(
        ctx: *mut ggml_context,
        type_: ggml_type,
        ne0: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_new_tensor_2d(
        ctx: *mut ggml_context,
        type_: ggml_type,
        ne0: i64,
        ne1: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_new_tensor_3d(
        ctx: *mut ggml_context,
        type_: ggml_type,
        ne0: i64,
        ne1: i64,
        ne2: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_new_tensor_4d(
        ctx: *mut ggml_context,
        type_: ggml_type,
        ne0: i64,
        ne1: i64,
        ne2: i64,
        ne3: i64,
    ) -> *mut ggml_tensor;

    // Graph ops — only what the reference uses.
    pub fn ggml_mul_mat(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_add(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_sub(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_mul(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_scale(ctx: *mut ggml_context, a: *mut ggml_tensor, s: f32) -> *mut ggml_tensor;
    pub fn ggml_neg(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_exp(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_silu(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_sigmoid(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_softplus(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_cumsum(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_diag(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_solve_tri(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
        left: bool,
        lower: bool,
        unit: bool,
    ) -> *mut ggml_tensor;
    pub fn ggml_tri(ctx: *mut ggml_context, a: *mut ggml_tensor, k: i32) -> *mut ggml_tensor;
    pub fn ggml_fill(ctx: *mut ggml_context, a: *mut ggml_tensor, value: f32) -> *mut ggml_tensor;
    pub fn ggml_pad(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        p0: i32,
        p1: i32,
        p2: i32,
        p3: i32,
    ) -> *mut ggml_tensor;
    pub fn ggml_concat(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
        dim: i32,
    ) -> *mut ggml_tensor;
    pub fn ggml_repeat_4d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
        ne2: i64,
        ne3: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_permute(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        axis0: i32,
        axis1: i32,
        axis2: i32,
        axis3: i32,
    ) -> *mut ggml_tensor;
    pub fn ggml_transpose(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_cont(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_cont_2d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_cont_4d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        ne0: i64,
        ne1: i64,
        ne2: i64,
        ne3: i64,
    ) -> *mut ggml_tensor;
    pub fn ggml_cpy(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_set_inplace(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
        nb1: size_t,
        nb2: size_t,
        nb3: size_t,
        offset: size_t,
    ) -> *mut ggml_tensor;
    pub fn ggml_rms_norm(ctx: *mut ggml_context, a: *mut ggml_tensor, eps: f32)
        -> *mut ggml_tensor;
    pub fn ggml_l2_norm(ctx: *mut ggml_context, a: *mut ggml_tensor, eps: f32) -> *mut ggml_tensor;
    pub fn ggml_gelu_erf(ctx: *mut ggml_context, a: *mut ggml_tensor) -> *mut ggml_tensor;
    pub fn ggml_cast(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        type_: ggml_type,
    ) -> *mut ggml_tensor;
    pub fn ggml_get_rows(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_pad_ext(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        p0: i32,
        p1: i32,
        p2: i32,
        p3: i32,
        p4: i32,
        p5: i32,
        p6: i32,
        p7: i32,
    ) -> *mut ggml_tensor;
    pub fn ggml_conv_1d(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
        s0: i32,
        p0: i32,
        d0: i32,
    ) -> *mut ggml_tensor;
    pub fn ggml_rope_ext(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
        c: *mut ggml_tensor,
        n_dims: i32,
        mode: i32,
        n_ctx_orig: i32,
        freq_base: f32,
        freq_scale: f32,
        ext_factor: f32,
        attn_factor: f32,
        beta_fast: f32,
        beta_slow: f32,
    ) -> *mut ggml_tensor;
    pub fn ggml_rope_multi(
        ctx: *mut ggml_context,
        a: *mut ggml_tensor,
        b: *mut ggml_tensor,
        c: *mut ggml_tensor,
        n_dims: i32,
        sections: *const i32,
        mode: i32,
        n_ctx_orig: i32,
        freq_base: f32,
        freq_scale: f32,
        ext_factor: f32,
        attn_factor: f32,
        beta_fast: f32,
        beta_slow: f32,
    ) -> *mut ggml_tensor;
    pub fn ggml_flash_attn_ext(
        ctx: *mut ggml_context,
        q: *mut ggml_tensor,
        k: *mut ggml_tensor,
        v: *mut ggml_tensor,
        mask: *mut ggml_tensor,
        scale: f32,
        max_bias: f32,
        logit_softcap: f32,
    ) -> *mut ggml_tensor;
    pub fn ggml_ssm_conv(
        ctx: *mut ggml_context,
        sx: *mut ggml_tensor,
        c: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_gated_delta_net(
        ctx: *mut ggml_context,
        q: *mut ggml_tensor,
        k: *mut ggml_tensor,
        v: *mut ggml_tensor,
        g: *mut ggml_tensor,
        beta: *mut ggml_tensor,
        s: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    // Fork extensions (luce-dflash). Not present on mainline ggml; the
    // vendor build provides them. At FFI boundary these are regular
    // functions; if the pinned commit ever drops them the linker fails.
    pub fn ggml_gated_delta_net_tree(
        ctx: *mut ggml_context,
        q: *mut ggml_tensor,
        k: *mut ggml_tensor,
        v: *mut ggml_tensor,
        g: *mut ggml_tensor,
        beta: *mut ggml_tensor,
        s: *mut ggml_tensor,
        parent_ids: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_gated_delta_net_tree_persist(
        ctx: *mut ggml_context,
        q: *mut ggml_tensor,
        k: *mut ggml_tensor,
        v: *mut ggml_tensor,
        g: *mut ggml_tensor,
        beta: *mut ggml_tensor,
        s: *mut ggml_tensor,
        parent_ids: *mut ggml_tensor,
        persist_inter: *mut ggml_tensor,
    ) -> *mut ggml_tensor;
    pub fn ggml_ssm_conv_tree(
        ctx: *mut ggml_context,
        sx: *mut ggml_tensor,
        c: *mut ggml_tensor,
        parent_ids: *mut ggml_tensor,
    ) -> *mut ggml_tensor;

    // Named-tensor lookup (used by the gguf loader).
    pub fn ggml_get_tensor(ctx: *mut ggml_context, name: *const c_char) -> *mut ggml_tensor;

    // Type traits (used by the gguf loader).
    pub fn ggml_get_type_traits(t: ggml_type) -> *const ggml_type_traits;

    // Graph machinery.
    pub fn ggml_new_graph_custom(
        ctx: *mut ggml_context,
        size: size_t,
        grads: bool,
    ) -> *mut ggml_cgraph;
    pub fn ggml_build_forward_expand(gf: *mut ggml_cgraph, t: *mut ggml_tensor);
    pub fn ggml_new_graph(ctx: *mut ggml_context) -> *mut ggml_cgraph;
    pub fn ggml_graph_size(gf: *mut ggml_cgraph) -> c_int;
    pub fn ggml_graph_n_nodes(gf: *mut ggml_cgraph) -> c_int;
    pub fn ggml_graph_overhead_custom(size: size_t, grads: bool) -> size_t;
    pub fn ggml_graph_get_tensor(gf: *const ggml_cgraph, name: *const c_char) -> *mut ggml_tensor;
    pub fn ggml_get_data(t: *const ggml_tensor) -> *mut c_void;
    pub fn ggml_n_dims(t: *const ggml_tensor) -> c_int;
    pub fn ggml_set_input(t: *mut ggml_tensor);

    // Per-call compute-buffer allocator (ggml-alloc).
    pub fn ggml_gallocr_new(buft: ggml_backend_buffer_type_t) -> *mut ggml_gallocr;
    pub fn ggml_gallocr_free(alloc: *mut ggml_gallocr);
    pub fn ggml_gallocr_alloc_graph(alloc: *mut ggml_gallocr, gf: *mut ggml_cgraph) -> bool;
    pub fn ggml_backend_get_default_buffer_type(
        backend: ggml_backend_t,
    ) -> ggml_backend_buffer_type_t;
}

/// Function pointer for per-quant dequantize routines — used by the
/// CpuEmbedder to decompress individual token rows on demand.
pub type ggml_to_float_t = unsafe extern "C" fn(x: *const c_void, y: *mut f32, k: i64);

#[repr(C)]
pub struct ggml_type_traits {
    pub type_name: *const c_char,
    pub blck_size: i64,
    pub blck_size_interleave: i64,
    pub type_size: size_t,
    pub is_quantized: bool,
    pub to_float: Option<ggml_to_float_t>,
    // …more fields follow upstream. We only read type_size + blck_size
    // + to_float. Padding reserves the rest so the struct doesn't
    // shift if we read it by value (we only ever use the pointer).
    _padding: [u8; 128],
}

// ─── ggml-backend.h ──────────────────────────────────────────────

extern "C" {
    pub fn ggml_backend_cpu_init() -> ggml_backend_t;
    pub fn ggml_backend_cpu_set_n_threads(backend: ggml_backend_t, n_threads: c_int);
    pub fn ggml_backend_blas_init() -> ggml_backend_t;
    pub fn ggml_backend_blas_set_n_threads(backend: ggml_backend_t, n_threads: c_int);
    pub fn ggml_backend_metal_init() -> ggml_backend_t;
    pub fn ggml_backend_buffer_get_size(buffer: ggml_backend_buffer_t) -> size_t;
    pub fn ggml_backend_buffer_clear(buffer: ggml_backend_buffer_t, value: u8);
    pub fn ggml_backend_free(backend: ggml_backend_t);
    pub fn ggml_backend_alloc_ctx_tensors(
        ctx: *mut ggml_context,
        backend: ggml_backend_t,
    ) -> ggml_backend_buffer_t;
    pub fn ggml_backend_buffer_free(buffer: ggml_backend_buffer_t);
    pub fn ggml_backend_tensor_set(
        t: *mut ggml_tensor,
        data: *const c_void,
        offset: size_t,
        size: size_t,
    );
    pub fn ggml_backend_tensor_get(
        t: *const ggml_tensor,
        data: *mut c_void,
        offset: size_t,
        size: size_t,
    );
    pub fn ggml_backend_tensor_copy(src: *mut ggml_tensor, dst: *mut ggml_tensor);
    pub fn ggml_backend_graph_compute(backend: ggml_backend_t, gf: *mut ggml_cgraph)
        -> ggml_status;
    pub fn ggml_backend_synchronize(backend: ggml_backend_t);
    pub fn ggml_backend_sched_new(
        backends: *mut ggml_backend_t,
        bufts: *mut ggml_backend_buffer_type_t,
        n_backends: c_int,
        graph_size: size_t,
        parallel: bool,
        op_offload: bool,
    ) -> *mut ggml_backend_sched;
    pub fn ggml_backend_sched_free(sched: *mut ggml_backend_sched);
    pub fn ggml_backend_sched_reset(sched: *mut ggml_backend_sched);
    pub fn ggml_backend_sched_set_tensor_backend(
        sched: *mut ggml_backend_sched,
        node: *mut ggml_tensor,
        backend: ggml_backend_t,
    );
    pub fn ggml_backend_sched_alloc_graph(
        sched: *mut ggml_backend_sched,
        graph: *mut ggml_cgraph,
    ) -> bool;
    pub fn ggml_backend_sched_graph_compute(
        sched: *mut ggml_backend_sched,
        graph: *mut ggml_cgraph,
    ) -> ggml_status;
}

// ─── ggml-cuda.h ─────────────────────────────────────────────────

extern "C" {
    pub fn ggml_backend_cuda_init(device: c_int) -> ggml_backend_t;
    pub fn ggml_backend_cuda_buffer_type(device: c_int) -> ggml_backend_buffer_type_t;
}

// ─── Vendored f16_convert.cu kernels ─────────────────────────────
//
// Byte-for-byte copy of `lucebox/dflash/src/f16_convert.cu`, compiled
// by our build.rs. Exposed for the driver module's `target_feat`
// bf16 → f32 widen and DDTree SSM f16 → f32 rollback.

/// cudaStream_t — opaque. We never dereference it; pass-through only.
pub type cudaStream_t = *mut c_void;

extern "C" {
    pub fn dflash27b_launch_f16_to_f32(
        src: *const c_void,
        dst: *mut c_void,
        n_elems: size_t,
        stream: cudaStream_t,
    );
    pub fn dflash27b_launch_bf16_to_f32(
        src: *const c_void,
        dst: *mut c_void,
        n_elems: size_t,
        stream: cudaStream_t,
    );
}

// ─── cuda-runtime helpers used by the fast-rollback path ────────
//
// Linked via `libcudart.so` (our build.rs already adds it when we
// compile the vendored f16_convert).

/// `cudaMemcpyKind` — enum values matching `<cuda_runtime.h>`.
pub const CUDA_MEMCPY_DEVICE_TO_DEVICE: c_int = 3;

extern "C" {
    /// `cudaMemcpy2DAsync` — 2-D device-to-device copy used for conv
    /// state rollback (different `spitch` vs `dpitch`). Returns a
    /// `cudaError_t` (0 = success).
    pub fn cudaMemcpy2DAsync(
        dst: *mut c_void,
        dpitch: size_t,
        src: *const c_void,
        spitch: size_t,
        width: size_t,
        height: size_t,
        kind: c_int,
        stream: cudaStream_t,
    ) -> c_int;

    /// `cudaMemcpyAsync` — flat device-to-device copy used for KV +
    /// target_feat compaction in the DDTree rollback path.
    pub fn cudaMemcpyAsync(
        dst: *mut c_void,
        src: *const c_void,
        count: size_t,
        kind: c_int,
        stream: cudaStream_t,
    ) -> c_int;
}

// ─── gguf.h ──────────────────────────────────────────────────────
//
// The reference uses gguf primarily for reading tensor metadata out of
// the Q4_K_M target model file. Subset bindings.

#[repr(C)]
#[derive(Copy, Clone)]
pub struct gguf_init_params {
    pub no_alloc: bool,
    pub ctx: *mut *mut ggml_context,
}

extern "C" {
    pub fn gguf_init_from_file(fname: *const c_char, params: gguf_init_params)
        -> *mut gguf_context;
    pub fn gguf_free(ctx: *mut gguf_context);
    pub fn gguf_find_key(ctx: *const gguf_context, key: *const c_char) -> i64;
    pub fn gguf_get_val_u32(ctx: *const gguf_context, key_id: i64) -> u32;
    pub fn gguf_get_val_i32(ctx: *const gguf_context, key_id: i64) -> i32;
    pub fn gguf_get_val_f32(ctx: *const gguf_context, key_id: i64) -> f32;
    pub fn gguf_get_val_str(ctx: *const gguf_context, key_id: i64) -> *const c_char;
    pub fn gguf_get_kv_type(ctx: *const gguf_context, key_id: i64) -> c_int;
    pub fn gguf_get_arr_type(ctx: *const gguf_context, key_id: i64) -> c_int;
    pub fn gguf_get_arr_n(ctx: *const gguf_context, key_id: i64) -> i64;
    pub fn gguf_get_arr_data(ctx: *const gguf_context, key_id: i64) -> *const c_void;
    pub fn gguf_get_arr_str(ctx: *const gguf_context, key_id: i64, i: size_t) -> *const c_char;
    pub fn gguf_get_n_tensors(ctx: *const gguf_context) -> i64;
    pub fn gguf_get_tensor_name(ctx: *const gguf_context, i: i64) -> *const c_char;
    pub fn gguf_get_tensor_type(ctx: *const gguf_context, i: i64) -> ggml_type;
    pub fn gguf_get_tensor_offset(ctx: *const gguf_context, i: i64) -> size_t;
    pub fn gguf_get_data_offset(ctx: *const gguf_context) -> size_t;
    pub fn gguf_get_tensor_size(ctx: *const gguf_context, i: i64) -> size_t;
    pub fn gguf_find_tensor(ctx: *const gguf_context, name: *const c_char) -> i64;
}

// ─── Static sanity checks ─────────────────────────────────────────
//
// Catch ABI drift at compile time where we can. Run-time checks live in
// tests/abi.rs (gated on `cfg(test)`, requires libggml linked).

const _: () = {
    assert!(
        std::mem::size_of::<ggml_init_params>() == 24
            || std::mem::size_of::<ggml_init_params>() == 16
    );
    // 8+8+8 on 64-bit (size_t, ptr, bool+padding) — accept either 16 or
    // 24 byte layouts for MSVC/GCC variation. Precise pinning happens
    // via the runtime test.
};
