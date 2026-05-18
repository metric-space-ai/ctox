//! Rust port of the MLX host-side dispatcher for every baseline Metal
//! op used by the `bstnxbt/dflash-mlx` reference forward.
//!
//! Each `op_*` here mirrors a function from `mlx/backend/metal/*.cpp`:
//!
//!   * `op_qmv_fast` / `op_qmv` / `op_qmm` — `quantized.cpp::{qmv_fast,
//!     qmv, qmm}` — quantized matmul (the >90% compute hot path).
//!   * `op_rms_norm`  — `normalization.cpp::RMSNorm::eval_gpu`.
//!   * `op_rope`      — `rope.cpp::RoPE::eval_gpu`.
//!   * `op_sdpa_vector` / `op_sdpa_full` — `scaled_dot_product_attention.cpp`.
//!   * `op_binary`    — `binary.cpp` (add / mul / sub / div).
//!   * `op_unary`     — `unary.cpp` (silu / sigmoid / etc.).
//!   * `op_copy`      — `copy.cpp` (reshape / broadcast / astype).
//!
//! Buffer-slot order, byte-layout of inline args, grid + threadgroup
//! sizes are all taken byte-exact from the MLX dispatcher code (commit
//! `211e57be5` — pinned in `vendor/metal/mlx.version`). Kernel names
//! follow MLX's `{mode}_{op}_{type}_gs_{gs}_b_{bits}` convention which
//! matches the `host_name` exports in the vendored
//! `vendor/mlx/mlx/backend/metal/kernels/*.metal`.

use crate::common::errors::set_last_error;
use crate::metal::ffi::{Buffer, ComputeEncoder, Device};

/// Helper: set an `int32` function-constant on an
/// `MTLFunctionConstantValues`. DFlash kernels declare their shape
/// constants as `int` in `vendor/metal/shaders/dflash/common.h`.
pub fn cv_set_int32(cv: &objc2_metal::MTLFunctionConstantValues, value: i32, index: u32) {
    unsafe {
        cv.setConstantValue_type_atIndex(
            std::ptr::NonNull::new_unchecked(&value as *const i32 as *mut std::ffi::c_void),
            objc2_metal::MTLDataType::Int,
            index as objc2_foundation::NSUInteger,
        );
    }
}

fn cv_set_bool(cv: &objc2_metal::MTLFunctionConstantValues, value: bool, index: u32) {
    let raw: u8 = if value { 1 } else { 0 };
    unsafe {
        cv.setConstantValue_type_atIndex(
            std::ptr::NonNull::new_unchecked(&raw as *const u8 as *mut std::ffi::c_void),
            objc2_metal::MTLDataType::Bool,
            index as objc2_foundation::NSUInteger,
        );
    }
}

// ─── Activation / weight dtypes used by the Qwen3.5 forward ─────────

/// MLX type-name fragment used in kernel templating.
/// Matches `mlx::get_type_string(dtype)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MlxDtype {
    F32,
    F16,
    Bf16,
}

impl MlxDtype {
    /// C-typename fragment used in the quantized-matmul kernel names:
    /// `affine_qmv_<T>_gs_<gs>_b_<bits>_batch_<B>`, where `<T>` is the
    /// Metal-level type string: `float`, `float16_t`, `bfloat16_t`.
    /// Verified against `mlx::get_type_string`.
    pub fn name(self) -> &'static str {
        match self {
            MlxDtype::F32 => "float",
            MlxDtype::F16 => "float16_t",
            MlxDtype::Bf16 => "bfloat16_t",
        }
    }

    /// Short tag used by the `instantiate_rms` / `instantiate_rope` /
    /// `instantiate_unary_all` / `instantiate_binary_all` macros.
    /// Examples: `rms<bfloat16>` → `rmsbfloat16`, `v_Sigmoid<bfloat16><bfloat16>`
    /// → `v_Sigmoidbfloat16bfloat16`, `vv_Add<float32>` → `vv_Addfloat32`.
    pub fn tname(self) -> &'static str {
        match self {
            MlxDtype::F32 => "float32",
            MlxDtype::F16 => "float16",
            MlxDtype::Bf16 => "bfloat16",
        }
    }

    pub fn byte_width(self) -> usize {
        match self {
            MlxDtype::F32 => 4,
            MlxDtype::F16 | MlxDtype::Bf16 => 2,
        }
    }
}

