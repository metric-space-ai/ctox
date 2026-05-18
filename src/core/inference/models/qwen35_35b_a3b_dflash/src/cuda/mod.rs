//! Linux + CUDA entry point for the Qwen3.5-35B-A3B DFlash engine.
//!
//! This file exists so the curated four-engine source layout is
//! physically present in-tree. It is intentionally not a pretend
//! implementation: the 35B-A3B CUDA target still needs its own
//! vendored CUDA kernels and Rust glue for MoE routing, expert matmul,
//! attention, draft verification, and DFlash rollback.
//!
//! The sibling 27B CUDA code cannot be reused here: 35B-A3B is an MoE
//! target with a different hot path and must remain a separate model
//! implementation per the CTOX curated-model contract.

use std::ffi::{c_int, c_void};

use anyhow::{bail, Result};

pub type CudaStream = *mut c_void;
pub type CudaError = c_int;

extern "C" {
    pub fn ctox_qwen35_35b_rms_norm_bf16_launch(
        x: *const c_void,
        weight: *const c_void,
        y: *mut c_void,
        d: c_int,
        eps: f32,
        weight_bias: f32,
        rows: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_moe_route_topk_bf16_launch(
        router_logits: *const c_void,
        topk_weights: *mut c_void,
        topk_ids: *mut i32,
        top_k: c_int,
        num_experts: c_int,
        n_tokens: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_dense_matmul_bf16_launch(
        x: *const c_void,
        w: *const c_void,
        bias: *const c_void,
        y: *mut c_void,
        rows: c_int,
        in_dim: c_int,
        out_dim: c_int,
        has_bias: bool,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_add_bf16_launch(
        a: *const c_void,
        b: *const c_void,
        y: *mut c_void,
        n: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_mul_bf16_launch(
        a: *const c_void,
        b: *const c_void,
        y: *mut c_void,
        n: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_silu_bf16_launch(
        x: *const c_void,
        y: *mut c_void,
        n: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_argmax_bf16_launch(
        x: *const c_void,
        out: *mut i32,
        vocab: c_int,
        rows: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_copy_hidden_slot_bf16_launch(
        src: *const c_void,
        dst: *mut c_void,
        src_row: c_int,
        dst_slot: c_int,
        hidden: c_int,
        dst_slots: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_repeat_hidden_slots_bf16_launch(
        src: *const c_void,
        dst: *mut c_void,
        hidden: c_int,
        dst_slots: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_fill_positions4_i32_launch(
        out: *mut i32,
        start_pos: c_int,
        n_tokens: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_causal_mask_f16_launch(
        out: *mut c_void,
        kv_start: c_int,
        n_tokens: c_int,
        kv_len: c_int,
        q_stride: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_kv_store_bf16_launch(
        src: *const c_void,
        cache: *mut c_void,
        positions4: *const i32,
        n_tokens: c_int,
        n_kv_heads: c_int,
        head_dim: c_int,
        max_ctx: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_sdpa_decode_bf16_launch(
        q: *const c_void,
        k_cache: *const c_void,
        v_cache: *const c_void,
        out: *mut c_void,
        n_q_heads: c_int,
        n_kv_heads: c_int,
        head_dim: c_int,
        kv_len: c_int,
        max_ctx: c_int,
        scale: f32,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_dequant_q4_k_bf16_launch(
        x: *const c_void,
        y: *mut c_void,
        n_blocks: c_int,
        stream: CudaStream,
    ) -> CudaError;

    pub fn ctox_qwen35_35b_q4_k_matvec_bf16_launch(
        w: *const c_void,
        x: *const c_void,
        y: *mut c_void,
        in_dim: c_int,
        out_dim: c_int,
        stream: CudaStream,
    ) -> CudaError;
}

fn cuda_ok(code: CudaError, what: &'static str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        bail!("{what}: CUDA error {code}")
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn launch_rms_norm_bf16(
    x: *const c_void,
    weight: *const c_void,
    y: *mut c_void,
    d: c_int,
    eps: f32,
    weight_bias: f32,
    rows: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_rms_norm_bf16_launch(x, weight, y, d, eps, weight_bias, rows, stream),
        "ctox_qwen35_35b_rms_norm_bf16_launch",
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn launch_moe_route_topk_bf16(
    router_logits: *const c_void,
    topk_weights: *mut c_void,
    topk_ids: *mut i32,
    top_k: c_int,
    num_experts: c_int,
    n_tokens: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_moe_route_topk_bf16_launch(
            router_logits,
            topk_weights,
            topk_ids,
            top_k,
            num_experts,
            n_tokens,
            stream,
        ),
        "ctox_qwen35_35b_moe_route_topk_bf16_launch",
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn launch_dense_matmul_bf16(
    x: *const c_void,
    w: *const c_void,
    bias: *const c_void,
    y: *mut c_void,
    rows: c_int,
    in_dim: c_int,
    out_dim: c_int,
    has_bias: bool,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_dense_matmul_bf16_launch(
            x, w, bias, y, rows, in_dim, out_dim, has_bias, stream,
        ),
        "ctox_qwen35_35b_dense_matmul_bf16_launch",
    )
}

pub unsafe fn launch_add_bf16(
    a: *const c_void,
    b: *const c_void,
    y: *mut c_void,
    n: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_add_bf16_launch(a, b, y, n, stream),
        "ctox_qwen35_35b_add_bf16_launch",
    )
}

pub unsafe fn launch_mul_bf16(
    a: *const c_void,
    b: *const c_void,
    y: *mut c_void,
    n: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_mul_bf16_launch(a, b, y, n, stream),
        "ctox_qwen35_35b_mul_bf16_launch",
    )
}

pub unsafe fn launch_silu_bf16(
    x: *const c_void,
    y: *mut c_void,
    n: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_silu_bf16_launch(x, y, n, stream),
        "ctox_qwen35_35b_silu_bf16_launch",
    )
}

pub unsafe fn launch_argmax_bf16(
    x: *const c_void,
    out: *mut i32,
    vocab: c_int,
    rows: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_argmax_bf16_launch(x, out, vocab, rows, stream),
        "ctox_qwen35_35b_argmax_bf16_launch",
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn launch_copy_hidden_slot_bf16(
    src: *const c_void,
    dst: *mut c_void,
    src_row: c_int,
    dst_slot: c_int,
    hidden: c_int,
    dst_slots: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_copy_hidden_slot_bf16_launch(
            src, dst, src_row, dst_slot, hidden, dst_slots, stream,
        ),
        "ctox_qwen35_35b_copy_hidden_slot_bf16_launch",
    )
}

pub unsafe fn launch_repeat_hidden_slots_bf16(
    src: *const c_void,
    dst: *mut c_void,
    hidden: c_int,
    dst_slots: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_repeat_hidden_slots_bf16_launch(src, dst, hidden, dst_slots, stream),
        "ctox_qwen35_35b_repeat_hidden_slots_bf16_launch",
    )
}

pub unsafe fn launch_fill_positions4_i32(
    out: *mut i32,
    start_pos: c_int,
    n_tokens: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_fill_positions4_i32_launch(out, start_pos, n_tokens, stream),
        "ctox_qwen35_35b_fill_positions4_i32_launch",
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn launch_causal_mask_f16(
    out: *mut c_void,
    kv_start: c_int,
    n_tokens: c_int,
    kv_len: c_int,
    q_stride: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_causal_mask_f16_launch(out, kv_start, n_tokens, kv_len, q_stride, stream),
        "ctox_qwen35_35b_causal_mask_f16_launch",
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn launch_kv_store_bf16(
    src: *const c_void,
    cache: *mut c_void,
    positions4: *const i32,
    n_tokens: c_int,
    n_kv_heads: c_int,
    head_dim: c_int,
    max_ctx: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_kv_store_bf16_launch(
            src, cache, positions4, n_tokens, n_kv_heads, head_dim, max_ctx, stream,
        ),
        "ctox_qwen35_35b_kv_store_bf16_launch",
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn launch_sdpa_decode_bf16(
    q: *const c_void,
    k_cache: *const c_void,
    v_cache: *const c_void,
    out: *mut c_void,
    n_q_heads: c_int,
    n_kv_heads: c_int,
    head_dim: c_int,
    kv_len: c_int,
    max_ctx: c_int,
    scale: f32,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_sdpa_decode_bf16_launch(
            q, k_cache, v_cache, out, n_q_heads, n_kv_heads, head_dim, kv_len, max_ctx, scale,
            stream,
        ),
        "ctox_qwen35_35b_sdpa_decode_bf16_launch",
    )
}

pub unsafe fn launch_dequant_q4_k_bf16(
    x: *const c_void,
    y: *mut c_void,
    n_blocks: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_dequant_q4_k_bf16_launch(x, y, n_blocks, stream),
        "ctox_qwen35_35b_dequant_q4_k_bf16_launch",
    )
}

pub unsafe fn launch_q4_k_matvec_bf16(
    w: *const c_void,
    x: *const c_void,
    y: *mut c_void,
    in_dim: c_int,
    out_dim: c_int,
    stream: CudaStream,
) -> Result<()> {
    cuda_ok(
        ctox_qwen35_35b_q4_k_matvec_bf16_launch(w, x, y, in_dim, out_dim, stream),
        "ctox_qwen35_35b_q4_k_matvec_bf16_launch",
    )
}

/// Returns whether this backend is ready for real inference.
pub const fn is_implemented() -> bool {
    false
}

/// Hard fail for callers that accidentally route 35B-A3B requests to
/// the CUDA backend before the bare-metal port lands.
pub fn ensure_implemented() -> Result<()> {
    bail!(
        "qwen35-35b-a3b-dflash CUDA backend is not implemented yet; \
         only the initial owned glue kernels are vendored/compiled so far"
    )
}
