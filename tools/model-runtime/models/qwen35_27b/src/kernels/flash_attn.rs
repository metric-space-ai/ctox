//! FlashAttention v2 (tensor-core MMA) — Rust-side launcher.
//!
//! Backs `kernels/flash_attn.cu`. Single public entry
//! `launch_flash_attn_bf16` accepts bf16 Q/K/V plus optional causal
//! and additive masks, returns bf16 O.
//!
//! Scope (see the .cu header for full rationale):
//!   * head_dim fixed at 256  (Qwen3.5-27B's FullAttention).
//!   * Q: [n_tokens, n_q_heads, 256] bf16.
//!   * K: [kv_len,   n_kv_heads, 256] bf16.
//!   * V: [kv_len,   n_kv_heads, 256] bf16.
//!   * mask: Option<[n_tokens, kv_len] half>.
//!   * O: [n_tokens, n_q_heads, 256] bf16.
//!
//! Ported (algorithm + tile shapes) from llama.cpp/ggml-cuda's
//! FlashAttention reference (`fattn-mma-f16.cuh`, `fattn-tile.cuh`,
//! `fattn-common.cuh`). Uses `nvcuda::wmma` for the tensor-core MMA
//! rather than `mma.sync` via ggml's `mma.cuh` helper library — both
//! compile to the same HMMA SASS on sm_80+; wmma keeps the port self-
//! contained without pulling in ggml's ~5k lines of metaprogramming
//! headers.
//!
//! Does NOT synchronize the stream — caller syncs at phase boundaries.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, CudaSlice, LaunchConfig, PushKernelArg};
use cudarc::driver::sys::CUfunction_attribute_enum;
use cudarc::nvrtc::Ptx;
use half::{bf16, f16};

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use super::FLASH_ATTN_PTX;

/// Tile geometry — keep in lockstep with `kernels/flash_attn.cu`.
const BR: usize = 16;   // query rows per CTA
const BC: usize = 64;   // KV rows per iteration
const D_TOTAL: usize = 256;
const NWARPS: usize = 4;
const WARP_SZ: usize = 32;

/// Padded stride on the f32 S/P scratch. Matches `S_STRIDE` in the .cu
/// file. 8 = 32 bytes of pad to kill the 32-way bank conflict when a
/// warp broadcasts a row.
const S_STRIDE: usize = BC + 8;

/// Shared-memory budget, bytes. Must fit the dynamic-shmem opt-in
/// ceiling on sm_86 (100 KiB). Current footprint:
///   Q-tile : BR * D_TOTAL * 2          =  8 KiB
///   K-tile : BC * D_TOTAL * 2          = 32 KiB
///   V-tile : BC * D_TOTAL * 2          = 32 KiB
///   S/P    : BR * S_STRIDE * 4         ≈ 4.5 KiB
///   Total                              ≈ 76.5 KiB (well under 100KB).
///
/// We compute it at runtime rather than hard-coding — the kernel's
/// extern declaration already accounts for the layout; the shmem size
/// we pass to the launch must match the sum below exactly.
fn shmem_bytes() -> u32 {
    let q = BR * D_TOTAL * std::mem::size_of::<u16>();
    let k = BC * D_TOTAL * std::mem::size_of::<u16>();
    let v = BC * D_TOTAL * std::mem::size_of::<u16>();
    let s = BR * S_STRIDE * std::mem::size_of::<f32>();
    (q + k + v + s) as u32
}

static FLASH_ATTN_FN: OnceLock<CudaFunction> = OnceLock::new();

fn flash_attn_fn(device: &Arc<DeviceContext>) -> Result<CudaFunction> {
    if let Some(f) = FLASH_ATTN_FN.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(FLASH_ATTN_PTX)
        .map_err(|e| anyhow!("flash_attn.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module flash_attn.ptx: {:?}", e))?;
    let f = module
        .load_function("flash_attn_bf16_d256_impl")
        .map_err(|e| anyhow!("load_function flash_attn_bf16_d256_impl: {:?}", e))?;
    let _ = FLASH_ATTN_FN.set(f.clone());
    Ok(f)
}