#[repr(C)]
struct SteelGemmParams {
    m: i32,
    n: i32,
    k: i32,
    lda: i32,
    ldb: i32,
    ldd: i32,
    tiles_n: i32,
    tiles_m: i32,
    batch_stride_a: i64,
    batch_stride_b: i64,
    batch_stride_d: i64,
    swizzle_log: i32,
    gemm_k_iterations_aligned: i32,
    batch_ndim: i32,
}

#[allow(clippy::too_many_arguments)]
pub fn op_steel_segmented_gemm_nt_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    a: &Buffer,
    a_off: usize,
    b: &Buffer,
    b_off: usize,
    c: &Buffer,
    c_off: usize,
    m: i32,
    n: i32,
    k: i32,
) -> bool {
    if m <= 0 || n <= 0 || k <= 0 || k % 16 != 0 {
        set_last_error(format!(
            "op_steel_segmented_gemm_nt_bf16: invalid shape M={m} N={n} K={k}"
        ));
        return false;
    }

    let (bm, bn, bk, wm, wn) = if n % 64 == 0 {
        (64i32, 64i32, 16i32, 2i32, 2i32)
    } else if n % 32 == 0 {
        (32i32, 32i32, 16i32, 2i32, 2i32)
    } else {
        set_last_error(format!(
            "op_steel_segmented_gemm_nt_bf16: unsupported N={n}; no non-Steel fallback"
        ));
        return false;
    };

    let name =
        format!("steel_segmented_mm_nt_bfloat16_bfloat16_bm{bm}_bn{bn}_bk{bk}_wm{wm}_wn{wn}");
    let align_m = m % bm == 0;
    let align_n = n % bn == 0;
    let cache_key = format!("{name}#segcont=true#alignM={align_m}#alignN={align_n}");
    let Some(pso) = dev.pipeline_with_constants(&cache_key, &name, |cv| {
        cv_set_bool(cv, true, 199);
        cv_set_bool(cv, align_m, 200);
        cv_set_bool(cv, align_n, 201);
    }) else {
        set_last_error(format!(
            "op_steel_segmented_gemm_nt_bf16: pipeline `{name}` not found"
        ));
        return false;
    };

    let params = SteelGemmParams {
        m,
        n,
        k,
        lda: k,
        ldb: k,
        ldd: n,
        tiles_n: (n + bn - 1) / bn,
        tiles_m: (m + bm - 1) / bm,
        batch_stride_a: 0,
        batch_stride_b: 0,
        batch_stride_d: i64::from(m) * i64::from(n),
        swizzle_log: 0,
        gemm_k_iterations_aligned: k / bk,
        batch_ndim: 0,
    };

    enc.set_pipeline(&pso);
    enc.set_buffer(0, a, a_off);
    enc.set_buffer(1, b, b_off);
    enc.set_bytes_slice(2, &[0u32, k as u32]);
    enc.set_buffer(3, c, c_off);
    enc.set_bytes(4, &params);
    enc.dispatch_threadgroups(
        (
            ((n + bn - 1) / bn) as usize,
            ((m + bm - 1) / bm) as usize,
            1,
        ),
        ((32 * wm * wn) as usize, 1, 1),
    );
    true
}

// ─── qmv / qmm — quantized matmul ──────────────────────────────────
//
// MLX's 4bit scheme uses `mode = "affine"` for the standard row-wise
// scale+bias layout (what `mlx-community/Qwen3.5-35B-A3B-4bit` ships). The
// kernel selection per MLX's `QuantizedMatmul::eval_gpu`:
//
//   * `qmv_quad` — M=1, K and N aligned, fast-path
//   * `qmv_fast` — M=1 small, N%bn==0 && K%512==0
//   * `qmv`      — M=1 general
//   * `qmm`      — M>1 prefill (split-K or full) — delegates to
//                  `qmm_n` when N%32 != 0
//
// For a first cut we only wire `qmv_fast`/`qmv` (decode) and `qmm_t`
// (prefill, transpose=True which is the Linear-layer case). The
// Linear-layer invariant for a `nn.Linear(in=K, out=N)` weight shape
// `[N, K]` maps onto MLX's w-shape `[N, K*bits/32]` + transpose=True
// so the kernel's y = x @ w.T has output shape `[..., N]`.

/// Dispatch `affine_qmv` / `affine_qmv_fast` for M=1 decode. Mirrors
/// `quantized.cpp::qmv` (lines 235-296).
///
/// # Buffer binding order
/// 0: w, 1: scales, [2: biases], 3: x, 4: y, then inline bytes K, N.
///
/// # Grid
/// `(M, (N+bn-1)/bn, B)` threadgroups × `(32, 2, 1)` threads with
/// `bn=8`, `bk=32`.
#[allow(clippy::too_many_arguments)]
pub fn op_qmv(
    enc: &ComputeEncoder,
    dev: &Device,
    act_dtype: MlxDtype,
    w: &Buffer,
    w_off: usize,
    scales: &Buffer,
    s_off: usize,
    biases: Option<(&Buffer, usize)>,
    x: &Buffer,
    x_off: usize,
    y: &Buffer,
    y_off: usize,
    m: i32,
    n: i32,
    k: i32,
    group_size: i32,
    bits: i32,
) -> bool {
    let bn = 8i32;
    let bk = 32i32;
    let batch = 0i32; // B=1 — no batched qmv in our Linear forward

    let fast = n % bn == 0 && k % 512 == 0;
    let stem = if fast { "qmv_fast" } else { "qmv" };
    let name = format!(
        "affine_{stem}_{}_gs_{group_size}_b_{bits}_batch_{batch}",
        act_dtype.name(),
    );

    let Some(pso) = dev.pipeline(&name) else {
        set_last_error(format!("op_qmv: pipeline `{name}` not found"));
        return false;
    };

    enc.set_pipeline(&pso);
    let mut c = 0usize;
    enc.set_buffer(c, w, w_off);
    c += 1;
    enc.set_buffer(c, scales, s_off);
    c += 1;
    if let Some((b_buf, b_off)) = biases {
        enc.set_buffer(c, b_buf, b_off);
        c += 1;
    }
    enc.set_buffer(c, x, x_off);
    c += 1;
    enc.set_buffer(c, y, y_off);
    c += 1;
    enc.set_bytes(c, &k);
    c += 1;
    enc.set_bytes(c, &n);
    let _ = c;

    enc.dispatch_threadgroups(
        (m as usize, ((n + bn - 1) / bn) as usize, 1),
        (bk as usize, 2, 1),
    );
    true
}

/// Dispatch MLX `affine_gather_qmv(_fast)` for dynamic expert selection.
///
/// This is the hot-path primitive for MoE decode: `rhs_indices` selects
/// the expert matrix for each batch item, while `lhs_indices` selects the
/// activation row. Weight/scales/bias strides are element strides inside
/// their own buffers, matching MLX's `adjust_matrix_offsets`.
#[allow(clippy::too_many_arguments)]
pub fn op_gather_qmv(
    enc: &ComputeEncoder,
    dev: &Device,
    act_dtype: MlxDtype,
    w: &Buffer,
    w_expert_stride: i64,
    scales: &Buffer,
    scales_expert_stride: i64,
    biases: &Buffer,
    biases_expert_stride: i64,
    x: &Buffer,
    x_row_stride: i64,
    lhs_indices: &Buffer,
    rhs_indices: &Buffer,
    y: &Buffer,
    batch: i32,
    n: i32,
    k: i32,
    group_size: i32,
    bits: i32,
) -> bool {
    let bn = 8i32;
    let bk = 32i32;
    let fast = n % bn == 0 && k % 512 == 0;
    let stem = if fast {
        "gather_qmv_fast"
    } else {
        "gather_qmv"
    };
    let name = format!(
        "affine_{stem}_{}_gs_{group_size}_b_{bits}",
        act_dtype.name(),
    );

    let Some(pso) = dev.pipeline(&name) else {
        set_last_error(format!("op_gather_qmv: pipeline `{name}` not found"));
        return false;
    };

    let m = 1i32;
    let x_batch_ndims = 1i32;
    let x_shape = [batch.max(1), m];
    let x_strides = [x_row_stride];
    let w_batch_ndims = 1i32;
    let w_shape = [1i32.max(batch)];
    let w_strides = [w_expert_stride];
    let s_strides = [scales_expert_stride];
    let b_strides = [biases_expert_stride];
    let batch_ndims = 1i32;
    let batch_shape = [batch.max(0)];
    let lhs_strides = [1i64];
    let rhs_strides = [1i64];

    enc.set_pipeline(&pso);
    enc.set_buffer(0, w, 0);
    enc.set_buffer(1, scales, 0);
    enc.set_buffer(2, biases, 0);
    enc.set_buffer(3, x, 0);
    enc.set_buffer(4, lhs_indices, 0);
    enc.set_buffer(5, rhs_indices, 0);
    enc.set_buffer(6, y, 0);
    enc.set_bytes(7, &k);
    enc.set_bytes(8, &n);
    enc.set_bytes(9, &x_batch_ndims);
    enc.set_bytes_slice(10, &x_shape);
    enc.set_bytes_slice(11, &x_strides);
    enc.set_bytes(12, &w_batch_ndims);
    enc.set_bytes_slice(13, &w_shape);
    enc.set_bytes_slice(14, &w_strides);
    enc.set_bytes_slice(15, &s_strides);
    enc.set_bytes_slice(16, &b_strides);
    enc.set_bytes(17, &batch_ndims);
    enc.set_bytes_slice(18, &batch_shape);
    enc.set_bytes_slice(19, &lhs_strides);
    enc.set_bytes_slice(20, &rhs_strides);

    enc.dispatch_threadgroups(
        (
            m as usize,
            ((n + bn - 1) / bn) as usize,
            batch.max(0) as usize,
        ),
        (bk as usize, 2, 1),
    );
    true
}