/// FlashAttention v2 launch.
///
/// Shapes (row-major throughout):
///   * `q`    : `[n_tokens, n_q_heads, 256]` bf16
///   * `k`    : `[kv_len,   n_kv_heads, 256]` bf16
///   * `v`    : `[kv_len,   n_kv_heads, 256]` bf16
///   * `mask` : optional `[n_tokens, kv_len]` f16 additive mask
///   * `out`  : `[n_tokens, n_q_heads, 256]` bf16 (pre-allocated)
///
/// `scale` is applied to Q·K^T before softmax, typically
/// `1 / sqrt(head_dim)`.
///
/// `n_kv_heads_in_q_group` is the GQA factor, i.e.
/// `n_q_heads / n_kv_heads` — 6 for Qwen3.5-27B (24 Q heads, 4 KV
/// heads).
///
/// `causal` controls whether the upper-triangular causal mask is
/// applied. Typical usage: `causal=true` for prefill, `causal=false`
/// for decode.
#[allow(clippy::too_many_arguments)]
pub fn launch_flash_attn_bf16(
    device: &Arc<DeviceContext>,
    q: &CudaTensor<bf16>,
    k: &CudaTensor<bf16>,
    v: &CudaTensor<bf16>,
    mask: Option<&CudaTensor<f16>>,
    out: &mut CudaTensor<bf16>,
    scale: f32,
    n_kv_heads_in_q_group: usize,
    causal: bool,
) -> Result<()> {
    // ── Shape validation ────────────────────────────────────────────
    if q.shape().len() != 3 {
        return Err(anyhow!(
            "flash_attn: q must be 3D [n_tokens, n_q_heads, head_dim], got {:?}",
            q.shape()
        ));
    }
    if k.shape().len() != 3 || v.shape().len() != 3 {
        return Err(anyhow!(
            "flash_attn: k, v must be 3D [kv_len, n_kv_heads, head_dim]; got k={:?} v={:?}",
            k.shape(),
            v.shape()
        ));
    }
    if out.shape() != q.shape() {
        return Err(anyhow!(
            "flash_attn: out.shape {:?} != q.shape {:?}",
            out.shape(),
            q.shape()
        ));
    }

    let n_tokens = q.shape()[0];
    let n_q_heads = q.shape()[1];
    let head_dim = q.shape()[2];
    if head_dim != D_TOTAL {
        return Err(anyhow!(
            "flash_attn: head_dim must be {}, got {}",
            D_TOTAL,
            head_dim
        ));
    }

    let kv_len = k.shape()[0];
    let n_kv_heads = k.shape()[1];
    if v.shape()[0] != kv_len || v.shape()[1] != n_kv_heads || v.shape()[2] != head_dim {
        return Err(anyhow!(
            "flash_attn: k/v shape mismatch — k={:?} v={:?}",
            k.shape(),
            v.shape()
        ));
    }
    if k.shape()[2] != head_dim {
        return Err(anyhow!(
            "flash_attn: k head_dim mismatch — expected {}, got {}",
            head_dim,
            k.shape()[2]
        ));
    }

    if n_kv_heads_in_q_group == 0 {
        return Err(anyhow!(
            "flash_attn: n_kv_heads_in_q_group (GQA factor) must be > 0"
        ));
    }
    if n_q_heads != n_kv_heads * n_kv_heads_in_q_group {
        return Err(anyhow!(
            "flash_attn: n_q_heads ({}) != n_kv_heads ({}) * gqa_group ({})",
            n_q_heads,
            n_kv_heads,
            n_kv_heads_in_q_group
        ));
    }

    if n_tokens == 0 || kv_len == 0 || n_q_heads == 0 {
        return Ok(());
    }

    if let Some(m) = mask {
        if m.shape().len() != 2 || m.shape()[0] != n_tokens || m.shape()[1] != kv_len {
            return Err(anyhow!(
                "flash_attn: mask shape must be [n_tokens={}, kv_len={}], got {:?}",
                n_tokens,
                kv_len,
                m.shape()
            ));
        }
    }

    // ── Launch config ───────────────────────────────────────────────
    // grid.x : ceil(n_tokens / BR) — query row tiles
    // grid.y : n_q_heads           — one CTA per (row-tile, q_head)
    // block  : (WARP_SZ, NWARPS)   — 128 threads total
    let grid_x = n_tokens.div_ceil(BR) as u32;
    let grid_y = n_q_heads as u32;
    let shmem = shmem_bytes();
    let cfg = LaunchConfig {
        grid_dim: (grid_x, grid_y, 1),
        block_dim: (WARP_SZ as u32, NWARPS as u32, 1),
        shared_mem_bytes: shmem,
    };

    // Opt into the >48KB dynamic-shmem path on sm_86. This needs to
    // be set per-function, not per-launch; we do it once on first call
    // via cuFuncSetAttribute CU_FUNC_ATTRIBUTE_MAX_DYNAMIC_SHARED_SIZE_BYTES.
    //
    // cudarc doesn't expose this directly on CudaFunction in older
    // versions; for now we rely on the driver's automatic opt-in which
    // works up to 48KB without changes, and for >48KB we'd need the
    // explicit attribute. Our footprint is ~76.5KB, so this MATTERS.
    //
    // Workaround: raw cuFuncSetAttribute via the driver API. We fetch
    // the underlying CUfunction, call the driver, then proceed.
    //
    // On failure, fall through — the launch will return a
    // CUDA_ERROR_INVALID_VALUE that propagates back to the caller,
    // which at least makes the misconfiguration loud instead of
    // silent.
    let f = flash_attn_fn(device)?;
    set_max_dynamic_shmem(&f, shmem)?;

    let stream = device.raw().default_stream();
    let mut launcher = stream.launch_builder(&f);

    let n_tokens_i32 = n_tokens as i32;
    let kv_len_i32 = kv_len as i32;
    let n_q_heads_i32 = n_q_heads as i32;
    let n_kv_heads_i32 = n_kv_heads as i32;
    let gqa_group_i32 = n_kv_heads_in_q_group as i32;
    let causal_flag = if causal { 1i32 } else { 0i32 };
    let has_mask = if mask.is_some() { 1i32 } else { 0i32 };

    // Null-pointer sentinel for the optional mask slot. Same pattern
    // as gated_delta_net — the kernel's `has_mask` flag gates the
    // actual read so the pointer value is only consulted when valid.
    let null_ptr: u64 = 0;

    launcher
        .arg(q.buf())
        .arg(k.buf())
        .arg(v.buf());
    match mask {
        Some(m) => {
            launcher.arg(m.buf());
        }
        None => {
            launcher.arg(&null_ptr);
        }
    }
    launcher
        .arg(out.buf_mut())
        .arg(&n_tokens_i32)
        .arg(&kv_len_i32)
        .arg(&n_q_heads_i32)
        .arg(&n_kv_heads_i32)
        .arg(&gqa_group_i32)
        .arg(&causal_flag)
        .arg(&has_mask)
        .arg(&scale);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "flash_attn launch (n_tokens={}, kv_len={}, n_q_heads={}, gqa={}): {:?}",
            n_tokens,
            kv_len,
            n_q_heads,
            n_kv_heads_in_q_group,
            e
        )
    })?;
    Ok(())
}