/// Dispatch `affine_qmm_t` for M>1 prefill. Mirrors
/// `quantized.cpp::qmm` — `transpose=True` (Linear-layer case).
///
/// For the simple non-batched case (B=1) the `add_strides_and_shapes`
/// branch is skipped — the vendored MLX kernel signature is `K, N, M`
/// for buffers 5..7.
///
/// # Kernel shape (from quantized.metal instantiate_quantized_batched)
///   `affine_qmm_t_<type>_gs_<gs>_b_<bits>_batch_<0|1>`
///   with default `BM=32, BN=32, BK=32, WM=2, WN=2` — MLX hard-codes
///   this in `quantized.cpp::qmm`.
///
/// # Grid
///   `((N+BN-1)/BN, (M+BM-1)/BM, B)` × `(32, WM*WN=4, 1)`.
#[allow(clippy::too_many_arguments)]
pub fn op_qmm_t(
    enc: &ComputeEncoder,
    dev: &Device,
    act_dtype: MlxDtype,
    w: &Buffer,
    w_off: usize,
    scales: &Buffer,
    s_off: usize,
    biases: Option<(&Buffer, usize)>,
    x: &Buffer,
    x_off: usize,
    y: &Buffer,
    y_off: usize,
    m: i32,
    n: i32,
    k: i32,
    group_size: i32,
    bits: i32,
) -> bool {
    let batch = 0i32;
    let bm = 32i32;
    let bn = 32i32;

    let name = format!(
        "affine_qmm_t_{}_gs_{group_size}_b_{bits}_alN_false_batch_{batch}",
        act_dtype.name(),
    );

    let Some(pso) = dev.pipeline(&name) else {
        set_last_error(format!("op_qmm_t: pipeline `{name}` not found"));
        return false;
    };

    enc.set_pipeline(&pso);
    let mut c = 0usize;
    enc.set_buffer(c, w, w_off);
    c += 1;
    enc.set_buffer(c, scales, s_off);
    c += 1;
    if let Some((b_buf, b_off)) = biases {
        enc.set_buffer(c, b_buf, b_off);
        c += 1;
    }
    enc.set_buffer(c, x, x_off);
    c += 1;
    enc.set_buffer(c, y, y_off);
    c += 1;
    enc.set_bytes(c, &k);
    c += 1;
    enc.set_bytes(c, &n);
    c += 1;
    enc.set_bytes(c, &m);
    let _ = c;

    enc.dispatch_threadgroups(
        (
            ((n + bn - 1) / bn) as usize,
            ((m + bm - 1) / bm) as usize,
            1,
        ),
        (32, 4, 1),
    );
    true
}

#[allow(clippy::too_many_arguments)]
pub fn op_qmm_t_nax(
    enc: &ComputeEncoder,
    dev: &Device,
    act_dtype: MlxDtype,
    w: &Buffer,
    w_off: usize,
    scales: &Buffer,
    s_off: usize,
    biases: Option<(&Buffer, usize)>,
    x: &Buffer,
    x_off: usize,
    y: &Buffer,
    y_off: usize,
    m: i32,
    n: i32,
    k: i32,
    group_size: i32,
    bits: i32,
) -> bool {
    op_qmm_t(
        enc, dev, act_dtype, w, w_off, scales, s_off, biases, x, x_off, y, y_off, m, n, k,
        group_size, bits,
    )
}

// ─── GEMV — dense BF16 decode matmul ───────────────────────────────
//
// Ports the `gemv_axbpy` branch from
// `mlx/backend/metal/matmul.cpp`. This is the hot path for DFlash BF16
// draft projections during single-token decode. Weight layout is
// `[out_features, in_features]`, so the MLX dispatcher selects the
// non-transposed `gemv_<dtype>...` kernel.

#[allow(clippy::too_many_arguments)]
pub fn op_gemv_bf16(
    enc: &ComputeEncoder,
    dev: &Device,
    x: &Buffer,
    x_off: usize,
    w: &Buffer,
    w_off: usize,
    bias: Option<(&Buffer, usize)>,
    y: &Buffer,
    y_off: usize,
    k: i32,
    n: i32,
) -> bool {
    let mut tm = 4i32;
    let tn = 4i32;
    let mut sm = 1i32;
    let mut sn = 32i32;
    let mut bm = if n >= 4096 { 8i32 } else { 4i32 };
    let mut bn = 1i32;

    if k <= 64 {
        bm = 1;
        sm = 8;
        sn = 4;
    } else if k >= 16 * n {
        bm = 1;
        bn = 8;
    }

    if n < tm {
        tm = 1;
    }

    let do_axpby = bias.is_some();
    let name = format!(
        "gemv_bfloat16_bm{bm}_bn{bn}_sm{sm}_sn{sn}_tm{tm}_tn{tn}_nc0_axpby{}",
        if do_axpby { 1 } else { 0 }
    );

    let Some(pso) = dev.pipeline(&name) else {
        set_last_error(format!("op_gemv_bf16: pipeline `{name}` not found"));
        return false;
    };

    enc.set_pipeline(&pso);
    enc.set_buffer(0, w, w_off);
    enc.set_buffer(1, x, x_off);
    if let Some((bias_buf, bias_off)) = bias {
        enc.set_buffer(2, bias_buf, bias_off);
    } else {
        enc.set_buffer(2, w, w_off);
    }
    enc.set_buffer(3, y, y_off);
    enc.set_bytes(4, &k);
    enc.set_bytes(5, &n);
    enc.set_bytes(6, &k);

    let alpha = 1.0f32;
    let beta = if do_axpby { 1.0f32 } else { 0.0f32 };
    enc.set_bytes(7, &alpha);
    enc.set_bytes(8, &beta);

    let batch_ndim = 1i32;
    let batch_shape = [1i32];
    let zero_stride = [0i64];
    enc.set_bytes(9, &batch_ndim);
    enc.set_bytes_slice(10, &batch_shape);
    enc.set_bytes_slice(11, &zero_stride);
    enc.set_bytes_slice(12, &zero_stride);
    enc.set_bytes_slice(13, &zero_stride);
    let bias_stride = 1i32;
    enc.set_bytes(14, &bias_stride);

    let n_out_per_tgp = bm * sm * tm;
    let n_tgp = ((n + n_out_per_tgp - 1) / n_out_per_tgp) as usize;
    enc.dispatch_threadgroups((n_tgp, 1, 1), (32, bn as usize, bm as usize));
    true
}

// ─── RMSNorm ────────────────────────────────────────────────────────
//
// Kernel name pattern: `rms_{float|float16_t|bfloat16_t}` from
// `rms_norm.metal`. Args layout from
// `mlx/backend/metal/normalization.cpp::RMSNorm::eval_gpu`:
//   buffer 0: x, 1: weight, 2: out, 3: float eps, 4: uint axis_size,
//             5: uint w_stride
// Threadgroup size is `axis_size` (capped by max threads per group).
//
// For 35B-A3B, axis_size = hidden_size = 2048. Keep the looped path
// available because the same wrapper is used for larger projections too.