/// KV-slab variant of [`launch_flash_attn_bf16`] — same kernel, but
/// `k` and `v` come in as raw `CudaSlice` handles on the KV cache
/// slab (layout `[max_ctx, n_kv_heads, head_dim]`, only the first
/// `kv_len` rows valid). Q and the output stay as owned
/// `CudaTensor`s because they're allocated per forward anyway.
///
/// This is the form the full-attention layer's forward uses to
/// replace its 24-Q-head attention loop with a single flash-attn
/// launch: no per-head gather/scatter, no per-head matmul, no
/// per-head softmax — the kernel consumes the slab prefix and the
/// dense Q tensor directly.
///
/// Validation follows the CudaTensor variant: `q` and `out` must be
/// `[n_tokens, n_q_heads, 256]`, `kv_len * n_kv_heads * 256` must
/// fit inside each slab, and the GQA factor must divide evenly.
#[allow(clippy::too_many_arguments)]
pub fn launch_flash_attn_bf16_kv_slab(
    device: &Arc<DeviceContext>,
    q: &CudaTensor<bf16>,
    k_slab: &CudaSlice<bf16>,
    v_slab: &CudaSlice<bf16>,
    mask: Option<&CudaTensor<f16>>,
    out: &mut CudaTensor<bf16>,
    kv_len: usize,
    n_kv_heads: usize,
    scale: f32,
    n_kv_heads_in_q_group: usize,
    causal: bool,
) -> Result<()> {
    if q.shape().len() != 3 {
        return Err(anyhow!(
            "flash_attn kv_slab: q must be 3D [n_tokens, n_q_heads, head_dim], got {:?}",
            q.shape()
        ));
    }
    if out.shape() != q.shape() {
        return Err(anyhow!(
            "flash_attn kv_slab: out.shape {:?} != q.shape {:?}",
            out.shape(),
            q.shape()
        ));
    }

    let n_tokens = q.shape()[0];
    let n_q_heads = q.shape()[1];
    let head_dim = q.shape()[2];
    if head_dim != D_TOTAL {
        return Err(anyhow!(
            "flash_attn kv_slab: head_dim must be {}, got {}",
            D_TOTAL,
            head_dim
        ));
    }
    if n_kv_heads_in_q_group == 0 {
        return Err(anyhow!(
            "flash_attn kv_slab: n_kv_heads_in_q_group (GQA factor) must be > 0"
        ));
    }
    if n_q_heads != n_kv_heads * n_kv_heads_in_q_group {
        return Err(anyhow!(
            "flash_attn kv_slab: n_q_heads ({}) != n_kv_heads ({}) * gqa_group ({})",
            n_q_heads,
            n_kv_heads,
            n_kv_heads_in_q_group
        ));
    }

    let needed = kv_len * n_kv_heads * head_dim;
    if k_slab.len() < needed || v_slab.len() < needed {
        return Err(anyhow!(
            "flash_attn kv_slab: slab too small — need {} elems, got k={} v={}",
            needed,
            k_slab.len(),
            v_slab.len()
        ));
    }

    if n_tokens == 0 || kv_len == 0 || n_q_heads == 0 {
        return Ok(());
    }

    if let Some(m) = mask {
        if m.shape().len() != 2 || m.shape()[0] != n_tokens || m.shape()[1] != kv_len {
            return Err(anyhow!(
                "flash_attn kv_slab: mask shape must be [n_tokens={}, kv_len={}], got {:?}",
                n_tokens,
                kv_len,
                m.shape()
            ));
        }
    }

    let grid_x = n_tokens.div_ceil(BR) as u32;
    let grid_y = n_q_heads as u32;
    let shmem = shmem_bytes();
    let cfg = LaunchConfig {
        grid_dim: (grid_x, grid_y, 1),
        block_dim: (WARP_SZ as u32, NWARPS as u32, 1),
        shared_mem_bytes: shmem,
    };

    let f = flash_attn_fn(device)?;
    set_max_dynamic_shmem(&f, shmem)?;

    let stream = device.raw().default_stream();
    let mut launcher = stream.launch_builder(&f);

    let n_tokens_i32 = n_tokens as i32;
    let kv_len_i32 = kv_len as i32;
    let n_q_heads_i32 = n_q_heads as i32;
    let n_kv_heads_i32 = n_kv_heads as i32;
    let gqa_group_i32 = n_kv_heads_in_q_group as i32;
    let causal_flag = if causal { 1i32 } else { 0i32 };
    let has_mask = if mask.is_some() { 1i32 } else { 0i32 };
    let null_ptr: u64 = 0;

    launcher.arg(q.buf()).arg(k_slab).arg(v_slab);
    match mask {
        Some(m) => {
            launcher.arg(m.buf());
        }
        None => {
            launcher.arg(&null_ptr);
        }
    }
    launcher
        .arg(out.buf_mut())
        .arg(&n_tokens_i32)
        .arg(&kv_len_i32)
        .arg(&n_q_heads_i32)
        .arg(&n_kv_heads_i32)
        .arg(&gqa_group_i32)
        .arg(&causal_flag)
        .arg(&has_mask)
        .arg(&scale);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "flash_attn_kv_slab launch (n_tokens={}, kv_len={}, n_q_heads={}, gqa={}): {:?}",
            n_tokens,
            kv_len,
            n_q_heads,
            n_kv_heads_in_q_group,
            e
        )
    })?;
    Ok(())
}