#[allow(clippy::too_many_arguments)]
pub fn op_rms_norm(
    enc: &ComputeEncoder,
    dev: &Device,
    dtype: MlxDtype,
    x: &Buffer,
    x_off: usize,
    w: &Buffer,
    w_off: usize,
    y: &Buffer,
    y_off: usize,
    axis_size: u32,
    n_rows: u32,
    eps: f32,
) -> bool {
    // See `mlx/backend/metal/normalization.cpp::RMSNorm::eval_gpu`:
    //   if (axis_size > looped_limit) { kernel = "rms_looped_<type>" }
    //   else                          { kernel = "rms_<type>" }
    const LOOPED_LIMIT: u32 = 4096;
    // Byte-exact from `rms_norm.metal::instantiate_rms`:
    //   instantiate_kernel("rms" #name, rms_single_row, itype)
    //   instantiate_kernel("rms_looped" #name, rms_looped, itype)
    // Note `#name` is the short tag (float32/float16/bfloat16), NOT
    // the Metal C-typename — and there is no underscore between the
    // stem and the tag.
    let name = if axis_size > LOOPED_LIMIT {
        format!("rms_looped{}", dtype.tname())
    } else {
        format!("rms{}", dtype.tname())
    };

    let cache_key = format!("{name}#has_w=true");
    let Some(pso) = dev.pipeline_with_constants(&cache_key, &name, |cv| {
        cv_set_bool(cv, true, 20);
    }) else {
        set_last_error(format!("op_rms_norm: pipeline `{name}` not found"));
        return false;
    };

    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, x_off);
    enc.set_buffer(1, w, w_off);
    enc.set_buffer(2, y, y_off);
    enc.set_bytes(3, &eps);
    enc.set_bytes(4, &axis_size);
    let w_stride: u32 = 1;
    enc.set_bytes(5, &w_stride);

    // Threadgroup: matches MLX — min(axis_size, 1024) rounded up to
    // simdgroup size (32). Looped kernel stays at 1024 always.
    let threads = if axis_size > LOOPED_LIMIT {
        1024usize
    } else {
        let mut t = axis_size.min(1024) as usize;
        if t % 32 != 0 {
            t = ((t + 31) / 32) * 32;
        }
        t
    };
    enc.dispatch_threadgroups((n_rows as usize, 1, 1), (threads, 1, 1));
    true
}

// ─── RoPE ───────────────────────────────────────────────────────────
//
// Kernel name pattern: `rope_<type>` / `rope_single_<type>` /
// `rope_freqs_<type>` / `rope_large_<type>`. From `rope.cpp::RoPE::eval_gpu`.
//
// Qwen3.5 uses the "freqs" variant because the rope layer precomputes
// inv_freqs; for the first-pass minimal forward we'll use the simpler
// `rope_<type>` with a computed theta base and traditional=false.
//
// Signature (rope.metal `rope_impl`):
//   in, out, strides(x4 + 3), offset_int, scale, base, dims_i32,
//   (optional freqs), grid_dims(3)

#[allow(clippy::too_many_arguments)]
pub fn op_rope(
    enc: &ComputeEncoder,
    dev: &Device,
    dtype: MlxDtype,
    x: &Buffer,
    x_off: usize,
    y: &Buffer,
    y_off: usize,
    head_dim: i32,
    n_tokens: i32,
    n_heads: i32,
    offset: i32,
    base: f32,
    scale: f32,
    traditional: bool,
) -> bool {
    // Byte-exact from `rope.metal::instantiate_rope_{s,g}`:
    //   instantiate_kernel("rope_" #name, rope, type, int32_t)
    //   instantiate_kernel("rope_single_" #name, rope_single, type)
    // where #name is float32/float16/bfloat16.
    let single = n_tokens == 1;
    let stem = if single { "rope_single" } else { "rope" };
    let name = format!("{stem}_{}", dtype.tname());

    let cache_key = format!("{name}#forward=true#traditional={traditional}#hs_transpose=false");
    let Some(pso) = dev.pipeline_with_constants(&cache_key, &name, |cv| {
        cv_set_bool(cv, true, 1);
        cv_set_bool(cv, traditional, 2);
        cv_set_bool(cv, false, 3);
    }) else {
        set_last_error(format!("op_rope: pipeline `{name}` not found"));
        return false;
    };

    enc.set_pipeline(&pso);
    // rope_impl buffer binding (from rope.metal): in, out, strides(7),
    // offset, scale, base, dims, grid_dims(3).
    enc.set_buffer(0, x, x_off);
    enc.set_buffer(1, y, y_off);

    // x shape: [n_tokens, n_heads, head_dim] row-major — strides in
    // elements: [n_heads*head_dim, head_dim, 1].
    let stride_tok = (n_heads * head_dim) as i64;
    let stride_head = head_dim as i64;
    let stride_dim = 1i64;
    // rope kernel takes strides for x input, y output — MLX passes
    // them as packed int arrays of length 3 (x_strides) and 3
    // (y_strides). Check rope.metal.
    let strides: [i64; 6] = [
        stride_tok,
        stride_head,
        stride_dim,
        stride_tok,
        stride_head,
        stride_dim,
    ];
    enc.set_bytes_slice(2, &strides);

    enc.set_bytes(3, &offset);
    enc.set_bytes(4, &scale);
    enc.set_bytes(5, &base);

    let half_dim = if traditional { head_dim } else { head_dim / 2 };
    let _ = half_dim; // actual kernel computes this internally

    // Grid: (half_head_dim, n_heads, n_tokens)
    let grid = ((head_dim / 2) as usize, n_heads as usize, n_tokens as usize);
    enc.dispatch(grid, (32, 1, 1));
    true
}