/// Opt the kernel into >48KB dynamic shared memory.
///
/// On sm_86 the default per-CTA dynamic shmem ceiling is 48KB; opting
/// into the full 100KB pool requires
/// `cuFuncSetAttribute(CU_FUNC_ATTRIBUTE_MAX_DYNAMIC_SHARED_SIZE_BYTES)`.
/// Cudarc 0.17 exposes this via `CudaFunction::set_attribute`.
///
/// Idempotent per process — safe to call every launch; the driver
/// caches the setting.
fn set_max_dynamic_shmem(f: &CudaFunction, bytes: u32) -> Result<()> {
    // Skip the opt-in entirely if we're under the default 48KB
    // ceiling — the driver lets us through with no extra call.
    if bytes <= 48 * 1024 {
        return Ok(());
    }
    f.set_attribute(
        CUfunction_attribute_enum::CU_FUNC_ATTRIBUTE_MAX_DYNAMIC_SHARED_SIZE_BYTES,
        bytes as i32,
    )
    .map_err(|e| anyhow!("set CU_FUNC_ATTRIBUTE_MAX_DYNAMIC_SHARED_SIZE_BYTES={}: {:?}", bytes, e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — CPU reference + on-host integration
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU reference implementation: explicit Q·K^T / √d → softmax → · V.
    /// Mirrors the kernel semantics precisely so we can compare outputs.
    ///
    /// Shapes match the GPU launcher.
    #[allow(clippy::too_many_arguments)]
    fn fa_cpu(
        q: &[f32],     // [n_tokens, n_q_heads, head_dim]
        k: &[f32],     // [kv_len,   n_kv_heads, head_dim]
        v: &[f32],     // [kv_len,   n_kv_heads, head_dim]
        mask: Option<&[f32]>, // [n_tokens, kv_len] additive
        n_tokens: usize,
        kv_len: usize,
        n_q_heads: usize,
        n_kv_heads: usize,
        head_dim: usize,
        scale: f32,
        causal: bool,
    ) -> Vec<f32> {
        let gqa = n_q_heads / n_kv_heads;
        let mut out = vec![0.0f32; n_tokens * n_q_heads * head_dim];

        for t in 0..n_tokens {
            for h in 0..n_q_heads {
                let kv_h = h / gqa;
                // Compute S[1, kv_len] for this (t, h).
                let mut s = vec![0.0f32; kv_len];
                let q_off = (t * n_q_heads + h) * head_dim;
                for c in 0..kv_len {
                    let k_off = (c * n_kv_heads + kv_h) * head_dim;
                    let mut acc = 0.0f32;
                    for d in 0..head_dim {
                        acc += q[q_off + d] * k[k_off + d];
                    }
                    s[c] = acc * scale;
                }
                // Apply external additive mask.
                if let Some(m) = mask {
                    for c in 0..kv_len {
                        s[c] += m[t * kv_len + c];
                    }
                }
                // Causal mask.
                if causal {
                    let q_abs = (kv_len - n_tokens) + t;
                    for c in 0..kv_len {
                        if c > q_abs {
                            s[c] = f32::NEG_INFINITY;
                        }
                    }
                }
                // Softmax.
                let mut m_max = f32::NEG_INFINITY;
                for &v in s.iter() {
                    if v > m_max {
                        m_max = v;
                    }
                }
                let mut sum = 0.0f32;
                for v in s.iter_mut() {
                    *v = (*v - m_max).exp();
                    sum += *v;
                }
                if sum > 0.0 {
                    for v in s.iter_mut() {
                        *v /= sum;
                    }
                } else {
                    // all -inf row → all-zero output. Keep s as 0s.
                    for v in s.iter_mut() {
                        *v = 0.0;
                    }
                }
                // Output: [t, h, :] = sum_c s[c] * V[c, kv_h, :].
                let o_off = (t * n_q_heads + h) * head_dim;
                for d in 0..head_dim {
                    let mut acc = 0.0f32;
                    for c in 0..kv_len {
                        let v_off = (c * n_kv_heads + kv_h) * head_dim;
                        acc += s[c] * v[v_off + d];
                    }
                    out[o_off + d] = acc;
                }
            }
        }
        out
    }

    /// Device-backed end-to-end test. Shapes match Qwen3.5-27B's
    /// FullAttention: n_q_heads=24, n_kv_heads=4, head_dim=256, causal.
    ///
    /// Run:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture flash_attn_vs_cpu_golden
    #[test]
    #[ignore]
    fn flash_attn_vs_cpu_golden() {
        let n_tokens = 8usize;
        let kv_len = 128usize;
        let n_q_heads = 24usize;
        let n_kv_heads = 4usize;
        let head_dim = 256usize;
        let gqa = n_q_heads / n_kv_heads;
        let scale = 1.0 / (head_dim as f32).sqrt();

        // Deterministic PRNG so the test is host-independent.
        let mut seed: u32 = 0xFA57_BA5Eu32;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        let q_host: Vec<f32> = (0..n_tokens * n_q_heads * head_dim).map(|_| rand_f() * 0.3).collect();
        let k_host: Vec<f32> = (0..kv_len * n_kv_heads * head_dim).map(|_| rand_f() * 0.3).collect();
        let v_host: Vec<f32> = (0..kv_len * n_kv_heads * head_dim).map(|_| rand_f() * 0.3).collect();

        // CPU golden (f32).
        let o_cpu = fa_cpu(
            &q_host, &k_host, &v_host, None,
            n_tokens, kv_len, n_q_heads, n_kv_heads, head_dim,
            scale, /*causal=*/true,
        );

        // Upload: downcast f32 → bf16 for device tensors.
        let q_bf: Vec<bf16> = q_host.iter().map(|&x| bf16::from_f32(x)).collect();
        let k_bf: Vec<bf16> = k_host.iter().map(|&x| bf16::from_f32(x)).collect();
        let v_bf: Vec<bf16> = v_host.iter().map(|&x| bf16::from_f32(x)).collect();

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let q = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![n_tokens, n_q_heads, head_dim],
            &q_bf,
        ).expect("upload q");
        let k = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![kv_len, n_kv_heads, head_dim],
            &k_bf,
        ).expect("upload k");
        let v = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![kv_len, n_kv_heads, head_dim],
            &v_bf,
        ).expect("upload v");
        let mut out = CudaTensor::<bf16>::zeros(
            dev.clone(),
            vec![n_tokens, n_q_heads, head_dim],
        ).expect("alloc out");

        launch_flash_attn_bf16(
            &dev, &q, &k, &v, None, &mut out, scale, gqa, /*causal=*/true,
        ).expect("launch");
        dev.synchronize().expect("sync");

        let o_bf = out.to_host().expect("download out");
        let o_gpu: Vec<f32> = o_bf.iter().map(|&x| x.to_f32()).collect();

        // Compare using the L2 relative-error norm that llama.cpp's
        // attention tests use (see `test-backend-ops.cpp`). Pure
        // per-element max_rel tripwires on near-zero outputs where bf16
        // roundoff dominates the softmax signal; L2 norms weight by
        // magnitude and give a stable bf16 tensor-core floor around
        // 1e-2 for head-dim=256 kv_len=128.
        //
        // Also track per-element max_abs and max_rel (with a
        // magnitude-scaled denominator floor) for diagnostic output.
        let l2_diff: f32 = o_cpu.iter().zip(o_gpu.iter())
            .map(|(&a, &b)| (a - b) * (a - b))
            .sum::<f32>()
            .sqrt();
        let l2_cpu: f32 = o_cpu.iter().map(|v| v * v).sum::<f32>().sqrt();
        let rel_l2 = l2_diff / l2_cpu.max(1e-9);

        let rms: f32 = l2_cpu / (o_cpu.len() as f32).sqrt();
        let floor = rms.max(1e-4); // one-tenth of RMS is well below bf16 resolution
        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        let mut worst_idx = 0usize;
        for (i, (&a, &b)) in o_cpu.iter().zip(o_gpu.iter()).enumerate() {
            let d = (a - b).abs();
            if d > max_abs {
                max_abs = d;
                worst_idx = i;
            }
            let denom = a.abs().max(b.abs()).max(floor);
            let rel = d / denom;
            if rel > max_rel {
                max_rel = rel;
            }
        }
        eprintln!(
            "flash_attn diff (n_tokens={}, kv_len={}, n_q_heads={}, gqa={}): \
             rel_l2={:.4e} rms={:.4e} max_abs={:.4e} max_rel={:.4e} \
             worst_idx={} cpu={:.4e} gpu={:.4e}",
            n_tokens, kv_len, n_q_heads, gqa,
            rel_l2, rms, max_abs, max_rel,
            worst_idx, o_cpu[worst_idx], o_gpu[worst_idx],
        );

        // L2 relative tolerance for bf16 tensor cores. llama.cpp uses
        // 2e-2 for equivalent D=128 fp16 MMA; doubling for D=256 and
        // bf16's narrower mantissa gives 5e-2 as the floor. This is
        // THE pass/fail signal.
        assert!(
            rel_l2 < 5e-2,
            "flash_attn diverges: rel_l2={} max_abs={} worst_idx={}",
            rel_l2, max_abs, worst_idx,
        );
    }
}