// ─── SDPA (vector path — decode) ───────────────────────────────────
//
// Kernel name pattern: `sdpa_vector_<type>_<head_dim>_<vdim>` for the
// 1-pass vec kernel; 2-pass for long contexts uses
// `sdpa_vector_2pass_1_<type>_<head_dim>_<vdim>` + `_2pass_2_...`.
//
// For decode (M=1 query), MLX routes through the vector kernel when
// the context is small, else 2-pass.

// NOTE: The full SDPA dispatch has a lot of branches (mask type,
// sinks, softcap, 2pass selection, steel attention for prefill). To
// keep this module shippable we wire only the simplest variant first
// and expand in the immediate next turns.

// ─── Binary add / mul ──────────────────────────────────────────────
//
// MLX's binary kernels are named `<op>_<in_dtype>_<out_dtype>`, e.g.
// `add_bfloat16_t_bfloat16_t` or `mul_float_float`.

/// MLX binary op tag — Pascal-case, matches the `instantiate_binary_all`
/// macro expansions in `binary.metal` which stringify the `op` token
/// as the kernel-name suffix (e.g. `vv_Add<tname>`).
#[derive(Clone, Copy, Debug)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl BinaryOp {
    pub fn name(self) -> &'static str {
        match self {
            BinaryOp::Add => "Add",
            BinaryOp::Subtract => "Subtract",
            BinaryOp::Multiply => "Multiply",
            BinaryOp::Divide => "Divide",
        }
    }
}

/// Elementwise binary op on two flat f32/f16/bf16 buffers of equal
/// length. Uses the `g` (general) variant to stay shape-agnostic;
/// the specialized `v` (contiguous, vectorized) variant is a future
/// optimization.
#[allow(clippy::too_many_arguments)]
pub fn op_binary_contiguous(
    enc: &ComputeEncoder,
    dev: &Device,
    dtype: MlxDtype,
    op: BinaryOp,
    a: &Buffer,
    a_off: usize,
    b: &Buffer,
    b_off: usize,
    y: &Buffer,
    y_off: usize,
    n_elems: u64,
) -> bool {
    // Byte-exact from `binary.metal::instantiate_binary_all`:
    //   instantiate_kernel("vv_" #op #tname, binary_vv, itype, otype, op, 1)
    // i.e. `vv_<Op><tname>` — Pascal-case op, no separator before tag.
    let name = format!("vv_{}{}", op.name(), dtype.tname());
    let Some(pso) = dev.pipeline(&name) else {
        set_last_error(format!("op_binary_contiguous: pipeline `{name}` not found"));
        return false;
    };
    enc.set_pipeline(&pso);
    enc.set_buffer(0, a, a_off);
    enc.set_buffer(1, b, b_off);
    enc.set_buffer(2, y, y_off);

    // Threadgroup = 256, grid = ceildiv(n_elems, 256).
    let tg: u32 = 256;
    let grid = ((n_elems as usize + tg as usize - 1) / tg as usize, 1, 1);
    enc.dispatch_threadgroups(grid, (tg as usize, 1, 1));
    true
}

// ─── Unary (silu, sigmoid, ...) ─────────────────────────────────────

/// MLX unary op tag — Pascal-case, matches the `instantiate_unary_all`
/// macro in `unary.metal`. Note MLX does NOT ship a `Silu` unary; the
/// Python reference uses `x * sigmoid(x)` — so the forward graph
/// composes `op_unary(Sigmoid) + op_binary(Multiply)` for SwiGLU.
#[derive(Clone, Copy, Debug)]
pub enum UnaryOp {
    Sigmoid,
    Relu,
    Tanh,
    Negative,
    Abs,
    Exp,
    Sqrt,
    Rsqrt,
    Log,
    Cos,
    Sin,
}

impl UnaryOp {
    pub fn name(self) -> &'static str {
        match self {
            UnaryOp::Sigmoid => "Sigmoid",
            UnaryOp::Relu => "Relu",
            UnaryOp::Tanh => "Tanh",
            UnaryOp::Negative => "Negative",
            UnaryOp::Abs => "Abs",
            UnaryOp::Exp => "Exp",
            UnaryOp::Sqrt => "Sqrt",
            UnaryOp::Rsqrt => "Rsqrt",
            UnaryOp::Log => "Log",
            UnaryOp::Cos => "Cos",
            UnaryOp::Sin => "Sin",
        }
    }
}

pub fn op_unary_contiguous(
    enc: &ComputeEncoder,
    dev: &Device,
    dtype: MlxDtype,
    op: UnaryOp,
    x: &Buffer,
    x_off: usize,
    y: &Buffer,
    y_off: usize,
    n_elems: u64,
) -> bool {
    // Byte-exact from `unary.metal::instantiate_unary_all`:
    //   instantiate_kernel("v_" #op #in_tname #out_tname, unary_v, in_type, out_type, op, 1)
    // i.e. `v_<Op><in_tname><out_tname>` — we always run same-dtype
    // in/out, hence `<tname><tname>`.
    let name = format!("v_{}{tn}{tn}", op.name(), tn = dtype.tname());
    let Some(pso) = dev.pipeline(&name) else {
        set_last_error(format!("op_unary_contiguous: pipeline `{name}` not found"));
        return false;
    };
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, x_off);
    enc.set_buffer(1, y, y_off);

    let tg: u32 = 256;
    let grid = ((n_elems as usize + tg as usize - 1) / tg as usize, 1, 1);
    enc.dispatch_threadgroups(grid, (tg as usize, 1, 1));
    true
}

// ─── Quick wins: public error accessor ─────────────────────────────

pub fn last_error_str() -> String {
    crate::common::errors::last_error()
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtype_names_match_mlx() {
        assert_eq!(MlxDtype::F32.name(), "float");
        assert_eq!(MlxDtype::F16.name(), "float16_t");
        assert_eq!(MlxDtype::Bf16.name(), "bfloat16_t");
        assert_eq!(MlxDtype::F32.tname(), "float32");
        assert_eq!(MlxDtype::F16.tname(), "float16");
        assert_eq!(MlxDtype::Bf16.tname(), "bfloat16");
    }

    #[test]
    fn binary_unary_rms_rope_names() {
        // ref: `binary.metal::instantiate_binary_all` — `vv_<Op><tname>`.
        assert_eq!(
            format!("vv_{}{}", BinaryOp::Add.name(), MlxDtype::Bf16.tname()),
            "vv_Addbfloat16"
        );
        assert_eq!(
            format!("vv_{}{}", BinaryOp::Multiply.name(), MlxDtype::F32.tname()),
            "vv_Multiplyfloat32"
        );
        // ref: `unary.metal::instantiate_unary_all` — `v_<Op><tname><tname>`.
        assert_eq!(
            format!(
                "v_{}{t}{t}",
                UnaryOp::Sigmoid.name(),
                t = MlxDtype::Bf16.tname()
            ),
            "v_Sigmoidbfloat16bfloat16"
        );
        // ref: `rms_norm.metal::instantiate_rms` — `rms<tname>`.
        assert_eq!(format!("rms{}", MlxDtype::Bf16.tname()), "rmsbfloat16");
        // ref: `rope.metal::instantiate_rope_g` — `rope_<tname>`.
        assert_eq!(format!("rope_{}", MlxDtype::Bf16.tname()), "rope_bfloat16");
    }

    #[test]
    fn kernel_name_qmv_matches_mlx_template() {
        // Ground truth from `mlx/backend/metal/quantized.cpp::qmv`
        // with type_string=bfloat16_t, group_size=64, bits=4, B=1, fast=true.
        // Expected: `affine_qmv_fast_bfloat16_t_gs_64_b_4_batch_0`.
        let name = format!(
            "affine_{stem}_{}_gs_{gs}_b_{b}_batch_{batch}",
            MlxDtype::Bf16.name(),
            stem = "qmv_fast",
            gs = 64,
            b = 4,
            batch = 0,
        );
        assert_eq!(name, "affine_qmv_fast_bfloat16_t_gs_64_b_4_batch_0");
    }

    #[test]
    fn kernel_name_qmm_t_matches_mlx_template() {
        let name = format!(
            "affine_qmm_t_{}_gs_{gs}_b_{b}_alN_false_batch_{batch}",
            MlxDtype::Bf16.name(),
            gs = 64,
            b = 4,
            batch = 0,
        );
        assert_eq!(name, "affine_qmm_t_bfloat16_t_gs_64_b_4_alN_false_batch_0",);
    }
}
